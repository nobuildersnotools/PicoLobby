use crate::configuration::boss_bar::BossBarConfig;
use crate::configuration::commands::CommandsConfig;
use crate::configuration::compression::CompressionConfig;
use crate::configuration::env_placeholders::{EnvPlaceholderError, expand_env_placeholders};
use crate::configuration::forwarding::ForwardingConfig;
use crate::configuration::game_mode_config::GameModeConfig;
use crate::configuration::lobby::LobbyConfig;
use crate::configuration::scoreboard::ScoreboardConfig;
use crate::configuration::server_list::ServerListConfig;
use crate::configuration::tab_list::TabListConfig;
use crate::configuration::title::TitleConfig;
use crate::configuration::world_config::WorldConfig;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::{fs, io};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("TOML serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("TOML deserialization error: {0}")]
    TomlDeserialize(#[from] toml::de::Error),

    #[error("Failed to apply environment placeholders: {0}")]
    EnvPlaceholder(#[from] EnvPlaceholderError),
}

/// Application configuration, serializable to/from TOML.
#[derive(Serialize, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
pub struct Config {
    /// Server listening address and port.
    ///
    /// Specify the IP address and port the server should bind to.
    /// Use 0.0.0.0 to listen on all network interfaces.
    pub bind: String,

    pub forwarding: ForwardingConfig,

    pub world: WorldConfig,

    pub server_list: ServerListConfig,

    pub lobby: LobbyConfig,

    /// Message sent to the player after spawning in the world.
    pub welcome_message: String,

    pub action_bar: String,

    /// Sets the default game mode for players
    /// Valid values are: "survival", "creative", "adventure" or "spectator"
    pub default_game_mode: GameModeConfig,

    /// If set to true, will spawn the player in hardcode mode
    pub hardcore: bool,

    pub compression: CompressionConfig,

    pub tab_list: TabListConfig,

    pub fetch_player_skins: bool,

    pub reduced_debug_info: bool,

    pub allow_unsupported_versions: bool,

    pub allow_flight: bool,

    pub accept_transfers: bool,

    pub boss_bar: BossBarConfig,

    pub title: TitleConfig,

    pub scoreboard: ScoreboardConfig,

    pub commands: CommandsConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:25565".into(),
            server_list: ServerListConfig::default(),
            lobby: LobbyConfig::default(),
            welcome_message: "Welcome to PicoLobby!".into(),
            action_bar: "Welcome to PicoLobby!".into(),
            forwarding: ForwardingConfig::default(),
            default_game_mode: GameModeConfig::default(),
            world: WorldConfig::default(),
            hardcore: false,
            tab_list: TabListConfig::default(),
            fetch_player_skins: false,
            reduced_debug_info: false,
            boss_bar: BossBarConfig::default(),
            compression: CompressionConfig::default(),
            title: TitleConfig::default(),
            scoreboard: ScoreboardConfig::default(),
            allow_unsupported_versions: false,
            allow_flight: false,
            accept_transfers: false,
            commands: CommandsConfig::default(),
        }
    }
}

/// Loads a `Config` from the given path.
/// If the file does not exist, it will be created (parent dirs too)
/// and populated with default values.
pub fn load_or_create<P: AsRef<Path>>(path: P) -> Result<Config, ConfigError> {
    let path = path.as_ref();

    if path.exists() {
        let raw_toml_str = fs::read_to_string(path)?;

        if raw_toml_str.trim().is_empty() {
            create_default_config(path)
        } else {
            let expanded_toml_str = expand_env_placeholders(&raw_toml_str)?;
            let cfg: Config = toml::from_str(expanded_toml_str.as_ref())?;
            Ok(cfg)
        }
    } else {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        create_default_config(path)
    }
}

fn create_default_config<P: AsRef<Path>>(path: P) -> Result<Config, ConfigError> {
    let cfg = Config::default();
    let toml_str = toml::to_string_pretty(&cfg)?;
    let toml_str = inline_npc_skins(&toml_str);
    fs::write(path, toml_str)?;
    Ok(cfg)
}

