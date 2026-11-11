//! 成果物に使える、レビューの記録（レポート）。**いつ・何をして・どう対応したか**を、Markdown・HTML（PDF 用）・Excel に出す。
//!
//! - 元データは、記録した値と、あなたの操作（仕分け・対応メモ・対応済み）だけ。アプリが推測で補った値は、「推定」と書く。
//! - 仕分けの時刻が無いもの（この機能より前の記録）は、「記録なし」と書く。時刻を作らない。
//! - 3 つの形式は、同じ `Report` から作る（内容が食い違わない）。
//! - 時刻は、画面から渡された時差（分）で、ローカル時刻にして出す（標準ライブラリに、時差の扱いが無いため）。

use crate::agent_report::{self, AgentReport};
use crate::db;
use crate::notes::{self, Note};
use crate::provider::Usage;
use crate::store::{self, ReviewRecord};
use crate::triage::{self, LogRow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ── 出力の指定 ────────────────────────────────────────────────────────────────

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct Options {
    /// リポジトリの根のパス。None ならすべて。
    #[serde(default)]
    pub repo: Option<String>,
    /// この時刻（UNIX ミリ秒）以降。None なら、最初から。
    #[serde(default)]
    pub since_ms: Option<u64>,
    /// この時刻以前。None なら、いままで。
    #[serde(default)]
    pub until_ms: Option<u64>,
    /// コードの引用を含める（成果物に入れるときは、秘密の値が混ざっていないか、出力前に確認する）
    #[serde(default = "yes")]
    pub include_evidence: bool,
    /// LLM に送った文章の全文を含める
    #[serde(default)]
    pub include_prompt: bool,
    #[serde(default = "yes")]
    pub include_tokens: bool,
    /// 仕分けの経過（いつ何にしたか）を含める
    #[serde(default = "yes")]
    pub include_log: bool,
    /// 表示する時刻の時差（分）。日本は 540。
    #[serde(default)]
    pub tz_offset_min: i32,
    /// レビューを 1 件ずつ選んだとき、その識別子。渡すと、**その分だけ**を対象にし、リポジトリと期間の指定は使わない。
    #[serde(default)]
    pub review_ids: Option<Vec<String>>,
    /// 出力言語（en / ja）
    #[serde(default)]
    pub lang: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options { repo: None, since_ms: None, until_ms: None, include_evidence: true, include_prompt: false, include_tokens: true, include_log: true, tz_offset_min: 0, review_ids: None, lang: None }
    }
}

// ── 集めたデータ ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub reviews: usize,
    pub failed: usize,
    pub findings: usize,
    pub fix: usize,
    pub wait: usize,
    pub dismiss: usize,
    pub new: usize,
    /// 「対応」のうち、対応済みの印を付けたもの
    pub done: usize,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// 成功したが、入力か出力のトークンが報告されなかったレビュー（上のトークンには、含めていない）
    pub unreported: usize,
}

/// 次のレビューで見えた、その指摘のその後。
#[derive(Debug, Clone, Serialize)]
pub struct Later {
    pub review_id: String,
    pub at: u64,
    /// persists | gone | not_reraised | file_missing | unknown
    pub fate: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FindingEntry {
    pub n: usize,
    pub file: String,
    pub line: Option<u32>,
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub evidence: String,
    pub verified: bool,
    pub rule: Option<String>,
    /// new | fix | wait | dismiss
    pub state: String,
    pub note: Note,
    pub log: Vec<LogRow>,
    /// 前回のレビューから、仕分けを引き継いだか
    pub carried_from: Option<String>,
    pub later: Option<Later>,
    /// エージェントの「対応の報告」（会話の記録から取り込んだ申告。事実の確認ではない）。取り込んでいなければ None
    pub agent: Option<AgentReport>,
    /// 会話の記録と結ぶための識別子（`mrcr:<レビュー>#<番号>`）
    pub tag: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FateEntry {
    pub title: String,
    pub file: String,
    pub state_before: String,
    pub fate: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewEntry {
    pub n: usize,
    pub id: String,
    pub created_at: u64,
    pub repo: String,
    pub recipe_title: String,
    pub provider_label: String,
    pub model: String,
    pub duration_ms: u64,
    pub base_label: String,
    pub base_hash: String,
    pub head: String,
    pub diff_hash: String,
    pub files: Vec<(String, usize)>,
    pub excluded: Vec<(String, String)>,
    pub usage: Usage,
    pub cost_usd: Option<f64>,
    /// 実行が失敗したときの理由
    pub error: Option<String>,
    pub parse_error: Option<String>,
    pub run_notes: Vec<String>,
    pub prompt: Option<String>,
    pub findings: Vec<FindingEntry>,
    pub fates: Vec<FateEntry>,
    pub tag: String,
}

impl ReviewEntry {
    pub fn counts(&self) -> (usize, usize, usize, usize, usize) {
        let c = |s: &str| self.findings.iter().filter(|f| f.state == s).count();
        let done = self.findings.iter().filter(|f| f.note.done_at.is_some()).count();
        (c("fix"), c("wait"), c("dismiss"), c("new"), done)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub generated_at: u64,
    /// 出力したアプリの名前（Free 殻・Pro 殻・自分用ビルドで、違う）。
    pub app_name: String,
    pub app_version: String,
    pub options_repo: Option<String>,
    pub since_ms: Option<u64>,
    pub until_ms: Option<u64>,
    pub tz_offset_min: i32,
    pub include_evidence: bool,
    pub include_prompt: bool,
    pub include_tokens: bool,
    pub include_log: bool,
    /// レビューを 1 件ずつ選んだか（選んだなら、リポジトリと期間の指定は使っていない）
    pub selected: bool,
    /// 選んだのに、履歴に見つからなかった識別子（削除された、など）。黙って飛ばさず、出力にも書く
    pub missing_ids: Vec<String>,
    /// 壊れていて読めず、この出力に含めていない記録の件数
    pub unreadable: usize,
    pub summary: Summary,
    /// 出力言語（en / ja）
    pub lang: String,
    /// 古い順
    pub reviews: Vec<ReviewEntry>,
}

// ── 時刻・数の整形 ────────────────────────────────────────────────────────────

/// 1970-01-01 からの日数を、（年, 月, 日）にする。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 「2026-10-26 16:33」。`tz_min` は、UTC からの時差（分）。
pub fn fmt_datetime(ms: u64, tz_min: i32) -> String {
    let secs = (ms / 1000) as i64 + tz_min as i64 * 60;
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", rem / 3600, rem % 3600 / 60)
}

/// 「+09:00」
pub fn fmt_offset(tz_min: i32) -> String {
    let sign = if tz_min < 0 { '-' } else { '+' };
    let a = tz_min.abs();
    format!("{sign}{:02}:{:02}", a / 60, a % 60)
}

pub fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[allow(dead_code)]
pub fn fmt_secs(ms: u64) -> String {
    fmt_secs_for(ms, crate::i18n::Lang::default())
}

pub fn fmt_secs_for(ms: u64, lang: crate::i18n::Lang) -> String {
    lang.texts().fmt_secs(ms)
}

#[allow(dead_code)]
pub fn state_label(s: &str) -> &'static str {
    state_label_for(s, crate::i18n::Lang::default())
}

pub fn state_label_for(s: &str, lang: crate::i18n::Lang) -> &'static str {
    lang.texts().state_label(s)
}

pub fn severity_label(s: &str) -> &str {
    match s {
        "high" => "high",
        "medium" => "medium",
        "low" => "low",
        other => other,
    }
}

#[allow(dead_code)]
pub fn fate_label(f: &str) -> &'static str {
    fate_label_for(f, crate::i18n::Lang::default())
}

pub fn fate_label_for(f: &str, lang: crate::i18n::Lang) -> &'static str {
    lang.texts().fate_label(f)
}

#[allow(dead_code)]
pub fn result_label(r: &ReviewEntry) -> &'static str {
    result_label_for(r, crate::i18n::Lang::default())
}

pub fn result_label_for(r: &ReviewEntry, lang: crate::i18n::Lang) -> &'static str {
    let t = lang.texts();
    if r.error.is_some() {
        t.result_failed
    } else if r.parse_error.is_some() {
        t.result_parse_err
    } else {
        t.result_ok
    }
}

pub fn recipe_title_for(title: &str, lang: crate::i18n::Lang) -> &str {
    lang.texts().recipe_title(title)
}

pub fn base_label_for(label: &str, lang: crate::i18n::Lang) -> &str {
    lang.texts().base_label(label)
}

// ── 集める ────────────────────────────────────────────────────────────────────

