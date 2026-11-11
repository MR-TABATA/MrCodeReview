//! LLM を呼ぶ部品。
//!
//! - `anthropic-api`: 自分の API キー（BYOK）で、Anthropic の API を直接呼ぶ。キーは macOS キーチェーンから、呼ぶ直前に読む。
//! - `mock`: 外へ何も送らない偽の応答（画面の試験用）。

use crate::{byok, keychain};
pub use mrcr_core::provider::{DEFAULT_TIMEOUT, LlmResponse, Provider, ProviderInfo, Usage};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

pub fn all(data_dir: PathBuf, byok: byok::Config) -> Vec<Box<dyn Provider>> {
    all_with(data_dir, DEFAULT_TIMEOUT, byok)
}

/// 画面に出す LLM の一覧。`[anthropic-api, mock]`（先頭が BYOK）。
pub fn all_with(_data_dir: PathBuf, timeout: Duration, byok: byok::Config) -> Vec<Box<dyn Provider>> {
    vec![Box::new(AnthropicApi::new(byok, timeout)), Box::new(Mock)]
}

pub fn get_with(id: &str, data_dir: PathBuf, timeout: Duration, byok: byok::Config) -> Option<Box<dyn Provider>> {
    all_with(data_dir, timeout, byok).into_iter().find(|p| p.info().id == id)
}

// ── Anthropic API（BYOK） ────────────────────────────────────────────────────

pub const ANTHROPIC_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// 自分の API キーで、Anthropic の Messages API を直接呼ぶ。キーは、macOS キーチェーンから、呼ぶ直前に読む
/// （この構造体にも、ログにも、記録にも、残さない）。
pub struct AnthropicApi {
    pub model: String,
    pub max_tokens: u64,
    pub timeout: Duration,
    /// 試験では、手元の偽サーバーに向ける。
    pub endpoint: String,
    /// 試験用: キーチェーンを使わず、このキーを使う。
    pub key_override: Option<String>,
}

impl AnthropicApi {
    pub fn new(cfg: byok::Config, timeout: Duration) -> Self {
        AnthropicApi { model: cfg.model, max_tokens: cfg.max_tokens, timeout, endpoint: ANTHROPIC_ENDPOINT.into(), key_override: None }
    }

    fn key(&self) -> Result<String, String> {
        self.key_with(|| keychain::get(keychain::SERVICE, keychain::ACCOUNT_ANTHROPIC))
    }

    /// キーの取り出し方を差し替えられる形（試験では、キーチェーンに触れずに、「項目が無い」「読めない」を作る）。
    fn key_with(&self, lookup: impl FnOnce() -> Result<Option<String>, String>) -> Result<String, String> {
        if let Some(k) = &self.key_override {
            return Ok(k.clone());
        }
        lookup()?.ok_or_else(|| "Anthropic API キーが未設定です。設定画面の「LLM プロバイダ」で、キーを保存してください".to_string())
    }
}

/// エラーの文から、キーを消す（API が文に含めて返すことは無いはずだが、念のため）。
fn redact(s: &str, key: &str) -> String {
    if key.is_empty() {
        s.to_string()
    } else {
        s.replace(key, "***")
    }
}

fn truncate_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

/// API のエラー応答の本文（JSON のはず）から、理由の文を取り出す。読めなければ、本文の先頭。
fn error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(String::from))
        .unwrap_or_else(|| truncate_chars(body.trim(), 200))
}

/// API のエラー応答から、画面に出す文を作る。
fn api_error(code: u16, body: &str, key: &str) -> String {
    let msg = error_message(body);
    let hint = match code {
        401 => "（キーが正しくないか、無効です）",
        403 => "（このキーには、使う権限がありません）",
        404 => "（モデル名が正しいか、設定で確かめてください）",
        429 => "（利用上限か、混み合っています。少し待つか、プランを確かめてください）",
        529 => "（API が混み合っています。少し待って、もう一度）",
        _ => "",
    };
    redact(&format!("Anthropic API のエラー（HTTP {code}）{hint}: {}", truncate_chars(&msg, 300)), key)
}

