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
const LOG_MAX_BYTES: u64 = 512 * 1024;
const LOG_KEEP_LINES: usize = 1500;
/// 開始のあと、終わりが書かれないまま、これだけ過ぎた実行は「中断」とみなす（プロセスが落ちた・止められた、など）
const STALE_RUNNING_MS: u64 = 30 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoHook {
    pub enabled: bool,
    pub recipes: Vec<String>,
    /// 設計書が要る観点（`spec` など）に渡す設計書。リポジトリ内の相対パス（.md / .txt）。章の指定はできない
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<String>,
    /// 設計書が 2 件以上のときだけ、こちらに書く（1 件のときは `spec`。古い版でも読めるように、1 件の形は変えない）。
    /// 古い版は、この項目を知らず、設定ファイル全体を読めなくなる（`deny_unknown_fields`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub specs: Vec<String>,
}

impl RepoHook {
    /// 設計書のパスの全部（指定した順）。`specs` があればそれ、無ければ `spec`。
    pub fn spec_list(&self) -> Vec<String> {
        if !self.specs.is_empty() {
            self.specs.clone()
        } else {
            self.spec.iter().cloned().collect()
        }
    }
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

/// 設計書のパスの形を確かめる（ファイルの有無は見ない）。OFF にするときも使う。
pub fn validate_spec_shape(spec: &str) -> Result<(), String> {
    if spec.is_empty() || spec.len() > 300 || spec.contains(['\n', '\r', '\0']) {
        return Err("設計書のパスが正しくありません（空・長すぎる・改行を含む）".into());
    }
    if spec.contains(':') {
        return Err("設計書のパスに `:` は使えません（章の指定は、まだできません）".into());
    }
    Ok(())
}

/// ON にするときの確認（設計書が 1 件以下の形）。
pub fn validate_for_enable(root: &Path, recipes: &[String], spec: Option<&str>) -> Result<(), String> {
    let specs: Vec<String> = spec.map(|s| vec![s.to_string()]).unwrap_or_default();
    validate_specs_for_enable(root, recipes, &specs)
}

/// 設計書のパスの並びから、前後の空白と先頭の `./` を取り除き、重複を 1 件にする（順番は保つ）。
pub fn dedup_specs(specs: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in specs {
        let t = s.trim();
        let t = t.strip_prefix("./").unwrap_or(t).to_string();
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// ON にするときの確認: 設計書のパスは、リポジトリ内の、あるファイル（.md / .txt）で、最大 `MAX_SPEC_FILES` 件。
/// 選んだ観点は、あるものだけ。設計書が要る観点を選んだら、設計書の指定が要る。
pub fn validate_specs_for_enable(root: &Path, recipes: &[String], specs: &[String]) -> Result<(), String> {
    if specs.len() > crate::spec::MAX_SPEC_FILES {
        return Err(format!("設計書が多すぎます（{} 件）。上限は {} 件です", specs.len(), crate::spec::MAX_SPEC_FILES));
    }
    for s in specs {
        validate_spec_shape(s)?;
        if !crate::spec::is_supported_spec_extension(Path::new(s)) {
            return Err("設計書は .md または .txt のファイルだけ指定できます".into());
        }
        let resolved = crate::spec::safe_repo_relative_path(root, Path::new(s))?;
        if !resolved.is_file() {
            return Err(format!("設計書がファイルではありません: {s}"));
        }
    }
    for id in recipes {
        let Some(r) = crate::recipe::find_with_lang(Some(root), id, "ja") else {
            return Err(format!("観点 `{id}` が見つかりません"));
        };
        if r.needs_spec && specs.is_empty() {
            return Err(format!("観点 `{id}` は設計書が要ります。リポジトリ内の設計書のパス（例: docs/spec.md）を指定してください"));
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
    /// 設計書が要る観点に渡す設計書（リポジトリ内の相対パス）。複数あるときは、先頭の 1 件（1 件だけを扱う画面のため）
    pub spec: Option<String>,
    /// 設計書の全部（指定した順）。1 件以下なら、`spec` と同じ中身
    pub specs: Vec<String>,
    /// フックのスクリプトに、管理のブロックが入っているか
    pub installed: bool,
    /// いま見つかる `mrcr`（無ければ、画面で案内する）
    pub cli: Option<String>,
    /// フックのスクリプトに、実際に書かれている `mrcr` のパス（フックが入っていないときは `None`）。`cli` と違うことがある
    pub hook_cli: Option<String>,
    /// フックの置き場に書けない理由（husky など）。あれば、入れられない
    pub blocked: Option<String>,
    /// フックは入っているが、記録した `mrcr` が無く、代わりに使える `mrcr` も見つからない（更新・削除で）。入れ直す必要がある
    pub target_missing: bool,
    pub last_run: Option<LastRun>,
    /// 最近の実行（新しい順。途中経過つき）
    pub recent: Vec<RunLog>,
    /// 実行ログの中で、読めなかった行の数（0 でなければ、画面で知らせる。ほかのリポジトリの分を含む）
    pub log_unreadable: usize,
}

/// リポジトリの一覧に出す、Git フックの要約（実行ログ・観点・設計書は、読まない）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Brief {
    /// 設定の ON/OFF
    pub enabled: bool,
    /// フックのスクリプトに、管理のブロックが入っているか
    pub installed: bool,
}

/// `status` の軽い版。設定の ON/OFF と、フックが入っているかだけ。
/// リポジトリではない・フックの置き場が読めないときは、`installed` は false（エラーにしない。一覧が、1 つの失敗で止まらないように）。
pub fn brief(dir: &Path, repo: &str) -> Result<Brief, String> {
    let key = repo_key(repo)?;
    let cfg = load(dir)?;
    let enabled = cfg.post_commit.get(&key).map(|e| e.enabled).unwrap_or(false);
    let installed = hooks_dir(Path::new(&key)).ok().and_then(|h| std::fs::read_to_string(h.join(HOOK_NAME)).ok()).map(|t| has_block(&t)).unwrap_or(false);
    Ok(Brief { enabled, installed })
}

pub fn status(dir: &Path, repo: &str) -> Result<Status, String> {
    let key = repo_key(repo)?;
    let cfg = load(dir)?;
    let entry = cfg.post_commit.get(&key);
    let cli = find_cli();
    let log_lines = read_lines(dir, &key); // ログは、1 回だけ読む（前回から変わっていなければ、読み直さない）
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
        spec: entry.and_then(|e| e.spec_list().into_iter().next()),
        specs: entry.map(|e| e.spec_list()).unwrap_or_default(),
        installed,
        cli: cli.map(|p| p.to_string_lossy().into_owned()),
        hook_cli: recorded.as_ref().map(|p| p.to_string_lossy().into_owned()),
        blocked,
        target_missing,
        last_run: last_run_from(&log_lines),
        recent: runs_from(log_lines, 5, now_ms()),
        log_unreadable: log_unreadable(dir),
    })
}

/// ON/OFF と観点を設定する。ON なら、フックを入れる（`mrcr` が見つからなければエラー）。OFF なら、外す（観点は残す）。
pub fn set(dir: &Path, repo: &str, enabled: bool, recipes: &[String], spec: Option<&str>) -> Result<Status, String> {
    let specs: Vec<String> = spec.map(|s| vec![s.to_string()]).unwrap_or_default();
    set_specs(dir, repo, enabled, recipes, &specs)
}

/// `set` の、設計書を複数渡せる形。設計書が 1 件なら `spec`（古い版でも読める形）、2 件以上なら `specs` に書く。
pub fn set_specs(dir: &Path, repo: &str, enabled: bool, recipes: &[String], specs: &[String]) -> Result<Status, String> {
    validate_recipes(recipes)?;
    let key = repo_key(repo)?;
    let specs = dedup_specs(specs);
    // 外すときは、設計書のファイルが無くても外せるように、形だけを確かめる
    if enabled {
        validate_specs_for_enable(Path::new(&key), recipes, &specs)?;
    } else {
        if specs.len() > crate::spec::MAX_SPEC_FILES {
            return Err(format!("設計書が多すぎます（{} 件）。上限は {} 件です", specs.len(), crate::spec::MAX_SPEC_FILES));
        }
        for s in &specs {
            validate_spec_shape(s)?;
        }
    }
    let mut cfg = load(dir)?;
    let hooks = hooks_dir(Path::new(&key));
    if enabled {
        let cli = find_cli().ok_or_else(|| "mrcr（CLI）が見つかりません。入れてから、もう一度 ON にしてください（例: brew install mr-tabata/tap/mrcr、または npm i -g @mr-tabata/mrcr）。npx で起動した mrcr は、実行ファイルが一時のキャッシュにあり、あとで消えるので、フックには使えません".to_string())?;
        install_block(&hooks?, &cli)?;
    } else if let Ok(h) = &hooks {
        uninstall_block(h)?;
    }
    let (one, many) = match specs.len() {
        0 => (None, Vec::new()),
        1 => (Some(specs[0].clone()), Vec::new()),
        _ => (None, specs),
    };
    cfg.post_commit.insert(key.clone(), RepoHook { enabled, recipes: recipes.to_vec(), spec: one, specs: many });
    if let Err(e) = save(dir, &cfg) {
        if enabled {
            // 入れたフックを外す。外せなかったときは、その旨も伝える（フックだけが残ると、設定は OFF のまま、コミットのたびに動く）
            let rollback = hook_dir_quiet(&key).and_then(|h| uninstall_block(&h).map(|_| ()));
            return Err(with_rollback_result(e, rollback));
        }
        return Err(e);
    }
    status(dir, &key)
}

/// 設定の保存に失敗したときのエラーに、フックを外せたかどうかを添える。外せなかったときだけ、手で消す方法まで書く。
fn with_rollback_result(save_error: String, rollback: Result<(), String>) -> String {
    match rollback {
        Ok(()) => save_error,
        Err(re) => format!("{save_error}（入れたフックを外せませんでした: {re}。`.git/hooks/post-commit` の `mrcr hook` の範囲を、手で消してください）"),
    }
}

fn hook_dir_quiet(key: &str) -> Result<PathBuf, String> {
    hooks_dir(Path::new(key))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// `hooks.log` の 1 行（JSON）。使わない項目は書かない。
/// - `kind` が無く `code` がある行は、従来の「終了」の 1 行（途中経過を記録する前の版が書いたもの・早い段階の失敗）。
/// - `kind` が `start` / `recipe` / `end` の行は、`run`（実行の識別子）でまとめて、1 回の実行になる。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LogLine {
    at_ms: u64,
    repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    run: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recipes: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recipe: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    total: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    findings: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ok: Option<bool>,
}

fn write_line(dir: &Path, line: &LogLine) -> Result<(), String> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|e| format!("データの置き場を作れません: {e}"))?;
    let p = dir.join(LOG);
    let text = serde_json::to_string(line).map_err(|e| format!("ログを書き出せません: {e}"))?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&p).map_err(|e| format!("ログを開けません（{}）: {e}", p.display()))?;
    writeln!(f, "{text}").map_err(|e| format!("ログを書けません: {e}"))?;
    if std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) > LOG_MAX_BYTES {
        if let Ok(all) = std::fs::read_to_string(&p) {
            // 新しい側から、行数と大きさの**両方**の範囲で残す。大きさ（上限の半分）でも切るのは、1 行が長いと、行数だけでは、
            // 切り詰めたあとも上限を超えたままになり、書き込むたびに、全体を書き直すことになるため
            let mut kept: Vec<&str> = Vec::new();
            let mut bytes = 0usize;
            for l in all.lines().rev() {
                if kept.len() >= LOG_KEEP_LINES || bytes + l.len() + 1 > (LOG_MAX_BYTES as usize) / 2 {
                    break;
                }
                bytes += l.len() + 1;
                kept.push(l);
            }
            kept.reverse();
            let _ = std::fs::write(&p, kept.join("\n") + "\n");
        }
    }
    Ok(())
}

