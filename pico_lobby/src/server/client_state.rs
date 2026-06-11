use crate::server::game_profile::GameProfile;
use crate::server_state::{
    ChatVisibility, LobbyChatPlan, LobbyMetadataPlan, LobbyMovementPlan, LobbyPrivateMessagePlan,
    LobbySessionId, LobbySwingPlan, OpenSelectorState,
};
use minecraft_packets::login::Property;
use minecraft_protocol::prelude::{ProtocolVersion, State, Uuid};
use std::time::{Duration, Instant};
use tracing::info;

#[derive(PartialEq, Eq)]
pub enum KeepAliveStatus {
    Disabled,
    ShouldEnable,
    Enabled,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            state: State::Handshake,
            protocol_version: ProtocolVersion::Any,
            kick_message: None,
            message_id: -1,
            game_profile: None,
            keep_alive_enabled: KeepAliveStatus::Disabled,
            entity_id: 0,
            lobby_session_id: None,
            chat_visibility: ChatVisibility::Unknown,
            pending_lobby_chat_plan: None,
            pending_lobby_private_message_plan: None,
            last_chat_message_at: None,
            pending_lobby_metadata_plan: None,
            pending_lobby_movement_plan: None,
            pending_lobby_swing_plan: None,
            last_movement_broadcast_at: None,
            last_swing_broadcast_at: None,
            position: (0.0, 0.0, 0.0),
            rotation: (0.0, 0.0),
            is_flight_allowed: false,
            is_flying: false,
            flying_speed: 0.05,
            selected_hotbar_slot: 0,
            next_window_id: 1,
            open_selector: None,
            players_visible: true,
        }
    }
}

pub struct ClientState {
    state: State,
    protocol_version: ProtocolVersion,
    kick_message: Option<String>,
    message_id: i32,
    game_profile: Option<GameProfile>,
    keep_alive_enabled: KeepAliveStatus,
    entity_id: i32,
    lobby_session_id: Option<LobbySessionId>,
    chat_visibility: ChatVisibility,
    pending_lobby_chat_plan: Option<LobbyChatPlan>,
    pending_lobby_private_message_plan: Option<LobbyPrivateMessagePlan>,
    last_chat_message_at: Option<Instant>,
    pending_lobby_metadata_plan: Option<LobbyMetadataPlan>,
    pending_lobby_movement_plan: Option<LobbyMovementPlan>,
    pending_lobby_swing_plan: Option<LobbySwingPlan>,
    last_movement_broadcast_at: Option<Instant>,
    last_swing_broadcast_at: Option<Instant>,
    position: (f64, f64, f64),
    rotation: (f32, f32),
    is_flight_allowed: bool,
    is_flying: bool,
    flying_speed: f32,
    selected_hotbar_slot: u8,
    next_window_id: u8,
    open_selector: Option<OpenSelectorState>,
    players_visible: bool,
}

impl ClientState {
    const ANONYMOUS: &'static str = "Anonymous";

    // Kick

    pub fn kick(&mut self, kick_message: &str) {
        self.kick_message = Some(kick_message.to_string());
    }

    pub fn should_kick(&self) -> Option<String> {
        self.kick_message.clone()
    }

    // State

    pub const fn state(&self) -> State {
        self.state
    }

    pub const fn set_state(&mut self, new_state: State) {
        self.state = new_state;
    }

    // Protocol version

    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    pub const fn set_protocol_version(&mut self, new_protocol_version: ProtocolVersion) {
        self.protocol_version = new_protocol_version;
    }

    // Velocity

    pub const fn set_velocity_login_message_id(&mut self, message_id: i32) {
        self.message_id = message_id;
    }

    pub const fn get_velocity_login_message_id(&self) -> i32 {
        self.message_id
    }

    // Game profile

    pub fn set_game_profile(&mut self, game_profile: GameProfile) {
        if let Some(ref mut existing_game_profile) = self.game_profile {
            existing_game_profile.set_name(&game_profile.username());
        } else {
            self.game_profile = Some(game_profile);
        }

        if let Some(ref existing_game_profile) = self.game_profile
            && !existing_game_profile.is_anonymous()
        {
            info!(
                "UUID of player {} is {}",
                existing_game_profile.username(),
                existing_game_profile.uuid()
            );
        }
    }

    pub fn game_profile(&self) -> Option<GameProfile> {
        self.game_profile.clone()
    }

    pub fn get_username(&self) -> String {
        self.game_profile().map_or_else(
            || Self::ANONYMOUS.to_owned(),
            |profile| profile.username().to_owned(),
        )
    }

    pub fn get_unique_id(&self) -> Uuid {
        self.game_profile()
            .map_or_else(Uuid::default, |profile| profile.uuid())
    }

    pub fn get_textures(&self) -> Option<Property> {
        self.game_profile()
            .and_then(|profile| profile.textures().cloned())
    }

    // Entity

    pub const fn entity_id(&self) -> i32 {
        self.entity_id
    }

    pub const fn set_entity_id(&mut self, entity_id: i32) {
        self.entity_id = entity_id;
    }

    pub const fn lobby_session_id(&self) -> Option<LobbySessionId> {
        self.lobby_session_id
    }

    pub const fn set_lobby_session_id(&mut self, session_id: LobbySessionId) {
        self.lobby_session_id = Some(session_id);
    }

    pub const fn clear_lobby_session_id(&mut self) {
        self.lobby_session_id = None;
    }

    pub const fn chat_visibility(&self) -> ChatVisibility {
        self.chat_visibility
    }

    pub const fn set_chat_visibility(&mut self, chat_visibility: ChatVisibility) {
        self.chat_visibility = chat_visibility;
    }

    pub fn set_pending_chat_plan(&mut self, plan: LobbyChatPlan) {
        self.pending_lobby_chat_plan = Some(plan);
    }

