//! 同時起動の防止。
//!
//! 同じデータ（SQLite）を、2 つのプロセスが同時に書かないよう、ロックのファイルを排他的に取る。
//! GUI は、起動したときに取り、**取れなければ、案内を出して終了する**。CLI は、置き場の移し（`datadir`）のあいだだけ取る。
//!
//! ロックは、プロセスが終わると、OS が外す（異常終了でも、残らない）。ファイルが残っていても、ロックではない。
//!
//! ロックのファイルは、データの置き場の**中ではなく、隣**に置く（`datadir::lock_file`）。
//! 移し（`datadir::resolve_in`）の前に取るため（2 つが同時に起動したとき、移しが競合しない）、
//! 置き場の「中身があるか」の判定に、ロックのファイルが混ざらないため。

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

/// 取ったロック。**持っているあいだ、ロックが続く**（プロセスの終わりまで持つなら、変数に持ち続ける）。
#[derive(Debug)]
pub struct AppLock {
    _file: File,
}

#[derive(Debug)]
pub enum Acquired {
    Locked(AppLock),
    /// もう一方（または別のプロセス）が、ロックを持っている。
    AlreadyRunning,
    /// ロックを試せなかった（ファイルを作れない・この場所のファイルシステムがロックに対応していない、など）。
    /// 同時起動を防げないが、**起動はさせる**（読み取り専用の場所などで、アプリが開かなくなるよりよい）。
    Unsupported(String),
}

pub fn acquire(lock_file: &Path) -> Acquired {
    if let Some(parent) = lock_file.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Acquired::Unsupported(format!("{}: {e}", parent.display()));
        }
    }
    // 中身は使わない。切り詰めない（他のプロセスが開いているものを、壊さない）。
    let file = match OpenOptions::new().create(true).truncate(false).write(true).open(lock_file) {
        Ok(f) => f,
        Err(e) => return Acquired::Unsupported(format!("{}: {e}", lock_file.display())),
    };
    match file.try_lock() {
        Ok(()) => Acquired::Locked(AppLock { _file: file }),
        Err(TryLockError::WouldBlock) => Acquired::AlreadyRunning,
        Err(TryLockError::Error(e)) => Acquired::Unsupported(format!("{}: {e}", lock_file.display())),
    }
}

/// `acquire` を、取れなかったとき（`AlreadyRunning`）だけ、`wait` あけて、あと `retries` 回、取り直す。
/// CLI は、置き場を決めるあいだ、一瞬だけロックを持つ。その一瞬に GUI を起動しても、「すでに起動しています」と断らないため。
/// 本当に別の GUI が動いているときは、`retries * wait` 待ってから、`AlreadyRunning` を返す。
pub fn acquire_retry(lock_file: &Path, retries: u32, wait: std::time::Duration) -> Acquired {
    acquire_retry_with(lock_file, retries, || std::thread::sleep(wait))
}

