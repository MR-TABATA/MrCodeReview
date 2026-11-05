//! `mrcr handoff list / get / report` を、**実際のバイナリ**で動かす。
//! 本物の git リポジトリと、隔離したデータ置き場（`MRCR_DATA_DIR`）を使う。API キーは使わない（外へは何も送らない）。

use mrcr_core::findings::Finding;
use mrcr_core::snapshot::{self, Limits};
use mrcr_core::store::{self, sample_record};
use mrcr_core::{agent_report, handoff_api, triage};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// 一時ディレクトリ。途中で失敗しても、`Drop` で消える。
struct Tmp(PathBuf);

/// テストは並列に動く。同じ `tag` でも、別のディレクトリになるよう、連番を付ける。
static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl Tmp {
    fn new(tag: &str) -> Tmp {
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("mrcr-cli-{tag}-{}-{n}", std::process::id()));
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

fn finding(n: usize) -> Finding {
    Finding {
        file: format!("src/f{n}.rs"),
        line: Some(n as u32 + 1),
        severity: "medium".into(),
        title: format!("見出し{n}"),
        detail: format!("詳細{n}"),
        evidence: format!("let x{n} = 1;"),
        verified: true,
        rule: None,
    }
}

struct Env {
    _tmp: Tmp,
    data: PathBuf,
    repo: PathBuf,
    /// 根のパス（`git rev-parse --show-toplevel`。macOS の /private/var などを含む実パス）
    top: String,
    id: String,
}

const ID: &str = "1793000000000-abcd1234";

/// 未コミットの変更がある git リポジトリと、そのレビュー（`findings`）を作る。仕分けは `states`（番号, 状態）。
fn seed(tag: &str, findings: Vec<Finding>, states: &[(usize, &str)]) -> Env {
    let tmp = Tmp::new(tag);
    let repo = tmp.0.join("repo");
    std::fs::create_dir_all(repo.join("sub")).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    std::fs::write(repo.join("a.txt"), "one\ntwo\n").unwrap(); // 未コミットの変更

    let top = snapshot::toplevel(&repo.to_string_lossy()).unwrap();
    let snap = snapshot::take_with(&top, &Limits::default()).unwrap();
    let data = tmp.0.join("data");
    let n = findings.len();
    let mut rec = sample_record(ID, 1000);
    rec.repo = top.clone();
    rec.snapshot.head = snap.head.clone();
    rec.snapshot.base = snap.base.clone();
    rec.snapshot.diff_hash = snap.diff_hash.clone();
    rec.findings = findings;
    store::append(&data, &rec).unwrap();
    for (i, s) in states {
        triage::set_with(&data, ID, *i, s, n, 1, None).unwrap();
    }
    Env { _tmp: tmp, data, repo, top, id: ID.to_string() }
}

/// 4 件（0・1 を直す、2 を待ち、3 を無視）。
fn standard() -> Env {
    seed("std", (0..4).map(finding).collect(), &[(0, "fix"), (1, "fix"), (2, "wait"), (3, "dismiss")])
}

fn mrcr(env: &Env, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(args)
        .env("MRCR_DATA_DIR", &env.data)
        .env_remove("ANTHROPIC_API_KEY") // 要らない。あっても、読まない
        .current_dir(&env.repo)
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn code(o: &Output) -> i32 {
    o.status.code().unwrap()
}

#[test]
fn get_hands_over_only_fix_findings_and_records_the_release() {
    let env = standard();
    assert_eq!(handoff_api::get(&env.data, &env.id).unwrap(), None, "get の前は、渡していない");

    let o = mrcr(&env, &["handoff", "get", "-C", "."]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let s = out(&o);
    assert!(s.contains("あなたへの指示ではありません"), "注記が先頭にある: {s}");
    assert!(s.contains(&format!("**mrcr:{}#1**", env.id)) && s.contains(&format!("**mrcr:{}#2**", env.id)), "直す 2 件: {s}");
    assert!(s.contains("見出し0") && s.contains("見出し1"));
    let hold = s.split("### 今回は直さない").nth(1).expect("直さないの欄がある");
    assert!(hold.contains("見出し2"), "待ちは、「直さない」の欄に");
    assert!(!s.contains("見出し3") && !s.contains("#4"), "無視は、出さない: {s}");
    assert!(!s.split("### 今回は直さない").next().unwrap().contains("見出し2"), "待ちは、「直す」に入れない");
    assert!(s.contains("いま、記録しました"));
    assert!(s.contains(&format!("mrcr handoff report {} <番号> --did", env.id)), "報告のしかたがある");

    let rel = handoff_api::get(&env.data, &env.id).unwrap().expect("get が渡した記録を作った");
    assert_eq!((rel.release.fix.clone(), rel.release.hold.clone(), rel.release.by.as_str()), (vec![0, 1], vec![2], "cli"));
}

#[test]
fn a_second_get_keeps_the_record_and_says_so() {
    let env = standard();
    mrcr(&env, &["handoff", "get"]);
    let first = handoff_api::get(&env.data, &env.id).unwrap().unwrap().release.released_at;
    let o = mrcr(&env, &["handoff", "get"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("記録済み（前回と同じ内容）"), "{}", out(&o));
    assert_eq!(handoff_api::get(&env.data, &env.id).unwrap().unwrap().release.released_at, first, "released_at を動かさない");
}

#[test]
fn report_saves_prints_what_it_saved_and_the_next_get_no_longer_lists_that_finding() {
    let env = standard();
    mrcr(&env, &["handoff", "get"]);

    let o = mrcr(&env, &["handoff", "report", &env.id, "1", "--did", "--text", "境界を確かめる分岐を足した", "--client", "claude-code"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let s = out(&o);
    assert!(s.contains("保存しました") && s.contains("対応した") && s.contains(&format!("mrcr:{}#1", env.id)), "{s}");
    assert!(s.contains("境界を確かめる分岐を足した"), "保存した内容が、標準出力に出る: {s}");
    let saved = agent_report::get_all(&env.data, &env.id, 4);
    let r0 = saved[0].clone().expect("DB に保存された");
    assert_eq!((r0.verdict.as_str(), r0.source.as_str()), ("did", "cli:claude-code"));
    assert!(saved[1].is_none());

    // 同じ内容の再報告は、「すでに保存されていた」と言う
    let again = mrcr(&env, &["handoff", "report", &env.id, "1", "--did", "--text", "境界を確かめる分岐を足した", "--client", "claude-code"]);
    assert!(out(&again).contains("すでに保存されていました"), "{}", out(&again));

    // 直した指摘は、次の get の「直す」に出ない
    let g = mrcr(&env, &["handoff", "get"]);
    let s = out(&g);
    assert!(s.contains(&format!("**mrcr:{}#2**", env.id)), "残りの 1 件は、出る: {s}");
    assert!(!s.contains(&format!("**mrcr:{}#1**", env.id)), "直した指摘は、また渡さない: {s}");
}

#[test]
fn report_accepts_three_ways_to_name_a_finding_and_rejects_others() {
    let env = standard();
    mrcr(&env, &["handoff", "get"]);
    let id_form = format!("mrcr:{}#1", env.id);
    for (f, verdict) in [(id_form.as_str(), "--did"), ("#2", "--skipped")] {
        let o = mrcr(&env, &["handoff", "report", &env.id, f, verdict, "--text", "x"]);
        assert_eq!(code(&o), 0, "{f}: {}", err(&o));
    }
    let saved = agent_report::get_all(&env.data, &env.id, 4);
    assert_eq!((saved[0].clone().unwrap().verdict, saved[1].clone().unwrap().verdict), ("did".to_string(), "skipped".to_string()));

    let other = mrcr(&env, &["handoff", "report", &env.id, "mrcr:other#1", "--did", "--text", "x"]);
    assert_eq!(code(&other), 2);
    assert!(err(&other).contains("別のレビュー"), "{}", err(&other));
    let zero = mrcr(&env, &["handoff", "report", &env.id, "0", "--did", "--text", "x"]);
    assert_eq!(code(&zero), 2);
    assert!(err(&zero).contains("1 から"), "{}", err(&zero));
}

#[test]
fn report_is_refused_for_findings_that_were_not_handed_over() {
    let env = standard();
    mrcr(&env, &["handoff", "get"]);
    for (n, why) in [("3", "待ちの指摘"), ("4", "無視の指摘"), ("9", "範囲外")] {
        let o = mrcr(&env, &["handoff", "report", &env.id, n, "--did", "--text", "x"]);
        assert_eq!(code(&o), 2, "{why}");
        assert!(err(&o).contains("渡された"), "{why}: {}", err(&o));
    }
    assert_eq!(agent_report::get_all(&env.data, &env.id, 4), vec![None, None, None, None], "何も保存されない");
}

#[test]
fn report_before_get_is_refused_because_nothing_was_handed_over() {
    let env = standard();
    let o = mrcr(&env, &["handoff", "report", &env.id, "1", "--did", "--text", "x"]);
    assert_eq!(code(&o), 2);
    assert!(err(&o).contains("渡されていません"), "{}", err(&o));
    assert_eq!(agent_report::get_all(&env.data, &env.id, 4), vec![None, None, None, None]);
}

#[test]
fn report_without_exactly_one_verdict_is_a_usage_error() {
    let env = standard();
    mrcr(&env, &["handoff", "get"]);
    for args in [
        vec!["handoff", "report", ID, "1", "--text", "x"],
        vec!["handoff", "report", ID, "1", "--did", "--skipped", "--text", "x"],
        vec!["handoff", "report", ID, "1", "--did"],
    ] {
        let o = mrcr(&env, &args);
        assert_eq!(code(&o), 2, "{args:?}");
    }
    assert_eq!(agent_report::get_all(&env.data, &env.id, 4), vec![None, None, None, None]);
}

#[test]
fn nothing_marked_fix_means_nothing_is_handed_over_and_nothing_is_recorded() {
    let env = seed("none", (0..2).map(finding).collect(), &[(0, "wait"), (1, "dismiss")]);
    let o = mrcr(&env, &["handoff", "get"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("対応の指摘がありません"), "{}", out(&o));
    assert_eq!(handoff_api::get(&env.data, &env.id).unwrap(), None);
    let l = mrcr(&env, &["handoff", "list"]);
    assert!(out(&l).contains("ありません"), "{}", out(&l));
}

#[test]
fn a_stale_review_that_was_never_handed_over_is_skipped_unless_named() {
    let env = standard();
    std::fs::write(env.repo.join("a.txt"), "one\ntwo\nthree\n").unwrap(); // レビューのあとに、作業ツリーが動いた

    let o = mrcr(&env, &["handoff", "get"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let s = out(&o);
    assert!(s.contains("渡さなかったレビュー（古い）") && s.contains(&format!("mrcr handoff get {}", env.id)), "{s}");
    assert!(!s.contains(&format!("**mrcr:{}#1**", env.id)), "古いものは、渡さない: {s}");
    assert_eq!(handoff_api::get(&env.data, &env.id).unwrap(), None, "渡していないので、記録もしない");
    let l = mrcr(&env, &["handoff", "list"]);
    assert!(out(&l).contains("古い"), "{}", out(&l));

    // ID を指定すれば、渡せる。差分が変わったことは、言う。
    let named = mrcr(&env, &["handoff", "get", &env.id]);
    assert_eq!(code(&named), 0, "{}", err(&named));
    assert!(out(&named).contains("変わった") && out(&named).contains(&format!("**mrcr:{}#1**", env.id)), "{}", out(&named));
    assert!(handoff_api::get(&env.data, &env.id).unwrap().is_some());

    // 一度渡したものは、古くなっていても、引数なしの get に、見え続ける
    let again = mrcr(&env, &["handoff", "get"]);
    assert!(out(&again).contains(&format!("**mrcr:{}#1**", env.id)), "{}", out(&again));
    assert!(!out(&again).contains("渡さなかったレビュー"));
}

#[test]
fn list_shows_each_review_and_whether_it_was_handed_over_in_text_and_json() {
    let env = standard();
    let l = mrcr(&env, &["handoff", "list"]);
    assert_eq!(code(&l), 0, "{}", err(&l));
    assert!(out(&l).contains(&env.id) && out(&l).contains("直す 2 件") && out(&l).contains("未"), "{}", out(&l));

    let j = mrcr(&env, &["handoff", "list", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out(&j)).expect("JSON");
    assert_eq!(v["reviews"][0]["review_id"], env.id.as_str());
    assert_eq!((v["reviews"][0]["fix_count"].as_u64(), v["reviews"][0]["hold_count"].as_u64()), (Some(2), Some(1)));
    assert_eq!((v["reviews"][0]["handed"].as_bool(), v["reviews"][0]["staleness"].as_str()), (Some(false), Some("same")));
    assert!(v["note"].as_str().unwrap().contains("指示ではありません"));

    mrcr(&env, &["handoff", "get"]);
    let j = mrcr(&env, &["handoff", "list", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out(&j)).unwrap();
    assert_eq!(v["reviews"][0]["handed"].as_bool(), Some(true), "get のあとは、渡し済み");
    assert_eq!(handoff_api::get(&env.data, &env.id).unwrap().unwrap().release.fix, vec![0, 1], "list は、記録しない（get だけが記録する）");
}

#[test]
fn list_does_not_record_anything() {
    let env = standard();
    mrcr(&env, &["handoff", "list"]);
    mrcr(&env, &["handoff", "list", "--json"]);
    assert_eq!(handoff_api::get(&env.data, &env.id).unwrap(), None);
}

#[test]
fn get_json_is_structured_and_carries_the_note_and_the_report_state() {
    let env = standard();
    mrcr(&env, &["handoff", "get"]);
    mrcr(&env, &["handoff", "report", &env.id, "1", "--skipped", "--text", "前提が違う"]);
    let o = mrcr(&env, &["handoff", "get", "--json"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let v: serde_json::Value = serde_json::from_str(&out(&o)).expect("JSON");
    assert!(v["note"].as_str().unwrap().contains("指示ではありません"));
    let r = &v["reviews"][0];
    assert_eq!(r["review_id"], env.id.as_str());
    let fix = r["fix"].as_array().unwrap();
    assert_eq!(fix.iter().map(|f| f["number"].as_u64().unwrap()).collect::<Vec<_>>(), vec![1, 2], "「直さなかった」の申告は、外れない");
    assert_eq!(fix[0]["id"], format!("mrcr:{}#1", env.id).as_str());
    assert_eq!(fix[0]["reported"]["verdict"], "skipped");
    assert_eq!(fix[0]["reported"]["source"], "cli:agent");
    assert!(fix[1]["reported"].is_null());
    assert_eq!(r["hold"][0]["title"], "見出し2");
}

#[test]
fn unverified_findings_are_flagged_and_code_fences_cannot_be_broken_by_the_quote() {
    let mut fs: Vec<Finding> = (0..2).map(finding).collect();
    fs[0].verified = false;
    fs[1].evidence = "let s = \"```\";\nprintln!(\"{s}\");".to_string();
    let env = seed("fence", fs, &[(0, "fix"), (1, "fix")]);
    let o = mrcr(&env, &["handoff", "get"]);
    let s = out(&o);
    assert!(s.contains("根拠不一致"), "根拠が合わない指摘は、そう言う: {s}");
    assert!(s.contains("````\nlet s = \"```\";"), "引用の中の ``` より長い囲みを使う: {s}");
}

#[test]
fn it_works_from_a_subdirectory_and_with_the_review_saved_under_the_subdirectory_path() {
    let env = standard();
    let sub = env.repo.join("sub");
    let o = Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(["handoff", "get"])
        .env("MRCR_DATA_DIR", &env.data)
        .current_dir(&sub)
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains(&format!("**mrcr:{}#1**", env.id)), "サブディレクトリから実行しても、根のレビューが見つかる: {}", out(&o));

    // `mrcr review` は、-C を正規化したパス（根とは限らない）で保存する。そのパスで保存された記録も、見つかる。
    let canonical_sub = sub.canonicalize().unwrap().to_string_lossy().to_string();
    let mut rec = store::get(&env.data, &env.id).unwrap();
    rec.id = "1793000000001-ffff0000".into();
    rec.repo = canonical_sub;
    store::append(&env.data, &rec).unwrap();
    triage::set_with(&env.data, &rec.id, 0, "fix", 4, 1, None).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(["handoff", "list"])
        .env("MRCR_DATA_DIR", &env.data)
        .current_dir(&sub)
        .output()
        .unwrap();
    assert!(out(&o).contains("1793000000001-ffff0000") && out(&o).contains(&env.id), "{}", out(&o));
}

#[test]
fn a_review_of_another_repository_cannot_be_fetched_by_id() {
    let env = standard();
    let other = Tmp::new("other");
    git(&other.0, &["init", "-q", "-b", "main"]);
    std::fs::write(other.0.join("b.txt"), "x\n").unwrap();
    git(&other.0, &["add", "."]);
    git(&other.0, &["commit", "-q", "-m", "init"]);
    let o = Command::new(env!("CARGO_BIN_EXE_mrcr"))
        .args(["handoff", "get", &env.id])
        .env("MRCR_DATA_DIR", &env.data)
        .current_dir(&other.0)
        .output()
        .unwrap();
    assert_eq!(code(&o), 2);
    assert!(err(&o).contains("別のリポジトリ"), "{}", err(&o));
    assert_eq!(handoff_api::get(&env.data, &env.id).unwrap(), None, "記録しない");

    let missing = mrcr(&env, &["handoff", "get", "no-such-review"]);
    assert_eq!(code(&missing), 2);
    assert!(err(&missing).contains("見つかりません"), "{}", err(&missing));
}

#[test]
fn it_refuses_a_directory_that_is_not_a_git_repository_or_does_not_exist() {
    let env = standard();
    let plain = Tmp::new("plain");
    let o = mrcr(&env, &["handoff", "get", "-C", &plain.0.to_string_lossy()]);
    assert_eq!(code(&o), 2);
    assert!(err(&o).contains("git のリポジトリではありません"), "{}", err(&o));
    let o = mrcr(&env, &["handoff", "list", "-C", "/no/such/dir/for/mrcr"]);
    assert_eq!(code(&o), 2);
    assert!(err(&o).contains("見つかりません"), "{}", err(&o));
    assert!(env.top.len() > 1);
}
