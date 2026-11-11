//! 「対応」にした指摘を、CLI（あなたのコーディングエージェント）に渡すための、ファイルの書き出し。
//!
//! - 書き出し先は、リポジトリの中の `.mrcodereview/out/handoff-<観点の id>.md`（観点ごとに別のファイル）。エージェントが、普通のファイル読みで読める。
//! - そのフォルダに、`.gitignore`（`*`）も作る。**誤ってコミットされない**。`.mrcodereview/` の他の場所（リポジトリ側の設定）は、
//!   コミットされるものなので、書き出し先は、`out/` に分ける。
//! - 内容は、データベースから作る**写し**（`triage::handoff_text`）。元のデータは、SQLite だけ。消しても困らない。
//! - リポジトリの中に、`.mrcodereview` や `out` を**シンボリックリンク**にして、外のファイルを書き換えさせる、という攻撃を防ぐ。
//!   リンクなら、書き出さない。

use crate::store::ReviewRecord;
use crate::triage;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const DIR: &str = ".mrcodereview/out";

/// 観点（recipe）の id から、書き出すファイル名を作る。観点ごとに別のファイルにして、複数の AI に別々の観点を任せても、読み合わない・上書きし合わない。
/// id は、ファイル名に使える文字（英数字・`-`・`_`）だけにする（それ以外は `_`）。
pub fn file_name(recipe_id: &str) -> String {
    let safe: String = recipe_id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if safe.is_empty() { "handoff.md".to_string() } else { format!("handoff-{safe}.md") }
}

#[derive(Debug, Clone, Serialize)]
pub struct HandoffFile {
    /// 書き出したファイルの絶対パス
    pub path: String,
    /// リポジトリの根からの相対パス（CLI に貼る 1 行に使う）
    pub rel: String,
    /// 「直す」にした指摘の件数
    pub count: usize,
    /// CLI に貼る 1 行
    pub prompt: String,
    /// 書き出した文面そのもの（何を渡したかを、画面で見せるため）
    pub text: String,
}

/// CLI に貼る 1 行。
pub fn prompt_for(rel: &str) -> String {
    format!("{rel} を読んで、そこに書いてある「直す」の指摘を直してください。「今回は直さない」は触らないでください。")
}

fn refuse_symlink(p: &Path) -> Result<(), String> {
    match fs::symlink_metadata(p) {
        Ok(m) if m.file_type().is_symlink() => Err(format!("{} がシンボリックリンクなので、書き出しません（リポジトリの外に書いてしまう恐れがあります）", p.display())),
        _ => Ok(()),
    }
}

