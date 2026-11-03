//! トークン使用量の集計。記録に残した「測った値」だけを足す。
//!
//! - 提供元が報告しなかった値は、0 として足さない（推定で埋めない）。報告の無かったレビューの件数を、別に出す。
//! - 失敗した実行は、トークンを足さず、件数だけ出す。
//! - linter は LLM を呼ばないので、測っての 0 として足る（報告なしではない）。

use crate::provider::Usage;
#[cfg(test)]
use crate::store::ReviewRecord;
use serde::Serialize;
use std::collections::BTreeMap;

/// 集計に要る値だけ（記録の JSON 全体は、読まない）。データベースの列から作る（`store::usage_rows`）。
#[derive(Debug, Clone)]
pub struct Slim {
    pub created_at: u64,
    pub repo: String,
    pub recipe_title: String,
    pub provider_id: String,
    pub provider_label: String,
    pub model: String,
    /// 実行に失敗した（`error` がある）。指摘が読めなかっただけの実行は、含めない（トークンは使っている）
    pub failed: bool,
    pub usage: Usage,
    pub cost_usd: Option<f64>,
}

#[cfg(test)]
impl From<&ReviewRecord> for Slim {
    fn from(r: &ReviewRecord) -> Slim {
        Slim {
            created_at: r.created_at,
            repo: r.repo.clone(),
            recipe_title: r.recipe.title.clone(),
            provider_id: r.provider.id.clone(),
            provider_label: r.provider.label.clone(),
            model: r.model.clone(),
            failed: r.error.is_some(),
            usage: r.usage.clone(),
            cost_usd: r.cost_usd,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Row {
    pub key: String,
    pub reviews: u64,
    /// 失敗した実行（トークンは足していない）
    pub failed: u64,
    /// 成功したが、入力か出力のトークンが報告されなかった実行
    pub unreported: u64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Row {
    fn new(key: &str) -> Row {
        Row { key: key.to_string(), ..Row::default() }
    }

    /// 入力＋出力（並べ替えと、棒の長さに使う）
    pub fn total(&self) -> u64 {
        self.input + self.output
    }

    fn add(&mut self, r: &Slim) {
        self.reviews += 1;
        if r.failed {
            self.failed += 1;
            return;
        }
        let u = &r.usage;
        if u.input_tokens.is_none() || u.output_tokens.is_none() {
            self.unreported += 1;
        }
        self.input += u.input_tokens.unwrap_or(0);
        self.output += u.output_tokens.unwrap_or(0);
        self.cache_read += u.cache_read_tokens.unwrap_or(0);
        self.cache_write += u.cache_write_tokens.unwrap_or(0);
    }
}

#[derive(Debug, Serialize)]
pub struct Summary {
    /// この時刻（UNIX ミリ秒）以降だけを数えた。None なら、すべて。
    pub since: Option<u64>,
    pub total: Row,
    pub by_recipe: Vec<Row>,
    pub by_repo: Vec<Row>,
    pub by_model: Vec<Row>,
    /// 提供元が報告した金額の合計（参考。報告したレビューだけ）
    pub cost_usd: Option<f64>,
    /// 金額を報告したレビューの件数
    pub cost_reviews: u64,
    /// 壊れていて読めず、この集計に含めていない記録の件数
    pub unreadable: usize,
}

/// 過去の実績からの、所要時間の目安。件数が少ないうちは、幅だけを見せる（画面で、回数つきで出す）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Durations {
    /// 実績の件数（0 なら、ほかは 0）
    pub n: usize,
    pub min_ms: u64,
    pub median_ms: u64,
    pub max_ms: u64,
}

/// `samples` は、新しい順の所要時間。モックは、本物の時間ではないので、呼ぶ側で除く。
pub fn durations(samples: &[u64]) -> Durations {
    if samples.is_empty() {
        return Durations { n: 0, min_ms: 0, median_ms: 0, max_ms: 0 };
    }
    let mut v = samples.to_vec();
    v.sort_unstable();
    let n = v.len();
    let median = if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2 };
    Durations { n, min_ms: v[0], median_ms: median, max_ms: v[n - 1] }
}

fn model_key(r: &Slim) -> String {
    if r.provider_id == "tool" {
        "リンター（LLM を使わない）".to_string()
    } else if r.model.is_empty() {
        format!("{} / モデル不明", r.provider_label)
    } else {
        format!("{} / {}", r.provider_label, r.model)
    }
}

fn rows(map: BTreeMap<String, Row>) -> Vec<Row> {
    let mut v: Vec<Row> = map.into_values().collect();
    v.sort_by(|a, b| b.total().cmp(&a.total()).then(b.reviews.cmp(&a.reviews)).then(a.key.cmp(&b.key)));
    v
}

#[cfg(test)]
pub fn summarize(records: &[ReviewRecord], since: Option<u64>) -> Summary {
    let slim: Vec<Slim> = records.iter().map(Slim::from).collect();
    summarize_slim(&slim, since)
}

/// 画面の集計は、こちら（`store::usage_rows` が、期間で絞って、必要な列だけを読む）。
pub fn summarize_slim(records: &[Slim], since: Option<u64>) -> Summary {
    let mut total = Row::new("合計");
    let (mut by_recipe, mut by_repo, mut by_model) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let (mut cost, mut cost_reviews) = (0.0_f64, 0_u64);
    for r in records.iter().filter(|r| since.is_none_or(|s| r.created_at >= s)) {
        total.add(r);
        by_recipe.entry(r.recipe_title.clone()).or_insert_with(|| Row::new(&r.recipe_title)).add(r);
        by_repo.entry(r.repo.clone()).or_insert_with(|| Row::new(&r.repo)).add(r);
        let mk = model_key(r);
        by_model.entry(mk.clone()).or_insert_with(|| Row::new(&mk)).add(r);
        if !r.failed {
            if let Some(c) = r.cost_usd {
                cost += c;
                cost_reviews += 1;
            }
        }
    }
    Summary {
        since,
        total,
        by_recipe: rows(by_recipe),
        by_repo: rows(by_repo),
        by_model: rows(by_model),
        cost_usd: (cost_reviews > 0).then_some(cost),
        cost_reviews,
        unreadable: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ProviderInfo, Usage};
    use crate::recipe::{Kind, Recipe};
    use crate::store::SnapshotRecord;

    fn rec(at: u64, repo: &str, recipe: &str, provider: &str, model: &str, usage: Usage) -> ReviewRecord {
        ReviewRecord {
            id: format!("{at}"),
            created_at: at,
            repo: repo.into(),
            recipe: Recipe { id: recipe.into(), title: recipe.into(), source: "builtin".into(), prompt: "p".into(), kind: Kind::Llm, needs_related: false },
            provider: ProviderInfo { id: provider.into(), label: format!("L-{provider}"), note: "n".into() },
            snapshot: SnapshotRecord { head: "h".into(), base: Default::default(), diff_hash: "d".into(), diff_bytes: 1, files: vec![], excluded: vec![] },
            prompt: "p".into(),
            model: model.into(),
            response_text: String::new(),
            raw: String::new(),
            usage,
            cost_usd: None,
            duration_ms: 1,
            findings: vec![],
            carries: vec![],
            fates: vec![],
            notes: vec![],
            related: None,
            repo_overrides: vec![],
            settings: None,
            parse_error: None,
            error: None,
        }
    }

    fn u(i: Option<u64>, o: Option<u64>) -> Usage {
        Usage { input_tokens: i, output_tokens: o, cache_read_tokens: None, cache_write_tokens: None }
    }

    #[test]
    fn sums_only_measured_values_and_counts_the_unreported_separately() {
        let mut a = rec(100, "/r/a", "bugs", "claude-cli", "m1", u(Some(1000), Some(200)));
        a.usage.cache_read_tokens = Some(50);
        a.cost_usd = Some(0.25);
        let b = rec(200, "/r/a", "bugs", "claude-cli", "m1", u(Some(500), None)); // 出力が報告されなかった
        let c = rec(300, "/r/b", "リンター", "tool", "Ruff", u(Some(0), Some(0))); // 測っての 0
        let mut d = rec(400, "/r/b", "bugs", "claude-cli", "m1", u(Some(9999), Some(9999)));
        d.error = Some("失敗".into()); // 失敗は、トークンを足さない
        let s = summarize(&[a, b, c, d], None);
        assert_eq!((s.total.reviews, s.total.failed, s.total.unreported), (4, 1, 1));
        assert_eq!((s.total.input, s.total.output, s.total.cache_read), (1500, 200, 50), "報告の無い出力を、0 として足さず、失敗の値も足さない");
        assert_eq!((s.cost_usd, s.cost_reviews), (Some(0.25), 1), "金額は、報告したレビューだけ");
        let bugs = s.by_recipe.iter().find(|r| r.key == "bugs").unwrap();
        assert_eq!((bugs.reviews, bugs.failed, bugs.unreported, bugs.input), (3, 1, 1, 1500));
        assert_eq!(s.by_recipe[0].key, "bugs", "使った量の多い順");
        let tool = s.by_model.iter().find(|r| r.key == "リンター（LLM を使わない）").unwrap();
        assert_eq!((tool.reviews, tool.unreported, tool.total()), (1, 0, 0), "リンターは、測っての 0 で、報告なしではない");
        assert!(s.by_model.iter().any(|r| r.key == "L-claude-cli / m1"));
        assert_eq!(s.by_repo.len(), 2);
    }

    #[test]
    fn durations_are_min_median_max_and_empty_is_all_zero() {
        assert_eq!(durations(&[]), Durations { n: 0, min_ms: 0, median_ms: 0, max_ms: 0 });
        assert_eq!(durations(&[7000]), Durations { n: 1, min_ms: 7000, median_ms: 7000, max_ms: 7000 });
        let d = durations(&[45_000, 25_000, 30_000]);
        assert_eq!((d.n, d.min_ms, d.median_ms, d.max_ms), (3, 25_000, 30_000, 45_000), "並び順に関わらず、中央値");
        assert_eq!(durations(&[10, 20, 30, 41]).median_ms, 25, "偶数件は、真ん中 2 つの平均");
    }

    #[test]
    fn since_filters_by_time_and_empty_is_all_zero_without_a_cost() {
        let recs = [rec(100, "/r", "a", "p", "m", u(Some(10), Some(1))), rec(200, "/r", "a", "p", "m", u(Some(20), Some(2)))];
        let s = summarize(&recs, Some(150));
        assert_eq!((s.total.reviews, s.total.input, s.since), (1, 20, Some(150)));
        let e = summarize(&[], None);
        assert_eq!((e.total.reviews, e.total.total(), e.cost_usd, e.by_recipe.len()), (0, 0, None, 0));
        assert_eq!(summarize(&recs, Some(999)).total.reviews, 0);
    }
}
