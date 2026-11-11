mod args;
mod handoff_cmd;

use args::{Cli, Commands, ReviewArgs};
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use mrcr_core::byok;
use mrcr_core::findings;
use mrcr_core::format::{CliReviewReport, RecipeReviewResult};
use mrcr_core::i18n::{fill, Lang};
use mrcr_core::provider::{self, AnthropicApi, Provider, ProviderInfo};
use mrcr_core::recipe::{self, Recipe};
use mrcr_core::related;
use mrcr_core::snapshot;
use mrcr_core::store::{self, ReviewRecord, SnapshotRecord};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// データの置き場。GUI と同じ規則（`mrcr_core::datadir`）。旧の名前の置き場があれば、ロックを取れたときだけ移す。
/// ロックを取れない（GUI が動いている、など）ときは、何も動かさずに、使う置き場だけを決める。
fn resolve_data_dir() -> PathBuf {
    resolve_data_dir_in(std::env::var("MRCR_DATA_DIR").ok().map(PathBuf::from), mrcr_core::datadir::default_base())
}

/// `resolve_data_dir` の本体。環境変数（`explicit`）と基準のフォルダ（`base`）を、引数で受ける（試験できるように）。
/// `explicit` があれば、ロックも移しもせず、そこをそのまま使う（隔離したデータを、本物と混ぜない）。
fn resolve_data_dir_in(explicit: Option<PathBuf>, base: Option<PathBuf>) -> PathBuf {
    use mrcr_core::applock::{acquire, Acquired};
    use mrcr_core::datadir::{self, Mode};
    if let Some(dir) = explicit {
        return dir;
    }
    let Some(base) = base else {
        return std::env::temp_dir().join(datadir::DATA_ID);
    };
    // ロックは、置き場を決めるあいだだけ持つ（関数を出ると外れる）。
    let lock = acquire(&datadir::lock_file(&base, None));
    let mode = match &lock {
        Acquired::Locked(_) => Mode::Migrate,
        // GUI などが、すでに置き場を使っている。移しは、そちらが済ませている。動かさずに、決めるだけ
        Acquired::AlreadyRunning => Mode::ReadOnly,
        // ロックを試せなかった。移さないが、理由は言う（旧の置き場が残っていても、なぜ移らないのか分かるように）
        Acquired::Unsupported(why) => {
            eprintln!("データの置き場の移しを行いません: ロックを試せませんでした（{why}）。いまある置き場を、そのまま使います。");
            Mode::ReadOnly
        }
    };
    datadir::resolve_in(&base, mode).dir
}

fn resolve_api_key() -> Result<String, String> {
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        let trimmed = key.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }
    Err("Anthropic API キーが見つかりません。\n\
         環境変数 `ANTHROPIC_API_KEY` にキーを設定してください。\n\
         例: export ANTHROPIC_API_KEY=\"sk-ant-...\"\n\
         ※Claude.app や月額サブスク（Pro/Team）の認証とは異なり、Anthropic Console で発行した従量課金 API キーが必要です。".to_string())
}

pub fn normalize_recipe_id(id: &str) -> &str {
    match id.trim() {
        "bug" => "bugs",
        "error" | "exception" => "errors",
        "perf" => "performance",
        "test" => "tests",
        other => other,
    }
}

fn main() {
    let cli = Cli::parse();
    if let Some(shell_str) = cli.completions {
        print_completions(&shell_str);
        std::process::exit(0);
    }
    let code = match cli.command {
        Some(Commands::Completions { shell }) => {
            print_completions(&shell);
            0
        }
        Some(Commands::Recipes { repo, lang }) => run_recipes(&repo, &lang),
        Some(Commands::Handoff { action }) => handoff_cmd::run(action),
        Some(Commands::Review(args)) => run_review(args),
        None => run_review(cli.review),
    };
    std::process::exit(code);
}

fn print_completions(shell_str: &str) {
    let shell = match shell_str.to_lowercase().as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "powershell" | "pwsh" => Shell::PowerShell,
        "elvish" => Shell::Elvish,
        other => {
            eprintln!("未対応のシェルです: {other} (対応: bash, zsh, fish, powershell, elvish)");
            std::process::exit(1);
        }
    };
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "mrcr", &mut std::io::stdout());
}

fn run_recipes(repo: &Path, lang: &str) -> i32 {
    let canonical_repo = match repo.canonicalize() {
        Ok(p) => p,
        Err(_) => repo.to_path_buf(),
    };
    let recipes = recipe::list_with_lang(Some(&canonical_repo), lang);
    let c = &Lang::from_code(lang).unwrap_or_default().texts().cli;
    println!("{}", c.recipes_heading);
    println!("{:<15} {:<20} {}", "ID", c.recipes_col_title, c.recipes_col_source);
    println!("{}", "-".repeat(50));
    for r in recipes {
        let src_label = if r.source == "builtin" {
            c.recipes_src_builtin
        } else {
            c.recipes_src_repo
        };
        println!("{:<15} {:<20} {}", r.id, r.title, src_label);
    }
    0
}