/// 「対応」の指摘を、ファイルに書き出す。「対応」が 1 件も無ければ、エラー（空のファイルは、作らない）。
/// `only` を渡すと、**その 1 件だけ**を書き出す（その指摘が「対応」でなければ、エラー。「今回は直さない」の欄も、付けない）。
pub fn write(rec: &ReviewRecord, states: &[String], only: Option<usize>, app_name: &str) -> Result<HandoffFile, String> {
    let one: Vec<String>;
    let states: &[String] = match only {
        Some(i) => {
            if i >= states.len() {
                return Err(format!("指摘 {i} 番は無い（{} 件）", states.len()));
            }
            if states[i] != triage::FIX {
                return Err("「対応」にした指摘だけ、CLI に渡せます（先に、この指摘を「対応」にしてください）".into());
            }
            one = (0..states.len()).map(|n| if n == i { triage::FIX.to_string() } else { triage::NEW.to_string() }).collect();
            &one
        }
        None => states,
    };
    let text = triage::handoff_text(rec, states, app_name);
    if text.is_empty() {
        return Err("「対応」にした指摘がありません（先に、指摘を「対応」にしてください）".into());
    }
    let count = states.iter().filter(|s| s.as_str() == triage::FIX).count();
    let root = Path::new(&rec.repo);
    if !root.is_dir() {
        return Err(format!("リポジトリが見つかりません: {}", rec.repo));
    }
    let root_canon = root.canonicalize().map_err(|e| format!("リポジトリを調べられません: {e}"))?;

    let mrc = root.join(".mrcodereview");
    let out = root.join(DIR);
    refuse_symlink(&mrc)?;
    refuse_symlink(&out)?;
    if !out.is_dir() {
        fs::create_dir_all(&out).map_err(|e| format!("書き出し先を作れません（{}）: {e}", out.display()))?;
    }
    // 作ったあとに、もう一度、リポジトリの中かを確かめる
    let out_canon = out.canonicalize().map_err(|e| format!("書き出し先を調べられません: {e}"))?;
    if !out_canon.starts_with(&root_canon) {
        return Err(format!("書き出し先が、リポジトリの外になります: {}", out_canon.display()));
    }

    // コミットされないようにする。すでに、あれば（あなたが書き換えていれば）、そのまま
    let ignore = out.join(".gitignore");
    if fs::symlink_metadata(&ignore).is_err() {
        fs::write(&ignore, "# コードレビューの書き出し。コミットしない\n*\n").map_err(|e| format!(".gitignore を書けません: {e}"))?;
    }

    // 一時ファイルに書いて、置き換える。一時ファイルが、リンクとして先に置かれていても、その先には書かない。
    // 確認した「実体」のフォルダ（`out_canon`）の下に作る。確認のあとに、`out` がリンクに差し替えられても、確認した先に書く。
    let file = file_name(&rec.recipe.id);
    let tmp: PathBuf = out_canon.join(format!("{file}.tmp"));
    let dest = out.join(&file);
    // 書く直前に、もう一度、リンクでないことを確かめる（確認と書き込みのすき間を、狭める）
    refuse_symlink(&mrc)?;
    refuse_symlink(&out)?;
    // 先に置かれた一時ファイルを消す。無ければ、そのまま。消せなければ、理由を言う（「すでに存在する」と取り違えさせない）
    match fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("前に残った一時ファイル（{}）を消せません: {e}", tmp.display())),
    }
    let written = (|| -> Result<(), String> {
        let mut f = fs::OpenOptions::new().write(true).create_new(true).open(&tmp).map_err(|e| format!("一時ファイルを作れません: {e}"))?;
        f.write_all(text.as_bytes()).map_err(|e| format!("書き込めません: {e}"))?;
        drop(f);
        // 作ったものの実体が、リポジトリの中かを、書いたあとにも確かめる（すき間で差し替えられていたら、置き換えない）
        let made = tmp.canonicalize().map_err(|e| format!("一時ファイルを調べられません: {e}"))?;
        if !made.starts_with(&root_canon) {
            return Err(format!("書き出し先が、リポジトリの外になりました: {}", made.display()));
        }
        fs::rename(&tmp, out_canon.join(&file)).map_err(|e| format!("書き出せません（{}）: {e}", dest.display()))
    })();
    if let Err(e) = written {
        // 失敗したときに、書きかけの一時ファイルを残さない。消せなければ、それも伝える
        let leftover = match fs::remove_file(&tmp) {
            Ok(()) => String::new(),
            Err(r) if r.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(r) => format!("（一時ファイル {} も消せませんでした: {r}）", tmp.display()),
        };
        return Err(format!("{e}{leftover}"));
    }

    let rel = format!("{DIR}/{file}");
    Ok(HandoffFile { path: dest.to_string_lossy().into_owned(), prompt: prompt_for(&rel), rel, count, text })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::findings::Finding;
    use crate::store::sample_record;
    use std::process::Command;

    fn repo(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mrcr-handoff-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let git = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(&d).args(args).status().unwrap().success());
        git(&["init", "-q", "-b", "main"]);
        fs::write(d.join("a.txt"), "x\n").unwrap();
        git(&["add", "."]);
        git(&["-c", "user.name=t", "-c", "user.email=t@example.com", "commit", "-q", "-m", "init"]);
        d
    }

    fn rec_in(dir: &Path) -> ReviewRecord {
        let mut r = sample_record("R1", 1000);
        r.repo = dir.canonicalize().unwrap().to_string_lossy().into_owned();
        r.snapshot.head = "abcdef1234567890".into();
        r.findings = ["最初の欠陥", "次の欠陥", "見送る欠陥"]
            .iter()
            .map(|t| Finding { file: "src/a.rs".into(), line: Some(3), severity: "high".into(), title: (*t).into(), detail: format!("{t}の理由"), evidence: format!("let x = {t};"), verified: true, rule: None })
            .collect();
        r
    }

    fn st(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn porcelain(d: &Path) -> String {
        String::from_utf8_lossy(&Command::new("git").arg("-C").arg(d).args(["status", "--porcelain", "--untracked-files=all"]).output().unwrap().stdout).trim().to_string()
    }

    #[test]
    fn each_recipe_gets_its_own_file_and_ids_are_made_safe() {
        assert_eq!(file_name("bugs"), "handoff-bugs.md");
        assert_eq!(file_name("../x y"), "handoff-___x_y.md");
        assert_eq!(file_name(""), "handoff.md");
        let d = repo("two-recipes");
        let mut a = rec_in(&d);
        a.recipe.id = "bugs".into();
        let mut b = rec_in(&d);
        b.recipe.id = "security".into();
        let t = vec![triage::FIX.to_string(); a.findings.len()];
        let ha = write(&a, &t, None, "App").unwrap();
        let hb = write(&b, &t, None, "App").unwrap();
        assert_ne!(ha.path, hb.path);
        assert!(ha.rel.ends_with("handoff-bugs.md") && hb.rel.ends_with("handoff-security.md") && Path::new(&ha.path).is_file() && Path::new(&hb.path).is_file());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn writes_the_handoff_text_into_an_ignored_folder_inside_the_repo() {
        let d = repo("ok");
        let rec = rec_in(&d);
        let h = write(&rec, &st(&["fix", "wait", "fix"]), None, "MrCodeReview").unwrap();
        assert_eq!((h.rel.as_str(), h.count), (".mrcodereview/out/handoff-security.md", 2));
        assert!(h.path.ends_with("/.mrcodereview/out/handoff-security.md") && Path::new(&h.path).is_file());
        let body = fs::read_to_string(&h.path).unwrap();
        assert_eq!(body, triage::handoff_text(&rec, &st(&["fix", "wait", "fix"]), "MrCodeReview"), "データベースから作る写しと、同じ");
        for want in ["最初の欠陥", "見送る欠陥", "次の欠陥", "mrcr:R1", "mrcr:R1#3"] {
            assert!(body.contains(want), "{want}");
        }
        // 「対応」の 2 件は「直す」の欄、「待ち」の 1 件は「今回は直さない」の欄
        let at = |s: &str| body.find(s).unwrap_or_else(|| panic!("{s} が無い"));
        assert!(at("## 直す") < at("最初の欠陥") && at("最初の欠陥") < at("## 今回は直さない"), "{body}");
        assert!(at("見送る欠陥") < at("## 今回は直さない"), "対応の指摘は、直す側");
        assert!(at("## 今回は直さない") < at("次の欠陥"), "待ちの指摘は、直さない側");
        assert!(h.prompt.starts_with(".mrcodereview/out/handoff-security.md を読んで") && h.prompt.contains("今回は直さない"), "{}", h.prompt);
        assert!(!porcelain(&d).contains(".mrcodereview"), "コミットの対象に、現れない: {}", porcelain(&d));
        assert_eq!(fs::read_to_string(d.join(".mrcodereview/out/.gitignore")).unwrap(), "# コードレビューの書き出し。コミットしない\n*\n");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn handing_over_one_finding_writes_only_that_one() {
        let d = repo("one");
        let rec = rec_in(&d);
        let states = st(&["fix", "wait", "fix"]);
        let h = write(&rec, &states, Some(2), "MrCodeReview").unwrap();
        assert_eq!(h.count, 1);
        assert!(h.text.contains("見送る欠陥") && h.text.contains("mrcr:R1#3"), "{}", h.text);
        assert!(!h.text.contains("最初の欠陥") && !h.text.contains("次の欠陥"), "ほかの指摘（対応の 1 番目と、待ち）は、含めない");
        assert!(!h.text.contains("今回は直さない"), "1 件だけのときは、「今回は直さない」の欄を付けない");
        assert_eq!(fs::read_to_string(&h.path).unwrap(), h.text, "返す文面は、書いたファイルと同じ");
        // 「対応」でない指摘と、範囲外は、断る（ファイルは、前のまま）
        let e = write(&rec, &states, Some(1), "MrCodeReview").unwrap_err();
        assert!(e.contains("「対応」にした指摘だけ"), "{e}");
        assert!(write(&rec, &states, Some(9), "MrCodeReview").unwrap_err().contains("指摘 9 番は無い"));
        assert_eq!(fs::read_to_string(&h.path).unwrap(), h.text, "断ったときは、前の内容を、壊さない");
        // まとめて渡すときは、従来どおり、対応の全部
        let all = write(&rec, &states, None, "MrCodeReview").unwrap();
        assert_eq!(all.count, 2);
        assert!(all.text.contains("最初の欠陥") && all.text.contains("見送る欠陥") && all.text.contains("今回は直さない"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn writing_again_replaces_the_file_and_keeps_a_gitignore_you_edited() {
        let d = repo("again");
        let rec = rec_in(&d);
        write(&rec, &st(&["fix", "new", "new"]), None, "MrCodeReview").unwrap();
        fs::write(d.join(".mrcodereview/out/.gitignore"), "# 自分で書いた\n*\n!keep.txt\n").unwrap();
        let h = write(&rec, &st(&["new", "fix", "new"]), None, "MrCodeReview").unwrap();
        let body = fs::read_to_string(&h.path).unwrap();
        assert!(body.contains("次の欠陥") && !body.contains("最初の欠陥"), "新しい内容に、置き換わる");
        assert!(fs::read_to_string(d.join(".mrcodereview/out/.gitignore")).unwrap().contains("自分で書いた"), "書き換えた .gitignore は、そのまま");
        assert!(!d.join(".mrcodereview/out/handoff-security.md.tmp").exists(), "一時ファイルが残らない");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn no_fix_findings_means_an_error_and_no_file() {
        let d = repo("none");
        let rec = rec_in(&d);
        let e = write(&rec, &st(&["wait", "dismiss", "new"]), None, "MrCodeReview").unwrap_err();
        assert!(e.contains("「対応」にした指摘がありません"), "{e}");
        assert!(!d.join(".mrcodereview").exists(), "何も作らない");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_missing_repo_is_an_error() {
        let d = repo("gone");
        let rec = rec_in(&d);
        fs::remove_dir_all(&d).unwrap();
        assert!(write(&rec, &st(&["fix", "new", "new"]), None, "MrCodeReview").unwrap_err().contains("リポジトリが見つかりません"));
    }

    #[test]
    fn failure_when_parent_is_a_file_does_not_break_existing_handoff() {
        let d = repo("parent-file");
        let rec = rec_in(&d);
        // 先に正常な handoff-security.md を作っておく
        let h = write(&rec, &st(&["fix", "new", "new"]), None, "MrCodeReview").unwrap();
        let original_body = fs::read_to_string(&h.path).unwrap();

        // .mrcodereview が通常ファイルのとき、書き出し先を作れずエラーになる
        let d2 = repo("parent-file-err");
        let rec2 = rec_in(&d2);
        fs::write(d2.join(".mrcodereview"), "通常ファイル").unwrap();
        let err = write(&rec2, &st(&["fix", "new", "new"]), None, "MrCodeReview").unwrap_err();
        assert!(err.contains("書き出し先を作れません"), "{err}");

        // 既存の handoff-security.md の内容は変わらない
        assert_eq!(fs::read_to_string(&h.path).unwrap(), original_body);

        let _ = fs::remove_dir_all(&d);
        let _ = fs::remove_dir_all(&d2);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_that_would_write_outside_the_repo_are_refused() {
        use std::os::unix::fs::symlink;
        let outside = std::env::temp_dir().join(format!("mrcr-handoff-outside-{}", std::process::id()));
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&outside).unwrap();

        // 1. .mrcodereview が、外のフォルダへのリンク
        let d = repo("link1");
        symlink(&outside, d.join(".mrcodereview")).unwrap();
        let rec = rec_in(&d);
        assert!(write(&rec, &st(&["fix", "new", "new"]), None, "MrCodeReview").unwrap_err().contains("シンボリックリンク"));
        assert!(fs::read_dir(&outside).unwrap().next().is_none(), "外に、何も書かれていない");

        // 2. out が、外のフォルダへのリンク
        let d2 = repo("link2");
        fs::create_dir_all(d2.join(".mrcodereview")).unwrap();
        symlink(&outside, d2.join(".mrcodereview/out")).unwrap();
        assert!(write(&rec_in(&d2), &st(&["fix", "new", "new"]), None, "MrCodeReview").unwrap_err().contains("シンボリックリンク"));
        assert!(fs::read_dir(&outside).unwrap().next().is_none());

        // 3. handoff-security.md が、外のファイルへのリンク（置き換えるのは、リンクそのもので、先ではない）
        let d3 = repo("link3");
        fs::create_dir_all(d3.join(".mrcodereview/out")).unwrap();
        let victim = outside.join("victim.txt");
        fs::write(&victim, "守るべき内容").unwrap();
        symlink(&victim, d3.join(".mrcodereview/out/handoff-security.md")).unwrap();
        // 4. 一時ファイルも、外のファイルへのリンクとして、先に置かれている
        symlink(&victim, d3.join(".mrcodereview/out/handoff-security.md.tmp")).unwrap();
        let h = write(&rec_in(&d3), &st(&["fix", "new", "new"]), None, "MrCodeReview").unwrap();
        assert_eq!(fs::read_to_string(&victim).unwrap(), "守るべき内容", "リンクの先は、書き換えられない");
        assert!(!fs::symlink_metadata(&h.path).unwrap().file_type().is_symlink(), "書いたのは、本物のファイル");
        assert!(fs::read_to_string(&h.path).unwrap().contains("最初の欠陥"));

        for p in [&d, &d2, &d3, &outside] {
            let _ = fs::remove_dir_all(p);
        }
    }

    #[test]
    fn a_failed_rename_leaves_no_temp_file_behind() {
        let d = repo("cleanup");
        // 置き換え先が「中身のあるフォルダ」だと、rename できない
        let dest = d.join(".mrcodereview/out/handoff-security.md");
        fs::create_dir_all(dest.join("inner")).unwrap();
        let err = write(&rec_in(&d), &st(&["fix", "new", "new"]), None, "MrCodeReview").unwrap_err();
        assert!(err.contains("書き出せません"), "{err}");
        assert!(!d.join(".mrcodereview/out/handoff-security.md.tmp").exists(), "書きかけの一時ファイルを残さない");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_stale_temp_file_that_cannot_be_removed_says_why() {
        let d = repo("stale");
        // 一時ファイルの場所が「中身のあるフォルダ」だと、ファイルとして消せない
        let tmp = d.join(".mrcodereview/out/handoff-security.md.tmp");
        fs::create_dir_all(tmp.join("inner")).unwrap();
        let err = write(&rec_in(&d), &st(&["fix", "new", "new"]), None, "MrCodeReview").unwrap_err();
        assert!(err.contains("消せません") && err.contains("handoff-security.md.tmp"), "消せなかった理由を言う: {err}");
        let _ = fs::remove_dir_all(&d);
    }
}
