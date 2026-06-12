use crate::configuration::antispam::AntispamConfig;
use crate::configuration::boss_bar::EnabledBossBarConfig;
use crate::configuration::commands::CommandsConfig;
use crate::configuration::lobby::{
    LobbyNpcConfig, PrivateMessagesConfig, SelectorItemConfig, VisibilityToggleConfig,
};
use crate::configuration::scoreboard::ScoreboardConfig;
use crate::server::chunk_packet_cache::ChunkPacketCache;
use crate::server::client_state::ClientState;
use crate::server::game_mode::GameMode;
use base64::engine::general_purpose;
use base64::{Engine, alphabet, engine};
pub use lobby::{
    ChatVisibility, EntityId, LobbyChatPlan, LobbyJoinPlan, LobbyLeavePlan,
    LobbyLifecycleMessagePlan, LobbyMetadataPlan, LobbyMovementPlan, LobbyNpc, LobbyNpcInteraction,
    LobbyNpcKind, LobbyNpcSpawnPlan, LobbyNpcValidationError, LobbyPosition,
    LobbyPrivateMessageError, LobbyPrivateMessagePlan, LobbyRecipient, LobbySession,
    LobbySessionId, LobbySpawnInfo, LobbyState, LobbySwingPlan, ScoreboardSessionSnapshot,
};
use minecraft_packets::login::Property;
use minecraft_packets::play::boss_bar_packet::{BossBarColor, BossBarDivision};
use minecraft_protocol::prelude::{BinaryReaderError, Dimension, ProtocolVersion};
pub use navigation::{LobbyDestination, NavigationError};
use net::raw_packet::RawPacket;
use pico_precomputed_registries::PrecomputedRegistries;
use pico_structures::prelude::{Schematic, SchematicError, World, WorldLoadingError};
use pico_text_component::prelude::{Component, MiniMessageError, parse_mini_message};
pub use selector::{
    LobbyFiller, LobbySelector, LobbyVisibilityToggle, MENU_SIZE, OpenSelectorState, SelectorClick,
    build_selector_menu,
};
pub use server_commands::{ServerCommand, ServerCommands};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::num::TryFromIntError;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::debug;

mod legacy_items;
mod lobby;
mod navigation;
mod selector;
mod server_commands;

#[derive(Clone)]
pub struct CompressionSettings {
    pub threshold: usize,
    pub level: u32,
}

#[derive(PartialEq, Eq, Default)]
pub enum ForwardingMode {
    #[default]
    Disabled,
    Legacy,
    BungeeGuard {
        tokens: Vec<String>,
    },
    Modern {
        secret: Vec<u8>,
    },
}

#[derive(Debug, Error)]
#[error("secret key not set")]
pub struct MisconfiguredForwardingError;

#[derive(Default)]
pub struct Boundaries {
    pub min_y: i32,
    pub teleport_message: Option<String>,
}

#[derive(Default)]
pub struct TabList {
    pub header: String,
    pub footer: String,
}

pub struct BossBar {
    pub title: String,
    pub health: f32,
    pub color: BossBarColor,
    pub division: BossBarDivision,
}

pub enum TitleType {
    Title(String),
    Subtitle(String),
    Both { title: String, subtitle: String },
}

pub struct Title {
    pub content: TitleType,
    pub fade_in: i32,
    pub stay: i32,
    pub fade_out: i32,
}

#[derive(Clone)]
pub struct Scoreboard {
    pub title_template: String,
    pub line_templates: Vec<String>,
    pub update_interval: Duration,
}

#[derive(Clone, PartialEq)]
pub struct RenderedScoreboard {
    pub title: Component,
    pub lines: Vec<Component>,
}

pub struct ScoreboardPlaceholders<'a> {
    pub player: &'a str,
    pub online: u32,
    pub max_players: u32,
    pub server: &'a str,
}

pub type ConfigPlaceholders<'a> = ScoreboardPlaceholders<'a>;

const DEFAULT_PLACEHOLDER_PLAYER: &str = "Player";
const DEFAULT_PLACEHOLDER_ONLINE: u32 = 1;
const DEFAULT_PLACEHOLDER_MAX_PLAYERS: u32 = 20;
const DEFAULT_PLACEHOLDER_SERVER: &str = "lobby";

impl Scoreboard {
    const MAX_LINES: usize = 15;
    const OBJECTIVE_NAME: &'static str = "picolobby";

    pub const fn objective_name() -> &'static str {
        Self::OBJECTIVE_NAME
    }

    pub const fn update_interval(&self) -> Duration {
        self.update_interval
    }

    pub fn new(config: ScoreboardConfig) -> Result<Self, ServerStateBuilderError> {
        validate_scoreboard_identifier(Self::OBJECTIVE_NAME)?;
        if config.lines.len() > Self::MAX_LINES {
            return Err(ServerStateBuilderError::TooManyScoreboardLines {
                count: config.lines.len(),
                max: Self::MAX_LINES,
            });
        }

        parse_scoreboard_template(&config.title)?;
        for line in &config.lines {
            parse_scoreboard_template(line)?;
        }

        Ok(Self {
            title_template: config.title,
            line_templates: config.lines,
            update_interval: Duration::from_millis(config.update_interval_ms.max(50)),
        })
    }

    pub fn render(
        &self,
        placeholders: &ScoreboardPlaceholders<'_>,
    ) -> Result<RenderedScoreboard, MiniMessageError> {
        let (title, line_strings) = self.render_strings(placeholders);
        let title = parse_mini_message(&title)?;
        let lines = line_strings
            .iter()
            .map(|line| parse_mini_message(line))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(RenderedScoreboard { title, lines })
    }

    pub fn render_strings(
        &self,
        placeholders: &ScoreboardPlaceholders<'_>,
    ) -> (String, Vec<String>) {
        (
            render_scoreboard_template(&self.title_template, placeholders),
            self.line_templates
                .iter()
                .map(|line| render_scoreboard_template(line, placeholders))
                .collect(),
        )
    }
}

#[derive(Clone)]
pub struct ChatAntispamSettings {
    pub enabled: bool,
    pub chat_cooldown: Duration,
    pub message: String,
}

impl Default for ChatAntispamSettings {
    fn default() -> Self {
        AntispamConfig::default().into()
    }
}

impl From<AntispamConfig> for ChatAntispamSettings {
    fn from(config: AntispamConfig) -> Self {
        Self {
            enabled: config.enabled,
            chat_cooldown: Duration::from_millis(config.chat_cooldown_ms),
            message: config.message,
        }
    }
}

#[derive(Clone)]
pub struct PrivateMessageSettings {
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

impl Default for PrivateMessageSettings {
    fn default() -> Self {
        PrivateMessagesConfig::default().into()
    }
}

impl From<PrivateMessagesConfig> for PrivateMessageSettings {
    fn from(config: PrivateMessagesConfig) -> Self {
        Self {
            sender_format: config.sender_format,
            recipient_format: config.recipient_format,
            unknown_target: config.unknown_target,
            ambiguous_target: config.ambiguous_target,
            hidden_target: config.hidden_target,
            missing_reply_target: config.missing_reply_target,
            self_message: config.self_message,
            empty_message: config.empty_message,
            too_long: config.too_long,
            rate_limit: config.rate_limit,
            unavailable: config.unavailable,
        }
    }
}

#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ServerState {
    forwarding_mode: ForwardingMode,
    spawn_dimension: Dimension,
    motd: String,
    time_world: i64,
    lock_time: bool,
    max_players: u32,
    welcome_message: Option<String>,
    chat_antispam: ChatAntispamSettings,
    connected_clients: Arc<AtomicU32>,
    lobby_enabled: bool,
    lobby_chat_format: String,
    lobby_private_messages: PrivateMessageSettings,
    lobby_join_message: Option<String>,
    lobby_leave_message: Option<String>,
    lobby_destinations: Vec<LobbyDestination>,
    lobby_destination_index: HashMap<String, usize>,
    lobby_selector: Option<LobbySelector>,
    lobby_visibility_toggle: Option<LobbyVisibilityToggle>,
    lobby_state: Arc<Mutex<LobbyState>>,
    lobby_broadcast_senders: Arc<RwLock<HashMap<LobbySessionId, mpsc::Sender<RawPacket>>>>,
    show_online_player_count: bool,
    game_mode: GameMode,
    hardcore: bool,
    spawn_position: (f64, f64, f64),
    spawn_rotation: (f32, f32),
    view_distance: i32,
    world: Option<Arc<World>>,
    chunk_packet_cache: Arc<ChunkPacketCache>,
    boundaries: Option<Boundaries>,
    tab_list: Option<TabList>,
    fetch_player_skins: bool,
    boss_bar: Option<BossBar>,
    fav_icon: Option<String>,
    compression_settings: Option<CompressionSettings>,
    title: Option<Title>,
    scoreboard: Option<Scoreboard>,
    action_bar: Option<String>,
    reduced_debug_info: bool,
    is_player_listed: bool,
    reply_to_status: bool,
    accept_transfers: bool,
    allow_unsupported_versions: bool,
    allow_flight: bool,
    server_commands: ServerCommands,
}

