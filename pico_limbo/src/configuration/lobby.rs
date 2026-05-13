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

/// Configuration for the hotbar selector item placed in the player's inventory.
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct SelectorItemConfig {
    /// Hotbar slot 0–8.
    pub slot: u8,
    /// Item identifier, e.g. `"minecraft:compass"`.
    pub item: String,
    /// Optional `MiniMessage` display name override.
    pub display_name: Option<String>,
    /// Optional `MiniMessage` lore lines.
    #[serde(default)]
    pub lore: Vec<String>,
}

impl Default for SelectorItemConfig {
    fn default() -> Self {
        Self {
            slot: 4,
            item: "minecraft:compass".to_string(),
            display_name: Some("<bold><gold>Server Selector".to_string()),
            lore: vec!["<gray>Right-click to choose a server.".to_string()],
        }
    }
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
    /// Optional hotbar selector item.  Only active when `enabled = true`.
    pub selector: Option<SelectorItemConfig>,
}

impl Default for LobbyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            chat_format: DEFAULT_CHAT_FORMAT.to_string(),
            servers: Vec::new(),
            selector: Some(SelectorItemConfig::default()),
        }
    }
}
