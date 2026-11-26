//! `mrcr hook` — コミットのあとに自動でレビューを走らせる Git フック（post-commit）。
//!
//! - `enable` / `disable` / `status`: 人（と GUI）が使う。設定は、データの置き場の `hooks.json`（`mrcr_core::hooks`）。
//! - `log`: 最近の実行の経過（どのコミットを、どの観点で、何件・何秒で）を見る。
//! - `run post-commit`: Git のフックから呼ばれる。設定が OFF・無ければ、何もしない。ON なら、**観点ごとに**自分自身を
//!   `review --commit <ハッシュ>` で呼んで、結果を通常の履歴に入れ、経過（開始・観点ごと・終了）を `hooks.log` に残す。
//!   観点ごとに分けるので、1 つが失敗しても、残りは続けて走る。コミットは止めないので、常に終了コード 0。

use crate::args::HookAction;
use mrcr_core::hooks;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

pub fn run(action: HookAction) -> i32 {
    let dir = crate::resolve_data_dir();
    match action {
        HookAction::Enable { repo, recipe, spec } => {
            let recipes: Vec<String> = recipe.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            report(hooks::set_specs(&dir, &repo.to_string_lossy(), true, &recipes, &spec))
        }
        HookAction::Disable { repo } => {
            // 観点と設計書は残す（いまの設定を読んで、そのまま渡す）
            let (recipes, spec) = match hooks::status(&dir, &repo.to_string_lossy()) {
                Ok(s) => (s.recipes, s.specs),
                Err(e) => {
                    eprintln!("エラー: {e}");
                    return 2;
                }
            };
            report(hooks::set_specs(&dir, &repo.to_string_lossy(), false, &recipes, &spec))
        }
        HookAction::Log { repo, count, json } => log_cmd(&dir, &repo.to_string_lossy(), count, json),
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
        HookAction::Run { name, verbose, mock } => run_hook(&dir, &name, verbose, mock),
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
    for sp in &s.specs {
        println!("設計書: {sp}");
    }
    println!("フック: {}", if s.installed { "入っている" } else { "入っていない" });
    match (&s.hook_cli, &s.cli) {
        (Some(h), Some(c)) if h != c => {
            println!("フックが呼ぶ mrcr: {h}");
            println!("いま見つかる mrcr: {c}（`mrcr hook disable` のあと `mrcr hook enable` で入れ直すと、切り替わります）");
        }
        (Some(h), _) => println!("フックが呼ぶ mrcr: {h}"),
        (None, Some(c)) => println!("ON にすると、フックが呼ぶ mrcr: {c}"),
        (None, None) => println!("mrcr: 見つかりません（ON にするには、先に入れてください）"),
    }
    if let Some(b) = &s.blocked {
        println!("注意: {b}");
    }
    if s.target_missing {
        println!("注意: フックが呼ぶ mrcr が見つかりません（更新や削除で消えた可能性があります）。mrcr を入れ直してから、`mrcr hook enable` をもう一度実行してください");
    }
    match &s.last_run {
        Some(l) => println!("直近の実行: {}（終了コード {}）。経過は `mrcr hook log`", l.message, l.code),
        None => println!("直近の実行: まだありません"),
    }
    if s.log_unreadable > 0 {
        println!("注意: 実行ログに、読めない行が {} 行あります（ほかのリポジトリの分を含みます）", s.log_unreadable);
    }
}

/// ミリ秒の時刻を、手元の時刻の文字列にする（macOS の `date -r`）。できなければ、数字のまま。
fn clock(ms: u64) -> String {
    Command::new("date")
        .args(["-r", &(ms / 1000).to_string(), "+%Y-%m-%d %H:%M:%S"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| ms.to_string())
}

fn secs(ms: u64) -> String {
    format!("{:.1} 秒", ms as f64 / 1000.0)
}

fn state_label(state: &str) -> &'static str {
    match state {
        "running" => "実行中",
        "done" => "完了",
        "failed" => "失敗",
        "aborted" => "中断（終わりが記録されていません）",
        _ => "?",
    }
}

fn log_cmd(dir: &Path, repo: &str, count: usize, json: bool) -> i32 {
    let key = match hooks::repo_key(repo) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("エラー: {e}");
            return 2;
        }
    };
    let runs = hooks::recent_runs(dir, &key, count.clamp(1, 50));
    // 読めなかった行は、黙って捨てない（実行が、見えないまま消えたように見えるため）
    let unreadable = hooks::log_unreadable(dir);
    if unreadable > 0 {
        eprintln!("警告: 実行ログに、読めない行が {unreadable} 行あります（ほかのリポジトリの分を含みます）");
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&runs).unwrap_or_default());
        return 0;
    }
    if runs.is_empty() {
        println!("まだ実行されていません（{key}）");
        return 0;
    }
    for r in &runs {
        let commit = match (&r.commit, &r.subject) {
            (Some(c), Some(s)) => format!("コミット {c}「{s}」"),
            (Some(c), None) => format!("コミット {c}"),
            _ => "（コミットの記録なし）".to_string(),
        };
        let tail = [r.findings.map(|n| format!("指摘 {n} 件")), r.ms.map(secs)].into_iter().flatten().collect::<Vec<_>>().join("・");
        println!("{}  {commit}  [{}]{}", clock(r.started_at_ms), state_label(&r.state), if tail.is_empty() { String::new() } else { format!("  {tail}") });
        for st in &r.steps {
            if st.ok {
                println!("  [{}/{}] {:<14} 指摘 {} 件  {}", st.index, st.total, st.recipe, st.findings, secs(st.ms));
            } else {
                println!("  [{}/{}] {:<14} 失敗  {}  {}", st.index, st.total, st.recipe, secs(st.ms), st.error.as_deref().unwrap_or(""));
            }
        }
        if r.state == "running" && r.recipes.len() > r.steps.len() {
            let next = &r.recipes[r.steps.len()];
            println!("  [{}/{}] {:<14} 実行中…", r.steps.len() + 1, r.recipes.len(), next);
        }
        if let Some(m) = &r.message {
            println!("  → {m}");
        }
    }
    0
}