/// `acquire_retry` の本体。取り直しの間に呼ぶ処理（`pause`）を、引数で受ける。試験では、実際には待たず、この中でロックを手放して、
/// 「手放された後は取れる」「持たれ続けたら、決めた回数だけ待って、断る」を、時間にも、別のスレッドの順序にも頼らずに確かめる。
pub fn acquire_retry_with(lock_file: &Path, retries: u32, mut pause: impl FnMut()) -> Acquired {
    let mut got = acquire(lock_file);
    for _ in 0..retries {
        if !matches!(got, Acquired::AlreadyRunning) {
            break;
        }
        pause();
        got = acquire(lock_file);
    }
    got
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Dir(PathBuf);
    impl Dir {
        fn new(tag: &str) -> Dir {
            let d = std::env::temp_dir().join(format!("mrcr-applock-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            Dir(d)
        }
        fn lock(&self) -> PathBuf {
            self.0.join("app.lock")
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_first_gets_the_lock_and_the_second_is_told_it_is_already_running() {
        let d = Dir::new("two");
        let first = acquire(&d.lock());
        assert!(matches!(first, Acquired::Locked(_)), "{first:?}");
        assert!(matches!(acquire(&d.lock()), Acquired::AlreadyRunning));
    }

    #[test]
    fn the_lock_is_released_when_the_holder_goes_away() {
        let d = Dir::new("release");
        let first = acquire(&d.lock());
        assert!(matches!(first, Acquired::Locked(_)));
        drop(first);
        // 並列に動く他のテストが、子プロセスを起こす一瞬（作ってから別のプログラムに入れ替わるまで）は、その子が、このファイルの
        // 記述子の写しを持ち、ロックがまだ外れて見えない。その一瞬は待つ。1 秒たっても取れないなら、本当に外れていない。
        let mut got = false;
        for _ in 0..50 {
            if matches!(acquire(&d.lock()), Acquired::Locked(_)) {
                got = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(got, "持ち主が終われば、取り直せる（1 秒待っても取れない）");
    }

    #[test]
    fn a_leftover_lock_file_is_not_a_lock() {
        let d = Dir::new("leftover");
        std::fs::write(d.lock(), "pid 1234").unwrap();
        assert!(matches!(acquire(&d.lock()), Acquired::Locked(_)), "ファイルが残っているだけでは、断らない");
    }

    #[test]
    fn different_lock_files_do_not_block_each_other() {
        // 試験用・自分用ビルド（`MRCR_DATA_DIR`）は、本物の置き場と別のロックを使う。
        let d = Dir::new("separate");
        let a = acquire(&d.0.join("a.lock"));
        let b = acquire(&d.0.join("b.lock"));
        assert!(matches!(a, Acquired::Locked(_)) && matches!(b, Acquired::Locked(_)));
    }

    #[test]
    fn the_parent_folder_is_created_when_it_is_missing() {
        let d = Dir::new("mkdir");
        let f = d.0.join("not").join("yet").join("app.lock");
        assert!(matches!(acquire(&f), Acquired::Locked(_)));
    }

    #[test]
    fn a_lock_that_cannot_be_tried_does_not_stop_the_app_and_says_why() {
        let d = Dir::new("unsupported");
        // ロックのファイルの場所が「フォルダ」だと、ファイルとして開けない。
        std::fs::create_dir_all(d.lock()).unwrap();
        match acquire(&d.lock()) {
            Acquired::Unsupported(reason) => assert!(reason.contains("app.lock"), "{reason}"),
            other => panic!("{other:?}"),
        }
    }

    /// 別のプロセスからも効くこと。python3 の `flock`（同じ OS のロック）で確かめる。
    #[cfg(unix)]
    #[test]
    fn another_process_cannot_take_the_lock_while_it_is_held_and_can_after_release() {
        let probe = "import fcntl,sys\nf=open(sys.argv[1],'a')\ntry:\n fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)\nexcept OSError:\n sys.exit(3)\n";
        let run = |p: &Path| std::process::Command::new("python3").arg("-I").arg("-c").arg(probe).arg(p).status();
        let d = Dir::new("process");
        let held = acquire(&d.lock());
        assert!(matches!(held, Acquired::Locked(_)));
        let st = match run(&d.lock()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("python3 を起動できないため、別プロセスの確認を飛ばす: {e}");
                return;
            }
        };
        assert_eq!(st.code(), Some(3), "持っているあいだは、別のプロセスは取れない");
        drop(held);
        assert_eq!(run(&d.lock()).unwrap().code(), Some(0), "外れたら、別のプロセスが取れる");
    }

    #[test]
    fn a_lock_released_between_two_tries_is_taken_by_the_retry() {
        let d = Dir::new("retry-short");
        let mut held = Some(acquire(&d.lock())); // 別のプロセス（CLI）が、持っている
        assert!(matches!(held.as_ref().unwrap(), Acquired::Locked(_)));
        let mut pauses = 0;
        // 1 回目は取れず、最初の「待ち」の中で手放される。2 回目で取れる（実際には待たない。順序は、決まっている）
        let got = acquire_retry_with(&d.lock(), 5, || {
            pauses += 1;
            held.take();
        });
        assert!(matches!(got, Acquired::Locked(_)), "手放されたら、取り直しで取れる: {got:?}");
        assert_eq!(pauses, 1, "取れたら、それ以上は、取り直さない");
    }

    #[test]
    fn a_lock_that_stays_held_is_refused_after_exactly_the_given_number_of_retries() {
        let d = Dir::new("retry-long");
        let held = acquire(&d.lock());
        assert!(matches!(held, Acquired::Locked(_)));
        let mut pauses = 0;
        let got = acquire_retry_with(&d.lock(), 3, || pauses += 1);
        assert!(matches!(got, Acquired::AlreadyRunning), "持ち続けられたら、断る: {got:?}");
        assert_eq!(pauses, 3, "決めた回数だけ、待った");
        let zero = acquire_retry_with(&d.lock(), 0, || panic!("取り直しが 0 回なら、待たない"));
        assert!(matches!(zero, Acquired::AlreadyRunning));
    }

    #[test]
    fn retrying_does_not_pause_when_the_first_try_succeeds_or_cannot_be_tried() {
        let d = Dir::new("retry-none");
        let got = acquire_retry_with(&d.lock(), 50, || panic!("最初に取れれば、待たない"));
        assert!(matches!(got, Acquired::Locked(_)));
        let d2 = Dir::new("retry-unsupported");
        std::fs::create_dir_all(d2.lock()).unwrap();
        let got = acquire_retry_with(&d2.lock(), 50, || panic!("試せないときは、取り直さない"));
        assert!(matches!(got, Acquired::Unsupported(_)));
    }

    /// 実際の `sleep` を使う公開の関数が、同じ動きをすること（待ち時間は、ごく短く）。
    #[test]
    fn the_public_retry_gives_up_with_already_running_when_the_lock_stays_held() {
        let d = Dir::new("retry-public");
        let held = acquire(&d.lock());
        assert!(matches!(held, Acquired::Locked(_)));
        let got = acquire_retry(&d.lock(), 2, std::time::Duration::from_millis(1));
        assert!(matches!(got, Acquired::AlreadyRunning));
    }
}
