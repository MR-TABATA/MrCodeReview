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
use crate::notes::{self, Note};
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
    /// **保存されている申告そのもの**（切り詰めたあとの文章を含む）。同じ内容の再提出（`changed == 0`）では、すでに保存されていたもの。
    /// 呼び出し側は、読み戻さず、これを表示する（読み戻しの失敗を、入力の値で埋めないため）。
    pub saved: AgentReport,
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
    // 読めなかったこと（壊れている・DB の失敗）と、無いことを、区別する（原因を失わせない）。
    let rec = store::try_get(dir, review_id)?.ok_or_else(|| format!("レビューが見つかりません: {review_id}"))?;
    let (fix, hold) = checked_numbers(&rec, fix, hold)?;
    let r = Release {
        review_id: review_id.to_string(),
        released_at: now,
        fix,
        hold,
        head: snapshot::head_of(&rec.repo),
        diff_hash: rec.snapshot.diff_hash.clone(),
        by: by.to_string(),
    };
    write_release(dir, &r)?;
    Ok(r)
}

/// 渡す番号を、昇順・重複なしにして、検証する（空・範囲外・重なり）。
fn checked_numbers(rec: &ReviewRecord, fix: &[usize], hold: &[usize]) -> Result<(Vec<usize>, Vec<usize>), String> {
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
    Ok((fix, hold))
}

fn write_release(dir: &Path, r: &Release) -> Result<(), String> {
    let json = serde_json::to_string(r).map_err(|e| format!("渡した記録を作れません: {e}"))?;
    let conn = db::open(dir)?;
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key(&r.review_id), json],
    )
    .map_err(|e| format!("渡した記録を保存できません: {e}"))?;
    Ok(())
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
    let rec = store::try_get(dir, review_id)?.ok_or_else(|| format!("渡した記録はありますが、レビューが見つかりません: {review_id}"))?;
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
    report_via(dir, review_id, index, verdict, text, client, "mcp", now)
}

/// `report` と同じ。**どの入口から来たか**（`via`。`cli`・`mcp` など。英数字と `-_.`）を、保存元に残す（`source = "<via>:<client>"`）。
#[allow(clippy::too_many_arguments)]
pub fn report_via(dir: &Path, review_id: &str, index: usize, verdict: &str, text: &str, client: &str, via: &str, now: u64) -> Result<ReportOutcome, String> {
    if !valid_name(via) {
        return Err(format!("入口の名前が正しくありません: {via}"));
    }
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
    let stored_text: String = text.chars().take(TEXT_MAX).collect();
    let source = format!("{via}:{client}");
    // **同じ渡し分の中で**、判定・文章・保存元が同じ再提出は、保存しない（時刻だけ動かさない）。
    // 新しい渡し分（古い申告の時刻が、渡した時刻より前）では、同じ文章でも保存する（さもないと、「前回以前の申告」に見えてしまう）。
    if let Some(Some(old)) = agent_report::try_get_all(dir, review_id, index + 1)?.into_iter().nth(index) {
        if old.verdict == verdict && old.text == stored_text && old.source == source && old.reported_at >= release.released_at {
            return Ok(ReportOutcome { truncated, changed: 0, saved: old });
        }
    }
    let r = AgentReport { verdict: verdict.to_string(), text: stored_text, reported_at: now, source };
    let changed = agent_report::save(dir, review_id, &[(index, r.clone())])?;
    Ok(ReportOutcome { truncated, changed, saved: r })
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

// ── 渡す対象の決め方（GUI の `handoff_triage` から移した規則。GUI と CLI が、同じものを使う） ──

/// 「対応済み」「申告で済み」を表す、仕分けの値。`triage` の `fix`・`wait`・`dismiss`・`new` とは別に、渡す対象から外すために使う。
pub const DONE: &str = "done";

/// 渡す対象の仕分けを作る。**「対応」の指摘のうち、あなたが「対応済み」にしたものと、エージェントが「対応した」と申告したものは、
/// 渡し済み・直し済みなので、`done` にして外す**（また渡さない。「今回は直さない」の欄にも、入れない）。
/// 「直さなかった」と申告されたものは、外さない（もう一度、渡す対象になる）。「待ち」「無視」「未仕分け」は、そのまま。
///
/// **規則だけを持つ**（読み取りは、呼び出し側）。GUI は、読み取りに失敗しても画面を開けるよう、寛容に読む。CLI は、`targets` で、厳密に読む。
pub fn handoff_states(triage_states: &[String], reports: &[Option<AgentReport>], note_rows: &[Note]) -> Vec<String> {
    triage_states
        .iter()
        .enumerate()
        .map(|(i, state)| {
            let agent_did = reports.get(i).and_then(Option::as_ref).is_some_and(|r| r.verdict == DID);
            let marked_done = note_rows.get(i).is_some_and(|n| n.done_at.is_some());
            if state == triage::FIX && (agent_did || marked_done) {
                DONE.to_string()
            } else {
                state.clone()
            }
        })
        .collect()
}

/// 渡す番号（0 始まり。昇順）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Targets {
    /// 直す指摘
    pub fix: Vec<usize>,
    /// 「今回は直さない」として、伝えるだけの指摘（`待ち`）
    pub hold: Vec<usize>,
}

