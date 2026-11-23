//! Git の post-commit フック。GUI（と CLI）から、入れる・外す・設定する。
//!
//! - **設定は、アプリのデータ領域の `hooks.json`**（リポジトリごとに、キーはリポジトリの絶対パス）。
//!   リポジトリの中には置かない。clone したリポジトリの設定で、コミットのたびに実行される内容が変わらないようにする。
//! - フックのスクリプトには、`mrcr` の絶対パスと `hook run post-commit` だけを書く。ON/OFF・観点は、実行時に `hooks.json` から読む。
//!   GUI が起動していなくても動く。
//! - フックは、コミットを止めない（バックグラウンドで走らせる）。結果は、通常のレビュー履歴に入り、GUI で見られる。
//!   失敗（API キーが無い、など）は、`hooks.log` に残し、GUI が見せる。
//! - 既存のフックは、壊さない。`# >>> mrcr hook` から `# <<< mrcr hook` までの範囲だけを、足す・外す。
//!   `core.hooksPath` で `.git` の外（husky など）を使っているときは、書かない。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const FILE: &str = "hooks.json";
pub const LOG: &str = "hooks.log";
pub const HOOK_NAME: &str = "post-commit";
pub const DEFAULT_RECIPES: &[&str] = &["bugs"];

const BEGIN: &str = "# >>> mrcr hook (MrCodeReview が管理。この範囲は、書き換えないでください) >>>";
const END: &str = "# <<< mrcr hook <<<";
const LOG_MAX_BYTES: u64 = 256 * 1024;
const LOG_KEEP_LINES: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoHook {
    pub enabled: bool,
    pub recipes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HooksConfig {
    /// キー: リポジトリの絶対パス（`git rev-parse --show-toplevel` を正規化したもの）
    pub post_commit: BTreeMap<String, RepoHook>,
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// 無ければ空の設定。読めない・正しくないときは、黙って空にせず、理由つきのエラー。
pub fn load(dir: &Path) -> Result<HooksConfig, String> {
    let p = path(dir);
    if !p.exists() {
        return Ok(HooksConfig::default());
    }
    let text = std::fs::read_to_string(&p).map_err(|e| format!("フックの設定を読めません（{}）: {e}", p.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("フックの設定 {} が正しくありません: {e}", p.display()))
}

/// 一時ファイルに書いてから置き換える（途中で止まっても、元のファイルを壊さない）。
pub fn save(dir: &Path, cfg: &HooksConfig) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("データの置き場を作れません（{}）: {e}", dir.display()))?;
    let p = path(dir);
    let tmp = dir.join(format!("{FILE}.tmp"));
    let text = serde_json::to_string_pretty(cfg).map_err(|e| format!("フックの設定を書き出せません: {e}"))?;
    std::fs::write(&tmp, text + "\n").map_err(|e| format!("フックの設定を書けません（{}）: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &p).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("フックの設定を置き換えられません（{}）: {e}", p.display())
    })
}

/// 観点の id を確かめる。空・長すぎる・使えない文字は、理由つきのエラー（スクリプトやコマンドへ渡すので、厳しめ）。
pub fn validate_recipes(recipes: &[String]) -> Result<(), String> {
    if recipes.is_empty() {
        return Err("観点が 1 つも選ばれていません".into());
    }
    for r in recipes {
        let ok = !r.is_empty() && r.len() <= 40 && r.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !ok {
            return Err(format!("観点の名前 `{}` は使えません（英数字・ハイフン・下線だけ。40 文字まで）", r.chars().take(40).collect::<String>()));
        }
    }
    Ok(())
}

/// `mrcr` の実行ファイル。環境変数 `MRCR_CLI`（試験用）→ PATH → Homebrew・npm の置き場、の順。
/// Finder から起動した GUI は PATH が短いので、よくある置き場も見る。
///
/// - Homebrew・npm どちらでも、**シンボリックリンクのまま**返す（実体は、版つきの場所で、更新で消える）。
/// - npm で入れたものは、Node の起動役（`mrcr.js`）ではなく、**本体のバイナリ**を返す（フックの時点で `node` が PATH に無くても動く）。
/// - `npx` のキャッシュ（`/_npx/`）の中は、あとで消えるので、候補にしない。
pub fn find_cli() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MRCR_CLI").map(PathBuf::from) {
        return p.is_file().then_some(p);
    }
    pick_cli(cli_candidates())
}

