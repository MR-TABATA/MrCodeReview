//! 設定で足す linter（あなた個人の `linters.json`）。
//!
//! 組み込みの ESLint・Ruff・Clippy・SwiftLint・ShellCheck 以外の linter（golangci-lint など）を、
//! コードを書かずに足せる。出力の読み方は、**SARIF**（標準形式）か、**正規表現**（`ファイル:行: メッセージ` のような行）。
//! 読めた指摘は、組み込みの linter と同じ道（`lint.rs`）を通る。変更した行への絞り込み・仕分け・引き継ぎ・
//! 差分への重ね描きが、そのまま使える。
//!
//! **コマンドを実行する設定なので、置いてよいのは、あなた個人のファイルだけ。**
//! リポジトリ側の `.mrcodereview/linters.json` は、読まない（信頼できないリポジトリで、ボタンを押しただけで
//! 任意のコマンドが走るのを防ぐ）。あれば、読まなかったことを画面に出す（`repo_notice`）。
//! 設定の間違いは、黙って無視せず、項目名つきのエラーにする。

use crate::lint::{self, Invocation, Issue};
use crate::proc;
use crate::recipe::{Kind, Recipe};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const FILE: &str = "linters.json";
/// 組み込みの観点・linter の id とぶつからないよう、設定で足した linter の id には、必ずこれを付ける。
pub const ID_PREFIX: &str = "lint-";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Def {
    /// 英小文字・数字・`-`。レシピの id は `lint-` + これ。
    id: String,
    title: String,
    /// 対象のファイルの拡張子（`swift` のように、ドットなし）。差分にこの拡張子のファイルがあるときだけ実行する。
    extensions: Vec<String>,
    /// 実行するコマンド（引数の配列。シェルは通さない）。
    command: Vec<String>,
    /// "append" = 差分のファイル名を末尾に足す（既定）/ "none" = 足さない（プロジェクト全体を見るツール）
    #[serde(default = "d_files")]
    files: String,
    /// "regex" | "sarif"
    format: String,
    /// `regex` のとき。名前つきグループ `file`・`line`・`message` が要る。`severity`・`rule` は省略できる。
    #[serde(default)]
    pattern: Option<String>,
    /// 正常と見なす終了コード。linter は、指摘があると 0 以外で終わるものが多い。
    #[serde(default = "d_ok")]
    ok_exit_codes: Vec<i32>,
    /// 指摘を読む出力: "stdout"（既定）| "stderr" | "both"
    #[serde(default = "d_output")]
    output: String,
    /// この中のどれかが、リポジトリの根にあれば、ボタンを出す。省略すると、実行ファイルがあれば常に出す。
    #[serde(default)]
    detect: Vec<String>,
    /// 重大度の指定が無い（または読めない）ときの重大度: "high" | "medium"（既定）| "low"
    #[serde(default = "d_severity")]
    default_severity: String,
}

