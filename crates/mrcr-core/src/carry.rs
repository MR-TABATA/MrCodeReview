//! 再レビューへの引き継ぎ（指紋化）。
//!
//! 押し直したとき、前に「待ち」「無視」「対応」にした指摘を、同じものと見分けて仕分けを引き継ぐ。
//! 手がかりは行番号ではなく、**ファイルとモデルが引用したコードの行**。
//! 行番号は、直すとずれる。タイトルや説明文は、モデルが毎回言い回しを変える。
//!
//! - 完全一致（空白の違いは無視）: 「対応」「待ち」「無視」を引き継ぐ。
//! - ほぼ一致（トークンの一致率 90% 以上）: 「待ち」「無視」だけ引き継ぐ。
//!   「対応」を引き継がないのは、引用が少し変わっていたら、直した可能性が高いから。
//! - 同じ引用が複数あるときは、行番号の順に 1 対 1 で対応づける。
//!
//! 前回の「対応」「待ち」の指摘がどうなったかは、**ファイルの現在の内容**で調べる。
//! 差分で見ると、直してコミットした途端に差分が空になり、「消えた＝直った」と誤判定するため。

use crate::findings::Finding;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path};

/// この指摘は、どの過去の指摘から仕分けを引き継いだか。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Carry {
    pub from_review: String,
    pub from_index: usize,
    pub state: String,
    /// 引用が完全に一致したか（false = ほぼ一致）。
    pub exact: bool,
}

/// 前回の指摘が、今回どうなったか。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fate {
    pub from_review: String,
    pub from_index: usize,
    pub title: String,
    pub file: String,
    /// 前回の仕分け（"fix" | "wait"）
    pub state_before: String,
    /// "persists" 今回も指摘された
    /// "gone" 引用の行がファイルから消えた（コードが変わった）
    /// "not_reraised" 引用の行はまだあるが、今回は指摘されなかった
    /// "file_missing" ファイルが無い
    /// "unknown" 引用が空などで調べられない
    pub fate: String,
}

/// 過去の指摘（引き継ぎの候補）。
#[derive(Debug, Clone)]
pub struct Known {
    pub review_id: String,
    pub index: usize,
    pub file: String,
    pub evidence: String,
    pub state: String,
    /// linter の規則名。規則が違う指摘は、同じ行・同じ引用でも別の指摘。
    pub rule: Option<String>,
}

/// ほぼ一致と見なすトークンの一致率の下限と、その判定に要る最小のトークン数（推奨値）。
pub const FUZZY_MIN: f64 = 0.9;
pub const FUZZY_MIN_TOKENS: usize = 4;
const MAX_FILE_READ: u64 = 2_000_000;

/// 空白の並びを 1 つにそろえる。
pub fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 単語はひとかたまり、記号は 1 文字ずつ別のトークンにする。
/// 記号を落とすと、`id` を `?, [id]` に直した行が「ほぼ同じ」に見えてしまう。
fn tokens(s: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut word = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            if !word.is_empty() {
                out.insert(std::mem::take(&mut word));
            }
            if !c.is_whitespace() {
                out.insert(c.to_string());
            }
        }
    }
    if !word.is_empty() {
        out.insert(word);
    }
    out
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.intersection(b).count() as f64 / a.union(b).count() as f64
}

/// 新しい指摘ごとに、引き継ぎ元を決める。`known` は新しい順に並べておく（先に当たったものが優先）。
/// 結果は `new` と同じ並び。
#[cfg_attr(not(test), allow(dead_code))]
pub fn match_known(new: &[Finding], known: &[Known]) -> Vec<Option<Carry>> {
    match_known_with(new, known, FUZZY_MIN, FUZZY_MIN_TOKENS)
}