/// 1 件のレビューの、レポート用の中身。**読み取りに失敗したら、エラーを返す**（空や「記録なし」に見せない）。
fn entry_of(conn: &rusqlite::Connection, rec: &ReviewRecord, n: usize, later: &HashMap<(String, usize), Later>, o: &Options) -> Result<ReviewEntry, String> {
    let count = rec.findings.len();
    let ctx = |e: String| format!("レビュー {} の記録を読み取れません: {e}", rec.id);
    let log_rows = triage::try_log_with_conn(conn, &rec.id).map_err(ctx)?;
    let states = triage::try_get_with_conn(conn, &rec.id, count).map_err(ctx)?;
    let notes = notes::try_get_all_with_logs(conn, &rec.id, count, &log_rows).map_err(ctx)?;
    let agents = agent_report::try_get_all_with_conn(conn, &rec.id, count).map_err(ctx)?;
    let mut logs: HashMap<usize, Vec<LogRow>> = HashMap::new();
    for row in log_rows {
        logs.entry(row.idx).or_default().push(row);
    }
    let findings = rec
        .findings
        .iter()
        .enumerate()
        .map(|(i, f)| FindingEntry {
            n: i + 1,
            file: f.file.clone(),
            line: f.line,
            severity: f.severity.clone(),
            title: f.title.clone(),
            detail: f.detail.clone(),
            evidence: f.evidence.clone(),
            verified: f.verified,
            rule: f.rule.clone(),
            state: states[i].clone(),
            note: notes[i].clone(),
            log: logs.remove(&i).unwrap_or_default(),
            carried_from: rec.carries.get(i).and_then(|c| c.as_ref()).map(|c| c.from_review.clone()),
            later: later.get(&(rec.id.clone(), i)).cloned(),
            agent: agents[i].clone(),
            tag: triage::finding_tag(&rec.id, i),
        })
        .collect();
    let lang = o.lang.as_deref().and_then(crate::i18n::Lang::from_code).unwrap_or_default();
    let t = lang.texts();
    Ok(ReviewEntry {
        n,
        id: rec.id.clone(),
        created_at: rec.created_at,
        repo: rec.repo.clone(),
        recipe_title: recipe_title_for(&rec.recipe.title, lang).to_string(),
        provider_label: rec.provider.label.clone(),
        model: rec.model.clone(),
        duration_ms: rec.duration_ms,
        base_label: match rec.snapshot.base.spec.as_str() {
            "" => String::new(),
            "HEAD" => t.base_uncommitted.to_string(),
            ":root" => t.base_root.to_string(),
            _ => t.base_from_commit(&rec.snapshot.base.hash.chars().take(7).collect::<String>(), &rec.snapshot.base.subject),
        },
        base_hash: rec.snapshot.base.hash.clone(),
        head: rec.snapshot.head.clone(),
        diff_hash: rec.snapshot.diff_hash.clone(),
        files: rec.snapshot.files.iter().map(|f| (f.path.clone(), f.bytes)).collect(),
        excluded: rec.snapshot.excluded.iter().map(|e| (e.path.clone(), e.reason.clone())).collect(),
        // 失敗した実行は、トークンも金額も数えない（概要の合計からも除いている）
        usage: if rec.error.is_some() { Usage::default() } else { rec.usage.clone() },
        cost_usd: if rec.error.is_some() { None } else { rec.cost_usd },
        error: rec.error.clone(),
        parse_error: rec.parse_error.clone(),
        run_notes: rec.notes.clone(),
        prompt: o.include_prompt.then(|| rec.prompt.clone()),
        findings,
        fates: rec
            .fates
            .iter()
            .map(|f| FateEntry { title: f.title.clone(), file: f.file.clone(), state_before: f.state_before.clone(), fate: f.fate.clone() })
            .collect(),
        tag: triage::review_tag(&rec.id),
    })
}

/// データベースから、レポートの元を集める。
/// **読み取りに失敗したら、エラーを返し、出力を止める**（空のレポートや、「記録なし」だらけのレポートに見せない）。
/// 壊れていて読めなかった記録は、飛ばして、その件数を、出力の「注意」に書く（黙って抜かない）。
pub fn build(data_dir: &Path, o: &Options, app_name: &str, app_version: &str) -> Result<Report, String> {
    let (pool, unreadable) = store::try_load_all(data_dir).map_err(|e| format!("レポートの元になる記録を読み取れませんでした。出力を止めました（{e}）"))?;
    // 「その後」は、選ばなかったレビューも含めて作る（選んだレビューの指摘が、あとのレビューで、どうなったか）。キーは、レビューの識別子なので、リポジトリで絞らなくてよい
    let all = &pool;
    let mut later: HashMap<(String, usize), Later> = HashMap::new();
    for r in all {
        for f in &r.fates {
            let key = (f.from_review.clone(), f.from_index);
            if later.get(&key).is_none_or(|l| l.at < r.created_at) {
                later.insert(key, Later { review_id: r.id.clone(), at: r.created_at, fate: f.fate.clone() });
            }
        }
    }
    let mut missing_ids: Vec<String> = Vec::new();
    let mut picked: Vec<&ReviewRecord> = match &o.review_ids {
        // 選んだレビュー: その分だけ。リポジトリと期間は、使わない
        Some(ids) => {
            let mut seen = std::collections::HashSet::new();
            let want: Vec<&String> = ids.iter().filter(|i| seen.insert(i.as_str())).collect();
            missing_ids = want.iter().filter(|i| !all.iter().any(|r| &r.id == **i)).map(|i| (*i).clone()).collect();
            all.iter().filter(|r| want.iter().any(|i| **i == r.id)).collect()
        }
        None => all
            .iter()
            .filter(|r| o.repo.as_deref().is_none_or(|p| p == r.repo))
            .filter(|r| o.since_ms.is_none_or(|s| r.created_at >= s) && o.until_ms.is_none_or(|u| r.created_at <= u))
            .collect(),
    };
    picked.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
    let conn = db::open(data_dir).map_err(|e| format!("レポートの元になる記録を読み取れませんでした。出力を止めました（{e}）"))?;
    let resolved_lang = match o.lang.as_deref().and_then(crate::i18n::Lang::from_code) {
        Some(l) => l.code().to_string(),
        None => {
            let s_lang = crate::settings::load(data_dir).ok().map(|s| s.language).unwrap_or_else(|| "auto".to_string());
            crate::i18n::Lang::resolve_arg(Some(&s_lang), crate::system_lang()).unwrap_or_default().code().to_string()
        }
    };
    let mut effective_o = o.clone();
    effective_o.lang = Some(resolved_lang.clone());

    let reviews: Vec<ReviewEntry> = picked
        .iter()
        .enumerate()
        .map(|(i, r)| entry_of(&conn, r, i + 1, &later, &effective_o))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("レポートの元になる記録を読み取れませんでした。出力を止めました（{e}）"))?;

    let mut s = Summary { reviews: reviews.len(), ..Summary::default() };
    for r in &reviews {
        if r.error.is_some() {
            s.failed += 1;
        } else {
            if r.usage.input_tokens.is_none() || r.usage.output_tokens.is_none() {
                s.unreported += 1;
            }
            s.input += r.usage.input_tokens.unwrap_or(0);
            s.output += r.usage.output_tokens.unwrap_or(0);
            s.cache_read += r.usage.cache_read_tokens.unwrap_or(0);
            s.cache_write += r.usage.cache_write_tokens.unwrap_or(0);
        }
        s.findings += r.findings.len();
        let (fix, wait, dismiss, new, done) = r.counts();
        s.fix += fix;
        s.wait += wait;
        s.dismiss += dismiss;
        s.new += new;
        s.done += done;
    }
    Ok(Report {
        generated_at: db::now_ms(),
        app_name: app_name.to_string(),
        app_version: app_version.to_string(),
        lang: resolved_lang,
        options_repo: o.repo.clone(),
        since_ms: o.since_ms,
        until_ms: o.until_ms,
        tz_offset_min: o.tz_offset_min,
        include_evidence: o.include_evidence,
        include_prompt: o.include_prompt,
        include_tokens: o.include_tokens,
        include_log: o.include_log,
        selected: o.review_ids.is_some(),
        missing_ids,
        unreadable,
        summary: s,
        reviews,
    })
}

// ── 経過（時系列）─────────────────────────────────────────────────────────────

/// 指摘 1 件の、「いつ何があったか」を、時刻順に並べた文。時刻が無い仕分けは、「記録なし」と言う。
#[allow(dead_code)]
pub fn timeline(f: &FindingEntry, tz: i32) -> Vec<String> {
    timeline_for(f, tz, crate::i18n::Lang::default())
}

pub fn timeline_for(f: &FindingEntry, tz: i32, lang: crate::i18n::Lang) -> Vec<String> {
    let t = lang.texts();
    let mut events: Vec<(u64, u8, String)> = Vec::new();
    for l in &f.log {
        let head_short = l.head.as_deref().map(|h| h.chars().take(7).collect::<String>());
        let state_str = if l.state == "new" {
            t.state_reverted_new_lower
        } else {
            state_label_for(&l.state, lang)
        };
        events.push((
            l.at,
            0,
            t.timeline_event(&fmt_datetime(l.at, tz), state_str, l.source == triage::SRC_CARRY, head_short.as_deref()),
        ));
    }
    if let (Some(at), true) = (f.note.memo_at, !f.note.memo.is_empty()) {
        events.push((at, 1, format!("{} — {}", fmt_datetime(at, tz), t.timeline_memo_added)));
    }
    if let Some(at) = f.note.done_at {
        let head_short = f.note.done_head.as_deref().map(|h| h.chars().take(7).collect::<String>());
        events.push((at, 2, t.timeline_done(&fmt_datetime(at, tz), head_short.as_deref())));
    }
    events.sort_by_key(|e| (e.0, e.1));
    let mut out: Vec<String> = events.into_iter().map(|e| e.2).collect();
    if out.is_empty() && f.state != "new" {
        out.push(t.timeline_no_record.to_string());
    }
    out
}

/// エージェントの申告の 1 行（「事実の確認ではない」ことを、必ず添える）。
#[allow(dead_code)]
pub fn agent_text(a: &AgentReport, tz: i32) -> String {
    agent_text_for(a, tz, crate::i18n::Lang::default())
}

pub fn agent_text_for(a: &AgentReport, tz: i32, lang: crate::i18n::Lang) -> String {
    lang.texts().agent_text(a.verdict == agent_report::DID, &fmt_datetime(a.reported_at, tz), &a.text)
}

#[allow(dead_code)]
pub fn later_text(l: &Later, tz: i32) -> String {
    later_text_for(l, tz, crate::i18n::Lang::default())
}

pub fn later_text_for(l: &Later, tz: i32, lang: crate::i18n::Lang) -> String {
    lang.texts().later_text(&fmt_datetime(l.at, tz), fate_label_for(&l.fate, lang))
}