/// 成功した応答（本文は JSON）を読む。トークンは、API が返した実測値だけ（足りなければ None）。
fn parse_anthropic(body: &str, max_tokens: u64, requested_model: &str) -> Result<LlmResponse, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("API の応答を読めませんでした: {e}"))?;
    if v["type"] == "error" {
        return Err(format!("Anthropic API のエラー: {}", truncate_chars(&error_message(body), 300)));
    }
    let text: String = v["content"]
        .as_array()
        .map(|blocks| blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join(""))
        .unwrap_or_default();
    if v["stop_reason"] == "max_tokens" {
        return Err(format!("応答が最大出力トークン（{max_tokens}）で途切れました。設定の「最大出力トークン」を上げて、もう一度実行してください"));
    }
    // 正常に終わった印（end_turn・stop_sequence）か、印が無いとき以外は、途中で終わった可能性がある（拒否・中断など）。
    // 欠けた結果を「指摘なし」と取り違えないよう、使わない。
    if let Some(reason) = v["stop_reason"].as_str().filter(|r| !matches!(*r, "end_turn" | "stop_sequence")) {
        return Err(format!("応答が途中で終わりました（stop_reason: {reason}）。結果が欠けている恐れがあるので、使いません。もう一度実行してください"));
    }
    if text.trim().is_empty() {
        return Err("API の応答に、本文がありませんでした".into());
    }
    let n = |k: &str| v["usage"][k].as_u64();
    Ok(LlmResponse {
        text,
        // 応答にモデル名が無いときは、要求したモデル名に「応答になし」を添えて残す（実際に使ったモデルを、あとから追えるように）
        model: v["model"].as_str().map(String::from).unwrap_or_else(|| format!("{requested_model}（応答にモデル名なし）")),
        usage: Usage {
            input_tokens: n("input_tokens"),
            output_tokens: n("output_tokens"),
            cache_read_tokens: n("cache_read_input_tokens"),
            cache_write_tokens: n("cache_creation_input_tokens"),
        },
        cost_usd: None,
        raw: body.to_string(),
    })
}

impl Provider for AnthropicApi {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "anthropic-api".into(),
            label: "Anthropic API（自分のキー）".into(),
            note: format!(
                "自分の API キーで、Anthropic の API を直接呼ぶ（BYOK）。モデル: {}。送るのは、「送った内容」タブのプロンプト全文（観点の文章・差分・関連コード）で、api.anthropic.com に送られる。キーは macOS キーチェーンにだけ保存し、履歴・ログ・記録には残さない。トークンは、API が返した実測値を記録する。",
                self.model
            ),
        }
    }

    fn run(&self, prompt: &str) -> Result<LlmResponse, String> {
        let key = self.key()?;
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "messages": [{ "role": "user", "content": prompt }],
        });
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();
        let res = agent
            .post(&self.endpoint)
            .set("x-api-key", &key)
            .set("anthropic-version", ANTHROPIC_VERSION)
            .set("content-type", "application/json")
            .send_json(body);
        match res {
            Ok(r) => {
                let text = r.into_string().map_err(|e| format!("API の応答を読めませんでした: {}", redact(&e.to_string(), &key)))?;
                parse_anthropic(&text, self.max_tokens, &self.model)
            }
            Err(ureq::Error::Status(code, r)) => {
                // 本文を読めなくても、HTTP コードの案内は出す。読めなかったことも、添える
                let (body, unreadable) = match r.into_string() {
                    Ok(b) => (b, None),
                    Err(e) => (String::new(), Some(redact(&e.to_string(), &key))),
                };
                let mut msg = api_error(code, &body, &key);
                if let Some(why) = unreadable {
                    msg.push_str(&format!("（エラーの本文を読めませんでした: {why}）"));
                }
                Err(msg)
            }
            Err(ureq::Error::Transport(t)) => Err(redact(&format!("API に接続できませんでした（{} 秒の待ち時間・ネットワークを確かめてください）: {t}", self.timeout.as_secs()), &key)),
        }
    }
}

// ── mock ────────────────────────────────────────────────────────────────────

pub struct Mock;

