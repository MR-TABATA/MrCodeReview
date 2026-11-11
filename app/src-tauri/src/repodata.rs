//! リポジトリごとのデータ（履歴・仕分け・メモ・エージェントの報告）の件数と、まとめての削除。
//!
//! 設定の「リポジトリ」タブが使う。登録の「枠」は、解除してもデータがあるうちは戻らない。枠を空けるには、
//! ここでデータを削除する（構想書 §8）。**削除は、1 つのトランザクション**で行い、途中で止まっても、半端に残らない。
//!
//! データは、レビューの ID に結びつく 5 つの表にある。`reviews.repo` には、リポジトリの**根のパス**が入っている。

use crate::{db, snapshot, store};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// 1 つのリポジトリのデータの量。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RepoData {
    pub reviews: usize,
    /// 指摘の合計
    pub findings: usize,
    /// 仕分けした指摘の数
    pub triaged: usize,
    /// メモのある指摘の数
    pub memos: usize,
    /// 「未」の指摘の数（未仕分け ＋ 対応中。履歴の各行の「未」の合計）。左のリポジトリ一覧に出す
    pub open: usize,
}

impl RepoData {
    /// 別のパスの分を足す（同じリポジトリの、クローン・ワークツリーの履歴）。
    pub fn add(&mut self, o: &RepoData) {
        self.reviews += o.reviews;
        self.findings += o.findings;
        self.triaged += o.triaged;
        self.memos += o.memos;
        self.open += o.open;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RepoDataRow {
    pub path: String,
    #[serde(flatten)]
    pub data: RepoData,
}

/// 履歴に残っている、リポジトリの「根のパス」。サブディレクトリを渡されても、根に直す。
/// フォルダが無くなっていて直せないときは、存在する一番近い上のフォルダを実体のパス（シンボリックリンクを解いたもの）に直して、
/// 残りをつなぐ。上も解けなければ、渡されたパスのまま（末尾の `/` だけ除く）。
pub fn repo_key(path: &str) -> String {
    snapshot::toplevel(path).unwrap_or_else(|_| canonicalize_lenient(path).unwrap_or_else(|| path.trim_end_matches('/').to_string()))
}

/// 存在する一番近い上のフォルダを `canonicalize` して、無い部分をつなぐ（フォルダが消えていても、リンク経由のパスを実体に寄せる）。
fn canonicalize_lenient(path: &str) -> Option<String> {
    let mut cur = Path::new(path.trim_end_matches('/'));
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(mut real) = std::fs::canonicalize(cur) {
            for part in tail.iter().rev() {
                real.push(part);
            }
            return Some(real.to_string_lossy().into_owned());
        }
        tail.push(cur.file_name()?.to_os_string());
        cur = cur.parent()?;
        if cur.as_os_str().is_empty() {
            return None;
        }
    }
}

/// 履歴に残っている根のパスの集まり（`reviews.repo` の索引で引く）。
fn known_repos(conn: &Connection) -> Result<HashSet<String>, String> {
    let mut stmt = conn.prepare("SELECT DISTINCT repo FROM reviews").map_err(|e| format!("履歴を読めません: {e}"))?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| format!("履歴を読めません: {e}"))?;
    rows.collect::<Result<HashSet<_>, _>>().map_err(|e| format!("履歴を読めません: {e}"))
}

/// 登録のパスから、履歴の根のパスを決める。履歴にそのままある根は、git を起動せずに使う。
/// 無ければ `repo_key`。それでも履歴に無ければ、大文字小文字だけが違うものを探す（macOS の既定のファイルシステムは、区別しない）。
fn resolve_key(known: &HashSet<String>, path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if known.contains(trimmed) {
        return trimmed.to_string();
    }
    let key = repo_key(path);
    if known.contains(&key) {
        return key;
    }
    known.iter().find(|k| k.eq_ignore_ascii_case(&key)).cloned().unwrap_or(key)
}

const IDS: &str = "(SELECT id FROM reviews WHERE repo = ?1)";

