//! SQLite の入口。履歴（reviews）と仕分け（triage）を持つ。
//!
//! 呼び出しごとに接続を開く（この規模なら十分速く、状態を持たずに済む）。WAL と busy_timeout で、
//! 画面からの操作と実行中のレビューが同時に書いても待ち合わせる。
//!
//! 以前の版の `reviews.jsonl` と `triage.json` は、初回に一度だけ取り込む。元のファイルは消さない。

use crate::store::ReviewRecord;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

const SCHEMA_VERSION: i64 = 5;
const FILE: &str = "mrcodereview.sqlite3";
const LEGACY_FLAG: &str = "imported_legacy_json";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS reviews (
  id                 TEXT PRIMARY KEY,
  created_at         INTEGER NOT NULL,
  repo               TEXT NOT NULL,
  recipe_id          TEXT NOT NULL,
  recipe_title       TEXT NOT NULL,
  provider_id        TEXT NOT NULL,
  provider_label     TEXT NOT NULL,
  model              TEXT NOT NULL,
  ok                 INTEGER NOT NULL,
  findings_count     INTEGER NOT NULL,
  verified_count     INTEGER NOT NULL,
  input_tokens       INTEGER,
  output_tokens      INTEGER,
  cache_read_tokens  INTEGER,
  cache_write_tokens INTEGER,
  cost_usd           REAL,
  duration_ms        INTEGER NOT NULL,
  record             TEXT NOT NULL,
  -- 同じリポジトリ・同じ観点の中の連番（「潜在バグ_1」の 1）。履歴を消しても、他の番号は変わらない
  seq                INTEGER,
  -- 採番時に決めた「日」（その時点の端末の時刻帯での日付）。あとから時刻帯が変わっても、保存済みの日は動かさない
  day                TEXT
);
CREATE INDEX IF NOT EXISTS reviews_repo_recipe ON reviews (repo, recipe_id, created_at DESC);
CREATE INDEX IF NOT EXISTS reviews_created ON reviews (created_at DESC);