fn d_files() -> String {
    "append".into()
}
fn d_ok() -> Vec<i32> {
    vec![0, 1]
}
fn d_output() -> String {
    "stdout".into()
}
fn d_severity() -> String {
    "medium".into()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDef {
    linters: Vec<Def>,
}

/// 検証を通った linter（正規表現はコンパイル済み）。
#[derive(Debug, Clone)]
pub struct Custom {
    def: Def,
    regex: Option<Regex>,
}

fn err(id: &str, msg: &str) -> String {
    format!("{FILE} の「{id}」: {msg}")
}

fn validate(def: &Def, seen: &mut HashSet<String>) -> Result<Option<Regex>, String> {
    let id = def.id.as_str();
    let ok_id = !id.is_empty()
        && id.len() <= 31
        && id.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !ok_id {
        return Err(err(id, "`id` は、英小文字・数字・`-` だけで、31 文字までにしてください"));
    }
    if !seen.insert(id.to_string()) {
        return Err(err(id, "`id` が重複しています"));
    }
    if ["eslint", "ruff", "clippy", "swiftlint", "shellcheck"].contains(&id) {
        return Err(err(id, "この `id` は、組み込みの linter が使っています。別の `id` にしてください"));
    }
    if def.title.trim().is_empty() {
        return Err(err(id, "`title` が空です"));
    }
    if def.extensions.is_empty() || def.extensions.iter().any(|e| e.trim().is_empty() || e.contains('.') || e.contains('/')) {
        return Err(err(id, "`extensions` は、ドットなしの拡張子（例: \"swift\"）を、1 つ以上書いてください"));
    }
    if def.command.first().is_none_or(|c| c.trim().is_empty()) {
        return Err(err(id, "`command` は、実行するコマンド（引数の配列）を書いてください。先頭が、実行ファイルです"));
    }
    if !["append", "none"].contains(&def.files.as_str()) {
        return Err(err(id, "`files` は \"append\" か \"none\" にしてください"));
    }
    if !["stdout", "stderr", "both"].contains(&def.output.as_str()) {
        return Err(err(id, "`output` は \"stdout\"・\"stderr\"・\"both\" のどれかにしてください"));
    }
    if def.ok_exit_codes.is_empty() {
        return Err(err(id, "`ok_exit_codes` は、1 つ以上書いてください（空だと、必ず失敗になります）"));
    }
    if !["high", "medium", "low"].contains(&def.default_severity.as_str()) {
        return Err(err(id, "`default_severity` は \"high\"・\"medium\"・\"low\" のどれかにしてください"));
    }
    if def.detect.iter().any(|d| d.trim().is_empty() || d.contains("..") || Path::new(d).is_absolute()) {
        return Err(err(id, "`detect` は、リポジトリの根からの相対パス（`..` なし）を書いてください"));
    }
    match def.format.as_str() {
        "sarif" => {
            if def.pattern.is_some() {
                return Err(err(id, "`format` が \"sarif\" のときは、`pattern` は要りません"));
            }
            Ok(None)
        }
        "regex" => {
            let p = def.pattern.as_deref().ok_or_else(|| err(id, "`format` が \"regex\" のときは、`pattern` が要ります"))?;
            let re = Regex::new(p).map_err(|e| err(id, &format!("`pattern` が正規表現として正しくありません: {e}")))?;
            let names: Vec<&str> = re.capture_names().flatten().collect();
            for need in ["file", "line", "message"] {
                if !names.contains(&need) {
                    return Err(err(id, &format!("`pattern` に、名前つきグループ `(?P<{need}>…)` が要ります（要るのは file・line・message。severity・rule は省略できます）")));
                }
            }
            Ok(Some(re))
        }
        other => Err(err(id, &format!("`format` は \"regex\" か \"sarif\" にしてください（書かれていたのは \"{other}\"）"))),
    }
}

/// あなた個人の `linters.json`（アプリのデータ領域）を読む。無ければ空。
pub fn load(dir: &Path) -> Result<Vec<Custom>, String> {
    let path = dir.join(FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{FILE} を読めない（{}）: {e}", path.display()))?;
    let f: FileDef = serde_json::from_str(&text).map_err(|e| format!("{FILE}（{}）が正しくありません: {e}", path.display()))?;
    let mut seen = HashSet::new();
    f.linters
        .into_iter()
        .map(|def| {
            let regex = validate(&def, &mut seen)?;
            Ok(Custom { def, regex })
        })
        .collect()
}

/// リポジトリ側に `.mrcodereview/linters.json` があれば、読まないことを知らせる文面。
pub fn repo_notice(repo_root: &Path) -> Option<String> {
    let p = repo_root.join(crate::settings::REPO_DIR).join(FILE);
    p.exists().then(|| {
        format!(
            "このリポジトリの {} は、読みません。コマンドを実行する設定は、安全のため、あなた個人の {FILE}（アプリのデータ領域）にだけ書けます",
            p.display()
        )
    })
}

fn expand_home(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(p),
    }
}

impl Custom {
    pub fn recipe_id(&self) -> String {
        format!("{ID_PREFIX}{}", self.def.id)
    }

    pub fn title(&self) -> &str {
        &self.def.title
    }

    pub fn exts_text(&self) -> String {
        self.def.extensions.join(", ")
    }

    pub fn handles(&self, path: &str) -> bool {
        path.rsplit_once('.').is_some_and(|(_, ext)| self.def.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
    }

    pub fn whole_project(&self) -> bool {
        self.def.files == "none"
    }

    /// 実行ファイル。`/` を含めば、そのパス（`~/` は展開）。含まなければ、PATH とよくある場所から探す。
    fn program(&self) -> Option<PathBuf> {
        let cmd = &self.def.command[0];
        if cmd.contains('/') {
            let p = expand_home(cmd);
            p.is_file().then_some(p)
        } else {
            proc::find_program(cmd)
        }
    }

    /// このリポジトリで使えるか（実行ファイルがあり、`detect` の目印があれば、その 1 つ以上がある）。
    pub fn available(&self, root: &Path) -> bool {
        self.program().is_some() && (self.def.detect.is_empty() || self.def.detect.iter().any(|d| root.join(d).exists()))
    }

    /// `detect` の目印はあるのに、実行ファイルが無いとき。ボタンの代わりに、理由を画面に出すためのもの。
    /// `detect` が無いものは、このリポジトリに出番があるか分からないので、何も言わない。
    pub fn missing(&self, root: &Path) -> Option<crate::lint::Missing> {
        let wanted = !self.def.detect.is_empty() && self.def.detect.iter().any(|d| root.join(d).exists());
        (wanted && self.program().is_none()).then(|| crate::lint::Missing {
            id: self.recipe_id(),
            title: self.def.title.clone(),
            hint: format!("`{}` を入れる（または、{FILE} の「{}」の `command` を直す）", self.def.command[0], self.def.id),
        })
    }

    pub fn invocation(&self, root: &Path, files: &[String]) -> Result<Invocation, String> {
        let program = self.program().ok_or_else(|| format!("{} が見つかりません（{FILE} の「{}」の `command`）", self.def.command[0], self.def.id))?;
        let mut args: Vec<String> = self.def.command[1..].to_vec();
        if self.def.files == "append" {
            args.extend(files.iter().cloned());
        }
        Ok(Invocation { program, args, cwd: root.to_path_buf() })
    }

    pub fn recipe(&self, root: &Path) -> Recipe {
        let shown = self
            .invocation(root, &["<差分のファイル>".to_string()])
            .map(|i| i.display())
            .unwrap_or_else(|e| e);
        let how = match self.def.format.as_str() {
            "sarif" => "出力は SARIF として読みます。".to_string(),
            _ => format!("出力は、正規表現（`pattern`）に一致する行を指摘として読みます。\n  pattern: {}", self.def.pattern.as_deref().unwrap_or("")),
        };
        let prompt = [
            format!(
                "LLM を使わず、差分に出ている {} のファイルに、あなたの {FILE} に書いた linter「{}」を実行します。トークンは使いません。",
                self.exts_text(),
                self.def.title
            ),
            format!("実行するコマンド:\n  {shown}\n  （作業ディレクトリ: リポジトリの根）"),
            how,
            "結果のうち、今回の差分で追加・変更した行への指摘だけを残します。それ以外の行への指摘は、件数だけ記録します。".to_string(),
            format!("⚠ これは、あなたが {FILE} に書いたコマンドです。信頼できるリポジトリでだけ押してください。"),
        ]
        .join("\n\n");
        Recipe {
            id: self.recipe_id(),
            title: format!("リンター: {}", self.def.title),
            source: "user".into(),
            kind: Kind::Tool,
            needs_related: false,
            prompt,
        }
    }

    /// linter の出力から、指摘と、実行メモを読む。終了コードが `ok_exit_codes` に無ければ、失敗として、理由つきで返す。
    pub fn read(&self, stdout: &str, stderr: &str, code: Option<i32>, root: &Path) -> Result<(Vec<Issue>, Vec<String>), String> {
        let hint = || {
            let h = stderr.lines().take(5).collect::<Vec<_>>().join(" / ");
            if h.is_empty() { String::new() } else { format!("（stderr: {h}）") }
        };
        match code {
            Some(c) if self.def.ok_exit_codes.contains(&c) => {}
            Some(c) => {
                return Err(format!(
                    "終了コード {c} は、{FILE} の「{}」の `ok_exit_codes` {:?} に含まれません。linter が失敗したと見なしました{}",
                    self.def.id,
                    self.def.ok_exit_codes,
                    hint()
                ))
            }
            None => return Err(format!("linter がシグナルで終了しました{}", hint())),
        }
        let text = match self.def.output.as_str() {
            "stderr" => stderr.to_string(),
            "both" => format!("{stdout}\n{stderr}"),
            _ => stdout.to_string(),
        };
        if self.def.format == "sarif" {
            return parse_sarif(&text, root).map(|v| (v, Vec::new())).map_err(|e| format!("{e}{}", hint()));
        }
        let re = self.regex.as_ref().expect("regex は、検証で作ってある");
        let issues = parse_regex(re, &text, root, &self.def.id, &self.def.default_severity);
        let mut notes = Vec::new();
        let lines = text.lines().filter(|l| !l.trim().is_empty()).count();
        if issues.is_empty() && lines > 0 {
            // 出力があるのに、1 件も読めなかった。パターンが合っていない可能性が高い（黙って「指摘なし」にしない）
            let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            notes.push(format!(
                "「{}」の出力が {lines} 行ありますが、`pattern` に一致する行がありませんでした。指摘が無いのか、`pattern` が合っていないのか、確かめてください（出力の先頭: {first}）",
                self.def.title
            ));
        }
        Ok((issues, notes))
    }
}

/// 重大度の文字列を、high / medium / low にする。分からなければ None。
fn severity_of(text: &str) -> Option<&'static str> {
    let t = text.trim().to_lowercase();
    if ["error", "fatal", "critical", "high", "blocker", "major"].iter().any(|k| t.contains(k)) {
        Some("high")
    } else if ["warn", "medium", "moderate"].iter().any(|k| t.contains(k)) {
        Some("medium")
    } else if ["info", "note", "style", "hint", "low", "minor", "convention", "refactor", "suggestion"].iter().any(|k| t.contains(k)) {
        Some("low")
    } else {
        None
    }
}

pub fn parse_regex(re: &Regex, text: &str, root: &Path, id: &str, default_severity: &str) -> Vec<Issue> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(c) = re.captures(line) else { continue };
        let Some(no) = c.name("line").and_then(|m| m.as_str().trim().parse::<u32>().ok()) else { continue };
        let file = c.name("file").map(|m| m.as_str().trim()).unwrap_or("");
        if file.is_empty() {
            continue;
        }
        out.push(Issue {
            file: lint::rel(root, file),
            line: no,
            rule: c.name("rule").map(|m| m.as_str().trim().to_string()).filter(|r| !r.is_empty()).unwrap_or_else(|| id.to_string()),
            severity: c.name("severity").and_then(|m| severity_of(m.as_str())).unwrap_or(match default_severity {
                "high" => "high",
                "low" => "low",
                _ => "medium",
            }).to_string(),
            message: c.name("message").map(|m| m.as_str().trim().to_string()).unwrap_or_default(),
        });
    }
    out
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            // 16 進の 2 桁（UTF-8 の途中で切らないよう、バイト列のまま読む）
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// SARIF の場所（`uri`）を、パスにする。`file://` と `%` エスケープを解く。
fn uri_to_path(uri: &str) -> String {
    let u = uri.strip_prefix("file://").map(|r| r.strip_prefix("localhost").unwrap_or(r)).unwrap_or(uri);
    percent_decode(u)
}

