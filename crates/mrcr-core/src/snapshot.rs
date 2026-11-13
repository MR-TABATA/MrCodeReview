//! レビューに送る差分を、ある時点の状態として固定する。
//!
//! 「直しながら押す」ので、結果が返るころには作業ツリーが変わっている。
//! 結果は HEAD と差分のハッシュに紐づけ、どの状態に対する指摘かを後から言えるようにする。
//! 送らなかったもの（未追跡・大きすぎる・ロックファイル・バイナリ）も記録して見せる。

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// 1 ファイルあたりの上限。これを超えるファイルは送らない。
pub const MAX_FILE_BYTES: usize = 60_000;
/// 送る差分の合計の上限。
pub const MAX_TOTAL_BYTES: usize = 200_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileDiff {
    pub path: String,
    pub bytes: usize,
    /// この版では保存と表示のために持つ。ファイルごとの差分本文。
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Excluded {
    pub path: String,
    pub reason: String,
}

/// 比較元（差分の元になるコミット）。差分は、これからいまの作業ツリーまで。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Base {
    /// 選んだ値（`HEAD`・`HEAD~1`・コミットのハッシュ・ブランチ名など）。
    pub spec: String,
    /// 解決したコミットのハッシュ（40 桁）。差分は、このハッシュで取る。
    pub hash: String,
    /// そのコミットの 1 行説明
    pub subject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub repo: String,
    pub head: String,
    /// 比較元。選ばなければ `HEAD`（未コミットの変更だけ）。
    pub base: Base,
    /// 送る差分全体の FNV-1a 64（hex）。結果をこの状態に紐づけるためのもの。
    pub diff_hash: String,
    pub files: Vec<FileDiff>,
    pub excluded: Vec<Excluded>,
    pub diff_bytes: usize,
    /// 実際に送る差分（files の text をつないだもの）。
    pub diff_text: String,
}

/// 未追跡ファイルを差分として取り込む上限の件数。超えた分は 1 行にまとめて「送らない」に出す。
pub const MAX_UNTRACKED: usize = 200;
/// 送らないロックファイル（ファイル名）。
pub const DEFAULT_LOCKFILES: [&str; 8] = [
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "Cargo.lock",
    "Gemfile.lock",
    "poetry.lock",
    "composer.lock",
    "go.sum",
];

/// 差分の取り込みの上限と除外。設定（`settings.rs`）から作る。`Default` は推奨値。
#[derive(Debug, Clone)]
pub struct Limits {
    pub max_file_bytes: usize,
    pub max_total_bytes: usize,
    pub max_untracked: usize,
    pub lockfiles: Vec<String>,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_file_bytes: MAX_FILE_BYTES,
            max_total_bytes: MAX_TOTAL_BYTES,
            max_untracked: MAX_UNTRACKED,
            lockfiles: DEFAULT_LOCKFILES.map(String::from).to_vec(),
        }
    }
}

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        // 日本語などのパスを八進数のエスケープにしない
        .args(["-c", "core.quotepath=false"])
        .args(args)
        .output()
        .map_err(|e| format!("git を起動できません: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!("git {}: {}", args.join(" "), err));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `git` を実行して、終了コードと出力を返す。**終了コードが 0 でなくても、エラーにしない**（コードで「該当なし」と「失敗」を分けるため）。
/// `git` を起動できないときだけ `Err`。
fn git_code(repo: &Path, args: &[&str]) -> Result<(i32, String, String), String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "core.quotepath=false"])
        .args(args)
        .output()
        .map_err(|e| format!("git を起動できません: {e}"))?;
    Ok((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    ))
}

/// FNV-1a 64。標準ライブラリの Hasher は版をまたいで値が変わりうるので使わない。
pub fn fnv1a64(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// `git diff` の出力を、ファイルごとの塊に分ける。
pub fn split_diff(diff: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut cur: Option<(String, String)> = None;
    for line in diff.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            if let Some(c) = cur.take() {
                out.push(c);
            }
            // `diff --git a/X b/X`。パスに空白があっても、最後の " b/" で切れば足りる。
            let path = line
                .trim_end()
                .rsplit_once(" b/")
                .map(|(_, p)| p.to_string())
                .unwrap_or_else(|| line.trim_end().to_string());
            cur = Some((path, String::new()));
        }
        if let Some((_, text)) = cur.as_mut() {
            text.push_str(line);
        }
    }
    if let Some(c) = cur.take() {
        out.push(c);
    }
    out
}

/// 新しいファイルの塊（`new file mode` で始まる）から、`index <旧>..<新>` の行を外す。
/// 変更・削除・名前の変更の塊は、そのまま（過去の記録の識別子を、変えないため）。
fn without_new_file_index_line(chunk: &str) -> String {
    let mut lines = chunk.split_inclusive('\n');
    let (Some(first), Some(second)) = (lines.next(), lines.next()) else { return chunk.to_string() };
    if !first.starts_with("diff --git ") || !second.starts_with("new file mode ") {
        return chunk.to_string();
    }
    let mut out = String::with_capacity(chunk.len());
    out.push_str(first);
    out.push_str(second);
    if let Some(third) = lines.next() {
        if !third.starts_with("index ") {
            out.push_str(third);
        }
    }
    out.extend(lines);
    out
}

fn is_lockfile(path: &str, lockfiles: &[String]) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    lockfiles.iter().any(|l| l == name)
}

/// 新規ファイルの内容を、全体が追加された行の差分（`git diff` と同じ形式）にする。
/// `git` を起動しない（未追跡ファイルが多いと、1 件ごとの起動が積み重なって遅い）。
/// `index` の行（blob のハッシュ）は付けない。この差分を読む側（`split_diff`・画面・`changed_lines`）は使わない。
pub fn new_file_diff(rel: &str, bytes: &[u8], executable: bool) -> String {
    let mode = if executable { "100755" } else { "100644" };
    let head = format!("diff --git a/{rel} b/{rel}\nnew file mode {mode}\n");
    // git と同じく、先頭 8000 バイトに NUL があればバイナリ
    if bytes.iter().take(8000).any(|b| *b == 0) {
        return format!("{head}Binary files /dev/null and b/{rel} differ\n");
    }
    let text = String::from_utf8_lossy(bytes);
    let ends_with_newline = text.ends_with('\n');
    let mut lines: Vec<&str> = text.split('\n').collect();
    if ends_with_newline {
        lines.pop(); // 末尾の改行の後ろの、空の要素
    }
    let range = if lines.len() == 1 { "1".to_string() } else { format!("1,{}", lines.len()) };
    let mut out = format!("{head}--- /dev/null\n+++ b/{rel}\n@@ -0,0 +{range} @@\n");
    for l in &lines {
        out.push('+');
        out.push_str(l);
        out.push('\n');
    }
    if !ends_with_newline {
        out.push_str("\\ No newline at end of file\n");
    }
    out
}

