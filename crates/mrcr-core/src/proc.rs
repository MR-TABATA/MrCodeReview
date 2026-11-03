//! 外部コマンドを、探して、時間切れ付きで実行する。`claude -p` と linter が共有する。
//!
//! - **探す**: Finder や Dock から起動したアプリは、シェルの PATH（/opt/homebrew/bin や ~/.cargo/bin）を
//!   引き継がない。PATH に加えて、よくある場所も探し、子プロセスにも補った PATH を渡す
//!   （`#!/usr/bin/env node` の linter が node を、cargo が rustc を見つけられるように）。
//! - **止める**: 子は専用のプロセスグループで起動し、時間切れではグループごと止める。
//!   親だけ止めると、孫（cargo が起動した rustc など）が残り、出力のパイプも握り続ける。

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Child, Command, Output};
use std::sync::mpsc;
use std::time::Duration;

#[derive(Debug)]
pub enum Wait {
    /// 時間内に終わらなかった（プロセスグループごと止めた）。
    Timeout,
    Io(String),
}

/// PATH に無くても探す、よくある場所。
fn extra_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = ["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from).to_vec();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.extend([".cargo/bin", ".local/bin", ".claude/local", ".npm-global/bin"].map(|d| home.join(d)));
    }
    dirs
}

/// 子プロセスに渡す PATH。いまの PATH に、よくある場所を足す（重複は除く）。
pub fn augmented_path() -> OsString {
    let current: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    // 順序は保ち、重複は除く（もとの PATH に重複があっても）
    let mut dirs: Vec<PathBuf> = Vec::new();
    for d in current.into_iter().chain(extra_dirs()) {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

/// PATH とよくある場所から、実行ファイルを探す。
pub fn find_program(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    dirs.extend(extra_dirs());
    dirs.into_iter().map(|d| d.join(name)).find(|c| c.is_file())
}

/// 専用のプロセスグループで起動する。補った PATH を渡す。
pub fn spawn(cmd: &mut Command) -> std::io::Result<Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0); // 子が、自分をグループの先頭（グループ ID = 子の PID）にする
    }
    cmd.env("PATH", augmented_path()).spawn()
}

/// 子プロセスの終了を待って出力を集める。`timeout` を過ぎたら、プロセスグループごと強制終了する。
/// `child` は、`spawn` で起動したもの。
pub fn wait_with_timeout(child: Child, timeout: Duration) -> Result<Output, Wait> {
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => r.map_err(|e| Wait::Io(e.to_string())),
        Err(_) => {
            // グループ全体（負の PID）。孫が出力のパイプを握ったまま残るのを防ぐ
            let _ = Command::new("kill").args(["-9", "--", &format!("-{pid}")]).status();
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            Err(Wait::Timeout)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;

    fn sh(script: &str) -> Child {
        let mut c = Command::new("sh");
        c.args(["-c", script]).stdout(Stdio::piped()).stderr(Stdio::piped());
        spawn(&mut c).unwrap()
    }

    fn alive(pid: u32) -> bool {
        Command::new("kill").args(["-0", &pid.to_string()]).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
    }

    #[test]
    fn collects_output_and_exit_status() {
        let out = wait_with_timeout(sh("echo hi; echo err >&2; exit 3"), Duration::from_secs(10)).unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hi");
        assert_eq!(String::from_utf8_lossy(&out.stderr).trim(), "err");
        assert_eq!(out.status.code(), Some(3));
    }

    #[test]
    fn kills_a_process_that_overruns() {
        let started = std::time::Instant::now();
        let r = wait_with_timeout(sh("sleep 30"), Duration::from_millis(300));
        assert!(matches!(r, Err(Wait::Timeout)));
        assert!(started.elapsed() < Duration::from_secs(5), "待たずに戻る");
    }

    #[test]
    fn a_timeout_also_kills_grandchildren() {
        let file = std::env::temp_dir().join(format!("mrcr-proc-grandchild-{}", std::process::id()));
        let _ = std::fs::remove_file(&file);
        // 孫（sleep）を起動して、その PID を書き、親は待つ。親だけ止めると、孫が残る
        let script = format!("sleep 31 & echo $! > '{}'; wait", file.display());
        let r = wait_with_timeout(sh(&script), Duration::from_millis(600));
        assert!(matches!(r, Err(Wait::Timeout)));
        let pid: u32 = std::fs::read_to_string(&file).unwrap().trim().parse().unwrap();
        // 終了の反映を少し待つ
        let mut gone = false;
        for _ in 0..40 {
            if !alive(pid) {
                gone = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status(); // 失敗時の後始末
        let _ = std::fs::remove_file(&file);
        assert!(gone, "孫プロセスが残っている（pid {pid}）");
    }

    #[test]
    fn finds_programs_in_common_places_and_augments_the_path() {
        assert!(find_program("sh").is_some());
        assert!(find_program("definitely-not-a-real-program-xyz").is_none());
        let p = augmented_path();
        let dirs: Vec<PathBuf> = std::env::split_paths(&p).collect();
        assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
        // 重複しない
        let mut seen = std::collections::HashSet::new();
        assert!(dirs.iter().all(|d| seen.insert(d.clone())));
    }

    #[test]
    fn wait_with_timeout_is_safe_even_if_child_was_not_spawned_with_proc_spawn() {
        // proc::spawn を経由せず通常の Command::spawn で起動した子プロセス
        let mut cmd = Command::new("sleep");
        cmd.arg("10");
        let child = cmd.spawn().unwrap();
        let pid = child.id();
        let r = wait_with_timeout(child, Duration::from_millis(200));
        assert!(matches!(r, Err(Wait::Timeout)));
        let mut gone = false;
        for _ in 0..20 {
            if !alive(pid) {
                gone = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
        assert!(gone, "通常 spawn の子プロセスも終了する");
    }
}