/// `handoff_states` の結果から、渡す番号を取る。
pub fn targets_from_states(states: &[String]) -> Targets {
    let pick = |want: &str| states.iter().enumerate().filter(|(_, s)| s.as_str() == want).map(|(i, _)| i).collect::<Vec<_>>();
    Targets { fix: pick(triage::FIX), hold: pick(triage::WAIT) }
}

/// そのレビューの、渡す番号。**読み取りに失敗したら、エラーを返す**（全部「未仕分け」に見せない）。
pub fn targets(dir: &Path, rec: &ReviewRecord) -> Result<Targets, String> {
    let n = rec.findings.len();
    let t = triage::try_get(dir, &rec.id, n)?;
    let reports = agent_report::try_get_all(dir, &rec.id, n)?;
    let note_rows = notes::try_get_all(dir, &rec.id, n)?;
    Ok(targets_from_states(&handoff_states(&t, &reports, &note_rows)))
}

/// `ensure_release` の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ensured {
    /// 新しく記録した（渡した時刻は、いま。新しい渡し分の始まり）
    Created,
    /// すでにある記録の、**対象の番号だけを更新した**（渡した時刻は、そのまま）
    Updated,
    /// 同じ内容だったので、何も書かなかった
    Unchanged,
}

/// 渡した記録を、**無ければ作り、あれば、渡した時刻を動かさずに、対象の番号を更新する**。
///
/// 「渡した時刻」は、**渡し分の始まり**を表す。申告のうち、その時刻より前のものは、「前回以前」として、今回の申告とは別に見せる。
/// - 記録が無い → `release` で作る（`Created`）。**新しい渡し分は、ここか、`revoke` のあとの、作り直しで始まる。**
/// - 記録がある（同じレビュー・同じ差分の識別子）→ `fix`・`hold` が同じなら何もしない（`Unchanged`）。違えば、番号だけ更新（`Updated`）。
///   対象が**減る**（エージェントが「対応した」と申告して、外れた）ときも、**増える**（あなたが、新しく「対応」にした）ときも、渡した時刻は動かさない。
///   動かすと、同じ作業の途中で書いた申告が、「前回以前」に回ってしまう。
/// - 記録の差分の識別子が、レビューのものと違う（別の差分に対して渡した記録）→ 作り直す（`Created`）。
///
/// **読めなかったことを、黙って埋めない**: レビューが読めない・渡した記録が壊れているときは、エラーを返す（記録を作り直して、原因を消さない）。
pub fn ensure_release(dir: &Path, review_id: &str, fix: &[usize], hold: &[usize], by: &str, now: u64) -> Result<(Release, Ensured), String> {
    if !valid_id(review_id) {
        return Err(format!("レビューの識別子が正しくありません: {review_id}"));
    }
    if !valid_name(by) {
        return Err(format!("渡した人の名前が正しくありません: {by}"));
    }
    let rec = store::try_get(dir, review_id)?.ok_or_else(|| format!("レビューが見つかりません: {review_id}"))?;
    let (fix, hold) = checked_numbers(&rec, fix, hold)?;
    let old = {
        let conn = db::open(dir)?;
        read_release(&conn, review_id)? // 壊れていれば、ここで Err（上書きしない）
    };
    match old {
        Some(old) if old.diff_hash == rec.snapshot.diff_hash => {
            if old.fix == fix && old.hold == hold {
                return Ok((old, Ensured::Unchanged));
            }
            let updated = Release { fix, hold, ..old };
            write_release(dir, &updated)?;
            Ok((updated, Ensured::Updated))
        }
        _ => release(dir, review_id, &fix, &hold, by, now).map(|r| (r, Ensured::Created)),
    }
}