/// 未追跡ファイル 1 つを、全体が追加された行の差分にする。
/// 読む前にサイズで弾くので、巨大なファイルをメモリに載せない。symlink は辿らない。
fn untracked_chunk(root: &Path, rel: &str, max_file_bytes: usize) -> Result<Option<(String, String)>, Excluded> {
    use std::os::unix::fs::PermissionsExt;
    let skip = |reason: String| Excluded { path: rel.to_string(), reason };
    if rel.contains('\n') {
        // `diff --git` の 1 行に収められない。差分の形式を壊さないよう、取り込まない
        return Err(skip("ファイル名に改行を含む".into()));
    }
    let full = root.join(rel);
    let meta = std::fs::symlink_metadata(&full).map_err(|e| skip(format!("読めない（{e}）")))?;
    if meta.file_type().is_symlink() {
        return Err(skip("シンボリックリンク（辿らない）".into()));
    }
    if meta.is_dir() {
        return Ok(None); // 入れ子の git リポジトリなど。中身は追わない
    }
    if meta.len() == 0 {
        // git は空ファイルでも「新規ファイル」の差分（中身なし）を返す。送ってもノイズなので弾く。
        return Err(skip("空のファイル".into()));
    }
    if meta.len() as usize > max_file_bytes {
        return Err(skip(format!("1 ファイルの上限を超えた（{} バイト > {max_file_bytes}）", meta.len())));
    }
    let bytes = std::fs::read(&full).map_err(|e| skip(format!("読めない（{e}）")))?;
    Ok(Some((rel.to_string(), new_file_diff(rel, &bytes, meta.permissions().mode() & 0o111 != 0))))
}

/// 未追跡ファイルの一覧から、差分の塊と、取り込めなかったものを作る。
#[cfg_attr(not(test), allow(dead_code))]
pub fn untracked_chunks(root: &Path, paths: &[String]) -> (Vec<(String, String)>, Vec<Excluded>) {
    untracked_chunks_with(root, paths, &Limits::default())
}

pub fn untracked_chunks_with(root: &Path, paths: &[String], limits: &Limits) -> (Vec<(String, String)>, Vec<Excluded>) {
    let mut chunks = Vec::new();
    let mut excluded = Vec::new();
    for rel in paths.iter().take(limits.max_untracked) {
        match untracked_chunk(root, rel, limits.max_file_bytes) {
            Ok(Some(c)) => chunks.push(c),
            Ok(None) => {}
            Err(e) => excluded.push(e),
        }
    }
    if paths.len() > limits.max_untracked {
        excluded.push(Excluded {
            path: format!("（他の未追跡 {} 件）", paths.len() - limits.max_untracked),
            reason: format!("未追跡が多すぎる（先頭 {} 件だけ取り込んだ）", limits.max_untracked),
        });
    }
    (chunks, excluded)
}

/// 分けた差分から、送るもの・送らないものを決める（git を呼ばない純粋な部分）。
#[cfg_attr(not(test), allow(dead_code))]
pub fn select(chunks: Vec<(String, String)>) -> (Vec<FileDiff>, Vec<Excluded>) {
    select_with(chunks, &Limits::default())
}

pub fn select_with(chunks: Vec<(String, String)>, limits: &Limits) -> (Vec<FileDiff>, Vec<Excluded>) {
    let mut files = Vec::new();
    let mut excluded = Vec::new();
    let mut total = 0usize;
    for (path, text) in chunks {
        let bytes = text.len();
        if is_lockfile(&path, &limits.lockfiles) {
            excluded.push(Excluded { path, reason: "ロックファイル".into() });
        } else if text.contains("\nBinary files ") || text.contains("\nGIT binary patch") {
            excluded.push(Excluded { path, reason: "バイナリ".into() });
        } else if bytes > limits.max_file_bytes {
            excluded.push(Excluded {
                path,
                reason: format!("1 ファイルの上限を超えた（{bytes} バイト > {}）", limits.max_file_bytes),
            });
        } else if total + bytes > limits.max_total_bytes {
            excluded.push(Excluded {
                path,
                reason: format!("合計の上限を超えた（上限 {} バイト）", limits.max_total_bytes),
            });
        } else {
            total += bytes;
            files.push(FileDiff { path, bytes, text });
        }
    }
    (files, excluded)
}

/// 作業ツリーを HEAD と比べた差分（ステージ済みと未ステージの両方）を固定する。
#[cfg_attr(not(test), allow(dead_code))]
pub fn take(repo: &str) -> Result<Snapshot, String> {
    take_with(repo, &Limits::default())
}

