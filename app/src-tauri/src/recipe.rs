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
        needs_spec: text.contains(mrcr_core::recipe::SPEC_MARKER),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_named_like_the_removed_linter_are_ordinary_repo_recipes_now() {
        let dir = std::env::temp_dir().join(format!("mrcr-recipe-lintname-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let rdir = dir.join(".mrcodereview").join("recipes");
        std::fs::create_dir_all(&rdir).unwrap();
        // 以前は、`lint` と `lint-` 始まりの名前は、リンター用に予約されていて、一覧から除かれていた
        std::fs::write(rdir.join("lint.md"), "# 名前だけ lint\n本文\n").unwrap();
        std::fs::write(rdir.join("lint-x.md"), "# lint-x\n本文\n").unwrap();

        let builtin: Vec<String> = list_with_lang(None, "ja").into_iter().map(|r| r.id).collect();
        let all = list_with_lang(Some(&dir), "ja");
        for id in ["lint", "lint-x"] {
            let r = all.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("{id} が、観点の一覧に出ない"));
            assert_eq!((r.source.as_str(), r.kind), ("repo", Kind::Llm), "{id}");
            assert!(find_with_lang(Some(&dir), id, "ja").is_some(), "{id} を、id で引ける");
        }
        for id in &builtin {
            assert!(all.iter().any(|r| &r.id == id), "組み込みの観点 {id} が、消えた");
        }
        assert_eq!(all.len(), builtin.len() + 2, "足したのは、この 2 つだけ");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
