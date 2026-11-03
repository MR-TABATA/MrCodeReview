use crate::findings::Finding;
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
        let mut out = String::new();
        out.push_str("# MrCodeReview レポート\n\n");
        out.push_str(&format!("- **リポジトリ**: `{}`\n", self.repo));
        out.push_str(&format!("- **比較範囲**: `{}`\n", self.base));
        out.push_str(&format!("- **変更ファイル数**: {} ファイル\n", self.changed_files_count));
        out.push_str(&format!("- **実施日時**: {}\n\n", self.timestamp));

        let total = self.total_findings();
        let verified = self.verified_findings();
        let unverified = self.unverified_findings();

        out.push_str("## 総合サマリー\n");
        if total == 0 {
            out.push_str("✓ **全観点で指摘はありませんでした。**\n\n");
        } else {
            out.push_str(&format!(
                "- **合計指摘数**: {} 件（根拠一致: {} 件、根拠不一致: {} 件）\n\n",
                total, verified, unverified
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
                out.push_str("指摘はありませんでした (OK)\n\n");
                continue;
            }

            for f in &res.findings {
                let verified_mark = if f.verified {
                    "根拠あり"
                } else {
                    "⚠️ 根拠不一致（ハルシネーションの可能性あり）"
                };

                let line_info = f
                    .line
                    .map(|l| format!(":{}", l))
                    .unwrap_or_default();

                out.push_str(&format!(
                    "### [{}] {}\n",
                    f.severity.to_uppercase(),
                    f.title
                ));
                out.push_str(&format!("- **場所**: `{}{}` ({})\n", f.file, line_info, verified_mark));

                if !f.evidence.trim().is_empty() {
                    out.push_str(&format!(
                        "- **引用コード**:\n  ```\n  {}\n  ```\n",
                        f.evidence.trim()
                    ));
                }

                if !f.detail.trim().is_empty() {
                    out.push_str(&format!("- **詳細**:\n  {}\n", f.detail.trim()));
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