impl ServerState {
    /// Start building a new `ServerState`.
    pub fn builder() -> ServerStateBuilder {
        ServerStateBuilder::default()
    }

    pub const fn is_legacy_forwarding(&self) -> bool {
        matches!(self.forwarding_mode, ForwardingMode::Legacy)
    }

    pub const fn is_modern_forwarding(&self) -> bool {
        matches!(self.forwarding_mode, ForwardingMode::Modern { .. })
    }

    pub fn secret_key(&self) -> Result<Vec<u8>, MisconfiguredForwardingError> {
        match &self.forwarding_mode {
            ForwardingMode::Modern { secret } => Ok(secret.clone()),
            _ => Err(MisconfiguredForwardingError),
        }
    }

    pub const fn is_bungee_guard_forwarding(&self) -> bool {
        matches!(self.forwarding_mode, ForwardingMode::BungeeGuard { .. })
    }

    pub fn tokens(&self) -> Result<Vec<String>, MisconfiguredForwardingError> {
        match &self.forwarding_mode {
            ForwardingMode::BungeeGuard { tokens } => Ok(tokens.clone()),
            _ => Err(MisconfiguredForwardingError),
        }
    }

    pub fn motd(&self) -> Result<Component, MiniMessageError> {
        Self::render_config_component(&self.motd, &self.config_placeholders(""))
    }

    pub const fn max_players(&self) -> u32 {
        self.max_players
    }

    pub fn welcome_message(
        &self,
        placeholders: &ConfigPlaceholders<'_>,
    ) -> Result<Option<Component>, MiniMessageError> {
        Self::render_optional_config_component(self.welcome_message.as_deref(), placeholders)
    }

    /// Returns the current number of connected clients.
    pub fn online_players(&self) -> u32 {
        if !self.show_online_player_count {
            return 0;
        }

        if self.lobby_enabled {
            u32::try_from(self.lobby_state().len()).unwrap_or(u32::MAX)
        } else {
            self.connected_clients.load(Ordering::Relaxed)
        }
    }

    pub const fn spawn_dimension(&self) -> Dimension {
        self.spawn_dimension
    }

    pub const fn reduced_debug_info(&self) -> bool {
        self.reduced_debug_info
    }

    pub const fn game_mode(&self) -> GameMode {
        self.game_mode
    }

    pub const fn is_hardcore(&self) -> bool {
        self.hardcore
    }

    pub const fn spawn_position(&self) -> (f64, f64, f64) {
        self.spawn_position
    }

    pub const fn spawn_rotation(&self) -> (f32, f32) {
        self.spawn_rotation
    }

    pub const fn view_distance(&self) -> i32 {
        self.view_distance
    }

    pub fn world(&self) -> Option<Arc<World>> {
        self.world.clone()
    }

    pub fn chunk_packet_cache(&self) -> Arc<ChunkPacketCache> {
        Arc::clone(&self.chunk_packet_cache)
    }

    pub const fn time_world_ticks(&self) -> i64 {
        self.time_world
    }

    pub const fn is_time_locked(&self) -> bool {
        self.lock_time
    }

    pub const fn boundaries(&self) -> Option<&Boundaries> {
        self.boundaries.as_ref()
    }

    pub const fn tab_list(&self) -> Option<&TabList> {
        self.tab_list.as_ref()
    }

    pub const fn fetch_player_skins(&self) -> bool {
        self.fetch_player_skins
    }

    pub const fn boss_bar(&self) -> Option<&BossBar> {
        self.boss_bar.as_ref()
    }

    pub fn fav_icon(&self) -> Option<String> {
        self.fav_icon.clone()
    }

    pub const fn compression_settings(&self) -> Option<&CompressionSettings> {
        self.compression_settings.as_ref()
    }

    pub const fn title(&self) -> Option<&Title> {
        self.title.as_ref()
    }

    pub const fn scoreboard(&self) -> Option<&Scoreboard> {
        self.scoreboard.as_ref()
    }

    pub fn action_bar(
        &self,
        placeholders: &ConfigPlaceholders<'_>,
    ) -> Result<Option<Component>, MiniMessageError> {
        Self::render_optional_config_component(self.action_bar.as_deref(), placeholders)
    }

