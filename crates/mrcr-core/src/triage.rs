//! 指摘の仕分け（対応／待ち／無視）と、CUI のエージェントに貼る文面の組み立て。
//!
//! 仕分けは、レビューの記録（reviews 表）とは別の triage 表に持つ。記録は書き換えない。
//! 仕分けは「そのレビューの n 番目の指摘」に紐づく。再レビューへの引き継ぎは `carry.rs` が、
//! 新しいレビューの初期値としてここへ書く。未仕分け（"new"）は行を持たない。

use crate::db;
use crate::store::ReviewRecord;
use rusqlite::params;
use std::path::Path;

pub const NEW: &str = "new";
pub const FIX: &str = "fix";
pub const WAIT: &str = "wait";
pub const DISMISS: &str = "dismiss";

fn valid(state: &str) -> bool {
    matches!(state, NEW | FIX | WAIT | DISMISS)
}

/// 表に行として持つ状態か（未仕分けは行を持たない）。以前の JSON の取り込みでも使う。
pub fn is_stored_state(state: &str) -> bool {
    matches!(state, FIX | WAIT | DISMISS)
}

/// n 件の指摘それぞれの状態。行が無い分は "new"（未仕分け）。表に未知の値があっても "new" に戻す。
/// 読み取りに失敗したときは、**エラーを返す**（全部「未仕分け」に見せない）。成果物（レポート）は、こちらを使う。
pub fn try_get(dir: &Path, review_id: &str, n: usize) -> Result<Vec<String>, String> {
    let conn = db::open(dir)?;
    try_get_with_conn(&conn, review_id, n)
}