fn cli_candidates() -> Vec<PathBuf> {
    let mut c: Vec<PathBuf> = std::env::var_os("PATH").map(|v| std::env::split_paths(&v).map(|d| d.join("mrcr")).collect()).unwrap_or_default();
    c.push(PathBuf::from("/opt/homebrew/bin/mrcr"));
    c.push(PathBuf::from("/usr/local/bin/mrcr"));
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        c.push(home.join(".npm-global/bin/mrcr"));
        c.push(home.join(".volta/bin/mrcr"));
        // nvm: 入っている Node の版ごとの置き場（新しい版を先に）
        if let Ok(rd) = std::fs::read_dir(home.join(".nvm/versions/node")) {
            let mut dirs: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            dirs.sort();
            dirs.reverse();
            c.extend(dirs.into_iter().map(|d| d.join("bin/mrcr")));
        }
    }
    c
}

/// 候補のうち、最初に使えるもの。`npx` のキャッシュの中は飛ばす。
pub fn pick_cli(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().filter(|p| !p.to_string_lossy().contains("/_npx/")).find(|p| p.is_file()).map(|p| native_of(&p))
}

/// npm の起動役（`mrcr.js`）なら、同じ npm の置き場にある本体のバイナリ（`@mr-tabata/mrcr-darwin`）を返す。
/// 見つからなければ（本体が無い・npm ではない）、そのまま返す。
pub fn native_of(cli: &Path) -> PathBuf {
    let Ok(real) = cli.canonicalize() else { return cli.to_path_buf() };
    if real.extension().and_then(|e| e.to_str()) != Some("js") {
        return cli.to_path_buf();
    }
    // <パッケージ>/bin/mrcr.js → パッケージの根は 2 つ上。本体は、入れ子（<根>/node_modules/…）か、並び（<根>/../mrcr-darwin）にある
    let Some(root) = real.parent().and_then(|b| b.parent()) else { return cli.to_path_buf() };
    let nested = root.join("node_modules/@mr-tabata/mrcr-darwin/bin/mrcr");
    let sibling = root.parent().map(|p| p.join("mrcr-darwin/bin/mrcr"));
    [Some(nested), sibling].into_iter().flatten().find(|p| p.is_file()).unwrap_or_else(|| cli.to_path_buf())
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git").arg("-C").arg(root).args(args).output().map_err(|e| format!("git を起動できません: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// リポジトリの正規化した絶対パス（設定のキー）。
pub fn repo_key(repo: &str) -> Result<String, String> {
    let top = git(Path::new(repo), &["rev-parse", "--show-toplevel"])?;
    Ok(Path::new(&top).canonicalize().map_err(|e| format!("リポジトリのパスを正規化できません（{top}）: {e}"))?.to_string_lossy().into_owned())
}

/// フックを置くフォルダ。`core.hooksPath` などで `.git` の外（追跡されているフォルダ）を指しているときは、書かないのでエラー。
pub fn hooks_dir(root: &Path) -> Result<PathBuf, String> {
    let rel = git(root, &["rev-parse", "--git-path", "hooks"])?;
    let dir = if Path::new(&rel).is_absolute() { PathBuf::from(&rel) } else { root.join(&rel) };
    let common = git(root, &["rev-parse", "--git-common-dir"])?;
    let common = if Path::new(&common).is_absolute() { PathBuf::from(common) } else { root.join(common) };
    let common = common.canonicalize().map_err(|e| format!(".git の場所を正規化できません: {e}"))?;
    let canon_dir = if dir.exists() { dir.canonicalize().map_err(|e| format!("フックの置き場を正規化できません: {e}"))? } else { dir.clone() };
    if !canon_dir.starts_with(&common) {
        return Err(format!(
            "フックの置き場が .git の外です（{}）。core.hooksPath を使う仕組み（husky など）の管理下では、自動では書きません",
            dir.display()
        ));
    }
    Ok(dir)
}

fn single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

const PATH_TAG: &str = "# mrcr-path: ";

/// 記録したパスが実行できなければ、PATH 上の `mrcr` を使う（更新で場所が変わったとき）。どちらも無ければ、何も起きない（`status` が気づかせる）。
fn block(cli: &Path) -> String {
    let q = single_quote(&cli.to_string_lossy());
    format!(
        "{BEGIN}\n{PATH_TAG}{}\n( if [ -x {q} ]; then {q} hook run {HOOK_NAME}; elif command -v mrcr >/dev/null 2>&1; then mrcr hook run {HOOK_NAME}; fi >/dev/null 2>&1 & )\n{END}\n",
        cli.to_string_lossy()
    )
}

/// フックに記録した `mrcr` のパス（無ければ `None`）。
pub fn recorded_cli(hook_text: &str) -> Option<PathBuf> {
    hook_text.lines().find_map(|l| l.strip_prefix(PATH_TAG)).map(|p| PathBuf::from(p.trim()))
}

fn has_block(text: &str) -> bool {
    text.contains(BEGIN) && text.contains(END)
}

fn strip_block(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in text.split_inclusive('\n') {
        let t = line.trim_end_matches(['\n', '\r']);
        if t == BEGIN {
            inside = true;
            continue;
        }
        if inside {
            if t == END {
                inside = false;
            }
            continue;
        }
        out.push_str(line);
    }
    out
}

fn is_shell_shebang(first: &str) -> bool {
    let l = first.trim();
    l.starts_with("#!")
        && ["/sh", "/bash", "/zsh", "/dash", " sh", " bash", " zsh"].iter().any(|s| l.ends_with(s) || l.contains(&format!("{s} ")))
}

/// フックに、管理のブロックを入れる（すでにあれば、パスだけ更新）。既存の内容は、そのまま残す。
/// ブロックは、1 行目（`#!`）の直後に入れる（既存の末尾に `exit` があっても、先に走るように。コミットは止めない）。
pub fn install_block(hooks_dir: &Path, cli: &Path) -> Result<(), String> {
    if cli.to_string_lossy().contains(['\n', '\r']) {
        return Err("mrcr のパスに改行が含まれています。フックには書けません".into());
    }
    std::fs::create_dir_all(hooks_dir).map_err(|e| format!("フックの置き場を作れません（{}）: {e}", hooks_dir.display()))?;
    let file = hooks_dir.join(HOOK_NAME);
    let new_block = block(cli);
    let text = if file.exists() {
        let existing = std::fs::read_to_string(&file).map_err(|e| format!("既存のフックを読めません（{}）: {e}", file.display()))?;
        let base = strip_block(&existing);
        let (first, rest) = match base.split_once('\n') {
            Some((f, r)) => (f.to_string(), r.to_string()),
            None => (base.clone(), String::new()),
        };
        if !is_shell_shebang(&first) {
            return Err(format!(
                "既存の {} は、シェルスクリプトではないようです（1 行目: `{}`）。壊さないよう、書きません",
                file.display(),
                first.chars().take(60).collect::<String>()
            ));
        }
        format!("{first}\n{new_block}{rest}")
    } else {
        format!("#!/bin/sh\n{new_block}")
    };
    std::fs::write(&file, text).map_err(|e| format!("フックを書けません（{}）: {e}", file.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&file).map_err(|e| format!("フックの権限を読めません: {e}"))?.permissions();
        perm.set_mode(perm.mode() | 0o755);
        std::fs::set_permissions(&file, perm).map_err(|e| format!("フックを実行できる権限にできません: {e}"))?;
    }
    Ok(())
}

/// 管理のブロックだけを外す。外したあとに `#!` の行しか残らなければ、ファイルごと消す。無ければ何もしない（`false`）。
pub fn uninstall_block(hooks_dir: &Path) -> Result<bool, String> {
    let file = hooks_dir.join(HOOK_NAME);
    if !file.exists() {
        return Ok(false);
    }
    let text = std::fs::read_to_string(&file).map_err(|e| format!("既存のフックを読めません（{}）: {e}", file.display()))?;
    if !has_block(&text) {
        return Ok(false);
    }
    let rest = strip_block(&text);
    let only_shebang = rest.lines().filter(|l| !l.trim().is_empty()).all(|l| l.starts_with("#!"));
    if only_shebang {
        std::fs::remove_file(&file).map_err(|e| format!("フックを消せません（{}）: {e}", file.display()))?;
    } else {
        std::fs::write(&file, rest).map_err(|e| format!("フックを書けません（{}）: {e}", file.display()))?;
    }
    Ok(true)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LastRun {
    pub at_ms: u64,
    /// 0=指摘なし・1=指摘あり・2=実行の失敗
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    pub repo: String,
    /// 設定の ON/OFF
    pub enabled: bool,
    pub recipes: Vec<String>,
    /// フックのスクリプトに、管理のブロックが入っているか
    pub installed: bool,
    /// いま見つかる `mrcr`（無ければ、画面で案内する）
    pub cli: Option<String>,
    /// フックの置き場に書けない理由（husky など）。あれば、入れられない
    pub blocked: Option<String>,
    /// フックは入っているが、記録した `mrcr` が無く、代わりに使える `mrcr` も見つからない（更新・削除で）。入れ直す必要がある
    pub target_missing: bool,
    pub last_run: Option<LastRun>,
}

pub fn status(dir: &Path, repo: &str) -> Result<Status, String> {
    let key = repo_key(repo)?;
    let cfg = load(dir)?;
    let entry = cfg.post_commit.get(&key);
    let cli = find_cli();
    let (installed, blocked, recorded) = match hooks_dir(Path::new(&key)) {
        Ok(h) => {
            let text = std::fs::read_to_string(h.join(HOOK_NAME)).ok().filter(|t| has_block(t));
            (text.is_some(), None, text.as_deref().and_then(recorded_cli))
        }
        Err(e) => (false, Some(e), None),
    };
    let target_missing = installed && cli.is_none() && !recorded.as_ref().map(|p| p.is_file()).unwrap_or(false);
    Ok(Status {
        repo: key.clone(),
        enabled: entry.map(|e| e.enabled).unwrap_or(false),
        recipes: entry.map(|e| e.recipes.clone()).unwrap_or_else(|| DEFAULT_RECIPES.iter().map(|s| s.to_string()).collect()),
        installed,
        cli: cli.map(|p| p.to_string_lossy().into_owned()),
        blocked,
        target_missing,
        last_run: last_run(dir, &key),
    })
}

/// ON/OFF と観点を設定する。ON なら、フックを入れる（`mrcr` が見つからなければエラー）。OFF なら、外す（観点は残す）。
pub fn set(dir: &Path, repo: &str, enabled: bool, recipes: &[String]) -> Result<Status, String> {
    validate_recipes(recipes)?;
    let key = repo_key(repo)?;
    let mut cfg = load(dir)?;
    let hooks = hooks_dir(Path::new(&key));
    if enabled {
        let cli = find_cli().ok_or_else(|| "mrcr（CLI）が見つかりません。入れてから、もう一度 ON にしてください（例: brew install mr-tabata/tap/mrcr、または npm i -g @mr-tabata/mrcr）。npx で起動した mrcr は、実行ファイルが一時のキャッシュにあり、あとで消えるので、フックには使えません".to_string())?;
        install_block(&hooks?, &cli)?;
    } else if let Ok(h) = &hooks {
        uninstall_block(h)?;
    }
    cfg.post_commit.insert(key.clone(), RepoHook { enabled, recipes: recipes.to_vec() });
    if let Err(e) = save(dir, &cfg) {
        if enabled {
            if let Ok(h) = hook_dir_quiet(&key) {
                let _ = uninstall_block(&h);
            }
        }
        return Err(e);
    }
    status(dir, &key)
}

fn hook_dir_quiet(key: &str) -> Result<PathBuf, String> {
    hooks_dir(Path::new(key))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[derive(Serialize, Deserialize)]
struct LogLine {
    at_ms: u64,
    repo: String,
    code: i32,
    message: String,
}

/// 実行結果を `hooks.log`（1 行 1 件の JSON）に足す。大きくなったら、新しい側だけ残す。
pub fn append_log(dir: &Path, repo: &str, code: i32, message: &str) -> Result<(), String> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|e| format!("データの置き場を作れません: {e}"))?;
    let p = dir.join(LOG);
    let line = serde_json::to_string(&LogLine { at_ms: now_ms(), repo: repo.into(), code, message: message.chars().take(500).collect() })
        .map_err(|e| format!("ログを書き出せません: {e}"))?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&p).map_err(|e| format!("ログを開けません（{}）: {e}", p.display()))?;
    writeln!(f, "{line}").map_err(|e| format!("ログを書けません: {e}"))?;
    if std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) > LOG_MAX_BYTES {
        if let Ok(text) = std::fs::read_to_string(&p) {
            let lines: Vec<&str> = text.lines().collect();
            let keep = lines[lines.len().saturating_sub(LOG_KEEP_LINES)..].join("\n") + "\n";
            let _ = std::fs::write(&p, keep);
        }
    }
    Ok(())
}