fn run_review(args: ReviewArgs) -> i32 {
    let repo_path = match args.repo.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("エラー: リポジトリパスが見つかりません ({}): {e}", args.repo.display());
            return 2;
        }
    };

    let repo_str = match repo_path.to_str() {
        Some(s) => s.to_string(),
        None => {
            eprintln!("エラー: リポジトリパスが有効な UTF-8 ではありません");
            return 2;
        }
    };

    let snap = match snapshot::take_with_base(&repo_str, &snapshot::Limits::default(), Some(&args.base)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("エラー: 差分の取得に失敗しました: {e}");
            return 2;
        }
    };

    if snap.files.is_empty() {
        if args.stdout {
            println!("差分がありませんでした (対象範囲: {})", args.base);
        } else {
            eprintln!("変更されているファイルがありません（差分 0 件）。レビューをスキップしました。");
        }
        return 0;
    }

    let all_recipes = recipe::list_with_lang(Some(&repo_path), &args.lang);
    let target_recipes: Vec<Recipe> = if args.all {
        all_recipes
    } else {
        let mut list = Vec::new();
        for raw_id in args.recipe.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            let id = normalize_recipe_id(raw_id);
            match all_recipes.iter().find(|r| r.id == id).cloned() {
                Some(r) => list.push(r),
                None => {
                    eprintln!("エラー: 観点 '{}' が見つかりません。`mrcr recipes` で一覧を確認してください。", raw_id);
                    return 2;
                }
            }
        }
        list
    };

    if target_recipes.is_empty() {
        eprintln!("エラー: 実行する観点が指定されていません。");
        return 2;
    }

    let model_name = args.model.unwrap_or_else(|| byok::DEFAULT_MODEL.to_string());

    let (provider_box, provider_id, provider_label): (Box<dyn Provider>, String, String) = if args.mock {
        (Box::new(provider::Mock), "mock".into(), "Mock Provider (CLI Test)".into())
    } else {
        let api_key = match resolve_api_key() {
            Ok(k) => k,
            Err(e) => {
                eprintln!("エラー: {e}");
                return 2;
            }
        };
        let api = AnthropicApi::new(
            byok::Config {
                model: model_name.clone(),
                max_tokens: byok::DEFAULT_MAX_TOKENS,
            },
            std::time::Duration::from_secs(args.timeout),
            Some(api_key),
        );
        (Box::new(api), "anthropic-api".into(), "Anthropic API (CLI)".into())
    };

    let mut results: Vec<RecipeReviewResult> = Vec::new();
    let data_dir = resolve_data_dir();

    let cli_texts = &Lang::from_code(&args.lang).unwrap_or_default().texts().cli;
    eprintln!(
        "{}",
        fill(cli_texts.review_start, &[("n", &snap.files.len().to_string()), ("base", &args.base)])
    );

    for (i, r) in target_recipes.iter().enumerate() {
        eprintln!(
            "{}",
            fill(
                cli_texts.review_running,
                &[
                    ("i", &(i + 1).to_string()),
                    ("n", &target_recipes.len().to_string()),
                    ("title", &r.title),
                    ("id", &r.id),
                ],
            )
        );

        let started = Instant::now();

        let related_block = if r.needs_related {
            related::render(&related::collect(&repo_path, &snap, &related::Limits::default()))
        } else {
            String::new()
        };

        let prompt = recipe::compose_with_lang(r, &related_block, &snap.diff_text, &args.lang);

        let llm_res = match provider_box.run(&prompt) {
            Ok(res) => res,
            Err(e) => {
                eprintln!("エラー (観点: {}): LLM の呼び出しに失敗しました: {e}", r.id);
                return 2;
            }
        };

        let duration_ms = started.elapsed().as_millis() as u64;

        let (mut findings, parse_err) = match findings::parse(&llm_res.text) {
            Ok(f) => (f, None),
            Err(e) => (Vec::new(), Some(e)),
        };

        findings::verify(&mut findings, &snap.files);

        if !args.no_save {
            let rec = ReviewRecord {
                id: format!("{}-{}", now_ms(), &snap.diff_hash[..8.min(snap.diff_hash.len())]),
                created_at: now_ms(),
                repo: repo_str.clone(),
                recipe: r.clone(),
                provider: ProviderInfo {
                    id: provider_id.clone(),
                    label: provider_label.clone(),
                    note: format!("モデル: {model_name}"),
                },
                snapshot: SnapshotRecord::from(&snap),
                prompt,
                model: model_name.clone(),
                response_text: llm_res.text.clone(),
                raw: llm_res.raw.clone(),
                usage: llm_res.usage.clone(),
                cost_usd: llm_res.cost_usd,
                duration_ms,
                findings: findings.clone(),
                carries: Vec::new(),
                fates: Vec::new(),
                notes: Vec::new(),
                related: None,
                repo_overrides: Vec::new(),
                settings: None,
                parse_error: parse_err,
                error: None,
            };

            if let Err(e) = store::append(&data_dir, &rec) {
                eprintln!("警告: SQLite への履歴保存に失敗しました: {e}");
            }
        }

        results.push(RecipeReviewResult {
            recipe_id: r.id.clone(),
            recipe_title: r.title.clone(),
            findings,
            usage: llm_res.usage,
            model: model_name.clone(),
        });
    }

    let report = CliReviewReport::new(&snap, results);
    let total_findings = report.total_findings();
    let verified = report.verified_findings();
    let unverified = report.unverified_findings();

    let output_content = if args.format == "json" {
        match report.to_json() {
            Ok(j) => j,
            Err(e) => {
                eprintln!("エラー: {e}");
                return 2;
            }
        }
    } else {
        report.to_markdown_lang(Lang::from_code(&args.lang).unwrap_or_default())
    };

    if args.stdout {
        println!("{output_content}");
    } else {
        let out_path = match args.output {
            Some(p) => p,
            None => repo_path
                .join(".mrcodereview/out")
                .join(format!("review.{}", args.format)),
        };

        if let Some(parent) = out_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("エラー: 出力先ディレクトリの作成に失敗しました: {e}");
                return 2;
            }
            let gitignore_path = parent.join(".gitignore");
            if parent.ends_with(".mrcodereview/out") && !gitignore_path.exists() {
                let _ = std::fs::write(&gitignore_path, "# コードレビューの書き出し。コミットしない\n*\n");
            }
        }

        if let Err(e) = std::fs::write(&out_path, &output_content) {
            eprintln!("エラー: レポートの書き出しに失敗しました ({}): {e}", out_path.display());
            return 2;
        }

        eprintln!("\n{}", fill(cli_texts.review_done, &[("path", &out_path.display().to_string())]));
        eprintln!(
            "{}",
            fill(
                cli_texts.review_total,
                &[
                    ("total", &total_findings.to_string()),
                    ("verified", &verified.to_string()),
                    ("unverified", &unverified.to_string()),
                ],
            )
        );
        if !args.no_save {
            eprintln!("{}", fill(cli_texts.review_history, &[("path", &data_dir.display().to_string())]));
        }
    }

    if total_findings > 0 {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod data_dir_tests {
    use super::*;
    use mrcr_core::applock::{acquire, Acquired};
    use mrcr_core::datadir::{lock_file, DATA_ID, LEGACY_ID};
    use std::fs;

    fn base(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mrcr-cli-datadir-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn with_legacy(b: &Path) {
        fs::create_dir_all(b.join(LEGACY_ID)).unwrap();
        fs::write(b.join(LEGACY_ID).join("marker"), "history").unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn without_the_lock_held_the_legacy_dir_is_moved_and_a_link_is_left() {
        let b = base("moves");
        with_legacy(&b);
        let got = resolve_data_dir_in(None, Some(b.clone()));
        assert_eq!(got, b.join(DATA_ID));
        assert_eq!(fs::read_to_string(got.join("marker")).unwrap(), "history");
        assert!(fs::symlink_metadata(b.join(LEGACY_ID)).unwrap().file_type().is_symlink(), "旧の名前に、リンクが残る");
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn while_another_process_holds_the_lock_nothing_is_moved() {
        let b = base("held");
        with_legacy(&b);
        // GUI が動いているつもり: ロックを先に取っておく
        let held = acquire(&lock_file(&b, None));
        assert!(matches!(held, Acquired::Locked(_)));
        let got = resolve_data_dir_in(None, Some(b.clone()));
        assert_eq!(got, b.join(LEGACY_ID), "移さず、いまある旧の置き場を使う");
        assert!(!b.join(DATA_ID).exists(), "新を作らない");
        assert_eq!(fs::read_to_string(b.join(LEGACY_ID).join("marker")).unwrap(), "history");
        drop(held);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn when_the_lock_cannot_even_be_tried_nothing_is_moved_either() {
        let b = base("unsupported");
        with_legacy(&b);
        // ロックのファイルの場所が「フォルダ」だと、ファイルとして開けない（ロックを試せない）
        fs::create_dir_all(lock_file(&b, None)).unwrap();
        let got = resolve_data_dir_in(None, Some(b.clone()));
        assert_eq!(got, b.join(LEGACY_ID));
        assert!(!b.join(DATA_ID).exists());
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn an_explicit_data_dir_is_used_as_is_with_no_lock_and_no_move() {
        let b = base("explicit");
        with_legacy(&b);
        let custom = b.join("isolated").join("data");
        let got = resolve_data_dir_in(Some(custom.clone()), Some(b.clone()));
        assert_eq!(got, custom, "指定どおり");
        assert_eq!(fs::read_to_string(b.join(LEGACY_ID).join("marker")).unwrap(), "history", "本物の置き場は、無傷");
        assert!(!b.join(DATA_ID).exists(), "新を作らない");
        assert!(!lock_file(&b, None).exists(), "ロックのファイルも作らない");
        let _ = fs::remove_dir_all(&b);
    }
}