/// 渡した記録のすべて（`meta` を 1 回読むだけ）。レビューの ID → 読めた記録、または、読めなかった理由。
/// 一覧で、「渡し済みか」を、レビューごとに DB を開き直さずに調べるため。
pub fn releases(dir: &Path) -> Result<std::collections::HashMap<String, Result<Release, String>>, String> {
    let conn = db::open(dir)?;
    let w = |e: rusqlite::Error| format!("渡した記録を読み取れません: {e}");
    let mut stmt = conn.prepare("SELECT key, value FROM meta WHERE substr(key, 1, ?1) = ?2").map_err(w)?;
    let rows: Vec<(String, String)> = stmt
        .query_map(params![KEY_PREFIX.len() as i64, KEY_PREFIX], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(w)?
        .collect::<Result<_, _>>()
        .map_err(w)?;
    Ok(rows
        .into_iter()
        .map(|(k, v)| {
            let id = k[KEY_PREFIX.len()..].to_string();
            let parsed = if valid_id(&id) { parse_release(&id, &v) } else { Err("識別子が正しくありません".to_string()) };
            (id, parsed)
        })
        .collect())
}

/// そのリポジトリで、**「対応」の仕分けが 1 件でもあり**、成功して、指摘のあるレビューの ID（新しい順。最大 `limit` 件）。
/// 1 回の SQL で絞る。仕分けのないレビューは、読み込まない（レビュー全体の JSON は、重い）。
/// `repo_keys` は、リポジトリのパスの候補（空ならエラー）。
pub fn pending_review_ids(dir: &Path, repo_keys: &[String], limit: usize) -> Result<Vec<String>, String> {
    if repo_keys.is_empty() {
        return Err("リポジトリのパスがありません".to_string());
    }
    let conn = db::open(dir)?;
    let marks = repo_keys.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT r.id FROM reviews r WHERE r.repo IN ({marks}) AND r.ok = 1 AND r.findings_count > 0 \
         AND EXISTS (SELECT 1 FROM triage t WHERE t.review_id = r.id AND t.state = 'fix') \
         ORDER BY r.created_at DESC, r.rowid DESC LIMIT {}",
        limit as i64
    );
    let w = |e: rusqlite::Error| format!("対応が残るレビューを調べられません: {e}");
    let mut stmt = conn.prepare(&sql).map_err(w)?;
    let ids: Vec<String> = stmt
        .query_map(rusqlite::params_from_iter(repo_keys.iter()), |r| r.get::<_, String>(0))
        .map_err(w)?
        .collect::<Result<_, _>>()
        .map_err(w)?;
    Ok(ids)
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

    /// 時刻が違っても、同じ渡し分の中で、同じ内容の再提出は、変えない。新しい渡し分では、同じ文章でも、今回の申告になる。
    #[test]
    fn an_identical_resubmission_changes_nothing_within_a_round_but_counts_in_a_new_round() {
        let d = tmp("resubmit");
        make_review(&d, "r1", 1);
        release(&d, "r1", &[0], &[], "gui", 100).unwrap();
        assert_eq!(report(&d, "r1", 0, DID, "直した", "claude-code", 200).unwrap().changed, 1);
        // 時刻だけが違う、同じ内容 → 変えない（最初の時刻のまま）
        assert_eq!(report(&d, "r1", 0, DID, "直した", "claude-code", 250).unwrap().changed, 0);
        assert_eq!(agent_report::get_all(&d, "r1", 1)[0].clone().unwrap().reported_at, 200);
        // 文章・判定・保存元のどれかが違えば、保存する
        assert_eq!(report(&d, "r1", 0, DID, "直した（追記）", "claude-code", 260).unwrap().changed, 1);
        assert_eq!(report(&d, "r1", 0, SKIPPED, "直した（追記）", "claude-code", 270).unwrap().changed, 1);
        assert_eq!(report_via(&d, "r1", 0, SKIPPED, "直した（追記）", "claude-code", "cli", 280).unwrap().changed, 1, "保存元（mcp → cli）が違う");

        // 渡し直した（新しい渡し分）あと、同じ文章を報告したら、今回の申告になる
        release(&d, "r1", &[0], &[], "gui", 300).unwrap();
        assert!(get(&d, "r1").unwrap().unwrap().fix[0].reported.is_none(), "渡し直した直後は、前回の申告");
        assert_eq!(report_via(&d, "r1", 0, SKIPPED, "直した（追記）", "claude-code", "cli", 400).unwrap().changed, 1);
        let h = get(&d, "r1").unwrap().unwrap();
        assert_eq!(h.fix[0].reported.as_ref().map(|r| r.reported_at), Some(400));
        assert!(h.fix[0].previous_report.is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn report_via_records_which_entrance_the_report_came_from() {
        let d = tmp("via");
        make_review(&d, "r1", 2);
        release(&d, "r1", &[0, 1], &[], "gui", 1).unwrap();
        report_via(&d, "r1", 0, DID, "直した", "claude-code", "cli", 2).unwrap();
        report(&d, "r1", 1, DID, "直した", "claude-code", 3).unwrap();
        let saved = agent_report::get_all(&d, "r1", 2);
        assert_eq!(saved[0].clone().unwrap().source, "cli:claude-code");
        assert_eq!(saved[1].clone().unwrap().source, "mcp:claude-code", "report は、これまでどおり mcp");
        assert!(report_via(&d, "r1", 0, DID, "直した", "claude-code", "bad via", 4).unwrap_err().contains("入口"));
        assert!(report_via(&d, "r1", 0, DID, "直した", "claude-code", "", 4).is_err());
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

    // ── 渡す対象の決め方 ──

    fn states(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn rep(verdict: &str) -> Option<AgentReport> {
        Some(AgentReport { verdict: verdict.into(), text: "x".into(), reported_at: 1, source: "mcp:t".into() })
    }

    fn done_note() -> Note {
        Note { done_at: Some(5), ..Default::default() }
    }

    #[test]
    fn handoff_states_removes_only_fix_findings_that_are_done_or_reported_as_done() {
        //                          0      1      2      3        4      5
        let t = states(&["fix", "fix", "fix", "wait", "dismiss", "new"]);
        let reports = vec![rep("did"), rep("skipped"), None, rep("did"), rep("did"), rep("did")];
        let notes = vec![Note::default(), Note::default(), done_note(), done_note(), Note::default(), done_note()];
        let out = handoff_states(&t, &reports, &notes);
        assert_eq!(out[0], DONE, "エージェントが「対応した」と申告 → 外す");
        assert_eq!(out[1], "fix", "「直さなかった」の申告は、外さない（もう一度、渡す対象）");
        assert_eq!(out[2], DONE, "あなたが対応済みにした → 外す");
        assert_eq!(out[3], "wait", "「待ち」は、申告や済みの印があっても、そのまま");
        assert_eq!(out[4], "dismiss");
        assert_eq!(out[5], "new");
    }

    #[test]
    fn handoff_states_tolerates_shorter_report_and_note_lists() {
        let t = states(&["fix", "fix"]);
        assert_eq!(handoff_states(&t, &[], &[]), t, "申告も印も無ければ、そのまま");
        assert_eq!(handoff_states(&[], &[rep("did")], &[done_note()]), Vec::<String>::new());
        assert_eq!(handoff_states(&t, &[rep("did")], &[]), states(&[DONE, "fix"]), "足りない分は、「無い」として扱う");
    }

    #[test]
    fn targets_from_states_picks_fix_and_wait_in_order() {
        let t = states(&["wait", "fix", DONE, "fix", "dismiss", "new", "wait"]);
        assert_eq!(targets_from_states(&t), Targets { fix: vec![1, 3], hold: vec![0, 6] });
        assert_eq!(targets_from_states(&[]), Targets::default());
    }

    #[test]
    fn targets_reads_triage_reports_and_notes_from_the_database() {
        let d = tmp("targets");
        let rec = make_review(&d, "r1", 6);
        for (i, s) in [(0, triage::FIX), (1, triage::FIX), (2, triage::FIX), (3, triage::WAIT), (4, triage::DISMISS)] {
            triage::set_with(&d, "r1", i, s, 6, 1, None).unwrap();
        }
        // 1 番は、エージェントが「対応した」と申告。2 番は、あなたが対応済みにした
        agent_report::save(&d, "r1", &[(1, AgentReport { verdict: DID.into(), text: "直した".into(), reported_at: 10, source: "mcp:t".into() })]).unwrap();
        notes::set_done(&d, "r1", 2, 6, true, 11, None).unwrap();
        assert_eq!(targets(&d, &rec).unwrap(), Targets { fix: vec![0], hold: vec![3] });

        // 「直さなかった」と申告し直せば、1 番は、また渡す対象になる
        agent_report::save(&d, "r1", &[(1, AgentReport { verdict: SKIPPED.into(), text: "前提が違う".into(), reported_at: 20, source: "mcp:t".into() })]).unwrap();
        assert_eq!(targets(&d, &rec).unwrap().fix, vec![0, 1]);
        let _ = std::fs::remove_dir_all(&d);
    }

    // ── ensure_release ──

    #[test]
    fn ensure_release_creates_once_and_leaves_an_identical_request_alone() {
        let d = tmp("ensure-same");
        make_review(&d, "r1", 3);
        let (a, how) = ensure_release(&d, "r1", &[2, 0], &[1], "cli", 100).unwrap();
        assert_eq!(how, Ensured::Created);
        assert_eq!((a.released_at, a.by.as_str()), (100, "cli"));

        let (b, how) = ensure_release(&d, "r1", &[0, 2], &[1], "gui", 999).unwrap();
        assert_eq!(how, Ensured::Unchanged, "同じ内容なら、何も書かない");
        assert_eq!((b.released_at, b.by.as_str()), (100, "cli"), "released_at も by も、動かさない");
        assert_eq!(get(&d, "r1").unwrap().unwrap().release.released_at, 100);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **3b のレビューが見つけた不具合**: エージェントが `#1` を「対応した」と申告すると、次の `get` の対象が減る。
    /// そのとき記録を作り直すと、`#2` の申告が、「前回以前」に回ってしまっていた。
    #[test]
    fn targets_shrinking_after_a_did_report_does_not_demote_the_other_reports() {
        let d = tmp("ensure-shrink");
        make_review(&d, "r1", 2);
        ensure_release(&d, "r1", &[0, 1], &[], "cli", 100).unwrap();
        report(&d, "r1", 0, DID, "直した", "claude-code", 200).unwrap();
        report(&d, "r1", 1, SKIPPED, "前提が違う", "claude-code", 210).unwrap();

        // 2 回目の get。「対応した」の #1 は、対象から外れて、[1] だけになる
        let (r, how) = ensure_release(&d, "r1", &[1], &[], "cli", 300).unwrap();
        assert_eq!(how, Ensured::Updated, "番号だけを更新する");
        assert_eq!((r.released_at, r.fix.clone()), (100, vec![1]), "渡した時刻は、動かさない");
        let h = get(&d, "r1").unwrap().unwrap();
        assert_eq!(h.fix.len(), 1);
        assert_eq!(h.fix[0].index, 1);
        assert_eq!(h.fix[0].reported.as_ref().map(|r| (r.verdict.as_str(), r.reported_at)), Some(("skipped", 210)), "今回の申告のまま");
        assert!(h.fix[0].previous_report.is_none(), "「前回以前」に降格しない");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_hand_over_time_stays_when_targets_grow_or_the_hold_list_changes() {
        let d = tmp("ensure-grow");
        make_review(&d, "r1", 4);
        ensure_release(&d, "r1", &[0], &[1], "cli", 100).unwrap();
        report(&d, "r1", 0, DID, "直した", "claude-code", 150).unwrap();

        // あなたが、2 番も「対応」にした（対象が増える）
        let (a, how) = ensure_release(&d, "r1", &[0, 2], &[1], "cli", 200).unwrap();
        assert_eq!((how, a.released_at, a.fix.clone()), (Ensured::Updated, 100, vec![0, 2]));
        // 「直さない」の欄が変わった
        let (b, how) = ensure_release(&d, "r1", &[0, 2], &[3], "cli", 300).unwrap();
        assert_eq!((how, b.released_at, b.hold.clone(), b.by.as_str()), (Ensured::Updated, 100, vec![3], "cli"));

        let h = get(&d, "r1").unwrap().unwrap();
        assert_eq!(h.fix.iter().map(|f| f.index).collect::<Vec<_>>(), vec![0, 2], "取る内容は、最新の対象");
        assert_eq!(h.hold.iter().map(|f| f.index).collect::<Vec<_>>(), vec![3]);
        assert_eq!(h.fix[0].reported.as_ref().map(|r| r.reported_at), Some(150), "途中で書いた申告は、今回のまま");
        assert!(h.fix[1].reported.is_none() && h.fix[1].previous_report.is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_new_hand_over_starts_after_a_revoke_or_for_a_different_diff() {
        let d = tmp("ensure-new");
        make_review(&d, "r1", 2);
        ensure_release(&d, "r1", &[0], &[], "cli", 100).unwrap();
        report(&d, "r1", 0, SKIPPED, "前提が違う", "claude-code", 200).unwrap();

        // 取り消してから、もう一度渡す → 新しい渡し分。前の申告は、「前回以前」
        assert!(revoke(&d, "r1").unwrap());
        let (r, how) = ensure_release(&d, "r1", &[0], &[], "cli", 300).unwrap();
        assert_eq!((how, r.released_at), (Ensured::Created, 300));
        let h = get(&d, "r1").unwrap().unwrap();
        assert!(h.fix[0].reported.is_none());
        assert_eq!(h.fix[0].previous_report.as_ref().map(|r| r.reported_at), Some(200));

        // 別の差分に対して渡した記録（差分の識別子が違う）は、作り直す
        let mut raw = raw_release("r1", vec![0], vec![]);
        raw.diff_hash = "別の差分".into();
        insert_release(&d, &raw);
        let (e, how) = ensure_release(&d, "r1", &[0], &[], "cli", 400).unwrap();
        assert_eq!((how, e.released_at, e.diff_hash.as_str()), (Ensured::Created, 400, "d"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ensure_release_with_nothing_to_fix_fails_and_changes_nothing() {
        let d = tmp("ensure-empty");
        make_review(&d, "r1", 2);
        assert!(ensure_release(&d, "r1", &[], &[1], "cli", 1).unwrap_err().contains("0 件"));
        assert_eq!(get(&d, "r1").unwrap(), None);
        ensure_release(&d, "r1", &[0], &[], "cli", 2).unwrap();
        assert!(ensure_release(&d, "r1", &[], &[], "cli", 3).is_err(), "すでにある記録があっても、空では上書きしない");
        assert_eq!(get(&d, "r1").unwrap().unwrap().release.fix, vec![0]);
        assert!(ensure_release(&d, "../x", &[0], &[], "cli", 1).is_err(), "識別子の検証は、release と同じ");
        assert!(ensure_release(&d, "r1", &[9], &[], "cli", 1).unwrap_err().contains("範囲外"), "範囲外の番号は、更新でも拒む");
        assert_eq!(get(&d, "r1").unwrap().unwrap().release.fix, vec![0], "拒んだ呼び出しは、何も変えない");
        let _ = std::fs::remove_dir_all(&d);
    }

    fn meta_value(d: &Path, k: &str) -> Option<String> {
        db::open(d).unwrap().query_row("SELECT value FROM meta WHERE key = ?1", [k], |r| r.get(0)).ok()
    }

    /// 読めなかったことを、「無い」に見せない。壊れた記録を、黙って作り直して、原因を消さない。
    #[test]
    fn unreadable_reviews_and_records_are_errors_not_silently_replaced() {
        let d = tmp("unreadable");
        make_review(&d, "r1", 2);
        release(&d, "r1", &[0], &[], "gui", 1).unwrap();
        let before = meta_value(&d, &key("r1")).expect("記録がある");

        // レビューの記録が壊れている → 「見つかりません」ではなく、読めない、と言う
        db::open(&d).unwrap().execute("UPDATE reviews SET record = 'これは壊れたJSON' WHERE id = 'r1'", []).unwrap();
        assert!(release(&d, "r1", &[0], &[], "gui", 2).unwrap_err().contains("読めません"));
        assert!(ensure_release(&d, "r1", &[0], &[], "cli", 2).unwrap_err().contains("読めません"));
        assert_eq!(meta_value(&d, &key("r1")).unwrap(), before, "渡した記録は、変わらない");
        // 本当に無いレビューは、「見つかりません」
        assert!(release(&d, "nope", &[0], &[], "gui", 2).unwrap_err().contains("見つかりません"));
        assert!(ensure_release(&d, "nope", &[0], &[], "cli", 2).unwrap_err().contains("見つかりません"));

        // 渡した記録が壊れている → 上書きせず、エラー
        make_review(&d, "r2", 2);
        db::open(&d).unwrap().execute("INSERT INTO meta (key, value) VALUES (?1, 'これはJSONではない')", [key("r2")]).unwrap();
        assert!(ensure_release(&d, "r2", &[0], &[], "cli", 3).unwrap_err().contains("読めません"));
        assert_eq!(meta_value(&d, &key("r2")).unwrap(), "これはJSONではない", "壊れた記録を、上書きして、原因を消さない");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn report_with_a_broken_release_record_fails_and_saves_nothing() {
        let d = tmp("report-broken");
        make_review(&d, "r1", 2);
        db::open(&d).unwrap().execute("INSERT INTO meta (key, value) VALUES (?1, 'これはJSONではない')", [key("r1")]).unwrap();
        let e = report(&d, "r1", 0, DID, "直した", "claude-code", 9).unwrap_err();
        assert!(e.contains("読めません"), "「渡されていません」に見せない: {e}");
        assert_eq!(agent_report::get_all(&d, "r1", 2), vec![None, None], "何も保存しない");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 保存した申告そのものを返す（切り詰めたあとの文章を含む）。呼び出し側が、読み戻さなくてよい。
    #[test]
    fn report_returns_what_it_saved_including_the_truncated_text() {
        let d = tmp("saved");
        make_review(&d, "r1", 1);
        release(&d, "r1", &[0], &[], "gui", 100).unwrap();
        let long = "あ".repeat(TEXT_MAX + 5);
        let o = report_via(&d, "r1", 0, DID, &long, "claude-code", "cli", 200).unwrap();
        assert!(o.truncated);
        assert_eq!(o.saved.text.chars().count(), TEXT_MAX, "返ってくるのは、保存された（切り詰めた）文章");
        assert_eq!((o.saved.verdict.as_str(), o.saved.source.as_str(), o.saved.reported_at), ("did", "cli:claude-code", 200));
        assert_eq!(Some(o.saved.clone()), agent_report::get_all(&d, "r1", 1)[0], "DB に入っているものと、同じ");

        // 同じ内容の再提出（時刻だけ違う）は、保存せず、すでにあるものを返す
        let again = report_via(&d, "r1", 0, DID, &long, "claude-code", "cli", 999).unwrap();
        assert_eq!((again.changed, again.saved.reported_at), (0, 200));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn releases_reads_every_record_at_once_and_says_why_one_is_unreadable() {
        let d = tmp("releases");
        make_review(&d, "a", 1);
        make_review(&d, "b", 1);
        assert!(releases(&d).unwrap().is_empty());
        release(&d, "a", &[0], &[], "gui", 5).unwrap();
        release(&d, "b", &[0], &[], "cli", 6).unwrap();
        let conn = db::open(&d).unwrap();
        conn.execute("INSERT INTO meta (key, value) VALUES (?1, 'これはJSONではない')", [key("broken")]).unwrap();
        conn.execute("INSERT INTO meta (key, value) VALUES ('handoff_release:a b', '{}')", []).unwrap();
        conn.execute("INSERT INTO meta (key, value) VALUES ('agent_log_consent_v2', '1') ON CONFLICT (key) DO NOTHING", []).unwrap();
        let all = releases(&d).unwrap();
        assert_eq!(all.len(), 4, "無関係な meta キーは、含まない");
        assert_eq!(all["a"].as_ref().unwrap().by, "gui");
        assert_eq!(all["b"].as_ref().unwrap().released_at, 6);
        assert!(all["broken"].as_ref().unwrap_err().contains("読めません"));
        assert!(all["a b"].is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 「対応」の仕分けがあるレビューだけを、1 回の SQL で絞る。仕分けのないもの・失敗したもの・指摘のないもの・別のリポジトリは、含めない。
    #[test]
    fn pending_review_ids_picks_only_successful_reviews_with_a_fix_mark_in_the_repo() {
        let d = tmp("pending");
        let mk = |id: &str, at: u64, repo: &str, n: usize| {
            let mut r = sample_record(id, at);
            r.repo = repo.into();
            r.findings = (0..n).map(finding).collect();
            r
        };
        let mut failed = mk("failed", 5, "/r", 1);
        failed.error = Some("LLM の呼び出しに失敗".into());
        for r in [mk("old", 1, "/r", 1), mk("new", 4, "/r", 2), mk("nofix", 2, "/r", 1), mk("other", 3, "/other", 1), mk("empty", 6, "/r", 0), failed, mk("sub", 7, "/r/sub", 1)] {
            store::append(&d, &r).unwrap();
        }
        for (id, n) in [("old", 1), ("new", 2), ("other", 1), ("empty", 1), ("failed", 1), ("sub", 1)] {
            triage::set_with(&d, id, 0, triage::FIX, n, 1, None).unwrap();
        }
        triage::set_with(&d, "nofix", 0, triage::WAIT, 1, 1, None).unwrap();

        let ids = |keys: &[&str], limit: usize| pending_review_ids(&d, &keys.iter().map(|s| s.to_string()).collect::<Vec<_>>(), limit).unwrap();
        assert_eq!(ids(&["/r"], 10), vec!["new", "old"], "新しい順。仕分けなし・失敗・指摘なし・別のリポジトリは、除く");
        assert_eq!(ids(&["/r", "/r/sub"], 10), vec!["sub", "new", "old"], "パスの候補が 2 つでも、1 回で引ける");
        assert_eq!(ids(&["/r"], 1), vec!["new"], "上限");
        assert!(ids(&["/nothing"], 10).is_empty());
        assert!(pending_review_ids(&d, &[], 10).is_err(), "候補が空なら、エラー");
        let _ = std::fs::remove_dir_all(&d);
    }
}