fn git_out(root: &str, args: &[&str]) -> String {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// レビュー結果（`--format json`）の指摘の件数。読めなければ `None`。
fn count_findings(stdout: &[u8]) -> Option<usize> {
    let v: serde_json::Value = serde_json::from_slice(stdout).ok()?;
    let results = v.get("results")?.as_array()?;
    Some(results.iter().map(|r| r.get("findings").and_then(|f| f.as_array()).map(|a| a.len()).unwrap_or(0)).sum())
}

/// レビューが失敗したとき、標準エラーから、理由を 1 行取り出す。日本語の「エラー」だけに頼らない
/// （clap の `error: unexpected argument …` や、Rust の panic は英語）。
/// 優先: ① 「エラー」を含む最後の行 ② `error`・`panicked` を含む行 ③ 最後の空でない行。何も出ていなければ、終了コードを添えて、そう言う。
fn failure_reason(stderr: &str, code: Option<i32>) -> String {
    let lines: Vec<&str> = stderr.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    let pick = lines
        .iter()
        .rev()
        .find(|l| l.contains("エラー"))
        .or_else(|| lines.iter().find(|l| l.to_lowercase().contains("error") || l.contains("panicked")))
        .or_else(|| lines.last());
    match (pick, code) {
        // 拾った行が、本当の原因とは限らない。最後の行が別なら、それも添える（標準エラー全体は、長いので、ログには入れない）
        (Some(l), _) => match lines.last() {
            Some(last) if last != l => format!("{l} ／ 最後の行: {last}"),
            _ => (*l).to_string(),
        },
        (None, Some(c)) => format!("理由は不明です（標準エラーに何も出ていません。終了コード {c}）"),
        (None, None) => "理由は不明です（シグナルで終了しました）".to_string(),
    }
}

/// レビュー 1 つの実行結果（終了コード・標準出力・標準エラー）から、(成功か, 指摘の件数, 失敗の理由) を決める。
/// 終了コード 0（指摘なし）・1（指摘あり）は成功。ただし、結果の JSON を読めなければ、件数を数えられないので、失敗にする。
fn classify(code: Option<i32>, stdout: &[u8], stderr: &str) -> (bool, usize, Option<String>) {
    match code {
        Some(0) | Some(1) => match count_findings(stdout) {
            Some(n) => (true, n, None),
            None => (false, 0, Some("レビューの結果を読めませんでした".to_string())),
        },
        other => (false, 0, Some(failure_reason(stderr, other))),
    }
}

/// 全観点の結果から、実行全体の（終了コード, メッセージ）を決める。0=指摘なし・1=指摘あり・2=失敗（一部の失敗を含む）。
fn summarize(total: usize, findings: usize, failed: &[String]) -> (i32, String) {
    let first = failed.first().cloned().unwrap_or_default();
    if !failed.is_empty() && failed.len() == total {
        (2, format!("レビューに失敗しました: {first}"))
    } else if !failed.is_empty() {
        (2, format!("一部の観点が失敗しました（{}/{}）: {first}。指摘は {findings} 件", failed.len(), total))
    } else if findings > 0 {
        (1, format!("レビュー完了（指摘あり {findings} 件）"))
    } else {
        (0, "レビュー完了（指摘なし）".to_string())
    }
}

/// 実行ログを書けなかったときは、握りつぶさず、標準エラーに出す
/// （フックの中では見えないが、手で走らせたとき・`--verbose` のときに分かる。画面に何も出ない原因は、たいていこれ）。
fn warn_log(r: Result<(), String>) {
    if let Err(e) = r {
        eprintln!("警告: 実行ログを書けません: {e}");
    }
}

/// フックの本体。失敗は、握りつぶさず `hooks.log` に残す（コミットは止めないので、終了コードは 0）。
fn run_hook(dir: &Path, name: &str, verbose: bool, mock: bool) -> i32 {
    if name != hooks::HOOK_NAME {
        eprintln!("エラー: 未対応のフックです: {name}（対応: {}）", hooks::HOOK_NAME);
        return 2;
    }
    let say = |text: String| {
        if verbose {
            eprintln!("{text}");
        }
    };
    let cwd = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| ".".into());
    let key = match hooks::repo_key(&cwd) {
        Ok(k) => k,
        Err(e) => {
            warn_log(hooks::append_log(dir, &cwd, 2, &format!("リポジトリを特定できません: {e}")));
            say(format!("リポジトリを特定できません: {e}"));
            return 0;
        }
    };
    let cfg = match hooks::load(dir) {
        Ok(c) => c,
        Err(e) => {
            warn_log(hooks::append_log(dir, &key, 2, &e));
            say(e);
            return 0;
        }
    };
    let Some(entry) = cfg.post_commit.get(&key).filter(|e| e.enabled) else {
        say("このリポジトリは、フックが OFF（または設定がありません）。何もしません".into());
        return 0;
    };
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            warn_log(hooks::append_log(dir, &key, 2, &format!("自分の実行ファイルを特定できません: {e}")));
            return 0;
        }
    };

    // 開始時のコミットを固定する（観点の実行中に次のコミットが入っても、同じコミットを見続ける）
    let full = git_out(&key, &["rev-parse", "HEAD"]);
    let short = git_out(&key, &["rev-parse", "--short", "HEAD"]);
    let subject = git_out(&key, &["log", "-1", "--format=%s"]);
    if full.is_empty() {
        warn_log(hooks::append_log(dir, &key, 2, "コミットを特定できません（HEAD が無い）"));
        return 0;
    }
    let run = hooks::new_run_id();
    let started = Instant::now();
    let total = entry.recipes.len();
    warn_log(hooks::log_start(dir, &key, &run, &short, &subject, &entry.recipes));
    say(format!("コミット {short}「{subject}」を、{total} 個の観点でレビューします"));

    let mut findings_total = 0usize;
    let mut failed: Vec<String> = Vec::new();
    for (i, recipe) in entry.recipes.iter().enumerate() {
        let t = Instant::now();
        say(format!("[{}/{}] {recipe} を実行中…", i + 1, total));
        let mut cmd = Command::new(&exe);
        cmd.args(["review", "--commit", &full, "--format", "json", "--stdout", "-C", &key, "-r", recipe]);
        for sp in entry.spec_list() {
            cmd.args(["--spec", &sp]);
        }
        if mock {
            cmd.arg("--mock");
        }
        let out = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output();
        let ms = t.elapsed().as_millis() as u64;
        let (ok, count, error) = match out {
            Ok(o) => classify(o.status.code(), &o.stdout, &String::from_utf8_lossy(&o.stderr)),
            Err(e) => (false, 0, Some(format!("レビューを起動できません: {e}"))),
        };
        warn_log(hooks::log_step(dir, &key, &run, recipe, i + 1, total, ok, count, ms, error.as_deref()));
        if ok {
            findings_total += count;
            say(format!("[{}/{}] {recipe}: 指摘 {count} 件（{}）", i + 1, total, secs(ms)));
        } else {
            let e = error.unwrap_or_default();
            say(format!("[{}/{}] {recipe}: 失敗（{}）{e}", i + 1, total, secs(ms)));
            failed.push(e);
        }
    }

    let ms = started.elapsed().as_millis() as u64;
    let (code, message) = summarize(total, findings_total, &failed);
    warn_log(hooks::log_end(dir, &key, &run, code, &message, findings_total, ms));
    say(format!("{message}（{}）", secs(ms)));
    0
}

