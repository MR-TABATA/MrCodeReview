//! ツール実行型のレシピ（linter）。LLM を使わず、差分に出ているファイルに linter を走らせる。
//!
//! 結果は LLM の指摘と同じ `Finding` にするので、仕分け・コピー・引き継ぎ・差分への重ね描きが、そのまま使える。
//!
//! - **実行するコマンドは、組み込みの固定の形だけ。** リポジトリ側のレシピで、任意のコマンドは足せない
//!   （信頼できないリポジトリで、ボタンを押しただけでコマンドが走るのを防ぐ）。
//! - 残す指摘は、**今回の差分で追加・変更した行**にかかるものだけ。それ以外は件数だけ記録する。
//! - 引用（evidence）は、差分のその行そのもの。ファイルを読み直さないので、差分と必ず一致する。
//! - LLM は呼ばないので、トークンは 0。

use crate::findings::Finding;
use crate::linters::Custom;
use crate::proc;
use crate::provider::{LlmResponse, ProviderInfo, Usage};
use crate::recipe::{Kind, Recipe};
use crate::snapshot::{FileDiff, Snapshot};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// linter 1 つあたりの待ち時間の上限。ふつうは数秒〜数十秒で終わる（Tauri のアプリの Clippy が、初回から 22 秒）。遅いマシンや大きなプロジェクトでも、打ち切らないよう、余裕を持たせてある。
pub const TOOL_TIMEOUT: Duration = Duration::from_secs(900);
/// 1 回に linter へ渡すファイル数の上限（コマンドラインが長すぎて失敗しないように）。
pub const MAX_FILES: usize = 200;
/// 記録に残す生の出力の上限（clippy などは大きくなる）。
pub const MAX_RAW: usize = 1_000_000;

/// 実行の上限。設定（`settings.rs`）から作る。`Default` は推奨値。
#[derive(Debug, Clone)]
pub struct LintLimits {
    pub timeout: Duration,
    pub max_files: usize,
    pub max_raw: usize,
}

impl Default for LintLimits {
    fn default() -> Self {
        LintLimits { timeout: TOOL_TIMEOUT, max_files: MAX_FILES, max_raw: MAX_RAW }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Eslint,
    Ruff,
    Clippy,
    Swiftlint,
    Shellcheck,
}

impl Tool {
    pub const ALL: [Tool; 5] = [Tool::Eslint, Tool::Ruff, Tool::Clippy, Tool::Swiftlint, Tool::Shellcheck];

    pub fn id(self) -> &'static str {
        match self {
            Tool::Eslint => "lint-eslint",
            Tool::Ruff => "lint-ruff",
            Tool::Clippy => "lint-clippy",
            Tool::Swiftlint => "lint-swiftlint",
            Tool::Shellcheck => "lint-shellcheck",
        }
    }

    pub fn from_id(id: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.id() == id)
    }

    pub fn title(self) -> &'static str {
        match self {
            Tool::Eslint => "ESLint",
            Tool::Ruff => "Ruff",
            Tool::Clippy => "Clippy",
            Tool::Swiftlint => "SwiftLint",
            Tool::Shellcheck => "ShellCheck",
        }
    }

    fn exts(self) -> &'static [&'static str] {
        match self {
            Tool::Eslint => &["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"],
            Tool::Ruff => &["py"],
            Tool::Clippy => &["rs"],
            Tool::Swiftlint => &["swift"],
            Tool::Shellcheck => &["sh", "bash"],
        }
    }

    /// ボタンや一覧に出す、対象の言語。名前だけでは、何の linter か分からないので。
    pub fn lang(self) -> &'static str {
        match self {
            Tool::Eslint => "JavaScript / TypeScript",
            Tool::Ruff => "Python",
            Tool::Clippy => "Rust",
            Tool::Swiftlint => "Swift",
            Tool::Shellcheck => "シェルスクリプト",
        }
    }

    /// そのリポジトリのコードを動かすか（ESLint は設定ファイル（JS）、Clippy は `build.rs` とマクロ）。
    /// 動かすものは、リポジトリごとに、あなたが許可するまで走らせない（`trust.rs`）。
    pub fn runs_repo_code(self) -> bool {
        matches!(self, Tool::Eslint | Tool::Clippy)
    }

    /// この linter が見るファイルか。
    pub fn handles(self, path: &str) -> bool {
        path.rsplit_once('.').is_some_and(|(_, ext)| self.exts().contains(&ext))
    }
}

// ── 検出 ──────────────────────────────────────────────────────────────────────

/// 実行ファイルを探す。Finder から起動したアプリは PATH が短いので、よくある場所も見る（`proc.rs`）。
pub(crate) fn on_path(name: &str) -> Option<PathBuf> {
    proc::find_program(name)
}

/// リポジトリに、git が知っているファイル（追跡中と未追跡）で、`pathspecs` に合うものがあるか。
fn has_files(root: &Path, pathspecs: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--"])
        .args(pathspecs)
        .output()
        .is_ok_and(|o| !o.stdout.is_empty())
}

fn has_marker(root: &Path, markers: &[&str]) -> bool {
    markers.iter().any(|m| root.join(m).is_file())
}

fn has_python(root: &Path) -> bool {
    has_marker(root, &["pyproject.toml", "ruff.toml", ".ruff.toml", "setup.py", "setup.cfg", "requirements.txt"]) || has_files(root, &["*.py"])
}

fn has_swift(root: &Path) -> bool {
    has_marker(root, &["Package.swift", ".swiftlint.yml"]) || has_files(root, &["*.swift"])
}

fn has_shell(root: &Path) -> bool {
    has_files(root, &["*.sh", "*.bash"])
}

/// ESLint は、プロジェクトごとに入れるもの。使う意思が見えるとき（設定ファイルがある、`package.json` に名前がある）だけ、
/// 入っていなくても「入っていない」と知らせる。`package.json` があるだけの、ESLint を使わないプロジェクトには、何も出さない。
fn eslint_intended(root: &Path) -> bool {
    const CONFIGS: [&str; 10] = [
        "eslint.config.js",
        "eslint.config.mjs",
        "eslint.config.cjs",
        "eslint.config.ts",
        ".eslintrc",
        ".eslintrc.js",
        ".eslintrc.cjs",
        ".eslintrc.json",
        ".eslintrc.yml",
        ".eslintrc.yaml",
    ];
    has_marker(root, &CONFIGS) || std::fs::read_to_string(root.join("package.json")).is_ok_and(|t| t.contains("eslint"))
}

fn clippy_installed() -> bool {
    Command::new(proc::find_program("cargo").unwrap_or_else(|| PathBuf::from("cargo")))
        .args(["clippy", "--version"])
        .env("PATH", proc::augmented_path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

impl Tool {
    /// このリポジトリで、この linter の出番があるか（入っているかは問わない）。
    fn applies(self, root: &Path) -> bool {
        match self {
            Tool::Eslint => root.join("package.json").is_file() && (root.join("node_modules/.bin/eslint").is_file() || eslint_intended(root)),
            Tool::Ruff => has_python(root),
            Tool::Clippy => root.join("Cargo.toml").is_file() || has_files(root, &["Cargo.toml", "*/Cargo.toml"]),
            Tool::Swiftlint => has_swift(root),
            Tool::Shellcheck => has_shell(root),
        }
    }

    fn installed(self, root: &Path) -> bool {
        match self {
            Tool::Eslint => root.join("node_modules/.bin/eslint").is_file(),
            Tool::Ruff => on_path("ruff").is_some(),
            Tool::Clippy => clippy_installed(),
            Tool::Swiftlint => on_path("swiftlint").is_some(),
            Tool::Shellcheck => on_path("shellcheck").is_some(),
        }
    }

    /// 入っていないときに、画面に出す入れ方。
    fn install_hint(self) -> &'static str {
        match self {
            Tool::Eslint => "このプロジェクトで `npm install` する（`package.json` に ESLint が無ければ、`npm install --save-dev eslint`）",
            Tool::Ruff => "`brew install ruff`",
            Tool::Clippy => "`rustup component add clippy`",
            Tool::Swiftlint => "`brew install swiftlint`",
            Tool::Shellcheck => "`brew install shellcheck`",
        }
    }
}

/// このリポジトリで使える linter。
pub fn available(root: &Path) -> Vec<Tool> {
    Tool::ALL.into_iter().filter(|t| t.applies(root) && t.installed(root)).collect()
}

/// 出番はあるのに、入っていない、設定で足した linter（`linters.rs`）。入れ方を画面に出すためのもの。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Missing {
    pub id: String,
    pub title: String,
    pub hint: String,
}

