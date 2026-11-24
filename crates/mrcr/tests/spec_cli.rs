use std::path::{Path, PathBuf};
use std::process::Command;

struct Tmp(PathBuf);
static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl Tmp {
    fn new(tag: &str) -> Tmp {
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("mrcr-spec-cli-{tag}-{}-{n}", std::process::id()));
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

#[test]
fn spec_recipe_without_spec_argument_exits_with_code_2() {
    let (repo, data) = setup_repo();
    let o = Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(["review", "--recipe", "spec"])
        .env("MRCR_DATA_DIR", &data.0)
        .env("ANTHROPIC_API_KEY", "sk-ant-test-dummy")
        .current_dir(&repo.0)
        .output()
        .unwrap();

    assert_eq!(o.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(stderr.contains("設計書（基準の文書）が必要です"), "{stderr}");
}

#[test]
fn spec_with_path_traversal_is_rejected_with_code_2() {
    let (repo, data) = setup_repo();
    let o = Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(["review", "--recipe", "spec", "--spec", "../outside.md"])
        .env("MRCR_DATA_DIR", &data.0)
        .env("ANTHROPIC_API_KEY", "sk-ant-test-dummy")
        .current_dir(&repo.0)
        .output()
        .unwrap();

    assert_eq!(o.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(stderr.contains("..") || stderr.contains("リポジトリ外"), "{stderr}");
}

#[test]
fn spec_with_unsupported_extension_is_rejected_with_code_2() {
    let (repo, data) = setup_repo();
    let docs = repo.0.join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("spec.rs"), "pub fn spec() {}\n").unwrap();

    let o = Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(["review", "--recipe", "spec", "--spec", "docs/spec.rs"])
        .env("MRCR_DATA_DIR", &data.0)
        .env("ANTHROPIC_API_KEY", "sk-ant-test-dummy")
        .current_dir(&repo.0)
        .output()
        .unwrap();

    assert_eq!(o.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(stderr.contains(".md または .txt のみ対応"), "{stderr}");
}

#[test]
fn file_mode_rejects_code_recipes_and_all_flag() {
    let repo = Tmp::new("file-recipe-repo");
    let data = Tmp::new("file-recipe-data");
    let md = repo.0.join("doc.md");
    let txt = repo.0.join("note.txt");
    std::fs::write(&md, "# Title\nDoc content\n").unwrap();
    std::fs::write(&txt, "Plain text content\n").unwrap();

    // 1. bugs は拒否（終了コード 2）
    let o1 = Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(["review", md.to_str().unwrap(), "-r", "bugs"])
        .env("MRCR_DATA_DIR", &data.0)
        .output()
        .unwrap();
    assert_eq!(o1.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o1.stderr).contains("文書レビュー観点"));

    // 2. --all は拒否（終了コード 2）
    let o2 = Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(["review", md.to_str().unwrap(), "--all"])
        .env("MRCR_DATA_DIR", &data.0)
        .output()
        .unwrap();
    assert_eq!(o2.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o2.stderr).contains("--all"));

    // 3. 一部だけ文書以外（doc-core,bugs）も拒否（終了コード 2）
    let o3 = Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(["review", md.to_str().unwrap(), "-r", "doc-core,bugs"])
        .env("MRCR_DATA_DIR", &data.0)
        .output()
        .unwrap();
    assert_eq!(o3.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o3.stderr).contains("文書レビュー観点"));

    // 4. .txt でも同様に拒否
    let o4 = Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(["review", txt.to_str().unwrap(), "-r", "bugs"])
        .env("MRCR_DATA_DIR", &data.0)
        .output()
        .unwrap();
    assert_eq!(o4.status.code(), Some(2));
}