/// そのリポジトリの、最新の実行結果（無ければ `None`。読めない行は飛ばす）。
pub fn last_run(dir: &Path, repo: &str) -> Option<LastRun> {
    let text = std::fs::read_to_string(dir.join(LOG)).ok()?;
    text.lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<LogLine>(l).ok())
        .find(|l| l.repo == repo)
        .map(|l| LastRun { at_ms: l.at_ms, code: l.code, message: l.message })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 環境変数（MRCR_CLI）を触る試験は、同時に走らせない
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 試験用の一時フォルダ。落とすとき（試験の終わり）に消す。
    struct Tmp(PathBuf);
    impl std::ops::Deref for Tmp {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tmp(tag: &str) -> Tmp {
        let d = std::env::temp_dir().join(format!("mrcr-hooks-{tag}-{}-{}", std::process::id(), now_ms()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Tmp(d)
    }

    /// 空のコミットが 1 つあるリポジトリ。(後始末の持ち主, 正規化したパス)
    fn git_repo(tag: &str) -> (Tmp, PathBuf) {
        let d = tmp(tag);
        for args in [&["init", "-q", "-b", "main"][..], &["-c", "user.name=t", "-c", "user.email=t@e.com", "commit", "-q", "--allow-empty", "-m", "i"]] {
            assert!(Command::new("git").arg("-C").arg(&*d).args(args).output().unwrap().status.success());
        }
        let canon = d.canonicalize().unwrap();
        (d, canon)
    }

    #[test]
    fn recipes_are_validated_strictly() {
        assert!(validate_recipes(&["bugs".into(), "tf-state".into(), "my_recipe".into()]).is_ok());
        assert!(validate_recipes(&[]).is_err());
        for bad in ["", "a b", "a;b", "$(x)", "a/b", "日本語", &"x".repeat(41)] {
            assert!(validate_recipes(&[bad.to_string()]).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn config_round_trips_and_a_broken_file_is_an_error_not_an_empty_config() {
        let d = tmp("cfg");
        assert_eq!(load(&d).unwrap(), HooksConfig::default());
        let mut c = HooksConfig::default();
        c.post_commit.insert("/r".into(), RepoHook { enabled: true, recipes: vec!["bugs".into()] });
        save(&d, &c).unwrap();
        assert_eq!(load(&d).unwrap(), c);
        assert!(!d.join("hooks.json.tmp").exists());
        std::fs::write(path(&d), "{ nope").unwrap();
        assert!(load(&d).unwrap_err().contains("正しくありません"));
        std::fs::write(path(&d), r#"{"post_commit":{},"unknown":1}"#).unwrap();
        assert!(load(&d).is_err(), "知らない項目は、黙って捨てない");
    }

    #[test]
    fn install_creates_a_hook_and_uninstall_removes_the_file_again() {
        let d = tmp("inst");
        install_block(&d, Path::new("/opt/x/mrcr")).unwrap();
        let t = std::fs::read_to_string(d.join("post-commit")).unwrap();
        assert!(t.starts_with("#!/bin/sh\n") && t.contains("'/opt/x/mrcr' hook run post-commit") && has_block(&t), "{t}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert!(std::fs::metadata(d.join("post-commit")).unwrap().permissions().mode() & 0o111 != 0);
        }
        assert!(uninstall_block(&d).unwrap());
        assert!(!d.join("post-commit").exists(), "自分のブロックだけだったので、ファイルごと消す");
        assert!(!uninstall_block(&d).unwrap(), "2 回目は、何もしない");
    }

    #[test]
    fn an_existing_hook_is_kept_and_installing_twice_does_not_duplicate_the_block() {
        let d = tmp("keep");
        std::fs::write(d.join("post-commit"), "#!/bin/sh\necho mine\nexit 0\n").unwrap();
        install_block(&d, Path::new("/a/mrcr")).unwrap();
        install_block(&d, Path::new("/b/mrcr")).unwrap();
        let t = std::fs::read_to_string(d.join("post-commit")).unwrap();
        assert_eq!(t.matches(BEGIN).count(), 1, "{t}");
        assert!(t.contains("'/b/mrcr'") && !t.contains("'/a/mrcr'"), "パスは更新される: {t}");
        assert!(t.contains("echo mine\nexit 0\n"), "既存の内容はそのまま: {t}");
        assert!(t.find(BEGIN).unwrap() < t.find("exit 0").unwrap(), "既存の exit より前に入る: {t}");
        assert!(uninstall_block(&d).unwrap());
        assert_eq!(std::fs::read_to_string(d.join("post-commit")).unwrap(), "#!/bin/sh\necho mine\nexit 0\n", "外すと、元の内容に戻る");
    }

    #[test]
    fn a_non_shell_hook_is_refused_and_left_untouched() {
        let d = tmp("py");
        let original = "#!/usr/bin/env python3\nprint('hi')\n";
        std::fs::write(d.join("post-commit"), original).unwrap();
        assert!(install_block(&d, Path::new("/a/mrcr")).unwrap_err().contains("シェルスクリプトではない"));
        assert_eq!(std::fs::read_to_string(d.join("post-commit")).unwrap(), original);
    }

    #[test]
    fn a_path_with_a_single_quote_is_quoted_safely() {
        assert_eq!(single_quote("/a/b'c"), "'/a/b'\\''c'");
    }

    #[test]
    fn hooks_dir_is_inside_dot_git_and_a_hookspath_outside_is_refused() {
        let (_g, r) = git_repo("dir");
        let h = hooks_dir(&r).unwrap();
        assert!(h.ends_with(".git/hooks"), "{}", h.display());
        assert!(Command::new("git").arg("-C").arg(&r).args(["config", "core.hooksPath", ".husky"]).output().unwrap().status.success());
        assert!(hooks_dir(&r).unwrap_err().contains("core.hooksPath"));
    }

    #[test]
    fn set_installs_and_removes_the_hook_and_keeps_the_recipes() {
        let _lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let (_g, r) = git_repo("set");
        let data = tmp("setdata");
        let fake_cli = data.join("mrcr");
        std::fs::write(&fake_cli, "#!/bin/sh\n").unwrap();
        std::env::set_var("MRCR_CLI", &fake_cli);

        let on = set(&data, r.to_str().unwrap(), true, &["bugs".into(), "errors".into()]).unwrap();
        assert!(on.enabled && on.installed && on.blocked.is_none(), "{on:?}");
        assert_eq!(on.recipes, vec!["bugs", "errors"]);
        assert!(r.join(".git/hooks/post-commit").exists());

        let off = set(&data, r.to_str().unwrap(), false, &["bugs".into(), "errors".into()]).unwrap();
        assert!(!off.enabled && !off.installed);
        assert!(!r.join(".git/hooks/post-commit").exists());
        assert_eq!(off.recipes, vec!["bugs", "errors"], "OFF にしても観点は残る");

        assert!(set(&data, r.to_str().unwrap(), true, &["a b".into()]).is_err(), "不正な観点は、何も変えずにエラー");
        std::env::remove_var("MRCR_CLI");
    }

    #[test]
    fn the_log_returns_the_latest_run_of_that_repo_and_skips_broken_lines() {
        let d = tmp("log");
        assert!(last_run(&d, "/r").is_none());
        append_log(&d, "/r", 1, "指摘あり").unwrap();
        append_log(&d, "/other", 0, "別のリポジトリ").unwrap();
        append_log(&d, "/r", 2, "API キーがありません").unwrap();
        use std::io::Write;
        writeln!(std::fs::OpenOptions::new().append(true).open(d.join(LOG)).unwrap(), "壊れた行").unwrap();
        let l = last_run(&d, "/r").unwrap();
        assert_eq!((l.code, l.message.as_str()), (2, "API キーがありません"));
    }

    #[cfg(unix)]
    fn write_exe(p: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn an_npm_launcher_resolves_to_the_native_binary_for_both_layouts() {
        use std::os::unix::fs::symlink;
        for nested in [false, true] {
            let d = tmp(if nested { "npmn" } else { "npms" });
            let lib = d.join("lib/node_modules/@mr-tabata");
            write_exe(&lib.join("mrcr/bin/mrcr.js"), "#!/usr/bin/env node\n");
            let native = if nested { lib.join("mrcr/node_modules/@mr-tabata/mrcr-darwin/bin/mrcr") } else { lib.join("mrcr-darwin/bin/mrcr") };
            write_exe(&native, "#!/bin/sh\n");
            std::fs::create_dir_all(d.join("bin")).unwrap();
            symlink(lib.join("mrcr/bin/mrcr.js"), d.join("bin/mrcr")).unwrap();
            assert_eq!(native_of(&d.join("bin/mrcr")).canonicalize().unwrap(), native.canonicalize().unwrap(), "nested={nested}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn an_npm_launcher_without_the_native_package_is_returned_as_is() {
        use std::os::unix::fs::symlink;
        let d = tmp("npmnone");
        write_exe(&d.join("lib/node_modules/@mr-tabata/mrcr/bin/mrcr.js"), "#!/usr/bin/env node\n");
        std::fs::create_dir_all(d.join("bin")).unwrap();
        symlink(d.join("lib/node_modules/@mr-tabata/mrcr/bin/mrcr.js"), d.join("bin/mrcr")).unwrap();
        assert_eq!(native_of(&d.join("bin/mrcr")), d.join("bin/mrcr"));
    }

    #[cfg(unix)]
    #[test]
    fn a_homebrew_style_symlink_is_kept_not_resolved_to_the_versioned_cellar_path() {
        use std::os::unix::fs::symlink;
        let d = tmp("brew");
        write_exe(&d.join("Cellar/mrcr/1.11.2/bin/mrcr"), "#!/bin/sh\n");
        std::fs::create_dir_all(d.join("bin")).unwrap();
        symlink(d.join("Cellar/mrcr/1.11.2/bin/mrcr"), d.join("bin/mrcr")).unwrap();
        assert_eq!(pick_cli(vec![d.join("bin/mrcr")]), Some(d.join("bin/mrcr")), "更新で消える版つきの実体ではなく、リンクを使う");
    }

    #[cfg(unix)]
    #[test]
    fn the_npx_cache_is_never_a_candidate_and_a_later_candidate_is_used() {
        let d = tmp("npx");
        let cache = d.join(".npm/_npx/abc/node_modules/.bin/mrcr");
        let real = d.join("bin/mrcr");
        write_exe(&cache, "#!/bin/sh\n");
        write_exe(&real, "#!/bin/sh\n");
        assert_eq!(pick_cli(vec![cache.clone(), real.clone()]), Some(real));
        assert_eq!(pick_cli(vec![cache]), None, "npx のキャッシュしか無いときは、見つからないことにする");
    }

    #[cfg(unix)]
    #[test]
    fn the_hook_falls_back_to_mrcr_on_path_when_the_recorded_path_is_gone() {
        let d = tmp("fallback");
        let marker = d.join("ran.txt");
        // PATH 上の mrcr（印を残す）。記録したパスは、存在しない
        write_exe(&d.join("bin/mrcr"), &format!("#!/bin/sh\necho \"$@\" > '{}'\n", marker.display()));
        let hooks = d.join("hooks");
        install_block(&hooks, Path::new("/no/such/mrcr")).unwrap();
        let o = Command::new("sh").arg(hooks.join("post-commit")).env("PATH", format!("{}:/usr/bin:/bin", d.join("bin").display())).output().unwrap();
        assert!(o.status.success());
        let t = std::time::Instant::now();
        while !marker.exists() && t.elapsed() < std::time::Duration::from_secs(5) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(std::fs::read_to_string(&marker).unwrap().trim(), "hook run post-commit");
    }

    #[test]
    fn the_recorded_path_is_read_back_from_the_hook() {
        let d = tmp("rec");
        install_block(&d, Path::new("/opt/x y/mrcr")).unwrap();
        let t = std::fs::read_to_string(d.join("post-commit")).unwrap();
        assert_eq!(recorded_cli(&t), Some(PathBuf::from("/opt/x y/mrcr")));
        assert!(install_block(&d, Path::new("/a\nb/mrcr")).is_err(), "改行を含むパスは、書かない");
    }

    #[test]
    fn status_says_when_the_hook_points_at_a_mrcr_that_is_gone() {
        let _lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let (_g, r) = git_repo("stale");
        let data = tmp("staledata");
        let cli = data.join("mrcr");
        std::fs::write(&cli, "#!/bin/sh\n").unwrap();
        std::env::set_var("MRCR_CLI", &cli);
        let on = set(&data, r.to_str().unwrap(), true, &["bugs".into()]).unwrap();
        assert!(on.installed && !on.target_missing);
        std::fs::remove_file(&cli).unwrap();
        let s = status(&data, r.to_str().unwrap()).unwrap();
        assert!(s.installed && s.cli.is_none() && s.target_missing, "{s:?}");
        std::env::remove_var("MRCR_CLI");
    }
}