// ── 実行するコマンド ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Invocation {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

impl Invocation {
    pub fn display(&self) -> String {
        std::iter::once(self.program.to_string_lossy().into_owned())
            .chain(self.args.iter().cloned())
            .map(|a| if a.contains(' ') { format!("'{a}'") } else { a })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// `files` は、この linter が見る、差分のファイル（リポジトリの根からの相対パス）。
pub fn invocation(tool: Tool, root: &Path, files: &[String]) -> Result<Invocation, String> {
    let (program, mut args): (PathBuf, Vec<String>) = match tool {
        Tool::Eslint => (
            root.join("node_modules/.bin/eslint"),
            ["--format", "json", "--no-error-on-unmatched-pattern", "--"].map(String::from).to_vec(),
        ),
        // --no-cache: リポジトリに .ruff_cache を作らない
        Tool::Ruff => (
            on_path("ruff").ok_or("ruff が見つかりません")?,
            ["check", "--output-format", "json", "--no-cache", "--"].map(String::from).to_vec(),
        ),
        Tool::Clippy => (
            proc::find_program("cargo").unwrap_or_else(|| PathBuf::from("cargo")),
            ["clippy", "--message-format=json", "--quiet"].map(String::from).to_vec(),
        ),
        Tool::Swiftlint => (
            on_path("swiftlint").ok_or("swiftlint が見つかりません")?,
            ["lint", "--quiet", "--reporter", "json", "--"].map(String::from).to_vec(),
        ),
        Tool::Shellcheck => (
            on_path("shellcheck").ok_or("shellcheck が見つかりません")?,
            ["-f", "json", "--"].map(String::from).to_vec(),
        ),
    };
    if tool != Tool::Clippy {
        args.extend(files.iter().cloned());
    }
    Ok(Invocation { program, args, cwd: root.to_path_buf() })
}

/// ボタンの説明に出す、実行するコマンドの形（ファイル名は、押したときの差分で決まる）。
fn command_template(tool: Tool, root: &Path) -> String {
    match invocation(tool, root, &["<差分のファイル>".to_string()]) {
        Ok(inv) => inv.display(),
        Err(e) => e,
    }
}

pub fn recipe(tool: Tool, root: &Path) -> Recipe {
    let caution = match tool {
        Tool::Clippy => "Clippy は、このリポジトリのビルドの定義（build.rs やマクロ）を実行し、target/ にビルドの成果物を書きます。",
        _ => "このリポジトリの設定ファイルを読み込んで実行します。",
    };
    let exts = tool.exts().join(", ");
    Recipe {
        id: tool.id().into(),
        title: format!("リンター: {}", tool.title()),
        source: "builtin".into(),
        kind: Kind::Tool,
        needs_related: false,
        prompt: format!(
            "LLM を使わず、差分に出ている {exts} のファイルに {} を実行します。トークンは使いません。\n\n\
             実行するコマンド:\n  {}\n  （作業ディレクトリ: リポジトリの根）\n\n\
             結果のうち、今回の差分で追加・変更した行への指摘だけを残します。それ以外の行への指摘は、件数だけ記録します。\n\n\
             ⚠ {caution}信頼できるリポジトリでだけ押してください。",
            tool.title(),
            command_template(tool, root),
        ),
    }
}

// ── 差分の変更行 ──────────────────────────────────────────────────────────────────

/// ファイルごとの、今回の差分で追加・変更した行（新側の行番号 → 行の内容）。
pub type Changed = HashMap<String, BTreeMap<u32, String>>;

pub fn changed_lines(files: &[FileDiff]) -> Changed {
    let mut out: Changed = HashMap::new();
    for f in files {
        let map = out.entry(f.path.clone()).or_default();
        let mut new_no: Option<u32> = None; // ハンクの中にいるときだけ Some
        for line in f.text.lines() {
            if let Some(rest) = line.strip_prefix("@@ ") {
                // `-a,b +c,d @@ ...`
                new_no = rest
                    .split_whitespace()
                    .find_map(|t| t.strip_prefix('+'))
                    .and_then(|t| t.split(',').next())
                    .and_then(|t| t.parse().ok());
                continue;
            }
            let Some(n) = new_no else { continue };
            match line.chars().next() {
                Some('+') => {
                    map.insert(n, line[1..].to_string());
                    new_no = Some(n + 1);
                }
                Some(' ') => new_no = Some(n + 1),
                _ => {} // '-' と "\ No newline at end of file"
            }
        }
    }
    out
}

// ── 出力の解析 ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub file: String,
    pub line: u32,
    pub rule: String,
    /// "high" | "medium" | "low"
    pub severity: String,
    pub message: String,
}

/// 絶対パスを、リポジトリの根からの相対パスにする。
pub(crate) fn rel(root: &Path, p: &str) -> String {
    let path = Path::new(p);
    if path.is_relative() {
        return p.trim_start_matches("./").to_string();
    }
    let canon = root.canonicalize().ok();
    path.strip_prefix(root)
        .ok()
        .or_else(|| canon.as_deref().and_then(|c| path.strip_prefix(c).ok()))
        .map(|r| r.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string())
}

pub fn parse_eslint(text: &str, root: &Path) -> Result<Vec<Issue>, String> {
    let v: Value = serde_json::from_str(text.trim()).map_err(|e| format!("ESLint の出力が JSON として読めない: {e}"))?;
    let files = v.as_array().ok_or("ESLint の出力が配列ではない")?;
    let mut out = Vec::new();
    for f in files {
        let file = rel(root, f.get("filePath").and_then(Value::as_str).unwrap_or(""));
        for m in f.get("messages").and_then(Value::as_array).into_iter().flatten() {
            let Some(line) = m.get("line").and_then(Value::as_u64) else { continue };
            out.push(Issue {
                file: file.clone(),
                line: line as u32,
                rule: m.get("ruleId").and_then(Value::as_str).unwrap_or("parse-error").to_string(),
                severity: if m.get("severity").and_then(Value::as_u64) == Some(2) { "high" } else { "medium" }.into(),
                message: m.get("message").and_then(Value::as_str).unwrap_or("").to_string(),
            });
        }
    }
    Ok(out)
}

pub fn parse_ruff(text: &str, root: &Path) -> Result<Vec<Issue>, String> {
    let v: Value = serde_json::from_str(text.trim()).map_err(|e| format!("Ruff の出力が JSON として読めない: {e}"))?;
    let items = v.as_array().ok_or("Ruff の出力が配列ではない")?;
    Ok(items
        .iter()
        .filter_map(|i| {
            let line = i.get("location")?.get("row")?.as_u64()?;
            let code = i.get("code").and_then(Value::as_str);
            Some(Issue {
                file: rel(root, i.get("filename").and_then(Value::as_str).unwrap_or("")),
                line: line as u32,
                rule: code.unwrap_or("syntax-error").to_string(),
                // Ruff に重大度は無い。構文エラーだけ high
                severity: if code.is_none() { "high" } else { "medium" }.into(),
                message: i.get("message").and_then(Value::as_str).unwrap_or("").to_string(),
            })
        })
        .collect())
}

/// `cargo clippy --message-format=json` は、1 行 1 JSON。コンパイラのメッセージ以外の行は飛ばす。
pub fn parse_clippy(text: &str, root: &Path) -> Result<Vec<Issue>, String> {
    parse_clippy_in(text, root, root)
}

/// `base` は、出力の相対パスの基準（cargo が rustc を走らせる、ワークスペースの根）。`root` は、リポジトリの根。
/// Tauri のように、`Cargo.toml` が `src-tauri/` にあるときは、`src/lib.rs` を `src-tauri/src/lib.rs` に直す。
pub fn parse_clippy_in(text: &str, root: &Path, base: &Path) -> Result<Vec<Issue>, String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v.get("reason").and_then(Value::as_str) != Some("compiler-message") {
            continue;
        }
        let Some(m) = v.get("message") else { continue };
        let level = m.get("level").and_then(Value::as_str).unwrap_or("");
        if !matches!(level, "warning" | "error") {
            continue;
        }
        let Some(span) = m
            .get("spans")
            .and_then(Value::as_array)
            .and_then(|s| s.iter().find(|s| s.get("is_primary").and_then(Value::as_bool) == Some(true)))
        else {
            continue; // 「N warnings emitted」のような、場所の無いメッセージ
        };
        let (Some(file), Some(line)) = (span.get("file_name").and_then(Value::as_str), span.get("line_start").and_then(Value::as_u64)) else {
            continue;
        };
        let abs = if Path::new(file).is_absolute() { file.to_string() } else { base.join(file).to_string_lossy().into_owned() };
        out.push(Issue {
            file: rel(root, &abs),
            line: line as u32,
            rule: m.get("code").and_then(|c| c.get("code")).and_then(Value::as_str).unwrap_or("rustc").to_string(),
            severity: if level == "error" { "high" } else { "medium" }.into(),
            message: m.get("message").and_then(Value::as_str).unwrap_or("").to_string(),
        });
    }
    Ok(out)
}

