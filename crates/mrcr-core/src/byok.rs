use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE: &str = "byok.json";
pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
pub const DEFAULT_MAX_TOKENS: u64 = 8192;
pub const MAX_TOKENS_RANGE: (u64, u64) = (256, 64_000);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub model: String,
    pub max_tokens: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            model: DEFAULT_MODEL.into(),
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

pub fn validate(c: &Config) -> Result<(), String> {
    let m = c.model.as_str();
    if m.is_empty() || m.len() > 100 {
        return Err("モデル名は、1〜100 文字で書いてください".into());
    }
    if !m.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | ':')) {
        return Err("モデル名に使えない文字（空白・日本語・記号など）が入っています。英数字と - . _ : だけです".into());
    }
    let (lo, hi) = MAX_TOKENS_RANGE;
    if c.max_tokens < lo || c.max_tokens > hi {
        return Err(format!("最大出力トークンは、{lo}〜{hi} の範囲で指定してください（初期値: {DEFAULT_MAX_TOKENS}）"));
    }
    Ok(())
}

pub fn load(dir: &Path) -> Result<Config, String> {
    let p = path(dir);
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(format!("{} を読めません: {e}", p.display())),
    };
    let c: Config = serde_json::from_str(&text).map_err(|e| format!("{} が正しくありません: {e}", p.display()))?;
    validate(&c).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(c)
}

pub fn save(dir: &Path, c: &Config) -> Result<(), String> {
    validate(c)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("保存先を作れません: {e}"))?;
    let p = path(dir);
    let tmp = dir.join(format!("{FILE}.tmp"));
    let text = serde_json::to_string_pretty(c).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text + "\n").map_err(|e| format!("書けません: {e}"))?;
    std::fs::rename(&tmp, &p).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("保存できません: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mrcr-byok-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn validate_model_name_rules() {
        // 正常な文字種
        assert!(validate(&Config { model: "claude-3-5-sonnet-20241022".into(), max_tokens: 8192 }).is_ok());
        assert!(validate(&Config { model: "model_with.dots:and-dashes".into(), max_tokens: 8192 }).is_ok());
        assert!(validate(&Config { model: "a".repeat(100), max_tokens: 8192 }).is_ok());

        // 不正な文字種・長さ
        assert!(validate(&Config { model: "".into(), max_tokens: 8192 }).is_err());
        assert!(validate(&Config { model: "a".repeat(101), max_tokens: 8192 }).is_err());
        assert!(validate(&Config { model: "claude sonnet".into(), max_tokens: 8192 }).is_err());
        assert!(validate(&Config { model: "クロード".into(), max_tokens: 8192 }).is_err());
        assert!(validate(&Config { model: "model/with/slashes".into(), max_tokens: 8192 }).is_err());
        assert!(validate(&Config { model: "model@home".into(), max_tokens: 8192 }).is_err());
    }

    #[test]
    fn validate_max_tokens_boundaries() {
        // 境界値の内側（Ok）
        assert!(validate(&Config { model: "valid-model".into(), max_tokens: 256 }).is_ok());
        assert!(validate(&Config { model: "valid-model".into(), max_tokens: 64_000 }).is_ok());

        // 境界値の外側（Err）
        assert!(validate(&Config { model: "valid-model".into(), max_tokens: 255 }).is_err());
        assert!(validate(&Config { model: "valid-model".into(), max_tokens: 64_001 }).is_err());
        assert!(validate(&Config { model: "valid-model".into(), max_tokens: 0 }).is_err());
    }

    #[test]
    fn load_fallback_on_missing_file() {
        let d = scratch("missing");
        let loaded = load(&d).unwrap();
        assert_eq!(loaded, Config::default());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn load_abnormal_handling_for_corrupt_and_invalid_json() {
        let d = scratch("corrupt");
        let p = path(&d);

        // 不正な構文の JSON
        std::fs::write(&p, "{ invalid json").unwrap();
        let err = load(&d).unwrap_err();
        assert!(err.contains("byok.json"));

        // 未知のキーが含まれる JSON (deny_unknown_fields)
        std::fs::write(&p, r#"{"api_key": "sk-ant-test"}"#).unwrap();
        let err2 = load(&d).unwrap_err();
        assert!(err2.contains("byok.json"));

        // 境界値外のバリデーション違反 JSON
        std::fs::write(&p, r#"{"model": "valid", "max_tokens": 10}"#).unwrap();
        let err3 = load(&d).unwrap_err();
        assert!(err3.contains("最大出力トークン"));

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn save_and_load_round_trip_with_atomic_file_handling() {
        let d = scratch("save-atomic");
        let cfg = Config {
            model: "claude-3-7-sonnet".into(),
            max_tokens: 4096,
        };

        // 正常保存
        save(&d, &cfg).unwrap();
        assert_eq!(load(&d).unwrap(), cfg);
        // 一時ファイルが残っていないこと
        assert!(!d.join(format!("{FILE}.tmp")).exists());

        // バリデーションエラー時は保存されず一時ファイルも残らない
        let invalid = Config {
            model: "invalid model with spaces".into(),
            max_tokens: 4096,
        };
        assert!(save(&d, &invalid).is_err());
        // 元の正常な設定が維持されていること
        assert_eq!(load(&d).unwrap(), cfg);
        assert!(!d.join(format!("{FILE}.tmp")).exists());

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn save_fails_on_unwritable_path_without_leaving_tmp_file() {
        let d = scratch("unwritable");
        // ファイルと同名のパスを作ってディレクトリ作成・書き込みを失敗させる
        let file_as_dir = d.join("not_a_dir");
        std::fs::write(&file_as_dir, "blocking file").unwrap();

        let cfg = Config::default();
        let err = save(&file_as_dir, &cfg);
        assert!(err.is_err());
        assert!(!file_as_dir.join(format!("{FILE}.tmp")).exists());

        let _ = std::fs::remove_dir_all(&d);
    }
}
