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
