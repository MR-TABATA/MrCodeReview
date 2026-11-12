//! レビューの観点（ボタンの正体 - GUI 版）。
//!
//! コアの型・文面・プロンプト組み立ては `mrcr_core::recipe` を使い、
//! ここでは、リポジトリの `.mrcodereview/recipes/*.md` の読み込みと、リンターツール（`lint.rs`, `linters.rs`）の連携を扱う。

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

/// リポジトリの `.mrcodereview/recipes/*.md`（id とパス）。ツール実行型の予約 id は除く。
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
            (id != crate::lint::LINT_ID && !id.starts_with(crate::linters::ID_PREFIX)).then_some((id, p))
        })
        .collect()
}

/// 試験用。日本語で読む。
#[cfg_attr(not(test), allow(dead_code))]
pub fn list(repo: Option<&Path>) -> Vec<Recipe> {
    list_with_lang(repo, &[], "ja")
}

pub fn list_with_lang(repo: Option<&Path>, custom: &[crate::linters::Custom], lang: &str) -> Vec<Recipe> {
    let t = crate::i18n::Lang::from_code(lang).unwrap_or_default().texts();
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
        let plan = crate::lint::plan(repo, custom, false);
        if plan.items.iter().any(|i| i.state != "missing") {
            let mut lr = crate::lint::lint_recipe(&plan);
            lr.title = t.recipe_title_lint.into();
            out.push(lr);
        }
    }
    out
}

#[allow(dead_code)]
pub fn find_with(repo: Option<&Path>, id: &str, custom: &[crate::linters::Custom]) -> Option<Recipe> {
    find_with_lang(repo, id, custom, crate::i18n::Lang::default().code())
}

pub fn find_with_lang(repo: Option<&Path>, id: &str, custom: &[crate::linters::Custom], lang: &str) -> Option<Recipe> {
    list_with_lang(repo, custom, lang).into_iter().find(|r| r.id == id).or_else(|| single_tool(repo?, id, custom))
}

fn single_tool(repo: &Path, id: &str, custom: &[crate::linters::Custom]) -> Option<Recipe> {
    if !id.starts_with(crate::linters::ID_PREFIX) {
        return None;
    }
    if let Some(t) = crate::lint::Tool::from_id(id) {
        return crate::lint::available(repo).contains(&t).then(|| crate::lint::recipe(t, repo));
    }
    custom.iter().find(|c| c.recipe_id() == id && c.available(repo)).map(|c| c.recipe(repo))
}
