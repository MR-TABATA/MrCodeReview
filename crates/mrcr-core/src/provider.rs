use crate::byok;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub text: String,
    pub model: String,
    pub usage: Usage,
    pub cost_usd: Option<f64>,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub label: String,
    pub note: String,
}

pub trait Provider: Send + Sync {
    fn info(&self) -> ProviderInfo;
    fn run(&self, prompt: &str) -> Result<LlmResponse, String>;
}

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
pub const ANTHROPIC_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";

pub struct AnthropicApi {
    pub model: String,
    pub max_tokens: u64,
    pub timeout: Duration,
    pub endpoint: String,
    pub api_key: Option<String>,
}

impl AnthropicApi {
    pub fn new(cfg: byok::Config, timeout: Duration, api_key: Option<String>) -> Self {
        AnthropicApi {
            model: cfg.model,
            max_tokens: cfg.max_tokens,
            timeout,
            endpoint: ANTHROPIC_ENDPOINT.into(),
            api_key,
        }
    }

    fn key(&self) -> Result<String, String> {
        self.api_key
            .clone()
            .ok_or_else(|| "Anthropic API キーが設定されていません。".to_string())
    }
}

impl Provider for AnthropicApi {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "anthropic-api".into(),
            label: "Anthropic API".into(),
            note: format!("モデル: {}", self.model),
        }
    }

    fn run(&self, prompt: &str) -> Result<LlmResponse, String> {
        let key = self.key()?;
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "messages": [{ "role": "user", "content": prompt }]
        });

        let resp = ureq::post(&self.endpoint)
            .timeout(self.timeout)
            .set("x-api-key", &key)
            .set("anthropic-version", "2023-06-01")
            .set("content-type", "application/json")
            .send_json(&body)
            .map_err(|e| format!("API リクエスト失敗: {e}"))?;

        let text = resp
            .into_string()
            .map_err(|e| format!("応答の読み込み失敗: {e}"))?;
        parse_anthropic(&text, self.max_tokens)
    }
}

fn parse_anthropic(body: &str, _max_tokens: u64) -> Result<LlmResponse, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("API 応答 JSON パース失敗: {e}"))?;
    if v["type"] == "error" {
        let msg = v["error"]["message"].as_str().unwrap_or("API エラー");
        return Err(msg.to_string());
    }

    let text = v["content"]
        .as_array()
        .and_then(|arr| arr.iter().find(|c| c["type"] == "text"))
        .and_then(|c| c["text"].as_str())
        .ok_or_else(|| "応答にテキストが見つかりません".to_string())?
        .to_string();

    let model = v["model"].as_str().unwrap_or("").to_string();
    let usage = Usage {
        input_tokens: v["usage"]["input_tokens"].as_u64(),
        output_tokens: v["usage"]["output_tokens"].as_u64(),
        cache_read_tokens: v["usage"]["cache_read_input_tokens"].as_u64(),
        cache_write_tokens: v["usage"]["cache_creation_input_tokens"].as_u64(),
    };

    Ok(LlmResponse {
        text,
        model,
        usage,
        cost_usd: None,
        raw: body.to_string(),
    })
}

pub struct Mock;

impl Provider for Mock {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "mock".into(),
            label: "Mock Provider".into(),
            note: "テスト用モック".into(),
        }
    }

    fn run(&self, _prompt: &str) -> Result<LlmResponse, String> {
        let text = r#"{
            "findings": [
                {
                    "file": "DEVELOPING.md",
                    "line": 1,
                    "severity": "medium",
                    "title": "（モック）追加された行の確認",
                    "detail": "モックの応答です。引用は実際の差分にあります。",
                    "evidence": "- 名前が `MrkCodeReview-dev`（窓・メニュー。Pro 殻の `productName` の末尾に `-dev`）。キーチェーンの項目も別（`com.hitoshi.mrcodereview.dev`）で、配布版の API キーと混ざらない。"
                },
                {
                    "file": "DEVELOPING.md",
                    "line": 2,
                    "severity": "high",
                    "title": "（モック）根拠不一致の見本",
                    "detail": "モックの応答です。この引用は差分に存在しないので「根拠不一致」と出るはずです。",
                    "evidence": "eval(request.body.code)"
                }
            ]
        }"#;

        Ok(LlmResponse {
            text: text.into(),
            model: "mock-model".into(),
            usage: Usage {
                input_tokens: Some(100),
                output_tokens: Some(50),
                cache_read_tokens: None,
                cache_write_tokens: None,
            },
            cost_usd: None,
            raw: text.into(),
        })
    }
}
