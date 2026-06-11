use serde::{Deserialize, Serialize};

pub const DEFAULT_CHAT_COOLDOWN_MS: u64 = 750;
pub const DEFAULT_CHAT_ANTISPAM_MESSAGE: &str = "<red>You are sending messages too quickly.</red>";

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct AntispamConfig {
    pub enabled: bool,
    pub chat_cooldown_ms: u64,
    pub message: String,
}

impl AntispamConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }
}

impl Default for AntispamConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            chat_cooldown_ms: DEFAULT_CHAT_COOLDOWN_MS,
            message: DEFAULT_CHAT_ANTISPAM_MESSAGE.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_antispam_config_matches_basic_chat_limit() {
        let config = AntispamConfig::default();

        assert!(config.enabled);
        assert_eq!(config.chat_cooldown_ms, DEFAULT_CHAT_COOLDOWN_MS);
        assert_eq!(config.message, DEFAULT_CHAT_ANTISPAM_MESSAGE);
    }
}