-- 登録リポジトリ（構想書 §8）。枠は、この表の行数。解除（hidden=1）した行も数える。position が小さいほど上。
CREATE TABLE IF NOT EXISTS repos (
  id           TEXT PRIMARY KEY,
  path         TEXT NOT NULL,
  name         TEXT,
  hidden       INTEGER NOT NULL DEFAULT 0,
  first_commit TEXT,
  position     INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS triage (
  review_id TEXT NOT NULL,
  idx       INTEGER NOT NULL,
  state     TEXT NOT NULL,
  PRIMARY KEY (review_id, idx)
);

-- 仕分けの履歴。仕分けを変えるたびに 1 行足す（古い行は消さない）。いつ・何にしたか・そのときのリポジトリの HEAD。
CREATE TABLE IF NOT EXISTS triage_log (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  review_id TEXT NOT NULL,
  idx       INTEGER NOT NULL,
  state     TEXT NOT NULL,
  at        INTEGER NOT NULL,
  head      TEXT,
  source    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS triage_log_review ON triage_log (review_id, idx, id);

-- エージェントの「対応の報告」（会話の記録から取り込んだ、申告）。指摘ごとに、一番新しい 1 件。
CREATE TABLE IF NOT EXISTS agent_report (
  review_id   TEXT NOT NULL,
  idx         INTEGER NOT NULL,
  verdict     TEXT NOT NULL,
  text        TEXT NOT NULL,
  reported_at INTEGER NOT NULL,
  source      TEXT NOT NULL,
  fetched_at  INTEGER NOT NULL,
  PRIMARY KEY (review_id, idx)
);

-- 指摘ごとの対応メモと、対応済みの印。
CREATE TABLE IF NOT EXISTS finding_note (
  review_id TEXT NOT NULL,
  idx       INTEGER NOT NULL,
  memo      TEXT NOT NULL DEFAULT '',
  memo_at   INTEGER,
  done_at   INTEGER,
  done_head TEXT,
  PRIMARY KEY (review_id, idx)
);

-- 連番の最終値。日ごと（その日の 0:00〜23:59）の採番。履歴の最新を消しても、同じ番号を使い回さないために、記録とは別に持つ。
CREATE TABLE IF NOT EXISTS review_seq (
  repo      TEXT NOT NULL,
  recipe_id TEXT NOT NULL,
  day       TEXT NOT NULL,
  last      INTEGER NOT NULL,
  PRIMARY KEY (repo, recipe_id, day)
);

CREATE TABLE IF NOT EXISTS meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
";

/// 現在の時刻（UNIX ミリ秒）。
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn e<T: std::fmt::Display>(what: &'static str) -> impl FnOnce(T) -> String {
    move |err| format!("{what}: {err}")
}

pub fn open(dir: &Path) -> Result<Connection, String> {
    std::fs::create_dir_all(dir).map_err(e("保存先を作れない"))?;
    let conn = Connection::open(dir.join(FILE)).map_err(e("データベースを開けない"))?;
    conn.busy_timeout(Duration::from_secs(5)).map_err(e("busy_timeout"))?;
    conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get::<_, String>(0))
        .map_err(e("journal_mode"))?;

    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(e("user_version"))?;
    if version > SCHEMA_VERSION {
        return Err(format!(
            "新しい版のアプリが作ったデータベースです（スキーマ {version} > {SCHEMA_VERSION}）。このアプリを更新してください"
        ));
    }
    conn.execute_batch(SCHEMA).map_err(e("表を作れない"))?;
    if version < SCHEMA_VERSION {
        // 列の追加・日ごと連番の振り直し・版の更新は、1 つのトランザクション。途中で失敗したら、すべて元に戻す
        let tx = conn.unchecked_transaction().map_err(e("移行を始められない"))?;
        let has_seq: bool = tx
            .prepare("SELECT 1 FROM pragma_table_info('reviews') WHERE name = 'seq'")
            .and_then(|mut s| s.exists([]))
            .map_err(e("列を調べられない"))?;
        if !has_seq {
            tx.execute("ALTER TABLE reviews ADD COLUMN seq INTEGER", []).map_err(e("seq 列を足せない"))?;
        }
        let has_review_day: bool = tx
            .prepare("SELECT 1 FROM pragma_table_info('reviews') WHERE name = 'day'")
            .and_then(|mut s| s.exists([]))
            .map_err(e("列を調べられない"))?;
        if !has_review_day {
            tx.execute("ALTER TABLE reviews ADD COLUMN day TEXT", []).map_err(e("day 列を足せない"))?;
        }
        tx.execute("UPDATE reviews SET day = date(created_at / 1000, 'unixepoch', 'localtime') WHERE day IS NULL", [])
            .map_err(e("day を振れない"))?;
        let has_day: bool = tx
            .prepare("SELECT 1 FROM pragma_table_info('review_seq') WHERE name = 'day'")
            .and_then(|mut s| s.exists([]))
            .map_err(e("列を調べられない"))?;
        if !has_day {
            // 旧形式（日の無い最終値）は、捨てずに引き継ぐ。消した最新の番号を、また使わないため。
            // 日は分からないので、そのリポジトリ・観点の最新の記録の日（記録が無ければ今日）に置く
            let old: Vec<(String, String, i64)> = {
                let mut st = tx.prepare("SELECT repo, recipe_id, last FROM review_seq").map_err(e("review_seq を読めない"))?;
                let rows = st
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                    .map_err(e("review_seq を読めない"))?;
                rows.collect::<Result<_, _>>().map_err(e("review_seq を読めない"))?
            };
            tx.execute("DROP TABLE IF EXISTS review_seq", []).map_err(e("古い review_seq を削除できない"))?;
            tx.execute(
                "CREATE TABLE review_seq (
                  repo      TEXT NOT NULL,
                  recipe_id TEXT NOT NULL,
                  day       TEXT NOT NULL,
                  last      INTEGER NOT NULL,
                  PRIMARY KEY (repo, recipe_id, day)
                )",
                [],
            )
            .map_err(e("review_seq を作れない"))?;
            for (repo, recipe, last) in old {
                tx.execute(
                    "INSERT INTO review_seq (repo, recipe_id, day, last)
                     VALUES (?1, ?2,
                        COALESCE((SELECT day FROM reviews WHERE repo = ?1 AND recipe_id = ?2 ORDER BY created_at DESC, rowid DESC LIMIT 1),
                                 date('now', 'localtime')),
                        ?3)
                     ON CONFLICT (repo, recipe_id, day) DO UPDATE SET last = MAX(last, excluded.last)",
                    params![repo, recipe, last],
                )
                .map_err(e("review_seq を引き継げない"))?;
            }
        }
        backfill_seq_daily_in(&tx)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION).map_err(e("user_version"))?;
        tx.commit().map_err(e("移行を確定できない"))?;
    }
    import_legacy(&conn, dir)?;
    Ok(conn)
}

/// 番号の無い記録にだけ、同じリポジトリ・同じ観点・同じ日の中で、実行時刻の順に日ごと連番を振る（すでにある番号は、変えない）。
/// 連番の最終値（review_seq）も、ここで揃える。何度呼んでも、振り直さない。
pub(crate) fn backfill_seq(conn: &Connection) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(|err| format!("連番を振れない: {err}"))?;
    backfill_seq_daily_in(&tx)?;
    tx.commit().map_err(|err| format!("連番を確定できない: {err}"))
}