fn cut(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// 従来の形の「終了」の 1 行（リポジトリを特定できない・設定を読めない、など、実行を始める前の失敗）。
pub fn append_log(dir: &Path, repo: &str, code: i32, message: &str) -> Result<(), String> {
    write_line(dir, &LogLine { at_ms: now_ms(), repo: repo.into(), code: Some(code), message: Some(cut(message, 500)), ..Default::default() })
}

/// 実行の識別子（時刻とプロセス番号）。
pub fn new_run_id() -> String {
    format!("{}-{}", now_ms(), std::process::id())
}

/// 実行の開始: どのコミットを、どの観点で見るか。
pub fn log_start(dir: &Path, repo: &str, run: &str, commit: &str, subject: &str, recipes: &[String]) -> Result<(), String> {
    write_line(
        dir,
        &LogLine {
            at_ms: now_ms(),
            repo: repo.into(),
            run: Some(run.into()),
            kind: Some("start".into()),
            commit: Some(cut(commit, 40)),
            subject: Some(cut(subject, 200)),
            recipes: Some(recipes.to_vec()),
            ..Default::default()
        },
    )
}

/// 観点 1 つの完了（成功も失敗も）。
#[allow(clippy::too_many_arguments)]
pub fn log_step(dir: &Path, repo: &str, run: &str, recipe: &str, index: usize, total: usize, ok: bool, findings: usize, ms: u64, error: Option<&str>) -> Result<(), String> {
    write_line(
        dir,
        &LogLine {
            at_ms: now_ms(),
            repo: repo.into(),
            run: Some(run.into()),
            kind: Some("recipe".into()),
            recipe: Some(recipe.into()),
            index: Some(index),
            total: Some(total),
            ok: Some(ok),
            findings: Some(findings),
            ms: Some(ms),
            message: error.map(|e| cut(e, 300)),
            ..Default::default()
        },
    )
}

/// 実行の終了。0=指摘なし・1=指摘あり・2=失敗（一部の観点の失敗を含む）。
pub fn log_end(dir: &Path, repo: &str, run: &str, code: i32, message: &str, findings: usize, ms: u64) -> Result<(), String> {
    write_line(
        dir,
        &LogLine {
            at_ms: now_ms(),
            repo: repo.into(),
            run: Some(run.into()),
            kind: Some("end".into()),
            code: Some(code),
            message: Some(cut(message, 500)),
            findings: Some(findings),
            ms: Some(ms),
            ..Default::default()
        },
    )
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepLog {
    pub recipe: String,
    pub index: usize,
    pub total: usize,
    pub ok: bool,
    pub findings: usize,
    pub ms: u64,
    pub error: Option<String>,
}

/// 1 回の実行（開始・観点ごとの結果・終了を、`run` でまとめたもの）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunLog {
    pub run: String,
    pub started_at_ms: u64,
    pub commit: Option<String>,
    pub subject: Option<String>,
    pub recipes: Vec<String>,
    pub steps: Vec<StepLog>,
    /// `running`（実行中）・`done`（完了）・`failed`（失敗）・`aborted`（終わりが書かれないまま止まった）
    pub state: String,
    pub code: Option<i32>,
    pub message: Option<String>,
    pub findings: Option<usize>,
    pub ms: Option<u64>,
}

/// ログ全体を解析した結果。`unreadable` は、JSON として読めなかった行の数（どのリポジトリの行かは、分からない）。
struct ParsedLog {
    lines: Vec<LogLine>,
    unreadable: usize,
}

type LogCacheEntry = (PathBuf, std::time::SystemTime, u64, std::sync::Arc<ParsedLog>);
static LOG_CACHE: std::sync::Mutex<Vec<LogCacheEntry>> = std::sync::Mutex::new(Vec::new());
const LOG_CACHE_ENTRIES: usize = 32;

/// ログ全体を解析する。**ファイルの更新時刻と大きさが前回と同じなら、読み直さない**
/// （設定画面が 2〜8 秒ごとに読むので、変わっていないあいだは、ファイルの情報を見るだけで済ませる）。
/// 読めなかった行は、捨てるが、数える（`unreadable`）。
fn parsed_log(dir: &Path) -> std::sync::Arc<ParsedLog> {
    let p = dir.join(LOG);
    let stamp = std::fs::metadata(&p).ok().and_then(|m| m.modified().ok().map(|t| (t, m.len())));
    if let Some((t, len)) = stamp {
        if let Ok(cache) = LOG_CACHE.lock() {
            if let Some(e) = cache.iter().find(|e| e.0 == p && e.1 == t && e.2 == len) {
                return e.3.clone();
            }
        }
    }
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    let mut lines = Vec::new();
    let mut unreadable = 0usize;
    for l in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<LogLine>(l) {
            Ok(x) => lines.push(x),
            Err(_) => unreadable += 1,
        }
    }
    let parsed = std::sync::Arc::new(ParsedLog { lines, unreadable });
    if let Some((t, len)) = stamp {
        if let Ok(mut cache) = LOG_CACHE.lock() {
            cache.retain(|e| e.0 != p);
            if cache.len() >= LOG_CACHE_ENTRIES {
                cache.remove(0);
            }
            cache.push((p, t, len, parsed.clone()));
        }
    }
    parsed
}

