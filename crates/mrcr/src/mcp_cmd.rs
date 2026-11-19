//! MCP（Model Context Protocol）サーバー — コーディングエージェント（agy, Claude Code 等）との双方向連携。
//!
//! - Stdio で JSON-RPC 2.0 メッセージを送受信する。
//! - ツール `mrcr_get_handoff`: MCR の SQLite DB から「直す」指摘を取得する。
//! - ツール `mrcr_report`: 指摘への対応結果を DB に直接書き戻す（手動取り込みボタン不要）。
//! - ツール `mrcr_list_handoffs`: 未対応レビューの一覧を取得する。
//! - サブコマンド `setup`: 各エージェント（agy, Claude Code 等）の設定ファイルに mrcr mcp を登録する。

use crate::args::McpAction;
use crate::handoff_cmd;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

const PROTOCOL_VERSION: &str = "2024-11-05";

pub fn run(action: Option<McpAction>) -> i32 {
    match action {
        Some(McpAction::Setup) => setup(),
        None => serve(),
    }
}

/// MCP サーバー本体（Stdio JSON-RPC ループ）
fn serve() -> i32 {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let dir = crate::resolve_data_dir();

    for line_res in stdin.lock().lines() {
        let line = match line_res {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[mrcr mcp] stdin 読み込みエラー: {e}");
                break;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[mrcr mcp] JSON パースエラー: {e} (入力: {trimmed})");
                let err_res = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {e}")
                    }
                });
                let _ = writeln!(stdout, "{err_res}");
                let _ = stdout.flush();
                continue;
            }
        };

        if let Some(res) = handle_message(&dir, &req) {
            let _ = writeln!(stdout, "{res}");
            let _ = stdout.flush();
        }
    }

    0
}

/// JSON-RPC メッセージを処理し、応答があれば返す（通知の場合は None）
pub(crate) fn handle_message(dir: &Path, req: &Value) -> Option<Value> {
    let id = req.get("id").cloned();
    let method = match req.get("method").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => {
            // id があればエラーを返す
            return id.map(|i| {
                json!({
                    "jsonrpc": "2.0",
                    "id": i,
                    "error": { "code": -32600, "message": "Invalid Request: missing method" }
                })
            });
        }
    };

    // 通知（id なし）の処理
    if id.is_none() {
        if method == "notifications/initialized" {
            // 初期化完了通知
            return None;
        }
        return None;
    }

    let id = id.unwrap();
    let params = req.get("params");

    match method {
        "initialize" => {
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "mrcodereview",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            }))
        }
        "ping" => {
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {}
            }))
        }
        "tools/list" => {
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": tools_def()
                }
            }))
        }
        "tools/call" => {
            let res = call_tool(dir, params);
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": res
            }))
        }
        _ => {
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {method}")
                }
            }))
        }
    }
}

fn tools_def() -> Vec<Value> {
    vec![
        json!({
            "name": "mrcr_get_handoff",
            "description": "MrCodeReview のローカル DB から、直近または指定レビューの「直す（fix）」指摘一覧を取得します。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "対象リポジトリのパス（省略時は現在のカレントディレクトリ）"
                    },
                    "review_id": {
                        "type": "string",
                        "description": "特定のレビューID（省略時は最新の未対応レビュー）"
                    },
                    "json": {
                        "type": "boolean",
                        "description": "JSON 形式で取得するかどうか（省略時は読みやすい Markdown）"
                    }
                }
            }
        }),
        json!({
            "name": "mrcr_report",
            "description": "MrCodeReview の指摘に対する修正完了（または見送り）を MCR のローカル DB に書き戻します。手動で取り込みボタンを押すことなく MCR 側も自動で対応済みになります。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "review_id": {
                        "type": "string",
                        "description": "レビューID（例: '1793-1a2b3c4d'）"
                    },
                    "finding": {
                        "type": "string",
                        "description": "指摘番号（1始まり。例: '1' や '#1'、またはカンマ区切り '1,2'）"
                    },
                    "did": {
                        "type": "boolean",
                        "description": "修正完了した場合は true（省略時: true）"
                    },
                    "skipped": {
                        "type": "boolean",
                        "description": "見送った場合は true（省略時: false）"
                    },
                    "text": {
                        "type": "string",
                        "description": "対応内容または見送り理由の簡潔なメモ（例: '境界値チェックを追加'）"
                    },
                    "client": {
                        "type": "string",
                        "description": "エージェント名（例: 'agy', 'claude-code'。省略時は 'mcp'）"
                    }
                },
                "required": ["review_id", "finding", "text"]
            }
        }),
        json!({
            "name": "mrcr_list_handoffs",
            "description": "MrCodeReview で未対応の指摘が残っているレビューの一覧を取得します。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "対象リポジトリのパス（省略時は現在のカレントディレクトリ）"
                    }
                }
            }
        })
    ]
}

