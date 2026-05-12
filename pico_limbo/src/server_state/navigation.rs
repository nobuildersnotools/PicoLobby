use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LobbyDestinationId(pub String);

impl LobbyDestinationId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for LobbyDestinationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone)]
pub struct LobbyDestination {
    pub id: LobbyDestinationId,
    /// Display name for selector GUIs and NPC labels (used in milestone 5+).
    #[allow(dead_code)]
    pub display_name: String,
    /// The Velocity server name (must match a key in `velocity.toml [servers]`).
    pub server: String,
}

impl LobbyDestination {
    pub fn new(
        id: impl Into<String>,
        display_name: impl Into<String>,
        server: impl Into<String>,
    ) -> Self {
        Self {
            id: LobbyDestinationId::new(id),
            display_name: display_name.into(),
            server: server.into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum NavigationError {
    #[error("unknown destination '{0}'")]
    UnknownDestination(String),
}
