//! エージェントの「対応の報告」を、会話の記録（Claude Code と agy のログ）から取り込む。
//!
//! 「CLI に渡す」文面の末部で、エージェントに、指摘ごとに 1 行ずつ、こう書かせている:
//!   `mrcr:<レビュー>#<番号>（<観点>） 対応した: <何をしたか>`   または   `... 対応しなかった: <理由>`
//! 識別子のあとの括弧（観点の名前）は、あってもなくても読める。アプリを開き直すと画面は初期化されるので、人が見て、どの観点のレビューの報告か分かるように、添えている。
//! ここでは、ログの中から、その行だけを探して取り出す。
//!
//! - 取れるのは、エージェントの**申告**。事実ではない（事実は git の差分）。画面でも、別の欄に出す。
//! - 読むのは、識別子を含む行だけ。会話の本文は、保存しない。保存するのは、その 1 行の文章と、ログの名前・時刻だけ。
//! - Claude Code: `~/.claude/projects/<プロジェクト>/<セッション>.jsonl`（1 行 1 JSON）。
//!   agy: `~/.gemini/antigravity-cli/conversations/<会話>.db`（SQLite の `steps` 表。回答は `step_type = 15`、
//!   本文と時刻は protobuf。形式は SyncLane の取り込みと同じ）。
//! - 渡した文面のひな形（`<何をしたか>`）を、エージェントが読むと、ログの `tool_result` に残る。
//!   **エージェントの返事（`assistant` の `text`）だけ**を対象にし、ひな形のままの行は除く。
//! - 書かなかった指摘・書式が違う指摘は、取れない。「記録なし」にする（推定で埋めない）。
//! - 読み取れないものは、黙って空にせず、件数で言う（`unreadable_lines`）。ログの場所が無いときは、エラー。

use crate::db;
use regex::Regex;
use rusqlite::params;
use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub const DID: &str = "did";
pub const SKIPPED: &str = "skipped";
/// 許可の印（meta）。初めて、ログを読むときに、1 回だけ、使う人に聞く。
/// （Copilot CLI のログを読む場所が増えたとき、`agent_log_consent` から名前を変えた。前の許可は引き継がず、新しい文言で、もう一度聞く。）
const CONSENT_KEY: &str = "agent_log_consent_v2";
/// 保存する文章の上限（文字数）。
const TEXT_MAX: usize = 2000;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentReport {
    /// "did"（対応した）| "skipped"（対応しなかった）
    pub verdict: String,
    pub text: String,
    /// 報告をログに書いた時刻（UNIX ミリ秒）
    pub reported_at: u64,
    /// 取り出したログの名前（セッションの ID）
    pub source: String,
}

/// 会話の記録の置き場所。開発や試験では、環境変数（`MRCR_CLAUDE_PROJECTS`・`MRCR_ANTIGRAVITY_DIR`・`MRCR_COPILOT_DIR`）で、別の場所に向けられる。
#[derive(Debug, Clone)]
pub struct Sources {
    /// Claude Code: `~/.claude/projects`
    pub claude: PathBuf,
    /// agy: `~/.gemini/antigravity-cli`（この下の `conversations/*.db`）
    pub agy: PathBuf,
    /// GitHub Copilot CLI: `~/.copilot`（この下の `session-state/<セッション>/events.jsonl`）。`COPILOT_HOME` で、置き場を変えている人にも合わせる
    pub copilot: PathBuf,
}

impl Sources {
    pub fn from_env() -> Sources {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        Sources {
            claude: std::env::var_os("MRCR_CLAUDE_PROJECTS").map(PathBuf::from).unwrap_or_else(|| home.join(".claude").join("projects")),
            agy: std::env::var_os("MRCR_ANTIGRAVITY_DIR").map(PathBuf::from).unwrap_or_else(|| home.join(".gemini").join("antigravity-cli")),
            copilot: std::env::var_os("MRCR_COPILOT_DIR")
                .or_else(|| std::env::var_os("COPILOT_HOME"))
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".copilot")),
        }
    }
}

#[derive(Debug, Default)]
pub struct Scan {
    /// (指摘の番号 0 始まり, 報告)。同じ指摘は、一番新しい報告 1 件。
    pub reports: Vec<(usize, AgentReport)>,
    /// 調べたログの数（`not_before_ms` より新しいもの。Claude Code と agy の合計）
    pub files_scanned: usize,
    /// そのうち、レビューの識別子を含んでいたログの数
    pub files_with_tag: usize,
    /// 識別子を含むが、読めなかった行・ログの数（壊れた JSON・開けない DB・時刻なし）
    pub unreadable_lines: usize,
    /// 一度に調べるログの上限（Claude Code・agy それぞれ）を超えたため、**調べていない**、古いログの数。
    /// 「見つからなかった」と「調べていない」を、取り違えないために、数えて、画面で言う
    pub skipped_files: usize,
    /// 書式は合っていたが、指摘の番号が、このレビューに無い報告の数
    pub ignored: usize,
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    let Some(&first) = needle.first() else { return false };
    if hay.len() < needle.len() {
        return false;
    }
    let max = hay.len() - needle.len();
    let mut offset = 0;
    while offset <= max {
        if let Some(pos) = hay[offset..=max].iter().position(|&b| b == first) {
            let start = offset + pos;
            if &hay[start..start + needle.len()] == needle {
                return true;
            }
            offset = start + 1;
        } else {
            break;
        }
    }
    false
}

/// `2026-10-27T04:13:42.568Z` → UNIX ミリ秒。ほかの形（タイムゾーン付きなど）は、読まない（None）。
fn parse_iso_ms(s: &str) -> Option<u64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-');
    let (y, m, day): (i64, i64, i64) = (d.next()?.parse().ok()?, d.next()?.parse().ok()?, d.next()?.parse().ok()?);
    let (hms, frac) = time.split_once('.').unwrap_or((time, "0"));
    let mut t = hms.split(':');
    let (hh, mm, ss): (i64, i64, i64) = (t.next()?.parse().ok()?, t.next()?.parse().ok()?, t.next()?.parse().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    let ms: i64 = format!("{:0<3}", frac).get(..3)?.parse().ok()?;
    // 暦日 → 1970-01-01 からの日数（Howard Hinnant の方法）
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let total = ((days * 24 + hh) * 60 + mm) * 60 + ss;
    u64::try_from(total * 1000 + ms).ok()
}

fn modified_ms(p: &Path) -> Option<u64> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64)
}

/// 一度に調べる Claude Code のセッションログファイル数の上限。
/// 更新されたセッションが大量にあっても、最終更新が新しい順に上位この件数だけを見る。
const CLAUDE_MAX_FILES: usize = 30;

/// Claude Code のログの一覧（`<置き場>/<プロジェクト>/<セッション>.jsonl`）。
/// `not_before_ms` より古い更新のものは除き、最終更新が新しいセッションから順に最大 `CLAUDE_MAX_FILES` 件に絞る。
/// 戻り値の 2 つ目は、上限のために調べなかった件数（黙って捨てない）。
fn claude_files(projects: &Path, not_before_ms: u64) -> std::io::Result<(Vec<PathBuf>, usize)> {
    let mut entries = Vec::new();
    for proj in std::fs::read_dir(projects)?.flatten() {
        if !proj.path().is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(proj.path()) else { continue };
        for f in files.flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            // 更新時刻が読めないログは、除かず、調べる（古いと決めつけない）
            let mtime = modified_ms(&p);
            if mtime.is_none_or(|m| m >= not_before_ms) {
                entries.push((mtime.unwrap_or(0), p));
            }
        }
    }
    // 最終更新が新しい順（降順）。同時刻ならパス順
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let skipped = entries.len().saturating_sub(CLAUDE_MAX_FILES);
    entries.truncate(CLAUDE_MAX_FILES);
    Ok((entries.into_iter().map(|(_, p)| p).collect(), skipped))
}

/// 一度に調べる agy の会話ファイル数の上限。
/// レビュー作成後に更新された会話が大量にあっても、最終更新が新しい順に上位この件数だけを見る。
const AGY_MAX_CONVERSATIONS: usize = 30;

