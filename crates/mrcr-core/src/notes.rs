//! 指摘ごとの、対応メモと、対応済みの印。
//!
//! - 対応メモ: 「何をしたか」「なぜ無視か」を、1 文で残す（成果物のレポートに載る）。
//! - 対応済み: 「対応」にした指摘だけに付けられる。付けた時刻と、そのときのリポジトリの HEAD を残す。
//!   仕分けを「対応」以外にすると、外れる（`triage::set_with`）。
//! - どちらも、あなたが書いたものだけ。アプリが推測で埋めない。

use crate::db;
use crate::triage;
use rusqlite::params;
use serde::Serialize;
use std::path::Path;

/// メモの最大の長さ（文字数）。
pub const MAX_MEMO_CHARS: usize = 2000;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Note {
    pub memo: String,
    pub memo_at: Option<u64>,
    /// 対応済みにした時刻。None = 済みにしていない
    pub done_at: Option<u64>,
    pub done_head: Option<String>,
    /// 最後に仕分けた時刻（履歴の最後の行）。仕分けの履歴が無い（この機能より前の記録）なら None
    pub triage_at: Option<u64>,
    pub triage_head: Option<String>,
    /// 最後の仕分けが、あなたの操作か、前回からの引き継ぎか（"user" | "carry"）
    pub triage_source: Option<String>,
}

/// `n` 件の指摘それぞれの記録（メモ・対応済み・最後に仕分けた時刻）。
/// 読み取りに失敗したときは、**エラーを返す**（メモや対応済みの印が消えたまま、何事も無かったように見せない）。
/// 成果物（レポート）は、こちらを使う。
pub fn try_get_all(dir: &Path, review_id: &str, n: usize) -> Result<Vec<Note>, String> {
    let conn = db::open(dir)?;
    try_get_all_with_conn(&conn, review_id, n)
}

/// 既存の接続を使い回して、対応メモ・対応済みの印・仕分け時刻を読み取る。
pub fn try_get_all_with_conn(conn: &rusqlite::Connection, review_id: &str, n: usize) -> Result<Vec<Note>, String> {
    let logs = triage::try_log_with_conn(conn, review_id)?;
    try_get_all_with_logs(conn, review_id, n, &logs)
}

/// 既存の接続と、すでに読み込んだ仕分け履歴を使い回して、対応メモと対応済みの印を読み取る。
pub fn try_get_all_with_logs(conn: &rusqlite::Connection, review_id: &str, n: usize, logs: &[triage::LogRow]) -> Result<Vec<Note>, String> {
    let mut out = vec![Note::default(); n];
    let w = |e: rusqlite::Error| format!("対応メモ・対応済みの印を読み取れません: {e}");
    let mut stmt = conn.prepare("SELECT idx, memo, memo_at, done_at, done_head FROM finding_note WHERE review_id = ?1").map_err(w)?;
    let rows = stmt
        .query_map([review_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, Option<String>>(4)?))
        })
        .map_err(w)?;
    for row in rows {
        let (idx, memo, memo_at, done_at, done_head) = row.map_err(w)?;
        if idx >= 0 && (idx as usize) < n {
            let note = &mut out[idx as usize];
            note.memo = memo;
            note.memo_at = memo_at.map(|v| v.max(0) as u64);
            note.done_at = done_at.map(|v| v.max(0) as u64);
            note.done_head = done_head;
        }
    }
    for row in logs {
        if row.idx < n {
            let note = &mut out[row.idx];
            note.triage_at = Some(row.at);
            note.triage_head = row.head.clone();
            note.triage_source = Some(row.source.clone());
        }
    }
    Ok(out)
}

/// 画面の表示用。**読み取りに失敗しても、空の記録で返す**。成果物（レポート）には使わない（`try_get_all` を使う）。
/// 失敗は、書き込み（メモを書く・対応済みにする）のときに、エラーとして出る。
pub fn get_all(dir: &Path, review_id: &str, n: usize) -> Vec<Note> {
    try_get_all(dir, review_id, n).unwrap_or_else(|_| vec![Note::default(); n])
}

/// メモを書く。空にすると、メモだけを消す（対応済みの印は、そのまま）。
pub fn set_memo(dir: &Path, review_id: &str, index: usize, n: usize, memo: &str, at: u64) -> Result<(), String> {
    if index >= n {
        return Err(format!("指摘 {index} 番は無い（{n} 件）"));
    }
    let memo = memo.trim();
    if memo.chars().count() > MAX_MEMO_CHARS {
        return Err(format!("メモが長すぎます（{} 文字まで。いまは {} 文字）", MAX_MEMO_CHARS, memo.chars().count()));
    }
    let conn = db::open(dir)?;
    let at = if memo.is_empty() { None } else { Some(at as i64) };
    conn.execute(
        "INSERT INTO finding_note (review_id, idx, memo, memo_at) VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT (review_id, idx) DO UPDATE SET memo = excluded.memo, memo_at = excluded.memo_at",
        params![review_id, index as i64, memo, at],
    )
    .map_err(|e| format!("メモを書けない: {e}"))?;
    Ok(())
}

