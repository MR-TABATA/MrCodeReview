//! データの置き場（履歴・仕分け・メモ・設定・登録リポジトリ）。GUI と CLI が、同じ規則で決める（ここが唯一の実装）。
//!
//! 置き場は、このアプリ（MrCodeReview）の名前（`DATA_ID`。GUI の `identifier` と同じ）。
//! 別のアプリ（将来の有償版など）は、自分の置き場を持つ。OSS 版のデータは、有償版が「インポートするか」を聞いてコピーする
//! （ここでは扱わない）。
//!
//! 以前の名前（`LEGACY_ID`。MrkCodeReview 1.x と mrcr 0.5.0 までが使っていた）の置き場があれば、**1 回、丸ごと移す**。
//! 移したあと、旧の名前から新へのシンボリックリンクを**しばらく残す**（旧版の mrcr / GUI が、同じデータを見られるように。
//! 残さないと、旧版が旧の名前に空のデータを作り、データが 2 か所に割れる）。
//! **このリンクは、旧版が使われなくなったら消す**（`LEGACY_LINK_REMOVAL`）。消さないと、将来、その名前を別のアプリが使えない。
//!
//! 移しは、ロック（`applock`）を取ってから行う。ロックを取れない（GUI が動いている、など）プロセスは、`Mode::ReadOnly` で、
//! 動かさずに、置き場を決めるだけにする。

use std::io;
use std::path::{Path, PathBuf};

/// このアプリの置き場の名前。
pub const DATA_ID: &str = "com.hitoshi.mrcodereview";
/// 以前の置き場の名前。
pub const LEGACY_ID: &str = "com.hitoshi.mrkcodereview";
/// 旧の名前に残すリンクを消す目安（旧版が使われなくなったとき）。コードでは消さない。
pub const LEGACY_LINK_REMOVAL: &str = "mrcr 0.5.0 と MrkCodeReview 1.x の利用が無くなったとき（リリースノートで、リンクを消すと告げてから）";

/// 置き場を決めるとき、動かしてよいか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// 旧の置き場があれば、移す（ロックを取ってから呼ぶこと）。
    Migrate,
    /// 何も動かさない・作らない。新に中身があれば新、無くて旧があれば旧、どちらも無ければ新。
    ReadOnly,
}

/// 置き場の基準のフォルダ（この下に `DATA_ID` を置く）。Tauri の `app_data_dir` の親と同じ。
pub fn default_base() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
            return Some(PathBuf::from(x));
        }
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Migration {
    /// 旧の置き場が無い（新規の利用者、または移し済みで、リンクも無い）。
    NothingToMove,
    /// 旧の置き場を、新の置き場に移した。
    Moved,
    /// 旧が、新へのリンク（前に移したときに、残したもの）。移し済み。
    AlreadyMigrated,
    /// 新の置き場に、すでに中身がある。**混ぜず**、どちらも触らない（新を使う）。
    BothHaveData,
    /// 旧の置き場が、ほかのフォルダへのシンボリックリンク（外付けドライブなどに置いている利用者）。
    /// **移さない**（リンク先を、勝手に動かさない）。**旧を、そのまま使い続ける**（新へ切り替えると、データが見えなくなる）。
    LegacyIsLink,
}

/// 同時起動のロックのファイル（`applock`）。置き場の**隣**に置く（置き場の中に置くと、「中身があるか」の判定に混ざり、
/// 移しの前にロックを取れない）。`MRCR_DATA_DIR` のとき（`explicit`）は、その隣＝本物の置き場とは別のロックになる。
pub fn lock_file(base: &Path, explicit: Option<&Path>) -> PathBuf {
    let sibling = |dir: &Path, name: &str| match dir.parent() {
        Some(parent) => parent.join(format!("{name}.lock")),
        None => PathBuf::from(format!("{name}.lock")),
    };
    match explicit {
        Some(dir) => {
            let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "data".into());
            sibling(dir, &name)
        }
        None => base.join(format!("{DATA_ID}.lock")),
    }
}

/// 使う置き場と、利用者に見せる通知。
pub struct Resolved {
    pub dir: PathBuf,
    /// 履歴が画面に出なくなる・CLI と別のデータを見る、といった、**利用者が気づけないと困る**ときだけ入る。
    /// 条件が続くかぎり、起動のたびに入る。
    pub notice: Option<String>,
}