/// `swiftlint lint --reporter json`。ファイル全体への指摘は `line` が null なので、行のある指摘だけ読む。
pub fn parse_swiftlint(text: &str, root: &Path) -> Result<Vec<Issue>, String> {
    let v: Value = serde_json::from_str(text.trim()).map_err(|e| format!("SwiftLint の出力が JSON として読めない: {e}"))?;
    let items = v.as_array().ok_or("SwiftLint の出力が配列ではない")?;
    Ok(items
        .iter()
        .filter_map(|i| {
            let line = i.get("line")?.as_u64()?;
            Some(Issue {
                file: rel(root, i.get("file")?.as_str()?),
                line: line as u32,
                rule: i.get("rule_id").and_then(Value::as_str).unwrap_or("swiftlint").to_string(),
                severity: if i.get("severity").and_then(Value::as_str).is_some_and(|s| s.eq_ignore_ascii_case("error")) { "high" } else { "medium" }.into(),
                message: i.get("reason").and_then(Value::as_str).unwrap_or("").to_string(),
            })
        })
        .collect())
}

/// `shellcheck -f json`。`level` は error / warning / info / style。
pub fn parse_shellcheck(text: &str, root: &Path) -> Result<Vec<Issue>, String> {
    let v: Value = serde_json::from_str(text.trim()).map_err(|e| format!("ShellCheck の出力が JSON として読めない: {e}"))?;
    let items = v.as_array().ok_or("ShellCheck の出力が配列ではない")?;
    Ok(items
        .iter()
        .filter_map(|i| {
            let line = i.get("line")?.as_u64()?;
            Some(Issue {
                file: rel(root, i.get("file")?.as_str()?),
                line: line as u32,
                rule: i.get("code").and_then(Value::as_u64).map(|c| format!("SC{c}")).unwrap_or_else(|| "shellcheck".into()),
                severity: match i.get("level").and_then(Value::as_str) {
                    Some("error") => "high",
                    Some("warning") => "medium",
                    _ => "low",
                }
                .into(),
                message: i.get("message").and_then(Value::as_str).unwrap_or("").to_string(),
            })
        })
        .collect())
}

pub fn parse(tool: Tool, text: &str, root: &Path) -> Result<Vec<Issue>, String> {
    match tool {
        Tool::Eslint => parse_eslint(text, root),
        Tool::Ruff => parse_ruff(text, root),
        Tool::Clippy => parse_clippy(text, root),
        Tool::Swiftlint => parse_swiftlint(text, root),
        Tool::Shellcheck => parse_shellcheck(text, root),
    }
}

/// 指摘のうち、今回の差分で追加・変更した行にかかるものだけを `Finding` にする。戻り値の数は、除外した件数。
#[cfg_attr(not(test), allow(dead_code))]
pub fn to_findings(tool: Tool, issues: Vec<Issue>, changed: &Changed) -> (Vec<Finding>, usize) {
    to_findings_titled(tool.title(), issues, changed)
}

/// `title` は、指摘の説明に添える linter の名前（組み込みでも、設定で足したものでも）。
pub fn to_findings_titled(title: &str, mut issues: Vec<Issue>, changed: &Changed) -> (Vec<Finding>, usize) {
    issues.sort_by(|a, b| (&a.file, a.line, &a.rule, &a.message).cmp(&(&b.file, b.line, &b.rule, &b.message)));
    issues.dedup();
    let mut findings = Vec::new();
    let mut dropped = 0;
    for i in issues {
        match changed.get(&i.file).and_then(|m| m.get(&i.line)) {
            Some(text) => findings.push(Finding {
                file: i.file,
                line: Some(i.line),
                severity: i.severity,
                title: i.message.lines().next().unwrap_or("").trim().to_string(),
                detail: format!("{title} {}", i.rule),
                evidence: text.trim().to_string(),
                verified: false, // 呼び出し側の `findings::verify` が、差分との一致を確かめて埋める
                rule: Some(i.rule),
            }),
            None => dropped += 1,
        }
    }
    (findings, dropped)
}

// ── 実行 ──────────────────────────────────────────────────────────────────────────

/// 記録に残す「実行したもの」の情報。LLM ではないので、プロバイダの欄はこれを使う。
pub fn provider_info() -> ProviderInfo {
    ProviderInfo {
        id: "tool".into(),
        label: "ツール実行（LLM を使わない）".into(),
        note: "LLM は呼びません。トークンは 0 です。実行したコマンドと差分は「送った内容」に、linter の生の出力は「応答」に残ります。".into(),
    }
}

pub struct ToolRun {
    pub response: LlmResponse,
    /// 「送った内容」の、差分より前の部分（実行したコマンドと対象ファイル）。
    pub head: String,
    /// 記録に残す「送った内容」。実行したコマンドと、差分の全文（差分タブが読む）。
    pub prompt: String,
    /// 画面に出す実行メモ（除外した件数など）。
    pub notes: Vec<String>,
}