fn count(conn: &Connection, key: &str) -> Result<RepoData, String> {
    let n = |sql: &str| -> Result<usize, String> {
        conn.query_row(sql, params![key], |r| r.get::<_, i64>(0)).map(|v| v.max(0) as usize).map_err(|e| format!("件数を数えられません: {e}"))
    };
    Ok(RepoData {
        reviews: n("SELECT COUNT(*) FROM reviews WHERE repo = ?1")?,
        findings: n("SELECT COALESCE(SUM(findings_count), 0) FROM reviews WHERE repo = ?1")?,
        triaged: n(&format!("SELECT COUNT(*) FROM triage WHERE review_id IN {IDS}"))?,
        memos: n(&format!("SELECT COUNT(*) FROM finding_note WHERE memo <> '' AND review_id IN {IDS}"))?,
        open: store::open_count(conn, key)?,
    })
}

/// 根のパスの集まりの、データの量。表ごとに GROUP BY で、まとめて集計する（リポジトリごとに何度も引かない）。
fn counts_for(conn: &Connection, keys: &[String]) -> Result<HashMap<String, RepoData>, String> {
    let mut out: HashMap<String, RepoData> = HashMap::new();
    if keys.is_empty() {
        return Ok(out);
    }
    let marks = vec!["?"; keys.len()].join(",");
    let grouped = |sql: String, mut each: Box<dyn FnMut(&str, usize, usize) + '_>| -> Result<(), String> {
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("件数を数えられません: {e}"))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(keys.iter()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.max(0) as usize, r.get::<_, i64>(2)?.max(0) as usize)))
            .map_err(|e| format!("件数を数えられません: {e}"))?;
        for row in rows {
            let (k, a, b) = row.map_err(|e| format!("件数を数えられません: {e}"))?;
            each(&k, a, b);
        }
        Ok(())
    };
    // 履歴の数と、指摘の合計
    let mut by: HashMap<String, (usize, usize, usize, usize)> = HashMap::new();
    grouped(format!("SELECT repo, COUNT(*), COALESCE(SUM(findings_count), 0) FROM reviews WHERE repo IN ({marks}) GROUP BY repo"), Box::new(|k, n, f| {
        let e = by.entry(k.to_string()).or_default();
        e.0 = n;
        e.1 = f;
    }))?;
    // 仕分けした指摘の数
    grouped(format!("SELECT r.repo, COUNT(*), 0 FROM triage t JOIN reviews r ON r.id = t.review_id WHERE r.repo IN ({marks}) GROUP BY r.repo"), Box::new(|k, n, _| {
        by.entry(k.to_string()).or_default().2 = n;
    }))?;
    // メモのある指摘の数
    grouped(format!("SELECT r.repo, COUNT(*), 0 FROM finding_note f JOIN reviews r ON r.id = f.review_id WHERE f.memo <> '' AND r.repo IN ({marks}) GROUP BY r.repo"), Box::new(|k, n, _| {
        by.entry(k.to_string()).or_default().3 = n;
    }))?;
    for (k, (reviews, findings, triaged, memos)) in by {
        // 「未」の数は、履歴のあるリポジトリだけ数える
        let open = if reviews > 0 { store::open_count(conn, &k)? } else { 0 };
        out.insert(k, RepoData { reviews, findings, triaged, memos, open });
    }
    Ok(out)
}

/// 各パスの、データの量（読み取りだけ）。
pub fn summaries(dir: &Path, paths: &[String]) -> Result<Vec<RepoDataRow>, String> {
    let conn = db::open(dir)?;
    let known = known_repos(&conn)?;
    let keys: Vec<String> = paths.iter().map(|p| resolve_key(&known, p)).collect();
    let mut uniq: Vec<String> = keys.clone();
    uniq.sort();
    uniq.dedup();
    let by_key = counts_for(&conn, &uniq)?;
    Ok(paths.iter().zip(&keys).map(|(p, k)| RepoDataRow { path: p.clone(), data: by_key.get(k).cloned().unwrap_or_default() }).collect())
}

/// このリポジトリのデータを、すべて消す（登録は残す）。消した量を返す。**元に戻せない。** 本体の「データを削除」は `repos::delete_data`（登録も同じトランザクションで消す）。
#[cfg(test)]
pub fn delete_all(dir: &Path, path: &str) -> Result<RepoData, String> {
    let key = repo_key(path);
    let conn = db::open(dir)?;
    let tx = conn.unchecked_transaction().map_err(|e| format!("削除を始められません: {e}"))?;
    let removed = delete_in(&tx, &key)?;
    tx.commit().map_err(|e| format!("削除を確定できません: {e}"))?;
    Ok(removed)
}