/// `backfill_seq` の中身。トランザクションは呼んだ側が持つ。
fn backfill_seq_daily_in(tx: &Connection) -> Result<(), String> {
    let w = |err: rusqlite::Error| format!("連番を振れない: {err}");
    tx.execute("UPDATE reviews SET day = date(created_at / 1000, 'unixepoch', 'localtime') WHERE day IS NULL", []).map_err(w)?;
    // (リポジトリ, 観点, 日) ごとの、いま使われている最大の番号（履歴の最大値と、記録した最終値の大きいほう）
    let mut max: HashMap<(String, String, String), i64> = HashMap::new();
    {
        let mut st = tx
            .prepare(
                "SELECT repo, recipe_id, day, MAX(m) FROM (
                    SELECT repo, recipe_id, day, MAX(seq) AS m FROM reviews WHERE seq IS NOT NULL GROUP BY repo, recipe_id, day
                    UNION ALL
                    SELECT repo, recipe_id, day, last AS m FROM review_seq
                 ) GROUP BY repo, recipe_id, day",
            )
            .map_err(w)?;
        let rows = st
            .query_map([], |r| Ok(((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?), r.get::<_, i64>(3)?)))
            .map_err(w)?;
        for row in rows {
            let (k, v) = row.map_err(w)?;
            max.insert(k, v);
        }
    }
    let todo: Vec<(i64, String, String, String)> = {
        let mut st = tx
            .prepare("SELECT rowid, repo, recipe_id, day FROM reviews WHERE seq IS NULL ORDER BY repo, recipe_id, day, created_at, rowid")
            .map_err(w)?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map_err(w)?;
        rows.collect::<Result<_, _>>().map_err(w)?
    };
    for (rowid, repo, recipe, day) in todo {
        let n = max.entry((repo, recipe, day)).or_insert(0);
        *n += 1;
        // 記録の JSON にも入れる（壊れた JSON は、触らない）
        tx.execute(
            "UPDATE reviews SET seq = ?1, record = CASE WHEN json_valid(record) THEN json_set(record, '$.seq', ?1) ELSE record END WHERE rowid = ?2",
            params![*n, rowid],
        )
        .map_err(w)?;
    }
    for ((repo, recipe, day), last) in &max {
        tx.execute(
            "INSERT INTO review_seq (repo, recipe_id, day, last) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (repo, recipe_id, day) DO UPDATE SET last = MAX(last, excluded.last)",
            params![repo, recipe, day, last],
        )
        .map_err(w)?;
    }
    Ok(())
}

