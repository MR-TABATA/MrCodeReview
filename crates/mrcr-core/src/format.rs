use crate::findings::Finding;
use crate::i18n::{fill, Lang};
use crate::provider::Usage;
use crate::snapshot::Snapshot;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeReviewResult {
    pub recipe_id: String,
    pub recipe_title: String,
    pub findings: Vec<Finding>,
    pub usage: Usage,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliReviewReport {
    pub repo: String,
    pub base: String,
    pub head: String,
    pub diff_hash: String,
    pub timestamp: String,
    pub changed_files_count: usize,
    pub results: Vec<RecipeReviewResult>,
}

impl CliReviewReport {
    pub fn new(snapshot: &Snapshot, results: Vec<RecipeReviewResult>) -> Self {
        let now = chrono_now_string();
        CliReviewReport {
            repo: snapshot.repo.clone(),
            base: snapshot.base.spec.clone(),
            head: snapshot.head.clone(),
            diff_hash: snapshot.diff_hash.clone(),
            timestamp: now,
            changed_files_count: snapshot.files.len(),
            results,
        }
    }

    pub fn total_findings(&self) -> usize {
        self.results.iter().map(|r| r.findings.len()).sum()
    }

    pub fn verified_findings(&self) -> usize {
        self.results
            .iter()
            .flat_map(|r| &r.findings)
            .filter(|f| f.verified)
            .count()
    }

    pub fn unverified_findings(&self) -> usize {
        self.results
            .iter()
            .flat_map(|r| &r.findings)
            .filter(|f| !f.verified)
            .count()
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| format!("JSON の生成に失敗しました: {e}"))
    }

    pub fn to_markdown(&self) -> String {
        self.to_markdown_lang(Lang::Ja)
    }

    pub fn to_markdown_lang(&self, lang: Lang) -> String {
        let c = &lang.texts().cli;
        let mut out = String::new();
        out.push_str(&format!("# {}\n\n", c.report_title));
        out.push_str(&format!("- **{}**: `{}`\n", c.report_repo, self.repo));
        out.push_str(&format!("- **{}**: `{}`\n", c.report_base, self.base));
        out.push_str(&format!(
            "- **{}**: {}\n",
            c.report_changed_files,
            fill(c.report_files, &[("n", &self.changed_files_count.to_string())])
        ));
        out.push_str(&format!("- **{}**: {}\n\n", c.report_timestamp, self.timestamp));

        let total = self.total_findings();
        let verified = self.verified_findings();
        let unverified = self.unverified_findings();

        out.push_str(&format!("## {}\n", c.report_summary));
        if total == 0 {
            out.push_str(&format!("{}\n\n", c.report_no_findings));
        } else {
            out.push_str(&format!(
                "- {}\n\n",
                fill(
                    c.report_total,
                    &[
                        ("total", &total.to_string()),
                        ("verified", &verified.to_string()),
                        ("unverified", &unverified.to_string()),
                    ],
                )
            ));
        }

        out.push_str("---\n\n");

        for (idx, res) in self.results.iter().enumerate() {
            out.push_str(&format!(
                "## {}. {} (`{}`)\n\n",
                idx + 1,
                res.recipe_title,
                res.recipe_id
            ));

            if res.findings.is_empty() {
                out.push_str(&format!("{}\n\n", c.report_recipe_ok));
                continue;
            }

            for f in &res.findings {
                let verified_mark = if f.verified { c.report_verified } else { c.report_unverified };

                let line_info = f
                    .line
                    .map(|l| format!(":{}", l))
                    .unwrap_or_default();

                out.push_str(&format!(
                    "### [{}] {}\n",
                    f.severity.to_uppercase(),
                    f.title
                ));
                out.push_str(&format!(
                    "- **{}**: `{}{}` ({})\n",
                    c.report_location, f.file, line_info, verified_mark
                ));

                if !f.evidence.trim().is_empty() {
                    out.push_str(&format!(
                        "- **{}**:\n  ```\n  {}\n  ```\n",
                        c.report_quote,
                        f.evidence.trim()
                    ));
                }

                if !f.detail.trim().is_empty() {
                    out.push_str(&format!("- **{}**:\n  {}\n", c.report_detail, f.detail.trim()));
                }

                out.push('\n');
            }

            out.push_str("---\n\n");
        }

        out
    }
}

fn chrono_now_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{now}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(verified: bool) -> CliReviewReport {
        CliReviewReport {
            repo: "/r".into(),
            base: "HEAD".into(),
            head: "h".into(),
            diff_hash: "d".into(),
            timestamp: "1".into(),
            changed_files_count: 2,
            results: vec![RecipeReviewResult {
                recipe_id: "bugs".into(),
                recipe_title: "Latent Bugs".into(),
                findings: vec![Finding {
                    file: "a.rs".into(),
                    line: Some(3),
                    severity: "high".into(),
                    title: "t".into(),
                    detail: "d".into(),
                    evidence: "x".into(),
                    verified,
                    rule: None,
                }],
                usage: Usage::default(),
                model: "m".into(),
            }],
        }
    }

    fn has_japanese(s: &str) -> bool {
        s.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c) || ('\u{4e00}'..='\u{9fff}').contains(&c))
    }

    #[test]
    fn english_report_has_no_japanese_for_verified_and_unverified() {
        for v in [true, false] {
            let md = sample(v).to_markdown_lang(Lang::En);
            assert!(!has_japanese(&md), "日本語が残っている: {md}");
        }
        let empty = CliReviewReport { results: vec![], ..sample(true) };
        assert!(!has_japanese(&empty.to_markdown_lang(Lang::En)));
    }

    #[test]
    fn japanese_report_is_unchanged_in_wording() {
        let md = sample(true).to_markdown();
        assert!(md.contains("## 総合サマリー"));
        assert!(md.contains("- **合計指摘数**: 1 件（根拠一致: 1 件、根拠不一致: 0 件）"));
        assert!(md.contains("- **変更ファイル数**: 2 ファイル"));
        assert!(md.contains("- **場所**: `a.rs:3` (根拠あり)"));
    }
}
