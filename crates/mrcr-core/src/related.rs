//! 関連コード: 差分に出ていないコードのうち、差分と食い違うかもしれない箇所を集める。
//!
//! 「他ファイルとの不整合」は、差分だけでは判断できない。関数の引数を減らしても、呼び出し元は差分に出ない。
//! そこで、差分で変更した**名前**（関数・型・定数・設定のキーなど）を取り出し、`git grep` で、
//! 差分に出ていない使用箇所を、前後数行つきで集める。
//!
//! - 名前は、削除・変更されたもの、定義らしい行にあるものを優先する（呼び出し元が壊れうるのは、そちら）。
//! - ありふれた名前（使用箇所が多すぎる）は、手がかりにならないので除く。**除いたものも記録する。**
//! - 上限（合計のバイト数・名前の数・1 つの名前あたりの件数・前後の行数）は、すべて設定。
//! - 送ったもの・送らなかったものは、レビューの記録に残す（何を根拠に見たのかを、あとから確かめられる）。

use crate::carry;
use crate::proc;
use crate::snapshot::{FileDiff, Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// 関連コードの合計の上限（推奨値）。
pub const MAX_BYTES: usize = 60_000;
/// 使用箇所を調べる名前の数の上限。
pub const MAX_NAMES: usize = 12;
/// 1 つの名前について送る使用箇所の数の上限。
pub const HITS_PER_NAME: usize = 6;
/// 使用箇所の前後に添える行数。
pub const CONTEXT_LINES: usize = 3;
/// これより使用箇所が多い名前は、ありふれていると見なして調べない。
pub const SKIP_OVER_HITS: usize = 40;

const GREP_TIMEOUT: Duration = Duration::from_secs(10);
const MIN_NAME_LEN: usize = 4;
/// 1 行がこれより長いときは、切る（minify したコードなどで、1 行が巨大になる）。
const MAX_LINE_CHARS: usize = 300;

#[derive(Debug, Clone)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_names: usize,
    pub hits_per_name: usize,
    pub context_lines: usize,
    pub skip_over_hits: usize,
    pub lockfiles: Vec<String>,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_bytes: MAX_BYTES,
            max_names: MAX_NAMES,
            hits_per_name: HITS_PER_NAME,
            context_lines: CONTEXT_LINES,
            skip_over_hits: SKIP_OVER_HITS,
            lockfiles: crate::snapshot::DEFAULT_LOCKFILES.map(String::from).to_vec(),
        }
    }
}

/// 送った使用箇所（前後つき）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snippet {
    pub name: String,
    pub file: String,
    /// 前後を含む、行の範囲（1 始まり）。
    pub start: u32,
    pub end: u32,
    /// 行番号つきのコード。
    pub text: String,
}

/// 調べた名前ごとの結果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NameReport {
    pub name: String,
    /// なぜ調べたか（差分で削除された名前、など）
    pub why: String,
    /// 差分に出ていない使用箇所の数（全体）
    pub hits: usize,
    /// "sent" 送った / "none" 差分の外に使用箇所が無い / "too_many" ありふれていて除いた / "budget" 上限に達して送らなかった
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Related {
    pub names: Vec<NameReport>,
    pub snippets: Vec<Snippet>,
    /// 画面に出す実行メモ。
    pub notes: Vec<String>,
}

// ── 差分から名前を取り出す ─────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
struct Seen {
    removed: bool,
    added: bool,
    definition: bool,
}

