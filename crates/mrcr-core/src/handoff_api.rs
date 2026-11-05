//! 「渡した」状態の記録と、エージェントの申告の受け取り（CLI の `mrcr handoff` と、後続の MCP の土台。設計: MrkCodeReview の `notes/MCP化設計.md`）。
//!
//! - **渡した状態**は、`meta` 表のキー `handoff_release:<review_id>` に、JSON で持つ。**スキーマの版は上げない**
//!   （版を上げると、古い `mrcr` が、「新しい版のアプリが作ったデータベース」として、開けなくなるため。`db.rs`）。
//! - 渡す番号（`fix`・`hold`）は、**呼び出し側が決めて渡す**。何を「渡す」対象にするか（「対応」で、直し済みを除く、など）は、
//!   GUI の `handoff_triage` にある。ここは、受け取った番号を、検証して、記録するだけ。
//! - 取得できるのは、**渡された指摘だけ**。報告を書けるのは、**渡された「直す」の指摘だけ**。
//! - 申告は、既存の `agent_report` 表に、`source = "mcp:<クライアント名>"` で保存する。**申告は、事実ではない**（事実は git の差分）。
//! - この層は、入口（CLI・MCP）を知らない。

use crate::agent_report::{self, AgentReport, DID, SKIPPED, TEXT_MAX};
use crate::db;
use crate::findings::Finding;
use crate::snapshot::{self, Limits};
use crate::store::{self, ReviewRecord};
use crate::triage;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;

const KEY_PREFIX: &str = "handoff_release:";
const ID_MAX: usize = 64;
const NAME_MAX: usize = 40;

/// 「渡す」を押した時点の記録。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub review_id: String,
    /// 渡した時刻（UNIX ミリ秒）
    pub released_at: u64,
    /// 「直す」として渡した指摘の番号（0 始まり。昇順・重複なし）
    pub fix: Vec<usize>,
    /// 「今回は直さない」として、伝えるだけの指摘の番号（0 始まり。昇順・重複なし）
    pub hold: Vec<usize>,
    /// 渡した時点の、リポジトリの HEAD。**取れなかったときは None**（空文字にすり替えない）
    #[serde(default)]
    pub head: Option<String>,
    /// レビューした時点の、差分の識別子
    pub diff_hash: String,
    /// だれが渡したか（`gui` など）
    pub by: String,
}