/// `base` の下で使う置き場を返す。`Mode::Migrate` なら、旧の置き場があれば、先に移す。
pub fn resolve_in(base: &Path, mode: Mode) -> Resolved {
    let new = base.join(DATA_ID);
    let old = base.join(LEGACY_ID);
    if mode == Mode::ReadOnly {
        return Resolved { dir: read_only_pick(&old, &new), notice: None };
    }
    match migrate(&old, &new) {
        Ok(Migration::BothHaveData) => {
            let notice = format!(
                "データの置き場が 2 か所にあり、混ぜずに、新しい方（{new}）だけを使っています。以前の置き場（{old}）にある履歴・仕分けは、画面に出ません（消してはいません）。",
                new = new.display(),
                old = old.display()
            );
            eprintln!("{notice}");
            Resolved { dir: new, notice: Some(notice) }
        }
        Ok(Migration::LegacyIsLink) => {
            // 新にも中身があるときだけ知らせる（新は、旧のリンクの中身と別のデータ。画面には出ない）。
            let notice = has_entries(&new).then(|| {
                format!(
                    "以前の置き場（{old}）がリンクなので、移さず、そちらを使っています。新しい置き場（{new}）にも中身がありますが、画面には出ません（消してはいません）。",
                    new = new.display(),
                    old = old.display()
                )
            });
            if let Some(n) = &notice {
                eprintln!("{n}");
            }
            Resolved { dir: old, notice }
        }
        Ok(_) => Resolved { dir: new, notice: None },
        // 移せなかったとき。旧の置き場は、そのまま残っている（移しは 1 回の rename だけ）ので、そちらを使い続ける。
        Err(e) => {
            let notice = format!(
                "データの置き場を新しい場所（{new}）へ移せませんでした（{e}）。以前の置き場（{old}）を使い続けます。同じ Mac の `mrcr`（コマンド）とは、別のデータを見ている可能性があります。起動のたびに、移しをやり直します。",
                new = new.display(),
                old = old.display()
            );
            eprintln!("{notice}");
            Resolved { dir: old, notice: Some(notice) }
        }
    }
}

/// 動かさずに決める。新に中身があれば新。無くて、旧がある（フォルダ、またはフォルダへのリンク）なら旧。どちらも無ければ新。
fn read_only_pick(old: &Path, new: &Path) -> PathBuf {
    if has_entries(new) {
        return new.to_path_buf();
    }
    match std::fs::metadata(old) {
        Ok(m) if m.is_dir() => old.to_path_buf(),
        _ => new.to_path_buf(),
    }
}

/// フォルダに、何か入っているか（読めない・無いときは、入っていないとみなす。通知を出すかの判断だけに使う）。
fn has_entries(dir: &Path) -> bool {
    std::fs::read_dir(dir).map(|mut it| it.next().is_some()).unwrap_or(false)
}

