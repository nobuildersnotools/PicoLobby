use serde::{Deserialize, Serialize};

pub const DEFAULT_CHAT_FORMAT: &str = "<white>&lt;{sender}&gt; {message}</white>";
pub const DEFAULT_JOIN_MESSAGE: &str = "<yellow>{player} joined the game</yellow>";
pub const DEFAULT_LEAVE_MESSAGE: &str = "<yellow>{player} left the game</yellow>";

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

#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct LobbyNpcConfig {
    pub id: String,
    pub destination: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    #[serde(default)]
    pub pitch: f32,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct LobbyConfig {
    pub enabled: bool,
    /// `MiniMessage` template for lobby chat messages.
    /// Use `{sender}` and `{message}` as placeholders; user input is automatically escaped.
    pub chat_format: String,
    /// `MiniMessage` template for lobby join messages.
    /// Use `{player}` as a placeholder; player names are automatically escaped.
    pub join_message: String,
    /// `MiniMessage` template for lobby leave messages.
    /// Use `{player}` as a placeholder; player names are automatically escaped.
    pub leave_message: String,
    pub servers: Vec<LobbyServerEntry>,
    /// Optional hotbar selector item.  Only active when `enabled = true`.
    pub selector: Option<SelectorItemConfig>,
    /// Player-style NPCs that navigate to configured lobby servers.
    pub npcs: Vec<LobbyNpcConfig>,
}

impl Default for LobbyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            chat_format: DEFAULT_CHAT_FORMAT.to_string(),
            join_message: DEFAULT_JOIN_MESSAGE.to_string(),
            leave_message: DEFAULT_LEAVE_MESSAGE.to_string(),
            servers: vec![LobbyServerEntry {
                id: "survival".to_string(),
                display_name: "Survival".to_string(),
                server: "survival".to_string(),
            }],
            selector: Some(SelectorItemConfig::default()),
            npcs: vec![LobbyNpcConfig {
                id: "survival-npc".to_string(),
                destination: "survival".to_string(),
                name: "Survival".to_string(),
                x: 0.0,
                y: 320.0,
                z: 4.0,
                yaw: 180.0,
                pitch: 0.0,
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_lobby_config_includes_lifecycle_messages() {
        let config = LobbyConfig::default();

        assert_eq!(config.join_message, DEFAULT_JOIN_MESSAGE);
        assert_eq!(config.leave_message, DEFAULT_LEAVE_MESSAGE);
    }
}