/// 渡された指摘 1 件。
#[derive(Debug, Clone, PartialEq)]
pub struct ReleasedFinding {
    /// 0 始まりの番号
    pub index: usize,
    /// `mrcr:<レビュー>#<番号>`（番号は 1 始まり。エージェントに見せる識別子）
    pub id: String,
    pub finding: Finding,
    /// **今回の渡し分**への、エージェントの申告（渡した時刻以降のもの）
    pub reported: Option<AgentReport>,
    /// **前回以前の渡し分**の申告（渡した時刻より前のもの）。今回の申告とは、見せ方を分ける
    pub previous_report: Option<AgentReport>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Handoff {
    pub release: Release,
    pub repo: String,
    pub recipe_id: String,
    pub recipe_title: String,
    pub fix: Vec<ReleasedFinding>,
    pub hold: Vec<ReleasedFinding>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseSummary {
    pub review_id: String,
    pub repo: String,
    pub recipe_id: String,
    pub recipe_title: String,
    pub released_at: u64,
    pub fix_count: usize,
    pub hold_count: usize,
    pub by: String,
}

/// 一覧で読めなかった記録の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnreadableKind {
    /// キーの識別子が、正しくない
    BadKey,
    /// 保存された JSON が、壊れている（読めない）
    Corrupt,
    /// 渡した記録は読めたが、そのレビューが、もう無い（リポジトリのデータを消した、など）
    ReviewMissing,
}

/// 一覧で読めなかった記録。黙って落とさず、種類と理由を返す。
#[derive(Debug, Clone, PartialEq)]
pub struct Unreadable {
    pub review_id: String,
    pub kind: UnreadableKind,
    pub reason: String,
}

/// 報告を保存した結果。
#[derive(Debug, Clone, PartialEq)]
pub struct ReportOutcome {
    /// 文章が上限（`TEXT_MAX`）を超えて、切り詰めたか
    pub truncated: bool,
    /// 保存で、変わった件数（同じ内容の再提出なら 0）
    pub changed: usize,
}

/// 渡した時点の差分と、いまの差分が、同じか。
#[derive(Debug, Clone, PartialEq)]
pub enum Staleness {
    Same,
    /// 違う。渡した後に、作業ツリーが動いた（エージェントが直した後も、これになる）。
    /// コミットだけでは、これにならない（比較元は、レビューのときに解決した、固定のコミットなので、差分が変わらない）
    Changed,
    /// 調べられなかった（リポジトリが消えた、など）。理由つき
    Unknown(String),
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= ID_MAX && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= NAME_MAX && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn key(review_id: &str) -> String {
    format!("{KEY_PREFIX}{review_id}")
}

fn sorted_unique(v: &[usize]) -> Vec<usize> {
    let mut out = v.to_vec();
    out.sort_unstable();
    out.dedup();
    out
}

fn parse_release(review_id: &str, json: &str) -> Result<Release, String> {
    let mut r: Release = serde_json::from_str(json).map_err(|e| format!("渡した記録を読めません（{review_id}）: {e}"))?;
    r.review_id = review_id.to_string(); // 本物の鍵は、キー。中の値とずれていても、キーを信じる
    Ok(r)
}

fn read_release(conn: &rusqlite::Connection, review_id: &str) -> Result<Option<Release>, String> {
    match conn.query_row("SELECT value FROM meta WHERE key = ?1", [key(review_id)], |r| r.get::<_, String>(0)) {
        Ok(json) => parse_release(review_id, &json).map(Some),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("渡した記録を読み取れません: {e}")),
    }
}

/// 「渡した」ことを記録する。同じレビューを、もう一度渡すと、置き換える。
///
/// - `fix`: 直す指摘の番号（0 始まり）。空ならエラー。
/// - `hold`: 今回は直さない（伝えるだけ）指摘の番号。`fix` と重ならない。
/// - どちらも、指摘の件数の範囲内であること。
pub fn release(dir: &Path, review_id: &str, fix: &[usize], hold: &[usize], by: &str, now: u64) -> Result<Release, String> {
    if !valid_id(review_id) {
        return Err(format!("レビューの識別子が正しくありません: {review_id}"));
    }
    if !valid_name(by) {
        return Err(format!("渡した人の名前が正しくありません: {by}"));
    }
    let rec = store::get(dir, review_id).ok_or_else(|| format!("レビューが見つかりません: {review_id}"))?;
    let n = rec.findings.len();
    let fix = sorted_unique(fix);
    let hold = sorted_unique(hold);
    if fix.is_empty() {
        return Err("渡す指摘がありません（「直す」が 0 件）".to_string());
    }
    if let Some(bad) = fix.iter().chain(hold.iter()).find(|&&i| i >= n) {
        return Err(format!("指摘の番号が範囲外です: {bad}（指摘は {n} 件）"));
    }
    if let Some(dup) = fix.iter().find(|i| hold.contains(i)) {
        return Err(format!("同じ指摘が、「直す」と「直さない」の両方に入っています: {dup}"));
    }
    let r = Release {
        review_id: review_id.to_string(),
        released_at: now,
        fix,
        hold,
        head: snapshot::head_of(&rec.repo),
        diff_hash: rec.snapshot.diff_hash.clone(),
        by: by.to_string(),
    };
    let json = serde_json::to_string(&r).map_err(|e| format!("渡した記録を作れません: {e}"))?;
    let conn = db::open(dir)?;
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key(review_id), json],
    )
    .map_err(|e| format!("渡した記録を保存できません: {e}"))?;
    Ok(r)
}

/// 「渡した」を取り消す。取り消したら true、もともと渡していなければ false。
pub fn revoke(dir: &Path, review_id: &str) -> Result<bool, String> {
    if !valid_id(review_id) {
        return Err(format!("レビューの識別子が正しくありません: {review_id}"));
    }
    let conn = db::open(dir)?;
    let n = conn
        .execute("DELETE FROM meta WHERE key = ?1", [key(review_id)])
        .map_err(|e| format!("渡した記録を取り消せません: {e}"))?;
    Ok(n > 0)
}

fn released_findings(rec: &ReviewRecord, indices: &[usize], reports: &[Option<AgentReport>], released_at: u64) -> Result<Vec<ReleasedFinding>, String> {
    indices
        .iter()
        .map(|&i| {
            let finding = rec.findings.get(i).ok_or_else(|| format!("渡した番号が、レビューの指摘にありません: {i}"))?.clone();
            // 渡した時刻より前の申告は、前回以前の渡し分のもの。今回の申告に見せない。
            let (reported, previous_report) = match reports.get(i).cloned().flatten() {
                Some(r) if r.reported_at >= released_at => (Some(r), None),
                Some(r) => (None, Some(r)),
                None => (None, None),
            };
            Ok(ReleasedFinding { index: i, id: triage::finding_tag(&rec.id, i), finding, reported, previous_report })
        })
        .collect()
}