/// Rewrite serialized NPC skins into the inline `skin = { ... }` form.
///
/// `toml::to_string_pretty` renders the optional [`NpcSkinConfig`] field as a
/// standalone `[lobby.npcs.skin]` sub-table (TOML document serializers never
/// emit inline tables). Both forms deserialize identically, but generated
/// configs use the inline `skin = { ... }` key on the owning `[[lobby.npcs]]`
/// entry, so we fold each sub-table back into that entry's body here. NPCs with
/// no skin serialize nothing and are left untouched.
fn inline_npc_skins(toml_str: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut lines = toml_str.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("[lobby.npcs.skin]") {
            // Collect the sub-table's `key = value` body lines.
            let mut kvs: Vec<&str> = Vec::new();
            while let Some(peek) = lines.peek() {
                let trimmed = peek.trim();
                if trimmed.is_empty() || trimmed.starts_with('[') {
                    break;
                }
                kvs.push(trimmed);
                lines.next();
            }
            // Drop the blank line that separated the entry's scalar fields from
            // the sub-table header so the inline key sits flush in the body.
            if out.last().is_some_and(|l| l.trim().is_empty()) {
                out.pop();
            }
            out.push(
                "# skin: use `player = \"<name or uuid>\"` to copy a real account's skin,"
                    .to_string(),
            );
            out.push(
                "# or `value = \"<base64>\"` with an optional `signature = \"<sig>\"` for a raw texture."
                    .to_string(),
            );
            out.push(format!("skin = {{ {} }}", kvs.join(", ")));
            continue;
        }
        out.push(line.to_string());
    }
    let mut joined = out.join("\n");
    if toml_str.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::antispam::{DEFAULT_CHAT_ANTISPAM_MESSAGE, DEFAULT_CHAT_COOLDOWN_MS};

    #[test]
    fn default_config_includes_antispam_defaults() {
        let config = Config::default();

        assert!(config.lobby.antispam.enabled);
        assert_eq!(
            config.lobby.antispam.chat_cooldown_ms,
            DEFAULT_CHAT_COOLDOWN_MS
        );
        assert_eq!(config.lobby.antispam.message, DEFAULT_CHAT_ANTISPAM_MESSAGE);
    }

    #[test]
    fn missing_antispam_section_uses_defaults() {
        let config: Config = toml::from_str("").unwrap();

        assert!(config.lobby.antispam.enabled);
        assert_eq!(
            config.lobby.antispam.chat_cooldown_ms,
            DEFAULT_CHAT_COOLDOWN_MS
        );
        assert_eq!(config.lobby.antispam.message, DEFAULT_CHAT_ANTISPAM_MESSAGE);
    }

    #[test]
    fn unknown_antispam_field_is_rejected() {
        let result = toml::from_str::<Config>(
            "
            [lobby.antispam]
            enabled = true
            unknown = true
            ",
        );

        assert!(result.is_err());
    }

    #[test]
    fn default_config_includes_lobby_gated_scoreboard() {
        let toml = toml::to_string_pretty(&Config::default()).unwrap();

        assert!(toml.contains("[scoreboard]"));
        assert!(toml.contains("enabled = \"lobby\""));
    }

    #[test]
    fn missing_scoreboard_section_uses_default() {
        let config: Config = toml::from_str("").unwrap();

        assert_eq!(
            config.scoreboard.enabled,
            crate::configuration::scoreboard::ScoreboardEnabledMode::Lobby
        );
        assert_eq!(config.scoreboard.lines.len(), 3);
    }

    #[test]
    fn unknown_scoreboard_field_is_rejected() {
        let result = toml::from_str::<Config>(
            "
            [scoreboard]
            enabled = \"lobby\"
            unknown = true
            ",
        );

        assert!(result.is_err());
    }

    #[test]
    fn generated_config_inlines_npc_skin_and_still_parses() {
        let toml_str = toml::to_string_pretty(&Config::default()).unwrap();
        let inlined = inline_npc_skins(&toml_str);

        // The skin is emitted as a real inline key on the NPC entry, not as the
        // old `[lobby.npcs.skin]` sub-table.
        assert!(inlined.contains("skin = { player = \"Notch\" }"));
        assert!(!inlined.contains("[lobby.npcs.skin]"));
        let skin_pos = inlined.find("skin = { player = \"Notch\" }").unwrap();
        let npc_pos = inlined.find("[[lobby.npcs]]").unwrap();
        let next_section = inlined.find("[lobby.private_messages]").unwrap();
        assert!(npc_pos < skin_pos && skin_pos < next_section);

        // The folded document parses back into the same configuration.
        let config: Config = toml::from_str(&inlined).unwrap();
        assert!(matches!(
            config.lobby.npcs[0].skin,
            Some(crate::configuration::lobby::NpcSkinConfig::Player { ref player }) if player == "Notch"
        ));
    }

    #[test]
    fn inline_npc_skins_folds_texture_variant_with_signature() {
        // The default NPC carries a player skin; swap in a texture skin to
        // exercise the multi-key inline form.
        let mut cfg = Config::default();
        cfg.lobby.npcs[0].skin = Some(crate::configuration::lobby::NpcSkinConfig::Texture {
            value: "abc".to_string(),
            signature: Some("sig".to_string()),
        });
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let inlined = inline_npc_skins(&toml_str);

        assert!(inlined.contains("skin = { value = \"abc\", signature = \"sig\" }"));
        assert!(!inlined.contains("[lobby.npcs.skin]"));
        // The generated config explains both skin sources just above the entry.
        assert!(inlined.contains("# skin: use `player ="));
        assert!(inlined.contains("`value ="));
        toml::from_str::<Config>(&inlined).unwrap();
    }

    #[test]
    fn inline_npc_skins_leaves_skinless_npcs_untouched() {
        let mut cfg = Config::default();
        cfg.lobby.npcs[0].skin = None;
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let inlined = inline_npc_skins(&toml_str);

        assert!(!inlined.contains("skin = "));
        assert!(!inlined.contains("[lobby.npcs.skin]"));
        toml::from_str::<Config>(&inlined).unwrap();
    }
}