/// リポジトリの、いまの HEAD のハッシュ。取れなければ None（リポジトリが消えた、コミットが無い、など）。
pub fn head_of(repo: &str) -> Option<String> {
    git(Path::new(repo), &["rev-parse", "HEAD"]).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// リポジトリの根（`git rev-parse --show-toplevel`）。サブディレクトリを渡されても、根を返す。
pub fn toplevel(repo: &str) -> Result<String, String> {
    let top = git(Path::new(repo), &["rev-parse", "--show-toplevel"]).map_err(|_| format!("git のリポジトリではありません: {repo}"))?;
    Ok(top.trim().to_string())
}

/// 最初のコミットのハッシュ（ルートコミットが複数あれば、並べて先頭）。
/// フォルダを移した・クローンした・ワークツリーを作った、を同じリポジトリと見分けるのに使う（構想書 §8.3）。
/// 出ているブランチ（`HEAD`）だけでなく、すべての参照（`--all`）から数える。ルートコミットが複数あるリポジトリで、
/// ブランチ違いのワークツリーやクローンが、別のリポジトリに見えないようにするため。
/// （片方が一部のブランチしか持たないクローン（`--single-branch` など）では、ルートの集まりが違い、同じとは見分けられないことがある）
///
/// - `Ok(None)`: 最初のコミットが「無い」。コミットの無いリポジトリ、git が「リポジトリではない」と言うフォルダ、フォルダが無い。
/// - `Err`: 調べられなかった（git を起動できない、リポジトリなのに `rev-list` が失敗した、権限・I/O のエラーなど理由の分からない失敗）。「無い」と取り違えない。
pub fn first_commit(repo: &str) -> Result<Option<String>, String> {
    match git(Path::new(repo), &["rev-list", "--max-parents=0", "--all"]) {
        Ok(out) => {
            let mut roots: Vec<&str> = out.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            roots.sort_unstable();
            Ok(roots.first().map(|h| h.to_string()))
        }
        Err(e) => {
            // 「無い」と言ってよいのは、リポジトリではないと git 自身が言ったとき（フォルダが無い場合も含む）だけ。
            // 権限・I/O のエラーなど、理由の分からない失敗は、「無い」にせず、失敗として返す
            if e.starts_with("git を起動できません") {
                return Err(format!("最初のコミットを調べられません: {e}"));
            }
            if !Path::new(repo).is_dir() {
                return Ok(None);
            }
            // 理由は、ロケールで変わる文なので、C ロケールで聞く
            let probe = Command::new("git").arg("-C").arg(repo).args(["rev-parse", "--git-dir"]).env("LC_ALL", "C").env("LANGUAGE", "C").output();
            match probe {
                Ok(o) if o.status.success() => Err(format!("最初のコミットを調べられません: {e}")),
                Ok(o) if String::from_utf8_lossy(&o.stderr).contains("not a git repository") => Ok(None),
                Ok(o) => Err(format!("最初のコミットを調べられません（git: {}）: {e}", String::from_utf8_lossy(&o.stderr).trim())),
                Err(pe) => Err(format!("最初のコミットを調べられません: git を起動できません: {pe}")),
            }
        }
    }
}

/// 「最初から（リポジトリ全体）」を表す、特別な値。`:` は、ブランチ名・タグ名に使えない文字なので、本物の名前とぶつからない。
pub const ROOT_SPEC: &str = ":root";

/// 比較元に使える書き方か。`git` に、オプションとして読まれる値（`-` で始まる）や、空白・制御文字を含む値は、渡さない。
fn check_spec(spec: &str) -> Result<(), String> {
    if spec.starts_with('-') || spec.chars().any(|c| c.is_whitespace() || c.is_control()) || spec.len() > 200 {
        return Err(format!("比較元 `{}` は使えません（コミット・ブランチ・タグの名前を、空白なしで入れてください）", spec.chars().take(60).collect::<String>()));
    }
    Ok(())
}

/// 比較元を、コミットのハッシュに解決する。空なら `HEAD`。見つからなければ、理由つきのエラー。
pub fn resolve_base(root: &Path, spec: &str) -> Result<Base, String> {
    let s = spec.trim();
    let s = if s.is_empty() { "HEAD" } else { s };
    if s == ROOT_SPEC {
        // 何も無い状態（git の空のツリー。SHA-1 でも SHA-256 のリポジトリでも、git に聞く）。最初のコミットの内容も、追加として出る
        let tree = git(root, &["hash-object", "-t", "tree", "/dev/null"])?.trim().to_string();
        return Ok(Base { spec: ROOT_SPEC.into(), hash: tree, subject: "（何も無い状態）".into() });
    }
    check_spec(s)?;
    let hash = git(root, &["rev-parse", "--verify", "--quiet", &format!("{s}^{{commit}}")])
        .map_err(|_| format!("比較元 `{s}` が見つかりません（ここに入れられるのは、いまの履歴にある、コミット・ブランチ・タグの名前です。コミットが 1 つしかないときは、1 つ前はありません。リポジトリ全体を見たいときは、一覧の「最初から（リポジトリ全体）」を選んでください）"))?
        .trim()
        .to_string();
    let subject = git(root, &["log", "-1", "--format=%s", &hash]).unwrap_or_default().trim().to_string();
    Ok(Base { spec: s.to_string(), hash, subject })
}

pub fn take_with(repo: &str, limits: &Limits) -> Result<Snapshot, String> {
    take_with_base(repo, limits, None)
}

/// `base` から、いまの作業ツリー（ステージ済み・未ステージ・未追跡）までの差分を固定する。`None` なら `HEAD`（未コミットの変更だけ）。
pub fn take_with_base(repo: &str, limits: &Limits, base: Option<&str>) -> Result<Snapshot, String> {
    let top = toplevel(repo)?;
    let root = Path::new(&top);
    let head = git(root, &["rev-parse", "HEAD"])
        .map_err(|_| "コミットがまだありません（HEAD が無い）".to_string())?
        .trim()
        .to_string();
    let base = resolve_base(root, base.unwrap_or("HEAD"))?;
    let diff = git(root, &["diff", &base.hash, "--no-color", "--no-ext-diff", "--"])?;
    // -z: 名前に改行を含むファイルがあっても、行で分けて壊さない
    let untracked_raw = git(root, &["ls-files", "-z", "--others", "--exclude-standard"])?;
    let untracked: Vec<String> = untracked_raw
        .split('\0')
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();

    let mut chunks = split_diff(&diff);
    // 登録した（`git add` / `git add -N`）新しいファイルは、`git diff` が `index` の行を付ける。未登録のものには付かない。
    // 登録の有無で、差分の識別子が変わらないよう、新しいファイルの塊からは、この行を外す。
    for (_, text) in &mut chunks {
        *text = without_new_file_index_line(text);
    }
    let (new_chunks, mut untracked_excluded) = untracked_chunks_with(root, &untracked, limits);
    chunks.extend(new_chunks);
    let (files, mut excluded) = select_with(chunks, limits);
    excluded.append(&mut untracked_excluded);
    let diff_text: String = files.iter().map(|f| f.text.as_str()).collect();
    Ok(Snapshot {
        repo: top,
        head,
        base,
        diff_hash: fnv1a64(diff_text.as_bytes()),
        diff_bytes: diff_text.len(),
        files,
        excluded,
        diff_text,
    })
}

/// 比較元の選択肢 1 つ。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BaseOption {
    /// `take_with_base` に渡す値
    pub value: String,
    pub label: String,
    pub hint: String,
    /// 「前回のレビューの続き」のときだけ。画面が、日付と、選んだ観点との突き合わせに使う。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<LastInfo>,
}

/// 「前回のレビューの続き」の材料。履歴（`store::last_review`）が作る。DB には触れない（このモジュールは、履歴を知らない）。
#[derive(Debug, Clone, PartialEq)]
pub struct LastReview {
    /// 前回（成功した最新の 1 件）の、レビュー時点の HEAD。
    pub head: String,
    /// 前回の実行時刻（UNIX ミリ秒）。
    pub created_at: u64,
    /// 前回と同じ HEAD で、そのリポジトリの最新のレビューが取られている観点。ここに無い観点は、前回より前の変更を見ていない。
    pub covered: Vec<String>,
}

/// 画面に渡す、前回の情報。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LastInfo {
    pub created_at: u64,
    pub head_short: String,
    pub covered: Vec<String>,
}