/// 渡された指摘を取る。渡していなければ None。**渡されていない指摘は、含まれない。**
pub fn get(dir: &Path, review_id: &str) -> Result<Option<Handoff>, String> {
    if !valid_id(review_id) {
        return Err(format!("レビューの識別子が正しくありません: {review_id}"));
    }
    let conn = db::open(dir)?;
    let Some(release) = read_release(&conn, review_id)? else { return Ok(None) };
    let rec = store::get(dir, review_id).ok_or_else(|| format!("渡した記録はありますが、レビューが見つかりません: {review_id}"))?;
    let reports = agent_report::try_get_all_with_conn(&conn, review_id, rec.findings.len())?;
    let fix = released_findings(&rec, &release.fix, &reports, release.released_at)?;
    let hold = released_findings(&rec, &release.hold, &reports, release.released_at)?;
    Ok(Some(Handoff { repo: rec.repo.clone(), recipe_id: rec.recipe.id.clone(), recipe_title: rec.recipe.title.clone(), release, fix, hold }))
}

/// 渡されたレビューの一覧（渡した時刻の新しい順）。`repo` を渡すと、そのリポジトリだけ。
///
/// 1 回の SQL で、`meta`（渡した記録）と `reviews` の**列**（リポジトリ・観点）だけを読む。レビュー全体の JSON（プロンプトと応答の全文を含む）は読まない。
/// 読めなかった記録は、黙って落とさず、**種類と理由つきで**返す。
pub fn list(dir: &Path, repo: Option<&str>) -> Result<(Vec<ReleaseSummary>, Vec<Unreadable>), String> {
    let conn = db::open(dir)?;
    let w = |e: rusqlite::Error| format!("渡した記録の一覧を読み取れません: {e}");
    let mut stmt = conn
        .prepare(
            "SELECT m.key, m.value, r.repo, r.recipe_id, r.recipe_title \
             FROM meta m LEFT JOIN reviews r ON r.id = substr(m.key, ?1 + 1) \
             WHERE substr(m.key, 1, ?1) = ?2",
        )
        .map_err(w)?;
    type Row = (String, String, Option<String>, Option<String>, Option<String>);
    let rows: Vec<Row> = stmt
        .query_map(params![KEY_PREFIX.len() as i64, KEY_PREFIX], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .map_err(w)?
        .collect::<Result<_, _>>()
        .map_err(w)?;

    let mut items = Vec::new();
    let mut unreadable = Vec::new();
    for (k, value, rec_repo, recipe_id, recipe_title) in rows {
        let id = k[KEY_PREFIX.len()..].to_string();
        if !valid_id(&id) {
            unreadable.push(Unreadable { review_id: id, kind: UnreadableKind::BadKey, reason: "識別子が正しくありません".to_string() });
            continue;
        }
        let release = match parse_release(&id, &value) {
            Ok(r) => r,
            Err(reason) => {
                unreadable.push(Unreadable { review_id: id, kind: UnreadableKind::Corrupt, reason });
                continue;
            }
        };
        let Some(rec_repo) = rec_repo else {
            unreadable.push(Unreadable { review_id: id, kind: UnreadableKind::ReviewMissing, reason: "渡した記録はありますが、レビューが見つかりません".to_string() });
            continue;
        };
        if repo.is_some_and(|p| p != rec_repo) {
            continue;
        }
        items.push(ReleaseSummary {
            review_id: id,
            repo: rec_repo,
            recipe_id: recipe_id.unwrap_or_default(),
            recipe_title: recipe_title.unwrap_or_default(),
            released_at: release.released_at,
            fix_count: release.fix.len(),
            hold_count: release.hold.len(),
            by: release.by,
        });
    }
    items.sort_by(|a, b| b.released_at.cmp(&a.released_at).then_with(|| a.review_id.cmp(&b.review_id)));
    unreadable.sort_by(|a, b| a.review_id.cmp(&b.review_id));
    Ok((items, unreadable))
}

/// エージェントの申告を保存する。**渡された「直す」の指摘にだけ**書ける。
///
/// - `verdict`: `"did"`（直した）か `"skipped"`（直さなかった）。
/// - `text`: 何をしたか・なぜ直さなかったか。空はエラー。`TEXT_MAX` 文字を超えたら、切り詰めて、`truncated` で知らせる。
/// - `client`: エージェントの名前（英数字と `-_.`、40 文字まで）。`source` は `mcp:<client>` になる。
pub fn report(dir: &Path, review_id: &str, index: usize, verdict: &str, text: &str, client: &str, now: u64) -> Result<ReportOutcome, String> {
    if !valid_id(review_id) {
        return Err(format!("レビューの識別子が正しくありません: {review_id}"));
    }
    if verdict != DID && verdict != SKIPPED {
        return Err(format!("verdict は \"{DID}\" か \"{SKIPPED}\" です: {verdict}"));
    }
    if !valid_name(client) {
        return Err(format!("クライアントの名前が正しくありません: {client}"));
    }
    let text = text.trim();
    if text.is_empty() {
        return Err("報告の文章が空です（何をしたか、なぜ直さなかったかを書いてください）".to_string());
    }
    let release = {
        let conn = db::open(dir)?;
        read_release(&conn, review_id)?
    }
    .ok_or_else(|| format!("このレビューは、渡されていません: {review_id}"))?;
    if !release.fix.contains(&index) {
        return Err(format!("渡された「直す」の指摘ではありません: {index}"));
    }
    let truncated = text.chars().count() > TEXT_MAX;
    let r = AgentReport { verdict: verdict.to_string(), text: text.chars().take(TEXT_MAX).collect(), reported_at: now, source: format!("mcp:{client}") };
    let changed = agent_report::save(dir, review_id, &[(index, r)])?;
    Ok(ReportOutcome { truncated, changed })
}

/// レビューした時点の差分と、いまの差分を比べる（`git` を読むだけ。何も書かない）。
/// 比較元は、レビューのときに解決したコミットで固定する（GUI の `current_diff` と同じ）。
/// `limits` は、そのレビューで使った上限と、同じものを渡す（違うと、差分が違って見える）。
pub fn staleness(rec: &ReviewRecord, limits: &Limits) -> Staleness {
    let base = if rec.snapshot.base.hash.is_empty() { rec.snapshot.head.clone() } else { rec.snapshot.base.hash.clone() };
    match snapshot::take_with_base(&rec.repo, limits, Some(&base)) {
        Ok(s) if s.diff_hash == rec.snapshot.diff_hash => Staleness::Same,
        Ok(_) => Staleness::Changed,
        Err(e) => Staleness::Unknown(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::sample_record;
    use std::path::PathBuf;
    use std::process::Command;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mrcr-handoffapi-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn finding(n: usize) -> Finding {
        Finding {
            file: format!("src/f{n}.rs"),
            line: Some(n as u32 + 1),
            severity: "medium".into(),
            title: format!("見出し{n}"),
            detail: format!("詳細{n}"),
            evidence: format!("let x{n} = 1;"),
            verified: true,
            rule: None,
        }
    }

    /// 指摘が `n` 件のレビューを、保存先に作る。
    fn make_review(dir: &Path, id: &str, n: usize) -> ReviewRecord {
        let mut rec = sample_record(id, 1000);
        rec.findings = (0..n).map(finding).collect();
        store::append(dir, &rec).unwrap();
        rec
    }

    /// 渡した記録を、検証を通さずに、直接書く（壊れた・ずれた記録を作るため）。
    fn insert_release(dir: &Path, r: &Release) {
        let json = serde_json::to_string(r).unwrap();
        db::open(dir)
            .unwrap()
            .execute("INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value", params![key(&r.review_id), json])
            .unwrap();
    }

    fn raw_release(review_id: &str, fix: Vec<usize>, hold: Vec<usize>) -> Release {
        Release { review_id: review_id.into(), released_at: 1, fix, hold, head: None, diff_hash: "d".into(), by: "gui".into() }
    }

    fn user_version(dir: &Path) -> i64 {
        db::open(dir).unwrap().query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
    }

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn release_then_get_returns_only_the_released_findings() {
        let d = tmp("get");
        make_review(&d, "r1", 4);
        let r = release(&d, "r1", &[2, 0], &[3], "gui", 5000).unwrap();
        assert_eq!((r.fix.clone(), r.hold.clone()), (vec![0, 2], vec![3]), "番号は昇順にそろう");
        let h = get(&d, "r1").unwrap().expect("渡している");
        assert_eq!(h.fix.iter().map(|f| f.index).collect::<Vec<_>>(), vec![0, 2]);
        assert_eq!(h.hold.iter().map(|f| f.index).collect::<Vec<_>>(), vec![3]);
        assert_eq!(h.fix[0].id, "mrcr:r1#1");
        assert_eq!(h.fix[1].id, "mrcr:r1#3");
        assert_eq!(h.fix[0].finding.title, "見出し0");
        assert!(h.fix.iter().chain(h.hold.iter()).all(|f| f.index != 1), "渡していない指摘（1 番）は、含まれない");
        assert_eq!((h.recipe_id.as_str(), h.repo.as_str()), ("security", "/r"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn get_without_release_is_none_and_an_unknown_review_cannot_be_released() {
        let d = tmp("none");
        make_review(&d, "r1", 2);
        assert_eq!(get(&d, "r1").unwrap(), None, "「対応」にしただけでは、何も渡らない");
        assert!(release(&d, "nope", &[0], &[], "gui", 1).unwrap_err().contains("見つかりません"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn release_validates_the_numbers_and_the_names() {
        let d = tmp("valid");
        make_review(&d, "r1", 3);
        assert!(release(&d, "r1", &[], &[1], "gui", 1).unwrap_err().contains("0 件"), "直すが空");
        assert!(release(&d, "r1", &[3], &[], "gui", 1).unwrap_err().contains("範囲外"));
        assert!(release(&d, "r1", &[0], &[7], "gui", 1).unwrap_err().contains("範囲外"));
        assert!(release(&d, "r1", &[1], &[1], "gui", 1).unwrap_err().contains("両方"));
        assert!(release(&d, "r1", &[0], &[], "a b", 1).is_err(), "名前に空白");
        assert!(release(&d, "../x", &[0], &[], "gui", 1).is_err(), "識別子に記号");
        assert_eq!(get(&d, "r1").unwrap(), None, "失敗した呼び出しは、何も記録しない");
        let r = release(&d, "r1", &[1, 1, 1], &[0, 0], "gui", 1).unwrap();
        assert_eq!((r.fix, r.hold), (vec![1], vec![0]), "重複は 1 つにまとまる");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn releasing_again_replaces_the_previous_release() {
        let d = tmp("again");
        make_review(&d, "r1", 4);
        release(&d, "r1", &[0, 1], &[], "gui", 1).unwrap();
        release(&d, "r1", &[3], &[2], "gui", 2).unwrap();
        let h = get(&d, "r1").unwrap().unwrap();
        assert_eq!(h.fix.iter().map(|f| f.index).collect::<Vec<_>>(), vec![3]);
        assert_eq!(h.release.released_at, 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn revoke_removes_the_release_and_blocks_reports() {
        let d = tmp("revoke");
        make_review(&d, "r1", 2);
        assert!(!revoke(&d, "r1").unwrap(), "渡していないものを取り消しても、false");
        release(&d, "r1", &[0], &[], "gui", 1).unwrap();
        assert!(revoke(&d, "r1").unwrap());
        assert_eq!(get(&d, "r1").unwrap(), None);
        assert!(report(&d, "r1", 0, DID, "直した", "claude-code", 9).unwrap_err().contains("渡されていません"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn report_works_only_for_released_fix_findings() {
        let d = tmp("report");
        make_review(&d, "r1", 4);
        assert!(report(&d, "r1", 0, DID, "直した", "claude-code", 9).unwrap_err().contains("渡されていません"), "渡す前は書けない");
        release(&d, "r1", &[0, 2], &[3], "gui", 5).unwrap();
        assert!(report(&d, "r1", 1, DID, "直した", "claude-code", 9).unwrap_err().contains("渡された"), "渡していない指摘");
        assert!(report(&d, "r1", 3, DID, "直した", "claude-code", 9).unwrap_err().contains("渡された"), "「直さない」で渡した指摘にも、書けない");
        assert!(report(&d, "r1", 9, DID, "直した", "claude-code", 9).is_err(), "範囲外");
        assert!(report(&d, "r1", 0, "done", "直した", "claude-code", 9).unwrap_err().contains("verdict"));
        assert!(report(&d, "r1", 0, DID, "   ", "claude-code", 9).unwrap_err().contains("空"));
        assert!(report(&d, "r1", 0, DID, "直した", "bad name", 9).is_err());
        assert_eq!(agent_report::get_all(&d, "r1", 4), vec![None, None, None, None], "失敗した呼び出しは、何も保存しない");

        let o = report(&d, "r1", 0, DID, "  境界を確かめる分岐を足した  ", "claude-code", 10).unwrap();
        assert_eq!((o.truncated, o.changed), (false, 1));
        let saved = agent_report::get_all(&d, "r1", 4);
        let r0 = saved[0].clone().expect("保存された");
        assert_eq!((r0.verdict.as_str(), r0.text.as_str(), r0.source.as_str(), r0.reported_at), ("did", "境界を確かめる分岐を足した", "mcp:claude-code", 10));
        assert!(saved[1].is_none() && saved[2].is_none() && saved[3].is_none());

        // 同じ内容の再提出は、変えない。別の内容は、新しい申告に置き換わる。
        assert_eq!(report(&d, "r1", 0, DID, "境界を確かめる分岐を足した", "claude-code", 10).unwrap().changed, 0);
        report(&d, "r1", 0, SKIPPED, "前提が成り立たないので、直さなかった", "claude-code", 11).unwrap();
        assert_eq!(agent_report::get_all(&d, "r1", 4)[0].clone().unwrap().verdict, "skipped");
        // get に、申告が出る（渡した時刻 5 より後なので、今回の申告）
        let h = get(&d, "r1").unwrap().unwrap();
        assert_eq!(h.fix[0].reported.as_ref().map(|r| r.verdict.as_str()), Some("skipped"));
        assert!(h.fix[0].previous_report.is_none());
        assert!(h.fix[1].reported.is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_previous_rounds_report_is_kept_apart_from_this_rounds() {
        let d = tmp("prev");
        make_review(&d, "r1", 2);
        release(&d, "r1", &[0, 1], &[], "gui", 100).unwrap();
        report(&d, "r1", 0, SKIPPED, "前提が違うので直さない", "claude-code", 200).unwrap();
        revoke(&d, "r1").unwrap();
        release(&d, "r1", &[0], &[], "gui", 300).unwrap(); // 渡し直し
        let h = get(&d, "r1").unwrap().unwrap();
        assert!(h.fix[0].reported.is_none(), "前回の渡し分の申告を、今回の申告として見せない");
        assert_eq!(h.fix[0].previous_report.as_ref().map(|r| (r.verdict.as_str(), r.reported_at)), Some(("skipped", 200)), "前回の申告は、別に見える");
        // 今回の申告が付けば、今回の側に出て、前回の側は空になる（置き換わる）
        report(&d, "r1", 0, DID, "直した", "claude-code", 400).unwrap();
        let h = get(&d, "r1").unwrap().unwrap();
        assert_eq!(h.fix[0].reported.as_ref().map(|r| r.reported_at), Some(400));
        assert!(h.fix[0].previous_report.is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn head_is_none_when_it_cannot_be_read_and_the_real_head_otherwise() {
        let d = tmp("head");
        make_review(&d, "r1", 1); // repo は "/r"（git のリポジトリではない）
        assert_eq!(release(&d, "r1", &[0], &[], "gui", 1).unwrap().head, None, "取れなかったことを、空文字にすり替えない");

        let repo = d.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "init"]);
        let repo_s = repo.to_string_lossy().to_string();
        let mut rec = sample_record("r2", 1000);
        rec.repo = repo_s.clone();
        rec.findings = vec![finding(0)];
        store::append(&d, &rec).unwrap();
        let r = release(&d, "r2", &[0], &[], "gui", 1).unwrap();
        assert!(r.head.is_some());
        assert_eq!(r.head, snapshot::head_of(&repo_s));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 文章の長さの境界。`>` と `>=` の取り違え、trim の順序を固定する。
    #[test]
    fn the_text_limit_boundary_is_exact() {
        let d = tmp("limit");
        make_review(&d, "r1", 1);
        release(&d, "r1", &[0], &[], "gui", 1).unwrap();
        let stored = |d: &Path| agent_report::get_all(d, "r1", 1)[0].clone().unwrap().text.chars().count();

        let exact = "あ".repeat(TEXT_MAX);
        let o = report(&d, "r1", 0, DID, &exact, "claude-code", 2).unwrap();
        assert!(!o.truncated, "ちょうど上限は、切り詰めない");
        assert_eq!(stored(&d), TEXT_MAX);

        let over = "あ".repeat(TEXT_MAX + 1);
        let o = report(&d, "r1", 0, DID, &over, "claude-code", 3).unwrap();
        assert!(o.truncated, "1 文字超えたら、切り詰めて、知らせる");
        assert_eq!(stored(&d), TEXT_MAX);

        let padded = format!("  {}  ", "い".repeat(TEXT_MAX));
        let o = report(&d, "r1", 0, DID, &padded, "claude-code", 4).unwrap();
        assert!(!o.truncated, "前後の空白は、数える前に除く");
        assert_eq!(stored(&d), TEXT_MAX);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 識別子（64）と名前（40）の上限、非 ASCII の拒否。`release` だけでなく、`get`・`revoke`・`report` にも。
    #[test]
    fn id_and_name_limits_are_enforced_everywhere() {
        let d = tmp("limits");
        let id64 = "a".repeat(64);
        let id65 = "a".repeat(65);
        make_review(&d, &id64, 1);
        assert!(release(&d, &id64, &[0], &[], "gui", 1).is_ok(), "64 文字は通る");
        assert!(get(&d, &id64).unwrap().is_some());

        assert!(release(&d, &id65, &[0], &[], "gui", 1).unwrap_err().contains("識別子"));
        assert!(get(&d, &id65).unwrap_err().contains("識別子"));
        assert!(revoke(&d, &id65).unwrap_err().contains("識別子"));
        assert!(report(&d, &id65, 0, DID, "直した", "claude-code", 1).unwrap_err().contains("識別子"));
        for bad in ["../x", "a b", "日本語"] {
            assert!(release(&d, bad, &[0], &[], "gui", 1).is_err(), "release {bad}");
            assert!(get(&d, bad).is_err(), "get {bad}");
            assert!(revoke(&d, bad).is_err(), "revoke {bad}");
            assert!(report(&d, bad, 0, DID, "直した", "claude-code", 1).is_err(), "report {bad}");
        }

        assert!(report(&d, &id64, 0, DID, "直した", &"c".repeat(40), 2).is_ok(), "40 文字の名前は通る");
        assert!(report(&d, &id64, 0, DID, "直した", &"c".repeat(41), 2).unwrap_err().contains("名前"));
        assert!(report(&d, &id64, 0, DID, "直した", "クロード", 2).unwrap_err().contains("名前"), "非 ASCII");
        assert!(release(&d, &id64, &[0], &[], &"b".repeat(41), 3).unwrap_err().contains("名前"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn get_fails_loudly_when_a_released_number_is_outside_the_review() {
        let d = tmp("range");
        make_review(&d, "r1", 2);
        insert_release(&d, &raw_release("r1", vec![5], vec![]));
        assert!(get(&d, "r1").unwrap_err().contains("指摘にありません"), "直す側の番号が範囲外");
        insert_release(&d, &raw_release("r1", vec![0], vec![9]));
        assert!(get(&d, "r1").unwrap_err().contains("指摘にありません"), "直さない側の番号が範囲外");
        insert_release(&d, &raw_release("r1", vec![0], vec![1]));
        assert!(get(&d, "r1").unwrap().is_some(), "範囲内なら通る");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn list_is_newest_first_and_filters_by_repo() {
        let d = tmp("list");
        make_review(&d, "a1", 2);
        let mut other = sample_record("b1", 2000);
        other.repo = "/other".into();
        other.findings = vec![finding(0)];
        store::append(&d, &other).unwrap();
        assert_eq!(list(&d, None).unwrap(), (vec![], vec![]));
        release(&d, "a1", &[0, 1], &[], "gui", 100).unwrap();
        release(&d, "b1", &[0], &[], "gui", 200).unwrap();
        let (all, bad) = list(&d, None).unwrap();
        assert!(bad.is_empty());
        assert_eq!(all.iter().map(|s| s.review_id.as_str()).collect::<Vec<_>>(), vec!["b1", "a1"], "渡した時刻の新しい順");
        assert_eq!((all[1].fix_count, all[1].hold_count, all[1].repo.as_str(), all[1].recipe_id.as_str()), (2, 0, "/r", "security"));
        let (only, _) = list(&d, Some("/other")).unwrap();
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].review_id, "b1");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 「レビューが消えた」「壊れた」「識別子が不正」を、種類と理由つきで区別する。
    /// 「レビューが消えた」は、**正しい渡した記録を保存したあとで、レビューだけを消して**作る（壊れた JSON とは別の経路）。
    #[test]
    fn list_says_why_each_unreadable_entry_could_not_be_read() {
        let d = tmp("bad");
        make_review(&d, "ok1", 1);
        make_review(&d, "gone", 1);
        release(&d, "ok1", &[0], &[], "gui", 1).unwrap();
        release(&d, "gone", &[0], &[], "gui", 2).unwrap();
        let conn = db::open(&d).unwrap();
        conn.execute("DELETE FROM reviews WHERE id = 'gone'", []).unwrap(); // 渡した記録は正しいが、レビューが無い
        conn.execute("INSERT INTO meta (key, value) VALUES (?1, 'これはJSONではない')", [key("broken")]).unwrap(); // 壊れた JSON
        conn.execute("INSERT INTO meta (key, value) VALUES ('handoff_release:a b', '{}')", []).unwrap(); // 識別子が不正
        conn.execute("INSERT INTO meta (key, value) VALUES ('agent_log_consent_v2', '1') ON CONFLICT (key) DO NOTHING", []).unwrap(); // 無関係

        let (items, bad) = list(&d, None).unwrap();
        assert_eq!(items.iter().map(|s| s.review_id.as_str()).collect::<Vec<_>>(), vec!["ok1"]);
        let find = |id: &str| bad.iter().find(|u| u.review_id == id).unwrap_or_else(|| panic!("{id} が出ていない: {bad:?}"));
        assert_eq!(find("gone").kind, UnreadableKind::ReviewMissing);
        assert_eq!(find("broken").kind, UnreadableKind::Corrupt);
        assert_eq!(find("a b").kind, UnreadableKind::BadKey);
        assert!(bad.iter().all(|u| !u.reason.is_empty()), "理由が付く");
        assert!(find("broken").reason.contains("読めません"));
        assert_eq!(bad.len(), 3, "無関係な meta キー（agent_log_consent_v2）は、数えない");

        assert!(get(&d, "gone").unwrap_err().contains("見つかりません"), "正しい記録＋レビュー無し → 「渡していない」に見せない");
        assert!(get(&d, "broken").unwrap_err().contains("読めません"), "壊れた記録 → 「渡していない」に見せない");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 一覧は、列だけを読む。レビュー全体の JSON（record）が壊れていても、一覧は出る。
    #[test]
    fn list_reads_only_the_light_columns_not_the_whole_record() {
        let d = tmp("light");
        make_review(&d, "r1", 1);
        release(&d, "r1", &[0], &[], "gui", 1).unwrap();
        db::open(&d).unwrap().execute("UPDATE reviews SET record = 'これは壊れたJSON' WHERE id = 'r1'", []).unwrap();
        let (items, bad) = list(&d, None).unwrap();
        assert_eq!((items.len(), bad.len()), (1, 0), "列（repo・観点）だけで、一覧が作れる");
        assert_eq!((items[0].repo.as_str(), items[0].recipe_title.as_str()), ("/r", "セキュリティ"));
        assert!(get(&d, "r1").is_err(), "1 件を取るときは、全体を読むので、壊れていれば、エラー");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn release_adds_no_schema_change_and_leaves_other_tables_alone() {
        let d = tmp("schema");
        make_review(&d, "r1", 3);
        triage::set_with(&d, "r1", 0, triage::FIX, 3, 1, None).unwrap();
        let before = (user_version(&d), triage::get(&d, "r1", 3), agent_report::get_all(&d, "r1", 3));
        release(&d, "r1", &[0], &[], "gui", 1).unwrap();
        revoke(&d, "r1").unwrap();
        release(&d, "r1", &[0], &[], "gui", 2).unwrap();
        let after = (user_version(&d), triage::get(&d, "r1", 3), agent_report::get_all(&d, "r1", 3));
        assert_eq!(before, after, "スキーマの版も、仕分けも、申告も、変わらない（古い mrcr が開けなくならない）");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// `staleness` の意味を固定する。**コミットしても、同じ**（比較元が固定のコミットなので、差分が変わらない）。
    #[test]
    fn staleness_compares_the_review_time_diff_with_the_current_one() {
        let d = tmp("stale");
        let repo = d.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "init"]);
        std::fs::write(repo.join("a.txt"), "one\ntwo\n").unwrap(); // 未コミットの変更

        let repo_s = repo.to_string_lossy().to_string();
        let snap = snapshot::take_with(&repo_s, &Limits::default()).unwrap();
        let mut rec = sample_record("r1", 1000);
        rec.repo = repo_s.clone();
        rec.snapshot.head = snap.head.clone();
        rec.snapshot.base = snap.base.clone();
        rec.snapshot.diff_hash = snap.diff_hash.clone();
        let lim = Limits::default();

        assert_eq!(staleness(&rec, &lim), Staleness::Same, "渡した直後");

        let mut no_hash = rec.clone();
        no_hash.snapshot.base.hash = String::new();
        assert_eq!(staleness(&no_hash, &lim), Staleness::Same, "base.hash が空でも、そのときの HEAD で比べる（古い記録）");

        git(&repo, &["commit", "-q", "-am", "agent commit"]);
        assert_eq!(staleness(&rec, &lim), Staleness::Same, "エージェントがコミットしても、固定の比較元からの差分は、同じ");

        std::fs::write(repo.join("a.txt"), "one\ntwo\nthree\n").unwrap(); // さらに編集
        assert_eq!(staleness(&rec, &lim), Staleness::Changed);

        rec.repo = d.join("missing").to_string_lossy().to_string();
        assert!(matches!(staleness(&rec, &lim), Staleness::Unknown(_)), "調べられないときは、同じと言わない");
        let _ = std::fs::remove_dir_all(&d);
    }
}