// ── Markdown ──────────────────────────────────────────────────────────────────

/// Markdown の特別な文字を、文字そのものとして出す（LLM の出力や、ファイル名に、記号が混ざっても、崩れない）。
/// 改行は、空白 1 つにする（箇条書きや表を壊さないため）。
fn md(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|' | '#' | '~' => {
                o.push('\\');
                o.push(c);
            }
            '\r' => {}
            '\n' => o.push(' '),
            _ => o.push(c),
        }
    }
    o
}

fn inline_code(s: &str) -> String {
    let s = s.replace(['\n', '\r'], " ");
    if s.contains('`') {
        format!("`` {s} ``")
    } else {
        format!("`{s}`")
    }
}

/// 本文の中の ``` より長い囲み。
fn fence_for(s: &str) -> String {
    let (mut max, mut run) = (0usize, 0usize);
    for c in s.chars() {
        if c == '`' {
            run += 1;
            max = max.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat((max + 1).max(3))
}

fn period_text(r: &Report) -> String {
    let t = crate::i18n::Lang::from_code(&r.lang).unwrap_or_default().texts();
    let tz = r.tz_offset_min;
    if r.selected {
        // 選んだレビューの、最初と最後の日時
        return match (r.reviews.first(), r.reviews.last()) {
            (Some(a), Some(b)) => t.period_selected_range(&fmt_datetime(a.created_at, tz), &fmt_datetime(b.created_at, tz)),
            _ => t.period_selected.to_string(),
        };
    }
    match (r.since_ms, r.until_ms) {
        (None, None) => t.period_all.to_string(),
        (Some(s), None) => t.period_since(&fmt_datetime(s, tz)),
        (None, Some(u)) => t.period_until(&fmt_datetime(u, tz)),
        (Some(s), Some(u)) => t.period_between(&fmt_datetime(s, tz), &fmt_datetime(u, tz)),
    }
}

fn tokens_text_for(u: &Usage, lang: crate::i18n::Lang) -> String {
    let t = lang.texts();
    let n = |v: Option<u64>| v.map(fmt_num).unwrap_or_else(|| t.unreported_lower.to_string());
    t.tokens_pair(&n(u.input_tokens), &n(u.output_tokens))
}

#[allow(dead_code)]
pub fn tokens_line(v: &ReviewEntry) -> String {
    tokens_line_for(v, crate::i18n::Lang::default())
}

pub fn tokens_line_for(v: &ReviewEntry, lang: crate::i18n::Lang) -> String {
    let t = lang.texts();
    if v.error.is_some() {
        t.tokens_failed_exec.to_string()
    } else if v.usage.input_tokens.is_none() && v.usage.output_tokens.is_none() {
        t.tokens_unreported_line.to_string()
    } else {
        t.tokens_reported_line(&tokens_text_for(&v.usage, lang))
    }
}

#[allow(dead_code)]
pub fn tokens_cell(v: &ReviewEntry) -> String {
    tokens_cell_for(v, crate::i18n::Lang::default())
}

pub fn tokens_cell_for(v: &ReviewEntry, lang: crate::i18n::Lang) -> String {
    let t = lang.texts();
    if v.error.is_some() {
        "—".to_string()
    } else if v.usage.input_tokens.is_none() && v.usage.output_tokens.is_none() {
        t.unreported_lower.to_string()
    } else {
        tokens_text_for(&v.usage, lang)
    }
}

#[allow(dead_code)]
pub fn base_or_none(v: &ReviewEntry) -> String {
    base_or_none_for(v, crate::i18n::Lang::default())
}

pub fn base_or_none_for(v: &ReviewEntry, lang: crate::i18n::Lang) -> String {
    let t = lang.texts();
    if v.base_label.is_empty() {
        t.no_record.to_string()
    } else {
        base_label_for(&v.base_label, lang).to_string()
    }
}

pub fn summary_lines(r: &Report) -> Vec<(String, String)> {
    let lang = crate::i18n::Lang::from_code(&r.lang).unwrap_or_default();
    let t = lang.texts();
    let s = &r.summary;
    let repos_text = if r.selected {
        let mut repos: Vec<&str> = Vec::new();
        for v in &r.reviews {
            if !repos.contains(&v.repo.as_str()) {
                repos.push(&v.repo);
            }
        }
        if repos.is_empty() { "—".to_string() } else { repos.join(t.list_sep) }
    } else {
        r.options_repo.clone().unwrap_or_else(|| t.all_repos.to_string())
    };
    let mut v = vec![
        (t.sum_repo.to_string(), repos_text),
        (t.sum_period.to_string(), period_text(r)),
        (t.sum_exported_at.to_string(), t.exported_at_val(&fmt_datetime(r.generated_at, r.tz_offset_min), &fmt_offset(r.tz_offset_min))),
        (t.sum_exported_by.to_string(), format!("{} {}", r.app_name, r.app_version)),
        (t.sum_reviews.to_string(), t.reviews_count_val(s.reviews, s.failed)),
        (t.sum_findings.to_string(), t.findings_count_val(s.findings, s.fix, s.done, s.wait, s.dismiss, s.new)),
    ];
    if !r.missing_ids.is_empty() {
        v.push((t.sum_notice.to_string(), t.missing_ids_notice(r.missing_ids.len(), &r.missing_ids.join(t.list_sep))));
    }
    if r.unreadable > 0 {
        v.push((t.sum_notice.to_string(), t.unreadable_notice(r.unreadable)));
    }
    if r.include_tokens {
        let cr = (s.cache_read > 0).then(|| fmt_num(s.cache_read));
        let cw = (s.cache_write > 0).then(|| fmt_num(s.cache_write));
        v.push((
            t.sum_tokens.to_string(),
            t.summary_tokens_val(&fmt_num(s.input), &fmt_num(s.output), cr.as_deref(), cw.as_deref(), s.unreported),
        ));
    }
    v
}

#[allow(dead_code)]
pub fn notice(app_name: &str) -> String {
    notice_for(app_name, crate::i18n::Lang::default())
}

pub fn notice_for(app_name: &str, lang: crate::i18n::Lang) -> String {
    lang.texts().notice(app_name)
}

pub fn markdown(r: &Report) -> String {
    let lang = crate::i18n::Lang::from_code(&r.lang).unwrap_or_default();
    let t = lang.texts();
    let tz = r.tz_offset_min;
    let mut o = String::new();
    o.push_str(&format!("# {}\n\n", t.report_title));
    o.push_str(t.md_item_header);
    for (k, v) in summary_lines(r) {
        o.push_str(&format!("| {} | {} |\n", md(&k), md(&v)));
    }
    o.push_str(&format!("\n> {}\n\n", md(&notice_for(&r.app_name, lang))));

    if r.reviews.is_empty() {
        o.push_str(&format!("{}\n", t.no_reviews_in_period));
        return o;
    }

    o.push_str(&format!("## {}\n\n", t.list_section_title));
    o.push_str(t.md_list_header);
    if r.include_tokens {
        o.push_str(t.md_list_tokens_col);
    }
    o.push_str("\n| --- | --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |");
    if r.include_tokens {
        o.push_str(" --- |");
    }
    o.push('\n');
    for v in &r.reviews {
        let (fix, wait, dismiss, new, done) = v.counts();
        o.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            v.n,
            fmt_datetime(v.created_at, tz),
            md(&v.recipe_title),
            md(&v.provider_label),
            md(&base_or_none_for(v, lang)),
            md(result_label_for(v, lang)),
            v.findings.len(),
            fix,
            wait,
            dismiss,
            new,
            done
        ));
        if r.include_tokens {
            o.push_str(&format!(" {} |", md(&tokens_cell_for(v, lang))));
        }
        o.push('\n');
    }
    o.push('\n');

    for v in &r.reviews {
        o.push_str(&format!("## {}. {} — {}\n\n", v.n, md(&v.recipe_title), fmt_datetime(v.created_at, tz)));
        o.push_str(&format!(
            "- **{}**: {}{} · {} {}\n",
            t.label_execution,
            md(&v.provider_label),
            if v.model.is_empty() { String::new() } else { format!(" / {}", md(&v.model)) },
            t.label_duration,
            fmt_secs_for(v.duration_ms, lang)
        ));
        if r.include_tokens {
            o.push_str(&format!("- **{}**: {}\n", t.sum_tokens, md(&tokens_line_for(v, lang))));
        }
        o.push_str(&format!("- **{}**: {}\n", t.label_result, result_label_for(v, lang)));
        if let Some(e) = &v.error {
            o.push_str(&format!("- **{}**: {}\n", t.label_failure_reason, md(e)));
        }
        if let Some(e) = &v.parse_error {
            o.push_str(&format!("- **{}**: {}\n", t.label_parse_err_reason, md(e)));
        }
        if !v.base_label.is_empty() {
            o.push_str(&format!("- **{}**: {}{}\n", t.label_range, md(&v.base_label), t.base_changes_suffix));
        }
        let base_code = (!v.base_hash.is_empty()).then(|| inline_code(&v.base_hash.chars().take(10).collect::<String>()));
        let rec_tag = t.record_tag_md(
            &inline_code(&v.head.chars().take(10).collect::<String>()),
            &inline_code(&v.diff_hash.chars().take(10).collect::<String>()),
            base_code.as_deref(),
            &inline_code(&v.tag),
        );
        o.push_str(&format!("- **{}**: {}\n", t.label_record, rec_tag));
        if !v.files.is_empty() {
            let label = t.files_sent_label(v.files.len());
            o.push_str(&format!("- **{label}**: {}\n", v.files.iter().map(|(p, _)| inline_code(p)).collect::<Vec<_>>().join(t.list_sep)));
        }
        if !v.excluded.is_empty() {
            let label = t.files_excluded_label(v.excluded.len());
            o.push_str(&format!("- **{label}**: {}\n", v.excluded.iter().map(|(p, why)| format!("{} ({})", inline_code(p), md(why))).collect::<Vec<_>>().join(t.list_sep)));
        }
        for n in &v.run_notes {
            o.push_str(&format!("- **{}**: {}\n", t.label_run_note, md(n)));
        }
        o.push('\n');

        if !v.fates.is_empty() {
            o.push_str(&format!("### {}\n\n", t.section_changes_from_prev));
            for f in &v.fates {
                let prev_label = t.prev_state_label(state_label_for(&f.state_before, lang));
                o.push_str(&format!(
                    "- {} ({} · {}) — {}\n",
                    md(if f.title.is_empty() { t.no_title } else { &f.title }),
                    inline_code(&f.file),
                    prev_label,
                    fate_label_for(&f.fate, lang)
                ));
            }
            o.push('\n');
        }

        o.push_str(&format!("### {}\n\n", t.findings_heading(v.findings.len())));
        if v.findings.is_empty() {
            o.push_str(&format!("{}\n\n", t.no_findings_detected));
        }
        for f in &v.findings {
            let place = match f.line {
                Some(l) => format!("{}:{}", f.file, l),
                None => f.file.clone(),
            };
            o.push_str(&format!("#### {}-{}. [{}] {}\n\n", v.n, f.n, severity_label(&f.severity), md(if f.title.is_empty() { t.no_title } else { &f.title })));
            let rule_str = f.rule.as_deref().map(|r| t.rule_inline_md(&inline_code(r))).unwrap_or_default();
            o.push_str(&format!("- **{}**: {}{}\n", t.label_location, inline_code(if place.is_empty() { t.unknown_place } else { &place }), rule_str));
            let ev_msg = if f.verified { t.evidence_match_full } else { t.evidence_mismatch_full };
            o.push_str(&format!("- **{}**: {}\n", t.label_evidence, ev_msg));
            o.push_str(&format!("- **{}**: {}\n", t.label_detail, md(&f.detail)));
            let done_tag = if f.note.done_at.is_some() { t.done_suffix } else { "" };
            o.push_str(&format!("- **{}**: **{}**{}\n", t.label_triage_status, state_label_for(&f.state, lang), done_tag));
            if r.include_log {
                let tl = timeline_for(f, tz, lang);
                if !tl.is_empty() {
                    o.push_str(&format!("- **{}**:\n", t.label_timeline));
                    for line in tl {
                        o.push_str(&format!("  - {}\n", md(&line)));
                    }
                }
            } else if f.state != "new" {
                if let Some(at) = f.note.triage_at {
                    o.push_str(&format!("- **{}**: {}\n", t.label_last_triaged_at, fmt_datetime(at, tz)));
                }
            }
            if !f.note.memo.is_empty() {
                o.push_str(&format!("- **{}**: {}\n", t.label_triage_memo, md(&f.note.memo)));
            }
            if let Some(from) = &f.carried_from {
                let carry_msg = t.carried_over_md(&inline_code(&triage::review_tag(from)));
                o.push_str(&format!("- **{}**: {}\n", t.label_carried_over, carry_msg));
            }
            if let Some(l) = &f.later {
                o.push_str(&format!("- **{}**: {}\n", t.label_subsequent_outcome, md(&later_text_for(l, tz, lang))));
            }
            if let Some(a) = &f.agent {
                o.push_str(&format!("- **{}**: {}\n", t.label_agent_statement, md(&agent_text_for(a, tz, lang))));
            }
            o.push_str(&format!("- **{}**: {}\n", t.label_identifier, inline_code(&f.tag)));
            if r.include_evidence {
                let fence = fence_for(&f.evidence);
                let ev_status = if f.verified { t.ev_found_short } else { t.ev_not_found_short };
                let ev_content = if f.evidence.is_empty() { t.no_snippet } else { &f.evidence };
                let ev_lbl = t.quotation_label_md(ev_status);
                o.push_str(&format!("\n{ev_lbl}:\n\n{fence}\n{ev_content}\n{fence}\n"));
            }
            o.push('\n');
        }

        if let Some(p) = &v.prompt {
            let fence = fence_for(p);
            let prompt_lbl = t.prompt_sent_label(p.chars().count());
            o.push_str(&format!("### {prompt_lbl}\n\n{fence}\n{p}\n{fence}\n\n"));
        }
    }
    o
}

