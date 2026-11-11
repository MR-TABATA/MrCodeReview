//! レビューの記録。1 回の実行 = reviews の 1 行。失敗した実行も、送った内容ごと残す。
//!
//! 記録の全文は `record` 列に JSON で持つ（形が増えても表を作り直さない）。
//! 検索・集計に使う項目（リポジトリ・観点・トークンなど）は、別の列に出してある。

use crate::carry::{Carry, Fate};
use crate::db;
use crate::findings::Finding;
use crate::provider::{ProviderInfo, Usage};
use crate::recipe::Recipe;
use crate::snapshot::{Excluded, Snapshot};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRecord {
    pub head: String,
    /// 比較元。古い記録には無い（そのときは、`HEAD` との比較）。
    #[serde(default)]
    pub base: crate::snapshot::Base,
    pub diff_hash: String,
    pub diff_bytes: usize,
    /// 送ったファイル（パスとバイト数。差分本文はプロンプトに全文があるので重ねて持たない）。
    pub files: Vec<FileRef>,
    pub excluded: Vec<Excluded>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRef {
    pub path: String,
    pub bytes: usize,
}

impl From<&Snapshot> for SnapshotRecord {
    fn from(s: &Snapshot) -> Self {
        SnapshotRecord {
            head: s.head.clone(),
            base: s.base.clone(),
            diff_hash: s.diff_hash.clone(),
            diff_bytes: s.diff_bytes,
            files: s.files.iter().map(|f| FileRef { path: f.path.clone(), bytes: f.bytes }).collect(),
            excluded: s.excluded.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub id: String,
    /// UNIX ミリ秒。
    pub created_at: u64,
    pub repo: String,
    pub recipe: Recipe,
    pub provider: ProviderInfo,
    pub snapshot: SnapshotRecord,
    /// 送ったプロンプトの全文。
    pub prompt: String,
    pub model: String,
    pub response_text: String,
    pub raw: String,
    pub usage: Usage,
    pub cost_usd: Option<f64>,
    pub duration_ms: u64,
    pub findings: Vec<Finding>,
    /// 指摘ごとの引き継ぎ元（`findings` と同じ並び）。無ければ None。古い記録には無いので空でよい。
    #[serde(default)]
    pub carries: Vec<Option<Carry>>,
    /// 前回の「対応」「待ち」の指摘が、今回どうなったか。
    #[serde(default)]
    pub fates: Vec<Fate>,
    /// 実行メモ（除外した件数など）。画面の指摘タブの上に出す。古い記録には無い。
    #[serde(default)]
    pub notes: Vec<String>,
    /// 送った関連コード（差分に出ていないコード）。関連コードを使う観点のときだけ。
    #[serde(default)]
    pub related: Option<crate::related::Related>,
    /// リポジトリ側の設定（`.mrcodereview/settings.json`）で上書きした項目。`settings` の値のうち、どれがそうか。
    #[serde(default)]
    pub repo_overrides: Vec<String>,
    /// そのレビューを実行したときに有効だった設定（型を持たない JSON。項目が増減しても、古い記録が読める）。
    #[serde(default)]
    pub settings: Option<serde_json::Value>,
    /// 応答から指摘を読めなかった理由。
    pub parse_error: Option<String>,
    /// 実行そのものが失敗した理由。
    pub error: Option<String>,
}

impl ReviewRecord {
    /// 実行も、指摘の読み取りも成功した記録か。
    pub fn is_ok(&self) -> bool {
        self.error.is_none() && self.parse_error.is_none()
    }
}

/// 履歴に出す、仕分けの件数。**4 つを足すと、指摘の数になる**（どれにも入らない指摘は、ない）。
/// - `open`（未）: まだ、終わっていないもの。未仕分けと、「対応」にしたが、まだ済んでいないもの
/// - `done`（済み）: 「対応」で、対応済みの印がある、またはエージェントが「対応した」と申告したもの
/// - `wait`（待ち）・`dismiss`（無視）
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct TriageCounts {
    pub open: usize,
    pub done: usize,
    pub wait: usize,
    pub dismiss: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewSummary {
    pub id: String,
    pub created_at: u64,
    pub repo: String,
    pub recipe_title: String,
    pub provider_label: String,
    pub findings: usize,
    pub verified: usize,
    pub total_tokens: Option<u64>,
    pub failed: bool,
    /// 仕分けの件数。読み取りに失敗したときは None（間違った数を出さない）。履歴の一覧を作るときだけ、入れる
    pub counts: Option<TriageCounts>,
}

fn total_tokens(input: Option<u64>, output: Option<u64>) -> Option<u64> {
    match (input, output) {
        (None, None) => None,
        (i, o) => Some(i.unwrap_or(0) + o.unwrap_or(0)),
    }
}

impl From<&ReviewRecord> for ReviewSummary {
    fn from(r: &ReviewRecord) -> Self {
        ReviewSummary {
            id: r.id.clone(),
            created_at: r.created_at,
            repo: r.repo.clone(),
            recipe_title: r.recipe.title.clone(),
            provider_label: r.provider.label.clone(),
            findings: r.findings.len(),
            verified: r.findings.iter().filter(|f| f.verified).count(),
            total_tokens: total_tokens(r.usage.input_tokens, r.usage.output_tokens),
            failed: !r.is_ok(),
            counts: None,
        }
    }
}

/// レビューごとの (待ち, 無視, 済み) の件数。指摘の範囲の外の番号は、数えない（`triage::get` と同じ）。
/// `repo` を渡すと、そのリポジトリのレビューだけを集計する（リポジトリごとに数えるとき、全部を集計し直さない）。
/// 「済み」＝「対応」の指摘のうち、対応済みの印がある、または、エージェントが「対応した」と申告したもの。
fn triage_counts(conn: &Connection, repo: Option<&str>) -> Result<std::collections::HashMap<String, (usize, usize, usize)>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT t.review_id, \
           SUM(CASE WHEN t.state = 'wait' THEN 1 ELSE 0 END), \
           SUM(CASE WHEN t.state = 'dismiss' THEN 1 ELSE 0 END), \
           SUM(CASE WHEN t.state = 'fix' AND ( \
                 EXISTS (SELECT 1 FROM finding_note n WHERE n.review_id = t.review_id AND n.idx = t.idx AND n.done_at IS NOT NULL) \
              OR EXISTS (SELECT 1 FROM agent_report a WHERE a.review_id = t.review_id AND a.idx = t.idx AND a.verdict = 'did') \
               ) THEN 1 ELSE 0 END) \
         FROM triage t JOIN reviews r ON r.id = t.review_id AND t.idx >= 0 AND t.idx < r.findings_count \
         WHERE (?1 IS NULL OR r.repo = ?1) \
         GROUP BY t.review_id",
    )?;
    let rows = stmt.query_map([repo], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)? as usize, r.get::<_, i64>(2)? as usize, r.get::<_, i64>(3)? as usize))))?;
    rows.collect()
}