/// `old` が、`new` を指すリンクか（移したときに残したもの）。
fn links_to(old: &Path, new: &Path) -> bool {
    match (std::fs::canonicalize(old), std::fs::canonicalize(new)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// 旧 → 新。**コピーでなく rename 1 回**（途中で止まっても、半端な新が残らない）。移したあと、旧の名前に新へのリンクを残す。
fn migrate(old: &Path, new: &Path) -> io::Result<Migration> {
    let old_meta = match std::fs::symlink_metadata(old) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Migration::NothingToMove),
        Err(e) => return Err(e),
    };
    if old_meta.file_type().is_symlink() {
        if links_to(old, new) {
            return Ok(Migration::AlreadyMigrated);
        }
        // リンク先がフォルダなら、そこにデータがある。切れたリンクやファイルへのリンクは、データが無いのと同じ。
        return Ok(if std::fs::metadata(old).map(|m| m.is_dir()).unwrap_or(false) {
            Migration::LegacyIsLink
        } else {
            Migration::NothingToMove
        });
    }
    if !old_meta.is_dir() {
        return Ok(Migration::NothingToMove);
    }
    match std::fs::read_dir(new) {
        Ok(mut it) => {
            if it.next().is_some() {
                return Ok(Migration::BothHaveData);
            }
            // 空の新（以前の起動が作っただけ）。rename できるよう、空のまま消す。
            std::fs::remove_dir(new)?;
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    std::fs::rename(old, new)?;
    leave_legacy_link(old, new);
    Ok(Migration::Moved)
}

/// 旧の名前に、新へのリンク（同じフォルダの中の相対）を残す。残せなくても、移しは成功のまま（旧版が、空のデータを作るだけ）。
fn leave_legacy_link(old: &Path, new: &Path) {
    #[cfg(unix)]
    {
        let target = new.file_name().map(PathBuf::from).unwrap_or_else(|| new.to_path_buf());
        if let Err(e) = std::os::unix::fs::symlink(&target, old) {
            eprintln!("旧の名前（{}）に、新しい置き場へのリンクを残せませんでした（{e}）。旧版を使うと、データが割れます。", old.display());
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (old, new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 試験ごとに、空の「アプリのデータ領域」を作る（終わりに消す）。
    struct Base(PathBuf);
    impl Base {
        fn new(tag: &str) -> Base {
            let d = std::env::temp_dir().join(format!("mrcr-datadir-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&d);
            fs::create_dir_all(&d).unwrap();
            Base(d)
        }
        fn old(&self) -> PathBuf {
            self.0.join(LEGACY_ID)
        }
        fn new_dir(&self) -> PathBuf {
            self.0.join(DATA_ID)
        }
        fn migrate(&self) -> Resolved {
            resolve_in(&self.0, Mode::Migrate)
        }
        fn read_only(&self) -> Resolved {
            resolve_in(&self.0, Mode::ReadOnly)
        }
    }
    impl Drop for Base {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn put(dir: &Path, name: &str, body: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn the_lock_file_sits_next_to_the_data_dir() {
        let b = Base::new("lockpath");
        let l = lock_file(&b.0, None);
        assert_eq!(l, b.0.join(format!("{DATA_ID}.lock")));
        assert!(!l.starts_with(b.new_dir()), "置き場の中には置かない");
    }

    #[test]
    fn an_explicit_data_dir_gets_its_own_lock() {
        let b = Base::new("lockexplicit");
        let custom = b.0.join("dev").join("data");
        let l = lock_file(&b.0, Some(&custom));
        assert_eq!(l, b.0.join("dev").join("data.lock"));
        assert_ne!(l, lock_file(&b.0, None));
    }

    #[test]
    fn a_fresh_install_uses_the_new_dir_and_creates_nothing_else() {
        let b = Base::new("fresh");
        let got = b.migrate().dir;
        assert_eq!(got, b.new_dir());
        assert!(fs::symlink_metadata(b.old()).is_err(), "旧の名前を作らない");
    }

    #[cfg(unix)]
    #[test]
    fn the_legacy_dir_is_moved_whole_and_a_link_is_left_under_the_old_name() {
        let b = Base::new("move");
        put(&b.old(), "store.sqlite", "history");
        put(&b.old().join("claude-cwd"), "x", "nested");
        let r = b.migrate();
        assert_eq!(r.dir, b.new_dir());
        assert!(r.notice.is_none());
        assert_eq!(fs::read_to_string(b.new_dir().join("store.sqlite")).unwrap(), "history");
        assert_eq!(fs::read_to_string(b.new_dir().join("claude-cwd/x")).unwrap(), "nested");
        let meta = fs::symlink_metadata(b.old()).unwrap();
        assert!(meta.file_type().is_symlink(), "旧の名前は、新へのリンクとして残す");
        assert_eq!(fs::read_link(b.old()).unwrap(), PathBuf::from(DATA_ID), "同じフォルダの中の相対リンク");
        assert_eq!(fs::read_to_string(b.old().join("store.sqlite")).unwrap(), "history", "旧版が、リンク越しに同じデータを読める");
    }

    #[cfg(unix)]
    #[test]
    fn running_twice_is_harmless_and_keeps_the_link() {
        let b = Base::new("twice");
        put(&b.old(), "store.sqlite", "history");
        b.migrate();
        let r = b.migrate();
        assert!(r.notice.is_none(), "移し済みでは、通知しない");
        assert_eq!(r.dir, b.new_dir());
        assert_eq!(fs::read_to_string(r.dir.join("store.sqlite")).unwrap(), "history");
        assert_eq!(fs::read_link(b.old()).unwrap(), PathBuf::from(DATA_ID));
    }

    #[cfg(unix)]
    #[test]
    fn an_old_version_writing_through_the_link_does_not_split_the_data() {
        let b = Base::new("oldwriter");
        put(&b.old(), "store.sqlite", "history");
        b.migrate();
        // 旧版は、旧の名前のフォルダに書く（リンク越しに、新に届く）
        fs::write(b.old().join("store.sqlite"), "history+more").unwrap();
        assert_eq!(fs::read_to_string(b.new_dir().join("store.sqlite")).unwrap(), "history+more");
        let r = b.migrate();
        assert!(r.notice.is_none() && r.dir == b.new_dir());
    }

    #[cfg(unix)]
    #[test]
    fn an_empty_new_dir_does_not_block_the_move() {
        let b = Base::new("emptynew");
        put(&b.old(), "store.sqlite", "history");
        fs::create_dir_all(b.new_dir()).unwrap();
        let got = b.migrate().dir;
        assert_eq!(fs::read_to_string(got.join("store.sqlite")).unwrap(), "history");
        assert!(fs::symlink_metadata(b.old()).unwrap().file_type().is_symlink());
    }

    #[test]
    fn when_both_have_data_nothing_is_merged_or_deleted() {
        let b = Base::new("both");
        put(&b.old(), "store.sqlite", "old-history");
        put(&b.new_dir(), "store.sqlite", "new-history");
        let r = b.migrate();
        let notice = r.notice.expect("両方にあるときは、利用者に知らせる");
        assert!(notice.contains(&b.old().display().to_string()), "旧の場所を書く（取り戻せるように）");
        assert!(notice.contains(&b.new_dir().display().to_string()));
        assert!(notice.contains("消してはいません"));
        assert_eq!(r.dir, b.new_dir());
        assert_eq!(fs::read_to_string(b.old().join("store.sqlite")).unwrap(), "old-history");
        assert_eq!(fs::read_to_string(b.new_dir().join("store.sqlite")).unwrap(), "new-history");
        assert!(!fs::symlink_metadata(b.old()).unwrap().file_type().is_symlink(), "混ぜないときは、リンクに替えない");
    }

    #[test]
    fn a_failed_move_falls_back_to_the_legacy_dir_and_loses_nothing() {
        let b = Base::new("fail");
        put(&b.old(), "store.sqlite", "history");
        // 新の位置に「ファイル」があると、ディレクトリとして読めず、移せない。
        fs::write(b.new_dir(), "not a dir").unwrap();
        let r = b.migrate();
        let notice = r.notice.expect("移せなかったときは、利用者に知らせる");
        assert!(notice.contains("移せませんでした") && notice.contains("別のデータを見ている可能性"));
        assert!(notice.contains(&b.old().display().to_string()));
        assert_eq!(r.dir, b.old());
        assert_eq!(fs::read_to_string(b.old().join("store.sqlite")).unwrap(), "history");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_legacy_dir_to_elsewhere_keeps_being_used_and_its_target_is_not_moved() {
        let b = Base::new("symlink");
        let target = b.0.join("elsewhere");
        put(&target, "store.sqlite", "history");
        std::os::unix::fs::symlink(&target, b.old()).unwrap();
        let r = b.migrate();
        assert_eq!(r.dir, b.old(), "旧のリンクを、そのまま使う（新へ切り替えると、履歴が見えなくなる）");
        assert!(r.notice.is_none());
        assert_eq!(fs::read_to_string(r.dir.join("store.sqlite")).unwrap(), "history", "リンク越しに、データが読める");
        assert!(target.join("store.sqlite").exists(), "リンク先は動かさない");
        assert!(!b.new_dir().exists(), "新を作らない");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_legacy_dir_with_data_in_the_new_dir_too_is_told_to_the_user() {
        let b = Base::new("symlink-both");
        let target = b.0.join("elsewhere");
        put(&target, "store.sqlite", "old-history");
        std::os::unix::fs::symlink(&target, b.old()).unwrap();
        put(&b.new_dir(), "store.sqlite", "new-history");
        let r = b.migrate();
        assert_eq!(r.dir, b.old());
        let notice = r.notice.expect("新にも中身があるときは、知らせる");
        assert!(notice.contains(&b.new_dir().display().to_string()) && notice.contains("消してはいません"));
        assert_eq!(fs::read_to_string(b.new_dir().join("store.sqlite")).unwrap(), "new-history");
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_is_treated_as_no_legacy_data() {
        let b = Base::new("dangling");
        std::os::unix::fs::symlink(b.0.join("gone"), b.old()).unwrap();
        let r = b.migrate();
        assert_eq!(r.dir, b.new_dir());
        assert!(r.notice.is_none());
    }

    #[test]
    fn read_only_never_moves_or_creates_anything() {
        // 新規: 何も作らない
        let b = Base::new("ro-fresh");
        assert_eq!(b.read_only().dir, b.new_dir());
        assert!(!b.new_dir().exists() && fs::symlink_metadata(b.old()).is_err());
        // 旧だけある: 旧を使う（動かさない）
        let b = Base::new("ro-old");
        put(&b.old(), "store.sqlite", "history");
        assert_eq!(b.read_only().dir, b.old());
        assert!(!b.new_dir().exists());
        assert_eq!(fs::read_to_string(b.old().join("store.sqlite")).unwrap(), "history");
        // 新に中身がある: 新を使う（旧も、動かさない）
        let b = Base::new("ro-both");
        put(&b.old(), "store.sqlite", "o");
        put(&b.new_dir(), "store.sqlite", "n");
        assert_eq!(b.read_only().dir, b.new_dir());
        assert_eq!(fs::read_to_string(b.old().join("store.sqlite")).unwrap(), "o");
    }
}
