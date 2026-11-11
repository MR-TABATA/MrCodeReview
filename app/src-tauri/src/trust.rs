//! 「このリポジトリでは、コードを動かす linter（ESLint・Clippy）を走らせてよい」と、あなたが許可したリポジトリの記録。
//!
//! - **あなた個人のデータ領域に置く**（`trusted_repos.json`）。リポジトリの中のファイルでは、許可できない
//!   （信頼できないリポジトリが、自分で許可を出せないようにする）。
//! - ファイルが壊れていれば、許可は無いものとして扱う（安全な側）。理由は `status` で画面に出す。

use std::path::Path;

const FILE: &str = "trusted_repos.json";

fn path(dir: &Path) -> std::path::PathBuf {
    dir.join(FILE)
}

/// 許可したリポジトリ（リポジトリの根の絶対パス）。ファイルが無ければ空。
pub fn list(dir: &Path) -> Result<Vec<String>, String> {
    let p = path(dir);
    match std::fs::read_to_string(&p) {
        Ok(t) => serde_json::from_str::<Vec<String>>(&t).map_err(|e| format!("{}（{}）が正しくありません: {e}。許可は無いものとして扱います", FILE, p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{} を読めません: {e}。許可は無いものとして扱います", p.display())),
    }
}

pub fn is_trusted(dir: &Path, repo: &str) -> bool {
    list(dir).is_ok_and(|l| l.iter().any(|r| r == repo))
}

fn save(dir: &Path, list: &[String]) -> Result<(), String> {
    let p = path(dir);
    std::fs::create_dir_all(dir).map_err(|e| format!("{} を作れません: {e}", dir.display()))?;
    let tmp = dir.join(format!("{FILE}.tmp"));
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| format!("{} に書けません: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &p).map_err(|e| format!("{} に書けません: {e}", p.display()))
}

/// 許可する。ファイルが壊れているときは、壊れたまま上書きせず、止める（別の許可を巻き込んで消さないため）。
pub fn trust(dir: &Path, repo: &str) -> Result<(), String> {
    let mut l = list(dir)?;
    if !l.iter().any(|r| r == repo) {
        l.push(repo.to_string());
        save(dir, &l)?;
    }
    Ok(())
}

pub fn untrust(dir: &Path, repo: &str) -> Result<(), String> {
    let mut l = list(dir)?;
    let before = l.len();
    l.retain(|r| r != repo);
    if l.len() != before {
        save(dir, &l)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mrcr-trust-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn trust_is_remembered_per_repo_and_can_be_revoked() {
        let d = scratch("basic");
        assert!(!is_trusted(&d, "/r/a") && list(&d).unwrap().is_empty(), "ファイルが無ければ、許可は無い");
        trust(&d, "/r/a").unwrap();
        trust(&d, "/r/a").unwrap();
        trust(&d, "/r/b").unwrap();
        assert_eq!(list(&d).unwrap(), ["/r/a", "/r/b"], "重ねて許可しても、1 件");
        assert!(is_trusted(&d, "/r/a") && !is_trusted(&d, "/r/c"));
        assert!(!is_trusted(&d, "/r/a/sub"), "親が許可でも、別のパスは許可ではない");
        untrust(&d, "/r/a").unwrap();
        assert_eq!(list(&d).unwrap(), ["/r/b"]);
        untrust(&d, "/r/zzz").unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_broken_file_means_no_trust_and_is_never_overwritten() {
        let d = scratch("broken");
        std::fs::write(path(&d), "{ not json").unwrap();
        assert!(!is_trusted(&d, "/r/a"), "壊れていれば、許可は無い（安全な側）");
        assert!(list(&d).unwrap_err().contains("正しくありません"));
        assert!(trust(&d, "/r/a").is_err(), "壊れたファイルを、黙って上書きしない");
        assert_eq!(std::fs::read_to_string(path(&d)).unwrap(), "{ not json");
        let _ = std::fs::remove_dir_all(&d);
    }
}