    pub fn config_placeholders<'a>(&self, player: &'a str) -> ConfigPlaceholders<'a> {
        ConfigPlaceholders {
            player,
            online: self.online_players(),
            max_players: self.max_players(),
            server: DEFAULT_PLACEHOLDER_SERVER,
        }
    }

    pub fn render_config_component(
        content: &str,
        placeholders: &ConfigPlaceholders<'_>,
    ) -> Result<Component, MiniMessageError> {
        parse_mini_message(&render_config_template(content, placeholders))
    }

    pub fn render_optional_config_component(
        content: Option<&str>,
        placeholders: &ConfigPlaceholders<'_>,
    ) -> Result<Option<Component>, MiniMessageError> {
        content
            .map(|content| Self::render_config_component(content, placeholders))
            .transpose()
    }

    pub const fn is_player_listed(&self) -> bool {
        self.is_player_listed
    }

    pub const fn reply_to_status(&self) -> bool {
        self.reply_to_status
    }

    pub const fn allow_unsupported_versions(&self) -> bool {
        self.allow_unsupported_versions
    }

    pub const fn allow_flight(&self) -> bool {
        self.allow_flight
    }

    pub const fn accept_transfers(&self) -> bool {
        self.accept_transfers
    }

    pub const fn server_commands(&self) -> &ServerCommands {
        &self.server_commands
    }

    pub const fn chat_antispam(&self) -> &ChatAntispamSettings {
        &self.chat_antispam
    }

    pub const fn private_message_settings(&self) -> &PrivateMessageSettings {
        &self.lobby_private_messages
    }

    pub const fn lobby_enabled(&self) -> bool {
        self.lobby_enabled
    }

    pub const fn lobby_selector(&self) -> Option<&LobbySelector> {
        self.lobby_selector.as_ref()
    }

    pub const fn lobby_visibility_toggle(&self) -> Option<&LobbyVisibilityToggle> {
        self.lobby_visibility_toggle.as_ref()
    }

    pub fn lobby_destinations(&self) -> &[LobbyDestination] {
        &self.lobby_destinations
    }

    pub fn resolve_lobby_destination(
        &self,
        id: &str,
    ) -> Result<&LobbyDestination, NavigationError> {
        self.lobby_destination_index
            .get(id)
            .and_then(|&index| self.lobby_destinations.get(index))
            .ok_or_else(|| NavigationError::UnknownDestination(id.to_string()))
    }

    pub fn plan_lobby_npc_spawn(&self) -> Option<LobbyNpcSpawnPlan> {
        if !self.lobby_enabled {
            return None;
        }
        Some(self.lobby_state().plan_npc_spawn())
    }

    pub fn plan_lobby_npc_interaction(
        &self,
        client_state: &ClientState,
        target_entity_id: i32,
        max_distance: f64,
    ) -> Option<LobbyNpcInteraction> {
        if !self.lobby_enabled {
            return None;
        }

        let (x, y, z) = client_state.position();
        let (yaw, pitch) = client_state.rotation();
        self.lobby_state().plan_npc_interaction(
            EntityId::new(target_entity_id),
            LobbyPosition::new(x, y, z, yaw, pitch),
            max_distance,
        )
    }

    pub fn register_lobby_session(&self, client_state: &mut ClientState) -> Option<LobbySession> {
        if !self.lobby_enabled {
            // Use entity id 1 rather than 0 in the limbo-fallback path: some clients
            // treat entity id 0 specially, so the player's own entity is given a
            // non-zero id (matching the lobby allocator's first player id).
            client_state.set_entity_id(1);
            client_state.clear_lobby_session_id();
            return None;
        }
        if self.max_players > 0 && self.online_players() >= self.max_players {
            return None;
        }

        let (x, y, z) = client_state.position();
        let (yaw, pitch) = client_state.rotation();
        let session = LobbySession::new(
            client_state.get_unique_id(),
            client_state.get_username(),
            client_state.get_textures(),
            client_state.protocol_version(),
            LobbyPosition::new(x, y, z, yaw, pitch),
        );
        let mut session = session;
        session.chat_visibility = client_state.chat_visibility();
        session.players_visible = client_state.players_visible();
        let session = self.lobby_state().insert(session);
        client_state.set_entity_id(session.entity_id.get());
        client_state.set_lobby_session_id(session.session_id);
        Some(session)
    }

    pub fn unregister_lobby_session_with_leave_plan(
        &self,
        session_id: Option<LobbySessionId>,
    ) -> Option<LobbyLeavePlan> {
        if !self.lobby_enabled {
            return None;
        }

        let session_id = session_id?;
        let plan = self
            .lobby_state()
            .remove_by_session_id_with_leave_plan(session_id);
        self.remove_lobby_broadcast_sender(session_id);
        plan
    }

    #[allow(dead_code)]
    pub fn unregister_lobby_session_by_entity_id(&self, entity_id: i32) -> Option<LobbySession> {
        if !self.lobby_enabled {
            return None;
        }

        self.lobby_state()
            .remove_by_entity_id(EntityId::new(entity_id))
    }

    pub fn update_lobby_position_with_movement_plan(
        &self,
        client_state: &ClientState,
    ) -> Option<LobbyMovementPlan> {
        if !self.lobby_enabled {
            return None;
        }

        let (x, y, z) = client_state.position();
        let (yaw, pitch) = client_state.rotation();
        self.lobby_state().update_position_with_movement_plan(
            EntityId::new(client_state.entity_id()),
            LobbyPosition::new(x, y, z, yaw, pitch),
        )
    }

    pub fn update_lobby_crouching_with_metadata_plan(
        &self,
        client_state: &ClientState,
        crouching: bool,
    ) -> Option<LobbyMetadataPlan> {
        if !self.lobby_enabled {
            return None;
        }

        self.lobby_state()
            .update_crouching_with_metadata_plan(EntityId::new(client_state.entity_id()), crouching)
    }

    pub fn plan_lobby_swing_broadcast(&self, client_state: &ClientState) -> Option<LobbySwingPlan> {
        if !self.lobby_enabled {
            return None;
        }

        self.lobby_state()
            .plan_swing_broadcast(EntityId::new(client_state.entity_id()))
    }

    pub fn update_lobby_chat_visibility(&self, client_state: &ClientState) -> bool {
        if !self.lobby_enabled {
            return false;
        }

        let Some(session_id) = client_state.lobby_session_id() else {
            return false;
        };

        self.lobby_state()
            .update_chat_visibility(session_id, client_state.chat_visibility())
    }

    pub fn update_lobby_players_visible(&self, client_state: &ClientState) -> bool {
        if !self.lobby_enabled {
            return false;
        }

        let Some(session_id) = client_state.lobby_session_id() else {
            return false;
        };

        self.lobby_state()
            .update_players_visible(session_id, client_state.players_visible())
    }

    pub fn plan_lobby_chat_broadcast(
        &self,
        client_state: &ClientState,
        message: impl Into<String>,
    ) -> Option<LobbyChatPlan> {
        if !self.lobby_enabled {
            return None;
        }

        let mut plan = self
            .lobby_state()
            .plan_chat_broadcast(client_state.lobby_session_id()?, message)?;
        plan.format.clone_from(&self.lobby_chat_format);
        Some(plan)
    }

    pub fn plan_lobby_private_message(
        &self,
        client_state: &ClientState,
        target: &str,
        message: impl Into<String>,
    ) -> Result<LobbyPrivateMessagePlan, LobbyPrivateMessageError> {
        if !self.lobby_enabled {
            return Err(LobbyPrivateMessageError::Unavailable);
        }

        let settings = &self.lobby_private_messages;
        self.lobby_state().plan_private_message(
            client_state
                .lobby_session_id()
                .ok_or(LobbyPrivateMessageError::Unavailable)?,
            target,
            message,
            settings.sender_format.clone(),
            settings.recipient_format.clone(),
        )
    }

    pub fn plan_lobby_reply_message(
        &self,
        client_state: &ClientState,
        message: impl Into<String>,
    ) -> Result<LobbyPrivateMessagePlan, LobbyPrivateMessageError> {
        if !self.lobby_enabled {
            return Err(LobbyPrivateMessageError::Unavailable);
        }

        let settings = &self.lobby_private_messages;
        self.lobby_state().plan_reply_message(
            client_state
                .lobby_session_id()
                .ok_or(LobbyPrivateMessageError::Unavailable)?,
            message,
            settings.sender_format.clone(),
            settings.recipient_format.clone(),
        )
    }

    pub fn validate_lobby_reply_target(
        &self,
        client_state: &ClientState,
    ) -> Result<(), LobbyPrivateMessageError> {
        if !self.lobby_enabled {
            return Err(LobbyPrivateMessageError::Unavailable);
        }
        self.lobby_state().validate_reply_target(
            client_state
                .lobby_session_id()
                .ok_or(LobbyPrivateMessageError::Unavailable)?,
        )
    }

    pub fn plan_lobby_join_message(
        &self,
        session_id: LobbySessionId,
    ) -> Option<LobbyLifecycleMessagePlan> {
        if !self.lobby_enabled {
            return None;
        }
        self.lobby_state()
            .plan_lifecycle_message(session_id, self.lobby_join_message.clone()?)
    }

    pub fn plan_lobby_leave_message(
        &self,
        leave_plan: &LobbyLeavePlan,
    ) -> Option<LobbyLifecycleMessagePlan> {
        if !self.lobby_enabled {
            return None;
        }
        Some(LobbyLifecycleMessagePlan {
            player_username: leave_plan.departed_username.clone(),
            template: self.lobby_leave_message.clone()?,
            recipients: leave_plan.lifecycle_message_recipients.clone(),
        })
    }

    pub fn plan_lobby_join(&self, session_id: LobbySessionId) -> Option<LobbyJoinPlan> {
        if !self.lobby_enabled {
            return None;
        }
        self.lobby_state().plan_join_visibility(session_id)
    }

    /// Returns the join-visibility plan for the toggling player (existing sessions
    /// are all other currently online players).  Used by the visibility toggle to
    /// know which entities to spawn or despawn for just this client.
    pub fn collect_sessions_for_visibility_toggle(
        &self,
        client_state: &crate::server::client_state::ClientState,
    ) -> Option<LobbyJoinPlan> {
        if !self.lobby_enabled {
            return None;
        }
        let session_id = client_state.lobby_session_id()?;
        self.lobby_state().plan_join_visibility(session_id)
    }

    pub fn set_lobby_broadcast_sender(
        &self,
        session_id: LobbySessionId,
        sender: mpsc::Sender<RawPacket>,
    ) {
        if !self.lobby_enabled {
            return;
        }
        self.lobby_broadcast_senders
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(session_id, sender);
    }

    pub fn remove_lobby_broadcast_sender(&self, session_id: LobbySessionId) {
        if !self.lobby_enabled {
            return;
        }
        self.lobby_broadcast_senders
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&session_id);
    }

    pub fn lobby_scoreboard_sessions(&self) -> Vec<ScoreboardSessionSnapshot> {
        if !self.lobby_enabled {
            return Vec::new();
        }
        self.lobby_state().scoreboard_sessions()
    }

    pub fn collect_lobby_broadcast_senders(
        &self,
        recipients: &[LobbyRecipient],
    ) -> HashMap<LobbySessionId, mpsc::Sender<RawPacket>> {
        if !self.lobby_enabled {
            return HashMap::new();
        }
        let senders = self
            .lobby_broadcast_senders
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut map = HashMap::with_capacity(recipients.len());
        for r in recipients {
            if let Some(sender) = senders.get(&r.session_id) {
                map.insert(r.session_id, sender.clone());
            }
        }
        map
    }

    /// Like [`Self::collect_lobby_broadcast_senders`] but groups senders by
    /// protocol version in a single pass. A broadcast can then encode each packet
    /// once per version and fan it out, without first building a session-keyed map
    /// and then re-bucketing it — saving an allocation per broadcast on the hot
    /// movement/swing/metadata/chat path.
    pub fn bucket_lobby_broadcast_senders_by_version(
        &self,
        recipients: &[LobbyRecipient],
    ) -> HashMap<ProtocolVersion, Vec<mpsc::Sender<RawPacket>>> {
        if !self.lobby_enabled {
            return HashMap::new();
        }
        let senders = self
            .lobby_broadcast_senders
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut buckets: HashMap<ProtocolVersion, Vec<mpsc::Sender<RawPacket>>> = HashMap::new();
        for r in recipients {
            if let Some(sender) = senders.get(&r.session_id) {
                buckets
                    .entry(r.protocol_version)
                    .or_default()
                    .push(sender.clone());
            }
        }
        buckets
    }

    #[allow(dead_code)]
    pub fn plan_lobby_recipients(
        &self,
        exclude_session_id: Option<LobbySessionId>,
    ) -> Vec<LobbyRecipient> {
        if !self.lobby_enabled {
            return Vec::new();
        }

        self.lobby_state().plan_recipients(exclude_session_id)
    }

    fn lobby_state(&self) -> MutexGuard<'_, LobbyState> {
        self.lobby_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn increment(&self) {
        self.connected_clients.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement(&self) {
        self.connected_clients.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ServerStateBuilder {
    forwarding_mode: ForwardingMode,
    dimension: Option<Dimension>,
    time_world: i64,
    lock_time: bool,
    description_text: String,
    max_players: u32,
    welcome_message: String,
    chat_antispam: ChatAntispamSettings,
    lobby_enabled: bool,
    lobby_chat_format: String,
    lobby_private_messages: PrivateMessageSettings,
    lobby_join_message: String,
    lobby_leave_message: String,
    lobby_destinations: Vec<LobbyDestination>,
    lobby_npcs: Vec<LobbyNpc>,
    lobby_selector: Option<LobbySelector>,
    lobby_visibility_toggle: Option<LobbyVisibilityToggle>,
    show_online_player_count: bool,
    game_mode: GameMode,
    hardcore: bool,
    spawn_position: (f64, f64, f64),
    spawn_rotation: (f32, f32),
    view_distance: i32,
    schematic_file_path: String,
    boundaries: Option<Boundaries>,
    tab_list: Option<TabList>,
    fetch_player_skins: bool,
    boss_bar: Option<BossBar>,
    fav_icon: Option<String>,
    compression_settings: Option<CompressionSettings>,
    title: Option<Title>,
    scoreboard: Option<Scoreboard>,
    action_bar: Option<String>,
    reduced_debug_info: bool,
    is_player_listed: bool,
    reply_to_status: bool,
    allow_unsupported_versions: bool,
    allow_flight: bool,
    accept_transfers: bool,
    server_commands: ServerCommands,
}

#[derive(Debug, Error)]
pub enum ServerStateBuilderError {
    #[error(transparent)]
    SchematicLoadingFailed(#[from] SchematicError),
    #[error(transparent)]
    BinaryReader(#[from] BinaryReaderError),
    #[error(transparent)]
    WorldLoading(#[from] WorldLoadingError),
    #[error(transparent)]
    MiniMessage(#[from] MiniMessageError),
    #[error("the configured spawn position Y is below the configured minimum Y position")]
    InvalidSpawnPosition,
    #[error(
        "lobby server '{0}' has an empty 'server' name; every [[lobby.servers]] entry must specify a Velocity server name"
    )]
    EmptyServerName(String),
    #[error("lobby NPC '{npc_id}' references unknown destination '{destination_id}'")]
    UnknownNpcDestination {
        npc_id: String,
        destination_id: String,
    },
    #[error(transparent)]
    LobbyNpcValidation(#[from] LobbyNpcValidationError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    TryFromInt(#[from] TryFromIntError),
    #[error("scoreboard has {count} lines but the sidebar supports at most {max}")]
    TooManyScoreboardLines { count: usize, max: usize },
    #[error(
        "lobby selector and visibility toggle are configured on the same hotbar slot ({slot}); they must use different slots"
    )]
    VisibilityToggleSlotConflict { slot: u8 },
    #[error("scoreboard objective name '{0}' is longer than 16 characters")]
    InvalidScoreboardObjectiveName(String),
    #[error("lobby destination id cannot be empty")]
    EmptyDestinationId,
    #[error("lobby destination '{0}' is defined more than once")]
    DuplicateDestinationId(String),
    #[error("lobby destination '{0}' has an empty display name")]
    EmptyDestinationDisplayName(String),
    #[error("lobby {item_kind} slot {slot} is outside the hotbar range 0..=8")]
    InvalidHotbarSlot { item_kind: &'static str, slot: u8 },
    #[error("lobby {item_kind} item '{item}' is not known to the current item registry")]
    UnknownLobbyItem {
        item_kind: &'static str,
        item: String,
    },
    #[error(
        "lobby selector entry '{destination}' uses slot {slot}, which is outside the menu range 0..=26"
    )]
    InvalidSelectorSlot { destination: String, slot: usize },
    #[error("more than one lobby selector entry is assigned to menu slot {slot}")]
    DuplicateSelectorSlot { slot: usize },
}

impl ServerStateBuilder {
    pub fn enable_legacy_forwarding(&mut self) -> &mut Self {
        self.forwarding_mode = ForwardingMode::Legacy;
        self
    }

    pub fn enable_bungee_guard_forwarding(&mut self, tokens: Vec<String>) -> &mut Self {
        self.forwarding_mode = ForwardingMode::BungeeGuard { tokens };
        self
    }

    pub fn enable_modern_forwarding<K>(&mut self, key: K) -> &mut Self
    where
        K: Into<Vec<u8>>,
    {
        self.forwarding_mode = ForwardingMode::Modern { secret: key.into() };
        self
    }

    pub fn disable_forwarding(&mut self) -> &mut Self {
        self.forwarding_mode = ForwardingMode::Disabled;
        self
    }

    /// Set the spawn dimension
    pub const fn dimension(&mut self, dimension: Dimension) -> &mut Self {
        self.dimension = Some(dimension);
        self
    }

    /// Set the time of the world
    pub const fn time_world(&mut self, time_world: i64) -> &mut Self {
        self.time_world = time_world;
        self
    }

    pub const fn lock_time(&mut self, lock_time: bool) -> &mut Self {
        self.lock_time = lock_time;
        self
    }

    pub fn description_text<S>(&mut self, text: S) -> &mut Self
    where
        S: Into<String>,
    {
        self.description_text = text.into();
        self
    }

    pub const fn max_players(&mut self, max_players: u32) -> &mut Self {
        self.max_players = max_players;
        self
    }

    pub fn welcome_message<S>(&mut self, message: S) -> &mut Self
    where
        S: Into<String>,
    {
        self.welcome_message = message.into();
        self
    }

    pub fn antispam(&mut self, config: AntispamConfig) -> &mut Self {
        self.chat_antispam = config.into();
        self
    }

    pub fn action_bar<S>(&mut self, message: S) -> Result<&mut Self, ServerStateBuilderError>
    where
        S: AsRef<str>,
    {
        self.action_bar = optional_config_template(message.as_ref())?;
        Ok(self)
    }

    pub const fn show_online_player_count(&mut self, show: bool) -> &mut Self {
        self.show_online_player_count = show;
        self
    }

    pub const fn set_lobby_enabled(&mut self, enabled: bool) -> &mut Self {
        self.lobby_enabled = enabled;
        self
    }

    pub fn set_lobby_chat_format<S: Into<String>>(&mut self, format: S) -> &mut Self {
        self.lobby_chat_format = format.into();
        self
    }

    pub fn set_lobby_private_messages(&mut self, config: PrivateMessagesConfig) -> &mut Self {
        self.lobby_private_messages = config.into();
        self
    }

    pub fn set_lobby_join_message<S: Into<String>>(&mut self, message: S) -> &mut Self {
        self.lobby_join_message = message.into();
        self
    }

    pub fn set_lobby_leave_message<S: Into<String>>(&mut self, message: S) -> &mut Self {
        self.lobby_leave_message = message.into();
        self
    }

    /// Sets the hotbar selector item from config.  Parses `MiniMessage` strings
    /// and pre-computes per-version item IDs.  Silently ignores `None`.
    pub fn set_lobby_selector(
        &mut self,
        config: Option<SelectorItemConfig>,
    ) -> Result<&mut Self, ServerStateBuilderError> {
        if let Some(cfg) = config {
            validate_hotbar_slot("selector", cfg.slot)?;
            let filler = cfg
                .filler
                .map(|f| {
                    let filler = LobbyFiller::new(&f.item, f.display_name.as_deref(), &f.lore)?;
                    validate_lobby_item("selector filler", &filler.item_identifier)?;
                    Ok::<_, ServerStateBuilderError>(filler)
                })
                .transpose()?;
            let selector =
                LobbySelector::new(cfg.slot, &cfg.item, cfg.display_name.as_deref(), &cfg.lore)?
                    .with_filler(filler);
            validate_lobby_item("selector", &selector.item_identifier)?;
            self.lobby_selector = Some(selector);
        }
        Ok(self)
    }

    /// Sets the hotbar visibility toggle item from config.  Parses `MiniMessage`
    /// strings and pre-computes per-version item IDs.  Silently ignores `None`.
    pub fn set_lobby_visibility_toggle(
        &mut self,
        config: Option<VisibilityToggleConfig>,
    ) -> Result<&mut Self, ServerStateBuilderError> {
        if let Some(cfg) = config {
            validate_hotbar_slot("visibility toggle", cfg.slot)?;
            let toggle = LobbyVisibilityToggle::new(cfg)?;
            validate_lobby_item("visibility toggle", &toggle.item_identifier)?;
            self.lobby_visibility_toggle = Some(toggle);
        }
        Ok(self)
    }

    pub fn set_lobby_destinations(
        &mut self,
        destinations: Vec<LobbyDestination>,
    ) -> Result<&mut Self, ServerStateBuilderError> {
        let mut ids = HashSet::new();
        let mut used_slots = HashSet::new();
        for dest in &destinations {
            let id = dest.id.as_str();
            if id.trim().is_empty() {
                return Err(ServerStateBuilderError::EmptyDestinationId);
            }
            if !ids.insert(id.to_string()) {
                return Err(ServerStateBuilderError::DuplicateDestinationId(
                    id.to_string(),
                ));
            }
            if dest.display_name.trim().is_empty() {
                return Err(ServerStateBuilderError::EmptyDestinationDisplayName(
                    id.to_string(),
                ));
            }
            if dest.server.trim().is_empty() {
                return Err(ServerStateBuilderError::EmptyServerName(id.to_string()));
            }
            validate_lobby_item("selector entry", &dest.item)?;
            if let Some(slot) = dest.slot {
                if slot >= MENU_SIZE {
                    return Err(ServerStateBuilderError::InvalidSelectorSlot {
                        destination: id.to_string(),
                        slot,
                    });
                }
                if !used_slots.insert(slot) {
                    return Err(ServerStateBuilderError::DuplicateSelectorSlot { slot });
                }
            }
        }
        self.lobby_destinations = destinations;
        Ok(self)
    }

    pub fn set_lobby_npcs(
        &mut self,
        configs: Vec<LobbyNpcConfig>,
        mut skins: HashMap<String, Option<Property>>,
    ) -> Result<&mut Self, ServerStateBuilderError> {
        let npcs = configs
            .into_iter()
            .map(|cfg| {
                let textures = skins.remove(&cfg.id).flatten();
                LobbyNpc::player(
                    cfg.id,
                    cfg.destination,
                    cfg.name,
                    LobbyPosition::new(cfg.x, cfg.y, cfg.z, cfg.yaw, cfg.pitch),
                )
                .with_textures(textures)
                .with_tab_list_remove_delay(
                    (cfg.tab_list_remove_delay_ms > 0)
                        .then(|| Duration::from_millis(cfg.tab_list_remove_delay_ms)),
                )
            })
            .collect::<Vec<_>>();
        LobbyState::validate_npcs(&npcs)?;
        self.lobby_npcs = npcs;
        Ok(self)
    }

    pub const fn game_mode(&mut self, game_mode: GameMode) -> &mut Self {
        self.game_mode = game_mode;
        self
    }

    pub const fn reduced_debug_info(&mut self, reduced_debug_info: bool) -> &mut Self {
        self.reduced_debug_info = reduced_debug_info;
        self
    }

    pub const fn set_player_listed(&mut self, is_player_listed: bool) -> &mut Self {
        self.is_player_listed = is_player_listed;
        self
    }

    pub const fn set_reply_to_status(&mut self, reply_to_status: bool) -> &mut Self {
        self.reply_to_status = reply_to_status;
        self
    }

    pub const fn set_allow_unsupported_versions(
        &mut self,
        allow_unsupported_versions: bool,
    ) -> &mut Self {
        self.allow_unsupported_versions = allow_unsupported_versions;
        self
    }

    pub const fn set_allow_flight(&mut self, allow_flight: bool) -> &mut Self {
        self.allow_flight = allow_flight;
        self
    }

    pub const fn set_accept_transfers(&mut self, accept_transfers: bool) -> &mut Self {
        self.accept_transfers = accept_transfers;
        self
    }

    pub const fn hardcore(&mut self, hardcore: bool) -> &mut Self {
        self.hardcore = hardcore;
        self
    }

    pub const fn spawn_position(&mut self, position: (f64, f64, f64)) -> &mut Self {
        self.spawn_position = position;
        self
    }

    pub const fn spawn_rotation(&mut self, rotation: (f32, f32)) -> &mut Self {
        self.spawn_rotation = rotation;
        self
    }

    pub fn view_distance(&mut self, view_distance: i32) -> &mut Self {
        self.view_distance = view_distance.max(0);
        self
    }

    pub fn schematic(&mut self, schematic_file_path: String) -> &mut Self {
        self.schematic_file_path = schematic_file_path;
        self
    }

    pub fn tab_list(
        &mut self,
        header: &str,
        footer: &str,
    ) -> Result<&mut Self, ServerStateBuilderError> {
        parse_config_template(header)?;
        parse_config_template(footer)?;
        self.tab_list = Some(TabList {
            header: header.to_string(),
            footer: footer.to_string(),
        });

        Ok(self)
    }

    pub fn boundaries<S>(
        &mut self,
        min_y: i32,
        teleport_message: S,
    ) -> Result<&mut Self, ServerStateBuilderError>
    where
        S: AsRef<str>,
    {
        let teleport_message = optional_config_template(teleport_message.as_ref())?;
        self.boundaries = Some(Boundaries {
            min_y,
            teleport_message,
        });
        Ok(self)
    }

    pub fn fav_icon<P>(&mut self, file_path: P) -> Result<&mut Self, ServerStateBuilderError>
    where
        P: AsRef<Path>,
    {
        let mut file = File::open(file_path)?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let engine = engine::GeneralPurpose::new(&alphabet::STANDARD, general_purpose::PAD);
        let base64_encoded = engine.encode(&buffer);

        self.fav_icon = Some(format!("data:image/png;base64,{base64_encoded}"));
        Ok(self)
    }

    pub const fn fetch_player_skins(&mut self, fetch_player_skins: bool) -> &mut Self {
        self.fetch_player_skins = fetch_player_skins;
        self
    }

    pub fn enable_compression(
        &mut self,
        threshold: i32,
        level: u32,
    ) -> Result<&mut Self, ServerStateBuilderError> {
        self.compression_settings = if threshold >= 0 {
            let threshold = usize::try_from(threshold)?;
            let level = level.clamp(0, 9);
            Some(CompressionSettings { threshold, level })
        } else {
            None
        };
        Ok(self)
    }

    pub fn boss_bar(
        &mut self,
        boss_bar_config: EnabledBossBarConfig,
    ) -> Result<&mut Self, ServerStateBuilderError> {
        parse_config_template(boss_bar_config.title.as_ref())?;
        self.boss_bar = Some(BossBar {
            title: boss_bar_config.title,
            health: boss_bar_config.health.clamp(0.0, 1.0),
            color: boss_bar_config.color.into(),
            division: boss_bar_config.division.into(),
        });
        Ok(self)
    }

    pub fn title(
        &mut self,
        title: &str,
        subtitle: &str,
        fade_in: i32,
        stay: i32,
        fade_out: i32,
    ) -> Result<&mut Self, ServerStateBuilderError> {
        let title_type = match (
            optional_config_template(title)?,
            optional_config_template(subtitle)?,
        ) {
            (Some(title), Some(subtitle)) => Some(TitleType::Both { title, subtitle }),
            (Some(title), None) => Some(TitleType::Title(title)),
            (None, Some(subtitle)) => Some(TitleType::Subtitle(subtitle)),
            (None, None) => None,
        };

        if let Some(title_type) = title_type {
            self.title = Some(Title {
                content: title_type,
                fade_in,
                stay,
                fade_out,
            });
        }
        Ok(self)
    }

    pub fn scoreboard(
        &mut self,
        config: ScoreboardConfig,
        lobby_enabled: bool,
    ) -> Result<&mut Self, ServerStateBuilderError> {
        if config.enabled.should_send(lobby_enabled) {
            self.scoreboard = Some(Scoreboard::new(config)?);
        }
        Ok(self)
    }

    pub fn server_commands(&mut self, commands_config: CommandsConfig) -> &mut Self {
        self.server_commands = commands_config.into();
        self
    }

    /// Finish building, returning an error if any required fields are missing.
    pub fn build(self) -> Result<ServerState, ServerStateBuilderError> {
        let world = if self.schematic_file_path.is_empty() {
            None
        } else {
            let schematic = time_operation("Loading schematic", || {
                let internal_mapping = blocks_report::load_internal_mapping()?;
                let schematic_file_path = PathBuf::from(self.schematic_file_path);
                Schematic::load_schematic_file(&schematic_file_path, &internal_mapping)
            })?;
            let world = time_operation("Loading world", || World::from_schematic(&schematic))?;
            Some(Arc::new(world))
        };

        for npc in &self.lobby_npcs {
            if !self
                .lobby_destinations
                .iter()
                .any(|dest| dest.id.as_str() == npc.destination_id)
            {
                return Err(ServerStateBuilderError::UnknownNpcDestination {
                    npc_id: npc.id.as_str().to_string(),
                    destination_id: npc.destination_id.clone(),
                });
            }
        }

        if let (Some(selector), Some(toggle)) =
            (&self.lobby_selector, &self.lobby_visibility_toggle)
            && selector.hotbar_slot == toggle.hotbar_slot
        {
            return Err(ServerStateBuilderError::VisibilityToggleSlotConflict {
                slot: selector.hotbar_slot,
            });
        }

        Ok(ServerState {
            forwarding_mode: self.forwarding_mode,
            spawn_dimension: self.dimension.unwrap_or_default(),
            motd: parse_config_template(&self.description_text)?,
            time_world: self.time_world,
            lock_time: self.lock_time,
            max_players: self.max_players,
            welcome_message: optional_config_template(&self.welcome_message)?,
            chat_antispam: self.chat_antispam,
            action_bar: self.action_bar,
            connected_clients: Arc::new(AtomicU32::new(0)),
            lobby_enabled: self.lobby_enabled,
            lobby_chat_format: self.lobby_chat_format,
            lobby_private_messages: self.lobby_private_messages,
            lobby_join_message: optional_lifecycle_template(&self.lobby_join_message)?,
            lobby_leave_message: optional_lifecycle_template(&self.lobby_leave_message)?,
            lobby_destination_index: self
                .lobby_destinations
                .iter()
                .enumerate()
                .map(|(index, dest)| (dest.id.as_str().to_string(), index))
                .collect(),
            lobby_destinations: self.lobby_destinations,
            lobby_selector: self.lobby_selector,
            lobby_visibility_toggle: self.lobby_visibility_toggle,
            lobby_state: Arc::new(Mutex::new(LobbyState::with_npcs(self.lobby_npcs))),
            lobby_broadcast_senders: Arc::new(RwLock::new(HashMap::new())),
            show_online_player_count: self.show_online_player_count,
            game_mode: self.game_mode,
            hardcore: self.hardcore,
            spawn_position: self.spawn_position,
            spawn_rotation: self.spawn_rotation,
            view_distance: self.view_distance,
            world,
            chunk_packet_cache: Arc::new(ChunkPacketCache::default()),
            boundaries: self.boundaries,
            tab_list: self.tab_list,
            fetch_player_skins: self.fetch_player_skins,
            boss_bar: self.boss_bar,
            fav_icon: self.fav_icon,
            compression_settings: self.compression_settings,
            title: self.title,
            scoreboard: self.scoreboard,
            reduced_debug_info: self.reduced_debug_info,
            is_player_listed: self.is_player_listed,
            reply_to_status: self.reply_to_status,
            allow_unsupported_versions: self.allow_unsupported_versions,
            allow_flight: self.allow_flight,
            accept_transfers: self.accept_transfers,
            server_commands: self.server_commands,
        })
    }
}

fn optional_config_template(content: &str) -> Result<Option<String>, MiniMessageError> {
    let template = if content.is_empty() {
        None
    } else {
        parse_config_template(content)?;
        Some(content.to_string())
    };
    Ok(template)
}

fn optional_lifecycle_template(content: &str) -> Result<Option<String>, MiniMessageError> {
    if content.is_empty() {
        return Ok(None);
    }

    parse_mini_message(&content.replace("{player}", "Player"))?;
    Ok(Some(content.to_string()))
}

fn validate_scoreboard_identifier(value: &str) -> Result<(), ServerStateBuilderError> {
    if value.chars().count() > 16 {
        Err(ServerStateBuilderError::InvalidScoreboardObjectiveName(
            value.to_string(),
        ))
    } else {
        Ok(())
    }
}

const fn validate_hotbar_slot(
    item_kind: &'static str,
    slot: u8,
) -> Result<(), ServerStateBuilderError> {
    if slot <= 8 {
        Ok(())
    } else {
        Err(ServerStateBuilderError::InvalidHotbarSlot { item_kind, slot })
    }
}

fn validate_lobby_item(item_kind: &'static str, item: &str) -> Result<(), ServerStateBuilderError> {
    if PrecomputedRegistries::new(ProtocolVersion::V26_1)
        .resolve_item_id(item)
        .is_some()
        || legacy_items::legacy_item(item).is_some()
    {
        Ok(())
    } else {
        Err(ServerStateBuilderError::UnknownLobbyItem {
            item_kind,
            item: item.to_string(),
        })
    }
}

fn parse_scoreboard_template(content: &str) -> Result<(), MiniMessageError> {
    parse_config_template(content).map(|_| ())
}

fn parse_config_template(content: &str) -> Result<String, MiniMessageError> {
    parse_mini_message(&render_config_template(
        content,
        &ScoreboardPlaceholders {
            player: DEFAULT_PLACEHOLDER_PLAYER,
            online: DEFAULT_PLACEHOLDER_ONLINE,
            max_players: DEFAULT_PLACEHOLDER_MAX_PLAYERS,
            server: DEFAULT_PLACEHOLDER_SERVER,
        },
    ))?;
    Ok(content.to_string())
}

pub fn render_scoreboard_template(
    content: &str,
    placeholders: &ScoreboardPlaceholders<'_>,
) -> String {
    render_config_template(content, placeholders)
}

pub fn render_config_template(content: &str, placeholders: &ConfigPlaceholders<'_>) -> String {
    content
        .replace("{player}", placeholders.player)
        .replace("{online}", &placeholders.online.to_string())
        .replace("{max_players}", &placeholders.max_players.to_string())
        .replace("{server}", placeholders.server)
}

fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs_f64();

    if total_secs >= 1.0 {
        format!("{total_secs:.1}s")
    } else {
        format!("{}ms", duration.as_millis())
    }
}

fn time_operation<T, F>(operation_name: &str, operation: F) -> T
where
    F: FnOnce() -> T,
{
    debug!("{operation_name}...");
    let start = std::time::Instant::now();
    let result = operation();
    let elapsed = start.elapsed();
    debug!("Time elapsed: {}", format_duration(elapsed));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::game_profile::GameProfile;
    use minecraft_protocol::prelude::{ProtocolVersion, Uuid};

    fn client(username: &str, uuid: Uuid) -> ClientState {
        let mut client = ClientState::default();
        client.set_protocol_version(ProtocolVersion::V1_20_5);
        client.set_game_profile(GameProfile::new(username, uuid, None));
        client.set_position((1.0, 2.0, 3.0));
        client.set_rotation((90.0, 45.0));
        client
    }

    fn server_state(lobby_enabled: bool) -> ServerState {
        let mut builder = ServerState::builder();
        builder
            .set_lobby_enabled(lobby_enabled)
            .show_online_player_count(true);
        builder.build().unwrap()
    }

    #[test]
    fn empty_lifecycle_messages_are_disabled() {
        let mut builder = ServerState::builder();
        builder
            .set_lobby_enabled(true)
            .set_lobby_join_message("")
            .set_lobby_leave_message("");

        let server_state = builder.build().unwrap();

        assert!(server_state.lobby_join_message.is_none());
        assert!(server_state.lobby_leave_message.is_none());
    }

    #[test]
    fn disabled_lobby_uses_legacy_count_and_has_no_recipients() {
        let server_state = server_state(false);
        let mut client = client("first", Uuid::from_u128(1));

        assert!(server_state.register_lobby_session(&mut client).is_none());
        // The limbo-fallback path assigns entity id 1 (not 0) since some clients
        // treat entity id 0 specially.
        assert_eq!(client.entity_id(), 1);
        assert_eq!(client.lobby_session_id(), None);
        assert!(server_state.plan_lobby_recipients(None).is_empty());

        server_state.increment();
        server_state.increment();
        assert_eq!(server_state.online_players(), 2);

        server_state.decrement();
        assert_eq!(server_state.online_players(), 1);
    }

    #[test]
    fn enabled_lobby_plans_owned_recipient_snapshots() {
        let server_state = server_state(true);
        let first_uuid = Uuid::from_u128(1);
        let second_uuid = Uuid::from_u128(2);
        let mut first = client("first", first_uuid);
        let mut second = client("second", second_uuid);

        let first_session = server_state.register_lobby_session(&mut first).unwrap();
        let second_session = server_state.register_lobby_session(&mut second).unwrap();
        assert_eq!(first.lobby_session_id(), Some(first_session.session_id));
        assert_eq!(second.lobby_session_id(), Some(second_session.session_id));

        let recipients = server_state.plan_lobby_recipients(first.lobby_session_id());
        assert_eq!(
            recipients,
            vec![LobbyRecipient {
                session_id: second_session.session_id,
                uuid: second_uuid,
                entity_id: second_session.entity_id,
                protocol_version: ProtocolVersion::V1_20_5,
            }]
        );

        assert!(
            server_state
                .unregister_lobby_session_with_leave_plan(second.lobby_session_id())
                .is_some()
        );
        assert_eq!(server_state.online_players(), 1);

        assert_eq!(recipients[0].entity_id, second_session.entity_id);
        assert_eq!(
            server_state.plan_lobby_recipients(first.lobby_session_id()),
            Vec::new()
        );

        assert!(
            server_state
                .unregister_lobby_session_with_leave_plan(first.lobby_session_id())
                .is_some()
        );
        assert_eq!(server_state.online_players(), 0);
    }

    #[test]
    fn stale_lobby_session_handle_does_not_unregister_replacement() {
        let server_state = server_state(true);
        let uuid = Uuid::from_u128(1);
        let mut first = client("first", uuid);
        let mut replacement = client("replacement", uuid);

        let first_session = server_state.register_lobby_session(&mut first).unwrap();
        let replacement_session = server_state
            .register_lobby_session(&mut replacement)
            .unwrap();

        assert_ne!(first_session.session_id, replacement_session.session_id);
        assert!(
            server_state
                .unregister_lobby_session_with_leave_plan(first.lobby_session_id())
                .is_none()
        );
        assert_eq!(server_state.online_players(), 1);

        assert!(
            server_state
                .unregister_lobby_session_with_leave_plan(replacement.lobby_session_id())
                .is_some()
        );
        assert_eq!(server_state.online_players(), 0);
    }

    #[test]
    fn scoreboard_placeholders_render_known_values_and_keep_unknown_literals() {
        let rendered = render_scoreboard_template(
            "{player} {online}/{max_players} {server} {unknown}",
            &ScoreboardPlaceholders {
                player: "Steve",
                online: 3,
                max_players: 20,
                server: "lobby",
            },
        );

        assert_eq!(rendered, "Steve 3/20 lobby {unknown}");
    }

    #[test]
    fn config_placeholders_render_outside_scoreboard_templates() {
        let mut builder = ServerState::builder();
        builder
            .description_text("MOTD {online}/{max_players} {server}")
            .welcome_message("Welcome {player}")
            .action_bar("Online {online}")
            .unwrap()
            .max_players(20)
            .show_online_player_count(true)
            .tab_list("Hi {player}", "{online}/{max_players}")
            .unwrap()
            .title("Title {player}", "Server {server}", 10, 70, 20)
            .unwrap()
            .boundaries(0, "Back to spawn, {player}")
            .unwrap();
        let server_state = builder.build().unwrap();
        server_state.increment();
        server_state.increment();

        let placeholders = server_state.config_placeholders("Alex");
        let plain = |component: Component| component.to_legacy_text().replace("\u{00a7}r", "");

        assert_eq!(plain(server_state.motd().unwrap()), "MOTD 2/20 lobby");
        assert_eq!(
            plain(
                server_state
                    .welcome_message(&placeholders)
                    .unwrap()
                    .unwrap()
            ),
            "Welcome Alex"
        );
        assert_eq!(
            plain(server_state.action_bar(&placeholders).unwrap().unwrap()),
            "Online 2"
        );

        let tab_list = server_state.tab_list().unwrap();
        assert_eq!(
            plain(ServerState::render_config_component(&tab_list.header, &placeholders).unwrap()),
            "Hi Alex"
        );
        assert_eq!(
            plain(ServerState::render_config_component(&tab_list.footer, &placeholders).unwrap()),
            "2/20"
        );

        let title = server_state.title().unwrap();
        let TitleType::Both { title, subtitle } = &title.content else {
            panic!("expected title and subtitle");
        };
        assert_eq!(
            plain(ServerState::render_config_component(title, &placeholders).unwrap()),
            "Title Alex"
        );
        assert_eq!(
            plain(ServerState::render_config_component(subtitle, &placeholders).unwrap()),
            "Server lobby"
        );

        let boundaries = server_state.boundaries().unwrap();
        assert_eq!(
            plain(
                ServerState::render_config_component(
                    boundaries.teleport_message.as_deref().unwrap(),
                    &placeholders,
                )
                .unwrap()
            ),
            "Back to spawn, Alex"
        );
    }

    #[test]
    fn scoreboard_rejects_more_than_fifteen_lines() {
        let result = Scoreboard::new(ScoreboardConfig {
            lines: vec!["line".to_string(); 16],
            ..ScoreboardConfig::default()
        });

        assert!(matches!(
            result,
            Err(ServerStateBuilderError::TooManyScoreboardLines { count: 16, max: 15 })
        ));
    }

    #[test]
    fn scoreboard_rejects_invalid_mini_message_templates() {
        let result = Scoreboard::new(ScoreboardConfig {
            title: "<unknown>PicoLobby</unknown>".to_string(),
            ..ScoreboardConfig::default()
        });

        assert!(matches!(
            result,
            Err(ServerStateBuilderError::MiniMessage(_))
        ));
    }

    fn visibility_toggle_config(slot: u8) -> crate::configuration::lobby::VisibilityToggleConfig {
        crate::configuration::lobby::VisibilityToggleConfig {
            slot,
            item: "minecraft:ender_eye".to_string(),
            display_name_on: None,
            display_name_off: None,
            lore_on: vec![],
            lore_off: vec![],
            message_on: None,
            message_off: None,
        }
    }

    #[test]
    fn slot_conflict_between_selector_and_toggle_is_rejected() {
        let mut builder = ServerState::builder();
        builder
            .set_lobby_enabled(true)
            .set_lobby_selector(Some(SelectorItemConfig {
                slot: 4,
                item: "minecraft:compass".to_string(),
                display_name: None,
                lore: vec![],
                filler: None,
            }))
            .unwrap()
            .set_lobby_visibility_toggle(Some(visibility_toggle_config(4)))
            .unwrap();

        assert!(matches!(
            builder.build(),
            Err(ServerStateBuilderError::VisibilityToggleSlotConflict { slot: 4 })
        ));
    }

    #[test]
    fn distinct_slots_for_selector_and_toggle_are_accepted() {
        let mut builder = ServerState::builder();
        builder
            .set_lobby_enabled(true)
            .set_lobby_selector(Some(SelectorItemConfig {
                slot: 4,
                item: "minecraft:compass".to_string(),
                display_name: None,
                lore: vec![],
                filler: None,
            }))
            .unwrap()
            .set_lobby_visibility_toggle(Some(visibility_toggle_config(8)))
            .unwrap();

        assert!(builder.build().is_ok());
    }

    #[test]
    fn invalid_hotbar_slots_are_rejected() {
        let mut builder = ServerState::builder();
        assert!(matches!(
            builder.set_lobby_selector(Some(SelectorItemConfig {
                slot: 99,
                item: "minecraft:compass".to_string(),
                display_name: None,
                lore: vec![],
                filler: None,
            })),
            Err(ServerStateBuilderError::InvalidHotbarSlot {
                item_kind: "selector",
                slot: 99
            })
        ));

        let mut builder = ServerState::builder();
        assert!(matches!(
            builder.set_lobby_visibility_toggle(Some(visibility_toggle_config(99))),
            Err(ServerStateBuilderError::InvalidHotbarSlot {
                item_kind: "visibility toggle",
                slot: 99
            })
        ));
    }

    #[test]
    fn hidden_players_do_not_receive_future_join_recipients() {
        let server_state = server_state(true);
        let mut hidden = client("hidden", Uuid::from_u128(1));
        let mut joining = client("joining", Uuid::from_u128(2));

        server_state.register_lobby_session(&mut hidden).unwrap();
        hidden.toggle_players_visible();
        assert!(server_state.update_lobby_players_visible(&hidden));

        let joining_session = server_state.register_lobby_session(&mut joining).unwrap();
        let join_plan = server_state
            .plan_lobby_join(joining_session.session_id)
            .expect("join plan");

        assert!(
            join_plan
                .existing_sessions
                .iter()
                .any(|s| s.username == "hidden")
        );
        assert!(join_plan.existing_recipients.is_empty());
    }

    fn server_with_destinations(destinations: Vec<LobbyDestination>) -> ServerState {
        let mut builder = ServerState::builder();
        builder.set_lobby_enabled(true);
        builder.set_lobby_destinations(destinations).unwrap();
        builder.build().unwrap()
    }

    #[test]
    fn destination_resolution() {
        let server = server_with_destinations(vec![
            LobbyDestination::new("survival", "Survival", "survival-1"),
            LobbyDestination::new("creative", "Creative", "creative-1"),
        ]);

        assert_eq!(
            server.resolve_lobby_destination("survival").unwrap().server,
            "survival-1"
        );
        assert_eq!(
            server.resolve_lobby_destination("creative").unwrap().server,
            "creative-1"
        );
        assert!(matches!(
            server.resolve_lobby_destination("minigames"),
            Err(NavigationError::UnknownDestination(id)) if id == "minigames"
        ));
    }

    #[test]
    fn builder_validates_destination_config() {
        // empty server name rejected immediately
        let mut builder = ServerState::builder();
        assert!(matches!(
            builder.set_lobby_destinations(vec![LobbyDestination::new("survival", "Survival", "")]),
            Err(ServerStateBuilderError::EmptyServerName(id)) if id == "survival"
        ));

        let mut builder = ServerState::builder();
        assert!(matches!(
            builder.set_lobby_destinations(vec![LobbyDestination::new("", "Survival", "survival")]),
            Err(ServerStateBuilderError::EmptyDestinationId)
        ));

        let mut builder = ServerState::builder();
        assert!(matches!(
            builder.set_lobby_destinations(vec![
                LobbyDestination::new("survival", "Survival", "survival"),
                LobbyDestination::new("survival", "Survival 2", "survival-2"),
            ]),
            Err(ServerStateBuilderError::DuplicateDestinationId(id)) if id == "survival"
        ));

        let mut builder = ServerState::builder();
        assert!(matches!(
            builder.set_lobby_destinations(vec![LobbyDestination::new("survival", "", "survival")]),
            Err(ServerStateBuilderError::EmptyDestinationDisplayName(id)) if id == "survival"
        ));

        // NPC referencing unknown destination rejected at build time
        let mut builder = ServerState::builder();
        builder.set_lobby_enabled(true);
        builder
            .set_lobby_destinations(vec![LobbyDestination::new(
                "survival",
                "Survival",
                "survival-1",
            )])
            .unwrap();
        builder
            .set_lobby_npcs(
                vec![crate::configuration::lobby::LobbyNpcConfig {
                    id: "creative-npc".to_string(),
                    destination: "creative".to_string(),
                    name: "Creative".to_string(),
                    x: 0.0,
                    y: 64.0,
                    z: 0.0,
                    yaw: 180.0,
                    pitch: 0.0,
                    tab_list_remove_delay_ms:
                        crate::configuration::lobby::DEFAULT_NPC_TAB_LIST_REMOVE_DELAY_MS,
                    skin: None,
                }],
                HashMap::new(),
            )
            .unwrap();
        assert!(matches!(
            builder.build(),
            Err(ServerStateBuilderError::UnknownNpcDestination { npc_id, destination_id })
                if npc_id == "creative-npc" && destination_id == "creative"
        ));
    }
}