/// 手がかりにならない語（キーワード・どこにでもある名前）。小文字で比べる。
const STOP: &[&str] = &[
    // キーワード（複数の言語）
    "function", "return", "const", "class", "import", "export", "from", "async", "await", "public", "private", "protected",
    "static", "void", "string", "number", "boolean", "true", "false", "null", "undefined", "self", "this", "else", "elif",
    "while", "match", "struct", "enum", "impl", "trait", "type", "interface", "extends", "implements", "default", "break",
    "continue", "throw", "catch", "finally", "new", "delete", "typeof", "instanceof", "with", "yield", "switch", "case",
    "None", "True", "False", "pass", "lambda", "global", "super", "crate", "mod", "use", "where", "then", "end", "begin",
    "println", "print", "var", "let", "def", "func", "fn", "pub", "mut", "ref", "int", "bool", "char", "float", "double",
    "long", "short", "byte", "object", "array", "list", "dict", "map", "set", "vec", "option", "result", "some", "ok", "err",
    "try", "for", "if", "in", "of", "as", "is", "not", "and", "or", "do",
    // どこにでもある名前
    "data", "value", "values", "index", "item", "items", "name", "names", "text", "length", "error", "errors", "message",
    "options", "params", "args", "props", "state", "event", "callback", "key", "keys", "id", "ids", "path", "file", "files",
    "node", "root", "path", "type", "info", "temp", "tmp", "test", "tests", "main", "init", "args", "kwargs", "self",
    "response", "request", "context", "config", "count", "size", "next", "prev", "first", "last", "left", "right",
];

fn is_stop(word: &str) -> bool {
    let w = word.to_lowercase();
    STOP.iter().any(|s| s.to_lowercase() == w)
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// 行から、名前（英数字と `_`。数字で始まらない）を取り出す。
fn ident_tokens(line: &str) -> Vec<&str> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if is_ident_char(b[i]) {
            let start = i;
            while i < b.len() && is_ident_char(b[i]) {
                i += 1;
            }
            let tok = &line[start..i];
            if !b[start].is_ascii_digit() && tok.len() >= MIN_NAME_LEN && tok.bytes().any(|c| c.is_ascii_alphabetic()) {
                out.push(tok);
            }
        } else {
            i += 1;
        }
    }
    out
}

/// 定義らしい行か（呼び出し元が壊れうる、変更の中心）。
fn is_definition(line: &str) -> bool {
    let l = line.trim_start();
    ["fn ", "function ", "def ", "class ", "struct ", "enum ", "trait ", "interface ", "type ", "func ", "const ", "pub ", "export "]
        .iter()
        .any(|k| l.starts_with(k) || l.contains(&format!(" {k}")))
}

/// 差分の変更行（追加・削除）から、名前を、調べる価値の高い順に返す。
fn names(files: &[FileDiff]) -> Vec<(String, Seen)> {
    let mut seen: HashMap<String, Seen> = HashMap::new();
    for f in files {
        let mut in_hunk = false;
        for line in f.text.lines() {
            if line.starts_with("@@ ") {
                in_hunk = true;
                continue;
            }
            if !in_hunk {
                continue;
            }
            let (mark, body) = match line.chars().next() {
                Some('+') => ('+', &line[1..]),
                Some('-') => ('-', &line[1..]),
                _ => continue, // 変更の無い行（前後）
            };
            let def = is_definition(body);
            for tok in ident_tokens(body) {
                if is_stop(tok) {
                    continue;
                }
                let s = seen.entry(tok.to_string()).or_default();
                if mark == '+' {
                    s.added = true;
                } else {
                    s.removed = true;
                }
                s.definition |= def;
            }
        }
    }
    let score = |s: &Seen| (if s.removed { 3 } else { 0 }) + (if s.added { 1 } else { 0 }) + (if s.definition { 2 } else { 0 }) + (if s.removed && s.added { 2 } else { 0 });
    let mut v: Vec<(String, Seen)> = seen.into_iter().collect();
    // 点数の高い順 → 長い名前（具体的）→ 名前の順（結果を安定させる）
    v.sort_by(|a, b| score(&b.1).cmp(&score(&a.1)).then(b.0.len().cmp(&a.0.len())).then(a.0.cmp(&b.0)));
    v
}