/// 以前の JSONL / JSON を、初回だけ取り込む。壊れた行は飛ばす。取り込み済みの印は meta に残す。
fn import_legacy(conn: &Connection, dir: &Path) -> Result<(), String> {
    let done: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [LEGACY_FLAG], |r| r.get(0))
        .ok();
    if done.is_some() {
        return Ok(());
    }
    let jsonl = dir.join("reviews.jsonl");
    let triage_json = dir.join("triage.json");
    if jsonl.exists() || triage_json.exists() {
        let tx = conn.unchecked_transaction().map_err(e("取り込みを始められない"))?;
        if let Ok(text) = std::fs::read_to_string(&jsonl) {
            for rec in text.lines().filter_map(|l| serde_json::from_str::<ReviewRecord>(l).ok()) {
                crate::store::insert(&tx, &rec, true)?;
            }
        }
        if let Ok(text) = std::fs::read_to_string(&triage_json) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, Vec<String>>>(&text) {
                for (id, states) in map {
                    for (i, s) in states.iter().enumerate() {
                        if crate::triage::is_stored_state(s) {
                            tx.execute(
                                "INSERT OR IGNORE INTO triage (review_id, idx, state) VALUES (?1, ?2, ?3)",
                                params![id, i as i64, s],
                            )
                            .map_err(e("仕分けを取り込めない"))?;
                        }
                    }
                }
            }
        }
        tx.commit().map_err(e("取り込みを確定できない"))?;
        backfill_seq(conn)?;
    }
    conn.execute("INSERT OR IGNORE INTO meta (key, value) VALUES (?1, '1')", [LEGACY_FLAG])
        .map_err(e("meta を書けない"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mrcr-db-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn creates_schema_and_sets_version() {
        let d = scratch("schema");
        let conn = open(&d).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .flatten()
            .collect();
        for t in ["meta", "reviews", "triage"] {
            assert!(tables.iter().any(|x| x == t), "{t}");
        }
        // 2 回目に開いても壊れない
        drop(open(&d).unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn refuses_a_database_from_a_newer_app() {
        let d = scratch("newer");
        let conn = open(&d).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1).unwrap();
        drop(conn);
        let err = open(&d).unwrap_err();
        assert!(err.contains("新しい版"), "{err}");
        let _ = std::fs::remove_dir_all(&d);
    }

    fn raw_insert(conn: &Connection, id: &str, at: i64, repo: &str, recipe: &str, record: &str) {
        conn.execute(
            "INSERT INTO reviews (id, created_at, repo, recipe_id, recipe_title, provider_id, provider_label, model, ok, findings_count, verified_count, duration_ms, record) \
             VALUES (?1, ?2, ?3, ?4, 't', 'p', 'p', 'm', 1, 0, 0, 0, ?5)",
            params![id, at, repo, recipe, record],
        )
        .unwrap();
    }

    fn seq_col(conn: &Connection, id: &str) -> Option<i64> {
        conn.query_row("SELECT seq FROM reviews WHERE id = ?1", [id], |r| r.get(0)).unwrap()
    }

    #[test]
    fn migration_survives_a_broken_record_and_a_column_that_already_exists() {
        let d = scratch("seq-broken");
        {
            // seq 列が、すでにある（新しい表の形）まま、版だけ古い・番号が無い・壊れた JSON の行が 1 件混ざる
            let conn = open(&d).unwrap();
            let rec = |id: &str, at: u64| {
                let mut r = crate::store::sample_record(id, at);
                r.repo = "/r".into();
                r.recipe.id = "bugs".into();
                serde_json::to_string(&r).unwrap()
            };
            raw_insert(&conn, "a", 10, "/r", "bugs", &rec("a", 10));
            raw_insert(&conn, "broken", 20, "/r", "bugs", "これは JSON ではない");
            raw_insert(&conn, "c", 30, "/r", "bugs", &rec("c", 30));
            conn.pragma_update(None, "user_version", 3).unwrap();
        }
        let conn = open(&d).expect("壊れた記録が 1 件あっても、列がすでにあっても、開ける");
        assert_eq!((seq_col(&conn, "a"), seq_col(&conn, "broken"), seq_col(&conn, "c")), (Some(1), Some(2), Some(3)), "壊れた行も数に入れ、他の行の番号は実行時刻の順で正しい");
        let json: String = conn.query_row("SELECT record FROM reviews WHERE id = 'broken'", [], |r| r.get(0)).unwrap();
        assert_eq!(json, "これは JSON ではない", "壊れた JSON は、触らない");
        let ok: String = conn.query_row("SELECT json_extract(record, '$.seq') FROM reviews WHERE id = 'c'", [], |r| r.get::<_, i64>(0).map(|n| n.to_string())).unwrap();
        assert_eq!(ok, "3", "読める記録の JSON には、番号が入る");
        let last: i64 = conn.query_row("SELECT last FROM review_seq WHERE repo = '/r' AND recipe_id = 'bugs'", [], |r| r.get(0)).unwrap();
        assert_eq!(last, 3);
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_failed_migration_rolls_back_everything_and_is_retried_cleanly() {
        let d = scratch("seq-rollback");
        {
            let conn = open(&d).unwrap();
            raw_insert(&conn, "a", 10, "/r", "bugs", "{}");
            // 古い形に戻す: 列が無く、版が 3
            conn.execute("ALTER TABLE reviews DROP COLUMN seq", []).unwrap();
            conn.pragma_update(None, "user_version", 3).unwrap();
            // 連番を振る UPDATE を失敗させる
            conn.execute_batch("CREATE TRIGGER fail_update BEFORE UPDATE ON reviews BEGIN SELECT RAISE(FAIL, 'boom'); END;").unwrap();
        }
        assert!(open(&d).is_err(), "移行に失敗したら、エラー（半端な状態で開かない）");
        {
            let raw = Connection::open(d.join(FILE)).unwrap();
            let v: i64 = raw.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
            let has_seq: bool = raw.prepare("SELECT 1 FROM pragma_table_info('reviews') WHERE name = 'seq'").unwrap().exists([]).unwrap();
            assert_eq!((v, has_seq), (3, false), "列の追加も、版の更新も、元に戻っている");
            raw.execute_batch("DROP TRIGGER fail_update").unwrap();
        }
        let conn = open(&d).expect("原因を取り除けば、そのまま、やり直せる");
        assert_eq!(seq_col(&conn, "a"), Some(1));
        let _ = std::fs::remove_dir_all(&d);
    }
}
