use crate::configuration::antispam::AntispamConfig;
use serde::{Deserialize, Serialize};

pub const DEFAULT_CHAT_FORMAT: &str = "<white>&lt;{sender}&gt; {message}</white>";
pub const DEFAULT_JOIN_MESSAGE: &str = "<yellow>{player} joined the game</yellow>";
pub const DEFAULT_LEAVE_MESSAGE: &str = "<yellow>{player} left the game</yellow>";
pub const DEFAULT_PRIVATE_MESSAGE_SENDER_FORMAT: &str =
    "<gray>[me -> {recipient}]</gray> <white>{message}</white>";
pub const DEFAULT_PRIVATE_MESSAGE_RECIPIENT_FORMAT: &str =
    "<gray>[{sender} -> me]</gray> <white>{message}</white>";
pub const DEFAULT_PRIVATE_MESSAGE_UNKNOWN_TARGET: &str =
    "<red>Player '{target}' is not online in the lobby.</red>";
pub const DEFAULT_PRIVATE_MESSAGE_AMBIGUOUS_TARGET: &str =
    "<red>More than one online player matches '{target}'.</red>";
pub const DEFAULT_PRIVATE_MESSAGE_HIDDEN_TARGET: &str =
    "<red>{target} cannot receive private messages with hidden chat.</red>";
pub const DEFAULT_PRIVATE_MESSAGE_MISSING_REPLY_TARGET: &str =
    "<red>You have nobody to reply to.</red>";
pub const DEFAULT_PRIVATE_MESSAGE_SELF_MESSAGE: &str =
    "<red>You cannot send a private message to yourself.</red>";
pub const DEFAULT_PRIVATE_MESSAGE_EMPTY_MESSAGE: &str =
    "<red>Private message cannot be empty.</red>";
pub const DEFAULT_PRIVATE_MESSAGE_TOO_LONG: &str = "<red>Private message is too long.</red>";
pub const DEFAULT_PRIVATE_MESSAGE_RATE_LIMIT: &str =
    "<red>You are sending messages too quickly.</red>";
pub const DEFAULT_PRIVATE_MESSAGE_UNAVAILABLE: &str =
    "<red>Private messages are only available in the lobby.</red>";
pub const DEFAULT_NPC_TAB_LIST_REMOVE_DELAY_MS: u64 = 3000;

/// Default item shown for a selector entry when none is configured.
pub const DEFAULT_SELECTOR_ENTRY_ITEM: &str = "minecraft:paper";

/// Default item used to fill the selector GUI's empty background slots.
pub const DEFAULT_SELECTOR_FILLER_ITEM: &str = "minecraft:gray_stained_glass_pane";

fn default_selector_entry_item() -> String {
    DEFAULT_SELECTOR_ENTRY_ITEM.to_string()
}

fn default_selector_entry_lore() -> Vec<String> {
    vec!["<gray>Click to connect.".to_string()]
}

/// A downstream server reachable via the Velocity proxy.
/// `server` must match a key in Velocity's `[servers]` block.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LobbyServerEntry {
    pub id: String,
    pub display_name: String,
    pub server: String,
    /// Item identifier rendered for this entry inside the selector GUI,
    /// e.g. `"minecraft:grass_block"`. Defaults to `minecraft:paper`.
    #[serde(default = "default_selector_entry_item")]
    pub item: String,
    /// Optional `MiniMessage` lore lines shown when hovering the entry.
    /// Defaults to a single `Click to connect.` line.
    #[serde(default = "default_selector_entry_lore")]
    pub lore: Vec<String>,
    /// Optional explicit GUI slot (0–26). When omitted the entry is placed in
    /// the first free slot after any explicitly-placed entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<u8>,
    /// When `true`, the entry's item is rendered with the enchantment glint for
    /// extra visual pop, without adding a visible enchantment to its tooltip.
    /// Defaults to `false`.
    #[serde(default)]
    pub enchanted: bool,
}

/// Configuration for the per-player visibility toggle item in the hotbar.
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct VisibilityToggleConfig {
    /// Hotbar slot 0–8.
    pub slot: u8,
    /// Item identifier, e.g. `"minecraft:ender_eye"`.
    pub item: String,
    /// Optional `MiniMessage` display name when players are visible.
    pub display_name_on: Option<String>,
    /// Optional `MiniMessage` display name when players are hidden.
    pub display_name_off: Option<String>,
    /// Optional `MiniMessage` lore lines when players are visible.
    #[serde(default)]
    pub lore_on: Vec<String>,
    /// Optional `MiniMessage` lore lines when players are hidden.
    #[serde(default)]
    pub lore_off: Vec<String>,
    /// Optional `MiniMessage` feedback message sent when players become visible.
    pub message_on: Option<String>,
    /// Optional `MiniMessage` feedback message sent when players become hidden.
    pub message_off: Option<String>,
}

