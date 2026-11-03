use serde::{Deserialize, Serialize};

pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
pub const DEFAULT_MAX_TOKENS: u64 = 8192;
pub const MAX_TOKENS_RANGE: (u64, u64) = (256, 64_000);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
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