fn opt_i64(v: Option<u64>) -> Option<i64> {
    v.map(|n| n as i64)
}

/// 1 行を書く。`or_ignore` は、取り込みで同じ ID が既にあっても失敗させないためのもの。
pub(crate) fn insert(conn: &Connection, rec: &ReviewRecord, or_ignore: bool) -> Result<(), String> {
    let json = serde_json::to_string(rec).map_err(|e| e.to_string())?;
    let sql = format!(
        "INSERT {} INTO reviews (id, created_at, repo, recipe_id, recipe_title, provider_id, provider_label, model, ok, \
         findings_count, verified_count, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost_usd, \
         duration_ms, record) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
        if or_ignore { "OR IGNORE" } else { "" }
    );
    conn.execute(
        &sql,
        params![
            rec.id,
            rec.created_at as i64,
            rec.repo,
            rec.recipe.id,
            rec.recipe.title,
            rec.provider.id,
            rec.provider.label,
            rec.model,
            rec.is_ok(),
            rec.findings.len() as i64,
            rec.findings.iter().filter(|f| f.verified).count() as i64,
            opt_i64(rec.usage.input_tokens),
            opt_i64(rec.usage.output_tokens),
            opt_i64(rec.usage.cache_read_tokens),
            opt_i64(rec.usage.cache_write_tokens),
            rec.cost_usd,
            rec.duration_ms as i64,
            json,
        ],
    )
    .map(|_| ())
    .map_err(|e| format!("履歴に書けない: {e}"))
}

pub fn append(dir: &Path, rec: &ReviewRecord) -> Result<(), String> {
    insert(&db::open(dir)?, rec, false)
}

fn records(conn: &Connection, sql: &str, args: &[&dyn rusqlite::ToSql]) -> Vec<ReviewRecord> {
    let Ok(mut stmt) = conn.prepare(sql) else { return Vec::new() };
    let Ok(rows) = stmt.query_map(args, |r| r.get::<_, String>(0)) else { return Vec::new() };
    // 読めない行（壊れた JSON）は飛ばす。履歴全体を読めなくしない。
    rows.flatten().filter_map(|j| serde_json::from_str(&j).ok()).collect()
}

/// 全件（新しい順）。**読めなくても、黙って空で返す**ので、成果物（レポート・集計）には使わない（`try_load_all` を使う）。
#[cfg_attr(not(test), allow(dead_code))]
pub fn load_all(dir: &Path) -> Vec<ReviewRecord> {
    let Ok(conn) = db::open(dir) else { return Vec::new() };
    records(&conn, "SELECT record FROM reviews ORDER BY created_at DESC, rowid DESC", &[])
}

/// 全件（新しい順）と、**読めなかった記録の件数**。成果物（レポート・集計）用。
/// データベースを開けない・読み取りに失敗したときは、空にせず、エラーにする。壊れた記録（JSON が読めない行）は、
/// 全体を止めず、飛ばして、その件数を返す（黙って抜かない。呼ぶ側が、出力に書く）。
pub fn try_load_all(dir: &Path) -> Result<(Vec<ReviewRecord>, usize), String> {
    let conn = db::open(dir)?;
    let mut stmt = conn.prepare("SELECT record FROM reviews ORDER BY created_at DESC, rowid DESC").map_err(|e| format!("履歴を読み取れません: {e}"))?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| format!("履歴を読み取れません: {e}"))?;
    let (mut out, mut unreadable) = (Vec::new(), 0usize);
    for row in rows {
        let json = row.map_err(|e| format!("履歴を読み取れません: {e}"))?;
        match serde_json::from_str::<ReviewRecord>(&json) {
            Ok(r) => out.push(r),
            Err(_) => unreadable += 1,
        }
    }
    Ok((out, unreadable))
}

/// トークン使用量の集計用。`since_ms` 以降だけを、**列だけ**から読む（記録の JSON 全体は、読まない）。
/// 履歴が増えても、重くならない。失敗した実行と、指摘が読めなかった実行は、列では区別できないので
/// （`ok` 列は、どちらも 0）、その少数の行だけ、記録の JSON を読んで見分ける。JSON が読めない行は、数に入れず、件数を返す。
pub fn usage_rows(dir: &Path, since_ms: Option<u64>) -> Result<(Vec<crate::usage::Slim>, usize), String> {
    use crate::provider::Usage;
    let conn = db::open(dir)?;
    let w = |e: rusqlite::Error| format!("履歴を読み取れません: {e}");
    let since = since_ms.map(|s| s.min(i64::MAX as u64) as i64);
    let mut stmt = conn
        .prepare(
            "SELECT id, ok, created_at, repo, recipe_title, provider_id, provider_label, model, \
             input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost_usd \
             FROM reviews WHERE (?1 IS NULL OR created_at >= ?1)",
        )
        .map_err(w)?;
    let rows = stmt
        .query_map([since], |r| {
            let tok = |i: usize| -> rusqlite::Result<Option<u64>> { Ok(r.get::<_, Option<i64>>(i)?.map(|n| n.max(0) as u64)) };
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, bool>(1)?,
                crate::usage::Slim {
                    created_at: r.get::<_, i64>(2)?.max(0) as u64,
                    repo: r.get(3)?,
                    recipe_title: r.get(4)?,
                    provider_id: r.get(5)?,
                    provider_label: r.get(6)?,
                    model: r.get(7)?,
                    failed: false,
                    usage: Usage { input_tokens: tok(8)?, output_tokens: tok(9)?, cache_read_tokens: tok(10)?, cache_write_tokens: tok(11)? },
                    cost_usd: r.get(12)?,
                },
            ))
        })
        .map_err(w)?;
    let (mut out, mut unreadable) = (Vec::new(), 0usize);
    for row in rows {
        let (id, ok, mut slim) = row.map_err(w)?;
        if !ok {
            let json: String = conn.query_row("SELECT record FROM reviews WHERE id = ?1", [&id], |r| r.get(0)).map_err(w)?;
            match serde_json::from_str::<ReviewRecord>(&json) {
                Ok(rec) => slim.failed = rec.error.is_some(),
                Err(_) => {
                    unreadable += 1;
                    continue;
                }
            }
        }
        out.push(slim);
    }
    Ok((out, unreadable))
}

