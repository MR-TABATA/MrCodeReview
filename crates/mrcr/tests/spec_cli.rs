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

fn write_docs(repo: &Tmp, files: &[(&str, &str)]) {
    for (name, text) in files {
        let f = repo.0.join(name);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, text).unwrap();
    }
}

fn review(repo: &Tmp, data: &Tmp, extra: &[&str]) -> std::process::Output {
    let mut a = vec!["review", "--recipe", "spec", "--mock"];
    a.extend_from_slice(extra);
    Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(a)
        .env("MRCR_DATA_DIR", &data.0)
        .env("ANTHROPIC_API_KEY", "sk-ant-test-dummy")
        .current_dir(&repo.0)
        .output()
        .unwrap()
}

#[test]
fn several_specs_all_go_into_the_prompt_and_the_record_in_the_given_order() {
    let (repo, data) = setup_repo();
    write_docs(&repo, &[("docs/req.md", "# 要件\n要件の本文ZZ1"), ("docs/design.md", "# 設計\n## 認証\n認証の本文ZZ2\n## 決済\n決済の本文ZZ3")]);
    // 設計書をコミットしておく（未コミットだと、新規ファイルとして差分に入り、プロンプトに全文が出てしまう）
    git(&repo.0, &["add", "docs"]);
    git(&repo.0, &["commit", "-q", "-m", "docs"]);
    let o = review(&repo, &data, &["--spec", "docs/design.md:##認証", "--spec", "docs/req.md"]);
    assert!(matches!(o.status.code(), Some(0) | Some(1)), "{}", String::from_utf8_lossy(&o.stderr));
    let records = mrcr_core::store::load_all(&data.0);
    assert_eq!(records.len(), 1);
    let r = &records[0];
    let names: Vec<&str> = r.specs.iter().map(|c| c.display_name.as_str()).collect();
    assert_eq!(names, ["docs/design.md # 認証", "docs/req.md"]);
    assert_eq!(r.spec.as_ref().map(|c| c.display_name.as_str()), Some("docs/design.md # 認証"), "spec には先頭の 1 件も入る");
    assert!(r.prompt.contains("認証の本文ZZ2") && r.prompt.contains("要件の本文ZZ1"), "両方の本文がプロンプトに入る");
    assert!(!r.prompt.contains("決済の本文ZZ3"), "節の指定は、ファイルごとに効く");
    assert!(r.prompt.find("認証の本文ZZ2") < r.prompt.find("要件の本文ZZ1"), "指定した順");
    assert_eq!(r.spec_docs().len(), 2);
}

#[test]
fn a_single_spec_still_fills_spec_and_specs_with_the_same_document() {
    let (repo, data) = setup_repo();
    write_docs(&repo, &[("docs/spec.md", "# 仕様\n本文")]);
    let o = review(&repo, &data, &["--spec", "docs/spec.md"]);
    assert!(matches!(o.status.code(), Some(0) | Some(1)));
    let r = &mrcr_core::store::load_all(&data.0)[0];
    assert_eq!(r.specs.len(), 1);
    assert_eq!(r.spec.as_ref().map(|c| &c.content), Some(&r.specs[0].content));
}

#[test]
fn one_bad_spec_among_several_fails_everything_and_never_calls_the_llm() {
    let (repo, data) = setup_repo();
    write_docs(&repo, &[("docs/a.md", "A")]);
    for bad in ["docs/missing.md", "../outside.md", "docs/code.rs"] {
        let o = review(&repo, &data, &["--spec", "docs/a.md", "--spec", bad]);
        assert_eq!(o.status.code(), Some(2), "{bad}: {}", String::from_utf8_lossy(&o.stderr));
        assert!(mrcr_core::store::load_all(&data.0).is_empty(), "{bad}: 失敗したら、記録も残さない（LLM を呼ばない）");
    }
}

#[test]
fn spec_and_spec_text_together_are_refused_instead_of_silently_ignoring_one() {
    let (repo, data) = setup_repo();
    write_docs(&repo, &[("docs/a.md", "A")]);
    let o = review(&repo, &data, &["--spec", "docs/a.md", "--spec-text", "貼り付け"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("同時に指定できません"), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(mrcr_core::store::load_all(&data.0).is_empty());
}

#[test]
fn too_many_specs_and_too_many_chars_are_refused_with_a_clear_message() {
    let (repo, data) = setup_repo();
    let many: Vec<(String, String)> = (0..6).map(|i| (format!("docs/f{i}.md"), "x".to_string())).collect();
    let refs: Vec<(&str, &str)> = many.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    write_docs(&repo, &refs);
    let mut a: Vec<&str> = Vec::new();
    for (n, _) in &many {
        a.push("--spec");
        a.push(n);
    }
    let o = review(&repo, &data, &a);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("多すぎます"), "{}", String::from_utf8_lossy(&o.stderr));

    let half = "a".repeat(15_001);
    write_docs(&repo, &[("docs/big1.md", &half), ("docs/big2.md", &half)]);
    let o = review(&repo, &data, &["--spec", "docs/big1.md", "--spec", "docs/big2.md"]);
    assert_eq!(o.status.code(), Some(2));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("超えています") && err.contains("docs/big1.md"), "内訳が出る: {err}");
    assert!(mrcr_core::store::load_all(&data.0).is_empty());
}