fn read_lines(dir: &Path, repo: &str) -> Vec<LogLine> {
    parsed_log(dir).lines.iter().filter(|l| l.repo == repo).cloned().collect()
}

/// 実行ログの中の、読めなかった行の数（壊れた行・知らない形の行。どのリポジトリの分かは、分からない）。
pub fn log_unreadable(dir: &Path) -> usize {
    parsed_log(dir).unreadable
}

fn state_of(code: Option<i32>) -> String {
    if code == Some(2) { "failed".into() } else { "done".into() }
}

/// そのリポジトリの、最近の実行（新しい順）。読めない行は飛ばす。従来の 1 行（途中経過なし）も、1 回の実行として出す。
pub fn recent_runs(dir: &Path, repo: &str, limit: usize) -> Vec<RunLog> {
    recent_runs_at(dir, repo, limit, now_ms())
}

fn recent_runs_at(dir: &Path, repo: &str, limit: usize, now: u64) -> Vec<RunLog> {
    runs_from(read_lines(dir, repo), limit, now)
}

/// 読んだ行から、最近の実行を作る（ファイルは、呼び出し側が 1 回だけ読む）。
fn runs_from(lines: Vec<LogLine>, limit: usize, now: u64) -> Vec<RunLog> {
    let mut runs: Vec<RunLog> = Vec::new();
    let blank = |id: String, at: u64| RunLog {
        run: id,
        started_at_ms: at,
        commit: None,
        subject: None,
        recipes: Vec::new(),
        steps: Vec::new(),
        state: "running".into(),
        code: None,
        message: None,
        findings: None,
        ms: None,
    };
    for l in lines {
        let Some(id) = l.run.clone() else {
            // 従来の 1 行
            if l.code.is_some() {
                let mut r = blank(format!("legacy-{}", l.at_ms), l.at_ms);
                r.state = state_of(l.code);
                r.code = l.code;
                r.message = l.message;
                runs.push(r);
            }
            continue;
        };
        let pos = match runs.iter().position(|r| r.run == id) {
            Some(i) => i,
            None => {
                runs.push(blank(id, l.at_ms));
                runs.len() - 1
            }
        };
        let r = &mut runs[pos];
        match l.kind.as_deref() {
            Some("start") => {
                r.started_at_ms = l.at_ms;
                r.commit = l.commit;
                r.subject = l.subject;
                r.recipes = l.recipes.unwrap_or_default();
            }
            Some("recipe") => r.steps.push(StepLog {
                recipe: l.recipe.unwrap_or_default(),
                index: l.index.unwrap_or(0),
                total: l.total.unwrap_or(0),
                ok: l.ok.unwrap_or(false),
                findings: l.findings.unwrap_or(0),
                ms: l.ms.unwrap_or(0),
                error: l.message,
            }),
            Some("end") => {
                r.state = state_of(l.code);
                r.code = l.code;
                r.message = l.message;
                r.findings = l.findings;
                r.ms = l.ms;
            }
            _ => {}
        }
    }
    for r in &mut runs {
        if r.state == "running" && now.saturating_sub(r.started_at_ms) > STALE_RUNNING_MS {
            r.state = "aborted".into();
        }
    }
    runs.reverse();
    runs.truncate(limit);
    runs
}

