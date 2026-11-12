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
}