#[test]
fn the_same_spec_given_twice_is_used_once() {
    let (repo, data) = setup_repo();
    write_docs(&repo, &[("docs/a.md", "# A\n本文")]);
    let o = review(&repo, &data, &["--spec", "docs/a.md", "--spec", "./docs/a.md"]);
    assert!(matches!(o.status.code(), Some(0) | Some(1)), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(mrcr_core::store::load_all(&data.0)[0].specs.len(), 1);
}

fn review_with_stdin(repo: &Tmp, data: &Tmp, extra: &[&str], stdin_text: &str, mock_response: Option<&str>) -> std::process::Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut a = vec!["review", "--recipe", "spec", "--mock"];
    a.extend_from_slice(extra);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_mrcr"));
    cmd.args(a)
        .env("MRCR_DATA_DIR", &data.0)
        .env("ANTHROPIC_API_KEY", "sk-ant-test-dummy")
        .env_remove("MRCR_MOCK_RESPONSE")
        .current_dir(&repo.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(m) = mock_response {
        cmd.env("MRCR_MOCK_RESPONSE", m);
    }
    let mut child = cmd.spawn().unwrap();
    child.stdin.take().unwrap().write_all(stdin_text.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn a_file_and_stdin_can_be_mixed_and_a_second_dash_is_refused() {
    let (repo, data) = setup_repo();
    write_docs(&repo, &[("docs/a.md", "# A\nファイルの本文XX1")]);
    let o = review_with_stdin(&repo, &data, &["--spec", "docs/a.md", "--spec", "-"], "標準入力の本文YY2\n", None);
    assert!(matches!(o.status.code(), Some(0) | Some(1)), "{}", String::from_utf8_lossy(&o.stderr));
    let records = mrcr_core::store::load_all(&data.0);
    assert_eq!(records.len(), 1);
    let r = &records[0];
    assert_eq!(r.specs.len(), 2);
    assert_eq!(r.specs[0].display_name, "docs/a.md");
    assert!(r.specs[1].display_name.starts_with("標準入力の要件"), "{}", r.specs[1].display_name);
    assert!(r.prompt.contains("ファイルの本文XX1") && r.prompt.contains("標準入力の本文YY2"), "両方の本文がプロンプトに入る");

    // `-` を 2 回書くと、LLM を呼ばずに終了コード 2
    let (repo2, data2) = setup_repo();
    let o = review_with_stdin(&repo2, &data2, &["--spec", "-", "--spec", "-"], "本文", None);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("1 回まで"), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(mrcr_core::store::load_all(&data2.0).is_empty());
}

#[test]
fn the_command_checks_quotes_against_every_document_and_names_the_source_file() {
    let (repo, data) = setup_repo();
    write_docs(&repo, &[("docs/a.md", "# 要件\n一枚目の文書にだけある一文。"), ("docs/b.md", "# 設計\n二枚目の文書にだけある一文。")]);
    let finding = |quote: &str| serde_json::json!({"file": "a.txt", "line": 1, "severity": "high", "title": "t", "detail": "d", "evidence": "hello world", "spec_quote": quote});
    let response = serde_json::json!({"findings": [
        finding("一枚目の文書にだけある一文。"),
        finding("二枚目の文書にだけある一文。"),
        finding("どの文書にも無い一文。"),
    ]})
    .to_string();
    let o = review_with_stdin(&repo, &data, &["--spec", "docs/a.md", "--spec", "docs/b.md"], "", Some(&response));
    assert!(matches!(o.status.code(), Some(0) | Some(1)), "{}", String::from_utf8_lossy(&o.stderr));
    let f = &mrcr_core::store::load_all(&data.0)[0].findings;
    assert_eq!(f.len(), 3);
    assert_eq!((f[0].spec_verified, f[0].spec_source.as_deref()), (Some(true), Some("docs/a.md")));
    assert_eq!((f[1].spec_verified, f[1].spec_source.as_deref()), (Some(true), Some("docs/b.md")), "2 件目の文書の原文でも成功し、その文書名が付く");
    assert_eq!((f[2].spec_verified, f[2].spec_source.as_deref()), (Some(false), None));
}