fn why(s: &Seen) -> String {
    let base = match (s.removed, s.added) {
        (true, true) => "差分で変更された名前（削除と追加の両方にある）",
        (true, false) => "差分で削除された名前",
        _ => "差分で追加・使われた名前",
    };
    if s.definition { format!("{base}・定義らしい行") } else { base.to_string() }
}

// ── 差分に、すでに出ている範囲 ───────────────────────────────────────────────────

/// ファイルごとの、差分に出ている行（新側）の範囲。ここにある使用箇所は、改めて送らない。
fn shown_ranges(files: &[FileDiff]) -> HashMap<String, Vec<(u32, u32)>> {
    let mut out: HashMap<String, Vec<(u32, u32)>> = HashMap::new();
    for f in files {
        for line in f.text.lines() {
            let Some(rest) = line.strip_prefix("@@ ") else { continue };
            let Some(plus) = rest.split_whitespace().find_map(|t| t.strip_prefix('+')) else { continue };
            let mut it = plus.split(',');
            let (Some(start), len) = (it.next().and_then(|s| s.parse::<u32>().ok()), it.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(1)) else {
                continue;
            };
            if len > 0 {
                out.entry(f.path.clone()).or_default().push((start, start + len - 1));
            }
        }
    }
    out
}

// ── 探す ────────────────────────────────────────────────────────────────────────

/// `git grep` の、1 行ぶんの結果を読む（`path:行:内容`）。
fn parse_grep_line(line: &str) -> Option<(String, u32)> {
    let mut parts = line.splitn(3, ':');
    let path = parts.next()?;
    let no = parts.next()?.parse::<u32>().ok()?;
    parts.next()?; // 内容
    Some((path.to_string(), no))
}

/// 名前を含む行（単語として）を探す。追跡されている・未追跡（`.gitignore` 外）のテキストファイル。
fn grep(root: &Path, name: &str) -> Result<Vec<(String, u32)>, String> {
    let child = proc::spawn(
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["-c", "core.quotepath=false", "grep", "-n", "-w", "-F", "-I", "--untracked", "--no-color", "-e", name])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .map_err(|e| format!("git を起動できません: {e}"))?;
    let out = proc::wait_with_timeout(child, GREP_TIMEOUT).map_err(|e| match e {
        proc::Wait::Timeout => format!("git grep が {} 秒で終わらなかった", GREP_TIMEOUT.as_secs()),
        proc::Wait::Io(e) => e,
    })?;
    match out.status.code() {
        Some(0) => {}
        Some(1) => return Ok(Vec::new()), // 該当なし
        _ => return Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
    }
    Ok(String::from_utf8_lossy(&out.stdout).lines().filter_map(parse_grep_line).collect())
}

fn numbered(lines: &[&str], start: u32) -> String {
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let shown: String = if l.chars().count() > MAX_LINE_CHARS { l.chars().take(MAX_LINE_CHARS).collect::<String>() + "…" } else { (*l).to_string() };
            format!("{:>5} | {}\n", start as usize + i, shown)
        })
        .collect()
}

