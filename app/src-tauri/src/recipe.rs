//! レビューの観点（ボタンの正体 - GUI 版）。
//!
//! コアの型・文面・プロンプト組み立ては `mrcr_core::recipe` を使い、
//! ここでは、リポジトリの `.mrcodereview/recipes/*.md` の読み込みを扱う。

pub use mrcr_core::recipe::*;

use std::path::{Path, PathBuf};

fn parse_recipe(id: &str, source: &str, text: &str) -> Recipe {
    let title = text
        .lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| id.to_string());
    Recipe {
        id: id.into(),
        title,
        source: source.into(),
        prompt: text.trim().to_string(),
        kind: Kind::Llm,
        needs_related: text.contains(RELATED_MARKER),
    }
}

/// リポジトリの `.mrcodereview/recipes/*.md`（id とパス）。
fn repo_recipe_files(repo: &Path) -> Vec<(String, PathBuf)> {
    let dir = repo.join(".mrcodereview").join("recipes");
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut paths: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    paths.sort();
    paths
        .into_iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .filter_map(|p| {
            let id = p.file_stem().and_then(|s| s.to_str())?.to_string();
            Some((id, p))
        })
        .collect()
}

pub fn list_with_lang(repo: Option<&Path>, lang: &str) -> Vec<Recipe> {
    let mut out: Vec<Recipe> = builtin_for(lang).iter().map(|(id, text)| parse_recipe(id, "builtin", text)).collect();
    if let Some(repo) = repo {
        for (id, p) in repo_recipe_files(repo) {
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let r = parse_recipe(&id, "repo", &text);
            match out.iter_mut().find(|x| x.id == r.id) {
                Some(slot) => *slot = r,
                None => out.push(r),
            }
        }
    }
    out
}

pub fn find_with_lang(repo: Option<&Path>, id: &str, lang: &str) -> Option<Recipe> {
    list_with_lang(repo, lang).into_iter().find(|r| r.id == id)
}