struct Exec {
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

fn exec(inv: &Invocation, timeout: Duration) -> Result<Exec, String> {
    let child = proc::spawn(
        Command::new(&inv.program)
            .args(&inv.args)
            .current_dir(&inv.cwd)
            .env("CARGO_TERM_PROGRESS_WHEN", "never") // cargo の進捗バーは、画面に出す標準エラーを埋めてしまう
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .map_err(|e| format!("{} を起動できません: {e}", inv.program.display()))?;
    let out = proc::wait_with_timeout(child, timeout).map_err(|e| match e {
        proc::Wait::Timeout => format!("{} 秒で終わらなかった（中断した）", timeout.as_secs()),
        proc::Wait::Io(e) => format!("実行に失敗: {e}"),
    })?;
    Ok(Exec {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        code: out.status.code(),
    })
}

fn cap(mut s: String, max_raw: usize) -> String {
    if s.len() > max_raw {
        let mut end = max_raw;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
        s.push_str("\n…（長いので、ここで切った）");
    }
    s
}

/// 実行する linter。組み込みか、あなたの `linters.json` で足したもの。
enum Linter<'a> {
    Builtin(Tool),
    Custom(&'a Custom),
}

impl<'a> Linter<'a> {
    fn find(recipe_id: &str, custom: &'a [Custom]) -> Option<Linter<'a>> {
        Tool::from_id(recipe_id)
            .map(Linter::Builtin)
            .or_else(|| custom.iter().find(|c| c.recipe_id() == recipe_id).map(Linter::Custom))
    }

    fn title(&self) -> String {
        match self {
            Linter::Builtin(t) => t.title().to_string(),
            Linter::Custom(c) => c.title().to_string(),
        }
    }

    fn handles(&self, path: &str) -> bool {
        match self {
            Linter::Builtin(t) => t.handles(path),
            Linter::Custom(c) => c.handles(path),
        }
    }

    fn exts(&self) -> String {
        match self {
            Linter::Builtin(t) => t.exts().join(", "),
            Linter::Custom(c) => c.exts_text(),
        }
    }

    /// 差分のファイルを渡さず、プロジェクト全体を見るか（Clippy など）。
    fn whole_project(&self) -> bool {
        match self {
            Linter::Builtin(t) => *t == Tool::Clippy,
            Linter::Custom(c) => c.whole_project(),
        }
    }

    fn invocation(&self, root: &Path, files: &[String]) -> Result<Invocation, String> {
        match self {
            Linter::Builtin(t) => invocation(*t, root, files),
            Linter::Custom(c) => c.invocation(root, files),
        }
    }

    /// 出力から、指摘と、実行メモを読む。
    fn read(&self, ex: &Exec, root: &Path) -> Result<(Vec<Issue>, Vec<String>), String> {
        match self {
            Linter::Builtin(t) => match parse(*t, &ex.stdout, root) {
                Ok(v) => Ok((v, Vec::new())),
                // 出力が読めない。終了コードや標準エラーに理由があるので、それを添える
                Err(e) => Err(read_failure(e, ex)),
            },
            Linter::Custom(c) => c.read(&ex.stdout, &ex.stderr, ex.code, root),
        }
    }
}

/// この linter が、差分のファイルのどれかを対象にするか。無ければ、実行せずに断る（記録も残さない）。
pub fn check_targets(recipe_id: &str, custom: &[Custom], files: &[FileDiff]) -> Result<(), String> {
    if recipe_id == LINT_ID {
        let any = |l: &Linter| files.iter().any(|f| l.handles(&f.path));
        let found = Tool::ALL.iter().any(|t| any(&Linter::Builtin(*t))) || custom.iter().any(|c| any(&Linter::Custom(c)));
        return if found {
            Ok(())
        } else {
            let exts: Vec<String> = Tool::ALL.iter().map(|t| format!("{}（{}）", t.title(), t.lang())).collect();
            Err(format!("差分に、linter が見るファイルがありません（見るのは: {}。設定で足した linter も含む）", exts.join("、")))
        };
    }
    let linter = Linter::find(recipe_id, custom).ok_or_else(|| format!("ツールが見つからない: {recipe_id}"))?;
    if files.iter().any(|f| linter.handles(&f.path)) {
        Ok(())
    } else {
        Err(format!("差分に {} の対象ファイルがありません", linter.title()))
    }
}


// ── 「リンター」ボタン: 差分に合う linter を、全部まとめて実行する ───────────────────────────

/// ボタンになるレシピの id。押すと、差分に合う linter を全部実行する。
/// 1 つずつの linter の id（`lint-ruff` など）は、記録と内部の実行に残るだけで、ボタンにはならない。
pub const LINT_ID: &str = "lint";

/// 画面に出す、linter 1 つの状態。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanItem {
    pub id: String,
    pub title: String,
    /// 対象の言語
    pub lang: String,
    /// 見るファイルの拡張子（今回の差分に対象があるかを、画面が判断する）
    pub exts: Vec<String>,
    /// 実行するコマンドの形（入っていないときは空）
    pub command: String,
    /// そのリポジトリのコードを動かすか（許可が要る）
    pub runs_repo_code: bool,
    /// "ready" = 実行できる / "needs_trust" = 入っているが、許可が要る / "missing" = 入っていない
    pub state: String,
    /// 入っていないときの入れ方
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub items: Vec<PlanItem>,
    /// このリポジトリで、コードを動かす linter を許可しているか
    pub trusted: bool,
}

/// このリポジトリで出番のある linter と、その状態。差分を見る前の、リポジトリ全体での見立て。
pub fn plan(root: &Path, custom: &[Custom], trusted: bool) -> Plan {
    let mut items = Vec::new();
    for t in Tool::ALL.into_iter().filter(|t| t.applies(root)) {
        let installed = t.installed(root);
        let state = if !installed {
            "missing"
        } else if t.runs_repo_code() && !trusted {
            "needs_trust"
        } else {
            "ready"
        };
        items.push(PlanItem {
            id: t.id().into(),
            title: t.title().into(),
            lang: t.lang().into(),
            exts: t.exts().iter().map(|e| e.to_string()).collect(),
            command: if installed { command_template(t, root) } else { String::new() },
            runs_repo_code: t.runs_repo_code(),
            state: state.into(),
            hint: (!installed).then(|| t.install_hint().to_string()),
        });
    }
    for c in custom {
        let lang = c.exts_text();
        let exts: Vec<String> = lang.split(", ").map(str::to_string).collect();
        if c.available(root) {
            let command = c.invocation(root, &["<差分のファイル>".to_string()]).map(|i| i.display()).unwrap_or_default();
            items.push(PlanItem { id: c.recipe_id(), title: c.title().into(), lang, exts, command, runs_repo_code: false, state: "ready".into(), hint: None });
        } else if let Some(m) = c.missing(root) {
            items.push(PlanItem { id: m.id, title: m.title, lang, exts, command: String::new(), runs_repo_code: false, state: "missing".into(), hint: Some(m.hint) });
        }
    }
    Plan { items, trusted }
}

/// 「リンター」ボタンのレシピ。実行するものは、押した時点の差分で決まる。
pub fn lint_recipe(plan: &Plan) -> Recipe {
    let runnable: Vec<String> = plan
        .items
        .iter()
        .filter(|i| i.state != "missing")
        .map(|i| format!("  - {}（{}）: {}", i.title, i.lang, i.command))
        .collect();
    Recipe {
        id: LINT_ID.into(),
        title: "リンター".into(),
        source: "builtin".into(),
        kind: Kind::Tool,
        needs_related: false,
        prompt: format!(
            "LLM を使わず、差分に出ているファイルに合う linter を、全部実行します。トークンは使いません。\n\n\
             このリポジトリで使える linter（実行するコマンド）:\n{}\n\n\
             差分に、その linter が見るファイルが無ければ、その linter は実行しません。\n\
             結果のうち、今回の差分で追加・変更した行への指摘だけを残します。それ以外の行への指摘は、件数だけ記録します。\n\n\
             ⚠ ESLint と Clippy は、そのリポジトリのコード（ESLint の設定ファイル、Clippy の `build.rs`）を動かします。\
             信頼できるリポジトリで、あなたが許可したときだけ実行します。",
            runnable.join("\n"),
        ),
    }
}

/// 差分に合う linter を全部実行して、1 つの結果にまとめる。
/// 入っていない・許可が無い・失敗した linter があっても、ほかは実行し、そのことを実行メモに書く（黙って飛ばさない）。
/// 1 つも実行できなかったときだけ、エラー。
pub fn run_all(snap: &Snapshot, limits: &LintLimits, custom: &[Custom], trusted: bool) -> Result<ToolRun, String> {
    let root = Path::new(&snap.repo);
    let linters: Vec<Linter> = Tool::ALL.iter().map(|t| Linter::Builtin(*t)).chain(custom.iter().map(Linter::Custom)).collect();
    let mut notes: Vec<String> = Vec::new();
    let mut runs: Vec<(String, ToolRun)> = Vec::new();
    for l in &linters {
        if !snap.files.iter().any(|f| l.handles(&f.path)) {
            continue;
        }
        let title = l.title();
        match l {
            Linter::Builtin(t) => {
                if !t.applies(root) {
                    notes.push(format!("{title}（{}）は、このリポジトリで使う形になっていないので、実行していません。使うには: {}", t.lang(), t.install_hint()));
                    continue;
                }
                if !t.installed(root) {
                    notes.push(format!("{title}（{}）は、入っていないので、実行していません。入れ方: {}", t.lang(), t.install_hint()));
                    continue;
                }
                if t.runs_repo_code() && !trusted {
                    notes.push(format!("{title}（{}）は、このリポジトリのコードを動かすので、許可が無く、実行していません。", t.lang()));
                    continue;
                }
            }
            Linter::Custom(c) => {
                if !c.available(root) {
                    if let Some(m) = c.missing(root) {
                        notes.push(format!("{title} は、入っていないので、実行していません。入れ方: {}", m.hint));
                    }
                    continue;
                }
            }
        }
        match run_linter(l, snap, limits) {
            Ok(run) => runs.push((title, run)),
            Err(e) => notes.push(format!("{title}: 実行できませんでした: {e}")),
        }
    }
    if runs.is_empty() {
        let why = if notes.is_empty() {
            let mut exts: Vec<String> = snap.files.iter().filter_map(|f| f.path.rsplit_once('.').map(|(_, e)| format!(".{e}"))).collect();
            exts.sort();
            exts.dedup();
            format!(
                "今回の差分のファイル（{exts}）を見る linter が、組み込みにも `{file}` にもありません。\n\
                 どうするか: ① このファイル向けの観点（bugs・security など）を使う。② 見たい linter があれば、`{file}` に追加する。",
                exts = if exts.is_empty() { "拡張子なし".to_string() } else { exts.join(" ") },
                file = crate::linters::FILE
            )
        } else {
            format!("{}\nどうするか: 上の「使うには／入れ方」のとおりに入れて、もう一度実行する。", notes.join("\n"))
        };
        return Err(format!("実行できる linter がありませんでした。\n{why}"));
    }

    let mut findings: Vec<Value> = Vec::new();
    let mut dropped: u64 = 0;
    let (mut heads, mut raws) = (Vec::new(), Vec::new());
    for (title, run) in &runs {
        let v: Value = serde_json::from_str(&run.response.text).map_err(|e| format!("{title} の結果をまとめられません: {e}"))?;
        findings.extend(v.get("findings").and_then(Value::as_array).cloned().unwrap_or_default());
        dropped += v.get("excluded_untouched_lines").and_then(Value::as_u64).unwrap_or(0);
        heads.push(run.head.clone());
        raws.push(format!("===== {title} =====\n{}", run.response.raw));
        notes.extend(run.notes.iter().map(|n| format!("{title}: {n}")));
    }
    let titles: Vec<&str> = runs.iter().map(|(t, _)| t.as_str()).collect();
    let model = titles.join(" + ");
    let text = serde_json::json!({ "tool": model, "tools": titles, "excluded_untouched_lines": dropped, "findings": findings }).to_string();
    Ok(ToolRun {
        response: LlmResponse {
            text,
            model,
            usage: Usage { input_tokens: Some(0), output_tokens: Some(0), cache_read_tokens: Some(0), cache_write_tokens: Some(0) },
            cost_usd: None,
            raw: raws.join("\n\n"),
        },
        prompt: format!("{}\n## 差分\n\n```diff\n{}```\n", heads.join("\n---\n\n"), snap.diff_text),
        head: heads.join("\n---\n\n"),
        notes,
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn run(recipe_id: &str, snap: &Snapshot) -> Result<ToolRun, String> {
    run_with(recipe_id, snap, &LintLimits::default(), &[])
}

pub fn run_with(recipe_id: &str, snap: &Snapshot, limits: &LintLimits, custom: &[Custom]) -> Result<ToolRun, String> {
    let linter = Linter::find(recipe_id, custom).ok_or_else(|| format!("ツールが見つからない: {recipe_id}"))?;
    run_linter(&linter, snap, limits)
}

/// 変更したファイルに、いちばん近い `Cargo.toml` のある場所（リポジトリの根からの相対パス。根なら空）。
pub(crate) fn clippy_crate_dir(root: &Path, file: &str) -> Option<String> {
    let mut p = Path::new(file).parent();
    while let Some(d) = p {
        if root.join(d).join("Cargo.toml").is_file() {
            return Some(d.to_string_lossy().into_owned());
        }
        p = d.parent();
    }
    None
}

/// 標準エラーの、意味のある行だけ。`\r` で上書きされる進捗（`[====>   ] 3/9: …`）は、落とす。
fn stderr_lines(stderr: &str) -> Vec<&str> {
    stderr
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|l| !(l.is_empty() || l.starts_with('[') && l.contains("] ") && l.contains('/')))
        .collect()
}

/// cargo が rustc を走らせる場所（ワークスペースの根）。調べられなければ、クレートの場所。
fn workspace_root(crate_dir: &Path) -> PathBuf {
    Command::new(proc::find_program("cargo").unwrap_or_else(|| PathBuf::from("cargo")))
        .args(["locate-project", "--workspace", "--message-format", "plain"])
        .current_dir(crate_dir)
        .env("PATH", proc::augmented_path())
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()).parent().map(Path::to_path_buf))
        .unwrap_or_else(|| crate_dir.to_path_buf())
}

/// 実行して集めたもの（linter ごとに違う実行の仕方を、そろえた形）。
struct Collected {
    head: String,
    issues: Vec<Issue>,
    notes: Vec<String>,
    raw: String,
    stderr: String,
}

fn read_failure(e: String, ex: &Exec) -> String {
    let hint = ex.stderr.lines().take(5).collect::<Vec<_>>().join(" / ");
    format!("{e}（終了コード {:?}{}）", ex.code, if hint.is_empty() { String::new() } else { format!("、stderr: {hint}") })
}

/// Clippy は、変更した `.rs` に近い `Cargo.toml` ごとに、その場所で 1 回ずつ走らせる（Tauri のように `src-tauri/` にあっても動く）。
fn collect_clippy(root: &Path, files: &[String], limits: &LintLimits) -> Result<Collected, String> {
    let mut crates: Vec<String> = Vec::new();
    let mut orphans: Vec<&String> = Vec::new();
    for f in files {
        match clippy_crate_dir(root, f) {
            Some(d) if !crates.contains(&d) => crates.push(d),
            Some(_) => {}
            None => orphans.push(f),
        }
    }
    if crates.is_empty() {
        return Err(format!("変更した Rust のファイルを含む Cargo.toml が見つかりません（{}）", orphans.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("、")));
    }
    let program = proc::find_program("cargo").unwrap_or_else(|| PathBuf::from("cargo"));
    let mut c = Collected { head: "ツール実行型（LLM は使っていません）\n".into(), issues: Vec::new(), notes: Vec::new(), raw: String::new(), stderr: String::new() };
    if !orphans.is_empty() {
        c.notes.push(format!("Cargo.toml の下に無い Rust のファイルは、見ていません: {}", orphans.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("、")));
    }
    let many = crates.len() > 1;
    for dir in &crates {
        let cwd = if dir.is_empty() { root.to_path_buf() } else { root.join(dir) };
        let inv = Invocation { program: program.clone(), args: ["clippy", "--message-format=json", "--quiet"].map(String::from).to_vec(), cwd: cwd.clone() };
        c.head.push_str(&format!("作業ディレクトリ: {}\n実行したコマンド:\n  {}\n\n対象: プロジェクト全体（{}）\n\n", cwd.display(), inv.display(), if dir.is_empty() { "リポジトリの根" } else { dir }));
        let ex = exec(&inv, limits.timeout)?;
        let base = workspace_root(&cwd);
        let issues = parse_clippy_in(&ex.stdout, root, &base).map_err(|e| read_failure(e, &ex))?;
        c.issues.extend(issues);
        if many {
            c.raw.push_str(&format!("----- {dir} -----\n"));
        }
        c.raw.push_str(&ex.stdout);
        if !ex.stderr.is_empty() {
            c.stderr.push_str(&ex.stderr);
            c.stderr.push('\n');
        }
    }
    Ok(c)
}

fn run_linter(linter: &Linter, snap: &Snapshot, limits: &LintLimits) -> Result<ToolRun, String> {
    let root = Path::new(&snap.repo);
    let mut files: Vec<String> = snap.files.iter().map(|f| f.path.clone()).filter(|p| linter.handles(p)).collect();
    if files.is_empty() {
        return Err(format!("差分に {} の対象ファイルがありません（{}）", linter.title(), linter.exts()));
    }
    let mut notes = Vec::new();
    if files.len() > limits.max_files {
        notes.push(format!("対象が {} ファイルあるので、先頭の {} ファイルだけ実行しました", files.len(), limits.max_files));
        files.truncate(limits.max_files);
    }
    let Collected { head: prompt_head, issues, notes: read_notes, raw, stderr } = if matches!(linter, Linter::Builtin(Tool::Clippy)) {
        collect_clippy(root, &files, limits)?
    } else {
        let inv = linter.invocation(root, &files)?;
        let shown = if linter.whole_project() { "（プロジェクト全体）".to_string() } else { files.join("\n  ") };
        let head = format!(
            "ツール実行型（LLM は使っていません）\n作業ディレクトリ: {}\n実行したコマンド:\n  {}\n\n対象ファイル:\n  {}\n",
            inv.cwd.display(),
            inv.display(),
            shown
        );
        let ex = exec(&inv, limits.timeout)?;
        let (issues, read_notes) = linter.read(&ex, root)?;
        Collected { head, issues, notes: read_notes, raw: ex.stdout, stderr: ex.stderr }
    };
    notes.extend(read_notes);

    let changed = changed_lines(&snap.files);
    let title = linter.title();
    let (findings, dropped) = to_findings_titled(&title, issues, &changed);
    if dropped > 0 {
        notes.push(format!("変更していない行への指摘 {dropped} 件は除外しました（今回の差分で追加・変更した行だけを対象にしています）"));
    }
    let err_lines = stderr_lines(&stderr);
    if !err_lines.is_empty() {
        notes.push(format!("標準エラー: {}", err_lines.into_iter().take(3).collect::<Vec<_>>().join(" / ")));
    }

    let text = serde_json::json!({
        "tool": title,
        "excluded_untouched_lines": dropped,
        "findings": findings.iter().map(|f| serde_json::json!({
            "file": f.file, "line": f.line, "severity": f.severity, "title": f.title,
            "detail": f.detail, "evidence": f.evidence, "rule": f.rule,
        })).collect::<Vec<_>>(),
    })
    .to_string();
    Ok(ToolRun {
        response: LlmResponse {
            text,
            model: title,
            // LLM を呼んでいないので、トークンは 0（測っての 0。報告なしではない）
            usage: Usage { input_tokens: Some(0), output_tokens: Some(0), cache_read_tokens: Some(0), cache_write_tokens: Some(0) },
            cost_usd: None,
            raw: cap(raw, limits.max_raw),
        },
        prompt: format!("{prompt_head}\n## 差分\n\n```diff\n{}```\n", snap.diff_text),
        head: prompt_head,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::FileDiff;

    fn fd(path: &str, text: &str) -> FileDiff {
        FileDiff { path: path.into(), bytes: text.len(), text: text.into() }
    }

    const PY_DIFF: &str = "diff --git a/app.py b/app.py\n--- a/app.py\n+++ b/app.py\n@@ -1,3 +1,5 @@\n import sys\n+import os\n+import json\n def f():\n-    return 1\n+    return sys.argv\n";

    #[test]
    fn ids_roundtrip_and_extension_filter() {
        for t in Tool::ALL {
            assert_eq!(Tool::from_id(t.id()), Some(t));
        }
        assert_eq!(Tool::from_id("security"), None);
        assert!(Tool::Eslint.handles("src/a.tsx") && !Tool::Eslint.handles("a.py"));
        assert!(Tool::Ruff.handles("pkg/x.py") && !Tool::Ruff.handles("x.pyc"));
        assert!(Tool::Clippy.handles("src/lib.rs") && !Tool::Clippy.handles("Cargo.toml"));
        assert!(!Tool::Ruff.handles("Makefile"));
    }

    #[test]
    fn changed_lines_uses_new_side_numbers_and_ignores_removed_and_headers() {
        let c = changed_lines(&[fd("app.py", PY_DIFF)]);
        let m = &c["app.py"];
        assert_eq!(m.get(&2).map(String::as_str), Some("import os"));
        assert_eq!(m.get(&3).map(String::as_str), Some("import json"));
        assert_eq!(m.get(&5).map(String::as_str), Some("    return sys.argv"));
        assert_eq!(m.len(), 3, "変更の無い行（import sys・def f）と、消した行は含めない");
    }

    #[test]
    fn changed_lines_handles_multiple_hunks_new_files_and_plusplus_content() {
        let two = "diff --git a/a.js b/a.js\n--- a/a.js\n+++ b/a.js\n@@ -1,2 +1,2 @@\n a\n-b\n+B\n@@ -50,2 +50,3 @@\n x\n+++counter;\n y\n";
        let m = &changed_lines(&[fd("a.js", two)])["a.js"];
        assert_eq!(m.keys().copied().collect::<Vec<_>>(), vec![2, 51]);
        assert_eq!(m[&51], "++counter;", "'+' で始まる内容の行も、追加行として読む");
        let new = "diff --git a/n.rs b/n.rs\nnew file mode 100644\n--- /dev/null\n+++ b/n.rs\n@@ -0,0 +1,2 @@\n+fn a() {}\n+fn b() {}\n\\ No newline at end of file\n";
        assert_eq!(changed_lines(&[fd("n.rs", new)])["n.rs"].len(), 2);
    }

    #[test]
    fn parses_eslint_with_absolute_paths_and_null_rule() {
        let root = Path::new("/repo");
        let out = r#"[{"filePath":"/repo/src/a.js","messages":[
            {"ruleId":"no-var","severity":2,"message":"Unexpected var.","line":3,"column":1},
            {"ruleId":"semi","severity":1,"message":"Missing semicolon.","line":4,"column":9},
            {"ruleId":null,"severity":2,"message":"Parsing error: Unexpected token","line":9,"column":2},
            {"ruleId":"x","severity":1,"message":"no line"}]},
          {"filePath":"/repo/b.js","messages":[]}]"#;
        let v = parse_eslint(out, root).unwrap();
        assert_eq!(v.len(), 3, "行の無いメッセージは飛ばす");
        assert_eq!((v[0].file.as_str(), v[0].line, v[0].rule.as_str(), v[0].severity.as_str()), ("src/a.js", 3, "no-var", "high"));
        assert_eq!((v[1].rule.as_str(), v[1].severity.as_str()), ("semi", "medium"));
        assert_eq!(v[2].rule, "parse-error");
        assert!(parse_eslint("not json", root).is_err());
        assert!(parse_eslint("{}", root).is_err());
    }

    /// 実物（SwiftLint 0.65.1・ShellCheck 0.11.0）の JSON 出力を、そのまま貼ったもの（ShellCheck の `fix` は省いた）。
    #[test]
    fn parses_real_swiftlint_and_shellcheck_json() {
        let root = Path::new("/Users/me/repo");
        let sw = r#"[
  { "character" : 16, "file" : "/Users/me/repo/a.swift", "line" : 4, "reason" : "Force tries should be avoided", "rule_id" : "force_try", "severity" : "Error", "type" : "Force Try" },
  { "character" : 1, "file" : "/Users/me/repo/b.swift", "line" : 9, "reason" : "Line should be 120 characters or less", "rule_id" : "line_length", "severity" : "Warning", "type" : "Line Length" },
  { "character" : null, "file" : "/Users/me/repo/b.swift", "line" : null, "reason" : "File should contain 400 lines or less", "rule_id" : "file_length", "severity" : "Warning", "type" : "File Length" }
]"#;
        let v = parse_swiftlint(sw, root).unwrap();
        let got: Vec<_> = v.iter().map(|i| (i.file.as_str(), i.line, i.rule.as_str(), i.severity.as_str())).collect();
        assert_eq!(got, [("a.swift", 4, "force_try", "high"), ("b.swift", 9, "line_length", "medium")], "行の無い指摘は読まない");
        assert_eq!(v[0].message, "Force tries should be avoided");
        assert!(parse_swiftlint("[\n\n]", root).unwrap().is_empty(), "指摘なしは空の配列");
        assert!(parse_swiftlint("Error: nope", root).unwrap_err().contains("JSON として読めない"));

        let sc = r#"[{"file":"run.sh","line":4,"endLine":4,"column":1,"endColumn":8,"level":"warning","code":2164,"message":"Use 'cd ... || exit' or 'cd ... || return' in case cd fails.","fix":null},
{"file":"run.sh","line":4,"endLine":4,"column":4,"endColumn":8,"level":"info","code":2086,"message":"Double quote to prevent globbing and word splitting.","fix":null},
{"file":"run.sh","line":5,"endLine":5,"column":10,"endColumn":21,"level":"error","code":2045,"message":"Iterating over ls output is fragile. Use globs.","fix":null},
{"file":"run.sh","line":6,"column":1,"level":"style","code":2250,"message":"Prefer putting braces around variable references.","fix":null}]"#;
        let v = parse_shellcheck(sc, root).unwrap();
        let got: Vec<_> = v.iter().map(|i| (i.line, i.rule.as_str(), i.severity.as_str())).collect();
        assert_eq!(got, [(4, "SC2164", "medium"), (4, "SC2086", "low"), (5, "SC2045", "high"), (6, "SC2250", "low")]);
        assert_eq!(v[0].file, "run.sh");
        assert!(parse_shellcheck("[]", root).unwrap().is_empty());
        assert!(parse_shellcheck("", root).is_err(), "出力が空なら、読めないと言う（指摘なしにしない）");
    }

    #[test]
    fn new_tools_have_ids_titles_extensions_and_install_hints() {
        assert_eq!(Tool::from_id("lint-swiftlint"), Some(Tool::Swiftlint));
        assert_eq!(Tool::from_id("lint-shellcheck"), Some(Tool::Shellcheck));
        assert!(Tool::Swiftlint.handles("Sources/App.swift") && !Tool::Swiftlint.handles("a.swiftdoc"));
        assert!(Tool::Shellcheck.handles("run.sh") && Tool::Shellcheck.handles("x/y.bash") && !Tool::Shellcheck.handles("run.zsh"));
        for t in Tool::ALL {
            assert!(t.install_hint().contains('`'), "{:?} の入れ方が書かれていない", t);
        }
        assert!(Tool::Swiftlint.install_hint().contains("brew install swiftlint"));
        assert!(Tool::Shellcheck.install_hint().contains("brew install shellcheck"));
    }

    fn item(d: &Path, id: &str, trusted: bool) -> Option<PlanItem> {
        plan(d, &[], trusted).items.into_iter().find(|i| i.id == id)
    }

    #[test]
    fn eslint_is_planned_only_when_the_project_means_to_use_it_and_needs_trust_to_run() {
        let dir = std::env::temp_dir().join(format!("mrcr-lint-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(item(&dir, "lint-eslint", false).is_none(), "package.json が無い");
        std::fs::write(dir.join("package.json"), r#"{"name":"x"}"#).unwrap();
        assert!(item(&dir, "lint-eslint", false).is_none(), "ESLint を使う様子が無いプロジェクトには、何も出さない");
        std::fs::write(dir.join("package.json"), r#"{"devDependencies":{"eslint":"^9"}}"#).unwrap();
        let m = item(&dir, "lint-eslint", true).expect("package.json に eslint があるのに、入っていない");
        assert_eq!((m.state.as_str(), m.title.as_str(), m.lang.as_str()), ("missing", "ESLint", "JavaScript / TypeScript"));
        assert!(m.hint.unwrap().contains("npm install") && m.command.is_empty());
        std::fs::create_dir_all(dir.join("node_modules/.bin")).unwrap();
        std::fs::write(dir.join("node_modules/.bin/eslint"), "#!/bin/sh\n").unwrap();
        let n = item(&dir, "lint-eslint", false).unwrap();
        assert_eq!((n.state.as_str(), n.runs_repo_code), ("needs_trust", true), "入っていても、許可が無ければ、走らせない");
        assert!(n.command.contains("node_modules/.bin/eslint"), "押す前に、実行するコマンドが見える");
        assert_eq!(item(&dir, "lint-eslint", true).unwrap().state, "ready");
        assert!(available(&dir).contains(&Tool::Eslint));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_eslint_and_clippy_run_the_repos_code() {
        let code: Vec<_> = Tool::ALL.into_iter().filter(|t| t.runs_repo_code()).collect();
        assert_eq!(code, [Tool::Eslint, Tool::Clippy]);
        for t in Tool::ALL {
            assert!(!t.lang().is_empty());
        }
    }

    /// 実行ファイルが入っているかは、マシンによる。どちらの場合でも、出番のあるものは、必ず「実行できる」か「入っていない」のどちらか。
    #[test]
    fn a_linter_with_work_to_do_is_either_ready_or_missing() {
        let dir = std::env::temp_dir().join(format!("mrcr-lint-either-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(&dir).args(args).status().unwrap().success());
        git(&["init", "-q", "-b", "main"]);
        assert!(!has_swift(&dir) && !has_shell(&dir), "空のリポジトリ");
        assert!(plan(&dir, &[], false).items.is_empty(), "出番が無ければ、何も出ない");
        std::fs::write(dir.join("a.swift"), "let x = 1\n").unwrap();
        std::fs::write(dir.join("run.sh"), "echo hi\n").unwrap();
        assert!(has_swift(&dir) && has_shell(&dir), "未追跡のファイルも数える");
        for id in ["lint-swiftlint", "lint-shellcheck"] {
            let i = item(&dir, id, false).unwrap_or_else(|| panic!("{id} が一覧に無い"));
            match i.state.as_str() {
                "ready" => assert!(!i.command.is_empty() && i.hint.is_none()),
                "missing" => assert!(i.command.is_empty() && i.hint.as_deref().is_some_and(|h| h.contains("brew install"))),
                other => panic!("{id}: {other}"),
            }
            assert_eq!(available(&dir).iter().any(|t| t.id() == id), i.state == "ready");
        }
        assert!(item(&dir, "lint-ruff", false).is_none(), "Python のファイルが無い");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_lint_recipe_lists_what_may_run_and_check_targets_accepts_any_matching_file() {
        let p = Plan {
            items: vec![
                PlanItem { id: "lint-ruff".into(), title: "Ruff".into(), lang: "Python".into(), exts: vec!["py".into()], command: "ruff check".into(), runs_repo_code: false, state: "ready".into(), hint: None },
                PlanItem { id: "lint-swiftlint".into(), title: "SwiftLint".into(), lang: "Swift".into(), exts: vec!["swift".into()], command: String::new(), runs_repo_code: false, state: "missing".into(), hint: Some("x".into()) },
            ],
            trusted: false,
        };
        let r = lint_recipe(&p);
        assert_eq!((r.id.as_str(), r.title.as_str(), r.kind), ("lint", "リンター", Kind::Tool));
        assert!(r.prompt.contains("Ruff（Python）: ruff check") && !r.prompt.contains("SwiftLint"), "入っていないものは、走らないので載せない");
        assert!(check_targets(LINT_ID, &[], &[fd("a.py", "x")]).is_ok());
        assert!(check_targets(LINT_ID, &[], &[fd("a.swift", "x")]).is_ok());
        let e = check_targets(LINT_ID, &[], &[fd("README.md", "x")]).unwrap_err();
        assert!(e.contains("linter が見るファイルがありません") && e.contains("Ruff（Python）"), "{e}");
    }

    #[test]
    fn clippy_paths_are_made_relative_to_the_repo_even_when_cargo_toml_is_in_a_subdirectory() {
        let line = |file: &str| format!(r#"{{"reason":"compiler-message","message":{{"level":"warning","code":{{"code":"clippy::x"}},"message":"m","spans":[{{"file_name":"{file}","line_start":3,"is_primary":true}}]}}}}"#);
        let root = Path::new("/r");
        // Tauri: Cargo.toml は /r/src-tauri にある。cargo は、そこを基準にしたパスを出す
        let v = parse_clippy_in(&line("src/lib.rs"), root, Path::new("/r/src-tauri")).unwrap();
        assert_eq!(v[0].file, "src-tauri/src/lib.rs");
        // ワークスペースの根が、リポジトリの根のとき: メンバーのパスが、根から始まる
        let v = parse_clippy_in(&line("crates/a/src/lib.rs"), root, Path::new("/r")).unwrap();
        assert_eq!(v[0].file, "crates/a/src/lib.rs");
        // 絶対パスは、そのまま（リポジトリの外なら、リポジトリのパスにならない＝差分に無いので落ちる）
        assert_eq!(parse_clippy_in(&line("/r/src-tauri/src/lib.rs"), root, Path::new("/x")).unwrap()[0].file, "src-tauri/src/lib.rs");
        assert_eq!(parse_clippy_in(&line("/other/dep/src/lib.rs"), root, Path::new("/x")).unwrap()[0].file, "/other/dep/src/lib.rs");
        // 従来の呼び方（基準＝リポジトリの根）は、そのまま
        assert_eq!(parse_clippy(&line("src/main.rs"), root).unwrap()[0].file, "src/main.rs");
    }

    #[test]
    fn the_nearest_cargo_toml_decides_where_clippy_runs() {
        let dir = std::env::temp_dir().join(format!("mrcr-lint-crates-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src-tauri/src")).unwrap();
        std::fs::create_dir_all(dir.join("tools/gen/src")).unwrap();
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::write(dir.join("src-tauri/Cargo.toml"), "").unwrap();
        std::fs::write(dir.join("tools/gen/Cargo.toml"), "").unwrap();
        assert_eq!(clippy_crate_dir(&dir, "src-tauri/src/lib.rs").as_deref(), Some("src-tauri"));
        assert_eq!(clippy_crate_dir(&dir, "tools/gen/src/a/b.rs").as_deref(), Some("tools/gen"), "深い場所のファイルも、近い Cargo.toml を見つける");
        assert_eq!(clippy_crate_dir(&dir, "docs/x.rs"), None, "Cargo.toml の下に無い");
        assert_eq!(clippy_crate_dir(&dir, "main.rs"), None);
        std::fs::write(dir.join("Cargo.toml"), "").unwrap();
        assert_eq!(clippy_crate_dir(&dir, "main.rs").as_deref(), Some(""), "根に Cargo.toml があれば、根");
        assert_eq!(clippy_crate_dir(&dir, "docs/x.rs").as_deref(), Some(""));
        assert_eq!(clippy_crate_dir(&dir, "src-tauri/src/lib.rs").as_deref(), Some("src-tauri"), "近いほうが勝つ");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clippy_applies_to_a_repo_whose_cargo_toml_is_only_in_a_subdirectory() {
        let dir = std::env::temp_dir().join(format!("mrcr-lint-tauri-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src-tauri")).unwrap();
        let git = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(&dir).args(args).status().unwrap().success());
        git(&["init", "-q", "-b", "main"]);
        assert!(!Tool::Clippy.applies(&dir), "Cargo.toml が無い");
        std::fs::write(dir.join("src-tauri/Cargo.toml"), "").unwrap();
        assert!(Tool::Clippy.applies(&dir), "Tauri のように、src-tauri/ にだけあっても、出番がある");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_ruff_json() {
        let root = Path::new("/repo");
        let out = r#"[{"code":"F401","message":"`os` imported but unused","location":{"column":8,"row":2},"end_location":{"column":10,"row":2},"filename":"/repo/app.py","fix":null,"noqa_row":2,"url":"x"},
                      {"code":null,"message":"SyntaxError: bad","location":{"column":1,"row":7},"filename":"/repo/app.py"}]"#;
        let v = parse_ruff(out, root).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!((v[0].file.as_str(), v[0].line, v[0].rule.as_str(), v[0].severity.as_str()), ("app.py", 2, "F401", "medium"));
        assert_eq!((v[1].rule.as_str(), v[1].severity.as_str()), ("syntax-error", "high"));
        assert!(parse_ruff("[]", root).unwrap().is_empty());
    }

    #[test]
    fn parses_clippy_lines_and_skips_noise() {
        let root = Path::new("/repo");
        let out = [
            r#"{"reason":"compiler-artifact","package_id":"x"}"#,
            "warning: not json at all",
            r#"{"reason":"compiler-message","message":{"level":"warning","message":"needless `return`\nmore","code":{"code":"clippy::needless_return"},"spans":[{"file_name":"src/lib.rs","line_start":3,"is_primary":false},{"file_name":"src/lib.rs","line_start":4,"is_primary":true}]}}"#,
            r#"{"reason":"compiler-message","message":{"level":"error","message":"mismatched types","code":null,"spans":[{"file_name":"src/main.rs","line_start":9,"is_primary":true}]}}"#,
            r#"{"reason":"compiler-message","message":{"level":"warning","message":"1 warning emitted","code":null,"spans":[]}}"#,
            r#"{"reason":"compiler-message","message":{"level":"note","message":"n","spans":[{"file_name":"a.rs","line_start":1,"is_primary":true}]}}"#,
            r#"{"reason":"build-finished","success":false}"#,
        ]
        .join("\n");
        let v = parse_clippy(&out, root).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!((v[0].file.as_str(), v[0].line, v[0].rule.as_str(), v[0].severity.as_str()), ("src/lib.rs", 4, "clippy::needless_return", "medium"));
        assert_eq!((v[1].rule.as_str(), v[1].severity.as_str()), ("rustc", "high"));
    }

    #[test]
    fn keeps_only_issues_on_changed_lines_and_counts_the_rest() {
        let changed = changed_lines(&[fd("app.py", PY_DIFF)]);
        let issue = |line: u32, rule: &str| Issue { file: "app.py".into(), line, rule: rule.into(), severity: "medium".into(), message: format!("{rule} msg\ndetail") };
        let issues = vec![
            issue(2, "F401"),
            issue(2, "F401"), // 完全な重複は 1 件にする
            issue(2, "E401"), // 同じ行の別の規則は別の指摘
            issue(1, "F401"), // 変更していない行
            issue(40, "E501"), // 差分の外
            Issue { file: "other.py".into(), line: 1, rule: "X".into(), severity: "low".into(), message: "m".into() },
        ];
        let (f, dropped) = to_findings(Tool::Ruff, issues, &changed);
        assert_eq!(f.len(), 2);
        assert_eq!(dropped, 3);
        assert_eq!(f[0].rule.as_deref(), Some("E401"), "規則名の順に並ぶ");
        assert_eq!(f[1].rule.as_deref(), Some("F401"));
        assert_eq!(f[1].evidence, "import os", "引用は差分のその行（前後の空白は除く）");
        assert_eq!(f[1].title, "F401 msg", "見出しはメッセージの 1 行目");
        assert_eq!(f[1].detail, "Ruff F401");
        assert_eq!(f[1].line, Some(2));
    }

    #[test]
    fn recipe_describes_the_command_and_warns_about_running_repo_config() {
        let r = recipe(Tool::Eslint, Path::new("/repo"));
        assert_eq!((r.id.as_str(), r.kind), ("lint-eslint", Kind::Tool));
        assert!(r.prompt.contains("/repo/node_modules/.bin/eslint --format json"));
        assert!(r.prompt.contains("<差分のファイル>"));
        assert!(r.prompt.contains("信頼できるリポジトリでだけ"));
        assert!(recipe(Tool::Clippy, Path::new("/repo")).prompt.contains("build.rs"));
    }

    #[test]
    fn availability_follows_the_repo_files() {
        let dir = std::env::temp_dir().join(format!("mrcr-lint-avail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("node_modules/.bin")).unwrap();
        assert!(!available(&dir).contains(&Tool::Eslint), "package.json が無い");
        std::fs::write(dir.join("package.json"), "{}").unwrap();
        assert!(!available(&dir).contains(&Tool::Eslint), "eslint が入っていない");
        std::fs::write(dir.join("node_modules/.bin/eslint"), "#!/bin/sh\n").unwrap();
        assert!(available(&dir).contains(&Tool::Eslint));
        assert!(!available(&dir).contains(&Tool::Clippy), "Cargo.toml が無い");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stderr_lines_drops_cargo_progress_but_keeps_real_messages() {
        let e = "[====>      ] 499/514: a, b\r[=====>     ] 500/514: c \nwarning: unused\n\n[=========>] 513/514: mrcodereview(bin)";
        assert_eq!(stderr_lines(e), vec!["warning: unused"]);
    }

    #[test]
    fn exec_times_out_and_reports_missing_programs() {
        let dir = std::env::temp_dir();
        let slow = Invocation { program: PathBuf::from("sh"), args: vec!["-c".into(), "sleep 30".into()], cwd: dir.clone() };
        let err = exec(&slow, Duration::from_millis(300)).err().unwrap();
        assert!(err.contains("中断した"), "{err}");
        let missing = Invocation { program: PathBuf::from("/nonexistent/linter"), args: vec![], cwd: dir };
        assert!(exec(&missing, Duration::from_secs(5)).err().unwrap().contains("起動できません"));
    }

    #[test]
    fn run_needs_files_the_tool_understands() {
        let snap = Snapshot {
            repo: "/nonexistent".into(),
            head: "h".into(),
            base: Default::default(),
            diff_hash: "d".into(),
            files: vec![crate::snapshot::FileDiff { path: "README.md".into(), bytes: 1, text: "x".into() }],
            excluded: vec![],
            diff_bytes: 1,
            diff_text: "x".into(),
        };
        let err = run("lint-ruff", &snap).err().unwrap();
        assert!(err.contains("対象ファイルがありません"), "{err}");
        assert!(run("security", &snap).err().unwrap().contains("ツールが見つからない"));
    }
}