    pub const fn take_pending_chat_plan(&mut self) -> Option<LobbyChatPlan> {
        self.pending_lobby_chat_plan.take()
    }

    pub fn set_pending_private_message_plan(&mut self, plan: LobbyPrivateMessagePlan) {
        self.pending_lobby_private_message_plan = Some(plan);
    }

    pub const fn take_pending_private_message_plan(&mut self) -> Option<LobbyPrivateMessagePlan> {
        self.pending_lobby_private_message_plan.take()
    }

    pub fn check_chat_rate_limit(&mut self, min_interval: Duration) -> bool {
        let now = Instant::now();
        if let Some(last) = self.last_chat_message_at
            && now.duration_since(last) < min_interval
        {
            return false;
        }
        self.last_chat_message_at = Some(now);
        true
    }

    pub fn consume_chat_rate_limit(&mut self) {
        self.last_chat_message_at = Some(Instant::now());
    }

    pub fn set_pending_metadata_plan(&mut self, plan: LobbyMetadataPlan) {
        self.pending_lobby_metadata_plan = Some(plan);
    }

    pub const fn take_pending_metadata_plan(&mut self) -> Option<LobbyMetadataPlan> {
        self.pending_lobby_metadata_plan.take()
    }

    pub fn set_pending_movement_plan(&mut self, plan: LobbyMovementPlan) {
        self.pending_lobby_movement_plan = Some(plan);
    }

    pub const fn take_pending_movement_plan(&mut self) -> Option<LobbyMovementPlan> {
        self.pending_lobby_movement_plan.take()
    }

    pub fn set_pending_swing_plan(&mut self, plan: LobbySwingPlan) {
        self.pending_lobby_swing_plan = Some(plan);
    }

    pub const fn take_pending_swing_plan(&mut self) -> Option<LobbySwingPlan> {
        self.pending_lobby_swing_plan.take()
    }

    pub fn check_movement_broadcast_rate_limit(&mut self, min_interval: Duration) -> bool {
        let now = Instant::now();
        if let Some(last) = self.last_movement_broadcast_at
            && now.duration_since(last) < min_interval
        {
            return false;
        }
        self.last_movement_broadcast_at = Some(now);
        true
    }

    pub fn check_swing_broadcast_rate_limit(&mut self, min_interval: Duration) -> bool {
        let now = Instant::now();
        if let Some(last) = self.last_swing_broadcast_at
            && now.duration_since(last) < min_interval
        {
            return false;
        }
        self.last_swing_broadcast_at = Some(now);
        true
    }

    // Keep alive

    pub fn should_enable_keep_alive(&self) -> bool {
        self.keep_alive_enabled == KeepAliveStatus::ShouldEnable
    }

    pub fn set_keep_alive_should_enable(&mut self) {
        if self.keep_alive_enabled == KeepAliveStatus::Disabled {
            self.keep_alive_enabled = KeepAliveStatus::ShouldEnable;
        }
    }

    pub fn set_keep_alive_enabled(&mut self) {
        if self.keep_alive_enabled == KeepAliveStatus::ShouldEnable {
            self.keep_alive_enabled = KeepAliveStatus::Enabled;
        }
    }

    // Position

    pub const fn get_y_position(&self) -> f64 {
        self.position.1
    }

    pub const fn position(&self) -> (f64, f64, f64) {
        self.position
    }

    pub const fn set_position(&mut self, position: (f64, f64, f64)) {
        self.position = position;
    }

    pub const fn rotation(&self) -> (f32, f32) {
        self.rotation
    }

    pub const fn set_rotation(&mut self, rotation: (f32, f32)) {
        self.rotation = rotation;
    }

    // Movement

    pub const fn is_flight_allowed(&self) -> bool {
        self.is_flight_allowed
    }

    pub const fn set_is_flight_allowed(&mut self, allow_flight: bool) {
        self.is_flight_allowed = allow_flight;
    }

    pub const fn is_flying(&self) -> bool {
        self.is_flying
    }

    pub const fn set_is_flying(&mut self, is_flying: bool) {
        self.is_flying = is_flying;
    }

    pub const fn get_flying_speed(&self) -> f32 {
        self.flying_speed
    }

    pub const fn set_flying_speed(&mut self, flying_speed: f32) {
        self.flying_speed = flying_speed;
    }

    pub const fn selected_hotbar_slot(&self) -> u8 {
        self.selected_hotbar_slot
    }

    pub const fn set_selected_hotbar_slot(&mut self, slot: u8) {
        self.selected_hotbar_slot = slot;
    }

    // Window ID

    /// Allocates the next window ID, wrapping from 200 back to 1 (never 0).
    pub const fn allocate_window_id(&mut self) -> u8 {
        let id = self.next_window_id;
        self.next_window_id = if self.next_window_id >= 200 {
            1
        } else {
            self.next_window_id + 1
        };
        id
    }

    // Open selector

    pub const fn open_selector(&self) -> Option<&OpenSelectorState> {
        self.open_selector.as_ref()
    }

    pub const fn open_selector_mut(&mut self) -> Option<&mut OpenSelectorState> {
        self.open_selector.as_mut()
    }

    pub fn set_open_selector(&mut self, state: OpenSelectorState) {
        self.open_selector = Some(state);
    }

    pub const fn take_open_selector(&mut self) -> Option<OpenSelectorState> {
        self.open_selector.take()
    }

    // Player visibility toggle

    #[allow(dead_code)]
    pub const fn players_visible(&self) -> bool {
        self.players_visible
    }

    /// Flips the visibility flag and returns the new value.
    pub const fn toggle_players_visible(&mut self) -> bool {
        self.players_visible = !self.players_visible;
        self.players_visible
    }
}
