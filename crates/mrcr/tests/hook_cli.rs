use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

struct Tmp(PathBuf);
static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl Tmp {
    fn new(tag: &str) -> Tmp {
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("mrcr-hook-cli-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Tmp(d.canonicalize().unwrap())
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
}

/// 2 つのコミットがあるリポジトリ（2 つ目で a.txt を変えた）。
fn setup_repo() -> (Tmp, Tmp) {
    let repo = Tmp::new("repo");
    let data = Tmp::new("data");
    git(&repo.0, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.0.join("a.txt"), "hello\n").unwrap();
    git(&repo.0, &["add", "."]);
    git(&repo.0, &["commit", "-q", "-m", "init"]);
    std::fs::write(repo.0.join("a.txt"), "hello world\n").unwrap();
    git(&repo.0, &["commit", "-q", "-am", "change a"]);
    (repo, data)
}

fn mrcr(repo: &Tmp, data: &Tmp, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(args)
        .env("MRCR_DATA_DIR", &data.0)
        .env("MRCR_CLI", env!("CARGO_BIN_EXE_mrcr"))
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("MRCR_MOCK_RESPONSE")
        .current_dir(&repo.0)
        .output()
        .unwrap()
}

fn status_json(repo: &Tmp, data: &Tmp) -> serde_json::Value {
    let o = mrcr(repo, data, &["hook", "status", "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}

fn wait_for_log(data: &Tmp) -> String {
    let p = data.0.join("hooks.log");
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(20) {
        if let Ok(s) = std::fs::read_to_string(&p) {
            // 開始の行だけでは、まだ終わっていない。終わりの行（code がある行）を待つ
            if s.contains("\"code\":") {
                return s;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("hooks.log が 20 秒以内にできなかった");
}

#[test]
fn review_commit_looks_only_at_the_last_commit_not_at_uncommitted_or_untracked_files() {
    let (repo, _data) = setup_repo();
    std::fs::write(repo.0.join("a.txt"), "uncommitted edit\n").unwrap();
    std::fs::write(repo.0.join("untracked.txt"), "new\n").unwrap();

    let snap = mrcr_core::snapshot::take_last_commit(repo.0.to_str().unwrap(), &mrcr_core::snapshot::Limits::default()).unwrap();
    let names: Vec<&str> = snap.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(names, vec!["a.txt"], "直近のコミットで変わったファイルだけ");
    assert!(snap.diff_text.contains("+hello world") && !snap.diff_text.contains("uncommitted edit"), "{}", snap.diff_text);
}

#[test]
fn review_commit_works_for_the_first_commit_too() {
    let repo = Tmp::new("first");
    git(&repo.0, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.0.join("a.txt"), "hello\n").unwrap();
    git(&repo.0, &["add", "."]);
    git(&repo.0, &["commit", "-q", "-m", "init"]);
    let snap = mrcr_core::snapshot::take_last_commit(repo.0.to_str().unwrap(), &mrcr_core::snapshot::Limits::default()).unwrap();
    assert_eq!(snap.files.len(), 1, "最初のコミットは、何も無い状態との差分");
}

#[test]
fn review_commit_conflicts_with_base() {
    let (repo, data) = setup_repo();
    let o = mrcr(&repo, &data, &["review", "--commit", "--base", "HEAD~1", "--mock", "--stdout"]);
    assert_eq!(o.status.code(), Some(2), "clap の使い方のエラー");
}

#[test]
fn enable_installs_the_hook_and_status_and_disable_follow() {
    let (repo, data) = setup_repo();
    let o = mrcr(&repo, &data, &["hook", "enable", "-r", "bugs,errors"]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));

    let hook = repo.0.join(".git/hooks/post-commit");
    let text = std::fs::read_to_string(&hook).unwrap();
    assert!(text.contains("hook run post-commit"), "{text}");

    let s = status_json(&repo, &data);
    assert_eq!((s["enabled"].as_bool(), s["installed"].as_bool()), (Some(true), Some(true)), "{s}");
    assert_eq!(s["recipes"], serde_json::json!(["bugs", "errors"]));

    let o = mrcr(&repo, &data, &["hook", "disable"]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(!hook.exists());
    let s = status_json(&repo, &data);
    assert_eq!((s["enabled"].as_bool(), s["installed"].as_bool()), (Some(false), Some(false)));
    assert_eq!(s["recipes"], serde_json::json!(["bugs", "errors"]), "OFF でも観点は残る");
}

#[test]
fn enable_without_the_cli_says_so_and_changes_nothing() {
    let (repo, data) = setup_repo();
    let o = Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(["hook", "enable"])
        .env("MRCR_DATA_DIR", &data.0)
        .env("MRCR_CLI", "/no/such/mrcr")
        .current_dir(&repo.0)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("mrcr（CLI）が見つかりません"), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(!repo.0.join(".git/hooks/post-commit").exists());
    assert!(!data.0.join("hooks.json").exists());
}

#[test]
fn enable_keeps_an_existing_hook_and_disable_restores_it() {
    let (repo, data) = setup_repo();
    let hook = repo.0.join(".git/hooks/post-commit");
    std::fs::write(&hook, "#!/bin/sh\necho mine\n").unwrap();
    assert_eq!(mrcr(&repo, &data, &["hook", "enable"]).status.code(), Some(0));
    let t = std::fs::read_to_string(&hook).unwrap();
    assert!(t.contains("echo mine") && t.contains("hook run post-commit"), "{t}");
    assert_eq!(mrcr(&repo, &data, &["hook", "disable"]).status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(&hook).unwrap(), "#!/bin/sh\necho mine\n");
}

#[test]
fn run_does_nothing_when_off_and_never_fails_the_commit() {
    let (repo, data) = setup_repo();
    let o = mrcr(&repo, &data, &["hook", "run", "post-commit", "--mock"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(!data.0.join("hooks.log").exists(), "OFF のときは、ログも履歴も作らない");
    assert!(mrcr_core::store::load_all(&data.0).is_empty());
}

#[test]
fn run_when_on_reviews_the_commit_saves_history_and_logs() {
    let (repo, data) = setup_repo();
    assert_eq!(mrcr(&repo, &data, &["hook", "enable"]).status.code(), Some(0));

    let o = mrcr(&repo, &data, &["hook", "run", "post-commit", "--mock"]);
    assert_eq!(o.status.code(), Some(0), "コミットは止めない: {}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(mrcr_core::store::load_all(&data.0).len(), 1, "通常のレビュー履歴に入る（GUI で見られる）");

    let s = status_json(&repo, &data);
    assert_eq!(s["last_run"]["code"], 1, "モックは指摘ありを返す: {s}");
    assert!(s["last_run"]["message"].as_str().unwrap().contains("指摘あり"));
}

#[test]
fn a_real_commit_triggers_the_hook_and_a_failure_is_logged_not_swallowed() {
    let (repo, data) = setup_repo();
    assert_eq!(mrcr(&repo, &data, &["hook", "enable"]).status.code(), Some(0));

    // git のフックの環境にも、データの置き場と mrcr の場所だけを渡す。API キーは、無い（失敗させる）
    std::fs::write(repo.0.join("b.txt"), "x\n").unwrap();
    git(&repo.0, &["add", "."]);
    let o = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "add b"])
        .env("MRCR_DATA_DIR", &data.0)
        .env_remove("ANTHROPIC_API_KEY")
        .current_dir(&repo.0)
        .output()
        .unwrap();
    assert!(o.status.success(), "フックが失敗しても、コミットは成功する: {}", String::from_utf8_lossy(&o.stderr));

    let log = wait_for_log(&data);
    assert!(log.contains("\"code\":2") && log.contains("API キー"), "失敗の理由が、ログに残る: {log}");
    let s = status_json(&repo, &data);
    assert_eq!(s["last_run"]["code"], 2, "{s}");
}

fn add_spec(repo: &Tmp) {
    std::fs::create_dir_all(repo.0.join("docs")).unwrap();
    std::fs::write(repo.0.join("docs/spec.md"), "# 仕様\n- 平均を返す\n").unwrap();
}

#[test]
fn a_recipe_that_needs_a_spec_is_refused_without_one_and_accepted_with_one() {
    let (repo, data) = setup_repo();
    add_spec(&repo);
    let o = mrcr(&repo, &data, &["hook", "enable", "-r", "spec"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("設計書が要ります"), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(!repo.0.join(".git/hooks/post-commit").exists(), "断ったとき、フックは入れない");

    let o = mrcr(&repo, &data, &["hook", "enable", "-r", "spec", "--spec", "docs/spec.md"]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let s = status_json(&repo, &data);
    assert_eq!(s["spec"], "docs/spec.md");
}

#[test]
fn a_spec_outside_the_repo_is_refused() {
    let (repo, data) = setup_repo();
    let o = mrcr(&repo, &data, &["hook", "enable", "-r", "bugs", "--spec", "../outside.md"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains(".."), "{}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn run_passes_the_spec_to_the_review_and_the_history_keeps_it() {
    let (repo, data) = setup_repo();
    add_spec(&repo);
    assert_eq!(mrcr(&repo, &data, &["hook", "enable", "-r", "spec", "--spec", "docs/spec.md"]).status.code(), Some(0));
    let o = mrcr(&repo, &data, &["hook", "run", "post-commit", "--mock"]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let records = mrcr_core::store::load_all(&data.0);
    assert_eq!(records.len(), 1);
    assert!(records[0].spec.is_some(), "設計書が、レビューに渡されている");
    assert_eq!(status_json(&repo, &data)["last_run"]["code"], 1);
}

#[test]
fn disable_keeps_the_spec_setting() {
    let (repo, data) = setup_repo();
    add_spec(&repo);
    assert_eq!(mrcr(&repo, &data, &["hook", "enable", "-r", "spec", "--spec", "docs/spec.md"]).status.code(), Some(0));
    assert_eq!(mrcr(&repo, &data, &["hook", "disable"]).status.code(), Some(0));
    let s = status_json(&repo, &data);
    assert_eq!((s["enabled"].as_bool(), s["spec"].as_str()), (Some(false), Some("docs/spec.md")));
}

fn log_json(repo: &Tmp, data: &Tmp) -> serde_json::Value {
    let o = mrcr(repo, data, &["hook", "log", "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}

#[test]
fn run_records_the_commit_each_recipe_and_the_result_in_the_log() {
    let (repo, data) = setup_repo();
    assert_eq!(mrcr(&repo, &data, &["hook", "enable", "-r", "bugs,errors"]).status.code(), Some(0));
    let o = mrcr(&repo, &data, &["hook", "run", "post-commit", "--mock"]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));

    assert_eq!(mrcr_core::store::load_all(&data.0).len(), 2, "観点ごとに、通常の履歴に入る");
    let runs = log_json(&repo, &data);
    assert_eq!(runs.as_array().unwrap().len(), 1);
    let r = &runs[0];
    assert_eq!(r["subject"], "change a", "どのコミットか");
    assert_eq!(r["commit"].as_str().unwrap().len() >= 7, true);
    assert_eq!(r["recipes"], serde_json::json!(["bugs", "errors"]));
    let steps = r["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    assert_eq!((steps[0]["recipe"].as_str(), steps[0]["index"].as_u64(), steps[0]["total"].as_u64(), steps[0]["ok"].as_bool()), (Some("bugs"), Some(1), Some(2), Some(true)));
    assert_eq!(steps[1]["recipe"], "errors");
    assert_eq!(steps[0]["findings"], 2, "モックは 2 件を返す");
    assert_eq!((r["state"].as_str(), r["code"].as_i64(), r["findings"].as_u64()), (Some("done"), Some(1), Some(4)));
    assert!(r["message"].as_str().unwrap().contains("指摘あり 4 件"));
}

#[test]
fn hook_log_prints_a_readable_summary_and_says_so_when_nothing_ran() {
    let (repo, data) = setup_repo();
    let none = mrcr(&repo, &data, &["hook", "log"]);
    assert_eq!(none.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&none.stdout).contains("まだ実行されていません"));

    assert_eq!(mrcr(&repo, &data, &["hook", "enable", "-r", "bugs,errors"]).status.code(), Some(0));
    assert_eq!(mrcr(&repo, &data, &["hook", "run", "post-commit", "--mock"]).status.code(), Some(0));
    let o = mrcr(&repo, &data, &["hook", "log"]);
    let t = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(t.contains("「change a」") && t.contains("[完了]"), "{t}");
    assert!(t.contains("[1/2] bugs") && t.contains("[2/2] errors") && t.contains("指摘 2 件"), "{t}");
    assert!(t.contains("→ レビュー完了（指摘あり 4 件）"), "{t}");
}

#[test]
fn verbose_prints_progress_to_stderr_and_the_log_is_written_either_way() {
    let (repo, data) = setup_repo();
    assert_eq!(mrcr(&repo, &data, &["hook", "enable", "-r", "bugs,errors"]).status.code(), Some(0));
    let o = mrcr(&repo, &data, &["hook", "run", "post-commit", "--mock", "--verbose"]);
    assert_eq!(o.status.code(), Some(0));
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(e.contains("「change a」") && e.contains("[1/2] bugs を実行中") && e.contains("[2/2] errors: 指摘 2 件"), "{e}");
    assert!(e.contains("レビュー完了（指摘あり 4 件）"), "{e}");
    assert_eq!(log_json(&repo, &data)[0]["steps"].as_array().unwrap().len(), 2);
}

#[test]
fn a_failing_recipe_does_not_stop_the_others_and_each_failure_is_logged() {
    let (repo, data) = setup_repo();
    assert_eq!(mrcr(&repo, &data, &["hook", "enable", "-r", "bugs,errors"]).status.code(), Some(0));
    let o = Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(["hook", "run", "post-commit", "--mock"])
        .env("MRCR_DATA_DIR", &data.0)
        .env("MRCR_MOCK_RESPONSE", "これは JSON ではありません")
        .current_dir(&repo.0)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "コミットは止めない");
    let r = &log_json(&repo, &data)[0];
    let steps = r["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2, "1 つ目が失敗しても、2 つ目まで走る");
    assert!(steps.iter().all(|s| s["ok"] == false && s["error"].as_str().unwrap().contains("指摘を読めませんでした")), "{steps:?}");
    assert_eq!((r["state"].as_str(), r["code"].as_i64()), (Some("failed"), Some(2)));
    assert!(r["message"].as_str().unwrap().starts_with("レビューに失敗しました"));
}

#[test]
fn every_recipe_of_a_run_reviews_the_same_commit_even_if_head_moves() {
    let (repo, _data) = setup_repo();
    let head = |args: &[&str]| {
        let o = Command::new("git").args(args).current_dir(&repo.0).output().unwrap();
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    let second = head(&["rev-parse", "HEAD"]);
    std::fs::write(repo.0.join("b.txt"), "next\n").unwrap();
    git(&repo.0, &["add", "."]);
    git(&repo.0, &["commit", "-q", "-m", "third"]);

    let limits = mrcr_core::snapshot::Limits::default();
    let at_second = mrcr_core::snapshot::take_commit(repo.0.to_str().unwrap(), &limits, &second).unwrap();
    assert_eq!(at_second.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), vec!["a.txt"], "HEAD が進んでも、指定したコミットを見る");
    assert_eq!(at_second.head, second);
    let at_head = mrcr_core::snapshot::take_commit(repo.0.to_str().unwrap(), &limits, "HEAD").unwrap();
    assert_eq!(at_head.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), vec!["b.txt"]);
    for bad in ["", "-x", "a b", "no-such-commit"] {
        assert!(mrcr_core::snapshot::take_commit(repo.0.to_str().unwrap(), &limits, bad).is_err(), "{bad:?}");
    }
}

#[test]
fn a_broken_hooks_json_is_logged_as_a_failure_and_the_run_still_exits_0() {
    let (repo, data) = setup_repo();
    std::fs::write(data.0.join("hooks.json"), "{ nope").unwrap();
    let o = mrcr(&repo, &data, &["hook", "run", "post-commit", "--mock"]);
    assert_eq!(o.status.code(), Some(0), "コミットは止めない: {}", String::from_utf8_lossy(&o.stderr));
    let log = std::fs::read_to_string(data.0.join("hooks.log")).expect("失敗が、ログに残る");
    assert!(log.contains("\"code\":2") && log.contains("正しくありません"), "{log}");
    assert!(mrcr_core::store::load_all(&data.0).is_empty(), "レビューは、走らない");
}

#[test]
fn running_outside_a_git_repository_is_logged_as_a_failure_and_exits_0() {
    let (_repo, data) = setup_repo();
    let outside = Tmp::new("outside");
    let o = Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(["hook", "run", "post-commit", "--mock"])
        .env("MRCR_DATA_DIR", &data.0)
        .current_dir(&outside.0)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0));
    let log = std::fs::read_to_string(data.0.join("hooks.log")).expect("失敗が、ログに残る");
    assert!(log.contains("\"code\":2") && log.contains("リポジトリを特定できません"), "{log}");
}

#[test]
fn a_log_that_cannot_be_written_is_reported_on_stderr_not_swallowed() {
    let (repo, data) = setup_repo();
    assert_eq!(mrcr(&repo, &data, &["hook", "enable"]).status.code(), Some(0));
    // ログの場所を、フォルダでふさぐ → 書き込みが、すべて失敗する
    std::fs::create_dir_all(data.0.join("hooks.log")).unwrap();
    let o = mrcr(&repo, &data, &["hook", "run", "post-commit", "--mock"]);
    assert_eq!(o.status.code(), Some(0), "ログが書けなくても、コミットは止めない");
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(e.contains("警告: 実行ログを書けません"), "{e}");
    assert_eq!(mrcr_core::store::load_all(&data.0).len(), 1, "レビュー自体は、走って、履歴に入る");
}

#[test]
fn take_commit_refuses_option_like_values_without_creating_files_and_accepts_tags_and_branches() {
    let (repo, data) = setup_repo();
    let limits = mrcr_core::snapshot::Limits::default();
    let repo_s = repo.0.to_str().unwrap();

    let target = data.0.join("x");
    let evil = format!("--output={}", target.display());
    assert!(mrcr_core::snapshot::take_commit(repo_s, &limits, &evil).is_err());
    assert!(!target.exists(), "git のオプションとして解釈されて、ファイルが作られてはならない");
    assert!(mrcr_core::snapshot::take_commit(repo_s, &limits, "--help").is_err());

    // タグ・ブランチの名前は、使える（2 つ目のコミットを指す）
    git(&repo.0, &["tag", "v1"]);
    git(&repo.0, &["branch", "topic"]);
    std::fs::write(repo.0.join("b.txt"), "next\n").unwrap();
    git(&repo.0, &["add", "."]);
    git(&repo.0, &["commit", "-q", "-m", "third"]);
    for name in ["v1", "topic"] {
        let s = mrcr_core::snapshot::take_commit(repo_s, &limits, name).unwrap();
        assert_eq!(s.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), vec!["a.txt"], "{name}");
    }
    let o = mrcr(&repo, &data, &["review", "--commit", "v1", "--mock", "--stdout"]);
    assert_eq!(o.status.code(), Some(1), "CLI の --commit にも、タグが使える: {}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn when_only_one_recipe_fails_the_run_is_recorded_as_failed_and_keeps_the_successful_count() {
    let (repo, data) = setup_repo();
    // 設計書が要る観点（spec）を、設計書の指定なしで混ぜる → bugs は成功、spec は「設計書が必要です」で失敗する
    let key = repo.0.to_string_lossy().to_string();
    let cfg = serde_json::json!({ "post_commit": { key: { "enabled": true, "recipes": ["bugs", "spec"] } } });
    std::fs::write(data.0.join("hooks.json"), cfg.to_string()).unwrap();

    let o = mrcr(&repo, &data, &["hook", "run", "post-commit", "--mock"]);
    assert_eq!(o.status.code(), Some(0), "コミットは止めない: {}", String::from_utf8_lossy(&o.stderr));

    let runs = log_json(&repo, &data);
    let r = &runs[0];
    let steps = r["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2, "1 つ失敗しても、残りまで走る");
    assert_eq!((steps[0]["recipe"].as_str(), steps[0]["ok"].as_bool(), steps[0]["findings"].as_u64()), (Some("bugs"), Some(true), Some(2)));
    assert_eq!((steps[1]["recipe"].as_str(), steps[1]["ok"].as_bool()), (Some("spec"), Some(false)));
    assert!(steps[1]["error"].as_str().unwrap().contains("設計書"), "失敗の理由が出る: {:?}", steps[1]["error"]);
    assert_eq!((r["state"].as_str(), r["code"].as_i64(), r["findings"].as_u64()), (Some("failed"), Some(2), Some(2)), "全体は失敗。成功した分の件数は保たれる");
    let m = r["message"].as_str().unwrap();
    assert!(m.contains("一部の観点が失敗しました（1/2）") && m.contains("指摘は 2 件"), "{m}");
    assert_eq!(mrcr_core::store::load_all(&data.0).len(), 1, "成功した観点だけが、履歴に入る");
}

#[test]
fn unreadable_log_lines_are_warned_about_by_log_and_status() {
    let (repo, data) = setup_repo();
    assert_eq!(mrcr(&repo, &data, &["hook", "enable"]).status.code(), Some(0));
    assert_eq!(mrcr(&repo, &data, &["hook", "run", "post-commit", "--mock"]).status.code(), Some(0));
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().append(true).open(data.0.join("hooks.log")).unwrap();
    writeln!(f, "壊れた行").unwrap();
    drop(f);

    let o = mrcr(&repo, &data, &["hook", "log"]);
    assert!(String::from_utf8_lossy(&o.stderr).contains("読めない行が 1 行"), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stdout).contains("レビュー完了"), "読める行は、これまでどおり出す");
    let st = mrcr(&repo, &data, &["hook", "status"]);
    assert!(String::from_utf8_lossy(&st.stdout).contains("読めない行が 1 行"), "{}", String::from_utf8_lossy(&st.stdout));
    assert_eq!(status_json(&repo, &data)["log_unreadable"], 1);
}
