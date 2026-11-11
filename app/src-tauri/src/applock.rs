//! 同時起動の防止（GUI 側）。ロックの本体は `mrcr_core::applock`（CLI と共有）。ここは、取れなかったときの案内だけ。
//!
//! 取れなければ、案内を出して、終了する。ロックのファイルは、データの置き場の**隣**（`datadir::lock_file`）。

pub use mrcr_core::applock::{acquire, AppLock, Acquired};

/// 利用者に見せる文（画面の帯にも使う）。
pub fn unsupported_notice(reason: &str) -> String {
    format!("同時起動を防げませんでした（{reason}）。MrCodeReview を、同時に開かないでください。")
}

const ALREADY_RUNNING_TITLE: &str = "すでに起動しています";
const ALREADY_RUNNING_MESSAGE: &str =
    "もう一つの MrCodeReview が、動いています。そちらを使うか、終了してから、開いてください。";

/// AppleScript の文（引用符・バックスラッシュを含まない定数だけで作る。外からの入力は入れない）。
fn alert_script() -> String {
    format!("display alert \"{ALREADY_RUNNING_TITLE}\" message \"{ALREADY_RUNNING_MESSAGE}\" as critical")
}

/// 案内を出して、終了する（コード 1 ＝ 起動しなかった）。
/// 起動の途中（画面の仕組みが立ち上がる前）なので、プラグインのダイアログではなく、OS のものを使う。
pub fn tell_already_running_and_exit() -> ! {
    exit_after_notice(|_title, _message| {
        #[cfg(target_os = "macos")]
        {
            // 案内の表示に失敗しても、標準エラー出力に残っている。どのみち、ここで終了する。
            if let Err(e) = std::process::Command::new("osascript").arg("-e").arg(alert_script()).status() {
                eprintln!("案内を表示できませんでした: {e}");
            }
        }
    })
}

/// 標準エラーに案内を書き、`show`（画面での案内）を呼んで、コード 1 で終了する。`show` は、試験では、何も出さないものに替える。
fn exit_after_notice(show: impl FnOnce(&str, &str)) -> ! {
    eprintln!("{ALREADY_RUNNING_TITLE}: {ALREADY_RUNNING_MESSAGE}");
    show(ALREADY_RUNNING_TITLE, ALREADY_RUNNING_MESSAGE);
    std::process::exit(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notice_says_why_and_what_to_do() {
        assert!(unsupported_notice("x").contains("同時に開かないでください"));
        assert!(alert_script().contains("すでに起動しています"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_alert_script_compiles() {
        let dir = std::env::temp_dir().join(format!("mrcr-applock-script-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("alert.scpt");
        let st = std::process::Command::new("osacompile").arg("-e").arg(alert_script()).arg("-o").arg(&out).status().unwrap();
        let ok = st.success() && out.exists();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(ok, "AppleScript の文が、文法として正しい");
    }

    /// 子プロセスとして起動されたときだけ働く（普段は、何もせず、通る）。`exit_after_notice` を呼ぶと、そのプロセスは、終了する。
    #[test]
    fn already_running_exit_child() {
        if std::env::var_os("MRCR_APPLOCK_EXIT_CHILD").is_some() {
            exit_after_notice(|_, _| {});
        }
    }

    #[test]
    fn telling_that_it_is_already_running_ends_the_process_with_code_1_and_says_why() {
        let exe = std::env::current_exe().unwrap();
        let out = std::process::Command::new(exe)
            .args(["--exact", "applock::tests::already_running_exit_child", "--nocapture"])
            .env("MRCR_APPLOCK_EXIT_CHILD", "1")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1), "二重に起動しない（終了コード 1）: {out:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("すでに起動しています"), "案内を標準エラーに残す: {err}");
    }
}