/// agy の会話の一覧（`<置き場>/conversations/*.db`）。書き込みは `-wal` に入ることがあるので、両方のうち新しい更新を見る。
/// 最終更新が新しい会話から順に見る。上限 `AGY_MAX_CONVERSATIONS` 件までに絞る。
/// 戻り値の 2 つ目は、上限のために調べなかった件数（黙って捨てない）。
fn agy_files(root: &Path, not_before_ms: u64) -> std::io::Result<(Vec<PathBuf>, usize)> {
    let mut entries = Vec::new();
    for f in std::fs::read_dir(root.join("conversations"))?.flatten() {
        let p = f.path();
        if p.extension().and_then(|e| e.to_str()) != Some("db") {
            continue;
        }
        let mut wal = p.as_os_str().to_owned();
        wal.push("-wal");
        let newest = modified_ms(&p).into_iter().chain(modified_ms(Path::new(&wal))).max();
        if newest.is_none_or(|m| m >= not_before_ms) {
            entries.push((newest.unwrap_or(0), p));
        }
    }
    // 最終更新が新しい順（降順）。同時刻ならファイル名の昇順
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let skipped = entries.len().saturating_sub(AGY_MAX_CONVERSATIONS);
    entries.truncate(AGY_MAX_CONVERSATIONS);
    Ok((entries.into_iter().map(|(_, p)| p).collect(), skipped))
}

/// 一度に調べる Copilot CLI のセッションログ数の上限。
const COPILOT_MAX_FILES: usize = 30;

/// Copilot CLI のログの一覧（`<置き場>/session-state/<セッション>/events.jsonl`）。
/// `not_before_ms` より古い更新のものは除き、最終更新が新しいセッションから順に最大 `COPILOT_MAX_FILES` 件に絞る。
/// 戻り値の 2 つ目は、上限のために調べなかった件数（黙って捨てない）。
/// 形式は公式に文書化されていない（実際のログから読んだ）。古い版の `history-session-state/` は、形式を確かめていないので、読まない。
fn copilot_files(root: &Path, not_before_ms: u64) -> std::io::Result<(Vec<PathBuf>, usize)> {
    let mut entries = Vec::new();
    for dir in std::fs::read_dir(root.join("session-state"))?.flatten() {
        let p = dir.path().join("events.jsonl");
        if !p.is_file() {
            continue;
        }
        // 更新時刻が読めないログは、除かず、調べる（古いと決めつけない）
        let mtime = modified_ms(&p);
        if mtime.is_none_or(|m| m >= not_before_ms) {
            entries.push((mtime.unwrap_or(0), p));
        }
    }
    // 最終更新が新しい順（降順）。同時刻ならパス順
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let skipped = entries.len().saturating_sub(COPILOT_MAX_FILES);
    entries.truncate(COPILOT_MAX_FILES);
    Ok((entries.into_iter().map(|(_, p)| p).collect(), skipped))
}

// ── agy の protobuf（SyncLane の取り込みと同じ読み方。必要な分だけ） ──

enum Pb<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed,
}

fn pb_varint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let (mut value, mut shift) = (0u64, 0u32);
    loop {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7f).checked_shl(shift)?;
        if byte < 0x80 {
            return Some(value);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

/// protobuf の 1 階層分を (フィールド番号, 値) に分ける。壊れていれば None。
fn pb_parse(bytes: &[u8]) -> Option<Vec<(u64, Pb<'_>)>> {
    let (mut at, mut out) = (0usize, Vec::new());
    while at < bytes.len() {
        let key = pb_varint(bytes, &mut at)?;
        let value = match key & 7 {
            0 => Pb::Varint(pb_varint(bytes, &mut at)?),
            1 => {
                at = at.checked_add(8)?;
                Pb::Fixed
            }
            2 => {
                let len = usize::try_from(pb_varint(bytes, &mut at)?).ok()?;
                let end = at.checked_add(len)?;
                let slice = bytes.get(at..end)?;
                at = end;
                Pb::Bytes(slice)
            }
            5 => {
                at = at.checked_add(4)?;
                Pb::Fixed
            }
            _ => return None,
        };
        if at > bytes.len() {
            return None;
        }
        out.push((key >> 3, value));
    }
    Some(out)
}

/// `path` のとおりに入れ子をたどって、最後のフィールド（バイト列）を返す。
fn pb_bytes<'a>(bytes: &'a [u8], path: &[u64]) -> Option<&'a [u8]> {
    let mut current = bytes;
    for number in path {
        current = pb_parse(current)?.into_iter().find_map(|(field, value)| match value {
            Pb::Bytes(slice) if field == *number => Some(slice),
            _ => None,
        })?;
    }
    Some(current)
}

fn pb_number(bytes: &[u8], number: u64) -> Option<u64> {
    pb_parse(bytes)?.into_iter().find_map(|(field, value)| match value {
        Pb::Varint(v) if field == number => Some(v),
        _ => None,
    })
}

const AGY_ANSWER_STEP: i64 = 15;
const AGY_STEP_DONE: i64 = 3;
/// 1 つの会話で読む、**識別子を含む回答**の件数の上限（最新のものから数える）。
/// 識別子で絞ってから、最新の N 件だけを読む（回答の全部を読み出さない）。**絞る前の N 件ではない**:
/// 報告のあとに、識別子と関係のない回答が、何十件続いても、報告を見逃さないため。
const AGY_MAX_ANSWERS_PER_CONV: i64 = 30;

/// 1 手の記録（回答）から (本文, 時刻ミリ秒) を取り出す。
fn agy_answer(payload: &[u8]) -> Option<(String, u64)> {
    let text = pb_bytes(payload, &[20, 1]).or_else(|| pb_bytes(payload, &[20, 8]))?;
    let stamp = pb_bytes(payload, &[5, 1])?;
    let seconds = pb_number(stamp, 1)?;
    let nanos = pb_number(stamp, 2).unwrap_or(0);
    Some((std::str::from_utf8(text).ok()?.to_string(), seconds.checked_mul(1000)?.checked_add((nanos / 1_000_000).min(999))?))
}

/// 返事の文章から、報告の行を取り出して、`best` に（新しいものを優先して）入れる。
struct Collector<'a> {
    line_re: &'a Regex,
    n_findings: usize,
    best: std::collections::BTreeMap<usize, AgentReport>,
    ignored: usize,
}

impl Collector<'_> {
    fn take(&mut self, text: &str, at: u64, source: &str) {
        for cap in self.line_re.captures_iter(text) {
            let body = cap[3].trim();
            // 渡した文面のひな形を、そのまま写しただけの行は、報告ではない
            if body.is_empty() || body.contains("<何をしたか>") || body.contains("<理由>") {
                continue;
            }
            let Ok(number) = cap[1].parse::<usize>() else { continue };
            if number == 0 || number > self.n_findings {
                self.ignored += 1;
                continue;
            }
            let report = AgentReport {
                verdict: if &cap[2] == "対応した" { DID } else { SKIPPED }.to_string(),
                text: body.chars().take(TEXT_MAX).collect(),
                reported_at: at,
                source: source.to_string(),
            };
            // 同じ指摘の報告が複数あれば、一番新しいもの（同時刻なら、あとに書かれたもの）
            if self.best.get(&(number - 1)).is_none_or(|old| old.reported_at <= report.reported_at) {
                self.best.insert(number - 1, report);
            }
        }
    }
}

/// Claude Code の 1 ログ。戻り値は、識別子を含んでいたか。
fn scan_claude_file(path: &Path, needle: &[u8], col: &mut Collector, scan: &mut Scan) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        scan.unreadable_lines += 1;
        return false;
    };
    scan.files_scanned += 1;
    let source = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut had_tag = false;
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => {
                scan.unreadable_lines += 1;
                break;
            }
        }
        // 識別子を含む行だけ、JSON として読む（大きいログを、全部は解析しない）
        if !contains(&buf, needle) {
            continue;
        }
        had_tag = true;
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&buf) else {
            scan.unreadable_lines += 1;
            continue;
        };
        // エージェントの返事だけ（渡した文面を読んだ結果や、使う人の発言は、対象にしない）
        if value.get("type").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        let Some(at) = value.get("timestamp").and_then(|v| v.as_str()).and_then(parse_iso_ms) else {
            scan.unreadable_lines += 1;
            continue;
        };
        let Some(blocks) = value.pointer("/message/content").and_then(|c| c.as_array()) else { continue };
        for text in blocks.iter().filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text")).filter_map(|b| b.get("text").and_then(|t| t.as_str())) {
            col.take(text, at, &source);
        }
    }
    had_tag
}

