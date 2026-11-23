//! `mrcr hook` — コミットのあとに自動でレビューを走らせる Git フック（post-commit）。
//!
//! - `enable` / `disable` / `status`: 人（と GUI）が使う。設定は、データの置き場の `hooks.json`（`mrcr_core::hooks`）。
//! - `run post-commit`: Git のフックから呼ばれる。設定が OFF・無ければ、何もしない。ON なら、自分自身を `review --commit` で呼んで、
//!   結果を通常の履歴に入れ、成否を `hooks.log` に残す。**コミットは止めない**ので、常に終了コード 0。

use crate::args::HookAction;
use mrcr_core::hooks;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn run(action: HookAction) -> i32 {
    let dir = crate::resolve_data_dir();
    match action {
        HookAction::Enable { repo, recipe, spec } => {
            let recipes: Vec<String> = recipe.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            report(hooks::set(&dir, &repo.to_string_lossy(), true, &recipes, spec.as_deref()))
        }
        HookAction::Disable { repo } => {
            // 観点と設計書は残す（いまの設定を読んで、そのまま渡す）
            let (recipes, spec) = match hooks::status(&dir, &repo.to_string_lossy()) {
                Ok(s) => (s.recipes, s.spec),
                Err(e) => {
                    eprintln!("エラー: {e}");
                    return 2;
                }
            };
            report(hooks::set(&dir, &repo.to_string_lossy(), false, &recipes, spec.as_deref()))
        }
        HookAction::Status { repo, json } => match hooks::status(&dir, &repo.to_string_lossy()) {
            Ok(s) if json => {
                println!("{}", serde_json::to_string_pretty(&s).unwrap_or_default());
                0
            }
            Ok(s) => {
                print_status(&s);
                0
            }
            Err(e) => {
                eprintln!("エラー: {e}");
                2
            }
        },
        HookAction::Run { name, mock } => run_hook(&dir, &name, mock),
    }
}

fn report(r: Result<hooks::Status, String>) -> i32 {
    match r {
        Ok(s) => {
            print_status(&s);
            0
        }
        Err(e) => {
            eprintln!("エラー: {e}");
            2
        }
    }
}

fn print_status(s: &hooks::Status) {
    println!("リポジトリ: {}", s.repo);
    println!("設定: {}（観点: {}）", if s.enabled { "ON" } else { "OFF" }, s.recipes.join(", "));
    if let Some(sp) = &s.spec {
        println!("設計書: {sp}");
    }
    println!("フック: {}", if s.installed { "入っている" } else { "入っていない" });
    println!("mrcr: {}", s.cli.as_deref().unwrap_or("見つかりません（ON にするには、先に入れてください）"));
    if let Some(b) = &s.blocked {
        println!("注意: {b}");
    }
    if s.target_missing {
        println!("注意: フックが呼ぶ mrcr が見つかりません（更新や削除で消えた可能性があります）。mrcr を入れ直してから、`mrcr hook enable` をもう一度実行してください");
    }
    match &s.last_run {
        Some(l) => println!("直近の実行: {}（終了コード {}）", l.message, l.code),
        None => println!("直近の実行: まだありません"),
    }
}

/// フックの本体。失敗は、握りつぶさず `hooks.log` に残す（コミットは止めないので、終了コードは 0）。
fn run_hook(dir: &Path, name: &str, mock: bool) -> i32 {
    if name != hooks::HOOK_NAME {
        eprintln!("エラー: 未対応のフックです: {name}（対応: {}）", hooks::HOOK_NAME);
        return 2;
    }
    let cwd = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| ".".into());
    let key = match hooks::repo_key(&cwd) {
        Ok(k) => k,
        Err(e) => {
            let _ = hooks::append_log(dir, &cwd, 2, &format!("リポジトリを特定できません: {e}"));
            return 0;
        }
    };
    let cfg = match hooks::load(dir) {
        Ok(c) => c,
        Err(e) => {
            let _ = hooks::append_log(dir, &key, 2, &e);
            return 0;
        }
    };
    let Some(entry) = cfg.post_commit.get(&key).filter(|e| e.enabled) else { return 0 };

    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            let _ = hooks::append_log(dir, &key, 2, &format!("自分の実行ファイルを特定できません: {e}"));
            return 0;
        }
    };
    let mut cmd = Command::new(exe);
    cmd.args(["review", "--commit", "--stdout", "-C", &key, "-r", &entry.recipes.join(",")]);
    if let Some(sp) = &entry.spec {
        cmd.args(["--spec", sp]);
    }
    if mock {
        cmd.arg("--mock");
    }
    let out = cmd.stdout(Stdio::null()).stderr(Stdio::piped()).output();
    match out {
        Ok(o) => {
            let code = o.status.code().unwrap_or(2);
            let stderr = String::from_utf8_lossy(&o.stderr);
            let (code, msg) = match code {
                0 => (0, "レビュー完了（指摘なし）".to_string()),
                1 => (1, "レビュー完了（指摘あり）".to_string()),
                _ => {
                    let tail: String = stderr.lines().rev().find(|l| l.contains("エラー")).unwrap_or("理由は不明です").to_string();
                    (2, format!("レビューに失敗しました: {tail}"))
                }
            };
            let _ = hooks::append_log(dir, &key, code, &msg);
        }
        Err(e) => {
            let _ = hooks::append_log(dir, &key, 2, &format!("レビューを起動できません: {e}"));
        }
    }
    0
}