pub fn match_known_with(new: &[Finding], known: &[Known], fuzzy_min: f64, fuzzy_min_tokens: usize) -> Vec<Option<Carry>> {
    let mut used = vec![false; known.len()];
    let mut out: Vec<Option<Carry>> = vec![None; new.len()];

    // 同じ引用が複数あるとき、行番号の順に 1 対 1 で対応づける。
    let mut order: Vec<usize> = (0..new.len()).collect();
    order.sort_by_key(|&i| (new[i].line.unwrap_or(u32::MAX), i));

    let candidate = |k: usize, used: &[bool]| !used[k] && known[k].state != crate::triage::NEW;

    // 1 周目: 完全一致
    for &i in &order {
        let ev = norm(&new[i].evidence);
        if ev.is_empty() {
            continue;
        }
        let hit = (0..known.len()).find(|&k| {
            candidate(k, &used)
                && crate::findings::same_file(&known[k].file, &new[i].file)
                && known[k].rule == new[i].rule
                && norm(&known[k].evidence) == ev
        });
        if let Some(k) = hit {
            used[k] = true;
            out[i] = Some(Carry {
                from_review: known[k].review_id.clone(),
                from_index: known[k].index,
                state: known[k].state.clone(),
                exact: true,
            });
        }
    }

    // 2 周目: ほぼ一致（「待ち」「無視」だけ）
    for &i in &order {
        if out[i].is_some() {
            continue;
        }
        let ev = tokens(&norm(&new[i].evidence));
        if ev.len() < fuzzy_min_tokens {
            continue;
        }
        let hit = (0..known.len()).find(|&k| {
            candidate(k, &used)
                && matches!(known[k].state.as_str(), crate::triage::WAIT | crate::triage::DISMISS)
                && known[k].rule == new[i].rule
                && crate::findings::same_file(&known[k].file, &new[i].file)
                && jaccard(&ev, &tokens(&norm(&known[k].evidence))) >= fuzzy_min
        });
        if let Some(k) = hit {
            used[k] = true;
            out[i] = Some(Carry {
                from_review: known[k].review_id.clone(),
                from_index: known[k].index,
                state: known[k].state.clone(),
                exact: false,
            });
        }
    }
    out
}

/// 引用から、diff の行頭記号（`+` / `-`）を外したもの。モデルは diff の行をそのまま引用することがある。
fn strip_markers(ev: &str) -> String {
    ev.lines()
        .map(|l| l.strip_prefix('+').or_else(|| l.strip_prefix('-')).unwrap_or(l))
        .collect::<Vec<_>>()
        .join("\n")
}

/// リポジトリの中のファイルを読む。`None` = 調べられない、`Some(None)` = ファイルが無い。
/// モデルが返したパスは信用しない。`..` や絶対パスに加えて、**リポジトリの外を指す symlink も辿らない**
/// （リンク先を読むと、外のファイルに「引用の行があるか」を漏らす）。
pub(crate) fn read_in_repo(root: &Path, file: &str) -> Option<Option<String>> {
    let rel = Path::new(file.trim_start_matches("./").trim_start_matches("a/").trim_start_matches("b/"));
    if file.is_empty() || rel.is_absolute() || rel.components().any(|c| matches!(c, Component::ParentDir)) {
        return None;
    }
    let Ok(root_real) = root.canonicalize() else { return None };
    // 実体を解決する（途中のディレクトリが symlink でも）。無ければ「ファイルが無い」
    let Ok(real) = root.join(rel).canonicalize() else { return Some(None) };
    if !real.starts_with(&root_real) {
        return None;
    }
    match std::fs::metadata(&real) {
        Ok(m) if !m.is_file() || m.len() > MAX_FILE_READ => None,
        Ok(_) => Some(std::fs::read(&real).ok().map(|b| String::from_utf8_lossy(&b).into_owned())),
        Err(_) => Some(None),
    }
}