/// 対応済みにする／戻す。「対応」にした指摘だけが、済みにできる。
pub fn set_done(dir: &Path, review_id: &str, index: usize, n: usize, done: bool, at: u64, head: Option<&str>) -> Result<(), String> {
    if index >= n {
        return Err(format!("指摘 {index} 番は無い（{n} 件）"));
    }
    if done && triage::get(dir, review_id, n)[index] != triage::FIX {
        return Err("「対応」にした指摘だけ、対応済みにできます（先に、仕分けを「対応」にしてください）".into());
    }
    let conn = db::open(dir)?;
    if done {
        conn.execute(
            "INSERT INTO finding_note (review_id, idx, done_at, done_head) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (review_id, idx) DO UPDATE SET done_at = excluded.done_at, done_head = excluded.done_head",
            params![review_id, index as i64, at as i64, head],
        )
    } else {
        conn.execute("UPDATE finding_note SET done_at = NULL, done_head = NULL WHERE review_id = ?1 AND idx = ?2", params![review_id, index as i64])
    }
    .map_err(|e| format!("対応済みの印を書けない: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mrcr-notes-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn triage_changes_leave_a_log_with_time_and_head_and_never_overwrite_it() {
        let d = scratch("log");
        triage::set_with(&d, "r1", 0, "fix", 3, 1000, Some("abc1234")).unwrap();
        triage::set_with(&d, "r1", 0, "dismiss", 3, 2000, Some("def5678")).unwrap();
        triage::set_with(&d, "r1", 1, "wait", 3, 1500, None).unwrap();
        triage::set_with(&d, "r1", 0, "new", 3, 3000, None).unwrap();
        let log = triage::try_log(&d, "r1").unwrap();
        let got: Vec<(usize, &str, u64, Option<&str>)> = log.iter().map(|l| (l.idx, l.state.as_str(), l.at, l.head.as_deref())).collect();
        assert_eq!(got, [(0, "fix", 1000, Some("abc1234")), (0, "dismiss", 2000, Some("def5678")), (1, "wait", 1500, None), (0, "new", 3000, None)], "変えるたびに、1 行ずつ。古い行は残る");
        assert!(log.iter().all(|l| l.source == "user"));
        assert_eq!(triage::get(&d, "r1", 3), ["new", "wait", "new"], "いまの状態は、従来どおり");
        let n = get_all(&d, "r1", 3);
        assert_eq!((n[0].triage_at, n[1].triage_at, n[2].triage_at), (Some(3000), Some(1500), None), "最後の仕分けの時刻。仕分けていなければ None");
        assert!(triage::try_log(&d, "other").unwrap().is_empty());
        assert!(triage::set_with(&d, "r1", 9, "fix", 3, 1, None).is_err() && triage::set_with(&d, "r1", 0, "bogus", 3, 1, None).is_err());
        assert_eq!(triage::try_log(&d, "r1").unwrap().len(), 4, "失敗した操作は、履歴に残さない");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn carried_states_are_logged_as_carry_not_as_your_action() {
        let d = scratch("carry");
        triage::put(&d, "r2", vec!["new".into(), "wait".into(), "dismiss".into()]).unwrap();
        let log = triage::try_log(&d, "r2").unwrap();
        assert_eq!(log.iter().map(|l| (l.idx, l.state.as_str(), l.source.as_str())).collect::<Vec<_>>(), [(1, "wait", "carry"), (2, "dismiss", "carry")]);
        let n = get_all(&d, "r2", 3);
        assert_eq!(n[1].triage_source.as_deref(), Some("carry"));
        assert!(n[0].triage_source.is_none(), "未仕分けは、履歴が無い");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn memo_is_trimmed_capped_and_cleared_by_empty_text_without_touching_done() {
        let d = scratch("memo");
        triage::set_with(&d, "r3", 0, "fix", 2, 10, None).unwrap();
        set_done(&d, "r3", 0, 2, true, 20, Some("abc")).unwrap();
        set_memo(&d, "r3", 0, 2, "  境界を直した  ", 30).unwrap();
        let n = &get_all(&d, "r3", 2)[0];
        assert_eq!((n.memo.as_str(), n.memo_at, n.done_at, n.done_head.as_deref()), ("境界を直した", Some(30), Some(20), Some("abc")));
        set_memo(&d, "r3", 0, 2, "   ", 40).unwrap();
        let n = &get_all(&d, "r3", 2)[0];
        assert_eq!((n.memo.as_str(), n.memo_at, n.done_at), ("", None, Some(20)), "メモだけが消え、対応済みは残る");
        assert!(set_memo(&d, "r3", 0, 2, &"あ".repeat(MAX_MEMO_CHARS + 1), 50).unwrap_err().contains("長すぎます"));
        assert!(set_memo(&d, "r3", 5, 2, "x", 50).is_err());
        set_memo(&d, "r3", 1, 2, &"あ".repeat(MAX_MEMO_CHARS), 60).unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn only_fix_findings_can_be_marked_done_and_changing_the_state_clears_the_mark() {
        let d = scratch("done");
        let e = set_done(&d, "r4", 0, 2, true, 10, None).unwrap_err();
        assert!(e.contains("「対応」にした指摘だけ"), "未仕分けは、済みにできない: {e}");
        triage::set_with(&d, "r4", 0, "wait", 2, 10, None).unwrap();
        assert!(set_done(&d, "r4", 0, 2, true, 10, None).is_err(), "待ちも、済みにできない");
        triage::set_with(&d, "r4", 0, "fix", 2, 20, Some("h1")).unwrap();
        set_done(&d, "r4", 0, 2, true, 30, Some("h2")).unwrap();
        assert_eq!(get_all(&d, "r4", 2)[0].done_at, Some(30));
        // 対応のまま、もう一度「対応」にしても、済みは外れない（状態は変わっていない）
        triage::set_with(&d, "r4", 0, "fix", 2, 35, None).unwrap();
        assert_eq!(get_all(&d, "r4", 2)[0].done_at, Some(30), "状態が変わらなければ、済みは外れない");
        set_done(&d, "r4", 0, 2, true, 40, None).unwrap();
        triage::set_with(&d, "r4", 0, "dismiss", 2, 50, None).unwrap();
        assert_eq!(get_all(&d, "r4", 2)[0].done_at, None, "対応以外にすると、済みは外れる");
        // 済みを戻す
        triage::set_with(&d, "r4", 0, "fix", 2, 60, None).unwrap();
        set_done(&d, "r4", 0, 2, true, 70, None).unwrap();
        set_done(&d, "r4", 0, 2, false, 80, None).unwrap();
        assert_eq!(get_all(&d, "r4", 2)[0].done_at, None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn display_readers_stay_lenient_but_the_strict_ones_report_the_failure() {
        let d = scratch("lenient");
        triage::set_with(&d, "r9", 0, "fix", 2, 10, None).unwrap();
        set_memo(&d, "r9", 0, 2, "メモ", 20).unwrap();
        // 表を壊す（列の無い表に、置き換える）
        {
            let c = rusqlite::Connection::open(d.join("mrcodereview.sqlite3")).unwrap();
            c.execute_batch("DROP TABLE finding_note; CREATE TABLE finding_note (x INTEGER); DROP TABLE triage; CREATE TABLE triage (x INTEGER);").unwrap();
        }
        // 画面の表示用: 失敗しても、空の記録・全部「未仕分け」で返す（画面を開けなくしない）
        assert_eq!(get_all(&d, "r9", 2), vec![Note::default(); 2]);
        assert_eq!(triage::get(&d, "r9", 2), ["new", "new"]);
        // 成果物用: 失敗を、エラーで言う
        assert!(try_get_all(&d, "r9", 2).unwrap_err().contains("対応メモ・対応済みの印を読み取れません"));
        assert!(triage::try_get(&d, "r9", 2).unwrap_err().contains("仕分けを読み取れません"));
        // 書き込みは、失敗を、エラーで返す（黙って捨てない）
        assert!(set_memo(&d, "r9", 0, 2, "もう一度", 30).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_older_database_gets_the_new_tables_without_losing_triage() {
        let d = scratch("migrate");
        std::fs::create_dir_all(&d).unwrap();
        // 版 1 のデータベース（triage の表だけ。新しい表は無い）を、手で作る
        {
            let c = rusqlite::Connection::open(d.join("mrcodereview.sqlite3")).unwrap();
            c.execute_batch(
                "CREATE TABLE triage (review_id TEXT NOT NULL, idx INTEGER NOT NULL, state TEXT NOT NULL, PRIMARY KEY (review_id, idx)); \
                 INSERT INTO triage VALUES ('old', 0, 'fix'); PRAGMA user_version = 1;",
            )
            .unwrap();
        }
        assert_eq!(triage::get(&d, "old", 1), ["fix"], "古い仕分けは、そのまま読める");
        assert!(triage::try_log(&d, "old").unwrap().is_empty(), "履歴は、この機能より前には無い");
        let n = get_all(&d, "old", 1);
        assert_eq!((n[0].triage_at, n[0].memo.as_str()), (None, ""), "時刻は、推定で埋めない");
        triage::set_with(&d, "old", 0, "dismiss", 1, 5, None).unwrap();
        assert_eq!(triage::try_log(&d, "old").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }
}