pub fn get(dir: &Path, id: &str) -> Option<ReviewRecord> {
    let conn = db::open(dir).ok()?;
    records(&conn, "SELECT record FROM reviews WHERE id = ?1", &[&id]).into_iter().next()
}

/// 1 件のレビュー。`get` と違い、**読めなかったことと、無いことを、区別する**。
/// 無ければ `Ok(None)`。データベースを開けない・読み取りに失敗・記録が壊れている（JSON が読めない）ときは、`Err`（原因つき）。
/// 原因を失わせたくないとき（渡す・報告する、など）は、こちらを使う。
pub fn try_get(dir: &Path, id: &str) -> Result<Option<ReviewRecord>, String> {
    let conn = db::open(dir)?;
    match conn.query_row("SELECT record FROM reviews WHERE id = ?1", [id], |r| r.get::<_, String>(0)) {
        Ok(json) => serde_json::from_str::<ReviewRecord>(&json).map(Some).map_err(|e| format!("レビューの記録を読めません（{id}）: {e}")),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("レビューを読み取れません（{id}）: {e}")),
    }
}

/// 同じリポジトリ・同じ観点の、成功したレビュー。新しい順に `limit` 件。引き継ぎの候補。
pub fn recent_ok(dir: &Path, repo: &str, recipe_id: &str, limit: usize) -> Vec<ReviewRecord> {
    let Ok(conn) = db::open(dir) else { return Vec::new() };
    records(
        &conn,
        "SELECT record FROM reviews WHERE repo = ?1 AND recipe_id = ?2 AND ok = 1 \
         ORDER BY created_at DESC, rowid DESC LIMIT ?3",
        &[&repo, &recipe_id, &(limit as i64)],
    )
}