/// Copilot CLI の 1 セッション（`events.jsonl`。1 行 1 イベントの JSON）。戻り値は、識別子を含んでいたか。
/// エージェントの返事（`type = "assistant.message"`）の、本文（`data.content`）だけを対象にする。
/// 渡した文面を読んだ結果（`tool.execution_complete`）や、使う人の発言（`user.message`）は、ひな形が残る場所なので、読まない。
fn scan_copilot_file(path: &Path, needle: &[u8], col: &mut Collector, scan: &mut Scan) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        scan.unreadable_lines += 1;
        return false;
    };
    scan.files_scanned += 1;
    // セッションの ID は、置き場のフォルダの名前（`session-state/<セッション>/events.jsonl`）
    let source = path.parent().and_then(|d| d.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut had_tag = false;
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => {
                scan.unreadable_lines += 1;
                break;
            }
        }
        // 識別子を含む行だけ、JSON として読む（大きいログを、全部は解析しない）
        if !contains(&buf, needle) {
            continue;
        }
        had_tag = true;
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&buf) else {
            scan.unreadable_lines += 1;
            continue;
        };
        if value.get("type").and_then(|v| v.as_str()) != Some("assistant.message") {
            continue;
        }
        let Some(at) = value.get("timestamp").and_then(|v| v.as_str()).and_then(parse_iso_ms) else {
            scan.unreadable_lines += 1;
            continue;
        };
        let Some(text) = value.pointer("/data/content").and_then(|c| c.as_str()) else {
            scan.unreadable_lines += 1;
            continue;
        };
        col.take(text, at, &source);
    }
    had_tag
}

