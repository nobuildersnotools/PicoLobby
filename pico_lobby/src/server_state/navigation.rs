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

/// Default item identifier used for a selector entry with no configured item.
pub const DEFAULT_SELECTOR_ENTRY_ITEM: &str = "minecraft:paper";

#[derive(Debug, Clone)]
pub struct LobbyDestination {
    pub id: LobbyDestinationId,
    /// Display name for selector GUIs and NPC labels.
    pub display_name: String,
    /// The Velocity server name (must match a key in `velocity.toml [servers]`).
    pub server: String,
    /// Item identifier rendered for this entry inside the selector GUI.
    pub item: String,
    /// `MiniMessage` lore lines shown when hovering the entry.
    pub lore: Vec<String>,
    /// Optional explicit GUI slot (0-based). `None` means auto-placement.
    pub slot: Option<usize>,
    /// When `true`, the entry's item renders with the enchantment glint.
    pub enchanted: bool,
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
            item: DEFAULT_SELECTOR_ENTRY_ITEM.to_string(),
            lore: vec!["<gray>Click to connect.".to_string()],
            slot: None,
            enchanted: false,
        }
    }

    /// Override the GUI item identifier for this entry.
    #[must_use]
    pub fn with_item(mut self, item: impl Into<String>) -> Self {
        self.item = item.into();
        self
    }

    /// Override the GUI hover lore for this entry.
    #[must_use]
    pub fn with_lore(mut self, lore: Vec<String>) -> Self {
        self.lore = lore;
        self
    }

    /// Set an explicit GUI slot for this entry.
    #[must_use]
    pub fn with_slot(mut self, slot: Option<u8>) -> Self {
        self.slot = slot.map(usize::from);
        self
    }

    /// Render this entry's item with the enchantment glint.
    #[must_use]
    pub const fn with_enchanted(mut self, enchanted: bool) -> Self {
        self.enchanted = enchanted;
        self
    }
}

#[derive(Debug, Error)]
pub enum NavigationError {
    #[error("unknown destination '{0}'")]
    UnknownDestination(String),
}
