//! 基準の文書（設計書・仕様書）の読み込み、節抽出、パス検証、照合。
//!
//! 構想 §23（基準の文書との一致）に準拠:
//! - Markdown（.md）とプレーンテキスト（.txt）のみ対応
//! - リポジトリ境界を厳格に遵守（パストラバーサルや外部シンボリックリンクを拒否）
//! - 大きさの上限超過時はサイレント切り捨てを禁止し、明確なエラーで中断
//! - spec_quote の原文完全一致照合によるハルシネーション排除

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// 基準文書の最大許容文字数（サイレント切り捨て防止。超過時はエラーで中断）
pub const MAX_SPEC_CHARS: usize = 30_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SectionMeta {
    pub level: usize,
    pub title: String,
    pub line_start: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SpecSource {
    File {
        path: String,
        section: Option<String>,
    },
    DirectPaste,
    /// 旧バージョン互換用（デシリアライズ専用）
    #[serde(rename = "Paste")]
    #[doc(hidden)]
    _LegacyPaste {
        title: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpecContext {
    pub source: SpecSource,
    pub display_name: String,
    pub content: String,
    pub content_hash: String,
    pub char_count: usize,
}

impl SpecContext {
    pub fn new(source: SpecSource, display_name: String, content: String) -> Result<Self, String> {
        let trimmed = content.trim().to_string();
        if trimmed.is_empty() {
            return Err("基準の文書が空です".into());
        }
        let char_count = trimmed.chars().count();
        if char_count > MAX_SPEC_CHARS {
            return Err(format!(
                "基準の文書の文字数（{} 文字）が上限（{} 文字）を超えています。対象の節（見出し）を絞り込むか、不要な記述を削ってください。",
                char_count, MAX_SPEC_CHARS
            ));
        }
        let hash = fnv1a64(&trimmed);
        Ok(Self {
            source,
            display_name,
            content: trimmed,
            content_hash: format!("{hash:016x}"),
            char_count,
        })
    }
}

/// FNV-1a 64bit ハッシュ
pub fn fnv1a64(text: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for b in text.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// リポジトリ内の相対パスを安全に解決する。
/// `..` や絶対パス、シンボリックリンク等によるリポジトリ境界外への脱出を拒絶する。
pub fn safe_repo_relative_path(repo_root: &Path, rel_path: &Path) -> Result<PathBuf, String> {
    if rel_path.is_absolute() {
        return Err("リポジトリからの相対パスを指定してください".into());
    }
    for c in rel_path.components() {
        if matches!(c, Component::ParentDir) {
            return Err("リポジトリの外を指すパス（..）は指定できません".into());
        }
    }
    let canonical_root = repo_root
        .canonicalize()
        .map_err(|e| format!("リポジトリのパスを解決できません: {e}"))?;
    let target = repo_root.join(rel_path);
    let canonical_target = target
        .canonicalize()
        .map_err(|e| format!("指定されたファイルが見つかりません: {e}"))?;

    if !canonical_target.starts_with(&canonical_root) {
        return Err("リポジトリ外へのシンボリックリンクは指定できません".into());
    }
    Ok(canonical_target)
}

/// 拡張子が `.md` または `.txt` か判定する。
pub fn is_supported_spec_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(),
        Some("md") | Some("markdown") | Some("txt")
    )
}

/// Markdown から見出し一覧（章・節）を抽出する。
pub fn parse_markdown_sections(text: &str) -> Vec<SectionMeta> {
    let mut sections = Vec::new();
    let mut in_code_block = false;

    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix('#') {
            let mut level = 1;
            let mut valid = false;
            for c in rest.chars() {
                if c == '#' {
                    level += 1;
                } else if c.is_whitespace() {
                    valid = true;
                    break;
                } else {
                    break;
                }
            }
            if valid && level <= 6 {
                let title = rest.trim_start_matches('#').trim().to_string();
                if !title.is_empty() {
                    sections.push(SectionMeta {
                        level,
                        title,
                        line_start: idx + 1,
                    });
                }
            }
        }
    }
    sections
}

/// Markdown の中から特定の見出し（節）の本文を抽出する。
/// 次の同レベルまたは上位の見出しが現れるまでをその節の内容とする。
pub fn extract_section_content(text: &str, heading_title: &str) -> Result<String, String> {
    let lines: Vec<&str> = text.lines().collect();
    let sections = parse_markdown_sections(text);

    let target_idx = sections
        .iter()
        .position(|s| s.title == heading_title)
        .or_else(|| {
            sections
                .iter()
                .position(|s| s.title.starts_with(heading_title))
        })
        .ok_or_else(|| format!("指定された見出し「{heading_title}」が見つかりません"))?;

    let target = &sections[target_idx];
    let start_line = target.line_start - 1; // 0-indexed

    // 次の「同レベル以下の見出し（level <= target.level）」を探す
    let end_line = sections[target_idx + 1..]
        .iter()
        .find(|s| s.level <= target.level)
        .map(|s| s.line_start - 1)
        .unwrap_or(lines.len());

    let section_text = lines[start_line..end_line].join("\n");
    Ok(section_text.trim().to_string())
}

/// 基準の文書として渡せる件数の上限（複数ファイルの指定。要件・設計・タスクの 3 枚＋余裕）。
pub const MAX_SPEC_FILES: usize = 5;
/// 渡した基準の文書の、合計の文字数の上限（サイレント切り捨て防止。超過時はエラー）。
pub const MAX_SPEC_TOTAL_CHARS: usize = 30_000;

/// `PATH` または `PATH:##節` を、（パス, 節）に分ける。
pub fn split_spec_arg(arg: &str) -> (String, Option<String>) {
    match arg.split_once(':') {
        Some((p, s)) => (p.trim().to_string(), Some(s.trim().trim_start_matches('#').trim().to_string())),
        None => (arg.trim().to_string(), None),
    }
}

/// リポジトリ内のファイル 1 つ（任意で節）を、基準の文書として読む。
pub fn load_spec_file(repo_root: &Path, arg: &str) -> Result<SpecContext, String> {
    let (rel, section) = split_spec_arg(arg);
    let safe_path = safe_repo_relative_path(repo_root, Path::new(&rel))?;
    if !is_supported_spec_extension(&safe_path) {
        return Err("基準の文書は .md または .txt のみ対応しています".into());
    }
    let full_text = std::fs::read_to_string(&safe_path)
        .map_err(|e| format!("基準文書を読み込めません: {}: {e}", safe_path.display()))?;
    let (content, display_name) = match section.as_deref().filter(|s| !s.is_empty()) {
        Some(sec) => (extract_section_content(&full_text, sec)?, format!("{rel} # {sec}")),
        None => (full_text, rel.clone()),
    };
    SpecContext::new(SpecSource::File { path: rel, section }, display_name, content)
}

/// 複数の基準の文書の、件数と合計の文字数を確かめる。超えたら、切り捨てずにエラー（内訳つき）。
pub fn check_spec_set(ctxs: &[SpecContext]) -> Result<(), String> {
    if ctxs.len() > MAX_SPEC_FILES {
        return Err(format!(
            "基準の文書が多すぎます（{} 件）。上限は {} 件です。必要なものに絞ってください。",
            ctxs.len(),
            MAX_SPEC_FILES
        ));
    }
    let total: usize = ctxs.iter().map(|c| c.char_count).sum();
    if total > MAX_SPEC_TOTAL_CHARS {
        let breakdown = ctxs.iter().map(|c| format!("{}: {} 文字", c.display_name, c.char_count)).collect::<Vec<_>>().join("、");
        return Err(format!(
            "基準の文書の合計の文字数（{total} 文字）が上限（{MAX_SPEC_TOTAL_CHARS} 文字）を超えています（{breakdown}）。対象の節（見出し）を絞り込むか、文書を減らしてください。"
        ));
    }
    Ok(())
}

/// `--spec` の指定（複数可）を、基準の文書の一覧にする。指定した順のまま。
///
/// - 1 件でも読めなければ、全体を失敗にする（一部だけで続けない）。
/// - 同じファイルの同じ節の重複は、1 件にまとめる。
/// - `-` は標準入力（1 回まで）。`read_stdin` が本文を返す。
/// - 件数と合計の文字数は `check_spec_set` で確かめる。
pub fn load_spec_set(
    repo_root: &Path,
    args: &[String],
    read_stdin: &mut dyn FnMut() -> Result<String, String>,
) -> Result<Vec<SpecContext>, String> {
    let mut out: Vec<SpecContext> = Vec::new();
    let mut seen: Vec<(PathBuf, Option<String>)> = Vec::new();
    let mut stdin_used = false;
    for arg in args {
        let arg = arg.trim();
        if arg == "-" {
            if stdin_used {
                return Err("標準入力（--spec -）は 1 回までです".into());
            }
            stdin_used = true;
            let text = read_stdin().map_err(|e| format!("標準入力からの読み込みに失敗しました: {e}"))?;
            let count = text.chars().count();
            out.push(SpecContext::new(SpecSource::DirectPaste, format!("標準入力の要件（{count}文字）"), text)?);
            continue;
        }
        let (rel, section) = split_spec_arg(arg);
        let key = (safe_repo_relative_path(repo_root, Path::new(&rel))?, section.filter(|s| !s.is_empty()));
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(load_spec_file(repo_root, arg)?);
    }
    check_spec_set(&out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_headings_and_skips_code_blocks() {
        let md = r#"# タイトル
はじめに
```rust
# not a heading
let x = 1;
```
## 第1章 認証
認証の仕様です。
### 1.1 トークン
JWTを使用。
## 第2章 決済
決済の仕様です。
"#;
        let secs = parse_markdown_sections(md);
        assert_eq!(secs.len(), 4);
        assert_eq!(secs[0].title, "タイトル");
        assert_eq!(secs[0].level, 1);
        assert_eq!(secs[1].title, "第1章 認証");
        assert_eq!(secs[1].level, 2);
        assert_eq!(secs[2].title, "1.1 トークン");
        assert_eq!(secs[2].level, 3);
        assert_eq!(secs[3].title, "第2章 決済");
        assert_eq!(secs[3].level, 2);
    }

    #[test]
    fn extracts_section_content_up_to_next_same_level() {
        let md = r#"# タイトル
概要

## 認証仕様
セッションは15分で失効する。
延長は不可とする。

### 詳細メモ
内部実装のメモ。

## 決済仕様
決済APIを呼ぶ。
"#;
        let extracted = extract_section_content(md, "認証仕様").unwrap();
        assert!(extracted.contains("## 認証仕様"));
        assert!(extracted.contains("セッションは15分で失効する。"));
        assert!(extracted.contains("### 詳細メモ"));
        assert!(!extracted.contains("## 決済仕様"));
    }

    #[test]
    fn size_limit_prevents_silent_truncation() {
        // 上限ちょうどは Ok
        let exact = "a".repeat(MAX_SPEC_CHARS);
        let ctx = SpecContext::new(SpecSource::DirectPaste, "上限ちょうどの仕様".into(), exact).unwrap();
        assert_eq!(ctx.char_count, MAX_SPEC_CHARS);

        // 1文字オーバーは Err
        let huge = "a".repeat(MAX_SPEC_CHARS + 1);
        let err = SpecContext::new(SpecSource::DirectPaste, "巨大な仕様".into(), huge).unwrap_err();
        assert!(err.contains("上限"));
        assert!(err.contains("超えています"));

        // 空白のみは「空です」の Err
        let empty_err = SpecContext::new(SpecSource::DirectPaste, "空の仕様".into(), "   \n\t  ".into()).unwrap_err();
        assert!(empty_err.contains("空です"));
    }

    #[test]
    fn safe_repo_relative_path_blocks_parent_escape() {
        let temp = std::env::temp_dir().join(format!("spec-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let repo = temp.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let inside = repo.join("spec.md");
        fs::write(&inside, "inside spec").unwrap();
        let outside = temp.join("outside.txt");
        fs::write(&outside, "outside").unwrap();

        // リポジトリ内の正常ファイルは Ok
        let resolved = safe_repo_relative_path(&repo, Path::new("spec.md")).unwrap();
        assert_eq!(resolved, inside.canonicalize().unwrap());

        // 相対パスの「..」は Err
        let err_parent = safe_repo_relative_path(&repo, Path::new("../outside.txt")).unwrap_err();
        assert!(err_parent.contains(".."));

        // 絶対パスは Err
        let err_abs = safe_repo_relative_path(&repo, Path::new("/etc/hosts")).unwrap_err();
        assert!(err_abs.contains("相対パス"));

        // 外部へのシンボリックリンクは Err、内部へのシンボリックリンクは Ok
        #[cfg(unix)]
        {
            let link_outside = repo.join("symlink_outside.txt");
            std::os::unix::fs::symlink(&outside, &link_outside).unwrap();
            let err_link = safe_repo_relative_path(&repo, Path::new("symlink_outside.txt")).unwrap_err();
            assert!(err_link.contains("シンボリックリンク") || err_link.contains("リポジトリ外"));

            let link_inside = repo.join("symlink_inside.txt");
            std::os::unix::fs::symlink(&inside, &link_inside).unwrap();
            let ok_link = safe_repo_relative_path(&repo, Path::new("symlink_inside.txt")).unwrap();
            assert_eq!(ok_link, inside.canonicalize().unwrap());
        }

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn extracts_section_prefers_exact_match_over_prefix() {
        let md = r#"# ドキュメント

## 認証仕様の補足
ここは補足情報です。

## 認証仕様
ここは本編の認証仕様です。

## 決済仕様
決済情報です。
"#;
        // 「認証仕様」を指定したとき、「認証仕様の補足」ではなく完全一致の「認証仕様」が選ばれること
        let extracted = extract_section_content(md, "認証仕様").unwrap();
        assert!(extracted.contains("ここは本編の認証仕様です。"));
        assert!(!extracted.contains("ここは補足情報です。"));

        // 末尾の節（次の同レベル見出しが無い）も最後まで取れること
        let last_extracted = extract_section_content(md, "決済仕様").unwrap();
        assert!(last_extracted.contains("決済情報です。"));

        // 存在しない見出しは Err になること
        let not_found = extract_section_content(md, "存在しない節").unwrap_err();
        assert!(not_found.contains("見つかりません"));
    }

    #[test]
    fn fnv1a64_deterministic() {
        assert_eq!(fnv1a64("hello world"), fnv1a64("hello world"));
        assert_ne!(fnv1a64("hello"), fnv1a64("world"));
    }

    fn repo_with(files: &[(&str, &str)]) -> (PathBuf, PathBuf) {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let base = std::env::temp_dir().join(format!("spec-set-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        for (p, c) in files {
            let f = repo.join(p);
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(f, c).unwrap();
        }
        (base, repo)
    }

    fn no_stdin() -> Result<String, String> {
        Err("標準入力は使わない".into())
    }

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn loads_several_files_in_the_given_order_with_sections() {
        let (base, repo) = repo_with(&[("docs/a.md", "# A\n本文A"), ("docs/b.md", "# B\n## 認証\nb の認証\n## 決済\nb の決済"), ("c.txt", "テキスト C")]);
        let set = load_spec_set(&repo, &args(&["docs/b.md:##認証", "docs/a.md", "c.txt"]), &mut no_stdin).unwrap();
        let names: Vec<&str> = set.iter().map(|c| c.display_name.as_str()).collect();
        assert_eq!(names, ["docs/b.md # 認証", "docs/a.md", "c.txt"], "指定した順");
        assert!(set[0].content.contains("b の認証") && !set[0].content.contains("b の決済"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn one_bad_file_fails_the_whole_set() {
        let (base, repo) = repo_with(&[("a.md", "A"), ("img.png", "x")]);
        for bad in ["missing.md", "img.png", "../outside.md", "/etc/hosts"] {
            let e = load_spec_set(&repo, &args(&["a.md", bad]), &mut no_stdin).unwrap_err();
            assert!(!e.is_empty(), "{bad}");
        }
        let _ = fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_inside_the_repo_that_points_outside_fails_the_whole_set() {
        let (base, repo) = repo_with(&[("a.md", "A"), ("inside.md", "内側の文書")]);
        let outside = base.join("secret.md");
        fs::write(&outside, "外のファイルの中身").unwrap();
        fs::create_dir_all(repo.join("docs")).unwrap();
        std::os::unix::fs::symlink(&outside, repo.join("docs/link.md")).unwrap();
        // 複数指定の中に混ぜても、1 件でも外へ出るリンクがあれば、全体が失敗する（外の中身は読まない）
        for order in [["a.md", "docs/link.md"], ["docs/link.md", "a.md"]] {
            let e = load_spec_set(&repo, &args(&order), &mut no_stdin).unwrap_err();
            assert!(e.contains("シンボリックリンク") || e.contains("リポジトリ外"), "{order:?}: {e}");
        }
        // 内側を指すリンクは、今までどおり通る
        std::os::unix::fs::symlink(repo.join("inside.md"), repo.join("docs/ok.md")).unwrap();
        assert_eq!(load_spec_set(&repo, &args(&["a.md", "docs/ok.md"]), &mut no_stdin).unwrap().len(), 2);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn duplicates_are_merged_but_other_sections_of_the_same_file_are_kept() {
        let (base, repo) = repo_with(&[("a.md", "# A\n## x\n1\n## y\n2")]);
        let set = load_spec_set(&repo, &args(&["a.md", "./a.md", "a.md"]), &mut no_stdin).unwrap();
        assert_eq!(set.len(), 1, "同じファイルは 1 件");
        let set = load_spec_set(&repo, &args(&["a.md:##x", "a.md:x", "a.md:##y"]), &mut no_stdin).unwrap();
        assert_eq!(set.len(), 2, "同じ節は 1 件・別の節は別");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn more_than_the_file_limit_is_refused() {
        let files: Vec<(String, String)> = (0..=MAX_SPEC_FILES).map(|i| (format!("f{i}.md"), "x".to_string())).collect();
        let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let (base, repo) = repo_with(&refs);
        let all: Vec<String> = files.iter().map(|f| f.0.clone()).collect();
        let e = load_spec_set(&repo, &all, &mut no_stdin).unwrap_err();
        assert!(e.contains("多すぎます") && e.contains(&MAX_SPEC_FILES.to_string()), "{e}");
        assert!(load_spec_set(&repo, &all[..MAX_SPEC_FILES], &mut no_stdin).is_ok(), "上限ちょうどは通る");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn total_size_limit_is_exact_and_the_error_shows_the_breakdown() {
        let half = "a".repeat(MAX_SPEC_TOTAL_CHARS / 2);
        let (base, repo) = repo_with(&[("a.md", &half), ("b.md", &half), ("c.md", "z")]);
        assert!(load_spec_set(&repo, &args(&["a.md", "b.md"]), &mut no_stdin).is_ok(), "合計がちょうど上限なら通る");
        let e = load_spec_set(&repo, &args(&["a.md", "b.md", "c.md"]), &mut no_stdin).unwrap_err();
        assert!(e.contains("超えています") && e.contains("a.md: 15000 文字") && e.contains("c.md: 1 文字"), "{e}");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn stdin_counts_as_one_document_and_only_once() {
        let (base, repo) = repo_with(&[("a.md", "A")]);
        let mut calls = 0;
        let mut stdin = || {
            calls += 1;
            Ok("標準入力の本文".to_string())
        };
        let set = load_spec_set(&repo, &args(&["a.md", "-"]), &mut stdin).unwrap();
        assert_eq!(set.len(), 2);
        assert!(set[1].display_name.starts_with("標準入力の要件"));
        let e = load_spec_set(&repo, &args(&["-", "-"]), &mut stdin).unwrap_err();
        assert!(e.contains("1 回まで"), "{e}");
        assert_eq!(calls, 2, "2 回目の `-` では読まない");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn split_spec_arg_separates_the_section() {
        assert_eq!(split_spec_arg("docs/a.md"), ("docs/a.md".into(), None));
        assert_eq!(split_spec_arg(" docs/a.md : ## 認証 "), ("docs/a.md".into(), Some("認証".into())));
    }
}
