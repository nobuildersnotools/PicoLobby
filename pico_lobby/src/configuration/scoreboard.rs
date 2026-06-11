use serde::de::{Error, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScoreboardEnabledMode {
    Lobby,
    Always,
    Never,
}

impl ScoreboardEnabledMode {
    pub const fn should_send(&self, lobby_enabled: bool) -> bool {
        match self {
            Self::Lobby => lobby_enabled,
            Self::Always => true,
            Self::Never => false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct ScoreboardConfig {
    pub enabled: ScoreboardEnabledMode,
    pub title: String,
    pub update_interval_ms: u64,
    pub lines: Vec<String>,
}

impl Default for ScoreboardConfig {
    fn default() -> Self {
        Self {
            enabled: ScoreboardEnabledMode::Lobby,
            title: "<bold>PicoLobby</bold>".to_string(),
            update_interval_ms: 1000,
            lines: vec![
                "<gray>Player: <white>{player}".to_string(),
                "<gray>Online: <green>{online}<dark_gray>/<green>{max_players}".to_string(),
                "<gray>Server: <aqua>{server}".to_string(),
            ],
        }
    }
}

impl Serialize for ScoreboardEnabledMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Lobby => serializer.serialize_str("lobby"),
            Self::Always => serializer.serialize_bool(true),
            Self::Never => serializer.serialize_bool(false),
        }
    }
}

impl<'de> Deserialize<'de> for ScoreboardEnabledMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ScoreboardEnabledModeVisitor)
    }
}

struct ScoreboardEnabledModeVisitor;

impl Visitor<'_> for ScoreboardEnabledModeVisitor {
    type Value = ScoreboardEnabledMode;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("true, false, or \"lobby\"")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(if value {
            ScoreboardEnabledMode::Always
        } else {
            ScoreboardEnabledMode::Never
        })
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        match value {
            "lobby" => Ok(ScoreboardEnabledMode::Lobby),
            _ => Err(E::custom("expected \"lobby\", true, or false")),
        }
    }
}