/// 「前回のレビューの続き」の選択肢。
///
/// - **使えないとき（該当なし）は `Ok(None)`**（黙って全体を送らない。選択肢を出さないだけ）: 前回の HEAD がいまの履歴に無い（rebase・amend・gc）／いまの HEAD の祖先でない（別のブランチ）。
/// - **git 自体の失敗は `Err`（原因つき）。** 「該当なし」と同じ「出さない」にして、理由を失わせない（呼ぶ側が、原因を残す）。
///
/// 値は、解決したコミットのハッシュ（`fork_point` と同じ型）。差分は、これまでどおり、そのハッシュからいまの作業ツリーまで。
pub fn last_review_option(root: &Path, last: &LastReview, lang: &str) -> Result<Option<BaseOption>, String> {
    let head = last.head.trim();
    if head.is_empty() || check_spec(head).is_err() {
        return Ok(None);
    }
    // `--verify --quiet` は、無いとき、何も出さずに終了コード 1。それ以外の失敗（リポジトリを開けない、など）は、無いのではなく、エラー
    let (code, out, err) = git_code(root, &["rev-parse", "--verify", "--quiet", &format!("{head}^{{commit}}")])?;
    match code {
        0 => {}
        1 => return Ok(None),
        c => return Err(format!("git rev-parse が失敗しました（終了コード {c}）: {err}")),
    }
    let hash = out.trim().to_string();
    if hash.is_empty() {
        return Ok(None);
    }
    // 終了コード 1 = 祖先でない（別のブランチ）。それ以外の失敗は、エラー
    let (code, _, err) = git_code(root, &["merge-base", "--is-ancestor", &hash, "HEAD"])?;
    match code {
        0 => {}
        1 => return Ok(None),
        c => return Err(format!("git merge-base が失敗しました（終了コード {c}）: {err}")),
    }
    let short: String = hash.chars().take(7).collect();
    let t = crate::i18n::Lang::from_code(lang).unwrap_or_default().texts();
    Ok(Some(BaseOption {
        value: hash,
        label: t.base_last_review.into(),
        hint: t.base_last_review_hint(&short),
        last: Some(LastInfo { created_at: last.created_at, head_short: short, covered: last.covered.clone() }),
    }))
}

