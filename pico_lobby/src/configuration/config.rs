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
    let toml_str = annotate_npc_skin_example(&toml_str);
    fs::write(path, toml_str)?;
    Ok(cfg)
}

/// Commented `[lobby.npcs.skin]` example injected into freshly generated configs.
///
/// The NPC `skin` field is optional and omitted from serialization when unset
/// (no skin = default Steve/Alex), so it never appears in the generated file on
/// its own. We surface it here as a discoverable, copy-pasteable example.
const NPC_SKIN_EXAMPLE: &str = "\
# Optional skin for the [[lobby.npcs]] entry above. Omit for the default skin.
# Skins render on Minecraft 1.8+ clients only; if a skin fails to resolve the
# NPC spawns skinless without blocking startup.
# Mirror an existing account by name or UUID (resolved from Mojang at startup):
# [lobby.npcs.skin]
# player = \"Notch\"
# Or provide a raw signed textures property (offline; signature is optional):
# [lobby.npcs.skin]
# value = \"ewogICJ0aW1lc3RhbXAiIDog...\"
# signature = \"GnG2...\"
";

/// Insert [`NPC_SKIN_EXAMPLE`] immediately after the first `[[lobby.npcs]]`
/// block, before the following section, so the commented `[lobby.npcs.skin]`
/// lands in a position where it is valid TOML when uncommented.
fn annotate_npc_skin_example(toml_str: &str) -> String {
    let Some(npc_pos) = toml_str.find("[[lobby.npcs]]") else {
        return toml_str.to_string();
    };

    // The next line beginning with `[` after the NPC block is the following
    // section header (NPC field lines never start with `[`). If there is none,
    // the NPC block is the final section and we append at the end.
    let insert_at = toml_str[npc_pos..]
        .find("\n[")
        .map_or(toml_str.len(), |rel| npc_pos + rel + 1);
    let (head, tail) = toml_str.split_at(insert_at);

    let mut out = String::with_capacity(toml_str.len() + NPC_SKIN_EXAMPLE.len() + 2);
    out.push_str(head);
    // Ensure a blank line separates the example from the NPC block above.
    if !head.ends_with("\n\n") {
        out.push_str(if head.ends_with('\n') { "\n" } else { "\n\n" });
    }
    out.push_str(NPC_SKIN_EXAMPLE);
    if tail.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
    } else {
        out.push('\n');
        out.push_str(tail);
    }
    out
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
    fn generated_config_documents_npc_skin_and_still_parses() {
        let toml_str = toml::to_string_pretty(&Config::default()).unwrap();
        let annotated = annotate_npc_skin_example(&toml_str);

        // The commented example is present and sits before the next section.
        assert!(annotated.contains("# [lobby.npcs.skin]"));
        assert!(annotated.contains("# player = \"Notch\""));
        let skin_pos = annotated.find("# [lobby.npcs.skin]").unwrap();
        let npc_pos = annotated.find("[[lobby.npcs]]").unwrap();
        let next_section = annotated.find("[lobby.private_messages]").unwrap();
        assert!(npc_pos < skin_pos && skin_pos < next_section);

        // Comments are ignored, so the generated file parses unchanged.
        toml::from_str::<Config>(&annotated).unwrap();
    }

    #[test]
    fn uncommented_skin_example_is_valid_config() {
        let toml_str = toml::to_string_pretty(&Config::default()).unwrap();
        let annotated = annotate_npc_skin_example(&toml_str);

        // Uncomment only the player-variant lines of the injected example.
        let uncommented = annotated.replace(
            "# [lobby.npcs.skin]\n# player = \"Notch\"",
            "[lobby.npcs.skin]\nplayer = \"Notch\"",
        );
        let config: Config = toml::from_str(&uncommented).unwrap();

        assert!(matches!(
            config.lobby.npcs[0].skin,
            Some(crate::configuration::lobby::NpcSkinConfig::Player { ref player }) if player == "Notch"
        ));
    }
}