/// 既存の接続を使い回して、仕分けを読み取る。
pub fn try_get_with_conn(conn: &rusqlite::Connection, review_id: &str, n: usize) -> Result<Vec<String>, String> {
    let mut out = vec![NEW.to_string(); n];
    let w = |e: rusqlite::Error| format!("仕分けを読み取れません: {e}");
    let mut stmt = conn.prepare("SELECT idx, state FROM triage WHERE review_id = ?1").map_err(w)?;
    let rows = stmt.query_map([review_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).map_err(w)?;
    for row in rows {
        let (idx, state) = row.map_err(w)?;
        if idx >= 0 && (idx as usize) < n && is_stored_state(&state) {
            out[idx as usize] = state;
        }
    }
    Ok(out)
}

/// 画面の表示用。**読み取りに失敗しても、全部「未仕分け」で返す**。成果物（レポート）には使わない（`try_get` を使う）。
/// 失敗は、書き込み（仕分けを変える）のときに、エラーとして出る。
pub fn get(dir: &Path, review_id: &str, n: usize) -> Vec<String> {
    try_get(dir, review_id, n).unwrap_or_else(|_| vec![NEW.to_string(); n])
}

/// 履歴の「だれが」: あなたが操作した／前回のレビューから引き継いだ。
pub const SRC_USER: &str = "user";
pub const SRC_CARRY: &str = "carry";

/// 引き継いだ仕分けを、新しいレビューの初期値として書く（未仕分けは書かない）。履歴には、「引き継ぎ」として残す。
pub fn put(dir: &Path, review_id: &str, states: Vec<String>) -> Result<(), String> {
    if states.iter().all(|s| s == NEW) {
        return Ok(());
    }
    let at = db::now_ms();
    let mut conn = db::open(dir)?;
    let tx = conn.transaction().map_err(|e| format!("仕分けを書き始められない: {e}"))?;
    for (i, s) in states.iter().enumerate().filter(|(_, s)| is_stored_state(s)) {
        tx.execute(
            "INSERT INTO triage (review_id, idx, state) VALUES (?1, ?2, ?3) \
             ON CONFLICT (review_id, idx) DO UPDATE SET state = excluded.state",
            params![review_id, i as i64, s],
        )
        .map_err(|e| format!("仕分けを書けない: {e}"))?;
        tx.execute(
            "INSERT INTO triage_log (review_id, idx, state, at, head, source) VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
            params![review_id, i as i64, s, at as i64, SRC_CARRY],
        )
        .map_err(|e| format!("仕分けの履歴を書けない: {e}"))?;
    }
    tx.commit().map_err(|e| format!("仕分けを確定できない: {e}"))
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn set(dir: &Path, review_id: &str, index: usize, state: &str, n: usize) -> Result<Vec<String>, String> {
    set_with(dir, review_id, index, state, n, db::now_ms(), None)
}

/// 仕分けを変える。履歴に、時刻と、そのときのリポジトリの HEAD（`head`）を残す。
/// 「対応」以外にしたときは、対応済みの印を外す（対応していないものを、済みにしておかない）。
pub fn set_with(dir: &Path, review_id: &str, index: usize, state: &str, n: usize, at: u64, head: Option<&str>) -> Result<Vec<String>, String> {
    if !valid(state) {
        return Err(format!("未知の状態: {state}"));
    }
    if index >= n {
        return Err(format!("指摘 {index} 番は無い（{n} 件）"));
    }
    let mut conn = db::open(dir)?;
    let tx = conn.transaction().map_err(|e| format!("仕分けを書き始められない: {e}"))?;
    let w = |e: rusqlite::Error| format!("仕分けを書けない: {e}");
    if state == NEW {
        tx.execute("DELETE FROM triage WHERE review_id = ?1 AND idx = ?2", params![review_id, index as i64]).map_err(w)?;
    } else {
        tx.execute(
            "INSERT INTO triage (review_id, idx, state) VALUES (?1, ?2, ?3) \
             ON CONFLICT (review_id, idx) DO UPDATE SET state = excluded.state",
            params![review_id, index as i64, state],
        )
        .map_err(w)?;
    }
    if state != FIX {
        tx.execute("UPDATE finding_note SET done_at = NULL, done_head = NULL WHERE review_id = ?1 AND idx = ?2", params![review_id, index as i64]).map_err(w)?;
    }
    tx.execute(
        "INSERT INTO triage_log (review_id, idx, state, at, head, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![review_id, index as i64, state, at as i64, head, SRC_USER],
    )
    .map_err(w)?;
    tx.commit().map_err(|e| format!("仕分けを確定できない: {e}"))?;
    drop(conn);
    Ok(get(dir, review_id, n))
}

/// 仕分けの履歴 1 行。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LogRow {
    pub idx: usize,
    pub state: String,
    pub at: u64,
    pub head: Option<String>,
    /// "user" | "carry"
    pub source: String,
}

/// このレビューの、仕分けの履歴（古い順）。
/// 読み取りに失敗したときは、**エラーを返す**（空の履歴を、「記録なし」に見せない）。
#[cfg_attr(not(test), allow(dead_code))]
pub fn try_log(dir: &Path, review_id: &str) -> Result<Vec<LogRow>, String> {
    let conn = db::open(dir)?;
    try_log_with_conn(&conn, review_id)
}

/// 既存の接続を使い回して、仕分けの履歴（古い順）を読み取る。
pub fn try_log_with_conn(conn: &rusqlite::Connection, review_id: &str) -> Result<Vec<LogRow>, String> {
    let w = |e: rusqlite::Error| format!("仕分けの履歴を読み取れません: {e}");
    let mut stmt = conn.prepare("SELECT idx, state, at, head, source FROM triage_log WHERE review_id = ?1 ORDER BY id").map_err(w)?;
    let rows = stmt
        .query_map([review_id], |r| Ok(LogRow { idx: r.get::<_, i64>(0)?.max(0) as usize, state: r.get(1)?, at: r.get::<_, i64>(2)?.max(0) as u64, head: r.get(3)?, source: r.get(4)? }))
        .map_err(w)?;
    rows.map(|r| r.map_err(w)).collect()
}

/// コピー文面に埋め込む識別子。エージェントの会話に残るので、後から会話の記録を検索して、
/// 「この指摘を渡したセッション」を見つけられる（SyncLane などの記録と結ぶための目印）。
/// 検索でそのまま一致するよう、空白を含まない ASCII にする。
pub fn review_tag(review_id: &str) -> String {
    format!("mrcr:{review_id}")
}

/// 指摘ごとの識別子。番号は、画面で数えた指摘の元の番号（1 始まり）。
pub fn finding_tag(review_id: &str, index0: usize) -> String {
    format!("mrcr:{review_id}#{}", index0 + 1)
}

/// 報告の行の、識別子のあとに添える、観点の名前（`（潜在バグ）`）。
/// アプリを開き直すと、画面は初期化されて、どのレビューの報告か分からなくなる。観点の名前が、報告の行に残っていれば、人が見て分かる。
/// 名前に括弧・改行が入っていると、取り込みが読めなくなるので除く。長いものは、20 文字まで。名前が空なら、添えない。
pub fn review_label(recipe_title: &str) -> String {
    let name: String = recipe_title.chars().filter(|c| !matches!(c, '（' | '）' | '(' | ')' | '\n' | '\r')).collect();
    let name: String = name.trim().chars().take(20).collect();
    let name = name.trim();
    if name.is_empty() {
        String::new()
    } else {
        format!("（{name}）")
    }
}

fn indent(text: &str) -> String {
    text.trim().replace('\n', "\n   ")
}

fn loc(file: &str, line: Option<u32>) -> String {
    match (file.is_empty(), line) {
        (true, _) => "(ファイル不明)".to_string(),
        (false, Some(l)) => format!("{file}:{l}"),
        (false, None) => file.to_string(),
    }
}

/// CUI のエージェントに貼る文面。「対応」だけを依頼し、「待ち」は触らないよう添える。
/// 「対応」が 1 件も無ければ空文字（コピーするものが無い）。
pub fn handoff_text(rec: &ReviewRecord, triage: &[String], app_name: &str) -> String {
    // (指摘の元の番号 0 始まり, 指摘)。識別子の番号は、仕分けの並びではなく元の番号を使う。
    let pick = |state: &str| -> Vec<(usize, &crate::findings::Finding)> {
        rec.findings
            .iter()
            .enumerate()
            .filter(|(i, _)| triage.get(*i).map(String::as_str) == Some(state))
            .collect()
    };
    let fix = pick(FIX);
    if fix.is_empty() {
        return String::new();
    }
    let wait = pick(WAIT);

    let mut out = format!(
        "以下のコードレビューの指摘を直してください。\n（{} ／ {} ／ HEAD {} ／ 差分 {}）\n識別子: {}\n\n## 直す\n",
        app_name,
        rec.recipe.title,
        &rec.snapshot.head[..rec.snapshot.head.len().min(8)],
        &rec.snapshot.diff_hash[..rec.snapshot.diff_hash.len().min(8)],
        review_tag(&rec.id),
    );
    for (n, (orig, f)) in fix.iter().enumerate() {
        // linter の指摘には、規則名も添える（LLM の指摘には無い）
        let rule = f.rule.as_ref().map(|r| format!("   規則: {r}\n")).unwrap_or_default();
        out.push_str(&format!(
            "\n{}. [{}] {}\n   場所: {}\n   引用: {}\n   内容: {}\n{}   識別子: {}\n",
            n + 1,
            f.severity,
            if f.title.is_empty() { "(見出しなし)" } else { &f.title },
            loc(&f.file, f.line),
            indent(&f.evidence),
            indent(&f.detail),
            rule,
            finding_tag(&rec.id, *orig),
        ));
        if !f.verified {
            out.push_str("   ※ 根拠不一致: この引用は差分に見つかりませんでした。実在を確認してから直してください。\n");
        }
    }
    if !wait.is_empty() {
        out.push_str("\n## 今回は直さない（触らないでください）\n");
        for (_, f) in wait {
            out.push_str(&format!(
                "- {}（{}）\n",
                if f.title.is_empty() { "(見出しなし)" } else { &f.title },
                loc(&f.file, f.line)
            ));
        }
    }
    // 末部: 対応の報告の書式。会話の記録を、識別子で検索して、「どの指摘に、どう答えたか」を取り出せるようにする。
    // 「対応した」「対応しなかった」の 2 語に絞る（言い回しのばらつきで、取り出せなくならないように）。
    out.push_str("\n## 最後の報告\n直し終えたら、返事の最後に、「直す」の指摘ごとに、次の形で 1 行ずつ書いてください。識別子（と、そのあとの括弧の観点の名前）は、そのまま写してください。\n");
    let label = review_label(&rec.recipe.title);
    for (orig, _) in &fix {
        out.push_str(&format!("{}{label} 対応した: <何をしたか> ／ 対応しなかった: <理由>\n", finding_tag(&rec.id, *orig)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::findings::Finding;
    use crate::provider::{ProviderInfo, Usage};
    use crate::recipe::Recipe;
    use crate::store::SnapshotRecord;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mrcr-triage-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn finding(title: &str, verified: bool) -> Finding {
        Finding {
            file: "db.js".into(),
            line: Some(2),
            severity: "high".into(),
            title: title.into(),
            detail: "1 行目\n2 行目".into(),
            evidence: "db.query(x)".into(),
            verified,
            rule: None,
        }
    }

    fn rec(findings: Vec<Finding>) -> ReviewRecord {
        ReviewRecord {
            id: "r1".into(),
            created_at: 0,
            repo: "/r".into(),
            recipe: Recipe { id: "security".into(), title: "セキュリティ".into(), source: "builtin".into(), prompt: "".into(), kind: crate::recipe::Kind::Llm, needs_related: false },
            provider: ProviderInfo { id: "mock".into(), label: "m".into(), note: "".into() },
            snapshot: SnapshotRecord { head: "216b161ddb5c".into(), base: Default::default(), diff_hash: "ce494d6b09234".into(), diff_bytes: 1, files: vec![], excluded: vec![] },
            prompt: "".into(),
            model: "".into(),
            response_text: "".into(),
            raw: "".into(),
            usage: Usage::default(),
            cost_usd: None,
            duration_ms: 0,
            findings,
            carries: vec![],
            fates: vec![],
            notes: vec![],
            related: None,
            repo_overrides: vec![],
            settings: None,
            parse_error: None,
            error: None,
        }
    }

    #[test]
    fn defaults_to_new_and_persists_per_review() {
        let d = scratch("persist");
        assert_eq!(get(&d, "r1", 3), vec!["new", "new", "new"]);
        assert_eq!(set(&d, "r1", 1, FIX, 3).unwrap(), vec!["new", "fix", "new"]);
        set(&d, "r1", 2, WAIT, 3).unwrap();
        set(&d, "r2", 0, DISMISS, 1).unwrap();
        assert_eq!(get(&d, "r1", 3), vec!["new", "fix", "wait"]);
        assert_eq!(get(&d, "r2", 1), vec!["dismiss"]);
        // 指摘の数が増えても、足りない分は未仕分け
        assert_eq!(get(&d, "r1", 4), vec!["new", "fix", "wait", "new"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rejects_unknown_state_and_out_of_range_index() {
        let d = scratch("reject");
        assert!(set(&d, "r1", 0, "bogus", 2).is_err());
        assert!(set(&d, "r1", 2, FIX, 2).is_err());
        let n: i64 = db::open(&d).unwrap().query_row("SELECT COUNT(*) FROM triage", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "失敗した操作は何も書かない");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn try_log_and_try_log_with_conn_read_history_in_order() {
        let d = scratch("try_log");
        set_with(&d, "r1", 0, FIX, 2, 1000, Some("head1")).unwrap();
        set_with(&d, "r1", 0, WAIT, 2, 2000, Some("head2")).unwrap();
        let logs = try_log(&d, "r1").unwrap();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].state, FIX);
        assert_eq!(logs[1].state, WAIT);
        let conn = db::open(&d).unwrap();
        let logs_conn = try_log_with_conn(&conn, "r1").unwrap();
        assert_eq!(logs_conn, logs);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn unknown_or_out_of_range_rows_fall_back_to_new() {
        let d = scratch("rows");
        let conn = db::open(&d).unwrap();
        for (idx, state) in [(0, "fix"), (1, "???"), (7, "wait"), (-1, "dismiss")] {
            conn.execute("INSERT INTO triage (review_id, idx, state) VALUES ('r1', ?1, ?2)", params![idx, state]).unwrap();
        }
        drop(conn);
        assert_eq!(get(&d, "r1", 3), vec!["fix", "new", "new"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn setting_back_to_new_removes_the_row_and_put_upserts() {
        let d = scratch("newrow");
        set(&d, "r1", 0, FIX, 2).unwrap();
        assert_eq!(set(&d, "r1", 0, NEW, 2).unwrap(), vec!["new", "new"]);
        let n: i64 = db::open(&d).unwrap().query_row("SELECT COUNT(*) FROM triage", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "未仕分けは行を持たない");

        put(&d, "r1", vec!["new".into(), "wait".into()]).unwrap();
        put(&d, "r1", vec!["dismiss".into(), "wait".into()]).unwrap();
        assert_eq!(get(&d, "r1", 2), vec!["dismiss", "wait"]);
        put(&d, "r2", vec!["new".into()]).unwrap(); // 全部が未仕分けなら何も書かない
        assert_eq!(get(&d, "r2", 1), vec!["new"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn handoff_lists_fix_and_wait_only() {
        let r = rec(vec![finding("A", true), finding("B", true), finding("C", true), finding("D", true)]);
        let t = handoff_text(&r, &["fix".into(), "wait".into(), "dismiss".into(), "new".into()], "MrCodeReview");
        assert!(t.starts_with("以下のコードレビューの指摘を直してください。"));
        assert!(t.contains("HEAD 216b161d ／ 差分 ce494d6b"));
        assert!(t.contains("## 直す\n\n1. [high] A\n   場所: db.js:2\n   引用: db.query(x)\n   内容: 1 行目\n   2 行目\n"));
        assert!(t.contains("## 今回は直さない（触らないでください）\n- B（db.js:2）\n"));
        assert!(t.contains("識別子: mrcr:r1\n"), "レビュー全体の識別子");
        assert!(t.contains("識別子: mrcr:r1#1\n"), "対応にした指摘の識別子");
        assert!(!t.contains("mrcr:r1#2\n"), "待ちの指摘には個別の識別子を付けない");
        // 末部に、報告の書式。「直す」の指摘の識別子だけを、行頭に置く（待ちの指摘は含めない）
        let tail = t.split("## 最後の報告\n").nth(1).expect("末部に報告の書式がある");
        assert!(tail.starts_with("直し終えたら、返事の最後に"), "{tail}");
        let label = review_label(&r.recipe.title);
        assert!(!label.is_empty(), "観点の名前が、報告の行に添えられる");
        assert!(tail.contains(&format!("\nmrcr:r1#1{label} 対応した: <何をしたか> ／ 対応しなかった: <理由>\n")), "{tail}");
        assert!(!tail.contains("mrcr:r1#2"), "待ちの指摘は、報告させない: {tail}");
        assert!(t.trim_end().ends_with("対応しなかった: <理由>"), "報告の書式が、文面の末部");
        assert!(!t.contains("] C") && !t.contains("- C"), "無視は含めない");
        assert!(!t.contains("] D") && !t.contains("- D"), "未仕分けは含めない");
        assert!(!t.contains("根拠不一致"));
    }

    #[test]
    fn the_review_name_added_to_report_lines_is_kept_short_and_never_carries_parentheses_or_newlines() {
        assert_eq!(review_label("潜在バグ"), "（潜在バグ）");
        assert_eq!(review_label("  セキュリティ \n"), "（セキュリティ）");
        // 括弧・改行は除く（取り込みが、括弧の中を、観点の名前として読むため）
        assert_eq!(review_label("独自（社内）観点(A)\nB"), "（独自社内観点AB）");
        // 長いものは、20 文字まで
        assert_eq!(review_label(&"あ".repeat(30)).chars().count(), 20 + 2);
        // 空・括弧だけのときは、添えない
        assert_eq!(review_label(""), "");
        assert_eq!(review_label("（）\n"), "");
    }

    #[test]
    fn handoff_flags_unverified_and_numbers_only_fix_items() {
        let r = rec(vec![finding("X", false), finding("Y", true)]);
        let t = handoff_text(&r, &["dismiss".into(), "fix".into()], "MrCodeReview");
        assert!(t.contains("1. [high] Y"));
        // 文面の連番は 1 から数え直すが、識別子は元の番号（2 番目の指摘）を指す
        assert!(t.contains("識別子: mrcr:r1#2\n"));
        assert!(!t.contains("mrcr:r1#1"));
        let r2 = rec(vec![finding("X", false)]);
        let t2 = handoff_text(&r2, &["fix".into()], "MrCodeReview");
        assert!(t2.contains("※ 根拠不一致: この引用は差分に見つかりませんでした。"));
        assert!(!t2.contains("## 今回は直さない"));
    }

    #[test]
    fn handoff_adds_the_rule_for_linter_findings_only() {
        let mut lint = finding("Unexpected var", true);
        lint.rule = Some("no-var".into());
        let r = rec(vec![lint, finding("LLM の指摘", true)]);
        let t = handoff_text(&r, &["fix".into(), "fix".into()], "MrCodeReview");
        assert!(t.contains("内容: 1 行目\n   2 行目\n   規則: no-var\n   識別子: mrcr:r1#1\n"), "{t}");
        assert!(t.contains("2. [high] LLM の指摘"));
        assert_eq!(t.matches("規則:").count(), 1, "LLM の指摘には規則の行を付けない");
    }

    #[test]
    fn handoff_names_the_app_that_made_it() {
        let r = rec(vec![finding("A", true)]);
        let free = handoff_text(&r, &["fix".into()], "MrCodeReview");
        let pro = handoff_text(&r, &["fix".into()], "MrkCodeReview");
        assert!(free.contains("（MrCodeReview ／ ") && !free.contains("MrkCodeReview"));
        assert!(pro.contains("（MrkCodeReview ／ ") && !pro.contains("（MrCodeReview"));
    }

    #[test]
    fn handoff_is_empty_without_fix_items() {
        let r = rec(vec![finding("A", true)]);
        assert_eq!(handoff_text(&r, &["wait".into()], "MrCodeReview"), "");
        assert_eq!(handoff_text(&r, &[], "MrCodeReview"), "");
    }
}