fn default_branch(root: &Path) -> Option<String> {
    if let Ok(s) = git(root, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]) {
        let s = s.trim();
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    ["main", "master"]
        .into_iter()
        .find(|n| git(root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{n}")]).is_ok())
        .map(str::to_string)
}

/// ブランチの上にいるとき、既定のブランチ（main など）と分かれた場所。既定のブランチの上にいる、または、分かれていなければ None。
fn fork_point(root: &Path) -> Option<(String, String, String)> {
    let cur = git(root, &["branch", "--show-current"]).ok()?.trim().to_string();
    if cur.is_empty() {
        return None; // detached HEAD
    }
    let def = default_branch(root)?;
    if def == cur || def == format!("origin/{cur}") {
        return None;
    }
    let mb = git(root, &["merge-base", "HEAD", &def]).ok()?.trim().to_string();
    let head = git(root, &["rev-parse", "HEAD"]).ok()?.trim().to_string();
    if mb.is_empty() || mb == head {
        return None;
    }
    Some((def, mb.clone(), git(root, &["log", "-1", "--format=%h %s", &mb]).unwrap_or_default().trim().to_string()))
}

/// 画面の「比較元」に出す選択肢。未コミットの変更・直近のコミット・（ブランチなら）既定のブランチとの分岐点・最近のコミット。
#[allow(dead_code)]
pub fn list_bases(repo: &str) -> Result<Vec<BaseOption>, String> {
    list_bases_with_lang(repo, crate::i18n::Lang::default().code())
}

pub fn list_bases_with_lang(repo: &str, lang: &str) -> Result<Vec<BaseOption>, String> {
    list_bases_with_last(repo, lang, None)
}

/// `list_bases_with_lang` に、「前回のレビューの続き」（`last`）を差し込む版。`last` が使えないときは、差し込まない。
pub fn list_bases_with_last(repo: &str, lang: &str, last: Option<&LastReview>) -> Result<Vec<BaseOption>, String> {
    let top = toplevel(repo)?;
    let root = Path::new(&top);
    let t = crate::i18n::Lang::from_code(lang).unwrap_or_default().texts();
    let mut out = vec![BaseOption {
        value: "HEAD".into(),
        label: t.base_uncommitted.into(),
        hint: t.base_uncommitted_hint.into(),
        last: None,
    }];
    // %x1f: 区切り文字（件名に何が入っても、壊れない）
    let log = git(root, &["log", "-n", "11", "--format=%H%x1f%h%x1f%P%x1f%s"]).unwrap_or_default();
    let commits: Vec<(String, String, bool, String)> = log
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\u{1f}').collect();
            (f.len() == 4).then(|| (f[0].to_string(), f[1].to_string(), !f[2].trim().is_empty(), f[3].to_string()))
        })
        .collect();
    if let Some((_, short, has_parent, subject)) = commits.first() {
        if *has_parent {
            out.push(BaseOption {
                value: "HEAD~1".into(),
                label: t.base_latest.into(),
                hint: t.base_latest_hint(short, subject),
                last: None,
            });
        }
    }
    if let Some(l) = last {
        match last_review_option(root, l, lang) {
            Ok(Some(o)) => out.push(o),
            Ok(None) => {}
            // git の失敗は、「該当なし」と同じ「出さない」にしつつ、原因を残す（ほかの選択肢は出す）
            Err(e) => eprintln!("前回のレビューの続きを出せません（ほかの選択肢だけを出します）: {e}"),
        }
    }
    if let Some((name, hash, what)) = fork_point(root) {
        out.push(BaseOption {
            value: hash,
            label: t.base_fork_label(&name),
            hint: t.base_fork_hint(&what, &name),
            last: None,
        });
    }
    // 最初のコミットの内容も含めて、全部。コミットが 1 つだけのリポジトリでも、使える
    if !commits.is_empty() {
        out.push(BaseOption {
            value: ROOT_SPEC.into(),
            label: t.base_root.into(),
            hint: t.base_root_hint.into(),
            last: None,
        });
    }
    for (hash, short, has_parent, subject) in commits.iter().skip(1) {
        if *has_parent {
            out.push(BaseOption {
                value: format!("{hash}^"),
                label: format!("{short} {subject}"),
                hint: t.base_commit_hint.into(),
                last: None,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "diff --git a/src/a.js b/src/a.js\nindex 1..2 100644\n--- a/src/a.js\n+++ b/src/a.js\n@@ -1 +1,2 @@\n x\n+y\ndiff --git a/dir with space/b.js b/dir with space/b.js\n--- a/dir with space/b.js\n+++ b/dir with space/b.js\n@@ -1 +1 @@\n-a\n+b\n";

    fn run_git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// 3 つのコミット（a.txt の追加 → a.txt に 2 行目 → b.txt の追加）と、未コミットの変更（a.txt に 3 行目・未追跡の c.txt）。
    fn base_repo(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mrcr-base-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        run_git(&dir, &["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-q", "-m", "最初"]);
        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        run_git(&dir, &["commit", "-q", "-am", "2 行目"]);
        std::fs::write(dir.join("b.txt"), "b\n").unwrap();
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-q", "-m", "b を足す"]);
        std::fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").unwrap();
        std::fs::write(dir.join("c.txt"), "c\n").unwrap();
        dir
    }

    fn paths(s: &Snapshot) -> Vec<&str> {
        let mut v: Vec<&str> = s.files.iter().map(|f| f.path.as_str()).collect();
        v.sort_unstable();
        v
    }

    /// 新しいファイルを `git add -N`（や `git add`）で登録しても、差分の識別子は変わらない。
    /// 登録の前後で変わるのは、Git が付ける `index` の行（blob のハッシュ）だけで、内容は同じ。
    /// これが変わると、「レビューのあとに、コードが変わった」と誤って判定される。
    #[test]
    fn registering_a_new_file_with_git_does_not_change_the_diff_hash() {
        let dir = base_repo("register");
        let d = dir.to_str().unwrap();
        let lim = Limits::default();
        let untracked = take_with(d, &lim).unwrap();
        let chunk = |s: &Snapshot, p: &str| s.files.iter().find(|f| f.path == p).unwrap().text.clone();
        assert!(!chunk(&untracked, "c.txt").contains("\nindex "), "未登録の新しいファイルには、index の行が無い");

        run_git(&dir, &["add", "-N", "c.txt"]);
        let intent = take_with(d, &lim).unwrap();
        assert_eq!(intent.diff_hash, untracked.diff_hash, "git add -N しても、識別子は同じ");
        assert_eq!(chunk(&intent, "c.txt"), chunk(&untracked, "c.txt"), "差分の文面も、同じ");

        run_git(&dir, &["add", "c.txt"]);
        let staged = take_with(d, &lim).unwrap();
        assert_eq!(staged.diff_hash, untracked.diff_hash, "git add しても、識別子は同じ");

        // 変更されたファイル（追跡中）の差分は、いじらない。index の行は、そのまま残る（過去の記録の識別子を変えないため）
        assert!(chunk(&staged, "a.txt").contains("\nindex "), "変更されたファイルの index の行は、残す");
        // 内容が変われば、ちゃんと変わる
        std::fs::write(dir.join("c.txt"), "c changed\n").unwrap();
        assert_ne!(take_with(d, &lim).unwrap().diff_hash, untracked.diff_hash, "内容が変われば、識別子も変わる");
    }

    #[test]
    fn the_index_line_is_dropped_only_from_new_file_chunks() {
        let new = "diff --git a/x b/x\nnew file mode 100644\nindex 0000000..e69de29\n--- /dev/null\n+++ b/x\n@@ -0,0 +1 @@\n+index 1\n";
        let want = "diff --git a/x b/x\nnew file mode 100644\n--- /dev/null\n+++ b/x\n@@ -0,0 +1 @@\n+index 1\n";
        assert_eq!(without_new_file_index_line(new), want, "内容の行（+index 1）は、触らない");
        assert_eq!(without_new_file_index_line(want), want, "もう無ければ、そのまま");
        let modified = "diff --git a/x b/x\nindex 1..2 100644\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n";
        assert_eq!(without_new_file_index_line(modified), modified, "変更されたファイルは、そのまま");
        let deleted = "diff --git a/x b/x\ndeleted file mode 100644\nindex e69de29..0000000\n--- a/x\n+++ /dev/null\n";
        assert_eq!(without_new_file_index_line(deleted), deleted, "削除されたファイルは、そのまま");
        for short in ["", "diff --git a/x b/x\n", "diff --git a/x b/x\nnew file mode 100644\n"] {
            assert_eq!(without_new_file_index_line(short), short, "短い塊も、壊さない: {short:?}");
        }
    }

    #[test]
    fn the_base_decides_the_range_and_the_default_is_unchanged() {
        let dir = base_repo("range");
        let d = dir.to_str().unwrap();
        let lim = Limits::default();
        // 既定（HEAD）: 従来どおり。未コミットの変更と、未追跡だけ
        let head = take_with(d, &lim).unwrap();
        let none = take_with_base(d, &lim, None).unwrap();
        let explicit = take_with_base(d, &lim, Some("HEAD")).unwrap();
        assert_eq!(paths(&head), ["a.txt", "c.txt"]);
        assert_eq!((head.diff_hash.as_str(), head.base.hash.as_str()), (none.diff_hash.as_str(), head.head.as_str()), "指定しなければ、HEAD");
        assert_eq!(explicit.diff_hash, head.diff_hash);
        assert_eq!((head.base.spec.as_str(), head.base.subject.as_str()), ("HEAD", "b を足す"));
        // 直近のコミット: 1 つ前から。b.txt の追加（直近のコミット）も入る。a.txt は 3 行目だけ
        let last = take_with_base(d, &lim, Some("HEAD~1")).unwrap();
        assert_eq!(paths(&last), ["a.txt", "b.txt", "c.txt"]);
        let a = last.files.iter().find(|f| f.path == "a.txt").unwrap();
        assert!(a.text.contains("+three") && !a.text.contains("+two"), "{}", a.text);
        assert_eq!(last.base.subject, "2 行目");
        assert_ne!(last.diff_hash, head.diff_hash);
        // 最初のコミットから: a.txt は 2 行目と 3 行目の両方
        let first = run_git(&dir, &["rev-list", "--max-parents=0", "HEAD"]);
        let all = take_with_base(d, &lim, Some(&first)).unwrap();
        let a = all.files.iter().find(|f| f.path == "a.txt").unwrap();
        assert!(a.text.contains("+two") && a.text.contains("+three"), "{}", a.text);
        assert_eq!(all.base.hash, first);
        // ブランチ名・タグ名・`ハッシュ^` も使える
        run_git(&dir, &["tag", "v0"]);
        let by_tag = take_with_base(d, &lim, Some("v0")).unwrap();
        assert_eq!(by_tag.diff_hash, head.diff_hash, "タグが HEAD を指していれば、HEAD と同じ");
        let by_caret = take_with_base(d, &lim, Some(&format!("{}^", head.head))).unwrap();
        assert_eq!(by_caret.diff_hash, last.diff_hash);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn from_the_beginning_shows_the_whole_repo_including_the_first_commit() {
        let dir = base_repo("root");
        let d = dir.to_str().unwrap();
        let all = take_with_base(d, &Limits::default(), Some(ROOT_SPEC)).unwrap();
        assert_eq!(paths(&all), ["a.txt", "b.txt", "c.txt"], "追跡中のファイルが、全部。未追跡の c.txt も");
        let a = all.files.iter().find(|f| f.path == "a.txt").unwrap();
        assert!(a.text.contains("+one") && a.text.contains("+two") && a.text.contains("+three"), "最初のコミットの内容（one）も、追加として出る: {}", a.text);
        assert!(a.text.contains("new file mode"), "ファイルが、新しく作られたものとして出る");
        assert_eq!((all.base.spec.as_str(), all.base.subject.as_str()), (":root", "（何も無い状態）"));
        assert_eq!(all.base.hash.len(), 40, "空のツリーのハッシュ");
        // 「最初のコミットから」とは違う（そちらは、最初のコミットの内容が出ない）
        let first = run_git(&dir, &["rev-list", "--max-parents=0", "HEAD"]);
        let after_first = take_with_base(d, &Limits::default(), Some(&first)).unwrap();
        let a2 = after_first.files.iter().find(|f| f.path == "a.txt").unwrap();
        assert!(!a2.text.contains("+one"), "最初のコミットを比較元にすると、最初の内容は出ない");
        // 見つからないときの案内に、この選択肢が出る
        let e = take_with_base(d, &Limits::default(), Some("最初から")).err().unwrap();
        assert!(e.contains("最初から（リポジトリ全体）"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unusable_bases_are_refused_before_git_diff_runs() {
        let dir = base_repo("bad");
        let d = dir.to_str().unwrap();
        let marker = std::env::temp_dir().join(format!("mrcr-base-marker-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let lim = Limits::default();
        // オプションとして読まれる値（git diff --output=<file> は、ファイルを書く）、空白・改行・制御文字、存在しないもの
        let output_opt = format!("--output={}", marker.display());
        for bad in [output_opt.as_str(), "-x", "--help", "a b", "a\nb", "tab\tx", "no-such-ref-xyz", "HEAD~99", "v9.9.9"] {
            let e = take_with_base(d, &lim, Some(bad)).err().unwrap_or_else(|| panic!("`{bad}` が通った"));
            assert!(e.contains("比較元"), "{bad}: {e}");
        }
        assert!(!marker.exists(), "--output が git に渡って、ファイルができた");
        // 長すぎる値
        assert!(take_with_base(d, &lim, Some(&"a".repeat(201))).is_err());
        // 空白だけは、HEAD として扱う（未選択と同じ）
        assert_eq!(take_with_base(d, &lim, Some("  ")).unwrap().base.spec, "HEAD");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_repo_with_one_commit_has_no_previous_commit_and_says_so() {
        let dir = std::env::temp_dir().join(format!("mrcr-base-one-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        run_git(&dir, &["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("a.txt"), "x\n").unwrap();
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-q", "-m", "only"]);
        let d = dir.to_str().unwrap();
        let e = take_with_base(d, &Limits::default(), Some("HEAD~1")).err().unwrap();
        assert!(e.contains("1 つ前はありません"), "{e}");
        let opts = list_bases(d).unwrap();
        assert_eq!(opts.iter().map(|o| o.value.as_str()).collect::<Vec<_>>(), ["HEAD", ":root"], "1 つ前は無いが、最初から（全体）は選べる");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn base_options_list_head_last_commit_the_fork_point_and_recent_commits() {
        let dir = base_repo("options");
        let d = dir.to_str().unwrap();
        let o = list_bases(d).unwrap();
        let values: Vec<&str> = o.iter().map(|x| x.value.as_str()).collect();
        let (h1, h2) = (run_git(&dir, &["rev-parse", "HEAD~1"]), run_git(&dir, &["rev-parse", "HEAD~2"]));
        // main の上にいるときは、分岐点は出ない。最初のコミットには、1 つ前が無いので出ない
        assert_eq!(values, ["HEAD", "HEAD~1", ":root", &format!("{h1}^")], "{values:?}");
        assert_eq!(o[0].label, "未コミットの変更");
        assert!(o[1].label == "直近のコミット" && o[1].hint.contains("b を足す"), "{:?}", o[1]);
        assert_eq!(o[2].label, "最初から（リポジトリ全体）");
        assert!(o[3].label.contains("2 行目") && o[3].hint.contains("このコミット以降すべて"), "{:?}", o[3]);
        // どの値も、実際に使える
        for v in &values {
            assert!(take_with_base(d, &Limits::default(), Some(v)).is_ok(), "{v}");
        }
        // ブランチに切ると、main との分岐点が、直近のコミットの次に入る
        run_git(&dir, &["stash", "-u", "-q"]);
        run_git(&dir, &["switch", "-q", "-c", "feature"]);
        std::fs::write(dir.join("f.txt"), "f\n").unwrap();
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-q", "-m", "feature の仕事"]);
        let o = list_bases(d).unwrap();
        assert_eq!(o[2].label, "main との分岐点から", "{o:?}");
        assert_eq!(o[3].value, ":root", "分岐点の次に、最初から");
        assert_eq!(o[2].value, run_git(&dir, &["rev-parse", "main"]), "分岐点は、main の先頭");
        assert!(o[2].hint.contains("b を足す"));
        let fork = take_with_base(d, &Limits::default(), Some(&o[2].value)).unwrap();
        assert_eq!(paths(&fork), ["f.txt"], "ブランチで行った変更だけ");
        // detached HEAD: 分岐点は出ない
        run_git(&dir, &["checkout", "-q", "--detach"]);
        assert!(list_bases(d).unwrap().iter().all(|x| !x.label.contains("分岐点")));
        let _ = h2;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn last_review_option_comes_after_the_latest_commit_and_diffs_from_the_previous_head() {
        let dir = base_repo("last-ok");
        let d = dir.to_str().unwrap();
        let prev_head = run_git(&dir, &["rev-parse", "HEAD"]);
        // 前回のレビューのあとに、1 つコミットする
        std::fs::write(dir.join("d.txt"), "d\n").unwrap();
        run_git(&dir, &["add", "d.txt"]);
        run_git(&dir, &["commit", "-q", "-m", "d を足す"]);
        let last = LastReview { head: prev_head.clone(), created_at: 1_700_000_000_000, covered: vec!["security".into()] };

        let o = list_bases_with_last(d, "ja", Some(&last)).unwrap();
        assert_eq!(o[1].value, "HEAD~1");
        assert_eq!(o[2].value, prev_head, "直近のコミットの次。値は、解決したコミットのハッシュ");
        assert_eq!(o[2].label, "前回のレビューの続き");
        assert!(o[2].hint.contains(&prev_head[..7]), "{:?}", o[2]);
        let info = o[2].last.as_ref().unwrap();
        assert_eq!((info.created_at, info.head_short.as_str(), info.covered.as_slice()), (1_700_000_000_000, &prev_head[..7], &["security".to_string()][..]));
        assert!(o.iter().filter(|b| b.last.is_some()).count() == 1, "他の選択肢には、前回の情報を付けない");

        // 英語
        let en = list_bases_with_last(d, "en", Some(&last)).unwrap();
        assert_eq!(en[2].label, "Since the last review");

        // 差分は、前回の HEAD から作業ツリーまで: 前回より前のコミット（b.txt）は入らず、前回のあとの変更（d.txt）は入る。
        // 前回のレビュー時に未コミットだった変更（a.txt・c.txt）は、また出る（見落とす側ではなく、重複する側）
        let snap = take_with_base(d, &Limits::default(), Some(&o[2].value)).unwrap();
        let names: Vec<&str> = snap.files.iter().map(|f| f.path.as_str()).collect();
        assert!(names.contains(&"d.txt"), "{names:?}");
        assert!(!names.contains(&"b.txt"), "{names:?}");
        assert!(names.contains(&"a.txt") && names.contains(&"c.txt"), "{names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn last_review_option_is_not_offered_when_it_cannot_be_used() {
        let dir = base_repo("last-unusable");
        let d = dir.to_str().unwrap();
        let root = Path::new(d);
        let mk = |head: &str| LastReview { head: head.into(), created_at: 1, covered: vec![] };
        // 履歴に無い（rebase・amend・gc で消えた）
        assert!(last_review_option(root, &mk("0123456789abcdef0123456789abcdef01234567"), "ja").unwrap().is_none());
        // 空・オプションに読まれる値
        assert!(last_review_option(root, &mk(""), "ja").unwrap().is_none());
        assert!(last_review_option(root, &mk("-x"), "ja").unwrap().is_none());
        // 祖先でない（別のブランチ）
        run_git(&dir, &["stash", "-u", "-q"]);
        let first = run_git(&dir, &["rev-list", "--max-parents=0", "HEAD"]);
        run_git(&dir, &["switch", "-q", "-c", "side", &first]);
        std::fs::write(dir.join("side.txt"), "s\n").unwrap();
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-q", "-m", "side"]);
        let side = run_git(&dir, &["rev-parse", "HEAD"]);
        run_git(&dir, &["switch", "-q", "main"]);
        assert!(last_review_option(root, &mk(&side), "ja").unwrap().is_none(), "別のブランチの HEAD は、続きにならない");
        // 祖先なら出る（同じ地点でも）
        assert!(last_review_option(root, &mk(&first), "ja").unwrap().is_some());
        let here = run_git(&dir, &["rev-parse", "HEAD"]);
        assert!(last_review_option(root, &mk(&here), "ja").unwrap().is_some(), "前回と同じ地点（未コミットの変更だけが続き）");
        // 前回が無いとき、選択肢は従来と同じ
        assert_eq!(list_bases_with_last(d, "ja", None).unwrap(), list_bases_with_lang(d, "ja").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn last_review_option_tells_a_git_failure_apart_from_not_applicable() {
        let mk = |head: &str| LastReview { head: head.into(), created_at: 1, covered: vec![] };
        // 該当なし（履歴に無い）は Ok(None)。リポジトリを開けない（git の失敗）は Err で、原因が残る
        let dir = base_repo("last-git-fail");
        assert!(last_review_option(Path::new(dir.to_str().unwrap()), &mk("0123456789abcdef0123456789abcdef01234567"), "ja").unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
        let gone = std::env::temp_dir().join(format!("mrcr-no-such-dir-{}", std::process::id()));
        let e = last_review_option(&gone, &mk("0123456789abcdef0123456789abcdef01234567"), "ja").unwrap_err();
        assert!(e.contains("git rev-parse が失敗しました"), "{e}");
    }

    #[test]
    fn list_bases_with_lang_returns_english_labels_and_hints_when_en() {
        let dir = base_repo("options-en");
        let d = dir.to_str().unwrap();
        let o = list_bases_with_lang(d, "en").unwrap();
        assert_eq!(o[0].value, "HEAD");
        assert_eq!(o[0].label, "Uncommitted changes");
        assert_eq!(o[0].hint, "Only uncommitted changes");
        assert_eq!(o[1].value, "HEAD~1");
        assert_eq!(o[1].label, "Latest commit");
        assert!(o[1].hint.contains("(Last commit and uncommitted changes)"), "{:?}", o[1]);
        assert_eq!(o[2].value, ROOT_SPEC);
        assert_eq!(o[2].label, "Entire repository (from start)");
        assert!(o[3].hint.contains("All changes from this commit forward"), "{:?}", o[3]);

        run_git(&dir, &["stash", "-u", "-q"]);
        run_git(&dir, &["switch", "-q", "-c", "feature"]);
        std::fs::write(dir.join("f.txt"), "f\n").unwrap();
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-q", "-m", "feature work"]);
        let o_branch = list_bases_with_lang(d, "en").unwrap();
        assert_eq!(o_branch[2].label, "Fork point with main");
        assert!(o_branch[2].hint.contains("(All changes since branching from main)"), "{:?}", o_branch[2]);
        assert_eq!(o_branch[3].value, ROOT_SPEC);
        assert_eq!(o_branch[3].label, "Entire repository (from start)");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn splits_per_file_and_keeps_paths_with_spaces() {
        let v = split_diff(SAMPLE);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].0, "src/a.js");
        assert_eq!(v[1].0, "dir with space/b.js");
        assert!(v[0].1.contains("+y"));
        assert!(!v[0].1.contains("+b"));
        // つなぎ直すと元に戻る
        let joined: String = v.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(joined, SAMPLE);
    }

    #[test]
    fn empty_diff_has_no_files() {
        assert!(split_diff("").is_empty());
    }

    #[test]
    fn excludes_lockfile_binary_and_oversize_with_reasons() {
        let big = format!("diff --git a/big.txt b/big.txt\n{}", "+x\n".repeat(MAX_FILE_BYTES));
        let chunks = vec![
            ("package-lock.json".to_string(), "diff --git a/package-lock.json b/package-lock.json\n+x\n".to_string()),
            ("img.png".to_string(), "diff --git a/img.png b/img.png\nindex 1..2\nBinary files a/img.png and b/img.png differ\n".to_string()),
            ("big.txt".to_string(), big),
            ("ok.js".to_string(), "diff --git a/ok.js b/ok.js\n+1\n".to_string()),
        ];
        let (files, excluded) = select(chunks);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "ok.js");
        let reason = |p: &str| excluded.iter().find(|e| e.path == p).unwrap().reason.clone();
        assert_eq!(reason("package-lock.json"), "ロックファイル");
        assert_eq!(reason("img.png"), "バイナリ");
        assert!(reason("big.txt").contains("1 ファイルの上限"));
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mrcr-snap-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn untracked_file_becomes_an_all_added_diff() {
        let dir = scratch("untracked");
        std::fs::write(dir.join("new.js"), "a\nb\n").unwrap();
        std::fs::create_dir_all(dir.join("日本語")).unwrap();
        std::fs::write(dir.join("日本語/メモ.md"), "x\n").unwrap();
        let (chunks, excluded) = untracked_chunks(&dir, &["new.js".into(), "日本語/メモ.md".into()]);
        assert!(excluded.is_empty(), "{excluded:?}");
        assert_eq!(chunks.len(), 2);
        // split_diff が読むのと同じパスで取れる（引用符・エスケープされない）
        let split = split_diff(&chunks[1].1);
        assert_eq!(split[0].0, "日本語/メモ.md");
        assert_eq!(chunks[0].0, "new.js");
        assert!(chunks[0].1.contains("new file mode"));
        assert!(chunks[0].1.contains("+a\n+b\n"));
        assert!(!chunks[0].1.contains("-a"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn untracked_empty_oversize_binary_and_vanished_files_are_excluded_with_reasons() {
        let dir = scratch("untracked-ex");
        std::fs::write(dir.join("empty.txt"), "").unwrap();
        std::fs::write(dir.join("big.txt"), vec![b'x'; MAX_FILE_BYTES + 1]).unwrap();
        std::fs::write(dir.join("bin.dat"), [0u8, 159, 146, 150, 0, 1, 2]).unwrap();
        let names: Vec<String> = ["empty.txt", "big.txt", "bin.dat", "gone.txt"].map(String::from).to_vec();
        let (chunks, excluded) = untracked_chunks(&dir, &names);
        let reason = |p: &str| excluded.iter().find(|e| e.path == p).map(|e| e.reason.clone());
        assert_eq!(reason("empty.txt").as_deref(), Some("空のファイル"));
        assert!(reason("big.txt").unwrap().contains("1 ファイルの上限"));
        assert!(reason("gone.txt").unwrap().contains("読めない"));
        // バイナリは差分としては取れるが、select で「バイナリ」として弾かれる
        let (files, ex2) = select(chunks);
        assert!(files.is_empty());
        assert_eq!(ex2.len(), 1);
        assert_eq!(ex2[0].reason, "バイナリ");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn too_many_untracked_files_are_capped_and_the_rest_is_reported() {
        let dir = scratch("untracked-cap");
        let names: Vec<String> = (0..MAX_UNTRACKED + 5).map(|i| format!("f{i}.txt")).collect();
        for n in &names {
            std::fs::write(dir.join(n), "x\n").unwrap();
        }
        let (chunks, excluded) = untracked_chunks(&dir, &names);
        assert_eq!(chunks.len(), MAX_UNTRACKED);
        assert_eq!(excluded.len(), 1);
        assert!(excluded[0].path.contains("他の未追跡 5 件"));
        assert!(excluded[0].reason.contains("未追跡が多すぎる"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn total_limit_drops_later_files() {
        let chunk = |name: &str| {
            (
                name.to_string(),
                format!("diff --git a/{name} b/{name}\n{}", "+x\n".repeat(15_000)),
            )
        };
        let (files, excluded) = select(vec![chunk("a"), chunk("b"), chunk("c"), chunk("d"), chunk("e")]);
        assert_eq!(files.len(), 4); // 1 つ約 45,000 バイト。4 つ（約 180,000）までは上限内
        assert_eq!(excluded.len(), 1);
        assert!(excluded[0].reason.contains("合計の上限"));
    }

    #[test]
    fn new_file_diff_has_the_shape_the_readers_expect() {
        assert_eq!(
            new_file_diff("a.js", b"x\ny\n", false),
            "diff --git a/a.js b/a.js\nnew file mode 100644\n--- /dev/null\n+++ b/a.js\n@@ -0,0 +1,2 @@\n+x\n+y\n"
        );
        assert!(new_file_diff("a.js", b"x\n", false).contains("@@ -0,0 +1 @@\n+x\n"), "1 行のときは、行数を書かない");
        assert!(new_file_diff("a.js", b"x\ny", false).ends_with("+y\n\\ No newline at end of file\n"));
        assert!(new_file_diff("run.sh", b"x\n", true).contains("new file mode 100755"));
        assert!(new_file_diff("a.bin", &[1, 0, 2], false).contains("\nBinary files /dev/null and b/a.bin differ\n"));
        // 読む側が、同じように読める
        let chunks = split_diff(&new_file_diff("dir/a b.js", b"p\nq\n", false));
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, "dir/a b.js");
    }

    /// 本物の `git diff --no-index` の出力と、同じ行を持つこと（`index` の行だけが違う）。
    #[test]
    fn new_file_diff_matches_real_git_output() {
        let dir = scratch("gitcmp");
        for (name, content) in [
            ("plain.txt", &b"alpha\nbeta\ngamma\n"[..]),
            ("single.txt", b"only\n"),
            ("no-newline.txt", b"a\nb"),
            ("crlf.txt", b"a\r\nb\r\n"),
            ("日本語.txt", "こんにちは\n世界\n".as_bytes()),
            ("tab\there.txt", b"\tindented\n  spaced\n"),
        ] {
            std::fs::write(dir.join(name), content).unwrap();
            let real = Command::new("git")
                .current_dir(&dir)
                .args(["-c", "core.quotepath=false", "diff", "--no-index", "--no-color", "--", "/dev/null", name])
                .output()
                .unwrap();
            // `lines()` は `\r` を落とすので使わない（CRLF のファイルでも、行末の `\r` を比べる）
            let real: String = String::from_utf8_lossy(&real.stdout)
                .split_inclusive('\n')
                .filter(|l| !l.starts_with("index "))
                .collect();
            // git は、引用符で包むパス（タブなど）を `"a/…"` と書く。ここでは、読む側が見る形（素のパス）だけを比べる
            if name.contains('\t') {
                continue;
            }
            assert_eq!(new_file_diff(name, content, false), real, "{name}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn untracked_symlinks_and_names_with_newlines_are_excluded_not_followed() {
        use std::os::unix::fs::symlink;
        let dir = scratch("symlink");
        let outside = scratch("symlink-outside");
        std::fs::write(outside.join("secret.txt"), "TOP SECRET\n").unwrap();
        symlink(outside.join("secret.txt"), dir.join("link.txt")).unwrap();
        std::fs::write(dir.join("ok.txt"), "fine\n").unwrap();
        std::fs::write(dir.join("a\nb.txt"), "weird\n").unwrap();
        let names: Vec<String> = ["link.txt", "ok.txt", "a\nb.txt"].map(String::from).to_vec();
        let (chunks, excluded) = untracked_chunks(&dir, &names);
        assert_eq!(chunks.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), vec!["ok.txt"]);
        assert!(chunks.iter().all(|c| !c.1.contains("TOP SECRET")), "リンク先の中身を読まない");
        let reason = |p: &str| excluded.iter().find(|e| e.path == p).map(|e| e.reason.clone());
        assert!(reason("link.txt").unwrap().contains("シンボリックリンク"));
        assert!(reason("a\nb.txt").unwrap().contains("改行"));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn fnv_is_stable() {
        // 値が版をまたいで変わらないことの固定（結果の紐づけに使うため）
        assert_eq!(fnv1a64(b""), "cbf29ce484222325");
        assert_eq!(fnv1a64(b"a"), "af63dc4c8601ec8c");
    }
}
