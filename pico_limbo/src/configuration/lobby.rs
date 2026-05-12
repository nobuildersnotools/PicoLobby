use serde::{Deserialize, Serialize};

pub const DEFAULT_CHAT_FORMAT: &str = "<white>&lt;{sender}&gt; {message}</white>";

#[derive(Serialize, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct LobbyConfig {
    pub enabled: bool,
    /// MiniMessage template for lobby chat messages.
    /// Use `{sender}` and `{message}` as placeholders; user input is automatically escaped.
    pub chat_format: String,
}

impl Default for LobbyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            chat_format: DEFAULT_CHAT_FORMAT.to_string(),
        }
    }
}
