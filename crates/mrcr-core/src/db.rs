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

const SCHEMA_VERSION: i64 = 3;
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
  record             TEXT NOT NULL
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
        conn.pragma_update(None, "user_version", SCHEMA_VERSION).map_err(e("user_version"))?;
    }
    import_legacy(&conn, dir)?;
    Ok(conn)
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
}
