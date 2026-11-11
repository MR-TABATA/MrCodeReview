//! 登録リポジトリ（構想書 §8）。登録の置き場は SQLite の `repos` 表。画面は、ここが返す一覧を表示するだけ。
//!
//! - **枠 ＝ 行数。** 解除（`hidden`）した行も数える。Free は `FREE_REPO_LIMIT`（3）、上限に達していると、新しい行が要る追加は断る。
//! - 解除は、履歴が 1 件もなければ行ごと消し（枠が戻る）、あれば非表示にするだけ（枠は戻らない）。
//! - 枠を空けるのは「データを削除」だけ（履歴・仕分け・メモ・報告と行を、1 つのトランザクションで消す）。
//! - **同じリポジトリの見分け方:** 根のパス → 最初のコミットのハッシュ。フォルダの移動・クローン・ワークツリーは、同じ行にまとまる。
//! - 超過（上限を超える登録）は、追加だけを止める。すでにある登録は、そのまま使える。
//! - 登録していない（または解除した）リポジトリは、実行の入口（`require_registered`）で断る。

use crate::{db, repodata, snapshot};
use rusqlite::{params, Connection, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 登録を変えるたびに進める番号。`require_registered` の「通した」記憶を、登録が変わったら捨てるために使う。
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// 「通した」記憶の有効な長さ。フォルダが別のリポジトリに置き換わったときに、いつまでも通し続けないための上限。
const ALLOWED_TTL: Duration = Duration::from_secs(60);

type AllowedKey = (PathBuf, String);

/// 登録のパスと一致しない入口（サブディレクトリ・クローン・ワークツリー）を通した記憶。`git` を毎回起動しないため。
fn allowed() -> &'static Mutex<HashMap<AllowedKey, (u64, Instant)>> {
    static CACHE: OnceLock<Mutex<HashMap<AllowedKey, (u64, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn bump_generation() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// 登録できる件数の上限。`None` は無制限（OSS 化で、枠は無くした）。
pub fn limit() -> Option<usize> {
    None
}

const IMPORTED_FLAG: &str = "imported_repos_localstorage";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RepoRow {
    pub id: String,
    /// リポジトリの根のパス（履歴は、このパスで結びつく）
    pub path: String,
    pub name: Option<String>,
    pub hidden: bool,
    #[serde(skip)]
    first_commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RepoList {
    pub repos: Vec<RepoRow>,
    /// 枠の上限。`None` は無制限
    pub limit: Option<usize>,
    /// 取り込みなどで起きた、止めなかった失敗（最初のコミットを調べられなかった、など）。その呼び出しの結果にだけ付く
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AddOutcome {
    #[serde(flatten)]
    pub list: RepoList,
    /// 追加（または復元）した行のパス。上限で断ったときは `None`
    pub path: Option<String>,
    /// 上限に達していて、新しい行を作れなかった
    pub limit_reached: bool,
    /// 選んだフォルダとは別の場所に、同じリポジトリが登録済みだった（クローン・ワークツリー）。その登録のパス。
    /// 行は変えず、`path` はその登録のパスを返す（旧パスのフォルダが残っているので、そちらを使える）。
    pub already_at: Option<String>,
    /// 登録のフォルダが見つからず、選んだフォルダが「移動した先」かもしれない。その、見つからない登録のパス。
    /// 外付けドライブが未接続なだけかもしれないので、何も変えていない。移動したと確認できたら、`confirm_move` つきで、もう一度追加する。
    pub move_from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnlinkOutcome {
    #[serde(flatten)]
    pub list: RepoList,
    /// 行ごと消えた（履歴が 0 件だった）。`false` は、非表示にしただけ
    pub removed: bool,
    /// フォルダが無くなっていて、どの登録のものか決められない履歴の件数。数えも消しもしていない
    pub unattributed_reviews: usize,
}

/// 以前の版が `localStorage` に持っていた登録（初回の取り込み用）。
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyRepo {
    pub id: Option<String>,
    pub path: String,
    pub name: Option<String>,
    pub hidden: Option<bool>,
}

// ── 同じリポジトリの見分け ───────────────────────────────────────────────

struct Ident {
    root: String,
    first_commit: Option<String>,
}

/// 根のパスだけ（git を 1 回）。見つからない・git でないときは、渡されたパスのまま。
fn root_of(path: &str) -> String {
    repodata::repo_key(path.trim())
}

/// 調べられなかったとき（git の失敗）は、エラー。「最初のコミットが無い」とは区別する（取り違えて、別の行を作らない）。
/// 根のパスと最初のコミットは、独立に調べられる（`git -C` は、サブディレクトリからでも同じ答え）ので、並行して聞く。
fn identify(path: &str) -> Result<Ident, String> {
    let path = path.trim();
    let (root, first_commit) = std::thread::scope(|s| {
        let fc = s.spawn(|| snapshot::first_commit(path));
        let root = root_of(path);
        (root, fc.join().unwrap_or_else(|_| Err("最初のコミットの確認が中断しました".into())))
    });
    Ok(Ident { root, first_commit: first_commit? })
}

/// 同時に走らせる git の数の上限（履歴のパスが多くても、プロセスを増やしすぎない）。
const GIT_PARALLEL: usize = 8;

/// 各パスの最初のコミットを、`GIT_PARALLEL` 件ずつ並行して調べる。1 件でも調べられなければ、エラー。
fn first_commits(paths: &[String]) -> Result<Vec<Option<String>>, String> {
    let mut out = Vec::with_capacity(paths.len());
    for chunk in paths.chunks(GIT_PARALLEL) {
        let results: Vec<Result<Option<String>, String>> = std::thread::scope(|s| {
            let handles: Vec<_> = chunk.iter().map(|p| s.spawn(move || snapshot::first_commit(p))).collect();
            handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("最初のコミットの確認が中断しました".into()))).collect()
        });
        for r in results {
            out.push(r?);
        }
    }
    Ok(out)
}

fn same_dir(a: &str, b: &str) -> bool {
    if a.trim_end_matches('/') == b.trim_end_matches('/') {
        return true;
    }
    matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

/// 同じリポジトリの行を探す。先にパス、次に最初のコミット。
fn find<'a>(rows: &'a [RepoRow], root: &str, first_commit: Option<&str>) -> Option<&'a RepoRow> {
    rows.iter().find(|r| same_dir(&r.path, root)).or_else(|| first_commit.and_then(|fc| rows.iter().find(|r| r.first_commit.as_deref() == Some(fc))))
}

// ── 読み書き ─────────────────────────────────────────────────────────────

fn rows(conn: &Connection) -> Result<Vec<RepoRow>, String> {
    let mut stmt = conn
        .prepare("SELECT id, path, name, hidden, first_commit FROM repos ORDER BY position, rowid")
        .map_err(|e| format!("登録を読めません: {e}"))?;
    let v = stmt
        .query_map([], |r| {
            Ok(RepoRow { id: r.get(0)?, path: r.get(1)?, name: r.get(2)?, hidden: r.get::<_, i64>(3)? != 0, first_commit: r.get(4)? })
        })
        .map_err(|e| format!("登録を読めません: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("登録を読めません: {e}"))?;
    Ok(v)
}

fn list_of(conn: &Connection) -> Result<RepoList, String> {
    Ok(RepoList { repos: rows(conn)?, limit: limit(), warnings: Vec::new() })
}

fn tx(conn: &Connection) -> Result<Transaction<'_>, String> {
    Transaction::new_unchecked(conn, TransactionBehavior::Immediate).map_err(|e| format!("登録を更新できません: {e}"))
}

fn new_id() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    format!("repo_{ms}_{}", N.fetch_add(1, Ordering::Relaxed))
}

fn clean_name(name: Option<&str>) -> Option<String> {
    name.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

fn min_position(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT COALESCE(MIN(position), 0) FROM repos", [], |r| r.get(0)).map_err(|e| format!("登録を読めません: {e}"))
}

fn insert(conn: &Connection, id: &str, root: &str, name: Option<&str>, hidden: bool, first_commit: Option<&str>, position: i64) -> Result<(), String> {
    conn.execute(
        "INSERT INTO repos (id, path, name, hidden, first_commit, position) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, root, name, hidden as i64, first_commit, position],
    )
    .map_err(|e| format!("登録を保存できません: {e}"))?;
    Ok(())
}

fn find_id<'a>(rows: &'a [RepoRow], id: &str) -> Result<&'a RepoRow, String> {
    rows.iter().find(|r| r.id == id).ok_or_else(|| "登録が見つかりません（すでに消えた可能性があります）".to_string())
}

/// この行に結びつく履歴のパス（`reviews.repo`）。
struct History {
    /// 行のパスに加えて、同じ最初のコミットを持つ別のパス（クローン・ワークツリー）で実行した履歴。
    /// 別の行として登録されているパスは、その行のものなので除く。
    keys: Vec<String>,
    /// どの登録のものか、決められない履歴のパス。フォルダが無くなっていて、最初のコミットを調べられない。
    /// このリポジトリのものかもしれないが、分からないので、数えも消しもしない（呼び出し側が、あることを伝える）。
    unattributed: Vec<String>,
}

/// `paths` のうち、まだ調べていないものの最初のコミットを、並行して調べて、`known` に入れる。
fn fill_first_commits(known: &mut HashMap<String, Option<String>>, paths: &[String]) -> Result<(), String> {
    let missing: Vec<String> = paths.iter().filter(|p| !known.contains_key(*p)).cloned().collect();
    for (p, found) in missing.iter().zip(first_commits(&missing)?) {
        known.insert(p.clone(), found);
    }
    Ok(())
}

/// 最初のコミットを調べられない（git の失敗）ときは、エラー。何も変えずに止める。
/// `known` は、調べ済みの最初のコミット（パス → 結果）。ここに無いものだけ、`git` で調べて足す。
/// 書き込みのロックの中で `git` を待たせないよう、呼び出し側は、先にロックの外で一度呼んで、`known` を温めておく。
fn history_of(conn: &Connection, all: &[RepoRow], row: &RepoRow, known: &mut HashMap<String, Option<String>>) -> Result<History, String> {
    let mut h = History { keys: vec![row.path.clone()], unattributed: Vec::new() };
    let mut stmt = conn.prepare("SELECT DISTINCT repo FROM reviews").map_err(|e| format!("履歴のパスを読めません: {e}"))?;
    let repos = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("履歴のパスを読めません: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("履歴のパスを読めません: {e}"))?;
    // フォルダが無いものは、git を起動せずに「決められない」とする。残りを、並行して調べる
    let mut candidates: Vec<String> = Vec::new();
    for repo in repos {
        if h.keys.contains(&repo) || all.iter().any(|r| same_dir(&r.path, &repo)) {
            continue; // この行のパス、または別の行のパス
        }
        if Path::new(&repo).is_dir() {
            candidates.push(repo);
        } else {
            h.unattributed.push(repo);
        }
    }
    // この行の最初のコミット。行に無ければ（取り込みのとき調べられなかった、など）、いま調べる。調べられなければ、エラー
    let fc = match &row.first_commit {
        Some(f) => Some(f.clone()),
        None if Path::new(&row.path).is_dir() => {
            fill_first_commits(known, std::slice::from_ref(&row.path))?;
            known.get(&row.path).cloned().flatten()
        }
        None => None,
    };
    match fc {
        Some(fc) => {
            fill_first_commits(known, &candidates)?;
            for repo in &candidates {
                match known.get(repo).and_then(|f| f.as_deref()) {
                    Some(x) if x == fc => h.keys.push(repo.clone()),
                    Some(_) => {} // 別のリポジトリのもの
                    None => h.unattributed.push(repo.clone()), // フォルダはあるが、リポジトリではなくなった
                }
            }
        }
        // この行の最初のコミットが分からないので、フォルダのあるパスも、どの登録のものか決められない。触らず、あることを伝える
        None => h.unattributed.extend(candidates),
    }
    Ok(h)
}

/// 書き込みのロックを取る前に、履歴の帰属に要る `git` の確認を済ませる（結果は `known` に残る）。
fn warm_history(conn: &Connection, id: &str) -> Result<HashMap<String, Option<String>>, String> {
    let mut known = HashMap::new();
    let all = rows(conn)?;
    let row = find_id(&all, id)?;
    history_of(conn, &all, row, &mut known)?;
    Ok(known)
}

/// 履歴のパスの一覧の、レビューの件数の合計。
fn review_total(conn: &Connection, keys: &[String]) -> Result<usize, String> {
    let mut n = 0;
    for k in keys {
        n += repodata::review_count(conn, k)?;
    }
    Ok(n)
}

// ── 操作 ─────────────────────────────────────────────────────────────────

#[cfg(test)]
pub fn list(dir: &Path) -> Result<RepoList, String> {
    list_of(&db::open(dir)?)
}

/// 追加する。同じリポジトリが（解除済みを含めて）すでにあれば、新しい行は作らず、復元して先頭に出す。
/// 名前は、渡されたときだけ更新する（渡されなければ、付けておいた名前を残す。消すのは `rename`）。
/// 新しい行が要るとき、上限に達していれば、何も変えずに `limit_reached` を返す。
#[cfg(test)]
pub fn add(dir: &Path, path: &str, name: Option<&str>) -> Result<AddOutcome, String> {
    add_with(dir, path, name, false)
}

/// `confirm_move` が偽のとき、登録のフォルダが無く、選んだフォルダと同じ最初のコミットだったら、何も変えずに `move_from` を返す
/// （登録と履歴の付け替えは、移動したと確認できたときだけ）。真なら、付け替える。
pub fn add_with(dir: &Path, path: &str, name: Option<&str>, confirm_move: bool) -> Result<AddOutcome, String> {
    add_at(dir, path, name, confirm_move, limit())
}

/// `add_with` の本体。上限を引数で受ける（試験で、Free と Pro を、旗を触らずに確かめるため）。
fn add_at(dir: &Path, path: &str, name: Option<&str>, confirm_move: bool, limit: Option<usize>) -> Result<AddOutcome, String> {
    if path.trim().is_empty() {
        return Err("パスが空です".into());
    }
    let ident = identify(path).map_err(|e| format!("{e}（何も変えていません。もう一度試してください）"))?;
    let name = clean_name(name);
    let conn = db::open(dir)?;
    let t = tx(&conn)?;
    let all = rows(&t)?;
    let front = min_position(&t)? - 1;
    let (result_path, already_at);
    if let Some(row) = find(&all, &ident.root, ident.first_commit.as_deref()) {
        // 旧パスのフォルダが無い（移動した）なら、登録を新しい根へ移す。履歴も、新しいパスへ付け替える（結びつきを切らない）。
        // 旧パスのフォルダが残っている（クローン・ワークツリー）なら、行はそのまま。
        let moved = !Path::new(&row.path).is_dir() && !same_dir(&row.path, &ident.root);
        if moved && !confirm_move {
            // 外付けドライブの未接続などで、見えないだけかもしれない。確認が取れるまで、何も変えない（tx は、確定せずに捨てる）
            return Ok(AddOutcome { list: RepoList { repos: all.clone(), limit, warnings: Vec::new() }, path: None, limit_reached: false, already_at: None, move_from: Some(row.path.clone()) });
        }
        if moved {
            // 新しい根のパスを持つ別の行があるときは、付け替えない（同じパスの行が 2 つになる）。通常は、パスの一致が先に選ばれるので、起きない
            let clash: i64 = t
                .query_row("SELECT COUNT(*) FROM repos WHERE path = ?2 AND id <> ?1", params![row.id, ident.root], |r| r.get(0))
                .map_err(|e| format!("登録を確かめられません: {e}"))?;
            if clash > 0 {
                return Err(format!("同じパスの登録がすでにあるので、付け替えられません: {}", ident.root));
            }
            t.execute("UPDATE reviews SET repo = ?2 WHERE repo = ?1", params![row.path, ident.root]).map_err(|e| format!("履歴のパスを付け替えられません: {e}"))?;
            t.execute("UPDATE repos SET path = ?2 WHERE id = ?1", params![row.id, ident.root]).map_err(|e| format!("登録のパスを更新できません: {e}"))?;
        }
        t.execute(
            "UPDATE repos SET name = COALESCE(?2, name), hidden = 0, position = ?3, first_commit = COALESCE(first_commit, ?4) WHERE id = ?1",
            params![row.id, name, front, ident.first_commit],
        )
        .map_err(|e| format!("登録を更新できません: {e}"))?;
        result_path = Some(if moved { ident.root.clone() } else { row.path.clone() });
        already_at = (!moved && !same_dir(&row.path, &ident.root)).then(|| row.path.clone());
    } else if limit.is_some_and(|n| all.len() >= n) {
        return Ok(AddOutcome { list: RepoList { repos: all, limit, warnings: Vec::new() }, path: None, limit_reached: true, already_at: None, move_from: None });
    } else {
        insert(&t, &new_id(), &ident.root, name.as_deref(), false, ident.first_commit.as_deref(), front)?;
        result_path = Some(ident.root.clone());
        already_at = None;
    }
    t.commit().map_err(|e| format!("登録を確定できません: {e}"))?;
    bump_generation();
    Ok(AddOutcome { list: RepoList { repos: rows(&conn)?, limit, warnings: Vec::new() }, path: result_path, limit_reached: false, already_at, move_from: None })
}

pub fn rename(dir: &Path, id: &str, name: Option<&str>) -> Result<RepoList, String> {
    let conn = db::open(dir)?;
    find_id(&rows(&conn)?, id)?;
    conn.execute("UPDATE repos SET name = ?2 WHERE id = ?1", params![id, clean_name(name)]).map_err(|e| format!("名前を保存できません: {e}"))?;
    list_of(&conn)
}

/// 解除済みを、復元する。先頭に出す。行は増えない。
pub fn restore(dir: &Path, id: &str) -> Result<RepoList, String> {
    let conn = db::open(dir)?;
    let t = tx(&conn)?;
    find_id(&rows(&t)?, id)?;
    let front = min_position(&t)? - 1;
    t.execute("UPDATE repos SET hidden = 0, position = ?2 WHERE id = ?1", params![id, front]).map_err(|e| format!("復元できません: {e}"))?;
    t.commit().map_err(|e| format!("登録を確定できません: {e}"))?;
    bump_generation();
    list_of(&conn)
}

/// 一覧の中で、上（-1）か下（+1）へ。見えているものの中で動かす（非表示は飛ばす）。端では動かさない。
pub fn move_row(dir: &Path, id: &str, delta: i32) -> Result<RepoList, String> {
    let conn = db::open(dir)?;
    let t = tx(&conn)?;
    let all = rows(&t)?;
    let visible: Vec<&RepoRow> = all.iter().filter(|r| !r.hidden).collect();
    if let Some(i) = visible.iter().position(|r| r.id == id) {
        let j = i as i64 + if delta < 0 { -1 } else { 1 };
        if j >= 0 && (j as usize) < visible.len() {
            // 入れ替える 2 行の位置だけを交換する（間にある解除済みの行の順序は、変わらない）
            let other = visible[j as usize].id.as_str();
            let position_of = |rid: &str| t.query_row("SELECT position FROM repos WHERE id = ?1", [rid], |r| r.get::<_, i64>(0)).map_err(|e| format!("位置を読めません: {e}"));
            let (pa, pb) = (position_of(id)?, position_of(other)?);
            let set = |rid: &str, pos: i64| t.execute("UPDATE repos SET position = ?2 WHERE id = ?1", params![rid, pos]).map_err(|e| format!("並べ替えできません: {e}"));
            if pa != pb {
                set(id, pb)?;
                set(other, pa)?;
            } else {
                // 位置が重なっているとき（交換しても動かない）だけ、全体を振り直す
                let mut order: Vec<&str> = all.iter().map(|r| r.id.as_str()).collect();
                let a = order.iter().position(|x| *x == id).unwrap_or(0);
                let b = order.iter().position(|x| *x == other).unwrap_or(0);
                order.swap(a, b);
                for (pos, rid) in order.iter().enumerate() {
                    set(rid, pos as i64)?;
                }
            }
        }
    }
    t.commit().map_err(|e| format!("登録を確定できません: {e}"))?;
    list_of(&conn)
}

/// 解除する。履歴が 1 件もなければ行ごと消す（枠が戻る）。あれば非表示にするだけ（枠は使い続ける）。
/// 履歴の件数を数えられないときは、何も変えずにエラー（データがあるかもしれないものを、消さない）。
pub fn unlink(dir: &Path, id: &str) -> Result<UnlinkOutcome, String> {
    let conn = db::open(dir)?;
    let mut known = warm_history(&conn, id)?; // git は、書き込みのロックの外で
    let t = tx(&conn)?;
    let all = rows(&t)?;
    let row = find_id(&all, id)?;
    let history = history_of(&t, &all, row, &mut known)?;
    let removed = review_total(&t, &history.keys)? == 0;
    let unattributed_reviews = review_total(&t, &history.unattributed)?;
    if removed {
        t.execute("DELETE FROM repos WHERE id = ?1", params![id]).map_err(|e| format!("登録を消せません: {e}"))?;
    } else {
        t.execute("UPDATE repos SET hidden = 1 WHERE id = ?1", params![id]).map_err(|e| format!("解除できません: {e}"))?;
    }
    t.commit().map_err(|e| format!("登録を確定できません: {e}"))?;
    bump_generation();
    Ok(UnlinkOutcome { list: list_of(&conn)?, removed, unattributed_reviews })
}

/// 「データを削除して、枠を空ける」: データ（履歴・仕分け・メモ・報告）と行を、1 つのトランザクションで消す。**元に戻せない。**
/// 戻り値の最後は、フォルダが無くなっていて、どの登録のものか決められない履歴の件数（消していない）。
pub fn delete_data(dir: &Path, id: &str) -> Result<(repodata::RepoData, RepoList, usize), String> {
    let conn = db::open(dir)?;
    let mut known = warm_history(&conn, id)?; // git は、書き込みのロックの外で
    let t = tx(&conn)?;
    let all = rows(&t)?;
    let row = find_id(&all, id)?;
    let mut removed = repodata::RepoData::default();
    let history = history_of(&t, &all, row, &mut known)?;
    for key in &history.keys {
        removed.add(&repodata::delete_in(&t, key)?);
    }
    let unattributed_reviews = review_total(&t, &history.unattributed)?;
    t.execute("DELETE FROM repos WHERE id = ?1", params![id]).map_err(|e| format!("登録を消せません: {e}"))?;
    t.commit().map_err(|e| format!("削除を確定できません: {e}"))?;
    bump_generation();
    Ok((removed, list_of(&conn)?, unattributed_reviews))
}

/// 取り込み済みか。「行が無い」だけを未取り込みとする。DB の不調などは、エラーにする（再取り込みで、解除した行を戻さない）。
fn import_done(conn: &Connection) -> Result<bool, String> {
    match conn.query_row("SELECT 1 FROM meta WHERE key = ?1", [IMPORTED_FLAG], |_| Ok(())) {
        Ok(()) => Ok(true),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
        Err(e) => Err(format!("取り込みの印を読めません: {e}")),
    }
}

/// 以前の版（`localStorage`）の登録を、初回だけ取り込む。**上限は見ない**（すでにある登録は、すべて取り込む。超過は追加だけを止める）。
/// 同じリポジトリが複数の行になっていたら、1 行にまとめる（片方が使用中なら、使用中）。取り込み済みなら、一覧を返すだけ。
pub fn import_legacy(dir: &Path, legacy: &[LegacyRepo]) -> Result<RepoList, String> {
    let conn = db::open(dir)?;
    // git を使う確認は、書き込みのロックを取る前に済ませる（ロック中に、仕分けなどの書き込みを待たせない）
    let mut idents: Vec<Option<Result<Ident, String>>> = if import_done(&conn)? {
        Vec::new()
    } else {
        legacy.iter().map(|l| (!l.path.trim().is_empty()).then(|| identify(&l.path))).collect()
    };
    let t = tx(&conn)?;
    let done = import_done(&t)?;
    let mut warnings: Vec<String> = Vec::new();
    if !done {
        let mut all = rows(&t)?;
        for (i, l) in legacy.iter().enumerate() {
            if l.path.trim().is_empty() {
                continue;
            }
            // 取り込みは止めない（1 件の不調で、すべての登録を取り込めなくしない）。最初のコミットを調べられなければ、パスだけで登録し、
            // 失敗を呼び出し側に伝える。最初のコミットは、あとで埋める（`backfill_first_commits`）
            // （ロックを取るまでの間に、取り込み済みでなくなった、というまれな場合は、ここで調べる）
            let found = idents.get_mut(i).and_then(Option::take).unwrap_or_else(|| identify(&l.path));
            let ident = match found {
                Ok(i) => i,
                Err(e) => {
                    warnings.push(format!("{}: {e}", l.path.trim()));
                    Ident { root: root_of(&l.path), first_commit: None }
                }
            };
            let hidden = l.hidden.unwrap_or(false);
            if let Some(existing) = find(&all, &ident.root, ident.first_commit.as_deref()) {
                if existing.hidden && !hidden {
                    t.execute("UPDATE repos SET hidden = 0 WHERE id = ?1", params![existing.id]).map_err(|e| format!("取り込めません: {e}"))?;
                }
                continue;
            }
            let id = match &l.id {
                Some(id) if !id.is_empty() && !all.iter().any(|r| &r.id == id) => id.clone(),
                _ => new_id(),
            };
            let name = clean_name(l.name.as_deref());
            insert(&t, &id, &ident.root, name.as_deref(), hidden, ident.first_commit.as_deref(), i as i64)?;
            // 全行を読み直さず、いま足した行を手元の一覧に足す（次の重複の判定に使う）
            all.push(RepoRow { id, path: ident.root, name, hidden, first_commit: ident.first_commit });
        }
        t.execute("INSERT OR IGNORE INTO meta (key, value) VALUES (?1, '1')", [IMPORTED_FLAG]).map_err(|e| format!("取り込みの印を書けません: {e}"))?;
    }
    t.commit().map_err(|e| format!("登録を確定できません: {e}"))?;
    backfill_first_commits(&conn)?; // git は、書き込みのロックの外で（1 行ずつ、短い書き込み）
    bump_generation();
    let mut l = list_of(&conn)?;
    l.warnings = warnings;
    Ok(l)
}

/// 最初のコミットが無い行（取り込みのとき調べられなかった・コミットが無かった、など）を、いま調べて埋める。
/// 調べられなかったものは、そのまま（次に開いたとき、また調べる）。失敗は、止めない。
/// 呼び出し側は、書き込みのロックを持っていないこと（`git` を待つので）。書き込みは、1 行ずつ。
fn backfill_first_commits(conn: &Connection) -> Result<(), String> {
    for row in rows(conn)?.iter().filter(|r| r.first_commit.is_none() && Path::new(&r.path).is_dir()) {
        if let Ok(Some(fc)) = snapshot::first_commit(&row.path) {
            conn.execute("UPDATE repos SET first_commit = ?2 WHERE id = ?1 AND first_commit IS NULL", params![row.id, fc]).map_err(|e| format!("最初のコミットを書けません: {e}"))?;
        }
    }
    Ok(())
}

/// 実行の入口で呼ぶ。登録していない（または解除した）リポジトリは、断る。
/// 登録は、根のパス → 最初のコミットで見分けるので、サブディレクトリ・シンボリックリンク・ワークツリーでも通る。
pub fn require_registered(dir: &Path, repo: &str) -> Result<(), String> {
    let key: AllowedKey = (dir.to_path_buf(), repo.to_string());
    let generation = GENERATION.load(Ordering::SeqCst);
    // 登録のパスと一致しない入口を、通した記憶（登録が変わるか、一定時間たつまで）。git と DB を、毎回使わない
    if allowed().lock().ok().and_then(|m| m.get(&key).copied()).is_some_and(|(g, at)| g == generation && at.elapsed() < ALLOWED_TTL) {
        return Ok(());
    }
    let result = check_registered(dir, repo);
    if result.is_ok() {
        if let Ok(mut m) = allowed().lock() {
            if m.len() > 256 {
                m.clear();
            }
            m.insert(key, (generation, Instant::now()));
        }
    }
    result
}

fn check_registered(dir: &Path, repo: &str) -> Result<(), String> {
    let conn = db::open(dir)?;
    let all = rows(&conn)?;
    // 同じリポジトリの行が、解除済みと使用中の 2 つになることがある（最初のコミットが未取得のまま別の行になった、など）。
    // 使用中の行が 1 つでもあれば、通す。解除済みの行しか無いときだけ、「解除されています」
    let mut matches: Vec<&RepoRow> = all.iter().filter(|r| r.path == repo).collect(); // 画面が渡すのは、登録のパス。git を起動しない
    let root = if matches.iter().any(|r| !r.hidden) { None } else { Some(root_of(repo)) };
    if let Some(root) = &root {
        for r in all.iter().filter(|r| same_dir(&r.path, root)) {
            if !matches.iter().any(|m| m.id == r.id) {
                matches.push(r);
            }
        }
        if !matches.iter().any(|r| !r.hidden) {
            let fc = snapshot::first_commit(root)
                .map_err(|e| format!("登録されているか確かめられませんでした（登録していない、という意味ではありません。もう一度試してください）: {e}"))?;
            if let Some(fc) = fc {
                for r in all.iter().filter(|r| r.first_commit.as_deref() == Some(fc.as_str())) {
                    if !matches.iter().any(|m| m.id == r.id) {
                        matches.push(r);
                    }
                }
            }
        }
    }
    if matches.iter().any(|r| !r.hidden) {
        Ok(())
    } else if !matches.is_empty() {
        Err(format!("このリポジトリは、解除されています。設定の「リポジトリ」で復元してください: {repo}"))
    } else {
        Err(format!("このリポジトリは、登録されていません。先に「リポジトリを追加」で登録してください: {repo}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mrcr-repos-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    /// 架空のリポジトリ（コミット 1 つ）。`seed` を変えると、最初のコミットが変わる（別のリポジトリになる）。
    fn repo(base: &Path, name: &str, seed: &str) -> String {
        let d = base.join(name);
        std::fs::create_dir_all(&d).unwrap();
        git(&d, &["init", "-q", "-b", "main"]);
        std::fs::write(d.join("a.txt"), seed).unwrap();
        git(&d, &["add", "."]);
        git(&d, &["commit", "-q", "-m", seed]);
        d.to_str().unwrap().to_string()
    }

    fn add_review(dir: &Path, repo: &str) {
        let conn = db::open(dir).unwrap();
        conn.execute(
            "INSERT INTO reviews (id, created_at, repo, recipe_id, recipe_title, provider_id, provider_label, model, ok, findings_count, verified_count, duration_ms, record) \
             VALUES (?1, 1, ?2, 'bugs', 'b', 'mock', 'm', 'mock', 1, 0, 0, 1, '{}')",
            params![format!("r-{repo}"), repodata::repo_key(repo)],
        )
        .unwrap();
    }

    fn paths(l: &RepoList) -> Vec<String> {
        l.repos.iter().map(|r| r.path.clone()).collect()
    }

    #[test]
    fn re_adding_a_hidden_repo_restores_it_without_a_new_row_and_the_same_path_never_grows_the_count() {
        let d = scratch("readd");
        let base = scratch("readd-repos");
        let p = repo(&base, "r", "s");
        add(&d, &p, Some("A")).unwrap();
        let id = list(&d).unwrap().repos[0].id.clone();
        add_review(&d, &p);
        unlink(&d, &id).unwrap();
        let o = add(&d, &format!("{p}/"), Some("B")).unwrap(); // 末尾の / があっても同じ
        assert_eq!(o.list.repos.len(), 1);
        let r = &o.list.repos[0];
        assert!(!r.hidden && r.name.as_deref() == Some("B") && r.id == id, "復元され、名前が更新される");
        add(&d, &p, None).unwrap();
        assert_eq!(list(&d).unwrap().repos.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn symlink_subdirectory_clone_and_worktree_count_as_the_same_repo() {
        let d = scratch("same");
        let base = scratch("same-repos");
        let p = repo(&base, "orig", "s");
        add(&d, &p, None).unwrap();
        // シンボリックリンク
        let link = base.join("link");
        std::os::unix::fs::symlink(&p, &link).unwrap();
        // サブディレクトリ
        std::fs::create_dir_all(Path::new(&p).join("sub")).unwrap();
        // クローン（最初のコミットが同じ）
        let clone = base.join("clone");
        assert!(Command::new("git").args(["clone", "-q", &p, clone.to_str().unwrap()]).status().unwrap().success());
        // ワークツリー
        let wt = base.join("wt");
        git(Path::new(&p), &["worktree", "add", "-q", wt.to_str().unwrap(), "-b", "wt"]);
        for same in [link.to_str().unwrap().to_string(), format!("{p}/sub"), clone.to_str().unwrap().to_string(), wt.to_str().unwrap().to_string()] {
            let o = add(&d, &same, None).unwrap();
            assert!(!o.limit_reached);
            assert_eq!(o.list.repos.len(), 1, "{same} は、同じリポジトリ");
            assert!(require_registered(&d, &same).is_ok(), "{same}");
        }
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_moved_folder_updates_the_registration_and_keeps_its_history_while_a_clone_keeps_the_original_row() {
        let d = scratch("moved");
        let base = scratch("moved-repos");
        let old = repo(&base, "old", "s");
        add(&d, &old, Some("名前")).unwrap();
        add_review(&d, &old);
        let old_root = root_of(&old); // 登録されるのは、git が返す正規のパス（/var は /private/var になる）
        let moved = base.join("moved");
        std::fs::rename(&old, &moved).unwrap();
        let new_root = root_of(moved.to_str().unwrap());
        // 確認が取れるまでは、何も変えない（外付けドライブの未接続かもしれない）
        let ask = add(&d, moved.to_str().unwrap(), None).unwrap();
        assert_eq!((ask.path.as_deref(), ask.move_from.as_deref()), (None, Some(old_root.as_str())));
        assert_eq!(ask.list.repos[0].path, old_root, "登録は、そのまま");
        let conn = db::open(&d).unwrap();
        let still: i64 = conn.query_row("SELECT COUNT(*) FROM reviews WHERE repo = ?1", [&old_root], |r| r.get(0)).unwrap();
        assert_eq!(still, 1, "履歴も、そのまま");
        drop(conn);
        assert!(require_registered(&d, &old_root).is_ok(), "確認前は、登録のパスのまま使える");
        // 移動したと確認できたら、付け替える
        let o = add_with(&d, moved.to_str().unwrap(), None, true).unwrap();
        assert_eq!(o.list.repos.len(), 1, "行は増えない");
        assert_eq!((o.path.as_deref(), o.already_at.as_deref()), (Some(new_root.as_str()), None), "新しい場所を選べる");
        assert_eq!(o.list.repos[0].path, new_root, "登録のパスが、新しい根になる");
        assert!(Path::new(&o.list.repos[0].path).is_dir());
        let conn = db::open(&d).unwrap();
        let kept: i64 = conn.query_row("SELECT COUNT(*) FROM reviews WHERE repo = ?1", [&new_root], |r| r.get(0)).unwrap();
        assert_eq!(kept, 1, "履歴も、新しいパスへ付け替わる");
        drop(conn);
        assert!(require_registered(&d, &new_root).is_ok());
        assert!(!unlink(&d, &o.list.repos[0].id).unwrap().removed, "付け替えた履歴が数えられる");

        // クローン: 元のフォルダが残っているので、行は変えず、登録済みの場所を伝える
        let clone = base.join("clone");
        assert!(Command::new("git").args(["clone", "-q", &new_root, clone.to_str().unwrap()]).status().unwrap().success());
        let o = add(&d, clone.to_str().unwrap(), None).unwrap();
        assert_eq!((o.path.as_deref(), o.already_at.as_deref()), (Some(new_root.as_str()), Some(new_root.as_str())));
        assert_eq!(o.list.repos[0].path, new_root);
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn moving_swaps_only_two_rows_and_still_works_when_positions_overlap() {
        let d = scratch("swap");
        let base = scratch("swap-repos");
        let ps: Vec<String> = ["a", "b", "c"].iter().map(|n| repo(&base, n, n)).collect();
        for p in &ps {
            add(&d, p, None).unwrap();
        }
        // 新しいものが先頭: c, b, a。間に解除済みの行があっても、見えている順で動く
        let l = list(&d).unwrap();
        let (id_c, id_b, id_a) = (l.repos[0].id.clone(), l.repos[1].id.clone(), l.repos[2].id.clone());
        add_review(&d, &ps[1]);
        unlink(&d, &id_b).unwrap(); // b を解除（履歴があるので、非表示で残る）
        let l = move_row(&d, &id_c, 1).unwrap();
        let order: Vec<&str> = l.repos.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(order, [id_a.as_str(), id_b.as_str(), id_c.as_str()], "c が a の下へ。解除済みの b は、2 つの間に、元の位置のまま");
        let shown: Vec<&str> = l.repos.iter().filter(|r| !r.hidden).map(|r| r.id.as_str()).collect();
        assert_eq!(shown, [id_a.as_str(), id_c.as_str()]);
        let conn = db::open(&d).unwrap();
        // 位置がすべて重なっていても、入れ替わる
        conn.execute("UPDATE repos SET position = 0", []).unwrap();
        drop(conn);
        let before: Vec<String> = list(&d).unwrap().repos.iter().filter(|r| !r.hidden).map(|r| r.id.clone()).collect();
        let l = move_row(&d, &before[0], 1).unwrap();
        let after: Vec<String> = l.repos.iter().filter(|r| !r.hidden).map(|r| r.id.clone()).collect();
        assert_eq!(after, [before[1].clone(), before[0].clone()]);
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn history_made_in_a_clone_or_worktree_counts_for_unlinking_and_is_deleted_with_the_row() {
        let d = scratch("clonehist");
        let base = scratch("clonehist-repos");
        let p = repo(&base, "orig", "s");
        let other = repo(&base, "other", "so");
        add(&d, &p, None).unwrap();
        add(&d, &other, None).unwrap();
        let clone = base.join("clone");
        assert!(Command::new("git").args(["clone", "-q", &p, clone.to_str().unwrap()]).status().unwrap().success());
        add_review(&d, clone.to_str().unwrap()); // 履歴は、クローンのパスにだけある
        add_review(&d, &other);
        let ids = |l: &RepoList, path: &str| l.repos.iter().find(|r| same_dir(&r.path, path)).unwrap().id.clone();
        let l = list(&d).unwrap();
        let (id_p, id_other) = (ids(&l, &p), ids(&l, &other));

        let u = unlink(&d, &id_p).unwrap();
        assert!(!u.removed, "クローンの履歴があるので、行ごとは消えず、枠も戻らない");
        assert!(u.list.repos.iter().find(|r| r.id == id_p).unwrap().hidden);

        let (removed, l, _) = delete_data(&d, &id_p).unwrap();
        assert_eq!(removed.reviews, 1, "クローンの履歴も、消す対象");
        assert_eq!(l.repos.len(), 1);
        let conn = db::open(&d).unwrap();
        let left: Vec<String> = conn.prepare("SELECT repo FROM reviews").unwrap().query_map([], |r| r.get(0)).unwrap().flatten().collect();
        assert_eq!(left, [repodata::repo_key(&other)], "孤児が残らない。別の行の履歴は、そのまま");
        assert!(l.repos.iter().any(|r| r.id == id_other));
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_failure_reading_the_import_flag_is_an_error_not_a_reimport() {
        let d = scratch("flagerr");
        let conn = db::open(&d).unwrap();
        assert_eq!(import_done(&conn), Ok(false), "行が無ければ、未取り込み");
        conn.execute("INSERT INTO meta (key, value) VALUES (?1, '1')", [IMPORTED_FLAG]).unwrap();
        assert_eq!(import_done(&conn), Ok(true));
        // 印の表を読めない状態（列が無い）: 「未取り込み」にせず、エラー
        let broken = Connection::open_in_memory().unwrap();
        broken.execute_batch("CREATE TABLE meta (x INTEGER);").unwrap();
        assert!(import_done(&broken).unwrap_err().contains("取り込みの印を読めません"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn delete_data_frees_the_slot_and_removes_the_data_and_the_row_together() {
        let d = scratch("del");
        let base = scratch("del-repos");
        let p = repo(&base, "r", "s");
        add(&d, &p, None).unwrap();
        let id = list(&d).unwrap().repos[0].id.clone();
        add_review(&d, &p);
        let (removed, l, _) = delete_data(&d, &id).unwrap();
        assert_eq!(removed.reviews, 1);
        assert!(l.repos.is_empty());
        let conn = db::open(&d).unwrap();
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM reviews", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        assert!(delete_data(&d, &id).unwrap_err().contains("見つかりません"));
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn import_takes_everything_even_over_the_limit_merges_duplicates_and_runs_only_once() {
        let d = scratch("import");
        let base = scratch("import-repos");
        let ps: Vec<String> = (0..5).map(|i| repo(&base, &format!("r{i}"), &format!("s{i}"))).collect();
        let mut legacy: Vec<LegacyRepo> = ps.iter().enumerate().map(|(i, p)| LegacyRepo { id: Some(format!("repo_{i}")), path: p.clone(), name: None, hidden: Some(i == 4) }).collect();
        legacy.push(LegacyRepo { id: Some("dup".into()), path: format!("{}/", ps[0]), name: Some("重複".into()), hidden: None }); // 末尾 / の重複
        let l = import_legacy(&d, &legacy).unwrap();
        assert_eq!(l.repos.len(), 5, "すべて取り込む。重複は 1 行にまとめる");
        assert_eq!(l.repos.iter().filter(|r| r.hidden).count(), 1);
        assert_eq!(l.repos[0].id, "repo_0", "並びは、元のまま");
        assert!(require_registered(&d, &ps[0]).is_ok(), "超過でも、既存の登録は使える");
        // 2 回目は、何もしない
        let again = import_legacy(&d, &[]).unwrap();
        assert_eq!(again.repos.len(), 5);
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn unregistered_and_hidden_repos_are_refused_with_a_reason() {
        let d = scratch("req");
        let base = scratch("req-repos");
        let p = repo(&base, "r", "s");
        let other = repo(&base, "o", "so");
        add(&d, &p, None).unwrap();
        assert!(require_registered(&d, &p).is_ok());
        assert!(require_registered(&d, &other).unwrap_err().contains("登録されていません"));
        assert!(require_registered(&d, "/no/such/dir").unwrap_err().contains("登録されていません"));
        let id = list(&d).unwrap().repos[0].id.clone();
        add_review(&d, &p);
        unlink(&d, &id).unwrap();
        assert!(require_registered(&d, &p).unwrap_err().contains("解除されています"), "非表示の行は、登録済みとして扱わない");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn rename_move_and_restore_keep_the_order_the_user_sees() {
        let d = scratch("order");
        let base = scratch("order-repos");
        let a = repo(&base, "a", "sa");
        let b = repo(&base, "b", "sb");
        let c = repo(&base, "c", "sc");
        for p in [&a, &b, &c] {
            add(&d, p, None).unwrap();
        }
        // 新しいものが先頭: c, b, a
        let l = list(&d).unwrap();
        assert_eq!(paths(&l), [root_of(&c), root_of(&b), root_of(&a)]);
        let id_c = l.repos[0].id.clone();
        let l = move_row(&d, &id_c, 1).unwrap();
        assert_eq!(paths(&l), [root_of(&b), root_of(&c), root_of(&a)], "下へ");
        let l = move_row(&d, &id_c, -1).unwrap();
        let l = move_row(&d, &l.repos[0].id.clone(), -1).unwrap();
        assert_eq!(l.repos[0].id, id_c, "端では動かさない");
        let l = rename(&d, &id_c, Some("  名前  ")).unwrap();
        assert_eq!(l.repos[0].name.as_deref(), Some("名前"));
        assert_eq!(rename(&d, &id_c, Some("  ")).unwrap().repos[0].name, None, "空にすると、フォルダ名に戻る");
        assert!(rename(&d, "nope", None).is_err());
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }
    /// レビュー 1 件と、それに結びつく 4 つの表の行（仕分け・仕分けの履歴・報告・メモ）を入れる。
    fn add_full_review(dir: &Path, id: &str, repo: &str) {
        add_review_with_id(dir, id, repo);
        let conn = db::open(dir).unwrap();
        conn.execute("INSERT INTO triage (review_id, idx, state) VALUES (?1, 0, 'fix')", [id]).unwrap();
        conn.execute("INSERT INTO triage_log (review_id, idx, state, at, source) VALUES (?1, 0, 'fix', 1, 'ui')", [id]).unwrap();
        conn.execute("INSERT INTO agent_report (review_id, idx, verdict, text, reported_at, source, fetched_at) VALUES (?1, 0, 'did', 't', 1, 's', 1)", [id]).unwrap();
        conn.execute("INSERT INTO finding_note (review_id, idx, memo) VALUES (?1, 0, 'メモ')", [id]).unwrap();
    }

    fn add_review_with_id(dir: &Path, id: &str, repo: &str) {
        let conn = db::open(dir).unwrap();
        conn.execute(
            "INSERT INTO reviews (id, created_at, repo, recipe_id, recipe_title, provider_id, provider_label, model, ok, findings_count, verified_count, duration_ms, record) \
             VALUES (?1, 1, ?2, 'bugs', 'b', 'mock', 'm', 'mock', 1, 1, 0, 1, '{}')",
            params![id, repodata::repo_key(repo)],
        )
        .unwrap();
    }

    fn table_rows(dir: &Path, table: &str) -> i64 {
        db::open(dir).unwrap().query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    fn id_of(l: &RepoList, path: &str) -> String {
        l.repos.iter().find(|r| same_dir(&r.path, path)).unwrap().id.clone()
    }

    // ── #1: 履歴の件数を数えられないとき、解除は何も変えずにエラー ──
    #[test]
    fn unlink_fails_without_changing_anything_when_the_history_cannot_be_counted() {
        let d = scratch("uncountable");
        let base = scratch("uncountable-repos");
        let p = repo(&base, "r", "s");
        add(&d, &p, None).unwrap();
        let id = list(&d).unwrap().repos[0].id.clone();
        // reviews から findings_count を無くす（db::open は通るが、件数の集計は失敗する）
        db::open(&d)
            .unwrap()
            .execute_batch("DROP TABLE reviews; CREATE TABLE reviews (id TEXT, repo TEXT, recipe_id TEXT, created_at INTEGER);")
            .unwrap();
        let e = unlink(&d, &id).unwrap_err();
        // 失敗の起点が、履歴の件数の集計（欠かした列 findings_count）であること。履歴のパスの読み取り（repo 列は残してある）など、別の失敗ではない
        assert!(e.contains("件数を数えられません") && e.contains("findings_count"), "{e}");
        assert!(!e.contains("履歴のパスを読めません"), "{e}");
        let l = list(&d).unwrap();
        assert_eq!(l.repos.len(), 1, "行は消えない");
        assert!(!l.repos[0].hidden, "非表示にもならない（履歴があるかもしれないものを、勝手に扱わない）");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── #2: データの削除は、途中で失敗したら、すべて元に戻る ──
    #[test]
    fn delete_data_rolls_back_the_data_and_the_row_when_it_fails_midway() {
        let d = scratch("delrollback");
        let base = scratch("delrollback-repos");
        let p = repo(&base, "r", "s");
        add(&d, &p, None).unwrap();
        let id = list(&d).unwrap().repos[0].id.clone();
        add_full_review(&d, "r1", &p);
        // 途中の表（agent_report）を壊して、削除を失敗させる
        db::open(&d).unwrap().execute_batch("DROP TABLE agent_report; CREATE TABLE agent_report (x INTEGER);").unwrap();
        assert!(delete_data(&d, &id).is_err());
        assert_eq!(table_rows(&d, "reviews"), 1, "履歴は残る");
        assert_eq!(table_rows(&d, "triage"), 1, "先に消した仕分けも、元に戻る");
        assert_eq!(table_rows(&d, "finding_note"), 1);
        assert_eq!(list(&d).unwrap().repos.len(), 1, "登録の行も残る（データだけ消えて、枠が空かない、にならない）");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── #3: データの削除は、5 つの表をすべて消し、別のリポジトリには触れない ──
    #[test]
    fn delete_data_clears_all_five_tables_and_leaves_another_repo_alone() {
        let d = scratch("delall");
        let base = scratch("delall-repos");
        let a = repo(&base, "a", "sa");
        let b = repo(&base, "b", "sb");
        add(&d, &a, None).unwrap();
        add(&d, &b, None).unwrap();
        let l = list(&d).unwrap();
        let (id_a, id_b) = (id_of(&l, &a), id_of(&l, &b));
        add_full_review(&d, "ra", &a);
        add_full_review(&d, "rb", &b);
        let (removed, left, _) = delete_data(&d, &id_a).unwrap();
        assert_eq!((removed.reviews, removed.triaged, removed.memos), (1, 1, 1));
        for table in ["reviews", "triage", "triage_log", "agent_report", "finding_note"] {
            assert_eq!(table_rows(&d, table), 1, "{table}: a の分だけ消え、b の分は残る");
        }
        let conn = db::open(&d).unwrap();
        let rest: String = conn.query_row("SELECT review_id FROM triage", [], |r| r.get(0)).unwrap();
        assert_eq!(rest, "rb");
        assert_eq!(left.repos.len(), 1);
        assert_eq!(left.repos[0].id, id_b);
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── #4: 並べ替えの境界 ──
    #[test]
    fn move_row_edges_hidden_rows_and_unknown_ids_leave_the_order_unchanged() {
        let d = scratch("moveedge");
        let base = scratch("moveedge-repos");
        let (a, b, c) = (repo(&base, "a", "sa"), repo(&base, "b", "sb"), repo(&base, "c", "sc"));
        // 追加順に、新しいものが先頭になる。a, b, c の順に並べたいので、c, b, a の順に追加する
        for p in [&c, &b, &a] {
            add(&d, p, None).unwrap();
        }
        let l = list(&d).unwrap();
        let (id_a, id_b, id_c) = (id_of(&l, &a), id_of(&l, &b), id_of(&l, &c));
        let order = |l: &RepoList| l.repos.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
        assert_eq!(order(&l), [id_a.clone(), id_b.clone(), id_c.clone()]);
        // b を解除（履歴があるので、非表示で残る）→ 並びは [a, (b), c]
        add_review(&d, &b);
        unlink(&d, &id_b).unwrap();
        let before = order(&list(&d).unwrap());

        // c を上へ: 解除済みの b を飛ばして a と入れ替わる
        let l = move_row(&d, &id_c, -1).unwrap();
        let shown: Vec<String> = l.repos.iter().filter(|r| !r.hidden).map(|r| r.id.clone()).collect();
        assert_eq!(shown, [id_c.clone(), id_a.clone()], "解除済みを飛ばして入れ替わる");
        // 戻す
        move_row(&d, &id_c, 1).unwrap();
        assert_eq!(order(&list(&d).unwrap()), before, "往復で、元の並びに戻る（解除済みの位置も変わらない）");

        // 変わらないもの: 先頭の上・末尾の下・解除済み自身・存在しない id
        for (id, delta) in [(&id_a, -1), (&id_c, 1), (&id_b, -1), (&id_b, 1), (&"nope".to_string(), 1)] {
            assert_eq!(order(&move_row(&d, id, delta).unwrap()), before, "{id} {delta}");
        }
        // delta が 0 や大きな値でも、1 つ分として扱われ、壊れない
        let l = move_row(&d, &id_a, 0).unwrap(); // 0 は下として扱う
        assert_eq!(l.repos.iter().filter(|r| !r.hidden).count(), 2);
        move_row(&d, &id_a, i32::MIN).unwrap();
        move_row(&d, &id_a, i32::MAX).unwrap();
        assert_eq!(list(&d).unwrap().repos.len(), 3, "行は増減しない");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── #5: 復元 ──
    #[test]
    fn restore_brings_a_hidden_row_back_to_the_top_without_adding_a_row() {
        let d = scratch("restore");
        let base = scratch("restore-repos");
        let (a, b) = (repo(&base, "a", "sa"), repo(&base, "b", "sb"));
        add(&d, &a, None).unwrap();
        add(&d, &b, None).unwrap(); // 並びは [b, a]
        let l = list(&d).unwrap();
        let (id_a, id_b) = (id_of(&l, &a), id_of(&l, &b));
        add_review(&d, &a);
        unlink(&d, &id_a).unwrap(); // a は履歴があるので、非表示で残る
        assert!(list(&d).unwrap().repos.iter().find(|r| r.id == id_a).unwrap().hidden);
        let l = restore(&d, &id_a).unwrap();
        assert_eq!(l.repos.len(), 2, "行は増えない");
        assert_eq!(l.repos[0].id, id_a, "先頭に出る");
        assert!(!l.repos[0].hidden);
        assert_eq!(l.repos[1].id, id_b);
        // 使用中のものを復元しても、壊れず、先頭に出る
        assert_eq!(restore(&d, &id_b).unwrap().repos[0].id, id_b);
        assert!(restore(&d, "nope").unwrap_err().contains("見つかりません"));
        assert_eq!(list(&d).unwrap().repos.len(), 2, "失敗したとき、何も変わらない");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── #6: 取り込みの統合と id 衝突 ──
    #[test]
    fn import_merges_hidden_with_visible_resolves_id_clashes_and_ignores_empty_paths() {
        let d = scratch("import2");
        let base = scratch("import2-repos");
        let (a, b) = (repo(&base, "a", "sa"), repo(&base, "b", "sb"));
        let legacy = vec![
            LegacyRepo { id: Some("same".into()), path: a.clone(), name: None, hidden: Some(true) },
            LegacyRepo { id: Some("same".into()), path: b.clone(), name: None, hidden: None }, // 別のパスで、同じ id
            LegacyRepo { id: Some("dup".into()), path: format!("{a}/"), name: Some("後の名前".into()), hidden: Some(false) }, // a の重複（使用中）
            LegacyRepo { id: None, path: "".into(), name: None, hidden: None },
            LegacyRepo { id: None, path: "   ".into(), name: None, hidden: None },
        ];
        let l = import_legacy(&d, &legacy).unwrap();
        assert_eq!(l.repos.len(), 2, "空のパスは無視。a の重複は 1 行にまとまる");
        let ra = l.repos.iter().find(|r| same_dir(&r.path, &a)).unwrap();
        let rb = l.repos.iter().find(|r| same_dir(&r.path, &b)).unwrap();
        assert!(!ra.hidden, "片方が使用中なら、使用中になる（解除済みが先でも）");
        assert_ne!(ra.id, rb.id, "同じ id を持つ別のパスの 2 件は、どちらも入る（片方は新しい id）");
        assert_eq!(ra.id, "same", "先に入った行の id は、そのまま");
        // 逆の順（使用中が先、解除済みが後）では、使用中のまま
        let d2 = scratch("import3");
        let l = import_legacy(
            &d2,
            &[
                LegacyRepo { id: None, path: a.clone(), name: None, hidden: Some(false) },
                LegacyRepo { id: None, path: a.clone(), name: None, hidden: Some(true) },
            ],
        )
        .unwrap();
        assert_eq!((l.repos.len(), l.repos[0].hidden), (1, false));
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&d2);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── #7: 空のパス・git でないフォルダ・コミットの無いリポジトリ ──
    #[test]
    fn add_rejects_an_empty_path_and_handles_non_git_and_commitless_repos() {
        let d = scratch("addedge");
        let base = scratch("addedge-repos");
        assert!(add(&d, "  ", None).unwrap_err().contains("空"));
        assert!(add(&d, "", None).is_err());
        assert!(list(&d).unwrap().repos.is_empty(), "断ったとき、何も書かない");

        // git でないフォルダ: 渡されたパスのまま、最初のコミット無しで登録。2 回登録しても、行は 1 つ
        let plain = base.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let o1 = add(&d, plain.to_str().unwrap(), None).unwrap();
        assert!(!o1.limit_reached);
        assert_eq!(o1.list.repos[0].first_commit, None);
        assert_eq!(add(&d, plain.to_str().unwrap(), Some("名前")).unwrap().list.repos.len(), 1);
        assert_eq!(add(&d, &format!("{}/", plain.to_str().unwrap()), None).unwrap().list.repos.len(), 1, "末尾の / でも、同じ");

        // コミットの無い空のリポジトリ 2 つは、別のもの（最初のコミットが無いので、パスだけで見分ける）
        let e1 = base.join("empty1");
        let e2 = base.join("empty2");
        for e in [&e1, &e2] {
            std::fs::create_dir_all(e).unwrap();
            git(e, &["init", "-q", "-b", "main"]);
        }
        add(&d, e1.to_str().unwrap(), None).unwrap();
        let l = add(&d, e2.to_str().unwrap(), None).unwrap().list;
        assert_eq!(l.repos.len(), 3, "空のリポジトリ同士を、同じものと誤って数えない");
        assert!(l.repos.iter().filter(|r| r.first_commit.is_none()).count() >= 3);
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── 再登録で、付けておいた表示名が消えない ──
    #[test]
    fn re_adding_without_a_name_keeps_the_custom_name_and_a_given_name_replaces_it() {
        let d = scratch("keepname");
        let base = scratch("keepname-repos");
        let p = repo(&base, "r", "s");
        add(&d, &p, Some("カスタム名")).unwrap();
        let id = list(&d).unwrap().repos[0].id.clone();
        // 名前なしで再登録（設定の「+ リポジトリを追加…」）: 名前は残る
        assert_eq!(add(&d, &p, None).unwrap().list.repos[0].name.as_deref(), Some("カスタム名"));
        // 解除してから、名前なしで再登録: 復元され、名前は残る
        add_review(&d, &p);
        unlink(&d, &id).unwrap();
        let r = &add(&d, &p, None).unwrap().list.repos[0];
        assert!(!r.hidden);
        assert_eq!(r.name.as_deref(), Some("カスタム名"));
        // 同じリポジトリのクローンを、名前なしで選んでも、残る
        let clone = base.join("clone");
        assert!(Command::new("git").args(["clone", "-q", &p, clone.to_str().unwrap()]).status().unwrap().success());
        assert_eq!(add(&d, clone.to_str().unwrap(), None).unwrap().list.repos[0].name.as_deref(), Some("カスタム名"));
        // 空白だけの名前は「渡されていない」と同じ。名前を渡せば、更新される。消すのは rename
        assert_eq!(add(&d, &p, Some("   ")).unwrap().list.repos[0].name.as_deref(), Some("カスタム名"));
        assert_eq!(add(&d, &p, Some("新しい名前")).unwrap().list.repos[0].name.as_deref(), Some("新しい名前"));
        assert_eq!(rename(&d, &id, None).unwrap().repos[0].name, None);
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── ルートコミットが複数あっても、ブランチ違いで別のリポジトリに見えない ──
    #[test]
    fn a_repo_with_several_root_commits_is_the_same_repo_from_any_branch() {
        let d = scratch("roots");
        let base = scratch("roots-repos");
        let p = repo(&base, "orig", "s"); // main のルート A
        let orig = Path::new(&p);
        git(orig, &["checkout", "-q", "--orphan", "other"]); // 別のルート B を持つブランチ
        git(orig, &["commit", "-q", "--allow-empty", "-m", "other root"]);
        git(orig, &["checkout", "-q", "main"]);
        let wt = base.join("wt");
        git(orig, &["worktree", "add", "-q", wt.to_str().unwrap(), "other"]); // ブランチ違いのワークツリー
        let (fc_main, fc_other) = (snapshot::first_commit(&p).unwrap(), snapshot::first_commit(wt.to_str().unwrap()).unwrap());
        assert!(fc_main.is_some());
        assert_eq!(fc_main, fc_other, "出ているブランチが違っても、同じ最初のコミット");
        add(&d, &p, None).unwrap();
        let o = add(&d, wt.to_str().unwrap(), None).unwrap();
        assert_eq!(o.list.repos.len(), 1, "枠を余計に使わない");
        assert!(require_registered(&d, wt.to_str().unwrap()).is_ok());
        // 別のパスの履歴も、同じ行のものとして数える
        add_review(&d, wt.to_str().unwrap());
        assert!(!unlink(&d, &o.list.repos[0].id).unwrap().removed);
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── 最初のコミットを調べられなかったとき（git の失敗）は、「無い」と区別する ──
    #[test]
    fn first_commit_tells_no_commit_apart_from_a_failure_to_look() {
        let base = scratch("fc-kinds");
        let p = repo(&base, "r", "s");
        assert!(snapshot::first_commit(&p).unwrap().is_some());
        // 「無い」: git でないフォルダ、フォルダが無い、コミットの無いリポジトリ
        let plain = base.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(snapshot::first_commit(plain.to_str().unwrap()), Ok(None));
        assert_eq!(snapshot::first_commit(base.join("gone").to_str().unwrap()), Ok(None));
        let empty = base.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        git(&empty, &["init", "-q", "-b", "main"]);
        assert_eq!(snapshot::first_commit(empty.to_str().unwrap()), Ok(None));
        // 失敗: リポジトリなのに、履歴を辿れない（最初のコミットのオブジェクトが壊れている）
        let broken = repo(&base, "broken", "b");
        let head = String::from_utf8(Command::new("git").arg("-C").arg(&broken).args(["rev-parse", "HEAD"]).output().unwrap().stdout).unwrap();
        let head = head.trim();
        std::fs::remove_file(Path::new(&broken).join(".git/objects").join(&head[..2]).join(&head[2..])).unwrap();
        let e = snapshot::first_commit(&broken).unwrap_err();
        assert!(e.contains("最初のコミットを調べられません"), "{e}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_failure_to_look_stops_add_and_require_registered_without_changing_anything() {
        let d = scratch("fc-fail");
        let base = scratch("fc-fail-repos");
        let ok = repo(&base, "ok", "s");
        add(&d, &ok, None).unwrap();
        let broken = repo(&base, "broken", "b");
        let head = String::from_utf8(Command::new("git").arg("-C").arg(&broken).args(["rev-parse", "HEAD"]).output().unwrap().stdout).unwrap();
        let head = head.trim().to_string();
        std::fs::remove_file(Path::new(&broken).join(".git/objects").join(&head[..2]).join(&head[2..])).unwrap();
        let e = add(&d, &broken, None).unwrap_err();
        assert!(e.contains("最初のコミットを調べられません") && e.contains("何も変えていません"), "{e}");
        assert_eq!(list(&d).unwrap().repos.len(), 1, "別の行を作らず、枠も使わない");
        // 登録のパスと違う書き方で、調べられないときは、「登録されていません」ではなく、調べられなかった理由
        let e = require_registered(&d, &broken).unwrap_err();
        assert!(e.contains("確かめられませんでした") && !e.contains("登録されていません。先に"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── フォルダが無くなったクローンの履歴は、数えも消しもせず、あることを伝える ──
    #[test]
    fn history_of_a_vanished_clone_is_neither_counted_nor_deleted_but_reported() {
        let d = scratch("vanished");
        let base = scratch("vanished-repos");
        let p = repo(&base, "orig", "s");
        add(&d, &p, None).unwrap();
        let id = list(&d).unwrap().repos[0].id.clone();
        let clone = base.join("clone");
        assert!(Command::new("git").args(["clone", "-q", &p, clone.to_str().unwrap()]).status().unwrap().success());
        add_review(&d, clone.to_str().unwrap()); // 履歴は、クローンのパスにだけある
        std::fs::remove_dir_all(&clone).unwrap(); // そのクローンが、消えた

        let u = unlink(&d, &id).unwrap();
        assert!(u.removed, "決められない履歴は、このリポジトリのものと数えない");
        assert_eq!(u.unattributed_reviews, 1, "あることを、呼び出し側に伝える");
        assert_eq!(table_rows(&d, "reviews"), 1, "消していない");

        add(&d, &p, None).unwrap();
        let id = list(&d).unwrap().repos[0].id.clone();
        let (removed, l, unattributed) = delete_data(&d, &id).unwrap();
        assert_eq!((removed.reviews, unattributed, l.repos.len()), (0, 1, 0));
        assert_eq!(table_rows(&d, "reviews"), 1, "データの削除でも、決められない履歴は消さない");

        // 別の登録のパスの履歴は、決められない履歴ではない
        let other = repo(&base, "other", "so");
        add(&d, &other, None).unwrap();
        add_review(&d, &other);
        let id_o = list(&d).unwrap().repos[0].id.clone();
        assert_eq!(unlink(&d, &id_o).unwrap().unattributed_reviews, 1, "消えたクローンの 1 件だけ（other の履歴は含まない）");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── 履歴のパスが多くても、並行して調べて、同じ結果になる ──
    #[test]
    fn many_history_paths_are_attributed_correctly_in_parallel_chunks() {
        let d = scratch("many");
        let base = scratch("many-repos");
        let p = repo(&base, "orig", "s");
        let other = repo(&base, "other", "so"); // 別のリポジトリのクローンも混ぜる
        add(&d, &p, None).unwrap();
        let id = list(&d).unwrap().repos[0].id.clone();
        let mut mine = 0;
        for i in 0..(GIT_PARALLEL * 2 + 3) {
            let (src, mark) = if i % 4 == 3 { (&other, false) } else { (&p, true) };
            let c = base.join(format!("c{i}"));
            assert!(Command::new("git").args(["clone", "-q", src, c.to_str().unwrap()]).status().unwrap().success());
            add_review_with_id(&d, &format!("r{i}"), c.to_str().unwrap());
            if mark {
                mine += 1;
            }
        }
        // フォルダが消えたもの 2 件と、リポジトリではなくなったもの 1 件
        for (i, kind) in ["gone1", "gone2", "plain"].iter().enumerate() {
            let c = base.join(kind);
            if *kind == "plain" {
                std::fs::create_dir_all(&c).unwrap();
            }
            add_review_with_id(&d, &format!("x{i}"), c.to_str().unwrap());
        }
        let u = unlink(&d, &id).unwrap();
        assert!(!u.removed, "自分のクローンの履歴がある");
        assert_eq!(u.unattributed_reviews, 3, "消えた 2 件と、リポジトリでなくなった 1 件");
        let (removed, _, un) = delete_data(&d, &id).unwrap();
        assert_eq!(removed.reviews, mine, "自分のクローンの分だけ（別のリポジトリの分は含まない）");
        assert_eq!(un, 3);
        assert_eq!(table_rows(&d, "reviews") as usize, (GIT_PARALLEL * 2 + 3) - mine + 3, "別のリポジトリと決められない分は、残る");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── #2: 上限を超えた状態でも、既存の行の再追加・復元は通り、新しい行だけが断られる ──
    // ── #3: 解除済みの行は、サブディレクトリ・クローン・ワークツリー経由でも「解除されています」 ──
    #[test]
    fn a_hidden_repo_is_reported_as_unlinked_through_a_subdirectory_clone_and_worktree() {
        let d = scratch("hiddenvia");
        let base = scratch("hiddenvia-repos");
        let p = repo(&base, "orig", "s");
        add(&d, &p, None).unwrap();
        let id = list(&d).unwrap().repos[0].id.clone();
        add_review(&d, &p);
        assert!(!unlink(&d, &id).unwrap().removed, "履歴があるので、非表示で残る");

        std::fs::create_dir_all(Path::new(&p).join("sub")).unwrap();
        let clone = base.join("clone");
        assert!(Command::new("git").args(["clone", "-q", &p, clone.to_str().unwrap()]).status().unwrap().success());
        let wt = base.join("wt");
        git(Path::new(&p), &["worktree", "add", "-q", wt.to_str().unwrap(), "-b", "wt"]);
        for via in [p.clone(), format!("{p}/sub"), clone.to_str().unwrap().to_string(), wt.to_str().unwrap().to_string()] {
            let e = require_registered(&d, &via).unwrap_err();
            assert!(e.contains("解除されています") && !e.contains("登録されていません"), "{via}: {e}");
        }
        // 復元すれば、同じ経路で通る
        restore(&d, &id).unwrap();
        for via in [format!("{p}/sub"), clone.to_str().unwrap().to_string(), wt.to_str().unwrap().to_string()] {
            assert!(require_registered(&d, &via).is_ok(), "{via}");
        }
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── 同じパスの行が増えない／同じリポジトリの行が解除済みと使用中の 2 つでも、使用中を見る ──
    #[test]
    fn a_path_match_wins_over_a_first_commit_match_so_no_duplicate_path_is_made() {
        let d = scratch("pathwins");
        let base = scratch("pathwins-repos");
        let p = repo(&base, "r", "s");
        let root = root_of(&p);
        let fc = snapshot::first_commit(&p).unwrap().unwrap();
        let conn = db::open(&d).unwrap();
        insert(&conn, "gone", "/gone/old-place", None, false, Some(&fc), 0).unwrap(); // 最初のコミットは同じで、フォルダが無い行
        insert(&conn, "same", &root, None, false, None, 1).unwrap(); // パスが同じで、最初のコミットが未取得の古い行
        drop(conn);
        let o = add_with(&d, &p, None, true).unwrap();
        assert_eq!(o.list.repos.len(), 2, "行は増えない");
        assert!(o.move_from.is_none(), "パスが同じ行が先に選ばれるので、付け替えの確認にならない");
        let same = o.list.repos.iter().find(|r| r.id == "same").unwrap();
        assert_eq!((same.path.as_str(), same.first_commit.as_deref()), (root.as_str(), Some(fc.as_str())), "同じパスの行が更新され、最初のコミットが埋まる");
        assert_eq!(o.list.repos.iter().find(|r| r.id == "gone").unwrap().path, "/gone/old-place", "もう一方は、そのまま");
        let paths: Vec<&str> = o.list.repos.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths.len(), paths.iter().collect::<std::collections::HashSet<_>>().len(), "同じパスの行は、できない");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn require_registered_passes_when_any_row_of_the_same_repo_is_in_use() {
        let d = scratch("tworows");
        let base = scratch("tworows-repos");
        let p = repo(&base, "orig", "s");
        let clone = base.join("clone");
        assert!(Command::new("git").args(["clone", "-q", &p, clone.to_str().unwrap()]).status().unwrap().success());
        let (root, croot) = (root_of(&p), root_of(clone.to_str().unwrap()));
        let fc = snapshot::first_commit(&p).unwrap().unwrap();
        let conn = db::open(&d).unwrap();
        // 同じリポジトリの行が 2 つ: 元のパスは解除済み（最初のコミット未取得）、クローンのパスは使用中
        insert(&conn, "hid", &root, None, true, None, 0).unwrap();
        insert(&conn, "use", &croot, None, false, Some(&fc), 1).unwrap();
        drop(conn);
        assert!(require_registered(&d, &p).is_ok(), "解除済みの行が先に当たっても、使用中の行を見る");
        assert!(require_registered(&d, &croot).is_ok());
        // 逆: 元のパスが使用中で、クローンが解除済み
        let d2 = scratch("tworows2");
        let conn = db::open(&d2).unwrap();
        insert(&conn, "use", &root, None, false, Some(&fc), 0).unwrap();
        insert(&conn, "hid", &croot, None, true, Some(&fc), 1).unwrap();
        drop(conn);
        assert!(require_registered(&d2, &croot).is_ok(), "解除済みのクローンのパスでも、同じリポジトリの使用中の行があれば通る");
        // どちらも解除済みなら、「解除されています」
        let d3 = scratch("tworows3");
        let conn = db::open(&d3).unwrap();
        insert(&conn, "h1", &root, None, true, Some(&fc), 0).unwrap();
        insert(&conn, "h2", &croot, None, true, Some(&fc), 1).unwrap();
        drop(conn);
        assert!(require_registered(&d3, &p).unwrap_err().contains("解除されています"));
        assert!(require_registered(&d3, &croot).unwrap_err().contains("解除されています"));
        for x in [d, d2, d3] {
            let _ = std::fs::remove_dir_all(&x);
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── 権限などの理由の分からない失敗は、「最初のコミットが無い」にしない ──
    #[test]
    fn an_unreadable_repository_is_a_failure_not_no_commit() {
        use std::os::unix::fs::PermissionsExt;
        if std::process::Command::new("id").arg("-u").output().map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0").unwrap_or(false) {
            return; // root は、権限に関係なく読めてしまう
        }
        let base = scratch("noperm");
        let p = repo(&base, "r", "s");
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();
        let r = snapshot::first_commit(&p);
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        let e = r.unwrap_err();
        assert!(e.contains("最初のコミットを調べられません"), "{e}");
        // 元に戻せば、読める。git でないフォルダと、無いフォルダは、いままでどおり「無い」
        assert!(snapshot::first_commit(&p).unwrap().is_some());
        let plain = base.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(snapshot::first_commit(plain.to_str().unwrap()), Ok(None));
        assert_eq!(snapshot::first_commit(base.join("gone").to_str().unwrap()), Ok(None));
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── 取り込みで最初のコミットを調べられなかった行は、あとで埋める。失敗は伝える ──
    #[test]
    fn import_reports_a_failed_lookup_keeps_the_row_and_fills_the_first_commit_later() {
        let d = scratch("impwarn");
        let base = scratch("impwarn-repos");
        let ok = repo(&base, "ok", "s");
        let broken = repo(&base, "broken", "b");
        let head = String::from_utf8(Command::new("git").arg("-C").arg(&broken).args(["rev-parse", "HEAD"]).output().unwrap().stdout).unwrap();
        let head = head.trim().to_string();
        let obj = Path::new(&broken).join(".git/objects").join(&head[..2]).join(&head[2..]);
        let saved = std::fs::read(&obj).unwrap();
        std::fs::remove_file(&obj).unwrap(); // 最初のコミットを辿れない（git の失敗）
        let legacy = [
            LegacyRepo { id: None, path: ok.clone(), name: None, hidden: None },
            LegacyRepo { id: None, path: broken.clone(), name: None, hidden: None },
        ];
        let l = import_legacy(&d, &legacy).unwrap();
        assert_eq!(l.repos.len(), 2, "1 件の不調で、ほかの登録を取り込めなくしない");
        assert_eq!(l.warnings.len(), 1, "{:?}", l.warnings);
        assert!(l.warnings[0].contains("最初のコミットを調べられません"), "{:?}", l.warnings);
        let rb = l.repos.iter().find(|r| same_dir(&r.path, &broken)).unwrap();
        assert_eq!(rb.first_commit, None, "調べられなかった行は、最初のコミット無し");
        assert!(l.repos.iter().find(|r| same_dir(&r.path, &ok)).unwrap().first_commit.is_some());
        // 直ったあと（次に開いたとき）、埋まる。警告も出ない
        std::fs::create_dir_all(obj.parent().unwrap()).unwrap();
        std::fs::write(&obj, saved).unwrap();
        let again = import_legacy(&d, &[]).unwrap();
        assert!(again.warnings.is_empty());
        assert!(again.repos.iter().find(|r| same_dir(&r.path, &broken)).unwrap().first_commit.is_some(), "あとで埋まる");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── 最初のコミットが無い行でも、別パスの履歴を、黙って見落とさない ──
    #[test]
    fn a_row_without_a_first_commit_still_accounts_for_other_path_history() {
        let d = scratch("nofc");
        let base = scratch("nofc-repos");
        let p = repo(&base, "orig", "s");
        let clone = base.join("clone");
        assert!(Command::new("git").args(["clone", "-q", &p, clone.to_str().unwrap()]).status().unwrap().success());
        add_review(&d, clone.to_str().unwrap()); // 履歴は、クローンのパスにだけある

        // フォルダのある行（最初のコミット無し）: いま調べて、クローンの履歴を、この行のものと数える
        let conn = db::open(&d).unwrap();
        insert(&conn, "withdir", &root_of(&p), None, false, None, 0).unwrap();
        drop(conn);
        let u = unlink(&d, "withdir").unwrap();
        assert!(!u.removed, "クローンの履歴があるので、行ごとは消えない");

        // フォルダの無い行（最初のコミット無し）: 見分けようがない。行ごと消えても、フォルダのある履歴を、決められない履歴として伝える
        let d2 = scratch("nofc2");
        add_review(&d2, clone.to_str().unwrap());
        let conn = db::open(&d2).unwrap();
        insert(&conn, "nodir", "/gone/nowhere", None, false, None, 0).unwrap();
        drop(conn);
        let u = unlink(&d2, "nodir").unwrap();
        assert!(u.removed);
        assert_eq!(u.unattributed_reviews, 1, "黙って見落とさず、あることを伝える");
        assert_eq!(table_rows(&d2, "reviews"), 1, "消していない");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&d2);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── 通した記憶は、登録が変わったら捨てる（解除したのに、通し続けない） ──
    #[test]
    fn a_remembered_pass_is_dropped_as_soon_as_the_registration_changes() {
        let d = scratch("remember");
        let base = scratch("remember-repos");
        let p = repo(&base, "orig", "s");
        std::fs::create_dir_all(Path::new(&p).join("sub")).unwrap();
        let clone = base.join("clone");
        assert!(Command::new("git").args(["clone", "-q", &p, clone.to_str().unwrap()]).status().unwrap().success());
        add(&d, &p, None).unwrap();
        let id = list(&d).unwrap().repos[0].id.clone();
        let via = [format!("{p}/sub"), clone.to_str().unwrap().to_string()];
        for v in &via {
            assert!(require_registered(&d, v).is_ok());
            assert!(require_registered(&d, v).is_ok(), "2 回目は、記憶から（結果は同じ）");
        }
        add_review(&d, &p);
        unlink(&d, &id).unwrap(); // 解除
        for v in &via {
            assert!(require_registered(&d, v).unwrap_err().contains("解除されています"), "記憶が残って、解除したのに通っている: {v}");
        }
        restore(&d, &id).unwrap();
        for v in &via {
            assert!(require_registered(&d, v).is_ok(), "{v}");
        }
        delete_data(&d, &id).unwrap(); // 削除
        for v in &via {
            assert!(require_registered(&d, v).unwrap_err().contains("登録されていません"), "{v}");
        }
        // 別のデータ領域には、影響しない
        let d2 = scratch("remember2");
        assert!(require_registered(&d2, &via[0]).unwrap_err().contains("登録されていません"));
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&d2);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── 解除・削除の git は、書き込みのロックを取る前に済ませる ──
    #[test]
    fn history_git_checks_are_done_before_the_write_lock_is_taken() {
        let d = scratch("warm");
        let base = scratch("warm-repos");
        let p = repo(&base, "orig", "s");
        add(&d, &p, None).unwrap();
        let id = list(&d).unwrap().repos[0].id.clone();
        let mut clones = Vec::new();
        for i in 0..3 {
            let c = base.join(format!("c{i}"));
            assert!(Command::new("git").args(["clone", "-q", &p, c.to_str().unwrap()]).status().unwrap().success());
            add_review_with_id(&d, &format!("r{i}"), c.to_str().unwrap());
            clones.push(repodata::repo_key(c.to_str().unwrap()));
        }
        let conn = db::open(&d).unwrap();
        let known = warm_history(&conn, &id).unwrap();
        for c in &clones {
            assert!(known.get(c).is_some_and(|f| f.is_some()), "{c} の最初のコミットを、ロックの前に調べてある");
        }
        // ロックの中の判定は、調べ済みの結果を使うだけ（結果は、ロックの外でやったときと同じ）
        let mut known2 = known.clone();
        let all = rows(&conn).unwrap();
        let h = history_of(&conn, &all, &all[0], &mut known2).unwrap();
        assert_eq!(h.keys.len(), 1 + clones.len());
        assert_eq!(known2.len(), known.len(), "新しく git で調べたものは、無い");
        // 実際の操作も、同じ結果
        let (removed, _, _) = delete_data(&d, &id).unwrap();
        assert_eq!(removed.reviews, 3);
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&base);
    }

}