impl Provider for Mock {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "mock".into(),
            label: "モック（偽の応答・外へ送らない）".into(),
            note: "画面の試験用。差分の最初の追加行を引用した指摘 1 件と、差分に無い行を引用した指摘 1 件（根拠不一致の見本）を返す。トークンは文字数からの偽の値。".into(),
        }
    }

    fn run(&self, prompt: &str) -> Result<LlmResponse, String> {
        // プロンプトの ```diff ... ``` の中から、最初の追加行とそのファイルを拾う。
        let diff = prompt.split("```diff\n").nth(1).unwrap_or("");
        let mut cur_file = String::new();
        let mut file = String::new();
        let mut quote: Option<String> = None;
        for line in diff.lines() {
            if let Some(rest) = line.strip_prefix("+++ b/") {
                cur_file = rest.to_string();
            } else if line.starts_with('+') && !line.starts_with("+++") && quote.is_none() {
                let q = line[1..].trim().to_string();
                if !q.is_empty() {
                    quote = Some(q);
                    file = cur_file.clone();
                }
            }
        }
        let mut findings = Vec::new();
        if let Some(q) = quote.filter(|q| !q.is_empty()) {
            findings.push(serde_json::json!({
                "file": file, "line": 1, "severity": "medium",
                "title": "（モック）追加された行の確認",
                "detail": "モックの応答です。引用は実際の差分にあります。",
                "evidence": q,
            }));
            findings.push(serde_json::json!({
                "file": file, "line": 2, "severity": "high",
                "title": "（モック）根拠不一致の見本",
                "detail": "モックの応答です。この引用は差分に存在しないので「根拠不一致」と出るはずです。",
                "evidence": "eval(request.body.code)",
            }));
        }
        let text = serde_json::json!({ "findings": findings }).to_string();
        let input = (prompt.chars().count() / 4) as u64;
        let output = (text.chars().count() / 4) as u64;
        Ok(LlmResponse {
            raw: text.clone(),
            text,
            model: "mock".into(),
            usage: Usage {
                input_tokens: Some(input),
                output_tokens: Some(output),
                cache_read_tokens: Some(0),
                cache_write_tokens: Some(0),
            },
            cost_usd: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 手元の偽サーバー: 1 回だけ受けて、決めた応答を返し、受け取った要求（ヘッダ＋本文）を返す。
    fn fake_server(status: &str, body: &str) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let (status, body) = (status.to_string(), body.to_string());
        let h = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let (mut buf, mut chunk) = (Vec::new(), [0u8; 4096]);
            loop {
                let n = s.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                    let cl = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                    if buf.len() >= pos + 4 + cl {
                        break;
                    }
                }
            }
            let resp = format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
            s.write_all(resp.as_bytes()).ok();
            String::from_utf8_lossy(&buf).to_string()
        });
        (format!("http://{addr}/v1/messages"), h)
    }

    const TEST_KEY: &str = "sk-ant-api03-TESTKEYTESTKEYTESTKEY";

    fn api_to(endpoint: String) -> AnthropicApi {
        AnthropicApi { model: "claude-sonnet-5-5".into(), max_tokens: 4096, timeout: Duration::from_secs(5), endpoint, key_override: Some(TEST_KEY.into()) }
    }

    #[test]
    fn anthropic_sends_key_version_model_and_prompt_and_records_measured_usage() {
        let ok = r#"{"id":"msg_1","type":"message","model":"claude-sonnet-5-5-20260101","content":[{"type":"text","text":"{\"findings\":[]}"}],"stop_reason":"end_turn","usage":{"input_tokens":120,"output_tokens":7,"cache_read_input_tokens":5}}"#;
        let (endpoint, server) = fake_server("200 OK", ok);
        let r = api_to(endpoint).run("これが差分です").unwrap();
        let req = server.join().unwrap();
        let lower = req.to_lowercase();
        assert!(req.starts_with("POST /v1/messages"), "{req}");
        assert!(lower.contains(&format!("x-api-key: {}", TEST_KEY.to_lowercase())), "キーをヘッダで送る");
        assert!(lower.contains("anthropic-version: 2023-06-01"));
        assert!(req.contains(r#""model":"claude-sonnet-5-5""#) && req.contains(r#""max_tokens":4096"#) && req.contains("これが差分です"));
        assert!(!req.split("\r\n\r\n").nth(1).unwrap_or("").contains(TEST_KEY), "キーは、本文には入れない");
        assert_eq!(r.text, r#"{"findings":[]}"#);
        assert_eq!(r.model, "claude-sonnet-5-5-20260101");
        assert_eq!((r.usage.input_tokens, r.usage.output_tokens, r.usage.cache_read_tokens, r.usage.cache_write_tokens), (Some(120), Some(7), Some(5), None));
        assert_eq!(r.cost_usd, None);
        assert!(!r.raw.contains(TEST_KEY));
    }

    #[test]
    fn anthropic_http_errors_say_why_and_never_echo_the_key() {
        // 万一、API が文にキーを含めて返しても、画面に出さない
        let body = format!(r#"{{"type":"error","error":{{"type":"authentication_error","message":"invalid x-api-key {TEST_KEY}"}}}}"#);
        let (endpoint, server) = fake_server("401 Unauthorized", &body);
        let e = api_to(endpoint).run("x").unwrap_err();
        server.join().unwrap();
        assert!(e.contains("401") && e.contains("キー"), "{e}");
        assert!(!e.contains("TESTKEY"), "キーを文に出さない: {e}");
        let (endpoint, server) = fake_server("404 Not Found", r#"{"type":"error","error":{"type":"not_found_error","message":"model: nope"}}"#);
        let e = api_to(endpoint).run("x").unwrap_err();
        server.join().unwrap();
        assert!(e.contains("404") && e.contains("モデル名") && e.contains("model: nope"), "{e}");
    }

    #[test]
    fn anthropic_truncated_or_empty_answers_are_errors_not_silent_zero_findings() {
        let cut = r#"{"model":"m","content":[{"type":"text","text":"{\"findings\":[{\"file\":"}],"stop_reason":"max_tokens","usage":{"input_tokens":1,"output_tokens":4096}}"#;
        let e = parse_anthropic(cut, 4096, "m").unwrap_err();
        assert!(e.contains("途切れ") && e.contains("4096") && e.contains("最大出力トークン"), "{e}");
        assert!(parse_anthropic(r#"{"model":"m","content":[],"stop_reason":"end_turn"}"#, 1, "m").unwrap_err().contains("本文"));
        assert!(parse_anthropic("not json", 1, "m").unwrap_err().contains("読めません"));
        assert!(parse_anthropic(r#"{"type":"error","error":{"message":"overloaded"}}"#, 1, "m").unwrap_err().contains("overloaded"));
    }

    #[test]
    fn rate_limit_and_overload_say_to_wait_and_keep_the_http_code() {
        for (status, code, hint) in [("429 Too Many Requests", "429", "利用上限"), ("529 Overloaded", "529", "混み合っています")] {
            let (endpoint, server) = fake_server(status, r#"{"type":"error","error":{"type":"x","message":"slow down"}}"#);
            let e = api_to(endpoint).run("x").unwrap_err();
            server.join().unwrap();
            assert!(e.contains(code) && e.contains(hint) && e.contains("少し待"), "{status}: {e}");
            assert!(!e.contains("キーが正しくない"), "上限・混雑を、キーの誤りと取り違えない: {e}");
        }
    }

    #[test]
    fn a_missing_key_says_where_to_save_it_and_a_keychain_failure_is_not_hidden() {
        let api = AnthropicApi { key_override: None, ..api_to("http://127.0.0.1:1/v1/messages".into()) };
        let e = api.key_with(|| Ok(None)).unwrap_err();
        assert!(e.contains("未設定") && e.contains("設定画面"), "{e}");
        let e = api.key_with(|| Err("キーチェーンから読めませんでした: boom".into())).unwrap_err();
        assert!(e.contains("boom"), "キーチェーンの失敗を、「未設定」にすり替えない: {e}");
        assert_eq!(api.key_with(|| Ok(Some("sk-ant-x".into()))).unwrap(), "sk-ant-x");
        assert_eq!(api_to("http://x".into()).key_with(|| panic!("上書きがあれば、キーチェーンを見ない")).unwrap(), TEST_KEY);
    }

    #[test]
    fn an_unreadable_error_body_is_said_and_the_http_hint_stays() {
        // 本文の長さ（content-length）を、実際より長く言って、読み取りの途中で切る
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let h = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).ok();
            // 要求を最後まで読んでから返す（読み切らずに閉じると、相手が接続の切断として受け取ってしまう）
            let (mut buf, mut chunk) = (Vec::new(), [0u8; 4096]);
            loop {
                let n = s.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                    let cl = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                    if buf.len() >= pos + 4 + cl {
                        break;
                    }
                }
            }
            s.write_all(b"HTTP/1.1 500 Internal Server Error\r\ncontent-type: application/json\r\ncontent-length: 500\r\nconnection: close\r\n\r\n{\"er").ok();
        });
        let e = api_to(format!("http://{addr}/v1/messages")).run("x").unwrap_err();
        h.join().unwrap();
        assert!(e.contains("500"), "{e}");
        assert!(e.contains("本文を読めませんでした"), "読めなかったことを、空の本文にすり替えない: {e}");
    }

    #[test]
    fn a_response_that_stopped_for_another_reason_is_not_used_as_a_complete_answer() {
        let refused = r#"{"model":"m","content":[{"type":"text","text":"{\"findings\":[]}"}],"stop_reason":"refusal"}"#;
        let e = parse_anthropic(refused, 4096, "m").unwrap_err();
        assert!(e.contains("途中で終わりました") && e.contains("refusal"), "{e}");
        for ok in ["end_turn", "stop_sequence"] {
            let body = format!(r#"{{"model":"m","content":[{{"type":"text","text":"{{}}"}}],"stop_reason":"{ok}"}}"#);
            assert!(parse_anthropic(&body, 4096, "m").is_ok(), "{ok}");
        }
        let no_reason = r#"{"model":"m","content":[{"type":"text","text":"{}"}]}"#;
        assert!(parse_anthropic(no_reason, 4096, "m").is_ok(), "印が無いときは、本文があれば使う（従来どおり）");
    }

    #[test]
    fn a_response_without_a_model_name_records_the_requested_one_and_says_so() {
        let body = r#"{"content":[{"type":"text","text":"{}"}],"stop_reason":"end_turn"}"#;
        let r = parse_anthropic(body, 4096, "claude-sonnet-5-5").unwrap();
        assert!(r.model.contains("claude-sonnet-5-5") && r.model.contains("応答にモデル名なし"), "{}", r.model);
    }

    #[test]
    fn anthropic_unreachable_server_is_an_error_without_the_key() {
        // 使われていないポートを借りて、閉じる
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let e = api_to(format!("http://127.0.0.1:{port}/v1/messages")).run("x").unwrap_err();
        assert!(e.contains("接続できませんでした") && !e.contains("TESTKEY"), "{e}");
    }

    #[test]
    fn distribution_build_lists_only_byok_and_mock() {
        let ids: Vec<String> = all(PathBuf::from("/data"), byok::Config::default()).iter().map(|p| p.info().id).collect();
        assert_eq!(ids, ["anthropic-api", "mock"], "配布ビルドに、CLI の LLM は入らない");
        assert!(get_with("claude-cli", PathBuf::from("/data"), Duration::from_secs(1), byok::Config::default()).is_none());
        assert!(get_with("agy-cli", PathBuf::from("/data"), Duration::from_secs(1), byok::Config::default()).is_none());
    }

    #[test]
    fn mock_quotes_a_real_line_and_one_invented() {
        let prompt = "x\n```diff\ndiff --git a/a.js b/a.js\n--- a/a.js\n+++ b/a.js\n@@ -1 +1,2 @@\n x\n+  danger(id);\n```\n";
        let r = Mock.run(prompt).unwrap();
        let mut f = crate::findings::parse(&r.text).unwrap();
        assert_eq!(f.len(), 2);
        let files = vec![crate::snapshot::FileDiff { path: "a.js".into(), bytes: 0, text: "+  danger(id);\n".into() }];
        crate::findings::verify(&mut f, &files);
        assert!(f[0].verified);
        assert!(!f[1].verified);
    }

    #[test]
    fn mock_with_empty_diff_returns_no_findings() {
        let r = Mock.run("```diff\n```\n").unwrap();
        assert!(crate::findings::parse(&r.text).unwrap().is_empty());
    }
}
