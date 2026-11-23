use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Tmp(PathBuf);
static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl Tmp {
    fn new(tag: &str) -> Tmp {
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("mrcr-parse-cli-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Tmp(d)
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

fn setup_repo() -> (Tmp, Tmp) {
    let repo = Tmp::new("repo");
    let data = Tmp::new("data");
    git(&repo.0, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.0.join("a.txt"), "hello\n").unwrap();
    git(&repo.0, &["add", "."]);
    git(&repo.0, &["commit", "-q", "-m", "init"]);
    std::fs::write(repo.0.join("a.txt"), "hello world\n").unwrap();
    (repo, data)
}

fn review_with_mock(repo: &Tmp, data: &Tmp, mock_response: Option<&str>, extra: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_mrcr"));
    cmd.args(["review", "--mock", "--stdout"])
        .args(extra)
        .env("MRCR_DATA_DIR", &data.0)
        .env_remove("MRCR_MOCK_RESPONSE")
        .current_dir(&repo.0);
    if let Some(r) = mock_response {
        cmd.env("MRCR_MOCK_RESPONSE", r);
    }
    cmd.output().unwrap()
}

#[test]
fn unparseable_llm_response_exits_with_code_2_and_says_why() {
    let (repo, data) = setup_repo();
    let o = review_with_mock(&repo, &data, Some("これは JSON ではありません"), &[]);

    assert_eq!(o.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&o.stderr));
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(stderr.contains("指摘を読めませんでした"), "{stderr}");
    assert!(stderr.contains("bugs"), "観点の名前が出る: {stderr}");
}

#[test]
fn unparseable_llm_response_exits_with_code_2_even_without_saving_history() {
    let (repo, data) = setup_repo();
    let o = review_with_mock(&repo, &data, Some("{\"findings\": \"配列ではない\"}"), &["--no-save"]);

    assert_eq!(o.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn empty_response_exits_with_code_2() {
    let (repo, data) = setup_repo();
    let o = review_with_mock(&repo, &data, Some(""), &[]);

    assert_eq!(o.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stderr).contains("指摘を読めませんでした"));
}

#[test]
fn truncated_json_response_exits_with_code_2() {
    let (repo, data) = setup_repo();
    let o = review_with_mock(&repo, &data, Some("{\"findings\": ["), &[]);

    assert_eq!(o.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stderr).contains("指摘を読めませんでした"));
}

#[test]
fn valid_response_with_zero_findings_exits_with_code_0() {
    let (repo, data) = setup_repo();
    let o = review_with_mock(&repo, &data, Some("{\"findings\": []}"), &[]);

    assert_eq!(o.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn valid_response_with_findings_exits_with_code_1() {
    let (repo, data) = setup_repo();
    let o = review_with_mock(&repo, &data, None, &[]);

    assert_eq!(o.status.code(), Some(1), "stderr: {}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn second_recipe_unparseable_exits_with_code_2_names_it_and_prints_no_report() {
    let (repo, data) = setup_repo();
    let o = Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(["review", "--mock", "--stdout", "--recipe", "bugs,errors"])
        .env("MRCR_DATA_DIR", &data.0)
        .env_remove("MRCR_MOCK_RESPONSE")
        .env("MRCR_MOCK_RESPONSE_1", "{\"findings\": []}")
        .env("MRCR_MOCK_RESPONSE_2", "これは JSON ではありません")
        .current_dir(&repo.0)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(2), "stderr: {stderr}");
    assert!(stderr.contains("観点: errors"), "読めなかった 2 つ目の観点名が出る: {stderr}");
    assert!(!stderr.contains("観点: bugs)"), "1 つ目の観点は失敗として出さない: {stderr}");
    assert!(String::from_utf8_lossy(&o.stdout).trim().is_empty(), "レポートは出さない: {}", String::from_utf8_lossy(&o.stdout));
}

#[test]
fn unparseable_response_is_kept_in_history_with_its_parse_error() {
    let (repo, data) = setup_repo();
    let o = review_with_mock(&repo, &data, Some("これは JSON ではありません"), &[]);
    assert_eq!(o.status.code(), Some(2));

    let records = mrcr_core::store::load_all(&data.0);
    assert_eq!(records.len(), 1, "壊れた返事も履歴に 1 件残る");
    assert!(records[0].parse_error.is_some(), "読めなかった理由が履歴に残る");
    assert!(records[0].findings.is_empty());
}