#[cfg(test)]
mod tests {
    use super::{classify, count_findings, failure_reason, summarize};

    #[test]
    fn a_japanese_error_line_wins_and_the_latest_one_is_used() {
        let e = "レビュー開始: 1 ファイル\nエラー: 古い\n[1/1] レビュー中\nエラー (観点: bugs): LLM の呼び出しに失敗しました: 接続できません\n";
        assert_eq!(failure_reason(e, Some(2)), "エラー (観点: bugs): LLM の呼び出しに失敗しました: 接続できません");
    }

    #[test]
    fn an_english_clap_error_is_used_and_the_last_line_is_added_when_it_differs() {
        let e = "error: unexpected argument '--provider' found\n\nUsage: mrcr review --commit [<REV>]\n\nFor more information, try '--help'.\n";
        assert_eq!(failure_reason(e, Some(2)), "error: unexpected argument '--provider' found ／ 最後の行: For more information, try '--help'.");
        assert!(failure_reason("thread 'main' panicked at src/x.rs:1:1:\nboom", Some(101)).contains("panicked"));
    }

    #[test]
    fn without_any_error_word_the_last_line_is_used_and_nothing_at_all_says_so_with_the_code() {
        assert_eq!(failure_reason("start\nsomething odd happened\n", Some(3)), "something odd happened");
        assert!(failure_reason("", Some(3)).contains("終了コード 3"));
        assert!(failure_reason("  \n\n", None).contains("シグナル"));
    }

