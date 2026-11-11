//! API キーの保管（macOS キーチェーン）。
//!
//! - キーは、`settings.json` にも、`byok.json` にも、履歴にも書かない。置き場所はキーチェーンだけ。
//! - `security` コマンドを使う（追加のクレートは入れない）。保存するときは、キーをコマンドの**引数に出さず**、
//!   `security -i`（対話モード）の標準入力に渡す（同じユーザーの `ps` から見えないように）。読み出しは、標準出力で受け取る。
//! - エラーの文に、キーは入れない。

#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};

/// キーチェーンの項目の「サービス名」（アプリの識別子と同じ）。
pub const SERVICE: &str = "com.hitoshi.mrcodereview";
pub const ACCOUNT_ANTHROPIC: &str = "anthropic-api-key";

/// 入力されたキーの形を確かめる。前後の空白は除く。使えない文字は、理由だけを返す（キー自身は、文に入れない）。
pub fn validate_key(input: &str) -> Result<String, String> {
    let key = input.trim();
    if key.is_empty() {
        return Err("キーが空です".into());
    }
    if key.len() < 20 || key.len() > 400 {
        return Err("キーの長さが合いません（20〜400 文字）。貼り付けの途中で切れていないか確かめてください".into());
    }
    if !key.chars().all(|c| c.is_ascii_graphic()) {
        return Err("キーに使えない文字（空白・改行・日本語など）が入っています".into());
    }
    if key.chars().any(|c| matches!(c, '"' | '\'' | '\\')) {
        return Err("キーに使えない文字（引用符・バックスラッシュ）が入っています".into());
    }
    Ok(key.to_string())
}

/// 画面に出してよい、キーの目印（末尾 4 文字）。
pub fn last4(key: &str) -> String {
    let n = key.chars().count();
    key.chars().skip(n.saturating_sub(4)).collect()
}

#[cfg(target_os = "macos")]
fn security(args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("/usr/bin/security")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("キーチェーンを使えません（security を起動できない: {e}）"))
}

/// `security -i` に渡す 1 行を作る。値は二重引用符で囲む。引用符・バックスラッシュ・制御文字・空白を含む値は、
/// 構文を壊す（別の命令に化ける）ので、作らずに断る。**キーそのものは、エラーの文に入れない。**
fn add_command_line(account: &str, service: &str, key: &str) -> Result<String, String> {
    let plain = |v: &str| !v.is_empty() && v.chars().all(|c| c.is_ascii_graphic() && !matches!(c, '"' | '\'' | '\\'));
    if !plain(account) || !plain(service) {
        return Err("キーチェーンの項目名に、使えない文字が入っています".into());
    }
    if !plain(key) {
        return Err("キーに使えない文字（空白・引用符・バックスラッシュ・改行など）が入っています".into());
    }
    Ok(format!("add-generic-password -a \"{account}\" -s \"{service}\" -w \"{key}\" -U\n"))
}

/// `security -i` を 1 回走らせた結果（試験で、差し替えられるようにするための、最小の形）。
struct Ran {
    success: bool,
    stderr: String,
}

/// 保存の本体。`security` の実行（`run`）と、保存後の読み戻し（`readback`）を、引数で受ける（試験で、失敗の経路を作れるように）。
/// 手順: 命令行を作る（作れなければ、実行せずに断る）→ 実行 → 終了コードが失敗、または標準エラーに出力があれば失敗
/// （対話モードは、命令が失敗しても、終了コードが 0 のことがある）→ 読み戻して、同じ値かを確かめる。**エラーの文に、キーは入れない。**
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn set_via(
    service: &str,
    account: &str,
    key: &str,
    run: impl FnOnce(&str) -> Result<Ran, String>,
    readback: impl FnOnce() -> Result<Option<String>, String>,
) -> Result<(), String> {
    let line = add_command_line(account, service, key)?;
    let ran = run(&line)?;
    if !ran.success || !ran.stderr.trim().is_empty() {
        return Err(format!("キーチェーンに保存できませんでした: {}", ran.stderr.trim()));
    }
    match readback()? {
        Some(saved) if saved == key => Ok(()),
        _ => Err("キーチェーンに保存できませんでした（保存したはずのキーを、読み戻せませんでした）".into()),
    }
}

/// 保存する（同じ項目があれば、置き換える）。キーは、標準入力で渡す（コマンドの引数に出さない）。
#[cfg(target_os = "macos")]
pub fn set(service: &str, account: &str, key: &str) -> Result<(), String> {
    use std::io::Write;
    set_via(
        service,
        account,
        key,
        |line| {
            let mut child = Command::new("/usr/bin/security")
                .arg("-i")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| format!("キーチェーンを使えません（security を起動できない: {e}）"))?;
            {
                let mut stdin = child.stdin.take().ok_or("キーチェーンを使えません（標準入力を開けない）")?;
                stdin.write_all(line.as_bytes()).map_err(|e| format!("キーチェーンに保存できませんでした（標準入力に書けない: {e}）"))?;
            } // ここで標準入力を閉じる（対話モードが、終わる）
            let out = child.wait_with_output().map_err(|e| format!("キーチェーンに保存できませんでした: {e}"))?;
            Ok(Ran { success: out.status.success(), stderr: String::from_utf8_lossy(&out.stderr).into_owned() })
        },
        || get(service, account),
    )
}