// ── HTML（PDF 用）─────────────────────────────────────────────────────────────

/// HTML の特別な文字を、文字そのものとして出す。**画面と PDF に出る文字は、必ず、これを通す**。
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            _ => o.push(c),
        }
    }
    o
}

/// 画面のプレビューと、PDF（印刷）に使う HTML の断片。見た目は、アプリの CSS（`.report`）が決める。
pub fn html(r: &Report) -> String {
    let lang = crate::i18n::Lang::from_code(&r.lang).unwrap_or_default();
    let t = lang.texts();
    let tz = r.tz_offset_min;
    let mut o = String::new();
    o.push_str(&format!("<article class=\"report\">\n<h1>{}</h1>\n<table class=\"rsum\"><tbody>\n", esc(t.report_title)));
    for (k, v) in summary_lines(r) {
        o.push_str(&format!("<tr><th>{}</th><td>{}</td></tr>\n", esc(&k), esc(&v)));
    }
    o.push_str(&format!("</tbody></table>\n<p class=\"rnotice\">{}</p>\n", esc(&notice_for(&r.app_name, lang))));
    if r.reviews.is_empty() {
        o.push_str(&format!("<p>{}</p>\n</article>\n", esc(t.no_reviews_in_period)));
        return o;
    }
    o.push_str(t.html_list_header);
    if r.include_tokens {
        o.push_str(&format!("<th>{}</th>", esc(t.sum_tokens)));
    }
    o.push_str("</tr></thead><tbody>\n");
    for v in &r.reviews {
        let (fix, wait, dismiss, new, done) = v.counts();
        o.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td>",
            v.n,
            fmt_datetime(v.created_at, tz),
            esc(&v.recipe_title),
            esc(&v.provider_label),
            esc(&base_or_none_for(v, lang)),
            esc(result_label_for(v, lang)),
            v.findings.len(),
            fix,
            wait,
            dismiss,
            new,
            done
        ));
        if r.include_tokens {
            o.push_str(&format!("<td>{}</td>", esc(&tokens_cell_for(v, lang))));
        }
        o.push_str("</tr>\n");
    }
    o.push_str("</tbody></table>\n");

    for v in &r.reviews {
        o.push_str(&format!("<section class=\"rreview\">\n<h2>{}. {} — {}</h2>\n<ul class=\"rmeta\">\n", v.n, esc(&v.recipe_title), fmt_datetime(v.created_at, tz)));
        let li = |label: &str, body: String| format!("<li><b>{}</b>: {}</li>\n", esc(label), body);
        o.push_str(&li(
            t.label_execution,
            format!(
                "{}{} · {} {}",
                esc(&v.provider_label),
                if v.model.is_empty() { String::new() } else { format!(" / {}", esc(&v.model)) },
                t.label_duration,
                fmt_secs_for(v.duration_ms, lang)
            ),
        ));
        if r.include_tokens {
            o.push_str(&li(t.sum_tokens, esc(&tokens_line_for(v, lang))));
        }
        o.push_str(&li(t.label_result, esc(result_label_for(v, lang))));
        if let Some(e) = &v.error {
            o.push_str(&li(t.label_failure_reason, esc(e)));
        }
        if let Some(e) = &v.parse_error {
            o.push_str(&li(t.label_parse_err_reason, esc(e)));
        }
        if !v.base_label.is_empty() {
            o.push_str(&li(t.label_range, format!("{}{}", esc(&v.base_label), t.base_changes_suffix)));
        }
        let base_esc = (!v.base_hash.is_empty()).then(|| esc(&v.base_hash.chars().take(10).collect::<String>()));
        let rec_tag = t.record_tag_html(
            &esc(&v.head.chars().take(10).collect::<String>()),
            &esc(&v.diff_hash.chars().take(10).collect::<String>()),
            base_esc.as_deref(),
            &esc(&v.tag),
        );
        o.push_str(&li(t.label_record, rec_tag));
        if !v.files.is_empty() {
            let label = t.files_sent_label(v.files.len());
            o.push_str(&li(&label, v.files.iter().map(|(p, _)| format!("<code>{}</code>", esc(p))).collect::<Vec<_>>().join(t.list_sep)));
        }
        if !v.excluded.is_empty() {
            let label = t.files_excluded_label(v.excluded.len());
            o.push_str(&li(&label, v.excluded.iter().map(|(p, why)| format!("<code>{}</code> ({})", esc(p), esc(why))).collect::<Vec<_>>().join(t.list_sep)));
        }
        for n in &v.run_notes {
            o.push_str(&li(t.label_run_note, esc(n)));
        }
        o.push_str("</ul>\n");

        if !v.fates.is_empty() {
            o.push_str(&format!("<h3>{}</h3>\n<ul>\n", esc(t.section_changes_from_prev)));
            for f in &v.fates {
                let prev_label = t.prev_state_label(state_label_for(&f.state_before, lang));
                o.push_str(&format!(
                    "<li>{} (<code>{}</code> · {}) — {}</li>\n",
                    esc(if f.title.is_empty() { t.no_title } else { &f.title }),
                    esc(&f.file),
                    esc(&prev_label),
                    esc(fate_label_for(&f.fate, lang))
                ));
            }
            o.push_str("</ul>\n");
        }
        o.push_str(&format!("<h3>{}</h3>\n", esc(&t.findings_heading(v.findings.len()))));
        if v.findings.is_empty() {
            o.push_str(&format!("<p>{}</p>\n", esc(t.no_findings_detected)));
        }
        for f in &v.findings {
            let place = match f.line {
                Some(l) => format!("{}:{}", f.file, l),
                None => f.file.clone(),
            };
            o.push_str(&format!(
                "<div class=\"rfinding s-{}\">\n<h4><span class=\"sev sev-{}\">{}</span> {}-{}. {}</h4>\n<ul>\n",
                esc(&f.state),
                esc(&f.severity),
                esc(severity_label(&f.severity)),
                v.n,
                f.n,
                esc(if f.title.is_empty() { t.no_title } else { &f.title })
            ));
            let rule_str = f.rule.as_deref().map(|r| t.rule_inline_html(&esc(r))).unwrap_or_default();
            o.push_str(&li(t.label_location, format!("<code>{}</code>{}", esc(if place.is_empty() { t.unknown_place } else { &place }), rule_str)));
            let ev_msg = if f.verified { t.evidence_match_full } else { t.evidence_mismatch_full };
            o.push_str(&li(t.label_evidence, esc(ev_msg)));
            o.push_str(&li(t.label_detail, esc(&f.detail)));
            let done_tag = if f.note.done_at.is_some() { t.done_suffix } else { "" };
            o.push_str(&li(t.label_triage_status, format!("<strong>{}</strong>{}", esc(state_label_for(&f.state, lang)), done_tag)));
            if r.include_log {
                let tl = timeline_for(f, tz, lang);
                if !tl.is_empty() {
                    o.push_str(&format!("<li><b>{}</b>:<ul>{}</ul></li>\n", esc(t.label_timeline), tl.iter().map(|line| format!("<li>{}</li>", esc(line))).collect::<String>()));
                }
            } else if f.state != "new" {
                if let Some(at) = f.note.triage_at {
                    o.push_str(&li(t.label_last_triaged_at, esc(&fmt_datetime(at, tz))));
                }
            }
            if !f.note.memo.is_empty() {
                o.push_str(&li(t.label_triage_memo, esc(&f.note.memo)));
            }
            if let Some(from) = &f.carried_from {
                let carry_msg = t.carried_over_html(&esc(&triage::review_tag(from)));
                o.push_str(&li(t.label_carried_over, carry_msg));
            }
            if let Some(l) = &f.later {
                o.push_str(&li(t.label_subsequent_outcome, esc(&later_text_for(l, tz, lang))));
            }
            if let Some(a) = &f.agent {
                o.push_str(&li(t.label_agent_statement, esc(&agent_text_for(a, tz, lang))));
            }
            o.push_str(&li(t.label_identifier, format!("<code>{}</code>", esc(&f.tag))));
            o.push_str("</ul>\n");
            if r.include_evidence {
                let ev_status = if f.verified { t.ev_found_short } else { t.ev_not_found_short };
                let ev_content = if f.evidence.is_empty() { t.no_snippet } else { &f.evidence };
                let ev_lbl = t.quotation_label_html(ev_status);
                o.push_str(&format!("<p class=\"ev-label\">{}</p>\n<pre class=\"evidence\">{}</pre>\n", esc(&ev_lbl), esc(ev_content)));
            }
            o.push_str("</div>\n");
        }
        if let Some(p) = &v.prompt {
            let prompt_lbl = t.prompt_sent_label(p.chars().count());
            o.push_str(&format!("<h3>{}</h3>\n<pre class=\"prompt\">{}</pre>\n", esc(&prompt_lbl), esc(p)));
        }
        o.push_str("</section>\n");
    }
    o.push_str("</article>\n");
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::findings::Finding;
    use crate::store::sample_record;

    fn build_ok(d: &Path, o: &Options, v: &str) -> Report {
        build(d, o, "MrCodeReview", v).unwrap()
    }

    #[test]
    fn the_app_name_goes_into_the_summary_and_the_notice_in_every_format() {
        let d = scratch("appname");
        seed(&d);
        let r = build(&d, &opts(), "MrkCodeReview", "0.0.1").unwrap();
        let line = summary_lines(&r).into_iter().find(|(k, _)| k == "出力したアプリ").unwrap();
        assert_eq!(line.1, "MrkCodeReview 0.0.1");
        assert!(notice(&r.app_name).starts_with("この記録は、MrkCodeReview がレビュー"));
        assert!(markdown(&r).contains("MrkCodeReview がレビュー") && !markdown(&r).contains("MrCodeReview"));
        assert!(html(&r).contains("MrkCodeReview がレビュー"));
        let _ = std::fs::remove_dir_all(&d);
    }

    const TZ: i32 = 540; // 日本
    const T1: u64 = 1_793_000_000_000; // 2026-10-26 07:33:20 UTC（日本では 16:33）
    const T2: u64 = T1 + 86_400_000; // 翌日

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mrcr-report-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn finding(title: &str, detail: &str, evidence: &str, verified: bool) -> Finding {
        Finding { file: "src/lib.rs".into(), line: Some(120), severity: "high".into(), title: title.into(), detail: detail.into(), evidence: evidence.into(), verified, rule: None }
    }

    /// A（3 件の指摘）→ B（A の 1 件目が「解消」と分かる。トークン報告なし）→ C（失敗）。別のリポジトリの D。
    fn seed(dir: &Path) {
        let mut a = sample_record("A", T1);
        a.repo = "/r".into();
        a.recipe.title = "潜在バグ".into();
        a.provider.label = "claude -p".into();
        a.model = "claude-x".into();
        a.usage = Usage { input_tokens: Some(1500), output_tokens: Some(320), cache_read_tokens: Some(50), cache_write_tokens: None };
        a.snapshot.head = "abc1234def5678".into();
        a.snapshot.base = crate::snapshot::Base { spec: "HEAD~1".into(), hash: "9999999aaaaaaa".into(), subject: "前のコミット".into() };
        a.snapshot.files = vec![crate::store::FileRef { path: "src/lib.rs".into(), bytes: 100 }];
        a.prompt = "送った文章の全文".into();
        a.findings = vec![
            finding("失敗を握りつぶしている", "Err を無視します", "let _ = store::append();", true),
            finding("境界値のずれ", "range の上限が多い", "let s = \"```\"; for i in 0..=n {", false),
            finding("<script>alert(1)</script>", "a | b * _c_ [x] # y", "=SUM(A1)", true),
            finding("=HYPERLINK(\"http://x\")", "古い記録の指摘", "x", true),
        ];
        store::append(dir, &a).unwrap();
        // 指摘 0: 対応 → メモ → 対応済み。1: 無視。2: 未仕分け。3: 仕分けはあるが、履歴が無い（この機能より前の記録）
        triage::set_with(dir, "A", 0, "fix", 4, T1 + 60_000, Some("abc1234def5678")).unwrap();
        notes::set_memo(dir, "A", 0, 4, "境界を直した", T1 + 120_000).unwrap();
        notes::set_done(dir, "A", 0, 4, true, T1 + 180_000, Some("def5678abcdef")).unwrap();
        agent_report::save(dir, "A", &[(0, AgentReport { verdict: agent_report::DID.into(), text: "境界を直した（lib.rs）".into(), reported_at: T1 + 200_000, source: "sess1".into() })]).unwrap();
        triage::set_with(dir, "A", 1, "dismiss", 4, T1 + 90_000, None).unwrap();
        db::open(dir).unwrap().execute("INSERT INTO triage (review_id, idx, state) VALUES ('A', 3, 'wait')", []).unwrap();

        let mut b = sample_record("B", T2);
        b.repo = "/r".into();
        b.recipe.title = "潜在バグ".into();
        b.usage = Usage { input_tokens: None, output_tokens: None, cache_read_tokens: None, cache_write_tokens: None };
        b.fates = vec![crate::carry::Fate { from_review: "A".into(), from_index: 0, title: "失敗を握りつぶしている".into(), file: "src/lib.rs".into(), state_before: "fix".into(), fate: "gone".into() }];
        b.notes = vec!["ESLint: 変更していない行への指摘 1 件は除外しました".into()];
        store::append(dir, &b).unwrap();

        let mut c = sample_record("C", T2 + 1000);
        c.repo = "/r".into();
        c.error = Some("タイムアウト".into());
        store::append(dir, &c).unwrap();

        let mut d = sample_record("D", T2 + 2000);
        d.repo = "/other".into();
        store::append(dir, &d).unwrap();
    }

    fn opts() -> Options {
        Options { repo: Some("/r".into()), tz_offset_min: TZ, ..Options::default() }
    }

    /// 実際のデータベースに、生の SQL で、表を壊す（アプリの通常の操作では、起きない状態を作る）。
    fn break_table(d: &Path, table: &str) {
        let c = rusqlite::Connection::open(d.join("mrcodereview.sqlite3")).unwrap();
        c.execute_batch(&format!("DROP TABLE {table}; CREATE TABLE {table} (x INTEGER);")).unwrap();
    }

    #[test]
    fn a_database_that_cannot_be_read_stops_the_report_instead_of_producing_an_empty_one() {
        // データベースのファイルが、壊れている（中身が、データベースではない）
        let d = scratch("unreadable-file");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("mrcodereview.sqlite3"), b"this is not a sqlite database, just some bytes").unwrap();
        let e = build(&d, &opts(), "MrCodeReview", "0.0.1").unwrap_err();
        assert!(e.contains("レポートの元になる記録を読み取れませんでした。出力を止めました"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn failing_to_read_triage_notes_or_the_log_stops_the_report_with_the_reason_not_a_misleading_content() {
        for (table, want) in [("finding_note", "対応メモ・対応済みの印を読み取れません"), ("triage", "仕分けを読み取れません"), ("triage_log", "出力を止めました")] {
            let d = scratch(&format!("broken-{table}"));
            seed(&d);
            break_table(&d, table);
            let e = build(&d, &opts(), "MrCodeReview", "0.0.1").unwrap_err();
            assert!(e.contains("出力を止めました") && e.contains(want), "{table}: {e}");
            // 空のレポートや、「記録なし」だらけのレポートには、ならない（Ok が返らない）
            assert!(build(&d, &Options { repo: None, ..opts() }, "MrCodeReview", "0.0.1").is_err(), "{table}");
            let _ = std::fs::remove_dir_all(&d);
        }
    }

    #[test]
    fn records_that_cannot_be_parsed_are_skipped_but_counted_and_said_in_the_output() {
        let d = scratch("unparsable");
        seed(&d);
        db::open(&d)
            .unwrap()
            .execute(
                "INSERT INTO reviews (id, created_at, repo, recipe_id, recipe_title, provider_id, provider_label, model, ok, findings_count, verified_count, duration_ms, record) \
                 VALUES ('BAD', 5, '/r', 'x', 'x', 'p', 'p', 'm', 1, 0, 0, 1, '{ これは JSON ではない')",
                [],
            )
            .unwrap();
        let r = build(&d, &opts(), "MrCodeReview", "0.0.1").unwrap();
        assert_eq!(r.unreadable, 1);
        assert_eq!(r.reviews.len(), 3, "読めた記録は、そのまま出る");
        let note = "壊れていて読めなかった記録が 1 件あり、この出力に含まれていません";
        assert!(markdown(&r).contains(note), "{}", markdown(&r));
        assert!(html(&r).contains(note));
        // 壊れた記録が無ければ、この注意は出ない
        let ok = scratch("parsable");
        seed(&ok);
        let r2 = build(&ok, &opts(), "MrCodeReview", "0.0.1").unwrap();
        assert_eq!(r2.unreadable, 0);
        assert!(!markdown(&r2).contains("壊れていて読めなかった"));
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&ok);
    }

    #[test]
    fn selected_reviews_are_exactly_those_ignoring_repo_and_period_and_missing_ones_are_said() {
        let d = scratch("select");
        seed(&d);
        // リポジトリ（/r）と期間（B だけ）の指定があっても、選んだものが優先される。別のリポジトリの D も選べる
        let o = Options { since_ms: Some(T2), until_ms: Some(T2), review_ids: Some(vec!["D".into(), "A".into(), "nope".into(), "A".into(), "zzz".into()]), ..opts() };
        let r = build_ok(&d, &o, "0.0.1");
        assert_eq!(r.reviews.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["A", "D"], "選んだものだけ。重ねて渡しても 1 件。実行した順");
        assert_eq!(r.reviews.iter().map(|v| v.n).collect::<Vec<_>>(), [1, 2]);
        assert!(r.selected);
        assert_eq!(r.missing_ids, ["nope", "zzz"], "見つからなかったものは、黙って飛ばさず、言う");
        assert_eq!(r.summary.reviews, 2);
        // 選ばなかった B のレビューで分かる「その後」も、A の指摘に付く
        assert_eq!(r.reviews[0].findings[0].later.as_ref().map(|l| (l.review_id.as_str(), l.fate.as_str())), Some(("B", "gone")));
        // 出力: 概要に、選んだこと・リポジトリ・見つからなかったことが出る
        let m = markdown(&r);
        assert!(m.contains("| 期間 | 選んだレビュー（2026-10-26 16:33 〜 2026-10-27 16:33） |"), "{m}");
        assert!(m.contains("| リポジトリ | /r、/other |"), "{m}");
        assert!(m.contains("選んだレビューのうち 2 件は、履歴に見つかりませんでした（削除された可能性があります）: nope、zzz"), "{m}");
        assert!(html(&r).contains("履歴に見つかりませんでした"));
        // 見つかったものが無ければ、0 件。選んだ状態のまま
        let none = build_ok(&d, &Options { review_ids: Some(vec!["nope".into()]), ..opts() }, "0.0.1");
        assert!(none.reviews.is_empty() && none.selected && none.missing_ids == ["nope"]);
        assert!(markdown(&none).contains("記録されたレビューはありません"));
        let empty = build_ok(&d, &Options { review_ids: Some(vec![]), ..opts() }, "0.0.1");
        assert!(empty.reviews.is_empty() && empty.missing_ids.is_empty(), "何も選んでいなければ、何も出さない（全件にはしない）");
        // 選んでいなければ、従来どおり
        let normal = build_ok(&d, &opts(), "0.0.1");
        assert!(!normal.selected && normal.missing_ids.is_empty() && normal.reviews.len() == 3);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn dates_are_formatted_in_the_given_offset_including_edges() {
        assert_eq!(fmt_datetime(0, 0), "1970-01-01 00:00");
        assert_eq!(fmt_datetime(1_700_000_000_000, 0), "2023-11-14 22:13");
        assert_eq!(fmt_datetime(1_700_000_000_000, 540), "2023-11-15 07:13", "日をまたぐ");
        assert_eq!(fmt_datetime(1_700_000_000_000, -300), "2023-11-14 17:13");
        assert_eq!(fmt_datetime(951_782_400_000, 0), "2000-02-29 00:00", "うるう日");
        assert_eq!(fmt_datetime(1_709_251_199_000, 0), "2024-02-29 23:59");
        assert_eq!(fmt_datetime(T1, TZ), "2026-10-26 16:33");
        assert_eq!((fmt_offset(540).as_str(), fmt_offset(-330).as_str(), fmt_offset(0).as_str()), ("+09:00", "-05:30", "+00:00"));
        assert_eq!((fmt_num(0).as_str(), fmt_num(999).as_str(), fmt_num(1000).as_str(), fmt_num(1_234_567).as_str()), ("0", "999", "1,000", "1,234,567"));
        assert_eq!((fmt_secs(450).as_str(), fmt_secs(8400).as_str()), ("450 ミリ秒", "8.4 秒"));
    }

    #[test]
    fn build_filters_by_repo_and_period_and_counts_only_what_was_recorded() {
        let d = scratch("build");
        seed(&d);
        let r = build_ok(&d, &opts(), "0.0.1");
        assert_eq!(r.reviews.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["A", "B", "C"], "このリポジトリだけ。古い順");
        assert_eq!(r.reviews.iter().map(|v| v.n).collect::<Vec<_>>(), [1, 2, 3]);
        let s = &r.summary;
        assert_eq!((s.reviews, s.failed, s.findings, s.fix, s.wait, s.dismiss, s.new, s.done), (3, 1, 4, 1, 1, 1, 1, 1));
        assert_eq!((s.input, s.output, s.cache_read, s.unreported), (1500, 320, 50, 1), "報告の無い B は 0 として足さず、件数で言う。失敗の C は足さない");
        // 期間
        let only_b = build_ok(&d, &Options { since_ms: Some(T2), until_ms: Some(T2), ..opts() }, "0.0.1");
        assert_eq!(only_b.reviews.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["B"], "境界を含む");
        assert!(build_ok(&d, &Options { since_ms: Some(T2 + 10_000_000), ..opts() }, "0.0.1").reviews.is_empty());
        // すべてのリポジトリ
        assert_eq!(build_ok(&d, &Options { repo: None, ..opts() }, "0.0.1").reviews.len(), 4);
        // その後: A の 0 番は、B のレビューで「解消」
        let a0 = &r.reviews[0].findings[0];
        assert_eq!(a0.later.as_ref().map(|l| (l.review_id.as_str(), l.at, l.fate.as_str())), Some(("B", T2, "gone")));
        assert!(r.reviews[0].findings[1].later.is_none(), "その後が分からないものは、作らない");
        // 期間を B だけにしても、「その後」は、A を含めずに出さない（B の節に、前回からの変化として出る）
        assert_eq!(only_b.reviews[0].fates.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_timeline_orders_events_and_says_no_record_instead_of_inventing_times() {
        let d = scratch("timeline");
        seed(&d);
        let r = build_ok(&d, &opts(), "0.0.1");
        let tl0 = timeline(&r.reviews[0].findings[0], TZ);
        assert_eq!(tl0.len(), 3);
        assert!(tl0[0].starts_with("2026-10-26 16:34 — 対応（あなたの操作・HEAD abc1234）"), "{}", tl0[0]);
        assert!(tl0[1].starts_with("2026-10-26 16:35 — 対応メモを書いた"), "{}", tl0[1]);
        assert!(tl0[2].starts_with("2026-10-26 16:36 — 対応済みにした（あなたの操作・HEAD def5678）"), "{}", tl0[2]);
        assert_eq!(timeline(&r.reviews[0].findings[2], TZ), Vec::<String>::new(), "未仕分けは、何も無い");
        assert_eq!(timeline(&r.reviews[0].findings[3], TZ), ["仕分けた時刻: 記録なし（この機能より前の記録）"], "履歴が無い仕分けは、時刻を作らない");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn markdown_has_the_summary_per_review_sections_and_the_handling_record() {
        let d = scratch("md");
        seed(&d);
        let m = markdown(&build_ok(&d, &opts(), "0.0.1"));
        for want in [
            "# レビューの記録",
            "| レビュー | 3 件（失敗 1 件） |",
            "| 指摘 | 4 件 — 対応 1（うち対応済み 1）・待ち 1・無視 1・未仕分け 1 |",
            "入力 1,500・出力 320",
            "報告の無かったレビュー 1 件は、含めていません",
            "## 1. 潜在バグ — 2026-10-26 16:33",
            "## 2. 潜在バグ — 2026-10-27 16:33",
            "入力 1,500・出力 320・キャッシュ読み取り 50（提供元が報告した値だけ",
            "#### 1-1. [high] 失敗を握りつぶしている",
            "- **現在の扱い**: **対応**・対応済み",
            "  - 2026-10-26 16:34 — 対応（あなたの操作・HEAD abc1234）",
            "- **対応メモ**: 境界を直した",
            "- **その後**: 2026-10-27 16:33 のレビューで「解消（引用した行が、ファイルから消えた。推定）」",
            "- **エージェントの申告**: 対応した（エージェントの申告・2026-10-26 16:36。実際に直ったかの確認ではありません）: 境界を直した（lib.rs）",
            "- **現在の扱い**: **無視**",
            "- **現在の扱い**: **未仕分け**",
            "仕分けた時刻: 記録なし（この機能より前の記録）",
            "### 前回のレビューからの変化",
            "- **失敗の理由**: タイムアウト",
            "- **トークン**: 記録なし（失敗した実行）",
            "- **トークン**: 報告なし（提供元が、値を返しませんでした）",
            "- **トークン**: 入力 1,500・出力 320（提供元が報告した値）",
            "- **比較元**: 9999999 前のコミット から（いまの作業ツリーまでの変更）",
            "識別子 `mrcr:A`",
            "`mrcr:A#1`",
            "- **実行メモ**: ESLint: 変更していない行への指摘 1 件は除外しました",
        ] {
            assert!(m.contains(want), "「{want}」が無い:\n{m}");
        }
        assert!(!m.contains("書き込み 0"), "報告されなかったキャッシュ書き込みを、0 と書かない");
        assert!(!m.contains("入力 5・出力 7"), "失敗した実行の値は、数えず、出さない");
        assert!(m.contains("| 3 | 2026-10-27 16:33 | セキュリティ | m | 記録なし | 失敗 | 0 | 0 | 0 | 0 | 0 | 0 | — |"), "失敗の行は、トークンが —。比較元が無い記録は、「記録なし」:\n{m}");
        assert!(!m.contains("送った文章の全文"), "既定では、送った文章の全文を含めない");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn markdown_escapes_markup_in_llm_text_and_picks_a_fence_longer_than_the_quote() {
        let d = scratch("mdesc");
        seed(&d);
        let m = markdown(&build_ok(&d, &opts(), "0.0.1"));
        assert!(m.contains("\\<script\\>alert(1)\\</script\\>"), "タイトルの < > は、文字として出す:\n{m}");
        assert!(!m.contains("<script>"), "生の <script> は出さない");
        assert!(m.contains("a \\| b \\* \\_c\\_ \\[x\\] \\# y"), "理由の記号は、文字として出す");
        // 引用に ``` が入っているので、囲みは 4 つ以上
        assert!(m.contains("````\nlet s = \"```\"; for i in 0..=n {\n````"), "{m}");
        assert!(m.contains("引用（差分に見つからない）"), "根拠不一致の引用は、そう書く");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn options_control_evidence_prompt_tokens_and_log() {
        let d = scratch("opts");
        seed(&d);
        let none = markdown(&build_ok(&d, &Options { include_evidence: false, include_tokens: false, include_log: false, ..opts() }, "0.0.1"));
        assert!(!none.contains("let _ = store::append();") && !none.contains("引用（"), "引用を含めない");
        assert!(!none.contains("入力 1,500") && !none.contains("トークン"), "トークンを含めない: {none}");
        assert!(!none.contains("- **経過**:"), "経過を含めない");
        assert!(none.contains("- **最後に仕分けた時刻**: 2026-10-26 16:34"), "経過を含めなくても、最後の仕分けの時刻は残す");
        let with_prompt = markdown(&build_ok(&d, &Options { include_prompt: true, ..opts() }, "0.0.1"));
        assert!(with_prompt.contains("### 送った文章の全文") && with_prompt.contains("送った文章の全文\n```"), "{with_prompt}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn html_escapes_everything_that_comes_from_the_records() {
        let d = scratch("html");
        seed(&d);
        let h = html(&build_ok(&d, &opts(), "0.0.1"));
        assert!(h.starts_with("<article class=\"report\">"));
        assert!(h.contains("&lt;script&gt;alert(1)&lt;/script&gt;"), "タイトル");
        assert!(!h.contains("<script"), "生の script タグを出さない");
        assert!(h.contains("=SUM(A1)") && h.contains("&quot;"), "引用の文字は、そのまま（エスケープ済み）で出る");
        for want in ["現在の扱い</b>: <strong>対応</strong>・対応済み", "対応メモ</b>: 境界を直した", "エージェントの申告</b>: 対応した（エージェントの申告・", "仕分けた時刻: 記録なし", "入力 1,500・出力 320"] {
            assert!(h.contains(want), "「{want}」が無い");
        }
        assert_eq!(esc("<a href=\"x\">'&'</a>"), "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;");
        // 空の期間
        let empty = html(&build_ok(&d, &Options { since_ms: Some(u64::MAX / 2), ..opts() }, "0.0.1"));
        assert!(empty.contains("記録されたレビューはありません"));
        assert!(markdown(&build_ok(&d, &Options { since_ms: Some(u64::MAX / 2), ..opts() }, "0.0.1")).contains("記録されたレビューはありません"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn xlsx_has_four_sheets_with_values_as_text_and_no_formulas() {
        use calamine::{open_workbook, Data, Reader, Xlsx};
        let d = scratch("xlsx");
        seed(&d);
        let out = d.join("out.xlsx");
        std::fs::create_dir_all(&d).unwrap();
        crate::report_xlsx::write(&build_ok(&d, &opts(), "0.0.1"), &out).unwrap();
        assert!(std::fs::read(&out).unwrap().starts_with(b"PK"), "xlsx は zip");
        let mut wb: Xlsx<_> = open_workbook(&out).unwrap();
        assert_eq!(wb.sheet_names(), ["概要", "レビュー", "指摘", "仕分けの履歴"]);
        let cell = |r: &calamine::Range<Data>, row: u32, col: u32| -> String {
            match r.get_value((row, col)) {
                Some(Data::String(s)) => s.clone(),
                Some(Data::Float(f)) => format!("{f}"),
                Some(Data::Int(i)) => format!("{i}"),
                Some(other) => format!("{other:?}"),
                None => String::new(),
            }
        };
        let sum = wb.worksheet_range("概要").unwrap();
        assert_eq!((cell(&sum, 0, 0).as_str(), cell(&sum, 0, 1).as_str()), ("項目", "内容"));
        assert!((0..12).any(|r| cell(&sum, r, 0) == "レビュー" && cell(&sum, r, 1) == "3 件（失敗 1 件）"));

        let rv = wb.worksheet_range("レビュー").unwrap();
        let head: Vec<String> = (0..rv.width() as u32).map(|c| cell(&rv, 0, c)).collect();
        assert!(head.contains(&"入力トークン".to_string()));
        let col = |name: &str| head.iter().position(|h| h == name).unwrap() as u32;
        assert_eq!(rv.height(), 4, "見出し + 3 回");
        assert_eq!((cell(&rv, 1, col("観点")).as_str(), cell(&rv, 1, col("入力トークン")).as_str(), cell(&rv, 1, col("指摘")).as_str()), ("潜在バグ", "1500", "4"));
        assert_eq!(cell(&rv, 2, col("入力トークン")), "報告なし", "報告の無い値は、0 ではなく「報告なし」");
        assert!(cell(&rv, 3, col("実行メモ・失敗の理由")).contains("失敗: タイムアウト"));

        let fd = wb.worksheet_range("指摘").unwrap();
        let fhead: Vec<String> = (0..fd.width() as u32).map(|c| cell(&fd, 0, c)).collect();
        let fcol = |name: &str| fhead.iter().position(|h| h == name).unwrap() as u32;
        assert_eq!(fd.height(), 5, "見出し + 指摘 4 件");
        assert_eq!(cell(&fd, 1, fcol("現在の扱い")), "対応");
        assert_eq!((cell(&fd, 1, fcol("対応済み")).as_str(), cell(&fd, 1, fcol("対応メモ")).as_str()), ("済", "境界を直した"));
        assert_eq!(cell(&fd, 1, fcol("最後に仕分けた日時")), "2026-10-26 16:34");
        assert!(cell(&fd, 1, fcol("その後（次のレビューでの変化）")).contains("解消"));
        let agent_col = "エージェントの申告（ログから。実際に直ったかの確認ではない）";
        assert!(cell(&fd, 1, fcol(agent_col)).starts_with("対応した（エージェントの申告・2026-10-26 16:36"), "{}", cell(&fd, 1, fcol(agent_col)));
        assert!(!cell(&fd, 2, fcol(agent_col)).contains("対応"), "取り込んでいない指摘は、空（作らない）: {}", cell(&fd, 2, fcol(agent_col)));
        assert_eq!(cell(&fd, 4, fcol("最後に仕分けた日時")), "記録なし", "履歴が無い仕分けは、時刻を作らない");
        assert_eq!(cell(&fd, 3, fcol("現在の扱い")), "未仕分け");
        assert_eq!(cell(&fd, 4, fcol("タイトル")), "=HYPERLINK(\"http://x\")", "= で始まる文章も、数式にならず、文字のまま");
        assert_eq!(cell(&fd, 3, fcol("引用")), "=SUM(A1)");
        assert!(matches!(fd.get_value((3, fcol("タイトル"))), Some(Data::String(_))));

        let lg = wb.worksheet_range("仕分けの履歴").unwrap();
        assert_eq!(lg.height(), 3, "見出し + 仕分けの変更 2 回（A の 0 番の対応、1 番の無視）。メモと対応済みは、別の記録。3 番は、履歴なし");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 目で確かめるための、サンプルの書き出し。`MRCR_SAMPLE_DIR=<フォルダ> cargo test dump_sample_report -- --ignored`
    #[test]
    #[ignore]
    fn dump_sample_report() {
        let Some(out) = std::env::var_os("MRCR_SAMPLE_DIR") else {
            eprintln!("MRCR_SAMPLE_DIR を指定してください");
            return;
        };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        let d = scratch("dump");
        seed(&d);
        let r = build_ok(&d, &Options { include_prompt: true, ..opts() }, "0.0.1");
        std::fs::write(out.join("sample.md"), markdown(&r)).unwrap();
        std::fs::write(out.join("sample.html"), html(&r)).unwrap();
        crate::report_xlsx::write(&r, &out.join("sample.xlsx")).unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn xlsx_options_drop_columns_and_the_log_sheet() {
        use calamine::{open_workbook, Reader, Xlsx};
        let d = scratch("xlsx-opts");
        seed(&d);
        std::fs::create_dir_all(&d).unwrap();
        let out = d.join("o.xlsx");
        crate::report_xlsx::write(&build_ok(&d, &Options { include_evidence: false, include_tokens: false, include_log: false, ..opts() }, "0.0.1"), &out).unwrap();
        let mut wb: Xlsx<_> = open_workbook(&out).unwrap();
        assert_eq!(wb.sheet_names(), ["概要", "レビュー", "指摘"], "経過を含めなければ、履歴のシートは無い");
        let fd = wb.worksheet_range("指摘").unwrap();
        let heads: Vec<String> = fd.rows().next().unwrap().iter().map(|c| c.to_string()).collect();
        assert!(!heads.contains(&"引用".to_string()));
        let rv = wb.worksheet_range("レビュー").unwrap();
        assert!(!rv.rows().next().unwrap().iter().any(|c| c.to_string().contains("トークン")));
        // 空の期間でも、壊れたファイルにならない
        let empty = d.join("e.xlsx");
        crate::report_xlsx::write(&build_ok(&d, &Options { since_ms: Some(u64::MAX / 2), ..opts() }, "0.0.1"), &empty).unwrap();
        let mut wb: Xlsx<_> = open_workbook(&empty).unwrap();
        assert_eq!(wb.worksheet_range("指摘").unwrap().height(), 1, "見出しだけ");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn english_report_outputs_english_in_markdown_html_and_xlsx_and_none_falls_back_to_ja() {
        use calamine::{open_workbook, Data, Reader, Xlsx};
        let d = scratch("lang-en");
        seed(&d);
        std::fs::create_dir_all(&d).unwrap();

        // lang: None は従来どおり日本語（"ja"）
        let r_ja = build_ok(&d, &Options { lang: None, ..opts() }, "0.0.1");
        assert_eq!(r_ja.lang, "ja");
        assert!(markdown(&r_ja).contains("# レビューの記録"));
        assert!(html(&r_ja).contains("<h1>レビューの記録</h1>"));

        // lang: Some("en") は Markdown / HTML / xlsx すべて英語見出し・ラベルで出力される
        let r_en = build_ok(&d, &Options { lang: Some("en".into()), include_prompt: true, ..opts() }, "0.0.1");
        assert_eq!(r_en.lang, "en");
        let m = markdown(&r_en);
        for want in [
            "# Review Report",
            "| Item | Details |",
            "| Reviews | 3 review(s) (1 failed) |",
            "| Findings | 4 finding(s) — Fix: 1 (Done: 1), Wait: 1, Dismiss: 1, New: 1 |",
            "## Review List",
            "| # | Date/Time | Angle | LLM | Range | Result | Findings | Fix | Wait | Dismiss | New | Done | Tokens |",
            "- **Triage status**: **Fix** · Done",
            "  - 2026-10-26 16:34 — Fix (manual action, HEAD abc1234)",
            "Triaged time: No record (prior to tracking)",
            "### Changes from previous review",
            "### Findings (4 finding(s))",
            "### Prompt sent to LLM",
            "Input 1,500 · Output 320",
        ] {
            assert!(m.contains(want), "英語 Markdown に「{want}」が無い:\n{m}");
        }
        for unwanted in ["# レビューの記録", "## 一覧", "### 前回のレビューからの変化", "### 指摘（", "現在の扱い", "仕分けた時刻"] {
            assert!(!m.contains(unwanted), "英語 Markdown に日本語見出し「{unwanted}」が混ざっている:\n{m}");
        }

        let h = html(&r_en);
        for want in [
            "<h1>Review Report</h1>",
            "<h2>Review List</h2>",
            "<th>Fix</th><th>Wait</th><th>Dismiss</th><th>New</th><th>Done</th>",
            "Triage status</b>: <strong>Fix</strong> · Done",
            "Triaged time: No record (prior to tracking)",
            "<h3>Changes from previous review</h3>",
            "<h3>Findings (4 finding(s))</h3>",
        ] {
            assert!(h.contains(want), "英語 HTML に「{want}」が無い:\n{h}");
        }
        for unwanted in ["<h1>レビューの記録</h1>", "<h2>一覧</h2>", "現在の扱い", "仕分けた時刻"] {
            assert!(!h.contains(unwanted), "英語 HTML に日本語見出し「{unwanted}」が混ざっている:\n{h}");
        }

        let out_xlsx = d.join("en.xlsx");
        crate::report_xlsx::write(&r_en, &out_xlsx).unwrap();
        let mut wb: Xlsx<_> = open_workbook(&out_xlsx).unwrap();
        assert_eq!(wb.sheet_names(), ["Summary", "Reviews", "Findings", "Triage History"]);
        let sum = wb.worksheet_range("Summary").unwrap();
        assert_eq!(sum.get_value((0, 0)), Some(&Data::String("Item".into())));
        assert_eq!(sum.get_value((0, 1)), Some(&Data::String("Details".into())));
        let fd = wb.worksheet_range("Findings").unwrap();
        let fhead: Vec<String> = fd.rows().next().unwrap().iter().map(|c| c.to_string()).collect();
        assert!(fhead.contains(&"Triage Status".to_string()) && fhead.contains(&"Done".to_string()));
        let status_col = fhead.iter().position(|c| c == "Triage Status").unwrap() as u32;
        assert_eq!(fd.get_value((1, status_col)), Some(&Data::String("Fix".into())));
        assert_eq!(fd.get_value((3, status_col)), Some(&Data::String("New".into())));

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn recipe_title_lang_and_base_label_match_builtin_recipes_in_both_directions() {
        let ja_recipes = crate::recipe::list_with_lang(None, "ja");
        let en_recipes = crate::recipe::list_with_lang(None, "en");
        assert_eq!(ja_recipes.len(), 14);
        assert_eq!(en_recipes.len(), 14);
        for (ja, en) in ja_recipes.iter().zip(en_recipes.iter()) {
            assert_eq!(ja.id, en.id);
            assert_eq!(
                recipe_title_for(&ja.title, crate::i18n::Lang::En),
                en.title,
                "組み込みレシピ `{}` の日本語タイトル `{}` が英語タイトル `{}` に変換されない",
                ja.id,
                ja.title,
                en.title
            );
            assert_eq!(
                recipe_title_for(&en.title, crate::i18n::Lang::Ja),
                ja.title,
                "組み込みレシピ `{}` の英語タイトル `{}` が日本語タイトル `{}` に変換されない",
                en.id,
                en.title,
                ja.title
            );
        }

        assert_eq!(base_label_for("未コミットの変更", crate::i18n::Lang::En), "Uncommitted changes");
        assert_eq!(base_label_for("直前のコミット", crate::i18n::Lang::En), "Latest commit");
        assert_eq!(base_label_for("最初から（リポジトリ全体）", crate::i18n::Lang::En), "Entire repository (from start)");
        assert_eq!(base_label_for("Uncommitted changes", crate::i18n::Lang::Ja), "未コミットの変更");
        assert_eq!(base_label_for("Latest commit", crate::i18n::Lang::Ja), "直前のコミット");
        assert_eq!(base_label_for("Entire repository (from start)", crate::i18n::Lang::Ja), "最初から（リポジトリ全体）");

        let d = scratch("base-label-lang");
        let mut r_head = sample_record("H", T1);
        r_head.repo = "/r".into();
        r_head.snapshot.base = crate::snapshot::Base { spec: "HEAD".into(), hash: "1111111".into(), subject: "s".into() };
        store::append(&d, &r_head).unwrap();
        let mut r_root = sample_record("R", T1 + 1000);
        r_root.repo = "/r".into();
        r_root.snapshot.base = crate::snapshot::Base { spec: ":root".into(), hash: "2222222".into(), subject: "s".into() };
        store::append(&d, &r_root).unwrap();
        let mut r_commit = sample_record("M", T1 + 2000);
        r_commit.repo = "/r".into();
        r_commit.snapshot.base = crate::snapshot::Base { spec: "HEAD~1".into(), hash: "abcdef1234567".into(), subject: "init".into() };
        store::append(&d, &r_commit).unwrap();

        let rep_en = build_ok(&d, &Options { lang: Some("en".into()), ..opts() }, "0.0.1");
        assert_eq!(rep_en.reviews[0].base_label, "Uncommitted changes");
        assert_eq!(rep_en.reviews[1].base_label, "Entire repository (from start)");
        assert_eq!(rep_en.reviews[2].base_label, "From abcdef1 init");

        let rep_ja = build_ok(&d, &Options { lang: Some("ja".into()), ..opts() }, "0.0.1");
        assert_eq!(rep_ja.reviews[0].base_label, "未コミットの変更");
        assert_eq!(rep_ja.reviews[1].base_label, "最初から（リポジトリ全体）");
        assert_eq!(rep_ja.reviews[2].base_label, "abcdef1 init から");
        let _ = std::fs::remove_dir_all(&d);
    }
}
