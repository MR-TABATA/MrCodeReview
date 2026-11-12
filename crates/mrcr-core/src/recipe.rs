use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Llm,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
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
    /// 基準文書（設計書・仕様書）をプロンプトに差し込むか。レシピに `<!-- spec -->` と書くと有効。
    #[serde(default)]
    pub needs_spec: bool,
}

pub const RELATED_MARKER: &str = "<!-- related -->";
pub const SPEC_MARKER: &str = "<!-- spec -->";

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
        needs_spec: text.contains(SPEC_MARKER),
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

pub fn compose_with_lang_and_spec_checked(
    recipe: &Recipe,
    related_block: &str,
    spec_block: &str,
    diff_text: &str,
    lang: &str,
) -> Result<String, String> {
    if recipe.needs_spec && spec_block.trim().is_empty() {
        return Err("設計書（基準の文書）が指定されていません。ファイルを選択するか要件テキストを貼り付けてください。".into());
    }
    Ok(compose_with_lang_and_spec(recipe, related_block, spec_block, diff_text, lang))
}

pub fn compose_with_lang_and_spec(
    recipe: &Recipe,
    related_block: &str,
    spec_block: &str,
    diff_text: &str,
    lang: &str,
) -> String {
    let t = crate::i18n::Lang::from_code(lang).unwrap_or_default().texts();
    let related = if related_block.is_empty() {
        String::new()
    } else {
        format!("{related_block}\n")
    };
    let fmt = t.output_format;
    let diff_title = t.diff_heading;

    let base_prompt = if !spec_block.is_empty() && recipe.prompt.contains(SPEC_MARKER) {
        recipe.prompt.replace(SPEC_MARKER, spec_block)
    } else if !spec_block.is_empty() {
        format!("{}\n\n{spec_block}", recipe.prompt)
    } else {
        recipe.prompt.replace(SPEC_MARKER, "").trim_end().to_string()
    };

    format!(
        "{base_prompt}\n\n{}\n\n{}{diff_title}\n\n```diff\n{}```\n",
        fmt, related, diff_text
    )
}

pub fn compose_with_lang(recipe: &Recipe, related_block: &str, diff_text: &str, lang: &str) -> String {
    compose_with_lang_and_spec(recipe, related_block, "", diff_text, lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_recipe_with_spec_marker() {
        let r = parse("spec", "builtin", "# 基準との一致\n本文\n<!-- spec -->\n");
        assert_eq!(r.id, "spec");
        assert!(r.needs_spec);
        assert!(!r.needs_related);
    }

    #[test]
    fn composes_with_spec_block_replacing_marker() {
        let r = parse("spec", "builtin", "# 基準\n本文\n<!-- spec -->\nフッター");
        let composed = compose_with_lang_and_spec(&r, "", "## 設計書\n認証仕様", "diff_content", "ja");
        assert!(composed.contains("## 設計書\n認証仕様"));
        assert!(!composed.contains("<!-- spec -->"));
        assert!(composed.contains("diff_content"));
    }

    #[test]
    fn composes_without_marker_puts_spec_before_diff() {
        // マーカーが無いレシピに設計書を渡した場合
        let r = parse("general", "builtin", "# 一般レビュー\n本文");
        let composed = compose_with_lang_and_spec(&r, "", "## 付加設計書\n仕様", "+code", "ja");
        assert!(composed.contains("## 付加設計書\n仕様"));
        // 設計書が差分見出し（## 差分）より前に配置されていること
        let spec_pos = composed.find("## 付加設計書\n仕様").unwrap();
        let diff_pos = composed.find("## 差分").unwrap();
        assert!(spec_pos < diff_pos);
    }

    #[test]
    fn composes_with_marker_and_empty_spec_removes_marker() {
        // マーカーがあり設計書が空の場合、マーカーが除去されること
        let r = parse("spec", "builtin", "# 基準\n本文\n<!-- spec -->\nフッター");
        let composed = compose_with_lang_and_spec(&r, "", "", "diff_content", "ja");
        assert!(!composed.contains("<!-- spec -->"));
        assert!(composed.contains("フッター"));
    }

    #[test]
    fn compose_checked_rejects_empty_spec_when_needed() {
        // 設計書が必要なレシピで設計書が空なら Err
        let r = parse("spec", "builtin", "# 基準\n本文\n<!-- spec -->\n");
        let err = compose_with_lang_and_spec_checked(&r, "", "", "diff", "ja").unwrap_err();
        assert!(err.contains("設計書"));

        // 空白のみでも Err
        let err_space = compose_with_lang_and_spec_checked(&r, "", "   \n\t  ", "diff", "ja").unwrap_err();
        assert!(err_space.contains("設計書"));

        // 設計書があれば Ok
        let ok = compose_with_lang_and_spec_checked(&r, "", "## 仕様", "diff", "ja").unwrap();
        assert!(ok.contains("## 仕様"));
    }

    #[test]
    fn builtin_recipes_include_spec() {
        let recipes = list_with_lang(None, "ja");
        let spec = recipes.iter().find(|r| r.id == "spec");
        assert!(spec.is_some());
        assert!(spec.unwrap().needs_spec);
    }
}