pub fn parse_sarif(text: &str, root: &Path) -> Result<Vec<Issue>, String> {
    let v: Value = serde_json::from_str(text.trim()).map_err(|e| format!("SARIF として読めない（JSON が正しくありません）: {e}"))?;
    let runs = v.get("runs").and_then(Value::as_array).ok_or("SARIF に `runs` が無い（SARIF 2.1.0 の形式ではありません）")?;
    let mut out = Vec::new();
    for run in runs {
        for r in run.get("results").and_then(Value::as_array).into_iter().flatten() {
            let Some(loc) = r.get("locations").and_then(Value::as_array).and_then(|l| l.first()) else { continue };
            let phys = loc.get("physicalLocation");
            let Some(uri) = phys.and_then(|p| p.get("artifactLocation")).and_then(|a| a.get("uri")).and_then(Value::as_str) else { continue };
            let Some(line_u64) = phys.and_then(|p| p.get("region")).and_then(|g| g.get("startLine")).and_then(Value::as_u64) else { continue };
            let Ok(line) = u32::try_from(line_u64) else { continue };
            let rule = r
                .get("ruleId")
                .and_then(Value::as_str)
                .or_else(|| r.get("rule").and_then(|x| x.get("id")).and_then(Value::as_str))
                .unwrap_or("sarif");
            // SARIF の level の既定は warning
            let level = r.get("level").and_then(Value::as_str).unwrap_or("warning");
            out.push(Issue {
                file: lint::rel(root, &uri_to_path(uri)),
                line,
                rule: rule.to_string(),
                severity: match level {
                    "error" => "high",
                    "warning" => "medium",
                    _ => "low",
                }
                .to_string(),
                message: r.get("message").and_then(|m| m.get("text").or_else(|| m.get("markdown"))).and_then(Value::as_str).unwrap_or("").to_string(),
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        // 並行して走るテストが、同じディレクトリを使わないよう、通し番号を付ける
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("mrcr-linters-{tag}-{n}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, text: &str) {
        std::fs::write(dir.join(FILE), text).unwrap();
    }

    const SWIFTLINT: &str = r#"{"linters":[{
        "id": "swiftlint-xcode", "title": "SwiftLint", "extensions": ["swift"],
        "command": ["swiftlint", "lint", "--quiet", "--reporter", "xcode", "--"],
        "format": "regex",
        "pattern": "^(?P<file>.+?):(?P<line>\\d+):(?:\\d+:)? (?P<severity>warning|error): (?P<message>.*?)(?: \\((?P<rule>[^)]+)\\))?$",
        "ok_exit_codes": [0, 2],
        "detect": ["Package.swift", ".swiftlint.yml"]
    }]}"#;

    #[test]
    fn loads_a_valid_definition_with_defaults() {
        let d = scratch("ok");
        write(&d, SWIFTLINT);
        let v = load(&d).unwrap();
        assert_eq!(v.len(), 1);
        let c = &v[0];
        assert_eq!((c.recipe_id().as_str(), c.title()), ("lint-swiftlint-xcode", "SwiftLint"));
        assert!(c.handles("App/Foo.swift") && c.handles("a.SWIFT") && !c.handles("a.swiftdoc") && !c.handles("Makefile"));
        assert!(!c.whole_project());
        // 省略した項目の既定値
        let d2 = scratch("defaults");
        write(&d2, r#"{"linters":[{"id":"x","title":"X","extensions":["x"],"command":["x"],"format":"sarif"}]}"#);
        let c2 = &load(&d2).unwrap()[0];
        assert_eq!((c2.def.files.as_str(), c2.def.output.as_str(), c2.def.default_severity.as_str()), ("append", "stdout", "medium"));
        assert_eq!(c2.def.ok_exit_codes, vec![0, 1]);
        assert!(c2.def.detect.is_empty());
        // ファイルが無ければ、空
        assert!(load(&scratch("none")).unwrap().is_empty());
        for p in [d, d2] {
            let _ = std::fs::remove_dir_all(p);
        }
    }

    fn bad(tag: &str, json: &str) -> String {
        let d = scratch(tag);
        write(&d, json);
        let e = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        e
    }

    fn def_with(field: &str) -> String {
        // 正しい定義の一部を差し替える
        format!(r#"{{"linters":[{{"id":"x","title":"X","extensions":["x"],"command":["x"],"format":"sarif"{field}}}]}}"#)
    }

    #[test]
    fn invalid_definitions_are_errors_naming_the_linter_and_the_field() {
        let e = bad("syntax", "{broken");
        assert!(e.contains("linters.json") && e.contains("正しくありません"), "{e}");
        assert!(bad("unknown", &def_with(r#","comand":["y"]"#)).contains("comand"), "綴りの間違いを黙って無視しない");
        let cases: Vec<(String, &str)> = vec![
            (def_with("").replace(r#""id":"x""#, r#""id":"Bad_Id""#), "`id` は、英小文字"),
            (def_with("").replace(r#""id":"x""#, r#""id":"eslint""#), "組み込みの linter"),
            (def_with("").replace(r#""title":"X""#, r#""title":" ""#), "`title` が空"),
            (def_with("").replace(r#""extensions":["x"]"#, r#""extensions":[]"#), "`extensions`"),
            (def_with("").replace(r#""extensions":["x"]"#, r#""extensions":[".swift"]"#), "ドットなし"),
            (def_with("").replace(r#""command":["x"]"#, r#""command":[]"#), "`command`"),
            (def_with("").replace(r#""command":["x"]"#, r#""command":[" "]"#), "`command`"),
            (def_with(r#","files":"all""#), "`files`"),
            (def_with(r#","output":"file""#), "`output`"),
            (def_with(r#","ok_exit_codes":[]"#), "`ok_exit_codes`"),
            (def_with(r#","default_severity":"fatal""#), "`default_severity`"),
            (def_with(r#","detect":["../x"]"#), "`detect`"),
            (def_with(r#","pattern":"x""#), "`pattern` は要りません"),
            (def_with("").replace(r#""format":"sarif""#, r#""format":"json""#), "\"json\""),
            (def_with("").replace(r#""format":"sarif""#, r#""format":"regex""#), "`pattern` が要ります"),
            (def_with(r#","pattern":"(""#).replace(r#""format":"sarif""#, r#""format":"regex""#), "正規表現として正しくありません"),
            (def_with(r#","pattern":"^(?P<file>.+):(?P<line>\\d+)$""#).replace(r#""format":"sarif""#, r#""format":"regex""#), "(?P<message>"),
        ];
        for (json, want) in cases {
            let e = bad("case", &json);
            assert!(e.contains(want), "期待: {want} / 実際: {e}\n{json}");
            assert!(e.contains("linters.json"), "{e}");
        }
        let dup = r#"{"linters":[{"id":"x","title":"X","extensions":["x"],"command":["x"],"format":"sarif"},{"id":"x","title":"Y","extensions":["y"],"command":["y"],"format":"sarif"}]}"#;
        assert!(bad("dup", dup).contains("重複"));
    }

    /// 実物（SwiftLint 0.65.1・ShellCheck 0.11.0）の出力を、そのまま貼ったもの。README の `pattern` と同じ式で読む。
    #[test]
    fn regex_reads_real_swiftlint_and_shellcheck_output() {
        let root = Path::new("/Users/me/repo");
        let sw = Regex::new(r"^(?P<file>.+?):(?P<line>\d+):(?:\d+:)? (?P<severity>warning|error): (?P<message>.*?)(?: \((?P<rule>[^)]+)\))?$").unwrap();
        let out = "/Users/me/repo/a.swift:5:16: error: Force Try Violation: Force tries should be avoided (force_try)\n\
                   /Users/me/repo/a.swift:6:9: error: Identifier Name Violation: Variable name 'x' should be between 3 and 40 characters long (identifier_name)\n";
        let v = parse_regex(&sw, out, root, "swiftlint", "medium");
        assert_eq!(v.len(), 2);
        assert_eq!((v[0].file.as_str(), v[0].line, v[0].rule.as_str(), v[0].severity.as_str()), ("a.swift", 5, "force_try", "high"));
        assert_eq!(v[0].message, "Force Try Violation: Force tries should be avoided");
        assert_eq!(v[1].rule, "identifier_name");

        let sc = Regex::new(r"^(?P<file>.+?):(?P<line>\d+):\d+: (?P<severity>error|warning|note): (?P<message>.*?)(?: \[(?P<rule>SC\d+)\])?$").unwrap();
        let out = "a.sh:3:1: warning: Use 'cd ... || exit' or 'cd ... || return' in case cd fails. [SC2164]\n\
                   a.sh:3:4: note: Double quote to prevent globbing and word splitting. [SC2086]\n\
                   a.sh:4:10: error: Iterating over ls output is fragile. Use globs. [SC2045]\n";
        let v = parse_regex(&sc, out, root, "shellcheck", "medium");
        let got: Vec<_> = v.iter().map(|i| (i.line, i.rule.as_str(), i.severity.as_str())).collect();
        assert_eq!(got, [(3, "SC2164", "medium"), (3, "SC2086", "low"), (4, "SC2045", "high")]);
        assert_eq!(v[0].file, "a.sh");
    }

    #[test]
    fn regex_reads_swiftlint_shellcheck_and_ruff_style_lines_and_skips_noise() {
        let root = Path::new("/repo");
        let sw = Regex::new(r"^(?P<file>.+?):(?P<line>\d+):(?:\d+:)? (?P<severity>warning|error): (?P<message>.*?)(?: \((?P<rule>[^)]+)\))?$").unwrap();
        let out = "/repo/Sources/App.swift:12:5: warning: Line should be 120 characters or less (line_length)\n\
                   /repo/Sources/App.swift:30: error: Force cast violation (force_cast)\n\
                   Done linting! Found 2 violations.\n\
                   \n\
                   Sources/Other.swift:3:1: warning: Trailing newline";
        let v = parse_regex(&sw, out, root, "swiftlint", "medium");
        assert_eq!(v.len(), 3);
        assert_eq!((v[0].file.as_str(), v[0].line, v[0].rule.as_str(), v[0].severity.as_str()), ("Sources/App.swift", 12, "line_length", "medium"));
        assert_eq!(v[0].message, "Line should be 120 characters or less");
        assert_eq!((v[1].line, v[1].rule.as_str(), v[1].severity.as_str()), (30, "force_cast", "high"), "列が無い行も読める");
        assert_eq!((v[2].rule.as_str(), v[2].severity.as_str(), v[2].message.as_str()), ("swiftlint", "medium", "Trailing newline"), "rule が無ければ、id");

        let sc = Regex::new(r"^(?P<file>.+?):(?P<line>\d+):\d+: (?P<severity>\w+): (?P<message>.*?)(?: \[(?P<rule>SC\d+)\])?$").unwrap();
        let v = parse_regex(&sc, "run.sh:7:12: note: Double quote to prevent globbing [SC2086]\nrun.sh:9:1: error: Syntax problem [SC1073]", root, "shellcheck", "medium");
        assert_eq!((v[0].rule.as_str(), v[0].severity.as_str(), v[0].line), ("SC2086", "low", 7));
        assert_eq!((v[1].rule.as_str(), v[1].severity.as_str()), ("SC1073", "high"));

        // severity が無いパターンは、default_severity
        let rf = Regex::new(r"^(?P<file>.+?):(?P<line>\d+):\d+: (?P<rule>[A-Z]+\d+) (?P<message>.*)$").unwrap();
        let v = parse_regex(&rf, "app.py:3:8: F401 `os` imported but unused", root, "ruff", "low");
        assert_eq!((v[0].rule.as_str(), v[0].severity.as_str(), v[0].file.as_str()), ("F401", "low", "app.py"));
        // 行番号が数字でないものは、飛ばす
        assert!(parse_regex(&Regex::new(r"^(?P<file>.+?):(?P<line>\w+): (?P<message>.*)$").unwrap(), "a.rs:xx: m", root, "t", "medium").is_empty());
    }

    #[test]
    fn severity_words_map_sensibly() {
        for (s, want) in [("error", "high"), ("Fatal", "high"), ("warning", "medium"), ("WARN", "medium"), ("note", "low"), ("info", "low"), ("convention", "low")] {
            assert_eq!(severity_of(s), Some(want), "{s}");
        }
        assert_eq!(severity_of("???"), None);
    }

    #[test]
    fn sarif_reads_relative_file_uri_and_percent_encoded_locations() {
        let root = Path::new("/repo");
        let doc = r#"{"version":"2.1.0","runs":[{"tool":{"driver":{"name":"x"}},"results":[
            {"ruleId":"R1","level":"error","message":{"text":"bad"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"src/a.go"},"region":{"startLine":4}}}]},
            {"ruleId":"R2","message":{"text":"default level"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"file:///repo/dir%20x/b.go"},"region":{"startLine":9}}}]},
            {"rule":{"id":"R3"},"level":"note","message":{"markdown":"md"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"c.go"},"region":{"startLine":1}}}]},
            {"ruleId":"R4","level":"none","message":{"text":"none level"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"e.go"},"region":{"startLine":10}}}]},
            {"ruleId":"ROVER","message":{"text":"line overflow"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"f.go"},"region":{"startLine":4294967297}}}]},
            {"ruleId":"NOLOC","message":{"text":"no location"}},
            {"ruleId":"NOLINE","message":{"text":"no line"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"d.go"}}}]}
        ]}]}"#;
        let v = parse_sarif(doc, root).unwrap();
        assert_eq!(v.len(), 4, "場所や行が無いもの、行番号が u32 を超えるものは飛ばす");
        assert_eq!((v[0].file.as_str(), v[0].line, v[0].rule.as_str(), v[0].severity.as_str(), v[0].message.as_str()), ("src/a.go", 4, "R1", "high", "bad"));
        assert_eq!((v[1].file.as_str(), v[1].severity.as_str()), ("dir x/b.go", "medium"), "file:// と %20 を解く。level が無ければ warning");
        assert_eq!((v[2].rule.as_str(), v[2].severity.as_str(), v[2].message.as_str()), ("R3", "low", "md"));
        assert_eq!((v[3].rule.as_str(), v[3].severity.as_str(), v[3].line), ("R4", "low", 10), "level が none のときは low");
        assert!(parse_sarif("not json", root).unwrap_err().contains("SARIF として読めない"));
        assert!(parse_sarif("{}", root).unwrap_err().contains("`runs` が無い"));
        assert!(parse_sarif(r#"{"runs":[]}"#, root).unwrap().is_empty());
        assert_eq!(percent_decode("a%20b%2Fc%zz%"), "a b/c%zz%", "不正な % は、そのまま");
    }

    fn custom(json: &str) -> Custom {
        let d = scratch("custom");
        write(&d, json);
        let c = load(&d).unwrap().remove(0);
        let _ = std::fs::remove_dir_all(&d);
        c
    }

    #[test]
    fn read_checks_exit_codes_and_warns_when_the_pattern_matches_nothing() {
        let c = custom(SWIFTLINT);
        let root = Path::new("/repo");
        // ok_exit_codes（0・2）にある終了コードなら、読む
        let (v, notes) = c.read("a.swift:3:1: warning: m (rule_x)\n", "", Some(2), root).unwrap();
        assert_eq!(v.len(), 1);
        assert!(notes.is_empty());
        // 指摘なし（出力が空）は、警告しない
        let (v, notes) = c.read("", "", Some(0), root).unwrap();
        assert!(v.is_empty() && notes.is_empty());
        // 範囲外の終了コードは、失敗。理由と stderr を添える
        let e = c.read("", "boom: config missing\n", Some(1), root).unwrap_err();
        assert!(e.contains("終了コード 1") && e.contains("ok_exit_codes") && e.contains("boom: config missing"), "{e}");
        assert!(c.read("", "", None, root).unwrap_err().contains("シグナル"));
        // 出力はあるのに、1 件も読めなかった → 警告（黙って「指摘なし」にしない）
        let (v, notes) = c.read("Linting 'App.swift' (1/1)\nsomething else\n", "", Some(0), root).unwrap();
        assert!(v.is_empty());
        assert!(notes[0].contains("pattern") && notes[0].contains("2 行") && notes[0].contains("Linting 'App.swift'"), "{notes:?}");
    }

    #[test]
    fn read_uses_the_configured_output_stream_and_sarif() {
        let regex_err = custom(&SWIFTLINT.replace(r#""ok_exit_codes": [0, 2],"#, r#""ok_exit_codes": [0, 2], "output": "stderr","#));
        let root = Path::new("/repo");
        assert_eq!(regex_err.read("a.swift:1:1: warning: ignored\n", "b.swift:2:1: error: from stderr\n", Some(0), root).unwrap().0.len(), 1);
        let both = custom(&SWIFTLINT.replace(r#""ok_exit_codes": [0, 2],"#, r#""ok_exit_codes": [0, 2], "output": "both","#));
        assert_eq!(both.read("a.swift:1:1: warning: x\n", "b.swift:2:1: error: y\n", Some(0), root).unwrap().0.len(), 2);
        let sarif = custom(&def_with(r#","ok_exit_codes":[0]"#));
        let doc = r#"{"runs":[{"results":[{"ruleId":"R","message":{"text":"m"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"a.x"},"region":{"startLine":2}}}]}]}]}"#;
        assert_eq!(sarif.read(doc, "", Some(0), root).unwrap().0.len(), 1);
        let e = sarif.read("garbage", "warn: x", Some(0), root).unwrap_err();
        assert!(e.contains("SARIF として読めない") && e.contains("warn: x"), "{e}");
    }

    #[test]
    fn a_missing_program_is_reported_only_when_the_marker_says_it_is_wanted() {
        let repo = scratch("miss-repo");
        let json = |detect: &str| format!(r#"{{"linters":[{{"id":"nope","title":"Nope","extensions":["x"],"command":["definitely-not-installed-xyz"],"format":"sarif"{detect}}}]}}"#);
        let d = scratch("miss-data");
        write(&d, &json(r#","detect":["Package.swift"]"#));
        let c = &load(&d).unwrap()[0];
        assert!(c.missing(&repo).is_none(), "目印が無い＝このリポジトリに出番は無い");
        std::fs::write(repo.join("Package.swift"), "").unwrap();
        let m = c.missing(&repo).expect("目印があるのに入っていない");
        assert_eq!((m.id.as_str(), m.title.as_str()), ("lint-nope", "Nope"));
        assert!(m.hint.contains("definitely-not-installed-xyz") && m.hint.contains(FILE), "{}", m.hint);
        write(&d, &json(""));
        assert!(load(&d).unwrap()[0].missing(&repo).is_none(), "detect が無ければ、出番があるか分からないので、何も言わない");
    }

    #[test]
    fn builtin_ids_are_reserved_for_the_new_tools_too() {
        for id in ["swiftlint", "shellcheck"] {
            let d = scratch(&format!("reserved-{id}"));
            write(&d, &format!(r#"{{"linters":[{{"id":"{id}","title":"X","extensions":["x"],"command":["x"],"format":"sarif"}}]}}"#));
            let e = load(&d).err().unwrap();
            assert!(e.contains("組み込み"), "{e}");
        }
    }

    #[test]
    fn available_needs_the_program_and_a_marker_when_given() {
        let repo = scratch("avail-repo");
        // 実行ファイルは、/ を含むパスで指す（PATH に依らず、試験できる）
        let prog = scratch("avail-prog").join("fake-lint");
        std::fs::write(&prog, "#!/bin/sh\n").unwrap();
        let json = |cmd: &str, detect: &str| format!(r#"{{"linters":[{{"id":"fake","title":"Fake","extensions":["x"],"command":["{cmd}"],"format":"sarif"{detect}}}]}}"#);
        let with_marker = custom(&json(prog.to_str().unwrap(), r#","detect":["Package.swift"]"#));
        assert!(!with_marker.available(&repo), "目印が無い");
        std::fs::write(repo.join("Package.swift"), "").unwrap();
        assert!(with_marker.available(&repo));
        let no_marker = custom(&json(prog.to_str().unwrap(), ""));
        assert!(no_marker.available(&scratch("avail-empty")), "detect を省略すれば、実行ファイルがあるだけで出す");
        let missing = custom(&json("/nonexistent/linter", ""));
        assert!(!missing.available(&repo), "実行ファイルが無い");
        assert!(missing.invocation(&repo, &[]).unwrap_err().contains("見つかりません"));
        // ファイルの渡し方
        let inv = no_marker.invocation(&repo, &["a.x".into(), "b.x".into()]).unwrap();
        assert_eq!(inv.args, vec!["a.x", "b.x"]);
        let none = custom(&json(prog.to_str().unwrap(), r#","files":"none""#));
        assert!(none.whole_project() && none.invocation(&repo, &["a.x".into()]).unwrap().args.is_empty());
    }

    #[test]
    fn the_recipe_shows_the_exact_command_and_warns_it_is_the_users_own() {
        let prog = scratch("recipe-prog").join("fake-lint");
        std::fs::write(&prog, "#!/bin/sh\n").unwrap();
        let c = custom(&format!(r#"{{"linters":[{{"id":"fake","title":"Fake","extensions":["x","y"],"command":["{}","--flag"],"format":"sarif"}}]}}"#, prog.display()));
        let r = c.recipe(Path::new("/repo"));
        assert_eq!((r.id.as_str(), r.title.as_str(), r.kind, r.source.as_str()), ("lint-fake", "リンター: Fake", Kind::Tool, "user"));
        assert!(r.prompt.contains("--flag <差分のファイル>"), "{}", r.prompt);
        assert!(r.prompt.contains("linters.json") && r.prompt.contains("信頼できるリポジトリでだけ"));
        assert!(r.prompt.contains("x, y"));
        assert!(!r.prompt.contains("\\n"), "改行が、文字のまま残らない");
    }

    #[test]
    fn a_repo_side_linters_file_is_never_read_but_is_announced() {
        let repo = scratch("repo-notice");
        assert!(repo_notice(&repo).is_none());
        std::fs::create_dir_all(repo.join(".mrcodereview")).unwrap();
        std::fs::write(repo.join(".mrcodereview/linters.json"), r#"{"linters":[{"id":"evil","title":"E","extensions":["js"],"command":["rm","-rf","/"],"format":"sarif"}]}"#).unwrap();
        let n = repo_notice(&repo).unwrap();
        assert!(n.contains("読みません") && n.contains("あなた個人の"), "{n}");
        // load は、渡されたディレクトリの直下の linters.json だけを見る。リポジトリの根を渡しても、
        // 中の .mrcodereview/linters.json（上の evil）は、読まない
        assert!(load(&repo).unwrap().is_empty());
        assert!(load(&scratch("repo-notice-data")).unwrap().is_empty());
    }
}