/// 呼び出し側のトランザクションの中で、このリポジトリ（根のパス）のデータを消す。確定は呼び出し側。
pub fn delete_in(tx: &Connection, key: &str) -> Result<RepoData, String> {
    let removed = count(tx, key)?;
    for table in ["triage", "triage_log", "agent_report", "finding_note"] {
        tx.execute(&format!("DELETE FROM {table} WHERE review_id IN {IDS}"), params![key]).map_err(|e| format!("{table} を消せません: {e}"))?;
    }
    tx.execute("DELETE FROM reviews WHERE repo = ?1", params![key]).map_err(|e| format!("履歴を消せません: {e}"))?;
    Ok(removed)
}

/// 履歴の数（根のパスで数える）。
pub fn review_count(conn: &Connection, key: &str) -> Result<usize, String> {
    Ok(count(conn, key)?.reviews)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mrcr-repodata-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn add_review(conn: &Connection, id: &str, repo: &str, findings: i64) {
        conn.execute(
            "INSERT INTO reviews (id, created_at, repo, recipe_id, recipe_title, provider_id, provider_label, model, ok, findings_count, verified_count, duration_ms, record) \
             VALUES (?1, 1, ?2, 'bugs', '潜在バグ', 'mock', 'モック', 'mock', 1, ?3, 0, 1, '{}')",
            params![id, repo, findings],
        )
        .unwrap();
        for i in 0..findings {
            conn.execute("INSERT INTO triage (review_id, idx, state) VALUES (?1, ?2, 'fix')", params![id, i]).unwrap();
        }
        conn.execute("INSERT INTO triage_log (review_id, idx, state, at, source) VALUES (?1, 0, 'fix', 1, 'ui')", params![id]).unwrap();
        conn.execute("INSERT INTO agent_report (review_id, idx, verdict, text, reported_at, source, fetched_at) VALUES (?1, 0, 'did', 't', 1, 's', 1)", params![id]).unwrap();
        conn.execute("INSERT INTO finding_note (review_id, idx, memo) VALUES (?1, 0, 'メモ')", params![id]).unwrap();
        conn.execute("INSERT INTO finding_note (review_id, idx, memo) VALUES (?1, 1, '')", params![id]).unwrap();
    }

    fn rows(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn counts_only_this_repo_and_deletes_every_related_table_leaving_others_intact() {
        let d = scratch("delete");
        let conn = db::open(&d).unwrap();
        add_review(&conn, "a1", "/gone/a", 2);
        add_review(&conn, "a2", "/gone/a", 1);
        add_review(&conn, "b1", "/gone/b", 3);
        drop(conn);

        let paths = vec!["/gone/a/".to_string(), "/gone/b".to_string(), "/gone/none".to_string()];
        let s = summaries(&d, &paths).unwrap();
        assert_eq!(s[0].data, RepoData { reviews: 2, findings: 3, triaged: 3, memos: 2, open: 1 }, "末尾の / があっても、同じ");
        assert_eq!(s[1].data.reviews, 1);
        assert_eq!(s[2].data, RepoData::default(), "履歴が無ければ、すべて 0");

        let removed = delete_all(&d, "/gone/a").unwrap();
        assert_eq!(removed, RepoData { reviews: 2, findings: 3, triaged: 3, memos: 2, open: 1 });
        let conn = db::open(&d).unwrap();
        // a のものは、5 つの表のすべてから消え、b のものだけが残る
        assert_eq!(rows(&conn, "reviews"), 1);
        for table in ["triage", "triage_log", "agent_report", "finding_note"] {
            let left_of_a: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table} WHERE review_id IN ('a1','a2')"), [], |r| r.get(0)).unwrap();
            assert_eq!(left_of_a, 0, "{table}");
            let left_of_b: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table} WHERE review_id = 'b1'"), [], |r| r.get(0)).unwrap();
            assert!(left_of_b > 0, "{table}: 別のリポジトリのものは、残す");
        }
        assert_eq!(summaries(&d, &["/gone/a".to_string()]).unwrap()[0].data, RepoData::default());
        // もう一度消しても、エラーにならず、0 件
        assert_eq!(delete_all(&d, "/gone/a").unwrap(), RepoData::default());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_failure_in_the_middle_leaves_nothing_half_deleted() {
        let d = scratch("rollback");
        let conn = db::open(&d).unwrap();
        add_review(&conn, "a1", "/gone/a", 1);
        // 途中の表を壊して、削除を失敗させる
        conn.execute_batch("DROP TABLE agent_report; CREATE TABLE agent_report (x INTEGER);").unwrap();
        drop(conn);
        assert!(delete_all(&d, "/gone/a").is_err());
        let conn = db::open(&d).unwrap();
        assert_eq!(rows(&conn, "reviews"), 1, "失敗したら、先に消した分も、元に戻る");
        assert_eq!(rows(&conn, "triage"), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_counting_queries_use_indexes_not_full_scans() {
        let d = scratch("plan");
        let conn = db::open(&d).unwrap();
        add_review(&conn, "a1", "/gone/a", 2);
        let plan = |sql: &str| -> String {
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let rows = stmt.query_map(params!["/gone/a"], |r| r.get::<_, String>(3)).unwrap();
            rows.map(|r| r.unwrap()).collect::<Vec<_>>().join(" | ")
        };
        for sql in [
            "SELECT COUNT(*) FROM reviews WHERE repo = ?1",
            "SELECT COUNT(*) FROM triage WHERE review_id IN (SELECT id FROM reviews WHERE repo = ?1)",
            "SELECT COUNT(*) FROM finding_note WHERE memo <> '' AND review_id IN (SELECT id FROM reviews WHERE repo = ?1)",
            "SELECT COUNT(*) FROM agent_report WHERE review_id IN (SELECT id FROM reviews WHERE repo = ?1)",
            "SELECT COUNT(*) FROM triage_log WHERE review_id IN (SELECT id FROM reviews WHERE repo = ?1)",
        ] {
            let p = plan(sql);
            assert!(p.contains("USING"), "索引を使う: {sql} => {p}");
            for table in ["reviews", "triage", "finding_note", "agent_report", "triage_log"] {
                assert!(!p.contains(&format!("SCAN {table}")), "{table} を全件走査しない: {sql} => {p}");
            }
        }
        drop(conn);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn many_repos_are_counted_together_and_match_the_one_by_one_counts() {
        let d = scratch("grouped");
        let conn = db::open(&d).unwrap();
        for (i, repo) in ["/gone/a", "/gone/b", "/gone/c"].iter().enumerate() {
            add_review(&conn, &format!("r{i}x"), repo, 1 + i as i64);
            add_review(&conn, &format!("r{i}y"), repo, 2);
        }
        let paths: Vec<String> = ["/gone/a/", "/gone/b", "/gone/c", "/gone/none", "/gone/a"].iter().map(|s| s.to_string()).collect();
        let s = summaries(&d, &paths).unwrap();
        for row in &s {
            assert_eq!(row.data, count(&conn, &repo_key(&row.path)).unwrap(), "{}: まとめて数えても、1 つずつ数えたものと同じ", row.path);
        }
        assert_eq!(s[0].data, s[4].data, "同じ根は、同じ結果");
        assert_eq!(s[3].data, RepoData::default());
        drop(conn);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn a_gone_folder_reached_through_a_symlink_or_with_different_case_still_finds_its_history() {
        let d = scratch("pathforms");
        let base = std::fs::canonicalize({
            std::fs::create_dir_all(&d).unwrap();
            &d
        })
        .unwrap();
        let real = base.join("real");
        std::fs::create_dir_all(&real).unwrap();
        std::os::unix::fs::symlink(&real, base.join("link")).unwrap();
        let conn = db::open(&d).unwrap();
        // 履歴には、実体のパスで入っている。フォルダ（proj）は、もう無い
        let stored = real.join("proj").to_string_lossy().into_owned();
        add_review(&conn, "p1", &stored, 2);
        add_review(&conn, "c1", "/gone/CaseRepo", 1);
        drop(conn);
        let via_link = base.join("link").join("proj").to_string_lossy().into_owned();
        let s = summaries(&d, &[via_link.clone(), "/gone/caserepo".to_string()]).unwrap();
        assert_eq!(s[0].data.reviews, 1, "リンク経由のパスでも、履歴が見える");
        assert_eq!(s[1].data.reviews, 1, "大文字小文字だけ違うパスでも、履歴が見える");
        assert_eq!(repo_key(&via_link), stored, "削除などに使う根のパスも、実体に寄る");
        let _ = std::fs::remove_dir_all(&d);
    }
}