/// そのリポジトリの、最新の実行結果（無ければ `None`）。実行中のもの（終わりが無いもの）は、数えない。
pub fn last_run(dir: &Path, repo: &str) -> Option<LastRun> {
    last_run_from(&read_lines(dir, repo))
}

fn last_run_from(lines: &[LogLine]) -> Option<LastRun> {
    lines
        .iter()
        .rev()
        .find(|l| l.code.is_some() && (l.kind.is_none() || l.kind.as_deref() == Some("end")))
        .map(|l| LastRun { at_ms: l.at_ms, code: l.code.unwrap_or(2), message: l.message.clone().unwrap_or_default() })
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
        c.post_commit.insert("/r".into(), RepoHook { enabled: true, recipes: vec!["bugs".into()], spec: None, specs: Vec::new() });
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

        let on = set(&data, r.to_str().unwrap(), true, &["bugs".into(), "errors".into()], None).unwrap();
        assert!(on.enabled && on.installed && on.blocked.is_none(), "{on:?}");
        assert_eq!(on.recipes, vec!["bugs", "errors"]);
        assert!(r.join(".git/hooks/post-commit").exists());

        let off = set(&data, r.to_str().unwrap(), false, &["bugs".into(), "errors".into()], None).unwrap();
        assert!(!off.enabled && !off.installed);
        assert!(!r.join(".git/hooks/post-commit").exists());
        assert_eq!(off.recipes, vec!["bugs", "errors"], "OFF にしても観点は残る");

        assert!(set(&data, r.to_str().unwrap(), true, &["a b".into()], None).is_err(), "不正な観点は、何も変えずにエラー");
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
        let on = set(&data, r.to_str().unwrap(), true, &["bugs".into()], None).unwrap();
        assert!(on.installed && !on.target_missing);
        std::fs::remove_file(&cli).unwrap();
        let s = status(&data, r.to_str().unwrap()).unwrap();
        assert!(s.installed && s.cli.is_none() && s.target_missing, "{s:?}");
        std::env::remove_var("MRCR_CLI");
    }

    fn repo_with_spec(tag: &str) -> (Tmp, PathBuf) {
        let (g, r) = git_repo(tag);
        std::fs::create_dir_all(r.join("docs")).unwrap();
        std::fs::write(r.join("docs/spec.md"), "# 仕様\n").unwrap();
        std::fs::write(r.join("docs/code.rs"), "fn main() {}\n").unwrap();
        (g, r)
    }

    #[test]
    fn spec_path_shape_is_checked() {
        assert!(validate_spec_shape("docs/spec.md").is_ok());
        for bad in ["", "a\nb.md", "a:b.md", "docs/spec.md:##章", &"x".repeat(301)] {
            assert!(validate_spec_shape(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn enabling_checks_the_spec_file_stays_inside_the_repo_and_is_md_or_txt() {
        let (_g, r) = repo_with_spec("specv");
        let bugs = vec!["bugs".to_string()];
        assert!(validate_for_enable(&r, &bugs, Some("docs/spec.md")).is_ok());
        assert!(validate_for_enable(&r, &bugs, Some("../outside.md")).unwrap_err().contains(".."));
        assert!(validate_for_enable(&r, &bugs, Some("/etc/passwd.md")).unwrap_err().contains("相対パス"));
        assert!(validate_for_enable(&r, &bugs, Some("docs/none.md")).unwrap_err().contains("見つかりません"));
        assert!(validate_for_enable(&r, &bugs, Some("docs/code.rs")).unwrap_err().contains(".md または .txt"));
        assert!(validate_for_enable(&r, &bugs, Some("docs")).is_err(), "フォルダは、設計書にならない");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_that_leaves_the_repo_is_refused() {
        let (_g, r) = repo_with_spec("speclink");
        let outside = tmp("specout");
        std::fs::write(outside.join("o.md"), "x").unwrap();
        std::os::unix::fs::symlink(outside.join("o.md"), r.join("docs/link.md")).unwrap();
        assert!(validate_for_enable(&r, &["bugs".to_string()], Some("docs/link.md")).is_err());
    }

    #[test]
    fn a_recipe_that_needs_a_spec_needs_one_and_unknown_recipes_are_refused() {
        let (_g, r) = repo_with_spec("specneed");
        let spec_recipe = vec!["spec".to_string()];
        assert!(validate_for_enable(&r, &spec_recipe, None).unwrap_err().contains("設計書が要ります"));
        assert!(validate_for_enable(&r, &spec_recipe, Some("docs/spec.md")).is_ok());
        assert!(validate_for_enable(&r, &["nope".to_string()], None).unwrap_err().contains("見つかりません"));
    }

    #[test]
    fn set_saves_the_spec_and_off_works_even_when_the_spec_file_is_gone() {
        let _lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let (_g, r) = repo_with_spec("specset");
        let data = tmp("specsetdata");
        let cli = data.join("mrcr");
        std::fs::write(&cli, "#!/bin/sh\n").unwrap();
        std::env::set_var("MRCR_CLI", &cli);
        let recipes = vec!["bugs".to_string(), "spec".to_string()];

        let on = set(&data, r.to_str().unwrap(), true, &recipes, Some("docs/spec.md")).unwrap();
        assert_eq!(on.spec.as_deref(), Some("docs/spec.md"));
        assert!(set(&data, r.to_str().unwrap(), true, &recipes, None).is_err(), "設計書なしでは、設計書の要る観点を ON にできない");
        assert_eq!(status(&data, r.to_str().unwrap()).unwrap().spec.as_deref(), Some("docs/spec.md"), "断られたとき、設定は変わらない");

        std::fs::remove_file(r.join("docs/spec.md")).unwrap();
        let off = set(&data, r.to_str().unwrap(), false, &recipes, Some("docs/spec.md")).unwrap();
        assert!(!off.enabled && !off.installed, "ファイルが無くても、外せる");
        assert_eq!(off.spec.as_deref(), Some("docs/spec.md"), "OFF でも設計書の設定は残る");
        std::env::remove_var("MRCR_CLI");
    }

    #[test]
    fn an_old_config_without_spec_still_loads() {
        let d = tmp("oldcfg");
        std::fs::write(path(&d), r#"{"post_commit":{"/r":{"enabled":true,"recipes":["bugs"]}}}"#).unwrap();
        let c = load(&d).unwrap();
        assert_eq!(c.post_commit["/r"].spec, None);
        save(&d, &c).unwrap();
        assert!(!std::fs::read_to_string(path(&d)).unwrap().contains("spec"), "無いときは、書き出さない");
    }

    #[test]
    fn a_run_is_grouped_from_start_steps_and_end() {
        let d = tmp("runs");
        let rs = vec!["bugs".to_string(), "spec".to_string()];
        log_start(&d, "/r", "run1", "abc1234", "件名です", &rs).unwrap();
        log_step(&d, "/r", "run1", "bugs", 1, 2, true, 4, 7000, None).unwrap();
        log_step(&d, "/r", "run1", "spec", 2, 2, false, 0, 100, Some("エラー: LLM 失敗")).unwrap();
        log_end(&d, "/r", "run1", 2, "一部の観点が失敗しました", 4, 7200).unwrap();
        let runs = recent_runs(&d, "/r", 5);
        assert_eq!(runs.len(), 1);
        let r = &runs[0];
        assert_eq!((r.commit.as_deref(), r.subject.as_deref()), (Some("abc1234"), Some("件名です")));
        assert_eq!(r.recipes, rs);
        assert_eq!(r.steps.len(), 2);
        assert_eq!((r.steps[0].recipe.as_str(), r.steps[0].findings, r.steps[0].ok), ("bugs", 4, true));
        assert_eq!((r.steps[1].ok, r.steps[1].error.as_deref()), (false, Some("エラー: LLM 失敗")));
        assert_eq!((r.state.as_str(), r.code, r.findings, r.ms), ("failed", Some(2), Some(4), Some(7200)));
    }

    #[test]
    fn a_run_without_an_end_is_running_then_aborted_when_it_is_old() {
        let d = tmp("running");
        log_start(&d, "/r", "run1", "abc", "x", &["bugs".to_string()]).unwrap();
        log_step(&d, "/r", "run1", "bugs", 1, 3, true, 1, 10, None).unwrap();
        let now = now_ms();
        assert_eq!(recent_runs_at(&d, "/r", 5, now)[0].state, "running");
        assert_eq!(recent_runs_at(&d, "/r", 5, now + STALE_RUNNING_MS + 60_000)[0].state, "aborted");
        assert!(last_run(&d, "/r").is_none(), "終わっていない実行は、最新の結果に数えない");
    }

    #[test]
    fn runs_are_newest_first_limited_per_repo_and_legacy_lines_are_runs_too() {
        let d = tmp("order");
        append_log(&d, "/r", 1, "従来の 1 行").unwrap();
        for i in 0..3 {
            let id = format!("run{i}");
            log_start(&d, "/r", &id, &format!("c{i}"), "s", &["bugs".to_string()]).unwrap();
            log_end(&d, "/r", &id, 0, "完了", 0, 1).unwrap();
        }
        log_start(&d, "/other", "o", "zzz", "s", &[]).unwrap();
        let runs = recent_runs(&d, "/r", 3);
        assert_eq!(runs.iter().map(|r| r.commit.clone().unwrap_or_default()).collect::<Vec<_>>(), vec!["c2", "c1", "c0"], "新しい順で、件数を絞る");
        let all = recent_runs(&d, "/r", 10);
        assert_eq!(all.len(), 4);
        assert_eq!((all[3].message.as_deref(), all[3].state.as_str(), all[3].commit.as_deref()), (Some("従来の 1 行"), "done", None));
        assert!(all.iter().all(|r| r.commit.as_deref() != Some("zzz")), "別のリポジトリの実行は、出さない");
        assert_eq!(last_run(&d, "/r").unwrap().message, "完了");
    }

    #[test]
    fn broken_lines_and_unknown_fields_in_the_log_do_not_break_reading() {
        let d = tmp("broken");
        log_start(&d, "/r", "run1", "abc", "x", &["bugs".to_string()]).unwrap();
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(d.join(LOG)).unwrap();
        writeln!(f, "壊れた行").unwrap();
        writeln!(f, r#"{{"at_ms":1,"repo":"/r","run":"run1","kind":"future-kind","new_field":1}}"#).unwrap();
        log_end(&d, "/r", "run1", 0, "完了", 0, 5).unwrap();
        let runs = recent_runs(&d, "/r", 5);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].state, "done");
    }

    #[test]
    fn a_failing_save_rolls_the_hook_back_and_the_error_says_whether_that_worked() {
        let _lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let (_g, r) = git_repo("rollback");
        let data = tmp("rollbackdata");
        let cli = data.join("mrcr");
        std::fs::write(&cli, "#!/bin/sh\n").unwrap();
        std::env::set_var("MRCR_CLI", &cli);
        // 設定の一時ファイルの場所を、フォルダでふさぐ → 保存だけが失敗する
        std::fs::create_dir_all(data.join("hooks.json.tmp")).unwrap();

        let err = set(&data, r.to_str().unwrap(), true, &["bugs".into()], None).unwrap_err();
        assert!(err.contains("設定を書けません") || err.contains("hooks.json"), "{err}");
        assert!(!err.contains("外せませんでした"), "巻き戻せたときは、その旨を足さない: {err}");
        assert!(!r.join(".git/hooks/post-commit").exists(), "保存に失敗したら、入れたフックは、残さない");
        assert!(!path(&data).exists(), "設定も、作らない");
        std::env::remove_var("MRCR_CLI");
    }

    #[test]
    fn the_save_error_says_when_the_rollback_failed_and_how_to_remove_the_hook_by_hand() {
        assert_eq!(with_rollback_result("保存できません".into(), Ok(())), "保存できません");
        let m = with_rollback_result("保存できません".into(), Err("権限がありません".into()));
        assert!(m.starts_with("保存できません"), "{m}");
        assert!(m.contains("外せませんでした: 権限がありません"), "{m}");
        assert!(m.contains(".git/hooks/post-commit"), "手で消す場所を、教える: {m}");
    }

    #[test]
    fn a_subject_with_newlines_and_quotes_keeps_every_log_entry_on_one_line_and_round_trips() {
        let d = tmp("subject");
        let subject = "1 行目\n2 行目 \"引用\" \\ バックスラッシュ 日本語";
        log_start(&d, "/r", "run1", "abc", subject, &["bugs".to_string()]).unwrap();
        log_end(&d, "/r", "run1", 0, "完了", 0, 1).unwrap();
        let text = std::fs::read_to_string(d.join(LOG)).unwrap();
        assert_eq!(text.lines().count(), 2, "改行を含んでも、1 件が 1 行のまま: {text}");
        assert_eq!(recent_runs(&d, "/r", 5)[0].subject.as_deref(), Some(subject));
    }

    #[test]
    fn reading_the_log_only_picks_that_repository_even_with_similar_or_quoted_paths() {
        let d = tmp("logfilter");
        for (repo, id) in [("/r", "a"), ("/r2", "b"), ("/r\"x", "c"), ("/日本/リポ", "d")] {
            log_start(&d, repo, id, id, "s", &[]).unwrap();
            log_end(&d, repo, id, 0, "完了", 0, 1).unwrap();
        }
        for (repo, id) in [("/r", "a"), ("/r2", "b"), ("/r\"x", "c"), ("/日本/リポ", "d")] {
            let runs = recent_runs(&d, repo, 10);
            assert_eq!(runs.len(), 1, "{repo}");
            assert_eq!(runs[0].commit.as_deref(), Some(id), "{repo}");
        }
    }

    #[test]
    fn status_reads_the_log_for_both_the_last_run_and_the_recent_runs() {
        let _lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let (_g, r) = git_repo("statuslog");
        let data = tmp("statuslogdata");
        let key = repo_key(r.to_str().unwrap()).unwrap();
        log_start(&data, &key, "run1", "abc", "件名", &["bugs".to_string()]).unwrap();
        log_end(&data, &key, "run1", 1, "レビュー完了（指摘あり 2 件）", 2, 5).unwrap();
        let s = status(&data, r.to_str().unwrap()).unwrap();
        assert_eq!(s.last_run.as_ref().map(|l| l.code), Some(1));
        assert_eq!(s.recent.len(), 1);
        assert_eq!(s.recent[0].commit.as_deref(), Some("abc"));
    }

    #[test]
    fn status_shows_the_path_written_in_the_hook_and_it_can_differ_from_the_one_found_now() {
        let _lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let (_g, r) = git_repo("hookcli");
        let data = tmp("hookclidata");
        let first = data.join("mrcr-first");
        let second = data.join("mrcr-second");
        std::fs::write(&first, "#!/bin/sh\n").unwrap();
        std::fs::write(&second, "#!/bin/sh\n").unwrap();

        std::env::set_var("MRCR_CLI", &first);
        let before = status(&data, r.to_str().unwrap()).unwrap();
        assert_eq!(before.hook_cli, None, "フックが入っていないときは、書かれたパスは無い");
        assert_eq!(before.cli.as_deref(), Some(first.to_str().unwrap()));

        let on = set(&data, r.to_str().unwrap(), true, &["bugs".into()], None).unwrap();
        assert_eq!(on.hook_cli.as_deref(), Some(first.to_str().unwrap()), "フックに書いたパス");

        // あとから、見つかる mrcr が変わっても、フックに書かれているパスは、そのまま
        std::env::set_var("MRCR_CLI", &second);
        let after = status(&data, r.to_str().unwrap()).unwrap();
        assert_eq!(after.hook_cli.as_deref(), Some(first.to_str().unwrap()));
        assert_eq!(after.cli.as_deref(), Some(second.to_str().unwrap()));

        // 入れ直す（OFF → ON）と、新しいほうに切り替わる
        set(&data, r.to_str().unwrap(), false, &["bugs".into()], None).unwrap();
        let again = set(&data, r.to_str().unwrap(), true, &["bugs".into()], None).unwrap();
        assert_eq!(again.hook_cli.as_deref(), Some(second.to_str().unwrap()));
        std::env::remove_var("MRCR_CLI");
    }

    #[test]
    fn with_an_old_single_line_and_a_new_running_run_the_last_result_is_the_old_line() {
        let d = tmp("mixed");
        append_log(&d, "/r", 1, "従来の 1 行（指摘あり）").unwrap();
        log_start(&d, "/r", "run2", "abc", "新しい", &["bugs".to_string()]).unwrap();
        log_step(&d, "/r", "run2", "bugs", 1, 3, true, 0, 5, None).unwrap();
        let last = last_run(&d, "/r").unwrap();
        assert_eq!((last.code, last.message.as_str()), (1, "従来の 1 行（指摘あり）"), "実行中の新しい実行は、最新の結果に数えない");
        let runs = recent_runs(&d, "/r", 5);
        assert_eq!(runs.len(), 2);
        assert_eq!((runs[0].state.as_str(), runs[0].commit.as_deref()), ("running", Some("abc")), "実行ログでは、新しい実行が先頭");
        assert_eq!(runs[1].run, format!("legacy-{}", runs[1].started_at_ms));
        // 新しい実行が終われば、そちらが最新の結果になる
        log_end(&d, "/r", "run2", 0, "完了", 0, 9).unwrap();
        assert_eq!(last_run(&d, "/r").unwrap().message, "完了");
    }

    #[test]
    fn the_log_is_trimmed_to_the_newest_lines_when_it_gets_too_large_and_reading_survives_it() {
        let d = tmp("trim");
        let long = "x".repeat(400);
        for i in 0..2000 {
            log_step(&d, "/r", "big", &format!("recipe{i}"), i, 2000, true, 0, 1, Some(&long)).unwrap();
        }
        log_start(&d, "/r", "last", "newest", "最新", &["bugs".to_string()]).unwrap();
        log_end(&d, "/r", "last", 0, "最新の完了", 0, 1).unwrap();
        let size = std::fs::metadata(d.join(LOG)).unwrap().len();
        assert!(size <= LOG_MAX_BYTES + 1024, "上限を大きく超えて、大きくならない: {size}");
        let text = std::fs::read_to_string(d.join(LOG)).unwrap();
        assert!(text.lines().count() <= 2002 && text.lines().count() >= 2, "{}", text.lines().count());
        assert!(text.lines().last().unwrap().contains("最新の完了"), "新しい側が残る");
        assert_eq!(log_unreadable(&d), 0, "切り詰めで、行が壊れない（行の単位で、切る）");
        let runs = recent_runs(&d, "/r", 5);
        assert_eq!(runs[0].commit.as_deref(), Some("newest"), "最新の実行が、先頭に出る");
        assert_eq!(runs[0].state, "done");
        assert_eq!(last_run(&d, "/r").unwrap().message, "最新の完了");
    }

    #[test]
    fn a_run_whose_start_line_was_trimmed_away_still_shows_its_steps_without_a_commit() {
        let d = tmp("orphan");
        // 開始の行が無く、途中経過と終了だけが残っている（切り詰めの結果）
        log_step(&d, "/r", "cut", "bugs", 1, 2, true, 3, 100, None).unwrap();
        log_step(&d, "/r", "cut", "spec", 2, 2, true, 0, 50, None).unwrap();
        log_end(&d, "/r", "cut", 1, "レビュー完了（指摘あり 3 件）", 3, 150).unwrap();
        let runs = recent_runs(&d, "/r", 5);
        assert_eq!(runs.len(), 1);
        let r = &runs[0];
        assert_eq!((r.commit.as_deref(), r.steps.len(), r.state.as_str()), (None, 2, "done"), "コミットは分からないが、結果は見える");
        assert_eq!(r.message.as_deref(), Some("レビュー完了（指摘あり 3 件）"));
    }

    #[test]
    fn the_parsed_log_is_reused_while_the_file_is_unchanged_and_read_again_after_a_write() {
        let d = tmp("cache");
        log_start(&d, "/r", "run1", "abc", "s", &[]).unwrap();
        let a = parsed_log(&d);
        let b = parsed_log(&d);
        assert!(std::sync::Arc::ptr_eq(&a, &b), "変わっていなければ、読み直さない");
        log_end(&d, "/r", "run1", 0, "完了", 0, 1).unwrap();
        let c = parsed_log(&d);
        assert!(!std::sync::Arc::ptr_eq(&a, &c), "書き込まれたら、読み直す");
        assert_eq!(c.lines.len(), 2);
        assert_eq!(recent_runs(&d, "/r", 5)[0].state, "done", "読み直した結果が、使われる");
    }

    #[test]
    fn unreadable_log_lines_are_counted_not_silently_dropped_and_status_reports_the_count() {
        let _lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let (_g, r) = git_repo("unreadable");
        let data = tmp("unreadabledata");
        let key = repo_key(r.to_str().unwrap()).unwrap();
        log_start(&data, &key, "run1", "abc", "s", &["bugs".to_string()]).unwrap();
        log_end(&data, &key, "run1", 0, "完了", 0, 1).unwrap();
        assert_eq!(status(&data, r.to_str().unwrap()).unwrap().log_unreadable, 0);
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(data.join(LOG)).unwrap();
        writeln!(f, "壊れた行 1").unwrap();
        writeln!(f, "{{\"at_ms\": \"数字ではない\", \"repo\": 1}}").unwrap();
        writeln!(f).unwrap();
        drop(f);
        let s = status(&data, r.to_str().unwrap()).unwrap();
        assert_eq!(s.log_unreadable, 2, "空行は数えず、壊れた行と、形の違う行を数える");
        assert_eq!(s.recent.len(), 1, "読める行は、これまでどおり使える");
        assert_eq!(log_unreadable(&data), 2);
    }
}