/// 前回の「対応」「待ち」の指摘が、今回どうなったか。
/// `carried` は、今回の指摘が引き継ぎ元にした前回の指摘の番号。
pub fn fates(
    prev_id: &str,
    prev_findings: &[Finding],
    prev_triage: &[String],
    carried: &HashSet<usize>,
    root: &Path,
) -> Vec<Fate> {
    let mut out = Vec::new();
    for (i, f) in prev_findings.iter().enumerate() {
        let state = prev_triage.get(i).map(String::as_str).unwrap_or(crate::triage::NEW);
        if !matches!(state, crate::triage::FIX | crate::triage::WAIT) {
            continue;
        }
        let fate = if carried.contains(&i) {
            "persists"
        } else if norm(&f.evidence).is_empty() {
            "unknown"
        } else {
            match read_in_repo(root, &f.file) {
                None => "unknown",
                Some(None) => "file_missing",
                Some(Some(text)) => {
                    let body = norm(&text);
                    if body.contains(&norm(&f.evidence)) || body.contains(&norm(&strip_markers(&f.evidence))) {
                        "not_reraised"
                    } else {
                        "gone"
                    }
                }
            }
        };
        out.push(Fate {
            from_review: prev_id.to_string(),
            from_index: i,
            title: f.title.clone(),
            file: f.file.clone(),
            state_before: state.to_string(),
            fate: fate.to_string(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(file: &str, line: Option<u32>, evidence: &str) -> Finding {
        Finding {
            file: file.into(),
            line,
            severity: "high".into(),
            title: "t".into(),
            detail: "".into(),
            evidence: evidence.into(),
            verified: true,
            rule: None,
            ..Default::default()
        }
    }

    fn k(review: &str, index: usize, file: &str, evidence: &str, state: &str) -> Known {
        Known { review_id: review.into(), index, file: file.into(), evidence: evidence.into(), state: state.into(), rule: None }
    }

    #[test]
    fn jaccard_basics() {
        let a = tokens("a b c d");
        assert_eq!(jaccard(&a, &a), 1.0);
        assert_eq!(jaccard(&a, &tokens("x y")), 0.0);
        assert_eq!(jaccard(&a, &HashSet::new()), 0.0);
        // 4 つのうち 3 つが共通 → 3 / 5
        assert!((jaccard(&a, &tokens("a b c e")) - 0.6).abs() < 1e-9);
    }

    #[test]
    fn tokens_keep_punctuation_so_a_fix_does_not_look_the_same() {
        let old = tokens(r#"return db.query("SELECT * FROM u WHERE id=" + id);"#);
        let fixed = tokens(r#"return db.query("SELECT * FROM u WHERE id=?", [id]);"#);
        assert!(jaccard(&old, &fixed) < FUZZY_MIN, "直した行を同じ指摘と見なさない");
    }

    #[test]
    fn exact_match_survives_line_shift_and_whitespace_changes() {
        let known = vec![k("r0", 1, "db.js", "return  db.query(x);", "wait")];
        let c = match_known(&[f("db.js", Some(40), "return db.query(x);")], &known);
        let c = c[0].as_ref().unwrap();
        assert_eq!((c.from_review.as_str(), c.from_index, c.state.as_str(), c.exact), ("r0", 1, "wait", true));
    }

    #[test]
    fn different_file_or_empty_evidence_never_matches() {
        let known = vec![k("r0", 0, "db.js", "x = 1", "dismiss")];
        assert!(match_known(&[f("other.js", None, "x = 1")], &known)[0].is_none());
        assert!(match_known(&[f("db.js", None, "")], &[k("r0", 0, "db.js", "", "dismiss")])[0].is_none());
    }

    #[test]
    fn new_state_is_not_inherited() {
        let known = vec![k("r0", 0, "db.js", "x = 1", "new")];
        assert!(match_known(&[f("db.js", None, "x = 1")], &known)[0].is_none());
    }

    #[test]
    fn fuzzy_inherits_wait_and_dismiss_but_not_fix() {
        let old = "const total = items.reduce((a, b) => a + b.price, 0) + shipping + tax + fee;";
        let new = "const total = items.reduce((a, b) => a + b.price, 0) + shipping + tax + fees;";
        // 1 トークンだけ違う（総数が多いので一致率は 90% 以上）
        assert!(jaccard(&tokens(old), &tokens(new)) >= FUZZY_MIN);
        for (state, expect) in [("wait", true), ("dismiss", true), ("fix", false)] {
            let c = match_known(&[f("a.js", None, new)], &[k("r0", 0, "a.js", old, state)]);
            assert_eq!(c[0].is_some(), expect, "{state}");
            if expect {
                assert!(!c[0].as_ref().unwrap().exact);
            }
        }
    }

    #[test]
    fn short_evidence_is_exact_only() {
        let c = match_known(&[f("a.js", None, "x = 2")], &[k("r0", 0, "a.js", "x = 1", "dismiss")]);
        assert!(c[0].is_none());
    }

    #[test]
    fn duplicates_pair_one_to_one_in_line_order() {
        // 同じ引用が 2 つ。既知は 1 つだけ → 行番号の小さいほうだけが引き継ぐ
        let new = [f("a.js", Some(30), "danger()"), f("a.js", Some(10), "danger()")];
        let c = match_known(&new, &[k("r0", 0, "a.js", "danger()", "dismiss")]);
        assert!(c[0].is_none());
        assert!(c[1].is_some());
        // 既知が 2 つなら、両方引き継ぐ
        let c = match_known(&new, &[k("r0", 0, "a.js", "danger()", "dismiss"), k("r0", 1, "a.js", "danger()", "wait")]);
        assert!(c[0].is_some() && c[1].is_some());
        assert_eq!(c[1].as_ref().unwrap().from_index, 0, "行番号の小さい指摘が、先に並んだ既知を取る");
    }

    #[test]
    fn newest_known_wins() {
        // known は新しい順。先に当たった（新しい）ほうの仕分けを使う
        let known = vec![k("new", 0, "a.js", "x = 1", "dismiss"), k("old", 0, "a.js", "x = 1", "wait")];
        let c = match_known(&[f("a.js", None, "x = 1")], &known);
        assert_eq!(c[0].as_ref().unwrap().state, "dismiss");
    }

    #[test]
    fn different_rule_on_the_same_line_is_a_different_finding() {
        let with_rule = |r: &str| {
            let mut x = f("a.js", Some(3), "var x = 1;");
            x.rule = Some(r.into());
            x
        };
        let mut known = k("r0", 0, "a.js", "var x = 1;", "dismiss");
        known.rule = Some("no-var".into());
        let c = match_known(&[with_rule("prefer-const"), with_rule("no-var")], &[known.clone()]);
        assert!(c[0].is_none(), "規則が違えば引き継がない");
        assert!(c[1].is_some());
        // LLM の指摘（規則なし）と、linter の指摘（規則あり）は、同じ行・同じ引用でも別
        assert!(match_known(&[f("a.js", Some(3), "var x = 1;")], &[known])[0].is_none());
    }

    #[test]
    fn symlinks_pointing_outside_the_repo_are_not_followed_when_judging_fates() {
        use std::os::unix::fs::symlink;
        let base = std::env::temp_dir().join(format!("mrcr-carry-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("repo");
        let outside = base.join("outside");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.js"), "danger(x);\n").unwrap();
        std::fs::write(repo.join("src/real.js"), "danger(x);\n").unwrap();
        symlink(outside.join("secret.js"), repo.join("src/leak.js")).unwrap(); // 外を指す
        symlink(repo.join("src/real.js"), repo.join("src/alias.js")).unwrap(); // 中を指す
        symlink(&outside, repo.join("linkdir")).unwrap(); // 外のディレクトリ
        let prev = vec![
            f("src/leak.js", Some(1), "danger(x);"),
            f("src/alias.js", Some(1), "danger(x);"),
            f("linkdir/secret.js", Some(1), "danger(x);"),
            f("src/real.js", Some(1), "danger(x);"),
        ];
        let triage: Vec<String> = vec!["fix".into(); 4];
        let out = fates("r0", &prev, &triage, &HashSet::new(), &repo);
        let by = |i: usize| out.iter().find(|x| x.from_index == i).map(|x| x.fate.as_str());
        assert_eq!(by(0), Some("unknown"), "外を指す symlink は読まない");
        assert_eq!(by(1), Some("not_reraised"), "リポジトリの中を指す symlink は、実体を読める");
        assert_eq!(by(2), Some("unknown"), "外のディレクトリを指す symlink の下も、読まない");
        assert_eq!(by(3), Some("not_reraised"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn fates_use_file_content_not_the_diff() {
        let dir = std::env::temp_dir().join(format!("mrcr-carry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/a.js"), "function a() {\n  danger( x );\n}\n").unwrap();

        let prev = vec![
            f("src/a.js", Some(2), "danger(x);"),          // 0: 引用が残っていない（空白だけ違う別の形）→ gone
            f("src/a.js", Some(2), "danger( x );"),        // 1: 引用がまだある → not_reraised
            f("src/a.js", Some(2), "+  danger( x );"),     // 2: diff の行頭記号つきの引用 → not_reraised
            f("src/missing.js", Some(1), "x"),             // 3: ファイルが無い
            f("src/a.js", Some(2), ""),                    // 4: 引用が空 → unknown
            f("src/a.js", Some(2), "danger( x );"),        // 5: 今回も指摘された → persists
            f("src/a.js", Some(2), "danger( x );"),        // 6: 前回の仕分けが「無視」→ 対象外
            f("../etc/passwd", Some(1), "root"),           // 7: 外のパスは読まない → unknown
        ];
        let triage: Vec<String> = ["fix", "wait", "fix", "wait", "fix", "fix", "dismiss", "fix"].map(String::from).to_vec();
        let carried: HashSet<usize> = [5].into_iter().collect();
        let out = fates("r0", &prev, &triage, &carried, &dir);
        let by = |i: usize| out.iter().find(|x| x.from_index == i).map(|x| x.fate.as_str());
        assert_eq!(by(0), Some("gone"));
        assert_eq!(by(1), Some("not_reraised"));
        assert_eq!(by(2), Some("not_reraised"));
        assert_eq!(by(3), Some("file_missing"));
        assert_eq!(by(4), Some("unknown"));
        assert_eq!(by(5), Some("persists"));
        assert_eq!(by(6), None);
        assert_eq!(by(7), Some("unknown"));
        assert_eq!(out.iter().find(|x| x.from_index == 0).unwrap().state_before, "fix");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