/// 履歴の一覧。記録の全文は読まず、列だけで作る。新しい順。
/// 所要時間（ミリ秒）。同じ LLM・同じ観点で、新しい順に `limit` 件まで。
/// 数えるのは、実行が最後まで行った（実行中のエラーが無い）もの。答えが返ったが、中身の JSON が読めなかったものも、時間は実際にかかっているので数える。
pub fn recent_durations(dir: &Path, provider_id: &str, recipe_id: &str, limit: usize) -> Vec<u64> {
    let Ok(conn) = db::open(dir) else { return Vec::new() };
    let Ok(mut stmt) = conn.prepare(
        "SELECT duration_ms FROM reviews WHERE provider_id = ?1 AND recipe_id = ?2 AND json_extract(record, '$.error') IS NULL \
         ORDER BY created_at DESC, rowid DESC LIMIT ?3",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map(params![provider_id, recipe_id, limit as i64], |r| r.get::<_, i64>(0)) else {
        return Vec::new();
    };
    rows.flatten().map(|n| n.max(0) as u64).collect()
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn summaries(dir: &Path, limit: usize) -> Vec<ReviewSummary> {
    summaries_in(dir, None, limit)
}

/// `repo` を渡すと、そのリポジトリ（記録に残っている根のパスと一致するもの）の履歴だけ。`limit` は、絞ったあとの件数。
pub fn summaries_in(dir: &Path, repo: Option<&str>, limit: usize) -> Vec<ReviewSummary> {
    let Ok(conn) = db::open(dir) else { return Vec::new() };
    let Ok(mut stmt) = conn.prepare(
        "SELECT id, created_at, repo, recipe_title, provider_label, findings_count, verified_count, \
         input_tokens, output_tokens, ok FROM reviews WHERE (?2 IS NULL OR repo = ?2) \
         ORDER BY created_at DESC, rowid DESC LIMIT ?1",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map(rusqlite::params![limit as i64, repo], |r| {
        let input: Option<i64> = r.get(7)?;
        let output: Option<i64> = r.get(8)?;
        Ok(ReviewSummary {
            id: r.get(0)?,
            created_at: r.get::<_, i64>(1)? as u64,
            repo: r.get(2)?,
            recipe_title: r.get(3)?,
            provider_label: r.get(4)?,
            findings: r.get::<_, i64>(5)?.max(0) as usize, // 壊れた記録の負の値は、0 に丸める（そのまま変換すると、巨大な数になる）
            verified: r.get::<_, i64>(6)?.max(0) as usize,
            total_tokens: total_tokens(input.map(|n| n as u64), output.map(|n| n as u64)),
            failed: !r.get::<_, bool>(9)?,
            counts: None,
        })
    }) else {
        return Vec::new();
    };
    let mut out: Vec<ReviewSummary> = rows.flatten().collect();
    // 仕分けの件数。読み取りに失敗したら、None のまま（0 件に見せない）
    if let Ok(by_review) = triage_counts(&conn, repo) {
        for s in &mut out {
            let (wait, dismiss, done) = by_review.get(&s.id).copied().unwrap_or((0, 0, 0));
            s.counts = Some(TriageCounts { open: s.findings.saturating_sub(wait + dismiss + done), done, wait, dismiss });
        }
    }
    out
}

/// このリポジトリ（記録に残っている根のパス）の、「未」の指摘の合計。履歴の各行の「未」（`TriageCounts::open`）を足したもの
/// （未仕分け ＋ 対応中。待ち・無視・済みは、入れない）。仕分けの読み取りに失敗したら、エラー（0 件に見せない）。
pub fn open_count(conn: &Connection, repo: &str) -> Result<usize, String> {
    Ok(open_counts(conn, &[repo.to_string()])?.get(repo).copied().unwrap_or(0))
}

/// 複数のリポジトリの「未」の合計を、まとめて数える（リポジトリごとに問い合わせを出さない）。返す表は、渡したすべての根のパスを持つ
/// （履歴が無ければ 0）。1 つだけのときは、そのリポジトリの仕分けだけを集計し、複数のときは、全体を 1 回で集計する。
/// 仕分けの読み取りに失敗したら、エラー（0 件に見せない）。
pub fn open_counts(conn: &Connection, repos: &[String]) -> Result<std::collections::HashMap<String, usize>, String> {
    let mut out: std::collections::HashMap<String, usize> = repos.iter().map(|r| (r.clone(), 0)).collect();
    if repos.is_empty() {
        return Ok(out);
    }
    let only = (repos.len() == 1).then(|| repos[0].as_str());
    let by_review = triage_counts(conn, only).map_err(|e| format!("仕分けを数えられません: {e}"))?;
    let marks = vec!["?"; repos.len()].join(",");
    let mut stmt = conn
        .prepare(&format!("SELECT id, repo, findings_count FROM reviews WHERE repo IN ({marks})"))
        .map_err(|e| format!("履歴を読めません: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(repos.iter()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?.max(0) as usize)))
        .map_err(|e| format!("履歴を読めません: {e}"))?;
    for row in rows {
        let (id, repo, findings) = row.map_err(|e| format!("履歴を読めません: {e}"))?;
        let (wait, dismiss, done) = by_review.get(&id).copied().unwrap_or((0, 0, 0));
        *out.entry(repo).or_insert(0) += findings.saturating_sub(wait + dismiss + done);
    }
    Ok(out)
}

/// 履歴のレビューを、選んだ分だけ消す。**元に戻せない。** そのレビューに紐づく、仕分け・仕分けの履歴・エージェントの申告・対応メモ・
/// 「CLI に渡した」記録も、同じトランザクションで消す（途中で失敗したら、何も消えない）。消した件数を返す（無い ID は、数えない）。
pub fn delete_reviews(dir: &Path, ids: &[String]) -> Result<usize, String> {
    let conn = db::open(dir)?;
    let tx = conn.unchecked_transaction().map_err(|e| format!("削除を始められません: {e}"))?;
    let mut removed = 0;
    for id in ids {
        for table in ["triage", "triage_log", "agent_report", "finding_note"] {
            tx.execute(&format!("DELETE FROM {table} WHERE review_id = ?1"), params![id]).map_err(|e| format!("{table} を消せません: {e}"))?;
        }
        tx.execute("DELETE FROM meta WHERE key = ?1", params![crate::handoff_api::key(id)]).map_err(|e| format!("渡した記録を消せません: {e}"))?;
        removed += tx.execute("DELETE FROM reviews WHERE id = ?1", params![id]).map_err(|e| format!("履歴を消せません: {e}"))?;
    }
    tx.commit().map_err(|e| format!("削除を確定できません: {e}"))?;
    Ok(removed)
}

/// 試験用の、最小の記録。
pub fn sample_record(id: &str, at: u64) -> ReviewRecord {
    ReviewRecord {
        id: id.into(),
        created_at: at,
        repo: "/r".into(),
        recipe: Recipe { id: "security".into(), title: "セキュリティ".into(), source: "builtin".into(), prompt: "p".into(), kind: crate::recipe::Kind::Llm, needs_related: false },
        provider: ProviderInfo { id: "mock".into(), label: "m".into(), note: "n".into() },
        snapshot: SnapshotRecord { head: "h".into(), base: Default::default(), diff_hash: "d".into(), diff_bytes: 1, files: vec![], excluded: vec![] },
        prompt: "p".into(),
        model: "mock".into(),
        response_text: "".into(),
        raw: "".into(),
        usage: Usage { input_tokens: Some(5), output_tokens: Some(7), cache_read_tokens: None, cache_write_tokens: None },
        cost_usd: None,
        duration_ms: 1,
        findings: vec![],
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: &str, at: u64) -> ReviewRecord {
        sample_record(id, at)
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mrcr-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn recent_durations_count_runs_that_got_an_answer_for_the_same_llm_and_recipe_only() {
        let dir = scratch("durations");
        let mk = |id: &str, at: u64, provider: &str, recipe: &str, ms: u64| {
            let mut r = rec(id, at);
            r.provider.id = provider.into();
            r.recipe.id = recipe.into();
            r.duration_ms = ms;
            r
        };
        let mut parse_failed = mk("p", 40, "agy-cli", "bugs", 45_000);
        parse_failed.parse_error = Some("応答に JSON が見つからない".into()); // 答えは返った＝時間は実際にかかった
        let mut failed = mk("f", 50, "agy-cli", "bugs", 999);
        failed.error = Some("タイムアウト".into()); // 実行が最後まで行っていない
        for r in [
            mk("a", 10, "agy-cli", "bugs", 25_000),
            mk("b", 20, "agy-cli", "bugs", 31_000),
            mk("o1", 25, "agy-cli", "security", 1), // 別の観点
            mk("o2", 26, "claude-cli", "bugs", 2),   // 別の LLM
            parse_failed,
            failed,
        ] {
            append(&dir, &r).unwrap();
        }
        assert_eq!(recent_durations(&dir, "agy-cli", "bugs", 10), [45_000, 31_000, 25_000], "新しい順。実行中のエラーは除く");
        assert_eq!(recent_durations(&dir, "agy-cli", "bugs", 2), [45_000, 31_000], "件数の上限");
        assert!(recent_durations(&dir, "agy-cli", "tests", 10).is_empty());
        assert!(recent_durations(&scratch("durations-empty"), "agy-cli", "bugs", 10).is_empty(), "履歴が無ければ、空");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn usage_rows_gives_the_same_totals_as_summing_the_full_records_and_filters_by_period_in_sql() {
        let d = scratch("usage-rows");
        let mut all = Vec::new();
        for (i, kind) in ["ok", "ok-no-tokens", "failed", "parse-error", "ok", "tool"].iter().enumerate() {
            let mut r = rec(&format!("u{i}"), 100 + i as u64 * 10);
            r.recipe.title = if i % 2 == 0 { "潜在バグ".into() } else { "セキュリティ".into() };
            r.repo = if i < 3 { "/a".into() } else { "/b".into() };
            r.model = if *kind == "tool" { String::new() } else { format!("m{}", i % 2) };
            if *kind == "tool" {
                r.provider.id = "tool".into();
            }
            r.usage = crate::provider::Usage { input_tokens: Some(10 + i as u64), output_tokens: Some(5), cache_read_tokens: Some(1), cache_write_tokens: None };
            r.cost_usd = Some(0.25);
            match *kind {
                "ok-no-tokens" => r.usage = Default::default(),
                "failed" => r.error = Some("タイムアウト".into()),
                // 指摘だけが読めなかった実行: トークンは使っているので、集計では、失敗に数えない
                "parse-error" => r.parse_error = Some("JSON ではない".into()),
                _ => {}
            }
            append(&d, &r).unwrap();
            all.push(r);
        }
        let json = |s: &crate::usage::Summary| serde_json::to_value(s).unwrap();
        for since in [None, Some(0), Some(125), Some(1_000)] {
            let (rows, unreadable) = usage_rows(&d, since).unwrap();
            assert_eq!(unreadable, 0);
            let fast = crate::usage::summarize_slim(&rows, since);
            let full = crate::usage::summarize(&all, since);
            assert_eq!(json(&fast), json(&full), "since={since:?}");
        }
        // 期間の絞りは、SQL で行う（絞った外の記録は、そもそも、行として来ない）
        assert_eq!(usage_rows(&d, Some(125)).unwrap().0.len(), 3);
        assert_eq!(usage_rows(&d, Some(1_000)).unwrap().0.len(), 0);
        // 指摘の読み取りに失敗した実行は、失敗ではない（トークンを足す）／実行に失敗した実行は、失敗
        let (rows, _) = usage_rows(&d, None).unwrap();
        assert_eq!(rows.iter().filter(|r| r.failed).count(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn usage_rows_does_not_need_the_json_for_successful_runs_and_counts_unreadable_failures() {
        let d = scratch("usage-rows-broken");
        append(&d, &rec("good", 10)).unwrap();
        let mut bad = rec("bad", 20);
        bad.error = Some("失敗".into());
        append(&d, &bad).unwrap();
        // 成功した実行の JSON が壊れても、列から集計できる／失敗した実行の JSON が壊れたら、見分けられないので、件数に出す
        let c = db::open(&d).unwrap();
        c.execute("UPDATE reviews SET record = '{broken'", []).unwrap();
        drop(c);
        let (rows, unreadable) = usage_rows(&d, None).unwrap();
        assert_eq!((rows.len(), unreadable), (1, 1));
        assert_eq!(rows[0].created_at, 10);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn history_rows_carry_triage_counts_that_add_up_to_the_findings() {
        let d = scratch("triage-counts");
        let f = |t: &str| Finding { file: "a.rs".into(), line: Some(1), severity: "high".into(), title: t.into(), detail: "d".into(), evidence: "e".into(), verified: true, rule: None };
        let mut r = rec("r1", 10);
        r.findings = (0..6).map(|i| f(&format!("f{i}"))).collect();
        append(&d, &r).unwrap();
        append(&d, &rec("r2", 20)).unwrap(); // 指摘 0 件
        let counts = |id: &str| summaries_in(&d, None, 10).into_iter().find(|s| s.id == id).unwrap().counts;

        // 何も仕分けていない: 全部、未
        assert_eq!(counts("r1"), Some(TriageCounts { open: 6, done: 0, wait: 0, dismiss: 0 }));
        assert_eq!(counts("r2"), Some(TriageCounts { open: 0, done: 0, wait: 0, dismiss: 0 }), "指摘が 0 件なら、全部 0");

        // 0: 対応（まだ済んでいない）→ 未／1: 対応＋対応済みの印 → 済み／2: 対応＋エージェントが「対応した」→ 済み
        // 3: 待ち／4: 無視／5: 仕分けなし → 未
        for (i, st) in [(0, "fix"), (1, "fix"), (2, "fix"), (3, "wait"), (4, "dismiss")] {
            crate::triage::set_with(&d, "r1", i, st, 6, 100, None).unwrap();
        }
        crate::notes::set_done(&d, "r1", 1, 6, true, 200, None).unwrap();
        crate::agent_report::save(&d, "r1", &[(2, crate::agent_report::AgentReport { verdict: "did".into(), text: "x".into(), reported_at: 1, source: "s".into() })]).unwrap();
        let c = counts("r1").unwrap();
        assert_eq!(c, TriageCounts { open: 2, done: 2, wait: 1, dismiss: 1 });
        assert_eq!(c.open + c.done + c.wait + c.dismiss, 6, "4 つを足すと、指摘の数になる");

        // 「対応しなかった」の申告は、済みにしない。仕分けを「待ち」に変えたら、済みの印があっても、済みに数えない
        crate::agent_report::save(&d, "r1", &[(0, crate::agent_report::AgentReport { verdict: "skipped".into(), text: "x".into(), reported_at: 1, source: "s".into() })]).unwrap();
        crate::triage::set_with(&d, "r1", 1, "wait", 6, 300, None).unwrap();
        assert_eq!(counts("r1"), Some(TriageCounts { open: 2, done: 1, wait: 2, dismiss: 1 }));

        // 指摘の範囲の外の番号（古い仕分けの残り）は、数えない
        db::open(&d).unwrap().execute("INSERT INTO triage (review_id, idx, state) VALUES ('r1', 99, 'dismiss')", []).unwrap();
        assert_eq!(counts("r1").unwrap().dismiss, 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn history_counts_are_none_not_zero_when_triage_cannot_be_read() {
        let d = scratch("triage-counts-broken");
        let mut r = rec("r1", 10);
        r.findings = vec![Finding { file: "a.rs".into(), line: None, severity: "low".into(), title: "t".into(), detail: "d".into(), evidence: "e".into(), verified: true, rule: None }];
        append(&d, &r).unwrap();
        let c = db::open(&d).unwrap();
        c.execute_batch("DROP TABLE triage; CREATE TABLE triage (x INTEGER);").unwrap();
        drop(c);
        let s = summaries_in(&d, None, 10);
        assert_eq!(s.len(), 1, "履歴の行は、出す");
        assert_eq!(s[0].counts, None, "読めなければ、0 件に見せず、None");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn try_load_all_errors_on_an_unreadable_database_and_counts_unparsable_records() {
        let bad = scratch("try-bad");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("mrcodereview.sqlite3"), b"not a database").unwrap();
        assert!(try_load_all(&bad).is_err(), "開けないときは、空にせず、エラー");
        assert!(load_all(&bad).is_empty(), "従来の load_all は、空で返す（成果物には使わない）");

        let d = scratch("try-ok");
        append(&d, &rec("a", 1)).unwrap();
        append(&d, &rec("b", 2)).unwrap();
        let (all, unreadable) = try_load_all(&d).unwrap();
        assert_eq!((all.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), unreadable), (vec!["b", "a"], 0));
        db::open(&d)
            .unwrap()
            .execute(
                "INSERT INTO reviews (id, created_at, repo, recipe_id, recipe_title, provider_id, provider_label, model, ok, findings_count, verified_count, duration_ms, record) \
                 VALUES ('BAD', 3, '/r', 'x', 'x', 'p', 'p', 'm', 1, 0, 0, 1, 'broken')",
                [],
            )
            .unwrap();
        let (all, unreadable) = try_load_all(&d).unwrap();
        assert_eq!((all.len(), unreadable), (2, 1), "読めた記録は返し、読めなかった件数を返す");
        let _ = std::fs::remove_dir_all(&bad);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_record_saved_before_bases_existed_still_loads_as_compared_with_head() {
        let old = serde_json::json!({ "head": "abc", "diff_hash": "d", "diff_bytes": 1, "files": [], "excluded": [] });
        let s: SnapshotRecord = serde_json::from_value(old).unwrap();
        assert_eq!(s.base, crate::snapshot::Base::default(), "古い記録には、比較元が無い");
        assert!(s.base.hash.is_empty() && s.base.spec.is_empty());
    }

    #[test]
    fn try_get_tells_a_missing_review_from_an_unreadable_one() {
        let dir = scratch("try-get");
        append(&dir, &rec("ok", 1)).unwrap();
        assert_eq!(try_get(&dir, "ok").unwrap().map(|r| r.id), Some("ok".to_string()));
        assert_eq!(try_get(&dir, "nope").unwrap().map(|r| r.id), None, "無いのは、エラーではない");

        db::open(&dir).unwrap().execute("UPDATE reviews SET record = 'これは壊れたJSON' WHERE id = 'ok'", []).unwrap();
        let e = try_get(&dir, "ok").unwrap_err();
        assert!(e.contains("読めません") && e.contains("ok"), "壊れていれば、エラー（無いことにしない）: {e}");
        assert_eq!(get(&dir, "ok").map(|r| r.id), None, "従来の get は、同じ状態を『無い』と見せる（だから、try_get がある）");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn summaries_can_be_limited_to_one_repo() {
        let dir = scratch("by-repo");
        let mut a = rec("a", 1);
        a.repo = "/r/one".into();
        let mut b = rec("b", 2);
        b.repo = "/r/two".into();
        let mut c = rec("c", 3);
        c.repo = "/r/one".into();
        for r in [&a, &b, &c] {
            append(&dir, r).unwrap();
        }
        let ids = |repo: Option<&str>, n: usize| summaries_in(&dir, repo, n).into_iter().map(|s| s.id).collect::<Vec<_>>();
        assert_eq!(ids(Some("/r/one"), 10), ["c", "a"], "そのリポジトリだけ、新しい順");
        assert_eq!(ids(Some("/r/two"), 10), ["b"]);
        assert!(ids(Some("/r/none"), 10).is_empty());
        assert_eq!(ids(None, 10), ["c", "b", "a"], "指定しなければ、全部");
        assert_eq!(ids(Some("/r/one"), 1), ["c"], "件数の上限は、絞ったあとに効く");
        assert!(ids(Some("/r/one/"), 10).is_empty(), "パスは、完全に一致したものだけ");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_then_load_newest_first() {
        let dir = scratch("order");
        append(&dir, &rec("a", 1)).unwrap();
        append(&dir, &rec("b", 2)).unwrap();
        append(&dir, &rec("c", 2)).unwrap(); // 同じ時刻なら、後に入れたほうが新しい
        let ids: Vec<String> = load_all(&dir).into_iter().map(|r| r.id).collect();
        assert_eq!(ids, vec!["c", "b", "a"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_id_is_an_error_not_a_silent_overwrite() {
        let dir = scratch("dup");
        append(&dir, &rec("a", 1)).unwrap();
        assert!(append(&dir, &rec("a", 2)).is_err());
        assert_eq!(load_all(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_and_recent_ok_filter_by_repo_recipe_and_success() {
        let dir = scratch("recent");
        let mut other_repo = rec("o", 5);
        other_repo.repo = "/other".into();
        let mut other_recipe = rec("p", 6);
        other_recipe.recipe.id = "perf".into();
        let mut failed = rec("f", 7);
        failed.error = Some("boom".into());
        let mut unparsed = rec("u", 8);
        unparsed.parse_error = Some("bad json".into());
        for r in [rec("a", 1), rec("b", 2), rec("c", 3), other_repo, other_recipe, failed, unparsed] {
            append(&dir, &r).unwrap();
        }
        let ids: Vec<String> = recent_ok(&dir, "/r", "security", 2).into_iter().map(|r| r.id).collect();
        assert_eq!(ids, vec!["c", "b"], "新しい順に limit 件。別リポジトリ・別観点・失敗は含めない");
        assert_eq!(get(&dir, "f").unwrap().error.as_deref(), Some("boom"));
        assert!(get(&dir, "nope").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn summaries_come_from_columns_and_match_the_record_view() {
        let dir = scratch("summ");
        let mut r = rec("a", 1);
        r.findings = vec![crate::findings::Finding {
            file: "a".into(), line: None, severity: "low".into(), title: "t".into(), detail: "".into(), evidence: "e".into(), verified: true, rule: None,
        }];
        let mut failed = rec("f", 2);
        failed.usage = Usage::default();
        failed.error = Some("x".into());
        append(&dir, &r).unwrap();
        append(&dir, &failed).unwrap();
        let s = summaries(&dir, 10);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].id, "f");
        assert!(s[0].failed && s[0].total_tokens.is_none());
        assert_eq!((s[1].findings, s[1].verified, s[1].total_tokens, s[1].failed), (1, 1, Some(12), false));
        let via_record = ReviewSummary::from(&r);
        assert_eq!((via_record.findings, via_record.verified, via_record.total_tokens), (1, 1, Some(12)));
        assert_eq!(summaries(&dir, 1).len(), 1, "limit が効く");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_unwritable_dir_loads_empty() {
        assert!(load_all(Path::new("/nonexistent-mrcr-dir/x")).is_empty());
        assert!(get(Path::new("/nonexistent-mrcr-dir/x"), "a").is_none());
    }

    #[test]
    fn broken_record_json_is_skipped_not_fatal() {
        let dir = scratch("broken");
        append(&dir, &rec("a", 1)).unwrap();
        let conn = db::open(&dir).unwrap();
        conn.execute("UPDATE reviews SET record = '{broken' WHERE id = 'a'", []).unwrap();
        append(&dir, &rec("b", 2)).unwrap();
        let ids: Vec<String> = load_all(&dir).into_iter().map(|r| r.id).collect();
        assert_eq!(ids, vec!["b"]);
        assert_eq!(summaries(&dir, 10).len(), 2, "一覧は列から作るので、全文が壊れていても出る");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn summary_total_tokens_none_when_nothing_reported() {
        let mut r = rec("a", 1);
        assert_eq!(ReviewSummary::from(&r).total_tokens, Some(12));
        r.usage = Usage::default();
        assert_eq!(ReviewSummary::from(&r).total_tokens, None);
    }

    #[test]
    fn old_records_without_carry_fields_still_deserialize() {
        let mut v = serde_json::to_value(rec("old", 1)).unwrap();
        let obj = v.as_object_mut().unwrap();
        obj.remove("carries");
        obj.remove("fates");
        let r: ReviewRecord = serde_json::from_value(v).unwrap();
        assert!(r.carries.is_empty() && r.fates.is_empty());
    }

    #[test]
    fn legacy_jsonl_and_triage_json_are_imported_once_and_kept() {
        let dir = scratch("legacy");
        std::fs::create_dir_all(&dir).unwrap();
        let line = |r: &ReviewRecord| serde_json::to_string(r).unwrap();
        std::fs::write(
            dir.join("reviews.jsonl"),
            format!("{}\n{{broken\n{}\n", line(&rec("a", 1)), line(&rec("b", 2))),
        )
        .unwrap();
        std::fs::write(dir.join("triage.json"), r#"{"a":["fix","???","wait"],"zzz":["dismiss"]}"#).unwrap();

        let ids: Vec<String> = load_all(&dir).into_iter().map(|r| r.id).collect();
        assert_eq!(ids, vec!["b", "a"], "壊れた行は飛ばして取り込む");
        assert_eq!(crate::triage::get(&dir, "a", 3), vec!["fix", "new", "wait"], "未知の状態は未仕分けにする");

        // 取り込みは一度だけ。元のファイルを書き換えても、取り込み済みの内容は増減しない
        std::fs::write(dir.join("reviews.jsonl"), format!("{}\n", line(&rec("c", 3)))).unwrap();
        assert_eq!(load_all(&dir).len(), 2);
        assert!(dir.join("reviews.jsonl").exists() && dir.join("triage.json").exists(), "元のファイルは消さない");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deleting_reviews_removes_their_related_rows_only_and_the_open_count_follows() {
        let d = scratch("delete-reviews");
        let f = |t: &str| Finding { file: "a.rs".into(), line: Some(1), severity: "high".into(), title: t.into(), detail: "d".into(), evidence: "e".into(), verified: true, rule: None };
        for id in ["a", "b", "c"] {
            let mut r = rec(id, 10);
            r.findings = (0..3).map(|i| f(&format!("f{i}"))).collect();
            append(&d, &r).unwrap();
            crate::triage::set_with(&d, id, 0, "dismiss", 3, 100, None).unwrap();
            crate::notes::set_memo(&d, id, 1, 3, "メモ", 100).unwrap();
        }
        let conn = db::open(&d).unwrap();
        conn.execute("INSERT INTO meta (key, value) VALUES (?1, 'x')", [crate::handoff_api::key("a")]).unwrap();
        conn.execute("INSERT INTO meta (key, value) VALUES (?1, 'x')", [crate::handoff_api::key("c")]).unwrap();
        // 3 件 × 2（未仕分け 2 ＋ 無視 1）＝ 未は 6
        assert_eq!(open_count(&conn, "/r").unwrap(), 6);
        assert_eq!(open_count(&conn, "/other").unwrap(), 0);
        drop(conn);

        // 無い ID は、数えず、失敗もしない
        assert_eq!(delete_reviews(&d, &["a".into(), "b".into(), "nope".into()]).unwrap(), 2);

        let conn = db::open(&d).unwrap();
        let n = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(n("SELECT COUNT(*) FROM reviews"), 1);
        for t in ["triage", "triage_log", "finding_note"] {
            assert_eq!(n(&format!("SELECT COUNT(*) FROM {t} WHERE review_id <> 'c'")), 0, "{t}: 消したレビューの行が残っている");
            assert!(n(&format!("SELECT COUNT(*) FROM {t} WHERE review_id = 'c'")) > 0, "{t}: 残すレビューの行まで消えた");
        }
        assert_eq!(n("SELECT COUNT(*) FROM meta WHERE key LIKE 'handoff_release:%'"), 1, "渡した記録は、残すレビューの分だけ");
        assert_eq!(open_count(&conn, "/r").unwrap(), 2);
        drop(conn);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_negative_findings_count_in_a_broken_record_is_read_as_zero() {
        let d = scratch("negative-count");
        // 壊れた記録 3 種（指摘だけ負・検証済みだけ負・両方負）と、ふつうの記録 1 件を、同じリポジトリに入れる
        for (id, at) in [("neg-f", 10), ("neg-v", 20), ("both", 30), ("ok", 40)] {
            append(&d, &rec(id, at)).unwrap();
        }
        let conn = db::open(&d).unwrap();
        for (id, findings, verified) in [("neg-f", -1, 2), ("neg-v", 4, -3), ("both", -1, -3), ("ok", 3, 3)] {
            conn.execute("UPDATE reviews SET findings_count = ?2, verified_count = ?3 WHERE id = ?1", params![id, findings, verified]).unwrap();
        }
        // そのまま usize にすると、約 1800 京になる。壊れた記録は 0 に丸め、ほかの記録の分（4 + 3）は、そのまま足す
        assert_eq!(open_count(&conn, "/r").unwrap(), 4 + 3);
        drop(conn);
        let s = summaries_in(&d, None, 10);
        let by = |id: &str| s.iter().find(|x| x.id == id).map(|x| (x.findings, x.verified, x.counts.map(|c| c.open))).unwrap();
        assert_eq!(by("neg-f"), (0, 2, Some(0)), "指摘だけ負");
        assert_eq!(by("neg-v"), (4, 0, Some(4)), "検証済みだけ負");
        assert_eq!(by("both"), (0, 0, Some(0)), "両方負");
        assert_eq!(by("ok"), (3, 3, Some(3)), "ふつうの記録は、そのまま");
        let _ = std::fs::remove_dir_all(&d);
    }

    fn review_with_findings(d: &Path, id: &str, at: u64, n: usize) {
        let f = |t: &str| Finding { file: "a.rs".into(), line: Some(1), severity: "high".into(), title: t.into(), detail: "d".into(), evidence: "e".into(), verified: true, rule: None };
        let mut r = rec(id, at);
        r.findings = (0..n).map(|i| f(&format!("f{i}"))).collect();
        append(d, &r).unwrap();
    }

    #[test]
    fn a_failure_in_the_middle_of_a_deletion_leaves_everything_as_it_was() {
        let d = scratch("delete-rollback");
        for (id, at) in [("a", 10), ("b", 20)] {
            review_with_findings(&d, id, at, 2);
            crate::triage::set_with(&d, id, 0, "wait", 2, 100, None).unwrap();
            crate::notes::set_memo(&d, id, 1, 2, "メモ", 100).unwrap();
        }
        // b の行を消そうとすると、失敗するようにする（a は、その前に消し終わっている）
        db::open(&d).unwrap().execute_batch("CREATE TRIGGER stop_b BEFORE DELETE ON reviews WHEN OLD.id = 'b' BEGIN SELECT RAISE(ABORT, 'stop'); END;").unwrap();
        let before = |c: &Connection| ["reviews", "triage", "triage_log", "finding_note"].map(|t| c.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get::<_, i64>(0)).unwrap());
        let snapshot = before(&db::open(&d).unwrap());
        assert!(delete_reviews(&d, &["a".into(), "b".into()]).is_err());
        assert_eq!(before(&db::open(&d).unwrap()), snapshot, "途中で失敗したら、a の分も含めて、何も消えない");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn deleting_nothing_or_the_same_id_twice_counts_each_review_once() {
        let d = scratch("delete-edge");
        review_with_findings(&d, "a", 10, 1);
        review_with_findings(&d, "b", 20, 1);
        assert_eq!(delete_reviews(&d, &[]).unwrap(), 0, "空の一覧は、0 件で、何も消さない");
        assert_eq!(db::open(&d).unwrap().query_row("SELECT COUNT(*) FROM reviews", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
        assert_eq!(delete_reviews(&d, &["a".into(), "a".into()]).unwrap(), 1, "同じ ID を 2 回渡しても、1 件");
        assert_eq!(delete_reviews(&d, &["a".into()]).unwrap(), 0, "もう無いものは、数えない");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn open_count_fails_instead_of_showing_zero_when_the_triage_cannot_be_read() {
        let d = scratch("open-count-broken");
        review_with_findings(&d, "a", 10, 3);
        let conn = db::open(&d).unwrap();
        assert_eq!(open_count(&conn, "/r").unwrap(), 3);
        conn.execute_batch("DROP TABLE triage; CREATE TABLE triage (x INTEGER);").unwrap();
        let err = open_count(&conn, "/r").unwrap_err();
        assert!(err.contains("仕分けを数えられません"), "0 件に見せず、理由つきのエラーにする: {err}");
        drop(conn);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn open_count_subtracts_wait_done_and_dismiss_and_never_goes_below_zero() {
        let d = scratch("open-count-mix");
        review_with_findings(&d, "a", 10, 3);
        // 0: 待ち／1: 無視／2: 対応＋対応済みの印 → 3 件とも、「未」ではない
        for (i, st) in [(0, "wait"), (1, "dismiss"), (2, "fix")] {
            crate::triage::set_with(&d, "a", i, st, 3, 100, None).unwrap();
        }
        crate::notes::set_done(&d, "a", 2, 3, true, 200, None).unwrap();
        // 別のレビュー（未仕分けが 2 件）と、別のリポジトリのレビューは、混ざらない
        review_with_findings(&d, "b", 20, 2);
        let conn = db::open(&d).unwrap();
        conn.execute("UPDATE reviews SET repo = '/other' WHERE id = 'b'", []).unwrap();
        assert_eq!(open_count(&conn, "/r").unwrap(), 0, "3 − (待ち 1 ＋ 無視 1 ＋ 済み 1) ＝ 0。別のリポジトリの分は入らない");
        assert_eq!(open_count(&conn, "/other").unwrap(), 2);
        // 指摘の件数を、仕分けより少なく書き換えても（範囲の外の仕分けは、数えない）、負にならず、0 で止まる
        conn.execute("UPDATE reviews SET findings_count = 1 WHERE id = 'a'", []).unwrap();
        assert_eq!(open_count(&conn, "/r").unwrap(), 0);
        drop(conn);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn open_counts_for_many_repos_match_the_one_by_one_counts() {
        let d = scratch("open-counts-many");
        review_with_findings(&d, "a", 10, 4);
        review_with_findings(&d, "b", 20, 3);
        review_with_findings(&d, "c", 30, 5);
        crate::triage::set_with(&d, "a", 0, "wait", 4, 100, None).unwrap();
        crate::triage::set_with(&d, "a", 1, "dismiss", 4, 101, None).unwrap();
        crate::triage::set_with(&d, "c", 2, "fix", 5, 102, None).unwrap();
        crate::notes::set_done(&d, "c", 2, 5, true, 200, None).unwrap();
        let conn = db::open(&d).unwrap();
        conn.execute("UPDATE reviews SET repo = '/other' WHERE id = 'b'", []).unwrap();
        conn.execute("UPDATE reviews SET repo = '/third' WHERE id = 'c'", []).unwrap();
        let repos: Vec<String> = ["/r", "/other", "/third", "/none"].iter().map(|s| s.to_string()).collect();
        let many = open_counts(&conn, &repos).unwrap();
        for r in &repos {
            assert_eq!(many[r], open_count(&conn, r).unwrap(), "{r}: まとめて数えても、1 つずつ数えたものと同じ");
        }
        assert_eq!((many["/r"], many["/other"], many["/third"], many["/none"]), (2, 3, 4, 0));
        assert!(open_counts(&conn, &[]).unwrap().is_empty());
        drop(conn);
        let _ = std::fs::remove_dir_all(&d);
    }
}