/// agy の 1 会話（SQLite）。戻り値は、識別子を含んでいたか。回答（`step_type = 15`）だけを見る。
/// 最新の `AGY_MAX_ANSWERS_PER_CONV` 手（回答）に対象を絞り、その中から識別子を含む手だけを SQL の側（`instr`）で選ぶ。
/// 内側のクエリで対象の rowid だけを特定してから BLOB を読むことで、長い会話でも過去の大量の回答 BLOB を走査せずに済む。
fn scan_agy_file(path: &Path, needle: &[u8], col: &mut Collector, scan: &mut Scan) -> bool {
    // agy が書いている最中のことがある。読み取り専用で開く
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let Ok(conn) = rusqlite::Connection::open_with_flags(path, flags) else {
        scan.unreadable_lines += 1;
        return false;
    };
    scan.files_scanned += 1;
    let source = format!("agy:{}", path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
    // 識別子は、BLOB として渡す（`instr` は、両方が BLOB のときだけ、バイト単位で探す）
    let sql = "SELECT step_payload FROM steps \
               WHERE status = ?1 AND step_type = ?2 AND instr(step_payload, ?3) > 0 \
               ORDER BY idx DESC \
               LIMIT ?4";
    let Ok(mut stmt) = conn.prepare(sql) else {
        scan.unreadable_lines += 1;
        return false;
    };
    let Ok(rows) = stmt.query_map(params![AGY_STEP_DONE, AGY_ANSWER_STEP, needle, AGY_MAX_ANSWERS_PER_CONV], |r| r.get::<_, Vec<u8>>(0)) else {
        scan.unreadable_lines += 1;
        return false;
    };
    let mut had_tag = false;
    for row in rows {
        let Ok(payload) = row else {
            scan.unreadable_lines += 1;
            continue;
        };
        // ここに来るのは、識別子を含む手だけ（SQL で選んでいる）
        had_tag = true;
        match agy_answer(&payload) {
            Some((text, at)) => col.take(&text, at, &source),
            None => scan.unreadable_lines += 1,
        }
    }
    had_tag
}

/// ログから、このレビューの報告を取り出す。`n_findings` は、このレビューの指摘の数。
/// 使っていないツールの置き場所が無いのは、エラーにしない。**3 つとも無ければ**、エラー（何も調べていないのに、「見つからない」と言わない）。
pub fn scan(sources: &Sources, review_id: &str, n_findings: usize, not_before_ms: u64) -> Result<Scan, String> {
    let needle = format!("mrcr:{review_id}#");
    let line_re = Regex::new(&format!(
        r"(?m)^[\s>\-*•`]*mrcr:{}#(\d+)(?:[（(][^）)\n]{{0,40}}[）)])?[\s`*]*(対応した|対応しなかった)[\s`*]*[:：]\s*(.+?)\s*$",
        regex::escape(review_id)
    ))
    .map_err(|e| e.to_string())?;

    let claude = claude_files(&sources.claude, not_before_ms);
    let agy = agy_files(&sources.agy, not_before_ms);
    let copilot = copilot_files(&sources.copilot, not_before_ms);
    // 使っていないエージェントの置き場所は、無いのが普通。3 つとも開けないときだけ、エラーにする
    if let (Err(c), Err(a), Err(p)) = (&claude, &agy, &copilot) {
        return Err(format!(
            "会話の記録の置き場所を開けません（Claude Code: {}: {c}／agy: {}: {a}／Copilot CLI: {}: {p}）",
            sources.claude.display(),
            sources.agy.display(),
            sources.copilot.display()
        ));
    }

    let mut scan = Scan::default();
    let mut col = Collector { line_re: &line_re, n_findings, best: Default::default(), ignored: 0 };
    let (claude_list, claude_skipped) = claude.unwrap_or_default();
    let (agy_list, agy_skipped) = agy.unwrap_or_default();
    let (copilot_list, copilot_skipped) = copilot.unwrap_or_default();
    scan.skipped_files = claude_skipped + agy_skipped + copilot_skipped;
    for path in claude_list {
        if scan_claude_file(&path, needle.as_bytes(), &mut col, &mut scan) {
            scan.files_with_tag += 1;
        }
    }
    for path in agy_list {
        if scan_agy_file(&path, needle.as_bytes(), &mut col, &mut scan) {
            scan.files_with_tag += 1;
        }
    }
    for path in copilot_list {
        if scan_copilot_file(&path, needle.as_bytes(), &mut col, &mut scan) {
            scan.files_with_tag += 1;
        }
    }
    scan.ignored = col.ignored;
    scan.reports = col.best.into_iter().collect();
    Ok(scan)
}

// ── 保存 ──

/// 取り出した報告を保存する。同じ指摘に、すでにある報告より**古くなければ**、置き換える。変えた件数を返す。
pub fn save(dir: &Path, review_id: &str, reports: &[(usize, AgentReport)]) -> Result<usize, String> {
    let mut conn = db::open(dir)?;
    let tx = conn.transaction().map_err(|e| format!("報告を保存できません: {e}"))?;
    let mut changed = 0;
    for (idx, r) in reports {
        changed += tx
            .execute(
                "INSERT INTO agent_report (review_id, idx, verdict, text, reported_at, source, fetched_at) VALUES (?1,?2,?3,?4,?5,?6,?7) \
                 ON CONFLICT (review_id, idx) DO UPDATE SET verdict = excluded.verdict, text = excluded.text, \
                 reported_at = excluded.reported_at, source = excluded.source, fetched_at = excluded.fetched_at \
                 WHERE excluded.reported_at >= agent_report.reported_at \
                 AND (excluded.reported_at, excluded.text, excluded.verdict) IS NOT (agent_report.reported_at, agent_report.text, agent_report.verdict)",
                params![review_id, *idx as i64, r.verdict, r.text, r.reported_at as i64, r.source, db::now_ms() as i64],
            )
            .map_err(|e| format!("報告を保存できません: {e}"))?;
    }
    tx.commit().map_err(|e| format!("報告を保存できません: {e}"))?;
    Ok(changed)
}

/// `n` 件の指摘それぞれの報告。無い指摘は None（「記録なし」）。読み取りに失敗したら、エラー（成果物用）。
pub fn try_get_all(dir: &Path, review_id: &str, n: usize) -> Result<Vec<Option<AgentReport>>, String> {
    let conn = db::open(dir)?;
    try_get_all_with_conn(&conn, review_id, n)
}

/// 既存の接続を使い回して、エージェントの報告を読み取る。
pub fn try_get_all_with_conn(conn: &rusqlite::Connection, review_id: &str, n: usize) -> Result<Vec<Option<AgentReport>>, String> {
    let mut out = vec![None; n];
    let w = |e: rusqlite::Error| format!("エージェントの報告を読み取れません: {e}");
    let mut stmt = conn.prepare("SELECT idx, verdict, text, reported_at, source FROM agent_report WHERE review_id = ?1").map_err(w)?;
    let rows = stmt
        .query_map([review_id], |r| {
            Ok((r.get::<_, i64>(0)?, AgentReport { verdict: r.get(1)?, text: r.get(2)?, reported_at: r.get::<_, i64>(3)?.max(0) as u64, source: r.get(4)? }))
        })
        .map_err(w)?;
    for row in rows {
        let (idx, rep) = row.map_err(w)?;
        if idx >= 0 && (idx as usize) < n {
            out[idx as usize] = Some(rep);
        }
    }
    Ok(out)
}

/// 画面の表示用。読み取りに失敗しても、「報告なし」で返す（画面を開けなくしない）。成果物には使わない（`try_get_all`）。
pub fn get_all(dir: &Path, review_id: &str, n: usize) -> Vec<Option<AgentReport>> {
    try_get_all(dir, review_id, n).unwrap_or_else(|_| vec![None; n])
}

// ── 許可（ログを読んでよいか。初めての、1 回だけ） ──

pub fn consented(dir: &Path) -> Result<bool, String> {
    let conn = db::open(dir)?;
    match conn.query_row("SELECT value FROM meta WHERE key = ?1", [CONSENT_KEY], |r| r.get::<_, String>(0)) {
        Ok(v) => Ok(v == "1"),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
        Err(e) => Err(format!("許可の印を読み取れません: {e}")),
    }
}

pub fn set_consent(dir: &Path) -> Result<(), String> {
    let conn = db::open(dir)?;
    conn.execute("INSERT INTO meta (key, value) VALUES (?1, '1') ON CONFLICT (key) DO UPDATE SET value = '1'", [CONSENT_KEY])
        .map_err(|e| format!("許可の印を書けません: {e}"))?;
    Ok(())
}

/// 許可を取り消す。取り込み済みの報告は、残す。次に「対応内容を取り込む」を押したときに、もう一度、聞く。
pub fn revoke_consent(dir: &Path) -> Result<(), String> {
    let conn = db::open(dir)?;
    conn.execute("DELETE FROM meta WHERE key = ?1", [CONSENT_KEY]).map_err(|e| format!("許可の印を消せません: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mrcr-agent-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn only_claude(p: &Path) -> Sources {
        Sources { claude: p.to_path_buf(), agy: p.join("no-agy"), copilot: p.join("no-copilot") }
    }

    const RID: &str = "1793059970739-8de6769b";

    fn assistant(ts: &str, text: &str) -> String {
        json!({ "type": "assistant", "timestamp": ts, "message": { "role": "assistant", "content": [{ "type": "text", "text": text }] } }).to_string()
    }
    fn tool_result(ts: &str, text: &str) -> String {
        json!({ "type": "user", "timestamp": ts, "message": { "role": "user", "content": [{ "type": "tool_result", "content": text }] } }).to_string()
    }
    fn write_log(projects: &Path, project: &str, session: &str, lines: &[String]) -> PathBuf {
        let dir = projects.join(project);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(format!("{session}.jsonl"));
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        p
    }

    #[test]
    fn iso_timestamps_become_unix_milliseconds() {
        assert_eq!(parse_iso_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso_ms("1970-01-01T00:00:01.5Z"), Some(1_500));
        assert_eq!(parse_iso_ms("2026-10-27T04:13:42.568Z"), Some(1_793_074_422_568));
        assert_eq!(parse_iso_ms("2000-03-01T00:00:00Z"), Some(951_868_800_000), "うるう年の 3 月 1 日");
        assert_eq!(parse_iso_ms("2026-10-27T04:13:42+09:00"), None, "タイムゾーン付きは読まない");
        assert_eq!(parse_iso_ms("2026-13-01T00:00:00Z"), None);
        assert_eq!(parse_iso_ms("あ"), None);
    }

    #[test]
    fn a_review_name_in_parentheses_after_the_identifier_is_read_over_and_the_text_is_unchanged() {
        let projects = scratch("label");
        write_log(
            &projects,
            "-p",
            "sess-label",
            &[assistant(
                "2026-10-27T04:20:00.000Z",
                &format!(
                    "mrcr:{RID}#1（潜在バグ） 対応した: 境界を直した\n\
                     - `mrcr:{RID}#2(セキュリティ)` 対応しなかった： 理由は別\n\
                     mrcr:{RID}#3 （全角の空白の前） 対応した: これは読まない\n\
                     mrcr:{RID}#4（{}） 対応した: 長すぎる名前は読まない\n\
                     mrcr:{RID}#5（括弧が閉じない 対応した: これも読まない\n",
                    "あ".repeat(41)
                ),
            )],
        );
        let s = scan(&only_claude(&projects), RID, 5, 0).unwrap();
        let by: std::collections::HashMap<usize, (&str, &str)> = s.reports.iter().map(|(i, r)| (*i, (r.verdict.as_str(), r.text.as_str()))).collect();
        assert_eq!(by.get(&0), Some(&("did", "境界を直した")), "観点の名前は、本文に入らない");
        assert_eq!(by.get(&1), Some(&("skipped", "理由は別")), "半角の括弧・全角コロンも読める");
        assert_eq!(by.get(&2), None, "識別子と括弧のあいだの空白は、許さない（読み違えを避ける）");
        assert_eq!(by.get(&3), None, "41 文字を超える括弧は、観点の名前ではない");
        assert_eq!(by.get(&4), None, "括弧が閉じないものは、読まない");
        assert_eq!(s.reports.len(), 2);
    }

    #[test]
    fn takes_only_the_agents_reply_lines_with_the_verdict_and_ignores_the_template_and_other_roles() {
        let projects = scratch("take");
        let template = format!("## 最後の報告\nmrcr:{RID}#1 対応した: <何をしたか> ／ 対応しなかった: <理由>\n");
        write_log(
            &projects,
            "-p",
            "sess1",
            &[
                // 渡した文面を読んだ結果（ひな形入り）: 使う人側の tool_result。対象外
                tool_result("2026-10-27T04:10:00.000Z", &template),
                // エージェントが、ひな形をそのまま写した返事: 報告ではない
                assistant("2026-10-27T04:11:00.000Z", &template),
                // 本物の報告（2 件。箇条書きと、全角コロン、バッククォートも許す）
                assistant(
                    "2026-10-27T04:20:00.000Z",
                    &format!("直しました。\n\nmrcr:{RID}#1 対応した: usage_summary を列だけから読む形にした\n- `mrcr:{RID}#2` 対応しなかった： 既に直っていた\n"),
                ),
                // 別のレビューの識別子: 対象外
                assistant("2026-10-27T04:21:00.000Z", "mrcr:other#1 対応した: 別のレビュー"),
                // 使う人の発言（user）: 対象外
                json!({ "type": "user", "timestamp": "2026-10-27T04:22:00.000Z", "message": { "role": "user", "content": format!("mrcr:{RID}#2 対応した: 偽") } }).to_string(),
            ],
        );
        let s = scan(&only_claude(&projects), RID, 2, 0).unwrap();
        assert_eq!((s.files_scanned, s.files_with_tag, s.unreadable_lines, s.ignored), (1, 1, 0, 0));
        assert_eq!(s.reports.len(), 2);
        let (i0, r0) = &s.reports[0];
        assert_eq!((*i0, r0.verdict.as_str(), r0.text.as_str(), r0.source.as_str()), (0, "did", "usage_summary を列だけから読む形にした", "sess1"));
        assert_eq!(r0.reported_at, parse_iso_ms("2026-10-27T04:20:00.000Z").unwrap());
        let (i1, r1) = &s.reports[1];
        assert_eq!((*i1, r1.verdict.as_str(), r1.text.as_str()), (1, "skipped", "既に直っていた"));
        let _ = std::fs::remove_dir_all(&projects);
    }

    #[test]
    fn the_newest_report_wins_across_logs_and_numbers_outside_the_review_are_ignored() {
        let projects = scratch("newest");
        write_log(&projects, "-a", "old", &[assistant("2026-10-27T04:00:00.000Z", &format!("mrcr:{RID}#1 対応しなかった: 後で"))]);
        write_log(
            &projects,
            "-b",
            "new",
            &[
                assistant("2026-10-27T05:00:00.000Z", &format!("mrcr:{RID}#1 対応した: やった\nmrcr:{RID}#9 対応した: 存在しない番号\nmrcr:{RID}#0 対応した: 0 番")),
            ],
        );
        let s = scan(&only_claude(&projects), RID, 1, 0).unwrap();
        assert_eq!(s.reports.len(), 1);
        assert_eq!((s.reports[0].1.verdict.as_str(), s.reports[0].1.source.as_str()), ("did", "new"));
        assert_eq!(s.ignored, 2, "このレビューに無い番号（0 と 9）は、数えて、取り込まない");
        assert_eq!(s.files_with_tag, 2);
        let _ = std::fs::remove_dir_all(&projects);
    }

    // ── Copilot CLI（`<置き場>/session-state/<セッション>/events.jsonl`。1 行 1 イベント） ──

    fn only_copilot(root: &Path) -> Sources {
        Sources { claude: root.join("no-claude"), agy: root.join("no-agy"), copilot: root.to_path_buf() }
    }
    fn copilot_event(ty: &str, ts: &str, data: serde_json::Value) -> String {
        json!({ "type": ty, "timestamp": ts, "id": "e", "parentId": "p", "data": data }).to_string()
    }
    fn write_copilot(root: &Path, session: &str, lines: &[String]) -> PathBuf {
        let dir = root.join("session-state").join(session);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("events.jsonl");
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        p
    }

    #[test]
    fn copilot_takes_only_the_assistant_message_content_and_ignores_tool_results_the_template_and_other_roles() {
        let root = scratch("copilot-take");
        let template = format!("## 最後の報告\nmrcr:{RID}#1 対応した: <何をしたか> ／ 対応しなかった: <理由>\n");
        write_copilot(
            &root,
            "sess-cp",
            &[
                // 渡した文面（handoff.md）を、ツールで読んだ結果（ひな形入り）: 対象外
                copilot_event("tool.execution_complete", "2026-10-27T04:10:00.000Z", json!({ "result": { "content": template, "detailedContent": template } })),
                // 返事だが、ひな形を写しただけ: 報告ではない
                copilot_event("assistant.message", "2026-10-27T04:11:00.000Z", json!({ "content": template })),
                // 本物の報告（2 件）。観点の名前の括弧つきと、箇条書き・全角コロンも読める
                copilot_event(
                    "assistant.message",
                    "2026-10-27T04:20:00.000Z",
                    json!({ "content": format!("直しました。\n\nmrcr:{RID}#1（リンター） 対応した: 関数名を直した ／ 対応しなかった: なし\n- `mrcr:{RID}#2` 対応しなかった： 既に直っていた\n") }),
                ),
                // 別のレビューの識別子: 対象外
                copilot_event("assistant.message", "2026-10-27T04:21:00.000Z", json!({ "content": "mrcr:other#1 対応した: 別のレビュー" })),
                // 使う人の発言: 対象外
                copilot_event("user.message", "2026-10-27T04:22:00.000Z", json!({ "content": format!("mrcr:{RID}#2 対応した: 偽") })),
                // 本文が空（ツールを呼ぶだけの返事）: 識別子を含まない
                copilot_event("assistant.message", "2026-10-27T04:23:00.000Z", json!({ "content": "", "toolRequests": [] })),
            ],
        );
        let s = scan(&only_copilot(&root), RID, 2, 0).unwrap();
        assert_eq!((s.files_scanned, s.files_with_tag, s.unreadable_lines, s.ignored), (1, 1, 0, 0));
        assert_eq!(s.reports.len(), 2);
        let (i0, r0) = &s.reports[0];
        assert_eq!((*i0, r0.verdict.as_str(), r0.text.as_str(), r0.source.as_str()), (0, "did", "関数名を直した ／ 対応しなかった: なし", "sess-cp"));
        assert_eq!(r0.reported_at, parse_iso_ms("2026-10-27T04:20:00.000Z").unwrap());
        let (i1, r1) = &s.reports[1];
        assert_eq!((*i1, r1.verdict.as_str(), r1.text.as_str()), (1, "skipped", "既に直っていた"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copilot_and_claude_logs_are_read_together_and_the_newest_report_wins() {
        let root = scratch("copilot-both");
        let claude = root.join("claude");
        write_log(&claude, "-p", "old", &[assistant("2026-10-27T04:00:00.000Z", &format!("mrcr:{RID}#1 対応しなかった: 古い"))]);
        write_copilot(&root, "new", &[copilot_event("assistant.message", "2026-10-27T05:00:00.000Z", json!({ "content": format!("mrcr:{RID}#1 対応した: 新しい") }))]);
        let both = Sources { claude, agy: root.join("no-agy"), copilot: root.clone() };
        let s = scan(&both, RID, 1, 0).unwrap();
        assert_eq!((s.files_scanned, s.files_with_tag), (2, 2));
        assert_eq!((s.reports[0].1.verdict.as_str(), s.reports[0].1.text.as_str(), s.reports[0].1.source.as_str()), ("did", "新しい", "new"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copilot_unreadable_lines_are_counted_and_a_missing_copilot_directory_is_not_an_error_while_another_source_opens() {
        let root = scratch("copilot-unreadable");
        write_copilot(
            &root,
            "s",
            &[
                format!("{{ これは JSON ではない mrcr:{RID}#1"),
                // 時刻が無い返事
                json!({ "type": "assistant.message", "data": { "content": format!("mrcr:{RID}#1 対応した: x") } }).to_string(),
                // data.content が文字列ではない返事
                copilot_event("assistant.message", "2026-10-27T04:00:00.000Z", json!({ "content": [format!("mrcr:{RID}#1 対応した: x")] })),
                copilot_event("assistant.message", "2026-10-27T04:00:00.000Z", json!({ "content": "識別子の無い返事" })),
            ],
        );
        let s = scan(&only_copilot(&root), RID, 1, 0).unwrap();
        assert_eq!((s.reports.len(), s.unreadable_lines, s.files_with_tag), (0, 3, 1), "報告なし。読めなかった 3 行は、件数で言う");
        // Copilot の置き場が無くても、Claude Code の置き場が開ければ、エラーにしない（使っていないエージェントは、無いのが普通）
        let ok = scan(&Sources { claude: root.clone(), agy: root.join("no-agy"), copilot: root.join("no-copilot") }, RID, 1, 0);
        assert!(ok.is_ok());
        // 3 つとも開けなければ、エラー。Copilot の置き場も、メッセージに出る
        let e = scan(&Sources { claude: root.join("x1"), agy: root.join("x2"), copilot: root.join("x3") }, RID, 1, 0).unwrap_err();
        assert!(e.contains("会話の記録の置き場所を開けません") && e.contains("Copilot CLI"), "{e}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copilot_logs_older_than_the_review_are_skipped_and_the_limit_is_counted_not_hidden() {
        let root = scratch("copilot-limit");
        // 35 件のセッション。mtime を、10 秒ずつ新しくする（s00 が一番古く、s34 が一番新しい）
        let base = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_800_000_000);
        for i in 0..35u64 {
            let p = write_copilot(&root, &format!("s{i:02}"), &[copilot_event("assistant.message", "2026-10-27T04:00:00.000Z", json!({ "content": "x" }))]);
            let f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
            f.set_times(std::fs::FileTimes::new().set_modified(base + std::time::Duration::from_secs(10 * i))).unwrap();
        }
        let (list, skipped) = copilot_files(&root, 0).unwrap();
        assert_eq!((list.len(), skipped), (COPILOT_MAX_FILES, 5), "上限を超えた 5 件は、調べずに、数えて言う");
        let names: Vec<String> = list.iter().map(|p| p.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names.first().map(String::as_str), Some("s34"), "最終更新が新しい順");
        assert_eq!(names.last().map(String::as_str), Some("s05"));
        // レビューを作った後に更新されたものだけ（s20 以降）
        let not_before = (1_800_000_000u64 + 200) * 1000;
        let (newer, skipped2) = copilot_files(&root, not_before).unwrap();
        assert_eq!((newer.len(), skipped2), (15, 0));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn what_cannot_be_read_is_counted_not_hidden_and_a_missing_directory_is_an_error() {
        let projects = scratch("unreadable");
        write_log(
            &projects,
            "-p",
            "s",
            &[
                format!("{{ これは JSON ではない mrcr:{RID}#1"),
                // 時刻が無い返事
                json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": format!("mrcr:{RID}#1 対応した: x") }] } }).to_string(),
                assistant("2026-10-27T04:00:00.000Z", "識別子の無い返事"),
            ],
        );
        let s = scan(&only_claude(&projects), RID, 1, 0).unwrap();
        assert_eq!((s.reports.len(), s.unreadable_lines, s.files_with_tag), (0, 2, 1), "報告なし。読めなかった 2 行は、件数で言う");
        let e = scan(&Sources { claude: projects.join("no-such-dir"), agy: projects.join("no-agy"), copilot: projects.join("no-copilot") }, RID, 1, 0).unwrap_err();
        assert!(e.contains("会話の記録の置き場所を開けません"), "{e}");
        let _ = std::fs::remove_dir_all(&projects);
    }

    #[test]
    fn logs_not_updated_since_the_review_are_not_even_opened() {
        let projects = scratch("mtime");
        write_log(&projects, "-p", "s", &[assistant("2026-10-27T04:00:00.000Z", &format!("mrcr:{RID}#1 対応した: x"))]);
        let future = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64 + 3_600_000;
        let s = scan(&only_claude(&projects), RID, 1, future).unwrap();
        assert_eq!((s.files_scanned, s.reports.len()), (0, 0));
        assert_eq!(scan(&only_claude(&projects), RID, 1, 0).unwrap().files_scanned, 1);
        let _ = std::fs::remove_dir_all(&projects);
    }

    #[test]
    fn saving_keeps_the_newest_report_and_never_replaces_it_with_an_older_one() {
        let d = scratch("save");
        let rep = |v: &str, t: &str, at: u64| AgentReport { verdict: v.into(), text: t.into(), reported_at: at, source: "s".into() };
        assert_eq!(save(&d, "r1", &[(0, rep(DID, "最初", 100))]).unwrap(), 1);
        assert_eq!(save(&d, "r1", &[(0, rep(DID, "最初", 100))]).unwrap(), 0, "同じ内容なら、変えない");
        assert_eq!(save(&d, "r1", &[(0, rep(SKIPPED, "古い報告", 50))]).unwrap(), 0, "古い報告で、新しいものを置き換えない");
        assert_eq!(save(&d, "r1", &[(0, rep(SKIPPED, "新しい報告", 200)), (2, rep(DID, "別の指摘", 10))]).unwrap(), 2);
        let all = try_get_all(&d, "r1", 3).unwrap();
        assert_eq!(all[0].as_ref().map(|r| (r.verdict.as_str(), r.text.as_str())), Some(("skipped", "新しい報告")));
        assert!(all[1].is_none(), "報告の無い指摘は、None（記録なし）");
        assert_eq!(all[2].as_ref().map(|r| r.text.as_str()), Some("別の指摘"));
        assert_eq!(try_get_all(&d, "r1", 1).unwrap().len(), 1, "範囲の外の指摘は、返さない");
        assert!(try_get_all(&d, "other", 2).unwrap().iter().all(Option::is_none));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn display_reader_is_lenient_but_the_strict_one_reports_a_broken_table() {
        let d = scratch("broken");
        save(&d, "r1", &[(0, AgentReport { verdict: DID.into(), text: "x".into(), reported_at: 1, source: "s".into() })]).unwrap();
        let c = rusqlite::Connection::open(d.join("mrcodereview.sqlite3")).unwrap();
        c.execute_batch("DROP TABLE agent_report; CREATE TABLE agent_report (x INTEGER);").unwrap();
        drop(c);
        assert_eq!(get_all(&d, "r1", 2), vec![None, None]);
        assert!(try_get_all(&d, "r1", 2).unwrap_err().contains("エージェントの報告を読み取れません"));
        let _ = std::fs::remove_dir_all(&d);
    }

    // ── agy（SQLite の steps 表に、protobuf で入っている） ──

    fn varint(mut v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return out;
            }
            out.push(b | 0x80);
        }
    }
    fn pb_bytes_field(num: u64, data: &[u8]) -> Vec<u8> {
        let mut out = varint(num << 3 | 2);
        out.extend(varint(data.len() as u64));
        out.extend_from_slice(data);
        out
    }
    fn pb_varint_field(num: u64, v: u64) -> Vec<u8> {
        let mut out = varint(num << 3);
        out.extend(varint(v));
        out
    }
    /// 回答の 1 手（時刻 `[5,1]`・本文 `[20,1]`）
    fn agy_payload(text: &str, secs: u64, nanos: u64) -> Vec<u8> {
        let stamp = [pb_varint_field(1, secs), pb_varint_field(2, nanos)].concat();
        [pb_bytes_field(5, &pb_bytes_field(1, &stamp)), pb_bytes_field(20, &pb_bytes_field(1, text.as_bytes()))].concat()
    }
    /// (idx, step_type, status, payload)
    fn write_agy_db(agy_root: &Path, name: &str, steps: &[(i64, i64, i64, Vec<u8>)]) -> PathBuf {
        let dir = agy_root.join("conversations");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(format!("{name}.db"));
        let c = rusqlite::Connection::open(&p).unwrap();
        c.execute_batch("CREATE TABLE steps (idx INTEGER, step_type INTEGER, status INTEGER, step_payload BLOB);").unwrap();
        for (idx, ty, status, payload) in steps {
            c.execute("INSERT INTO steps (idx, step_type, status, step_payload) VALUES (?1, ?2, ?3, ?4)", params![idx, ty, status, payload]).unwrap();
        }
        p
    }
    fn agy_only(root: &Path) -> Sources {
        Sources { claude: root.join("no-claude"), agy: root.to_path_buf(), copilot: root.join("no-copilot") }
    }
    // 2026-10-27T04:20:00Z
    const SECS: u64 = 1_793_074_800;

    #[test]
    fn takes_the_agy_answer_with_the_verdict_and_time_and_ignores_everything_else() {
        let root = scratch("agy-take");
        let template = format!("mrcr:{RID}#1 対応した: <何をしたか> ／ 対応しなかった: <理由>");
        let steps = vec![
            // 使う人の発言（14）・ツールの結果など（7）: 対象外
            (0, 14, 3, agy_payload(&format!("mrcr:{RID}#1 対応した: 偽"), SECS, 0)),
            (1, 7, 3, agy_payload(&template, SECS, 0)),
            // 書いている途中（status 1）の回答: 対象外
            (2, 15, 1, agy_payload(&format!("mrcr:{RID}#1 対応した: 途中"), SECS, 0)),
            // ひな形の写し: 報告ではない
            (3, 15, 3, agy_payload(&template, SECS, 0)),
            // 別のレビュー: 対象外
            (4, 15, 3, agy_payload("mrcr:other#1 対応した: 別", SECS, 0)),
            // 本物の報告
            (5, 15, 3, agy_payload(&format!("直しました。\nmrcr:{RID}#1 対応した: usage_summary を列だけから読む形にした\nmrcr:{RID}#2 対応しなかった: 既に直っていた"), SECS + 60, 500_000_000)),
        ];
        write_agy_db(&root, "conv-1", &steps);
        let s = scan(&agy_only(&root), RID, 2, 0).unwrap();
        assert_eq!((s.files_scanned, s.files_with_tag, s.unreadable_lines, s.ignored), (1, 1, 0, 0));
        assert_eq!(s.reports.len(), 2);
        let (i0, r0) = &s.reports[0];
        assert_eq!((*i0, r0.verdict.as_str(), r0.text.as_str(), r0.source.as_str()), (0, "did", "usage_summary を列だけから読む形にした", "agy:conv-1"));
        assert_eq!(r0.reported_at, (SECS + 60) * 1000 + 500, "時刻は、秒とナノ秒から、ミリ秒に直す");
        let (i1, r1) = &s.reports[1];
        assert_eq!((*i1, r1.verdict.as_str(), r1.text.as_str()), (1, "skipped", "既に直っていた"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn claude_code_and_agy_are_read_together_and_the_newest_report_wins() {
        let root = scratch("both");
        write_log(&root.join("claude"), "-p", "old", &[assistant("2026-10-27T04:00:00.000Z", &format!("mrcr:{RID}#1 対応しなかった: 古い"))]);
        write_agy_db(&root.join("agy"), "newer", &[(0, 15, 3, agy_payload(&format!("mrcr:{RID}#1 対応した: agy でやった"), SECS, 0))]);
        let both = Sources { claude: root.join("claude"), agy: root.join("agy"), copilot: root.join("no-copilot") };
        let s = scan(&both, RID, 1, 0).unwrap();
        assert_eq!((s.files_scanned, s.files_with_tag), (2, 2));
        assert_eq!((s.reports[0].1.verdict.as_str(), s.reports[0].1.source.as_str()), ("did", "agy:newer"));
        // 逆に、Claude Code のほうが新しければ、そちらが勝つ
        write_log(&root.join("claude"), "-p", "newest", &[assistant("2026-10-27T09:00:00.000Z", &format!("mrcr:{RID}#1 対応しなかった: あとで"))]);
        let s = scan(&both, RID, 1, 0).unwrap();
        assert_eq!((s.reports[0].1.verdict.as_str(), s.reports[0].1.source.as_str()), ("skipped", "newest"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_tool_directory_is_fine_but_having_neither_is_an_error() {
        let root = scratch("dirs");
        write_agy_db(&root.join("agy"), "c", &[(0, 15, 3, agy_payload("識別子の無い返事", SECS, 0))]);
        // Claude Code を使っていない（置き場所が無い）: エラーにしない
        let s = scan(&Sources { claude: root.join("none"), agy: root.join("agy"), copilot: root.join("no-copilot") }, RID, 1, 0).unwrap();
        assert_eq!((s.files_scanned, s.files_with_tag, s.reports.len()), (1, 0, 0));
        // どちらも無い: 何も調べていないので、「見つからない」とは言わず、エラー
        let e = scan(&Sources { claude: root.join("none"), agy: root.join("none2"), copilot: root.join("no-copilot") }, RID, 1, 0).unwrap_err();
        assert!(e.contains("Claude Code:") && e.contains("agy:"), "{e}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unreadable_agy_data_is_counted_not_hidden() {
        let root = scratch("agy-bad");
        let dir = root.join("conversations");
        std::fs::create_dir_all(&dir).unwrap();
        // データベースではないファイル
        std::fs::write(dir.join("broken.db"), b"this is not a sqlite database at all, just bytes").unwrap();
        // 識別子を含むが、protobuf として読めない回答
        let mut bad = format!("mrcr:{RID}#1 対応した: x").into_bytes();
        bad.insert(0, 0xff);
        write_agy_db(&root, "garbled", &[(0, 15, 3, bad)]);
        let s = scan(&agy_only(&root), RID, 1, 0).unwrap();
        assert_eq!(s.reports.len(), 0);
        assert_eq!(s.unreadable_lines, 2, "開けない DB と、読めない回答を、それぞれ数える");
        assert_eq!(s.files_with_tag, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn agy_steps_without_the_tag_are_not_read_at_all_even_if_their_data_is_unreadable() {
        let root = scratch("agy-sql-filter");
        let path = write_agy_db(&root, "c", &[(0, 15, 3, agy_payload(&format!("mrcr:{RID}#1 対応した: やった"), SECS, 0))]);
        // 識別子を含まない手: データが NULL・文字列・壊れたバイト列でも、読まない（読んでいれば、「読めなかった」と数える）
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute("INSERT INTO steps (idx, step_type, status, step_payload) VALUES (1, 15, 3, NULL)", []).unwrap();
        c.execute("INSERT INTO steps (idx, step_type, status, step_payload) VALUES (2, 15, 3, 'text')", []).unwrap();
        c.execute("INSERT INTO steps (idx, step_type, status, step_payload) VALUES (3, 15, 3, x'ffffffff')", []).unwrap();
        drop(c);
        let s = scan(&agy_only(&root), RID, 1, 0).unwrap();
        assert_eq!((s.reports.len(), s.unreadable_lines, s.files_with_tag), (1, 0, 1), "識別子を含む手だけが、読まれる");
        // 識別子を含む手が、1 つも無い会話は、「識別子を含むログ」に数えない
        let none = scratch("agy-sql-none");
        write_agy_db(&none, "c", &[(0, 15, 3, agy_payload("識別子の無い返事", SECS, 0))]);
        let s = scan(&agy_only(&none), RID, 1, 0).unwrap();
        assert_eq!((s.files_scanned, s.files_with_tag, s.reports.len()), (1, 0, 0));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&none);
    }

    #[test]
    fn agy_conversations_not_updated_since_the_review_are_not_opened() {
        let root = scratch("agy-mtime");
        write_agy_db(&root, "c", &[(0, 15, 3, agy_payload(&format!("mrcr:{RID}#1 対応した: x"), SECS, 0))]);
        let future = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64 + 3_600_000;
        assert_eq!(scan(&agy_only(&root), RID, 1, future).unwrap().files_scanned, 0);
        assert_eq!(scan(&agy_only(&root), RID, 1, 0).unwrap().reports.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn consent_is_asked_once_and_remembered() {
        let d = scratch("consent");
        assert!(!consented(&d).unwrap(), "初めは、許可なし");
        set_consent(&d).unwrap();
        set_consent(&d).unwrap();
        assert!(consented(&d).unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_consent_given_before_copilot_cli_was_added_is_not_carried_over_and_the_new_one_is_asked_again() {
        let d = scratch("consent-v1");
        // 前の版（Claude Code と agy だけ）の許可の印
        db::open(&d).unwrap().execute("INSERT INTO meta (key, value) VALUES ('agent_log_consent', '1')", []).unwrap();
        assert!(!consented(&d).unwrap(), "読む場所が増えたので、前の許可は、引き継がない（もう一度、聞く）");
        set_consent(&d).unwrap();
        assert!(consented(&d).unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn revoking_the_consent_asks_again_next_time_and_keeps_the_reports_already_imported() {
        let d = scratch("revoke");
        // 許可していなくても、取り消しは、失敗しない
        revoke_consent(&d).unwrap();
        assert!(!consented(&d).unwrap());
        set_consent(&d).unwrap();
        save(&d, "r1", &[(0, AgentReport { verdict: DID.into(), text: "やった".into(), reported_at: 1, source: "s".into() })]).unwrap();
        assert!(consented(&d).unwrap());
        revoke_consent(&d).unwrap();
        assert!(!consented(&d).unwrap(), "取り消したら、許可なし");
        assert_eq!(try_get_all(&d, "r1", 1).unwrap()[0].as_ref().map(|r| r.text.as_str()), Some("やった"), "取り込み済みの報告は、残る");
        // 取り消したあとに、もう一度許可できる
        set_consent(&d).unwrap();
        assert!(consented(&d).unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn agy_does_not_miss_a_report_even_when_many_unrelated_answers_follow_it() {
        let root = scratch("agy-long-after");
        // 40 手の回答。古い 0 手目と、35 手目に報告がある。あいだと、あとは、識別子と関係のない回答（30 件より多い）
        let mut steps = Vec::new();
        for i in 0..40 {
            let payload = if i == 0 {
                agy_payload(&format!("mrcr:{RID}#1 対応した: 古い報告（最新 30 手の外）"), SECS + i as u64, 0)
            } else if i == 5 {
                agy_payload(&format!("mrcr:{RID}#2 対応しなかった: 5 手目の報告"), SECS + i as u64, 0)
            } else {
                agy_payload("普通の返答", SECS + i as u64, 0)
            };
            steps.push((i, 15, 3, payload));
        }
        write_agy_db(&root, "long-conv", &steps);
        let s = scan(&agy_only(&root), RID, 2, 0).unwrap();
        // 識別子で絞ってから、最新の N 件を読む: 回答が 30 件より多く続いても、両方の報告が見つかる
        // （絞る前の最新 30 件で打ち切ると、どちらも見逃す。以前の版は、そうだった）
        assert_eq!(s.reports.iter().map(|(i, r)| (*i, r.text.as_str())).collect::<Vec<_>>(), vec![(0, "古い報告（最新 30 手の外）"), (1, "5 手目の報告")]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn agy_reads_only_the_newest_matching_answers_when_there_are_more_than_the_limit() {
        let root = scratch("agy-many-matching");
        // 同じ指摘の報告が、40 回出てくる（毎回、時刻が新しくなる）。上限（30）を超えても、一番新しい報告は、必ず読まれる
        let steps: Vec<_> = (0..40).map(|i| (i, 15, 3, agy_payload(&format!("mrcr:{RID}#1 対応した: 報告 {i}"), SECS + i as u64, 0))).collect();
        write_agy_db(&root, "many", &steps);
        let s = scan(&agy_only(&root), RID, 1, 0).unwrap();
        assert_eq!(s.reports.len(), 1);
        assert_eq!(s.reports[0].1.text, "報告 39", "最新の報告が、採られる");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn logs_beyond_the_per_tool_limit_are_counted_as_skipped_not_silently_dropped() {
        let root = scratch("skipped-count");
        let claude = root.join("claude");
        let copilot = root.join("copilot");
        for i in 0..(CLAUDE_MAX_FILES + 5) {
            write_log(&claude, "-p", &format!("s{i:03}"), &[assistant("2026-10-27T04:00:00.000Z", "識別子の無い返事")]);
        }
        for i in 0..(AGY_MAX_CONVERSATIONS + 2) {
            write_agy_db(&root.join("agy"), &format!("c{i:03}"), &[(0, 15, 3, agy_payload("識別子の無い返事", SECS, 0))]);
        }
        for i in 0..(COPILOT_MAX_FILES + 3) {
            write_copilot(&copilot, &format!("cp{i:03}"), &[copilot_event("assistant.message", "2026-10-27T04:00:00.000Z", json!({ "content": "識別子の無い返事" }))]);
        }
        let s = scan(&Sources { claude, agy: root.join("agy"), copilot }, RID, 1, 0).unwrap();
        assert_eq!(s.files_scanned, CLAUDE_MAX_FILES + AGY_MAX_CONVERSATIONS + COPILOT_MAX_FILES, "上限までを調べた");
        assert_eq!(s.skipped_files, 5 + 2 + 3, "調べなかった数を言う（Claude Code 5 件・agy 2 件・Copilot CLI 3 件）");
        // 上限に達していなければ、0
        let few = scratch("skipped-zero");
        write_log(&few, "-p", "s", &[assistant("2026-10-27T04:00:00.000Z", "x")]);
        assert_eq!(scan(&only_claude(&few), RID, 1, 0).unwrap().skipped_files, 0);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&few);
    }

    #[test]
    fn copilot_source_prefers_mrcr_copilot_dir_then_copilot_home_then_default() {
        let prev_mrcr = std::env::var_os("MRCR_COPILOT_DIR");
        let prev_home = std::env::var_os("COPILOT_HOME");

        // 1. 両方設定されているときは MRCR_COPILOT_DIR が優先される
        std::env::set_var("MRCR_COPILOT_DIR", "/custom/mrcr-copilot");
        std::env::set_var("COPILOT_HOME", "/custom/copilot-home");
        assert_eq!(Sources::from_env().copilot, PathBuf::from("/custom/mrcr-copilot"));

        // 2. MRCR_COPILOT_DIR が無いときは COPILOT_HOME が使われる
        std::env::remove_var("MRCR_COPILOT_DIR");
        assert_eq!(Sources::from_env().copilot, PathBuf::from("/custom/copilot-home"));

        // 3. 両方とも未設定のときは ~/.copilot にフォールバックする
        std::env::remove_var("COPILOT_HOME");
        let expected_default = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(".copilot");
        assert_eq!(Sources::from_env().copilot, expected_default);

        match prev_mrcr {
            Some(v) => std::env::set_var("MRCR_COPILOT_DIR", v),
            None => std::env::remove_var("MRCR_COPILOT_DIR"),
        }
        match prev_home {
            Some(v) => std::env::set_var("COPILOT_HOME", v),
            None => std::env::remove_var("COPILOT_HOME"),
        }
    }

    #[test]
    fn agy_files_prioritizes_newest_conversations_and_limits() {
        let root = scratch("agy-files-limit");
        let dir = root.join("conversations");
        std::fs::create_dir_all(&dir).unwrap();
        // 40 件の会話ファイルを作成し、mtime を 10 秒ずつ増加させて設定（conv_00 が一番古く、conv_39 が一番新しい）
        for i in 0..40 {
            let p = dir.join(format!("conv_{i:02}.db"));
            let c = rusqlite::Connection::open(&p).unwrap();
            c.execute_batch("CREATE TABLE steps (idx INTEGER, step_type INTEGER, status INTEGER, step_payload BLOB);").unwrap();
            drop(c);
            let time = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000 + i as u64 * 10);
            let times = std::fs::FileTimes::new().set_modified(time);
            std::fs::File::options().write(true).open(&p).unwrap().set_times(times).unwrap();
        }
        let (files, skipped) = agy_files(&root, 0).unwrap();
        assert_eq!(files.len(), AGY_MAX_CONVERSATIONS, "上限件数までに絞られる");
        assert_eq!(skipped, 40 - AGY_MAX_CONVERSATIONS, "調べなかった数");
        // 最も新しい conv_39 から conv_10 までの 30 件が新しい順（降順）に並び、古い conv_00〜conv_09 は除外される
        for (rank, file) in files.iter().enumerate() {
            let want_i = 39 - rank;
            assert_eq!(file.file_name().unwrap(), format!("conv_{want_i:02}.db").as_str());
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn claude_files_prioritizes_newest_and_limits() {
        let root = scratch("claude-files-limit");
        let proj = root.join("project-x");
        std::fs::create_dir_all(&proj).unwrap();
        for i in 0..40 {
            let p = proj.join(format!("session_{i:02}.jsonl"));
            std::fs::write(&p, "{}\n").unwrap();
            let time = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000 + i as u64 * 10);
            let times = std::fs::FileTimes::new().set_modified(time);
            std::fs::File::options().write(true).open(&p).unwrap().set_times(times).unwrap();
        }
        let (files, skipped) = claude_files(&root, 0).unwrap();
        assert_eq!(files.len(), CLAUDE_MAX_FILES, "上限件数までに絞られる");
        assert_eq!(skipped, 40 - CLAUDE_MAX_FILES, "調べなかった数");
        for (rank, file) in files.iter().enumerate() {
            let want_i = 39 - rank;
            assert_eq!(file.file_name().unwrap(), format!("session_{want_i:02}.jsonl").as_str());
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn contains_fast_search_handles_matches_and_boundaries() {
        assert!(contains(b"prefix mrcr:123# suffix", b"mrcr:123#"));
        assert!(contains(b"mrcr:123#", b"mrcr:123#"), "完全一致");
        assert!(contains(b"mrcr:123# suffix", b"mrcr:123#"), "先頭一致");
        assert!(contains(b"prefix mrcr:123#", b"mrcr:123#"), "末尾一致");
        assert!(!contains(b"prefix mrcr:12X# suffix", b"mrcr:123#"), "途中で不一致");
        assert!(!contains(b"short", b"longer_needle"), "hay が needle より短い");
        assert!(!contains(b"any", b""), "空の needle");
    }

    #[test]
    fn agy_answer_handles_nanos_boundary_fallback_and_invalid_utf8() {
        // 1. nanos が 999_999_999 のとき 999ms になる
        let p_999 = agy_payload("text", 100, 999_999_999);
        let (text, at) = agy_answer(&p_999).unwrap();
        assert_eq!(text, "text");
        assert_eq!(at, 100_999);

        // nanos が 1 秒以上でも min(999) で 999ms に丸められる
        let p_over = agy_payload("text", 100, 1_500_000_000);
        assert_eq!(agy_answer(&p_over).unwrap().1, 100_999);

        // 2. [20, 1] がなく [20, 8] だけを持つ回答から本文が取れる
        let stamp = [pb_varint_field(1, 100), pb_varint_field(2, 50_000_000)].concat();
        let p_fallback = [pb_bytes_field(5, &pb_bytes_field(1, &stamp)), pb_bytes_field(20, &pb_bytes_field(8, "fallback".as_bytes()))].concat();
        let (text, at) = agy_answer(&p_fallback).unwrap();
        assert_eq!(text, "fallback");
        assert_eq!(at, 100_050);

        // 3. 本文が UTF-8 でないときは None になる
        let p_bad_utf8 = [pb_bytes_field(5, &pb_bytes_field(1, &stamp)), pb_bytes_field(20, &pb_bytes_field(1, &[0xff, 0xfe]))].concat();
        assert!(agy_answer(&p_bad_utf8).is_none());
    }

    #[test]
    fn same_timestamp_prefers_later_reported_verdict() {
        let root = scratch("same-time");
        let ts = "2026-10-27T04:00:00.000Z";
        write_log(
            &root.join("claude"),
            "-p",
            "same_time",
            &[
                assistant(ts, &format!("mrcr:{RID}#1 対応しなかった: 最初に書いた理由")),
                assistant(ts, &format!("mrcr:{RID}#1 対応した: あとで直した内容")),
            ],
        );
        let s = scan(&Sources { claude: root.join("claude"), agy: root.join("no-agy"), copilot: root.join("no-copilot") }, RID, 1, 0).unwrap();
        assert_eq!(s.reports.len(), 1);
        assert_eq!(s.reports[0].1.verdict, "did");
        assert_eq!(s.reports[0].1.text, "あとで直した内容");
        let _ = std::fs::remove_dir_all(&root);
    }
}
