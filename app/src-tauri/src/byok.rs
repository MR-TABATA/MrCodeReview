//! BYOK（自分の API キーで、LLM の API を直接呼ぶ）の設定（GUI 側）。
//!
//! コアロジック（検証・ファイル保存）は `mrcr_core::byok` を使い、
//! ここでは macOS キーチェーン（`keychain.rs`）と連携した画面状態（`Status`）を扱う。

pub use mrcr_core::byok::*;

use crate::keychain;
use serde::Serialize;
use std::path::Path;

/// 画面に出す状態。キーそのものは返さない（末尾 4 文字だけ）。
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    /// このマシンで、キーを保管できるか（いまは macOS だけ）。
    pub supported: bool,
    pub key_set: bool,
    pub key_last4: Option<String>,
    /// キーを読めなかった理由（あれば）。
    pub key_error: Option<String>,
    pub model: String,
    pub max_tokens: u64,
    /// 設定ファイルが壊れているときの理由（あれば。その間は初期値を見せる）。
    pub config_error: Option<String>,
}

pub fn status(dir: &Path) -> Status {
    let (cfg, config_error) = match load(dir) {
        Ok(c) => (c, None),
        Err(e) => (Config::default(), Some(e)),
    };
    let (key_set, key_last4, key_error) = if keychain::SUPPORTED {
        match keychain::get(keychain::SERVICE, keychain::ACCOUNT_ANTHROPIC) {
            Ok(Some(k)) => (true, Some(keychain::last4(&k)), None),
            Ok(None) => (false, None, None),
            Err(e) => (false, None, Some(e)),
        }
    } else {
        (false, None, None)
    };
    Status {
        supported: keychain::SUPPORTED,
        key_set,
        key_last4,
        key_error,
        model: cfg.model,
        max_tokens: cfg.max_tokens,
        config_error,
    }
}
