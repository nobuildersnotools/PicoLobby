use serde::{Deserialize, Serialize};

pub const DEFAULT_CHAT_FORMAT: &str = "<white>&lt;{sender}&gt; {message}</white>";

/// A downstream server reachable via the Velocity proxy.
/// `server` must match a key in Velocity's `[servers]` block.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LobbyServerEntry {
    pub id: String,
    pub display_name: String,
    pub server: String,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct LobbyConfig {
    pub enabled: bool,
    /// `MiniMessage` template for lobby chat messages.
    /// Use `{sender}` and `{message}` as placeholders; user input is automatically escaped.
    pub chat_format: String,
    pub servers: Vec<LobbyServerEntry>,
}

impl Default for LobbyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            chat_format: DEFAULT_CHAT_FORMAT.to_string(),
            servers: Vec::new(),
        }
    }
}
