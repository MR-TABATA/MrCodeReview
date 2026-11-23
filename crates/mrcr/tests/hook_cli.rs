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
            if !s.trim().is_empty() {
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