    #[test]
    fn the_japanese_error_that_is_also_the_last_line_is_not_repeated() {
        assert_eq!(failure_reason("開始\nエラー: 失敗しました\n", Some(2)), "エラー: 失敗しました");
    }

    const ONE_FINDING: &[u8] = br#"{"results":[{"recipe_id":"bugs","findings":[{"title":"x"},{"title":"y"}]},{"recipe_id":"errors","findings":[{"title":"z"}]}]}"#;

    #[test]
    fn findings_are_counted_across_recipes_and_unreadable_output_is_none() {
        assert_eq!(count_findings(ONE_FINDING), Some(3));
        assert_eq!(count_findings(br#"{"results":[]}"#), Some(0));
        assert_eq!(count_findings(b""), None);
        assert_eq!(count_findings(b"not json"), None);
        assert_eq!(count_findings(br#"{"other":1}"#), None);
    }

    #[test]
    fn exit_codes_0_and_1_with_readable_output_succeed_with_the_count() {
        assert_eq!(classify(Some(1), ONE_FINDING, ""), (true, 3, None));
        assert_eq!(classify(Some(0), br#"{"results":[]}"#, ""), (true, 0, None));
    }

    #[test]
    fn exit_code_0_or_1_with_broken_output_is_a_failure_not_zero_findings() {
        for code in [0, 1] {
            let (ok, n, err) = classify(Some(code), b"{ broken", "");
            assert!(!ok && n == 0, "code={code}");
            assert!(err.unwrap().contains("結果を読めませんでした"));
        }
        assert!(!classify(Some(0), b"", "").0, "出力が空でも、成功にしない");
    }

    #[test]
    fn other_exit_codes_and_signals_are_failures_with_a_reason() {
        let (ok, _, err) = classify(Some(2), b"", "エラー: LLM の呼び出しに失敗しました\n");
        assert!(!ok);
        assert_eq!(err.as_deref(), Some("エラー: LLM の呼び出しに失敗しました"));
        let (ok, _, err) = classify(None, b"", "");
        assert!(!ok && err.unwrap().contains("シグナル"));
        assert!(!classify(Some(101), ONE_FINDING, "thread 'main' panicked").0, "終了コードが 0/1 以外なら、出力が読めても、失敗");
    }

    #[test]
    fn summarize_covers_none_found_all_failed_and_partial_failure() {
        assert_eq!(summarize(2, 0, &[]), (0, "レビュー完了（指摘なし）".to_string()));
        assert_eq!(summarize(2, 4, &[]), (1, "レビュー完了（指摘あり 4 件）".to_string()));
        let all = summarize(2, 0, &["a".to_string(), "b".to_string()]);
        assert_eq!(all.0, 2);
        assert!(all.1.starts_with("レビューに失敗しました: a"), "{}", all.1);
        let partial = summarize(2, 3, &["理由".to_string()]);
        assert_eq!(partial.0, 2, "一部の失敗でも、終了コードは 2");
        assert!(partial.1.contains("一部の観点が失敗しました（1/2）") && partial.1.contains("理由") && partial.1.contains("指摘は 3 件"), "{}", partial.1);
    }
}