/// 同じファイルの近い使用箇所を、1 つの範囲にまとめる。
fn merge_ranges(mut hits: Vec<u32>, ctx: u32, total: u32) -> Vec<(u32, u32)> {
    hits.sort_unstable();
    hits.dedup();
    let mut out: Vec<(u32, u32)> = Vec::new();
    for h in hits {
        let (s, e) = (h.saturating_sub(ctx).max(1), (h + ctx).min(total.max(1)));
        match out.last_mut() {
            Some(last) if s <= last.1 + 1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

/// 差分で変更した名前の、差分に出ていない使用箇所を集める。
pub fn collect(root: &Path, snap: &Snapshot, limits: &Limits) -> Related {
    let mut rel = Related::default();
    let shown = shown_ranges(&snap.files);
    let candidates = names(&snap.files);
    if candidates.is_empty() {
        rel.notes.push("関連コード: 差分から、調べる名前が見つかりませんでした".into());
        return rel;
    }
    let mut budget = limits.max_bytes;
    let mut budget_hit = false;

    for (name, seen) in candidates.into_iter().take(limits.max_names) {
        let why = why(&seen);
        let hits = match grep(root, &name) {
            Ok(h) => h,
            Err(e) => {
                rel.notes.push(format!("関連コード: 「{name}」を探せませんでした（{e}）"));
                continue;
            }
        };
        // 差分にすでに出ている範囲・ロックファイルは除く
        let mut outside: Vec<(String, u32)> = hits
            .into_iter()
            .filter(|(p, n)| {
                let base = p.rsplit('/').next().unwrap_or(p);
                !limits.lockfiles.iter().any(|l| l == base) && !shown.get(p).is_some_and(|rs| rs.iter().any(|(s, e)| n >= s && n <= e))
            })
            .collect();
        outside.sort();
        outside.dedup();
        let total = outside.len();
        let mut report = NameReport { name: name.clone(), why, hits: total, status: "sent".into() };
        if total == 0 {
            report.status = "none".into();
            rel.names.push(report);
            continue;
        }
        if total > limits.skip_over_hits {
            report.status = "too_many".into();
            rel.notes.push(format!("関連コード: 「{name}」は使用箇所が {total} か所と多く、手がかりにならないので除きました"));
            rel.names.push(report);
            continue;
        }
        if budget_hit {
            report.status = "budget".into();
            rel.names.push(report);
            continue;
        }

        // 先頭から hits_per_name 件までを、ファイルごとにまとめて、前後つきで読む
        let mut by_file: Vec<(String, Vec<u32>)> = Vec::new();
        for (p, n) in outside.into_iter().take(limits.hits_per_name) {
            match by_file.last_mut() {
                Some((lp, ns)) if *lp == p => ns.push(n),
                _ => by_file.push((p, vec![n])),
            }
        }
        let mut sent_any = false;
        'files: for (file, lines_hit) in by_file {
            let Some(Some(content)) = carry::read_in_repo(root, &file) else { continue };
            let all: Vec<&str> = content.lines().collect();
            for (s, e) in merge_ranges(lines_hit, limits.context_lines as u32, all.len() as u32) {
                let body = numbered(&all[(s as usize - 1).min(all.len())..(e as usize).min(all.len())], s);
                if body.len() > budget {
                    budget_hit = true;
                    break 'files;
                }
                budget -= body.len();
                rel.snippets.push(Snippet { name: name.clone(), file: file.clone(), start: s, end: e, text: body });
                sent_any = true;
            }
        }
        if !sent_any {
            report.status = if budget_hit { "budget".into() } else { "none".into() };
        }
        rel.names.push(report);
    }
    if budget_hit {
        rel.notes.push(format!("関連コード: 合計の上限（{} バイト）に達したので、残りの使用箇所は送りませんでした", limits.max_bytes));
    }
    let sent = rel.snippets.len();
    if sent == 0 && !rel.notes.iter().any(|n| n.contains("探せませんでした")) {
        rel.notes.push("関連コード: 差分に出ていない使用箇所は見つかりませんでした".into());
    }
    rel
}

/// プロンプトに入れる、関連コードの節。無ければ空。
/// 差分より**前**に置く（画面の差分タブが、差分をプロンプトの最後から読むため）。
pub fn render(rel: &Related) -> String {
    if rel.snippets.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "## 関連するコード（差分に出ていない箇所）\n\n\
         差分で変更した名前が、ほかの場所でどう使われているかです。差分と食い違っていないかを見る材料で、**ここにあるコード自体は、レビューの対象ではありません。**\n\
         指摘は、差分に出ている行に付けてください（`evidence` も、差分の行を引用します）。食い違っている場所は、ファイル名と行番号で、`detail` に書いてください。\n",
    );
    let mut current = "";
    for s in &rel.snippets {
        if s.name != current {
            current = &s.name;
            let why = rel.names.iter().find(|n| n.name == s.name).map(|n| n.why.as_str()).unwrap_or("");
            out.push_str(&format!("\n### `{}`（{}）\n", s.name, why));
        }
        out.push_str(&format!("\n`{}` {}〜{} 行\n~~~~\n{}~~~~\n", s.file, s.start, s.end, s.text));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{self, FileDiff};

    fn fd(path: &str, text: &str) -> FileDiff {
        FileDiff { path: path.into(), bytes: text.len(), text: text.into() }
    }

    const SIG_DIFF: &str = "diff --git a/lib.js b/lib.js\n--- a/lib.js\n+++ b/lib.js\n@@ -1,4 +1,4 @@\n-export function computeArea(width, height) {\n-  return width * height;\n+export function computeArea(size) {\n+  return size * size;\n }\n const MAX_SIZE = 3;\n";

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C").arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
            .args(args)
            .status().unwrap().success();
        assert!(ok, "git {args:?}");
    }

    fn repo(tag: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mrcr-related-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        for (p, c) in files {
            if let Some(parent) = Path::new(p).parent() {
                std::fs::create_dir_all(dir.join(parent)).unwrap();
            }
            std::fs::write(dir.join(p), c).unwrap();
        }
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "init"]);
        dir
    }

    fn snap_of(dir: &Path) -> Snapshot {
        snapshot::take(dir.to_str().unwrap()).unwrap()
    }

    #[test]
    fn names_prefer_removed_and_defined_and_skip_keywords_and_short_words() {
        let n = names(&[fd("lib.js", SIG_DIFF)]);
        let order: Vec<&str> = n.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(order[0], "computeArea", "削除と追加の両方にあり、定義らしい行 → 最優先");
        assert!(order.contains(&"width") && order.contains(&"height") && order.contains(&"size") == false, "size はありふれた語として除く");
        assert!(!order.contains(&"function") && !order.contains(&"export") && !order.contains(&"return"), "キーワードは除く");
        assert!(!order.contains(&"MAX_SIZE"), "変更の無い行（前後）の名前は拾わない");
        let (_, s) = &n[0];
        assert!(s.removed && s.added && s.definition);
        // 短い語・数字だけの語は拾わない
        let n2 = names(&[fd("a.js", "@@ -0,0 +1 @@\n+let a = 1234 + ab;\n")]);
        assert!(n2.is_empty(), "{n2:?}");
    }

    #[test]
    fn shown_ranges_follow_the_new_side_of_each_hunk() {
        let diff = "diff --git a/a.js b/a.js\n--- a/a.js\n+++ b/a.js\n@@ -1,2 +1,3 @@\n a\n+b\n c\n@@ -50 +60,0 @@\n-x\n@@ -70,2 +80 @@\n-y\n z\n";
        let r = shown_ranges(&[fd("a.js", diff)]);
        assert_eq!(r["a.js"], vec![(1, 3), (80, 80)], "追加が 0 行のハンクは範囲に入れない。行数が省かれていれば 1 行");
    }

    #[test]
    fn grep_lines_parse_even_when_the_content_has_colons() {
        assert_eq!(parse_grep_line("src/a.ts:12:const x = {a: 1};"), Some(("src/a.ts".into(), 12)));
        assert_eq!(parse_grep_line("garbage"), None);
        assert_eq!(parse_grep_line("a:b:c"), None);
    }

    #[test]
    fn ranges_merge_nearby_hits_and_clamp_to_the_file() {
        assert_eq!(merge_ranges(vec![10, 12, 40], 3, 100), vec![(7, 15), (37, 43)]);
        assert_eq!(merge_ranges(vec![1], 3, 5), vec![(1, 4)]);
        assert_eq!(merge_ranges(vec![5, 5], 0, 9), vec![(5, 5)]);
        assert_eq!(merge_ranges(vec![10, 17], 3, 100), vec![(7, 20)], "隣り合う範囲（1 行の隙間もない）はまとめる");
    }

    #[test]
    fn collects_callers_outside_the_diff_with_context_and_line_numbers() {
        let dir = repo("callers", &[
            ("lib.js", "export function computeArea(width, height) {\n  return width * height;\n}\nconst MAX_SIZE = 3;\n"),
            ("main.js", "import { computeArea } from './lib.js';\n\nconst a = computeArea(2, 3);\nconsole.log(a);\n"),
            ("report.js", "// report\nexport const total = computeArea(4, 5) + computeArea(6, 7);\n"),
        ]);
        std::fs::write(dir.join("lib.js"), "export function computeArea(size) {\n  return size * size;\n}\nconst MAX_SIZE = 3;\n").unwrap();
        let snap = snap_of(&dir);
        let rel = collect(&dir, &snap, &Limits::default());

        let files: Vec<&str> = rel.snippets.iter().filter(|s| s.name == "computeArea").map(|s| s.file.as_str()).collect();
        assert_eq!(files, vec!["main.js", "report.js"], "差分に出ているファイル（lib.js）の、差分の範囲内は送らない");
        let main = rel.snippets.iter().find(|s| s.file == "main.js").unwrap();
        assert!(main.text.contains("    3 | const a = computeArea(2, 3);"), "{}", main.text);
        assert_eq!((main.start, main.end), (1, 4), "前後 3 行。ファイルの端で止まる");
        let report = rel.snippets.iter().find(|s| s.file == "report.js").unwrap();
        assert_eq!((report.start, report.end), (1, 2), "同じ行の 2 つの使用箇所は 1 つにまとまる");
        let r = rel.names.iter().find(|n| n.name == "computeArea").unwrap();
        assert_eq!((r.status.as_str(), r.hits), ("sent", 3), "main.js の import と呼び出し、report.js の 1 行（同じ行は 1 つ）: {r:?}");
        assert!(r.why.contains("変更された名前") && r.why.contains("定義らしい行"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn untracked_files_outside_the_diff_are_searched_and_lockfiles_ignored() {
        let dir = repo("untracked", &[
            ("lib.js", "export function computeArea(width, height) {\n  return width * height;\n}\n"),
            ("package-lock.json", "{ \"computeArea\": 1 }\n"),
        ]);
        std::fs::write(dir.join("lib.js"), "export function computeArea(size) {\n  return size * size;\n}\n").unwrap();
        // 小さな未追跡ファイルは、全体が差分に入る（= 差分に出ている）ので、関連コードとしては送らない
        std::fs::write(dir.join("small_user.js"), "const v = computeArea(1, 2);\n").unwrap();
        // 大きすぎて差分から外れた未追跡ファイルは、関連コードとして探す
        let big = format!("const w = computeArea(3, 4);\n{}", "// padding\n".repeat(8_000));
        std::fs::write(dir.join("big_user.js"), big).unwrap();
        let snap = snap_of(&dir);
        assert!(snap.files.iter().any(|f| f.path == "small_user.js") && !snap.files.iter().any(|f| f.path == "big_user.js"));
        let rel = collect(&dir, &snap, &Limits::default());
        let files: Vec<&str> = rel.snippets.iter().map(|s| s.file.as_str()).collect();
        assert!(files.contains(&"big_user.js"), "差分から外れた未追跡ファイルは探す: {files:?}");
        assert!(!files.contains(&"small_user.js"), "差分に全体が出ているファイルは、重ねて送らない: {files:?}");
        assert!(!files.contains(&"package-lock.json"), "ロックファイルは除く: {files:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn common_names_are_skipped_and_reported_not_silently_dropped() {
        let many: String = (0..60).map(|i| format!("computeArea({i}, 1);\n")).collect();
        let dir = repo("common", &[
            ("lib.js", "export function computeArea(width, height) {\n  return width * height;\n}\n"),
            ("calls.js", &many),
        ]);
        std::fs::write(dir.join("lib.js"), "export function computeArea(size) {\n  return size * size;\n}\n").unwrap();
        let rel = collect(&dir, &snap_of(&dir), &Limits::default());
        let r = rel.names.iter().find(|n| n.name == "computeArea").unwrap();
        assert_eq!(r.status, "too_many");
        assert_eq!(r.hits, 60);
        assert!(rel.snippets.iter().all(|s| s.name != "computeArea"));
        assert!(rel.notes.iter().any(|n| n.contains("computeArea") && n.contains("60 か所") && n.contains("除きました")), "{:?}", rel.notes);
        // 上限を上げれば、送る
        let rel2 = collect(&dir, &snap_of(&dir), &Limits { skip_over_hits: 100, ..Limits::default() });
        assert!(rel2.snippets.iter().any(|s| s.name == "computeArea"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn limits_cap_hits_per_name_bytes_and_names() {
        let calls: String = (0..30).map(|i| format!("const v{i} = computeArea({i}, 1);\n\n\n\n\n\n\n\n\n")).collect();
        let dir = repo("limits", &[
            ("lib.js", "export function computeArea(width, height) {\n  return width * height;\n}\n"),
            ("calls.js", &calls),
        ]);
        std::fs::write(dir.join("lib.js"), "export function computeArea(size) {\n  return size * size;\n}\n").unwrap();
        let snap = snap_of(&dir);
        // 1 つの名前あたり 2 件まで（離れているので、別々の範囲）
        let rel = collect(&dir, &snap, &Limits { hits_per_name: 2, ..Limits::default() });
        assert_eq!(rel.snippets.iter().filter(|s| s.name == "computeArea").count(), 2);
        // 合計の上限が小さければ、送らず、理由を残す
        let tiny = collect(&dir, &snap, &Limits { max_bytes: 50, ..Limits::default() });
        assert!(tiny.snippets.is_empty());
        assert!(tiny.notes.iter().any(|n| n.contains("上限（50 バイト）")), "{:?}", tiny.notes);
        assert!(tiny.names.iter().any(|n| n.status == "budget"), "{:?}", tiny.names);
        // 名前の数
        let one = collect(&dir, &snap, &Limits { max_names: 1, ..Limits::default() });
        assert_eq!(one.names.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_used_nowhere_else_is_reported_as_none() {
        let dir = repo("none", &[("lib.js", "export function onlyHere(a) {\n  return a;\n}\n")]);
        std::fs::write(dir.join("lib.js"), "export function onlyHere(a, b) {\n  return a + b;\n}\n").unwrap();
        let rel = collect(&dir, &snap_of(&dir), &Limits::default());
        assert!(rel.snippets.is_empty());
        assert_eq!(rel.names.iter().find(|n| n.name == "onlyHere").unwrap().status, "none");
        assert!(rel.notes.iter().any(|n| n.contains("見つかりませんでした")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn render_puts_each_name_under_its_own_heading_and_is_empty_without_snippets() {
        assert_eq!(render(&Related::default()), "");
        let rel = Related {
            names: vec![NameReport { name: "computeArea".into(), why: "差分で削除された名前".into(), hits: 2, status: "sent".into() }],
            snippets: vec![
                Snippet { name: "computeArea".into(), file: "main.js".into(), start: 1, end: 3, text: "    1 | a\n".into() },
                Snippet { name: "computeArea".into(), file: "report.js".into(), start: 4, end: 6, text: "    4 | b\n".into() },
            ],
            notes: vec![],
        };
        let t = render(&rel);
        assert!(t.starts_with("## 関連するコード（差分に出ていない箇所）"));
        assert!(t.contains("ここにあるコード自体は、レビューの対象ではありません"));
        assert_eq!(t.matches("### `computeArea`（差分で削除された名前）").count(), 1, "名前の見出しは 1 回だけ");
        assert!(t.contains("`main.js` 1〜3 行\n~~~~\n    1 | a\n~~~~"));
        assert!(t.contains("`report.js` 4〜6 行"));
    }
}