/// Configuration for the item placed in the selector GUI's empty background
/// slots (every slot not occupied by a server entry).
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct SelectorFillerConfig {
    /// Item identifier, e.g. `"minecraft:gray_stained_glass_pane"`.
    pub item: String,
    /// Optional `MiniMessage` display name. Defaults to an empty name so the
    /// filler is visually blank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Optional `MiniMessage` lore lines.
    #[serde(default)]
    pub lore: Vec<String>,
}

impl Default for SelectorFillerConfig {
    fn default() -> Self {
        Self {
            item: DEFAULT_SELECTOR_FILLER_ITEM.to_string(),
            display_name: Some(" ".to_string()),
            lore: Vec::new(),
        }
    }
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
    /// Optional item used to fill the selector GUI's empty background slots.
    /// When omitted those slots are left empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filler: Option<SelectorFillerConfig>,
}

impl Default for SelectorItemConfig {
    fn default() -> Self {
        Self {
            slot: 4,
            item: "minecraft:compass".to_string(),
            display_name: Some("<bold><gold>Server Selector".to_string()),
            lore: vec!["<gray>Right-click to choose a server.".to_string()],
            filler: Some(SelectorFillerConfig::default()),
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
    /// Delay before removing this NPC from tab lists on clients that need a
    /// player list entry to load skins. Set to `0` to keep the NPC entry for the
    /// whole session on those clients.
    #[serde(default = "default_npc_tab_list_remove_delay_ms")]
    pub tab_list_remove_delay_ms: u64,
    /// Optional skin applied to the NPC. When omitted the NPC renders with the
    /// default Steve/Alex skin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<NpcSkinConfig>,
}

const fn default_npc_tab_list_remove_delay_ms() -> u64 {
    DEFAULT_NPC_TAB_LIST_REMOVE_DELAY_MS
}

/// How an NPC's skin is sourced.
///
/// `Texture` is tried first because it requires a `value` field, whereas
/// `Player` requires a `player` field; the two variants are unambiguous.
#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum NpcSkinConfig {
    /// Raw signed textures property (base64 `value` and optional `signature`),
    /// applied directly without any network lookup.
    Texture {
        value: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    /// Resolve the signed textures of an existing Minecraft account by player
    /// name or UUID at startup via Mojang's session servers.
    Player { player: String },
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct PrivateMessagesConfig {
    pub sender_format: String,
    pub recipient_format: String,
    pub unknown_target: String,
    pub ambiguous_target: String,
    pub hidden_target: String,
    pub missing_reply_target: String,
    pub self_message: String,
    pub empty_message: String,
    pub too_long: String,
    pub rate_limit: String,
    pub unavailable: String,
}

impl Default for PrivateMessagesConfig {
    fn default() -> Self {
        Self {
            sender_format: DEFAULT_PRIVATE_MESSAGE_SENDER_FORMAT.to_string(),
            recipient_format: DEFAULT_PRIVATE_MESSAGE_RECIPIENT_FORMAT.to_string(),
            unknown_target: DEFAULT_PRIVATE_MESSAGE_UNKNOWN_TARGET.to_string(),
            ambiguous_target: DEFAULT_PRIVATE_MESSAGE_AMBIGUOUS_TARGET.to_string(),
            hidden_target: DEFAULT_PRIVATE_MESSAGE_HIDDEN_TARGET.to_string(),
            missing_reply_target: DEFAULT_PRIVATE_MESSAGE_MISSING_REPLY_TARGET.to_string(),
            self_message: DEFAULT_PRIVATE_MESSAGE_SELF_MESSAGE.to_string(),
            empty_message: DEFAULT_PRIVATE_MESSAGE_EMPTY_MESSAGE.to_string(),
            too_long: DEFAULT_PRIVATE_MESSAGE_TOO_LONG.to_string(),
            rate_limit: DEFAULT_PRIVATE_MESSAGE_RATE_LIMIT.to_string(),
            unavailable: DEFAULT_PRIVATE_MESSAGE_UNAVAILABLE.to_string(),
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
    /// `MiniMessage` template for lobby join messages.
    /// Use `{player}` as a placeholder; player names are automatically escaped.
    pub join_message: String,
    /// `MiniMessage` template for lobby leave messages.
    /// Use `{player}` as a placeholder; player names are automatically escaped.
    pub leave_message: String,
    pub servers: Vec<LobbyServerEntry>,
    /// Optional hotbar selector item.  Only active when `enabled = true`.
    pub selector: Option<SelectorItemConfig>,
    /// Optional per-player visibility toggle item.  Only active when `enabled = true`.
    pub visibility_toggle: Option<VisibilityToggleConfig>,
    /// Player-style NPCs that navigate to configured lobby servers.
    pub npcs: Vec<LobbyNpcConfig>,
    pub private_messages: PrivateMessagesConfig,
    pub antispam: AntispamConfig,
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
                item: default_selector_entry_item(),
                lore: default_selector_entry_lore(),
                slot: None,
                enchanted: false,
            }],
            selector: Some(SelectorItemConfig::default()),
            visibility_toggle: Some(VisibilityToggleConfig {
                slot: 8,
                item: "minecraft:ender_eye".to_string(),
                display_name_on: Some("<bold><green>Players Visible".to_string()),
                display_name_off: Some("<bold><red>Players Hidden".to_string()),
                lore_on: vec!["<gray>Right-click to hide other players.".to_string()],
                lore_off: vec!["<gray>Right-click to show other players.".to_string()],
                message_on: Some("<green>Other players are now visible.".to_string()),
                message_off: Some("<red>Other players are now hidden.".to_string()),
            }),
            npcs: vec![LobbyNpcConfig {
                id: "survival-npc".to_string(),
                destination: "survival".to_string(),
                name: "Survival".to_string(),
                x: 0.0,
                y: 320.0,
                z: 4.0,
                yaw: 180.0,
                pitch: 0.0,
                tab_list_remove_delay_ms: DEFAULT_NPC_TAB_LIST_REMOVE_DELAY_MS,
                skin: Some(NpcSkinConfig::Player {
                    player: "Notch".to_string(),
                }),
            }],
            private_messages: PrivateMessagesConfig::default(),
            antispam: AntispamConfig::default(),
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

    #[test]
    fn default_lobby_config_delays_npc_tab_list_removal() {
        let config = LobbyConfig::default();

        assert_eq!(
            config.npcs[0].tab_list_remove_delay_ms,
            DEFAULT_NPC_TAB_LIST_REMOVE_DELAY_MS
        );
    }

    const NPC_BASE: &str =
        "id = \"x\"\ndestination = \"d\"\nname = \"N\"\nx = 0.0\ny = 0.0\nz = 0.0\nyaw = 0.0\n";

    fn parse_npc(extra: &str) -> Result<LobbyNpcConfig, toml::de::Error> {
        toml::from_str(&format!("{NPC_BASE}{extra}"))
    }

    #[test]
    fn npc_without_skin_defaults_to_none() {
        let npc = parse_npc("").unwrap();
        assert!(npc.skin.is_none());
    }

    #[test]
    fn npc_skin_player_variant_parses() {
        let npc = parse_npc("skin = { player = \"Notch\" }\n").unwrap();
        assert!(matches!(npc.skin, Some(NpcSkinConfig::Player { player }) if player == "Notch"));
    }

    #[test]
    fn npc_skin_texture_variant_parses() {
        let npc = parse_npc("skin = { value = \"abc\", signature = \"sig\" }\n").unwrap();
        assert!(matches!(
            npc.skin,
            Some(NpcSkinConfig::Texture { value, signature })
                if value == "abc" && signature.as_deref() == Some("sig")
        ));
    }

    #[test]
    fn npc_skin_texture_variant_allows_missing_signature() {
        let npc = parse_npc("skin = { value = \"abc\" }\n").unwrap();
        assert!(matches!(
            npc.skin,
            Some(NpcSkinConfig::Texture { value, signature })
                if value == "abc" && signature.is_none()
        ));
    }

    #[test]
    fn npc_skin_without_recognised_fields_is_rejected() {
        assert!(parse_npc("skin = { nope = \"x\" }\n").is_err());
    }

    #[test]
    fn npc_unknown_top_level_field_is_rejected() {
        assert!(parse_npc("bogus = true\n").is_err());
    }
}
