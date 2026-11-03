use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Llm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub title: String,
    /// "builtin" | "repo"
    pub source: String,
    /// LLM レシピでは、送るプロンプトの本文。
    pub prompt: String,
    #[serde(default)]
    pub kind: Kind,
    /// 差分に出ていない関連コードも、プロンプトに添えるか。レシピに `<!-- related -->` と書くと有効。
    #[serde(default)]
    pub needs_related: bool,
}

pub const RELATED_MARKER: &str = "<!-- related -->";

pub fn builtin_for(lang: &str) -> &'static [(&'static str, &'static str)] {
    crate::i18n::Lang::from_code(lang).unwrap_or_default().texts().builtin_recipes
}

fn parse(id: &str, source: &str, text: &str) -> Recipe {
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
        prompt: text.to_string(),
        kind: Kind::Llm,
        needs_related: text.contains(RELATED_MARKER),
    }
}

pub fn repo_recipe_files(repo: &Path) -> Vec<(String, String)> {
    let dir = repo.join(".mrcodereview").join("recipes");
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let id = stem.trim().to_lowercase();
        if id.is_empty() {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&p) {
            out.push((id, content));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

pub fn list_with_lang(repo: Option<&Path>, lang: &str) -> Vec<Recipe> {
    let t = crate::i18n::Lang::from_code(lang).unwrap_or_default().texts();
    let repo_recipes = repo.map(repo_recipe_files).unwrap_or_default();

    let mut out = Vec::new();
    // 1. 組み込み観点（リポジトリ側の同名ファイルがあれば上書き）
    for &(id, default_text) in t.builtin_recipes {
        if let Some((_, custom_text)) = repo_recipes.iter().find(|(rid, _)| rid == id) {
            out.push(parse(id, "repo", custom_text));
        } else {
            out.push(parse(id, "builtin", default_text));
        }
    }

    // 2. リポジトリ側で新規追加されたカスタム観点
    for (id, text) in &repo_recipes {
        if !t.builtin_recipes.iter().any(|(bid, _)| bid == id) {
            out.push(parse(id, "repo", text));
        }
    }

    out
}

pub fn find_with_lang(repo: Option<&Path>, id: &str, lang: &str) -> Option<Recipe> {
    list_with_lang(repo, lang).into_iter().find(|r| r.id == id)
}

pub fn compose_with_lang(recipe: &Recipe, related_block: &str, diff_text: &str, lang: &str) -> String {
    let t = crate::i18n::Lang::from_code(lang).unwrap_or_default().texts();
    let related = if related_block.is_empty() {
        String::new()
    } else {
        format!("{related_block}\n")
    };
    let fmt = t.output_format;
    let diff_title = t.diff_heading;
    format!(
        "{}\n\n{}\n\n{}{diff_title}\n\n```diff\n{}```\n",
        recipe.prompt, fmt, related, diff_text
    )
}