fn call_tool(dir: &Path, params: Option<&Value>) -> Value {
    let params = match params {
        Some(p) => p,
        None => return tool_error("params がありません"),
    };

    let name = match params.get("name").and_then(|n| n.as_str()) {
        Some(n) => n,
        None => return tool_error("ツール名 (name) が指定されていません"),
    };

    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

    match name {
        "mrcr_get_handoff" => {
            let repo_str = args.get("repo").and_then(|r| r.as_str()).unwrap_or(".");
            let repo = PathBuf::from(repo_str);
            let review_id = args.get("review_id").and_then(|r| r.as_str());
            let as_json = args.get("json").and_then(|j| j.as_bool()).unwrap_or(false);

            match handoff_cmd::get(dir, &repo, review_id, as_json) {
                Ok((out, _)) => tool_ok(&out),
                Err(e) => tool_error(&e),
            }
        }
        "mrcr_report" => {
            let review_id = match args.get("review_id").and_then(|r| r.as_str()) {
                Some(r) => r,
                None => return tool_error("review_id は必須です"),
            };
            let finding = match args.get("finding") {
                Some(f) if f.is_string() => f.as_str().unwrap().to_string(),
                Some(f) if f.is_number() => f.to_string(),
                _ => return tool_error("finding（指摘番号）は必須です"),
            };
            let text = match args.get("text").and_then(|t| t.as_str()) {
                Some(t) => t,
                None => return tool_error("text（対応内容のメモ）は必須です"),
            };
            let skipped = args.get("skipped").and_then(|s| s.as_bool()).unwrap_or(false);
            let did = args.get("did").and_then(|d| d.as_bool()).unwrap_or(!skipped);
            let client = args.get("client").and_then(|c| c.as_str()).unwrap_or("mcp");

            let verdict = if did && !skipped {
                mrcr_core::agent_report::DID
            } else {
                mrcr_core::agent_report::SKIPPED
            };

            match handoff_cmd::report(dir, review_id, &finding, verdict, text, client) {
                Ok((out, _)) => tool_ok(&out),
                Err(e) => tool_error(&e),
            }
        }
        "mrcr_list_handoffs" => {
            let repo_str = args.get("repo").and_then(|r| r.as_str()).unwrap_or(".");
            let repo = PathBuf::from(repo_str);

            match handoff_cmd::list(dir, &repo, false) {
                Ok((out, _)) => tool_ok(&out),
                Err(e) => tool_error(&e),
            }
        }
        _ => tool_error(&format!("未定義のツールです: {name}")),
    }
}

fn tool_ok(text: &str) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ],
        "isError": false
    })
}

fn tool_error(msg: &str) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": format!("エラー: {msg}")
            }
        ],
        "isError": true
    })
}

/// 各エージェントへの設定を支援する
fn setup() -> i32 {
    println!("=== MrCodeReview MCP 設定アシスタント ===");
    println!();

    let mut configured = Vec::new();

    // 1. agy の設定 (~/.gemini/config/mcp_config.json)
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let agy_config_dir = home.join(".gemini").join("config");
        let agy_config_path = agy_config_dir.join("mcp_config.json");

        match register_mcp_to_file(&agy_config_path) {
            Ok(true) => {
                println!("✓ agy (Google Antigravity): 設定ファイルに追加しました ({})", agy_config_path.display());
                configured.push("agy");
            }
            Ok(false) => {
                println!("- agy (Google Antigravity): 既に登録済みです ({})", agy_config_path.display());
                configured.push("agy");
            }
            Err(e) => {
                println!("! agy の設定ファイル更新をスキップしました: {e}");
            }
        }
    }

    println!();
    println!("--- 各ツールの利用方法 ---");
    println!("【agy】");
    println!("  ターミナルで `agy` を起動し、");
    println!("  「MCR のレビュー指摘を直して」と伝えるだけでツールが呼ばれます。");
    println!();
    println!("【Claude Code】");
    println!("  以下のコマンドを実行して MCP サーバーを登録できます:");
    println!("    claude mcp add mrcodereview -- mrcr mcp");
    println!();
    println!("【VS Code / Copilot】");
    println!("  `.vscode/mcp.json` に以下を記述してください:");
    println!(r#"    {{
      "mcpServers": {{
        "mrcodereview": {{
          "command": "mrcr",
          "args": ["mcp"]
        }}
      }}
    }}"#);
    println!();

    0
}

/// 指定パスの mcp_config.json に mrcodereview サーバーを追加する（変更があれば true）
fn register_mcp_to_file(path: &Path) -> Result<bool, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }

    let mut doc: Value = if path.exists() {
        let content = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    let servers = doc.as_object_mut()
        .and_then(|o| {
            if !o.contains_key("mcpServers") || !o["mcpServers"].is_object() {
                o.insert("mcpServers".to_string(), json!({}));
            }
            o.get_mut("mcpServers")?.as_object_mut()
        })
        .ok_or_else(|| "mcpServers オブジェクトを作成できませんでした".to_string())?;

    let exe_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "mrcr".to_string());

    let target = json!({
        "command": exe_path,
        "args": ["mcp"]
    });

    if servers.get("mrcodereview") == Some(&target) {
        return Ok(false);
    }

    servers.insert("mrcodereview".to_string(), target);

    let pretty = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    std::fs::write(path, format!("{pretty}\n")).map_err(|e| format!("{}: {e}", path.display()))?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_and_ping() {
        let dir = Path::new("/dummy");
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        let res = handle_message(dir, &init_req).unwrap();
        assert_eq!(res["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(res["result"]["serverInfo"]["name"], "mrcodereview");

        let ping_req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "ping"
        });
        let ping_res = handle_message(dir, &ping_req).unwrap();
        assert_eq!(ping_res["id"], 2);
    }

    #[test]
    fn test_tools_list() {
        let dir = Path::new("/dummy");
        let list_req = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/list"
        });
        let res = handle_message(dir, &list_req).unwrap();
        let tools = res["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"mrcr_get_handoff"));
        assert!(names.contains(&"mrcr_report"));
        assert!(names.contains(&"mrcr_list_handoffs"));
    }

    #[test]
    fn test_unknown_method_and_tool() {
        let dir = Path::new("/dummy");
        let unk_req = json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "unknown_method"
        });
        let res = handle_message(dir, &unk_req).unwrap();
        assert_eq!(res["error"]["code"], -32601);

        let unk_tool = json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "non_existent_tool"
            }
        });
        let res2 = handle_message(dir, &unk_tool).unwrap();
        assert_eq!(res2["result"]["isError"], true);
    }
}