/// 読む。項目が無ければ `None`。
#[cfg(target_os = "macos")]
pub fn get(service: &str, account: &str) -> Result<Option<String>, String> {
    let out = security(&["find-generic-password", "-a", account, "-s", service, "-w"])?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim_end_matches(['\n', '\r']).to_string();
        return Ok(if s.is_empty() { None } else { Some(s) });
    }
    // 44 = 項目が見つからない
    if out.status.code() == Some(44) {
        return Ok(None);
    }
    Err(format!("キーチェーンから読めませんでした: {}", String::from_utf8_lossy(&out.stderr).trim()))
}

/// 消す。項目が無くても、成功。
#[cfg(target_os = "macos")]
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    let out = security(&["delete-generic-password", "-a", account, "-s", service])?;
    if out.status.success() || out.status.code() == Some(44) {
        Ok(())
    } else {
        Err(format!("キーチェーンから消せませんでした: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

#[cfg(not(target_os = "macos"))]
const UNSUPPORTED: &str = "API キーの保管は、いまは macOS のキーチェーンだけに対応しています";
#[cfg(not(target_os = "macos"))]
pub fn set(_: &str, _: &str, _: &str) -> Result<(), String> {
    Err(UNSUPPORTED.into())
}
#[cfg(not(target_os = "macos"))]
pub fn get(_: &str, _: &str) -> Result<Option<String>, String> {
    Err(UNSUPPORTED.into())
}
#[cfg(not(target_os = "macos"))]
pub fn delete(_: &str, _: &str) -> Result<(), String> {
    Err(UNSUPPORTED.into())
}

/// このマシンで、キーを保管できるか。
pub const SUPPORTED: bool = cfg!(target_os = "macos");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_must_be_plain_ascii_without_spaces_and_of_sane_length() {
        assert_eq!(validate_key("  sk-ant-api03-abcdefghijklmnop  \n").unwrap(), "sk-ant-api03-abcdefghijklmnop");
        assert!(validate_key("").is_err());
        assert!(validate_key("   ").is_err());
        assert!(validate_key("short").is_err());
        assert!(validate_key(&"a".repeat(401)).is_err());
        assert!(validate_key("sk-ant-api03-abcdefgh ijklmnop").is_err(), "途中の空白");
        assert!(validate_key("sk-ant-api03-abcdefgh\nijklmnop").is_err(), "途中の改行");
        assert!(validate_key("sk-ant-api03-あいうえおかきくけこ").is_err(), "日本語");
    }

    #[test]
    fn the_command_line_for_security_never_puts_a_value_that_could_break_the_syntax() {
        let ok = add_command_line("anthropic-api-key", "com.hitoshi.mrcodereview", "sk-ant-api03-abcdefghijklmnop").unwrap();
        assert_eq!(ok, "add-generic-password -a \"anthropic-api-key\" -s \"com.hitoshi.mrcodereview\" -w \"sk-ant-api03-abcdefghijklmnop\" -U\n");
        // 先頭が「-」のキーも、引用符で囲まれた値として渡る（オプションと取り違えない）
        assert!(add_command_line("a", "s", "-dash-leading-key-0123456789").unwrap().contains("-w \"-dash-leading-key-0123456789\""));
        // エラーの文は、固定の 2 種類だけ（値を文に入れない）。値の全体・各行・各断片のどれも、文に含まれない
        let known = ["キーに使えない文字（空白・引用符・バックスラッシュ・改行など）が入っています", "キーチェーンの項目名に、使えない文字が入っています"];
        for bad in ["abc\"def", "abc'def", "abc\\def", "abc def", "abc\ndef", "abc\"; delete-generic-password -a x -s y\n", "line1-secret\nline2-secret", ""] {
            let e = add_command_line("a", "s", bad).unwrap_err();
            assert!(known.contains(&e.as_str()), "固定の文だけ: {e}");
            assert!(bad.is_empty() || !e.contains(bad), "全体を含まない: {e}");
            for frag in bad.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '\\' | ';')).filter(|f| f.len() >= 4) {
                assert!(!e.contains(frag), "断片 {frag:?} を含まない: {e}");
            }
        }
        assert!(add_command_line("a b", "s", "k").is_err() && add_command_line("a", "s\"", "k").is_err() && add_command_line("", "s", "k").is_err());
    }

    const KEY: &str = "sk-ant-api03-abcdefghijklmnop";

    fn ran(success: bool, stderr: &str) -> Result<Ran, String> {
        Ok(Ran { success, stderr: stderr.into() })
    }

    #[test]
    fn a_save_succeeds_only_when_the_command_is_clean_and_the_key_reads_back() {
        let mut sent = String::new();
        let r = set_via("svc", "acct", KEY, |line| { sent = line.to_string(); ran(true, "") }, || Ok(Some(KEY.to_string())));
        assert!(r.is_ok());
        assert!(sent.contains(KEY) && sent.starts_with("add-generic-password"), "キーは、実行に渡す命令行（標準入力）にだけ入る");
    }

    #[test]
    fn output_on_stderr_is_a_failure_even_when_the_exit_code_is_zero() {
        // 対話モードは、命令が失敗しても、終了コードが 0 のことがある
        let e = set_via("svc", "acct", KEY, |_| ran(true, "security: SecKeychainItemCreateFromContent: User interaction is not allowed.\n"), || panic!("失敗したら、読み戻さない")).unwrap_err();
        assert!(e.contains("保存できませんでした") && e.contains("User interaction is not allowed"), "{e}");
        assert!(!e.contains(KEY), "エラーにキーを入れない");
    }

    #[test]
    fn a_failing_exit_code_is_a_failure() {
        let e = set_via("svc", "acct", KEY, |_| ran(false, ""), || panic!("読み戻さない")).unwrap_err();
        assert!(e.contains("保存できませんでした"), "{e}");
    }

    #[test]
    fn a_value_that_reads_back_differently_or_not_at_all_is_a_failure() {
        for read in [Ok(None), Ok(Some("another-value-0123456789".to_string()))] {
            let e = set_via("svc", "acct", KEY, |_| ran(true, ""), || read).unwrap_err();
            assert!(e.contains("読み戻せませんでした"), "{e}");
            assert!(!e.contains(KEY) && !e.contains("another-value"), "どちらの値も、エラーに入れない: {e}");
        }
    }

    #[test]
    fn a_failure_to_run_or_to_read_back_is_passed_on_as_it_is() {
        let e = set_via("svc", "acct", KEY, |_| Err("security を起動できない: boom".into()), || panic!("読み戻さない")).unwrap_err();
        assert!(e.contains("boom"), "{e}");
        let e = set_via("svc", "acct", KEY, |_| ran(true, ""), || Err("キーチェーンから読めませんでした: denied".into())).unwrap_err();
        assert!(e.contains("denied"), "{e}");
    }

    #[test]
    fn a_value_that_cannot_be_quoted_is_refused_before_anything_runs() {
        let e = set_via("svc", "acct", "bad key with spaces 0123456789", |_| panic!("実行しない"), || panic!("読み戻さない")).unwrap_err();
        assert!(e.contains("使えない文字"), "{e}");
    }

    #[test]
    fn keys_with_quotes_or_backslashes_are_refused_up_front() {
        assert!(validate_key("sk-ant-api03-abc\"defghijklmnop").is_err());
        assert!(validate_key("sk-ant-api03-abc'defghijklmnop").is_err());
        assert!(validate_key("sk-ant-api03-abc\\defghijklmnop").is_err());
    }

    /// 本物のキーチェーンに、使い捨ての項目を作って、保存・読み出し・上書き・削除を確かめる。
    /// キーチェーンを書き換えるので、通常は走らせない: `cargo test -p mrcodereview keychain_roundtrip -- --ignored`
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "本物のキーチェーンに、使い捨ての項目を作る"]
    fn keychain_roundtrip_with_a_throwaway_service() {
        struct Cleanup(String, String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = delete(&self.0, &self.1);
            }
        }
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let service = format!("mrcr-test-throwaway-{}-{nanos}", std::process::id());
        let account = "test-account";
        let _cleanup = Cleanup(service.clone(), account.to_string());

        assert_eq!(get(&service, account).unwrap(), None, "項目が無いときは None（終了コード 44 を、エラーにしない）");
        assert!(delete(&service, account).is_ok(), "無い項目を消しても、成功");
        set(&service, account, "test-key-first-0123456789").unwrap();
        assert_eq!(get(&service, account).unwrap().as_deref(), Some("test-key-first-0123456789"));
        set(&service, account, "-test-key-second-0123456789").unwrap();
        assert_eq!(get(&service, account).unwrap().as_deref(), Some("-test-key-second-0123456789"), "上書きできる（先頭が「-」でも）");
        delete(&service, account).unwrap();
        assert_eq!(get(&service, account).unwrap(), None, "消したあとは、読めない");
    }

    #[test]
    fn error_messages_never_contain_the_key() {
        let key = "sk-ant-api03-SECRETSECRETSECRET";
        for bad in [format!("{key} x"), format!("{key}\nあ")] {
            let e = validate_key(&bad).unwrap_err();
            assert!(!e.contains("SECRET"), "{e}");
        }
    }

    #[test]
    fn last4_is_the_tail_only() {
        assert_eq!(last4("sk-ant-api03-abcdefgh1234"), "1234");
        assert_eq!(last4("ab"), "ab");
    }
}
