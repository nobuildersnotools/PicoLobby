use minecraft_packets::login::Property;
use minecraft_protocol::prelude::{ProtocolVersion, Uuid};
use net::raw_packet::RawPacket;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct EntityId(i32);

impl EntityId {
    const FIRST_PLAYER_ID: i32 = 1;

    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> i32 {
        self.0
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LobbySessionId(u64);

impl LobbySessionId {
    const FIRST: u64 = 1;

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[allow(dead_code)]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct LobbyPosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
}

impl LobbyPosition {
    pub const fn new(x: f64, y: f64, z: f64, yaw: f32, pitch: f32) -> Self {
        Self {
            x,
            y,
            z,
            yaw,
            pitch,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LobbySessionLifecycle {
    Joined,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum ChatVisibility {
    Full,
    CommandsOnly,
    Hidden,
    #[default]
    Unknown,
}

impl ChatVisibility {
    pub const fn from_client_mode(mode: i32) -> Self {
        match mode {
            0 => Self::Full,
            1 => Self::CommandsOnly,
            2 => Self::Hidden,
            _ => Self::Unknown,
        }
    }

    pub const fn receives_normal_chat(self) -> bool {
        !matches!(self, Self::Hidden)
    }

    pub const fn receives_private_messages(self) -> bool {
        !matches!(self, Self::Hidden)
    }
}

#[derive(Clone)]
pub struct LobbySession {
    pub session_id: LobbySessionId,
    pub uuid: Uuid,
    pub username: String,
    pub textures: Option<Property>,
    pub protocol_version: ProtocolVersion,
    pub entity_id: EntityId,
    pub position: LobbyPosition,
    pub crouching: bool,
    pub chat_visibility: ChatVisibility,
    #[allow(dead_code)]
    pub lifecycle: LobbySessionLifecycle,
}

impl LobbySession {
    pub fn new(
        uuid: Uuid,
        username: impl Into<String>,
        textures: Option<Property>,
        protocol_version: ProtocolVersion,
        position: LobbyPosition,
    ) -> Self {
        Self {
            session_id: LobbySessionId::new(0),
            uuid,
            username: username.into(),
            textures,
            protocol_version,
            entity_id: EntityId::new(0),
            position,
            crouching: false,
            chat_visibility: ChatVisibility::Unknown,
            lifecycle: LobbySessionLifecycle::Joined,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LobbyNpcId(String);

impl LobbyNpcId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LobbyNpcKind {
    Player,
}

#[derive(Clone)]
pub struct LobbyNpc {
    pub id: LobbyNpcId,
    pub destination_id: String,
    pub name: String,
    pub uuid: Uuid,
    pub entity_id: EntityId,
    pub position: LobbyPosition,
    pub kind: LobbyNpcKind,
}

impl LobbyNpc {
    pub fn player(
        id: impl Into<String>,
        destination_id: impl Into<String>,
        name: impl Into<String>,
        position: LobbyPosition,
    ) -> Self {
        let id = id.into();
        Self {
            uuid: deterministic_npc_uuid(&id),
            id: LobbyNpcId::new(id),
            destination_id: destination_id.into(),
            name: name.into(),
            entity_id: EntityId::new(0),
            position,
            kind: LobbyNpcKind::Player,
        }
    }
}

#[derive(Clone)]
pub struct LobbyNpcSpawnPlan {
    pub npcs: Vec<LobbyNpc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LobbyNpcInteraction {
    pub npc_id: LobbyNpcId,
    pub destination_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct LobbyRecipient {
    pub session_id: LobbySessionId,
    pub uuid: Uuid,
    pub entity_id: EntityId,
    pub protocol_version: ProtocolVersion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LobbyLeavePlan {
    pub departed_uuid: Uuid,
    pub departed_username: String,
    pub departed_entity_id: EntityId,
    pub recipients: Vec<LobbyRecipient>,
    pub lifecycle_message_recipients: Vec<LobbyRecipient>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LobbyMovementPlan {
    pub moving_session_id: LobbySessionId,
    pub moving_entity_id: EntityId,
    pub previous_position: LobbyPosition,
    pub current_position: LobbyPosition,
    pub recipients: Vec<LobbyRecipient>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LobbyMetadataPlan {
    pub session_id: LobbySessionId,
    pub entity_id: EntityId,
    pub crouching: bool,
    pub recipients: Vec<LobbyRecipient>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LobbySwingPlan {
    pub swinging_session_id: LobbySessionId,
    pub swinging_entity_id: EntityId,
    pub recipients: Vec<LobbyRecipient>,
}

#[derive(Clone)]
pub struct LobbyJoinPlan {
    pub new_session: LobbySession,
    pub existing_sessions: Vec<LobbySession>,
    pub existing_recipients: Vec<LobbyRecipient>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LobbyChatPlan {
    pub sender_session_id: LobbySessionId,
    pub sender_username: String,
    pub message: String,
    pub format: String,
    pub recipients: Vec<LobbyRecipient>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LobbyPrivateMessagePlan {
    pub sender_session_id: LobbySessionId,
    pub recipient_session_id: LobbySessionId,
    pub sender_username: String,
    pub recipient_username: String,
    pub message: String,
    pub sender_format: String,
    pub recipient_format: String,
    pub sender_recipient: LobbyRecipient,
    pub message_recipient: LobbyRecipient,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LobbyLifecycleMessagePlan {
    pub player_username: String,
    pub template: String,
    pub recipients: Vec<LobbyRecipient>,
}

pub struct LobbyState {
    next_session_id: u64,
    next_entity_id: i32,
    npcs: Vec<LobbyNpc>,
    npc_entity_to_index: HashMap<EntityId, usize>,
    sessions_by_uuid: HashMap<Uuid, LobbySession>,
    entity_to_uuid: HashMap<EntityId, Uuid>,
    session_to_uuid: HashMap<LobbySessionId, Uuid>,
    reply_peers: HashMap<LobbySessionId, Uuid>,
    broadcast_senders: HashMap<LobbySessionId, mpsc::UnboundedSender<RawPacket>>,
}

impl LobbyState {
    pub fn new() -> Self {
        Self::with_npcs(Vec::new())
    }

    pub fn with_npcs(mut npcs: Vec<LobbyNpc>) -> Self {
        let mut state = Self {
            next_session_id: LobbySessionId::FIRST,
            next_entity_id: EntityId::FIRST_PLAYER_ID,
            npcs: Vec::new(),
            npc_entity_to_index: HashMap::new(),
            sessions_by_uuid: HashMap::new(),
            entity_to_uuid: HashMap::new(),
            session_to_uuid: HashMap::new(),
            reply_peers: HashMap::new(),
            broadcast_senders: HashMap::new(),
        };

        for npc in &mut npcs {
            npc.entity_id = state.allocate_entity_id();
        }
        state.npc_entity_to_index = npcs
            .iter()
            .enumerate()
            .map(|(index, npc)| (npc.entity_id, index))
            .collect();
        state.npcs = npcs;
        state
    }

    pub fn validate_npcs(npcs: &[LobbyNpc]) -> Result<(), LobbyNpcValidationError> {
        let mut ids = HashSet::new();
        for npc in npcs {
            if npc.id.as_str().trim().is_empty() {
                return Err(LobbyNpcValidationError::EmptyId);
            }
            if !ids.insert(npc.id.clone()) {
                return Err(LobbyNpcValidationError::DuplicateId(
                    npc.id.as_str().to_string(),
                ));
            }
            if npc.destination_id.trim().is_empty() {
                return Err(LobbyNpcValidationError::EmptyDestination(
                    npc.id.as_str().to_string(),
                ));
            }
            if npc.name.trim().is_empty() {
                return Err(LobbyNpcValidationError::EmptyName(
                    npc.id.as_str().to_string(),
                ));
            }
            if npc.name.chars().count() > 16 {
                return Err(LobbyNpcValidationError::NameTooLong(
                    npc.id.as_str().to_string(),
                ));
            }
        }
        Ok(())
    }

    pub fn plan_npc_spawn(&self) -> LobbyNpcSpawnPlan {
        LobbyNpcSpawnPlan {
            npcs: self.npcs.clone(),
        }
    }

    pub fn plan_npc_interaction(
        &self,
        target_entity_id: EntityId,
        player_position: LobbyPosition,
        max_distance: f64,
    ) -> Option<LobbyNpcInteraction> {
        let npc = self
            .npc_entity_to_index
            .get(&target_entity_id)
            .and_then(|index| self.npcs.get(*index))?;
        if !positions_within(player_position, npc.position, max_distance) {
            return None;
        }
        Some(LobbyNpcInteraction {
            npc_id: npc.id.clone(),
            destination_id: npc.destination_id.clone(),
        })
    }

    pub fn insert(&mut self, mut session: LobbySession) -> LobbySession {
        if let Some(previous) = self.sessions_by_uuid.remove(&session.uuid) {
            self.entity_to_uuid.remove(&previous.entity_id);
            self.session_to_uuid.remove(&previous.session_id);
            self.reply_peers.remove(&previous.session_id);
        }

        session.session_id = self.allocate_session_id();
        session.entity_id = self.allocate_entity_id();
        self.session_to_uuid
            .insert(session.session_id, session.uuid);
        self.entity_to_uuid.insert(session.entity_id, session.uuid);
        self.sessions_by_uuid.insert(session.uuid, session.clone());
        session
    }

    #[allow(dead_code)]
    pub fn remove_by_uuid(&mut self, uuid: Uuid) -> Option<LobbySession> {
        let session = self.sessions_by_uuid.remove(&uuid)?;
        self.entity_to_uuid.remove(&session.entity_id);
        self.session_to_uuid.remove(&session.session_id);
        self.reply_peers.remove(&session.session_id);
        self.broadcast_senders.remove(&session.session_id);
        Some(session)
    }

    pub fn remove_by_session_id(&mut self, session_id: LobbySessionId) -> Option<LobbySession> {
        let uuid = self.session_to_uuid.remove(&session_id)?;
        let session = self.sessions_by_uuid.remove(&uuid)?;
        self.entity_to_uuid.remove(&session.entity_id);
        self.reply_peers.remove(&session_id);
        self.broadcast_senders.remove(&session_id);
        Some(session)
    }

    pub fn remove_by_session_id_with_leave_plan(
        &mut self,
        session_id: LobbySessionId,
    ) -> Option<LobbyLeavePlan> {
        let removed = self.remove_by_session_id(session_id)?;
        let recipients = self.plan_recipients(None);
        let lifecycle_message_recipients = self.plan_chat_recipients();
        Some(LobbyLeavePlan {
            departed_uuid: removed.uuid,
            departed_username: removed.username,
            departed_entity_id: removed.entity_id,
            recipients,
            lifecycle_message_recipients,
        })
    }

    #[allow(dead_code)]
    pub fn remove_by_entity_id(&mut self, entity_id: EntityId) -> Option<LobbySession> {
        let uuid = self.entity_to_uuid.remove(&entity_id)?;
        let session = self.sessions_by_uuid.remove(&uuid)?;
        self.session_to_uuid.remove(&session.session_id);
        self.reply_peers.remove(&session.session_id);
        self.broadcast_senders.remove(&session.session_id);
        Some(session)
    }

    #[allow(dead_code)]
    pub fn session_by_uuid(&self, uuid: Uuid) -> Option<&LobbySession> {
        self.sessions_by_uuid.get(&uuid)
    }

    #[allow(dead_code)]
    pub fn session_by_entity_id(&self, entity_id: EntityId) -> Option<&LobbySession> {
        let uuid = self.entity_to_uuid.get(&entity_id)?;
        self.sessions_by_uuid.get(uuid)
    }

    #[allow(dead_code)]
    pub fn session_by_session_id(&self, session_id: LobbySessionId) -> Option<&LobbySession> {
        let uuid = self.session_to_uuid.get(&session_id)?;
        self.sessions_by_uuid.get(uuid)
    }

    pub fn update_position(&mut self, entity_id: EntityId, position: LobbyPosition) -> bool {
        self.update_position_with_movement_plan(entity_id, position)
            .is_some()
    }

    pub fn update_position_with_movement_plan(
        &mut self,
        entity_id: EntityId,
        position: LobbyPosition,
    ) -> Option<LobbyMovementPlan> {
        let uuid = self.entity_to_uuid.get(&entity_id)?;
        let (moving_session_id, previous_position) = {
            let session = self.sessions_by_uuid.get_mut(uuid)?;
            let previous_position = session.position;
            session.position = position;
            (session.session_id, previous_position)
        };

        let recipients = self.plan_recipients(Some(moving_session_id));
        Some(LobbyMovementPlan {
            moving_session_id,
            moving_entity_id: entity_id,
            previous_position,
            current_position: position,
            recipients,
        })
    }

    pub fn update_crouching_with_metadata_plan(
        &mut self,
        entity_id: EntityId,
        crouching: bool,
    ) -> Option<LobbyMetadataPlan> {
        let uuid = self.entity_to_uuid.get(&entity_id)?;
        let session_id = {
            let session = self.sessions_by_uuid.get_mut(uuid)?;
            if session.crouching == crouching {
                return None;
            }
            session.crouching = crouching;
            session.session_id
        };

        let recipients = self.plan_recipients(Some(session_id));
        Some(LobbyMetadataPlan {
            session_id,
            entity_id,
            crouching,
            recipients,
        })
    }

    pub fn plan_swing_broadcast(&self, entity_id: EntityId) -> Option<LobbySwingPlan> {
        let uuid = self.entity_to_uuid.get(&entity_id)?;
        let session = self.sessions_by_uuid.get(uuid)?;
        let recipients = self.plan_recipients(Some(session.session_id));
        Some(LobbySwingPlan {
            swinging_session_id: session.session_id,
            swinging_entity_id: entity_id,
            recipients,
        })
    }

    pub fn update_chat_visibility(
        &mut self,
        session_id: LobbySessionId,
        chat_visibility: ChatVisibility,
    ) -> bool {
        let Some(uuid) = self.session_to_uuid.get(&session_id) else {
            return false;
        };
        let Some(session) = self.sessions_by_uuid.get_mut(uuid) else {
            return false;
        };
        session.chat_visibility = chat_visibility;
        true
    }

    pub fn plan_chat_broadcast(
        &self,
        sender_session_id: LobbySessionId,
        message: impl Into<String>,
    ) -> Option<LobbyChatPlan> {
        let sender_uuid = self.session_to_uuid.get(&sender_session_id)?;
        let sender = self.sessions_by_uuid.get(sender_uuid)?;
        let recipients = self.plan_chat_recipients();

        Some(LobbyChatPlan {
            sender_session_id,
            sender_username: sender.username.clone(),
            message: message.into(),
            format: String::new(),
            recipients,
        })
    }

    pub fn plan_private_message(
        &mut self,
        sender_session_id: LobbySessionId,
        target: &str,
        message: impl Into<String>,
        sender_format: impl Into<String>,
        recipient_format: impl Into<String>,
    ) -> Result<LobbyPrivateMessagePlan, LobbyPrivateMessageError> {
        let sender = self
            .session_by_session_id(sender_session_id)
            .ok_or(LobbyPrivateMessageError::Unavailable)?
            .clone();
        let recipient = self.find_session_by_username_ignore_case(target)?;
        self.plan_private_message_to_session(
            &sender,
            &recipient,
            message,
            sender_format,
            recipient_format,
        )
    }

    pub fn validate_private_message_target(
        &self,
        sender_session_id: LobbySessionId,
        target: &str,
    ) -> Result<(), LobbyPrivateMessageError> {
        let sender = self
            .session_by_session_id(sender_session_id)
            .ok_or(LobbyPrivateMessageError::Unavailable)?;
        let recipient = self.find_session_by_username_ignore_case(target)?;
        validate_private_message_pair(sender, &recipient)
    }

    pub fn validate_reply_target(
        &self,
        sender_session_id: LobbySessionId,
    ) -> Result<(), LobbyPrivateMessageError> {
        let sender = self
            .session_by_session_id(sender_session_id)
            .ok_or(LobbyPrivateMessageError::Unavailable)?;
        let peer_uuid = *self
            .reply_peers
            .get(&sender_session_id)
            .ok_or(LobbyPrivateMessageError::MissingReplyTarget)?;
        let recipient = self
            .session_by_uuid(peer_uuid)
            .ok_or(LobbyPrivateMessageError::MissingReplyTarget)?;
        validate_private_message_pair(sender, recipient)
    }

    pub fn plan_reply_message(
        &mut self,
        sender_session_id: LobbySessionId,
        message: impl Into<String>,
        sender_format: impl Into<String>,
        recipient_format: impl Into<String>,
    ) -> Result<LobbyPrivateMessagePlan, LobbyPrivateMessageError> {
        let sender = self
            .session_by_session_id(sender_session_id)
            .ok_or(LobbyPrivateMessageError::Unavailable)?
            .clone();
        let peer_uuid = *self
            .reply_peers
            .get(&sender_session_id)
            .ok_or(LobbyPrivateMessageError::MissingReplyTarget)?;
        let recipient = self
            .session_by_uuid(peer_uuid)
            .ok_or(LobbyPrivateMessageError::MissingReplyTarget)?
            .clone();
        self.plan_private_message_to_session(
            &sender,
            &recipient,
            message,
            sender_format,
            recipient_format,
        )
    }

    pub fn plan_lifecycle_message(
        &self,
        session_id: LobbySessionId,
        template: impl Into<String>,
    ) -> Option<LobbyLifecycleMessagePlan> {
        let uuid = self.session_to_uuid.get(&session_id)?;
        let session = self.sessions_by_uuid.get(uuid)?;
        Some(LobbyLifecycleMessagePlan {
            player_username: session.username.clone(),
            template: template.into(),
            recipients: self.plan_chat_recipients(),
        })
    }

    pub fn len(&self) -> usize {
        self.sessions_by_uuid.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.sessions_by_uuid.is_empty()
    }

    #[allow(dead_code)]
    pub fn plan_recipients(
        &self,
        exclude_session_id: Option<LobbySessionId>,
    ) -> Vec<LobbyRecipient> {
        let mut recipients = self
            .sessions_by_uuid
            .values()
            .filter(|session| Some(session.session_id) != exclude_session_id)
            .map(|session| LobbyRecipient {
                session_id: session.session_id,
                uuid: session.uuid,
                entity_id: session.entity_id,
                protocol_version: session.protocol_version,
            })
            .collect::<Vec<_>>();
        recipients.sort_by_key(|recipient| recipient.session_id);
        recipients
    }

    fn plan_chat_recipients(&self) -> Vec<LobbyRecipient> {
        let mut recipients = self
            .sessions_by_uuid
            .values()
            .filter(|session| session.chat_visibility.receives_normal_chat())
            .map(|session| LobbyRecipient {
                session_id: session.session_id,
                uuid: session.uuid,
                entity_id: session.entity_id,
                protocol_version: session.protocol_version,
            })
            .collect::<Vec<_>>();
        recipients.sort_by_key(|recipient| recipient.session_id);
        recipients
    }

    fn plan_private_message_to_session(
        &mut self,
        sender: &LobbySession,
        recipient: &LobbySession,
        message: impl Into<String>,
        sender_format: impl Into<String>,
        recipient_format: impl Into<String>,
    ) -> Result<LobbyPrivateMessagePlan, LobbyPrivateMessageError> {
        validate_private_message_pair(sender, recipient)?;

        self.reply_peers.insert(sender.session_id, recipient.uuid);
        self.reply_peers.insert(recipient.session_id, sender.uuid);

        Ok(LobbyPrivateMessagePlan {
            sender_session_id: sender.session_id,
            recipient_session_id: recipient.session_id,
            sender_username: sender.username.clone(),
            recipient_username: recipient.username.clone(),
            message: message.into(),
            sender_format: sender_format.into(),
            recipient_format: recipient_format.into(),
            sender_recipient: recipient_for_session(sender),
            message_recipient: recipient_for_session(recipient),
        })
    }

    fn find_session_by_username_ignore_case(
        &self,
        target: &str,
    ) -> Result<LobbySession, LobbyPrivateMessageError> {
        let matches = self
            .sessions_by_uuid
            .values()
            .filter(|session| session.username.eq_ignore_ascii_case(target))
            .cloned()
            .collect::<Vec<_>>();

        match matches.as_slice() {
            [] => Err(LobbyPrivateMessageError::UnknownTarget),
            [session] => Ok(session.clone()),
            _ => Err(LobbyPrivateMessageError::AmbiguousTarget),
        }
    }

    pub fn plan_join_visibility(&self, new_session_id: LobbySessionId) -> Option<LobbyJoinPlan> {
        let uuid = self.session_to_uuid.get(&new_session_id)?;
        let new_session = self.sessions_by_uuid.get(uuid)?.clone();

        let mut existing_sessions: Vec<LobbySession> = self
            .sessions_by_uuid
            .values()
            .filter(|s| s.session_id != new_session_id)
            .cloned()
            .collect();
        existing_sessions.sort_by_key(|s| s.session_id);

        let existing_recipients = self.plan_recipients(Some(new_session_id));

        Some(LobbyJoinPlan {
            new_session,
            existing_sessions,
            existing_recipients,
        })
    }

    pub fn set_broadcast_sender(
        &mut self,
        session_id: LobbySessionId,
        sender: mpsc::UnboundedSender<RawPacket>,
    ) {
        self.broadcast_senders.insert(session_id, sender);
    }

    pub fn get_broadcast_sender(
        &self,
        session_id: LobbySessionId,
    ) -> Option<mpsc::UnboundedSender<RawPacket>> {
        self.broadcast_senders.get(&session_id).cloned()
    }

    fn allocate_session_id(&mut self) -> LobbySessionId {
        let session_id = LobbySessionId::new(self.next_session_id);
        self.next_session_id = self
            .next_session_id
            .saturating_add(1)
            .max(LobbySessionId::FIRST);
        session_id
    }

    fn allocate_entity_id(&mut self) -> EntityId {
        let entity_id = EntityId::new(self.next_entity_id);
        self.next_entity_id = self
            .next_entity_id
            .saturating_add(1)
            .max(EntityId::FIRST_PLAYER_ID);
        entity_id
    }
}

#[derive(Debug, Copy, Clone, thiserror::Error, PartialEq, Eq)]
pub enum LobbyPrivateMessageError {
    #[error("private messages are only available in the lobby")]
    Unavailable,
    #[error("target player is not online")]
    UnknownTarget,
    #[error("target player is ambiguous")]
    AmbiguousTarget,
    #[error("target player has hidden chat")]
    HiddenTarget,
    #[error("no reply target")]
    MissingReplyTarget,
    #[error("cannot message self")]
    SelfMessage,
}

const fn recipient_for_session(session: &LobbySession) -> LobbyRecipient {
    LobbyRecipient {
        session_id: session.session_id,
        uuid: session.uuid,
        entity_id: session.entity_id,
        protocol_version: session.protocol_version,
    }
}

fn validate_private_message_pair(
    sender: &LobbySession,
    recipient: &LobbySession,
) -> Result<(), LobbyPrivateMessageError> {
    if sender.session_id == recipient.session_id {
        return Err(LobbyPrivateMessageError::SelfMessage);
    }
    if !recipient.chat_visibility.receives_private_messages() {
        return Err(LobbyPrivateMessageError::HiddenTarget);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LobbyNpcValidationError {
    #[error("lobby NPC id cannot be empty")]
    EmptyId,
    #[error("lobby NPC '{0}' is defined more than once")]
    DuplicateId(String),
    #[error("lobby NPC '{0}' has an empty destination")]
    EmptyDestination(String),
    #[error("lobby NPC '{0}' has an empty name")]
    EmptyName(String),
    #[error("lobby NPC '{0}' has a name longer than 16 characters")]
    NameTooLong(String),
}

fn deterministic_npc_uuid(id: &str) -> Uuid {
    let mut hash = 0xcbf2_9ce4_8422_2325_u128;
    for byte in id.as_bytes() {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    Uuid::from_u128(hash)
}

fn positions_within(first: LobbyPosition, second: LobbyPosition, max_distance: f64) -> bool {
    let dx = first.x - second.x;
    let dy = first.y - second.y;
    let dz = first.z - second.z;
    dx.mul_add(dx, dy.mul_add(dy, dz * dz)) <= max_distance * max_distance
}

impl Default for LobbyPosition {
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0, 0.0)
    }
}

impl Default for LobbyState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(uuid: Uuid, username: &str) -> LobbySession {
        LobbySession::new(
            uuid,
            username,
            None,
            ProtocolVersion::V1_20_5,
            LobbyPosition::new(1.0, 2.0, 3.0, 90.0, 45.0),
        )
    }

    fn npc(id: &str, destination: &str, x: f64) -> LobbyNpc {
        LobbyNpc::player(
            id,
            destination,
            id,
            LobbyPosition::new(x, 64.0, 0.0, 180.0, 0.0),
        )
    }

    #[test]
    fn allocates_unique_entity_ids() {
        let mut state = LobbyState::new();
        let first = state.insert(session(Uuid::from_u128(1), "first"));
        let second = state.insert(session(Uuid::from_u128(2), "second"));

        assert_ne!(first.entity_id, second.entity_id);
        assert_eq!(first.entity_id.get(), 1);
        assert_eq!(second.entity_id.get(), 2);
    }

    #[test]
    fn npc_entity_ids_do_not_collide_with_players() {
        let mut state = LobbyState::with_npcs(vec![npc("survival-npc", "survival", 0.0)]);
        let first_player = state.insert(session(Uuid::from_u128(1), "player"));
        let npc_spawn_plan = state.plan_npc_spawn();

        assert_eq!(npc_spawn_plan.npcs.len(), 1);
        assert_ne!(npc_spawn_plan.npcs[0].entity_id, first_player.entity_id);
        assert_eq!(npc_spawn_plan.npcs[0].entity_id.get(), 1);
        assert_eq!(first_player.entity_id.get(), 2);
    }

    #[test]
    fn npc_validation_rejects_duplicate_ids() {
        let err = LobbyState::validate_npcs(&[
            npc("duplicate", "survival", 0.0),
            npc("duplicate", "creative", 1.0),
        ])
        .unwrap_err();

        assert_eq!(
            err,
            LobbyNpcValidationError::DuplicateId("duplicate".to_string())
        );
    }

    #[test]
    fn npc_interaction_requires_known_entity_and_range() {
        let state = LobbyState::with_npcs(vec![npc("survival-npc", "survival", 0.0)]);
        let target = state.plan_npc_spawn().npcs[0].entity_id;

        let valid =
            state.plan_npc_interaction(target, LobbyPosition::new(0.0, 64.0, 2.0, 0.0, 0.0), 6.0);
        assert_eq!(valid.unwrap().destination_id, "survival");

        assert!(
            state
                .plan_npc_interaction(target, LobbyPosition::new(0.0, 64.0, 10.0, 0.0, 0.0), 6.0)
                .is_none()
        );
        assert!(
            state
                .plan_npc_interaction(
                    EntityId::new(999),
                    LobbyPosition::new(0.0, 64.0, 2.0, 0.0, 0.0),
                    6.0
                )
                .is_none()
        );
    }

    #[test]
    fn removal_cleans_all_indexes() {
        let mut state = LobbyState::new();
        let uuid = Uuid::from_u128(1);

        // remove_by_uuid
        let inserted = state.insert(session(uuid, "player"));
        assert!(state.remove_by_uuid(uuid).is_some());
        assert!(state.session_by_uuid(uuid).is_none());
        assert!(state.session_by_entity_id(inserted.entity_id).is_none());
        assert!(state.session_by_session_id(inserted.session_id).is_none());
        assert!(state.is_empty());

        // remove_by_session_id
        let inserted = state.insert(session(uuid, "player"));
        let removed = state.remove_by_session_id(inserted.session_id);
        assert_eq!(removed.unwrap().uuid, inserted.uuid);
        assert!(state.session_by_uuid(inserted.uuid).is_none());
        assert!(state.session_by_entity_id(inserted.entity_id).is_none());
        assert!(state.session_by_session_id(inserted.session_id).is_none());
        assert!(state.is_empty());
    }

    #[test]
    fn removing_missing_session_is_harmless() {
        let mut state = LobbyState::new();

        assert!(state.remove_by_uuid(Uuid::from_u128(1)).is_none());
        assert!(state.remove_by_session_id(LobbySessionId::new(1)).is_none());
        assert!(state.remove_by_entity_id(EntityId::new(1)).is_none());
    }

    #[test]
    fn duplicate_uuid_replaces_entity_index() {
        let mut state = LobbyState::new();
        let uuid = Uuid::from_u128(1);
        let first = state.insert(session(uuid, "first"));
        let second = state.insert(session(uuid, "second"));

        assert_ne!(first.entity_id, second.entity_id);
        assert_ne!(first.session_id, second.session_id);
        assert!(state.session_by_entity_id(first.entity_id).is_none());
        assert!(state.session_by_session_id(first.session_id).is_none());
        assert_eq!(
            state
                .session_by_entity_id(second.entity_id)
                .unwrap()
                .username,
            "second"
        );
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn stale_session_id_does_not_remove_replacement() {
        let mut state = LobbyState::new();
        let uuid = Uuid::from_u128(1);
        let first = state.insert(session(uuid, "first"));
        let second = state.insert(session(uuid, "second"));

        assert!(state.remove_by_session_id(first.session_id).is_none());
        assert_eq!(state.len(), 1);
        assert_eq!(
            state
                .session_by_session_id(second.session_id)
                .unwrap()
                .username,
            "second"
        );
    }

    #[test]
    fn leave_plan_removes_departed_player_before_collecting_recipients() {
        let mut state = LobbyState::new();
        let first = state.insert(session(Uuid::from_u128(1), "first"));
        let second = state.insert(session(Uuid::from_u128(2), "second"));
        let third = state.insert(session(Uuid::from_u128(3), "third"));

        let plan = state
            .remove_by_session_id_with_leave_plan(second.session_id)
            .unwrap();

        assert_eq!(plan.departed_uuid, second.uuid);
        assert_eq!(plan.departed_username, "second");
        assert_eq!(plan.departed_entity_id, second.entity_id);
        assert_eq!(state.len(), 2);
        assert!(state.session_by_uuid(second.uuid).is_none());
        assert!(
            !plan
                .recipients
                .iter()
                .any(|recipient| recipient.session_id == second.session_id)
        );
        assert_eq!(
            plan.recipients,
            vec![
                LobbyRecipient {
                    session_id: first.session_id,
                    uuid: first.uuid,
                    entity_id: first.entity_id,
                    protocol_version: ProtocolVersion::V1_20_5,
                },
                LobbyRecipient {
                    session_id: third.session_id,
                    uuid: third.uuid,
                    entity_id: third.entity_id,
                    protocol_version: ProtocolVersion::V1_20_5,
                },
            ]
        );
    }

    #[test]
    fn recipient_planning_can_exclude_sender_session() {
        let mut state = LobbyState::new();
        let first_uuid = Uuid::from_u128(1);
        let second_uuid = Uuid::from_u128(2);
        state.insert(session(first_uuid, "first"));
        let second = state.insert(session(second_uuid, "second"));

        let first_session_id = state.session_by_uuid(first_uuid).unwrap().session_id;
        let recipients = state.plan_recipients(Some(first_session_id));

        assert_eq!(
            recipients,
            vec![LobbyRecipient {
                session_id: second.session_id,
                uuid: second_uuid,
                entity_id: second.entity_id,
                protocol_version: ProtocolVersion::V1_20_5,
            }]
        );
    }

    #[test]
    fn chat_plan_visibility_rules() {
        assert_eq!(ChatVisibility::from_client_mode(0), ChatVisibility::Full);
        assert_eq!(
            ChatVisibility::from_client_mode(1),
            ChatVisibility::CommandsOnly
        );
        assert_eq!(ChatVisibility::from_client_mode(2), ChatVisibility::Hidden);
        assert_eq!(
            ChatVisibility::from_client_mode(99),
            ChatVisibility::Unknown
        );

        let mut state = LobbyState::new();
        let sender = state.insert(session(Uuid::from_u128(1), "sender"));
        let visible = state.insert(session(Uuid::from_u128(2), "visible"));
        let hidden = state.insert(session(Uuid::from_u128(3), "hidden"));

        state.update_chat_visibility(visible.session_id, ChatVisibility::Full);
        state.update_chat_visibility(hidden.session_id, ChatVisibility::Hidden);

        let plan = state
            .plan_chat_broadcast(sender.session_id, "hello")
            .expect("chat plan");

        assert_eq!(plan.sender_session_id, sender.session_id);
        assert_eq!(plan.sender_username, "sender");
        assert_eq!(plan.message, "hello");
        assert!(
            plan.recipients
                .iter()
                .any(|r| r.session_id == sender.session_id)
        );
        assert!(
            plan.recipients
                .iter()
                .any(|r| r.session_id == visible.session_id)
        );
        assert!(
            !plan
                .recipients
                .iter()
                .any(|r| r.session_id == hidden.session_id)
        );
    }

    #[test]
    #[allow(clippy::literal_string_with_formatting_args)]
    fn private_message_exact_ignore_case_match_succeeds() {
        let mut state = LobbyState::new();
        let sender = state.insert(session(Uuid::from_u128(1), "Sender"));
        state.insert(session(Uuid::from_u128(2), "Steve"));

        let plan = state
            .plan_private_message(
                sender.session_id,
                "steve",
                "hello",
                "to {recipient}: {message}",
                "from {sender}: {message}",
            )
            .unwrap();

        assert_eq!(plan.sender_username, "Sender");
        assert_eq!(plan.recipient_username, "Steve");
        assert_eq!(plan.message, "hello");
    }

    #[test]
    fn private_message_unknown_self_hidden_and_ambiguous_targets_fail() {
        let mut state = LobbyState::new();
        let sender = state.insert(session(Uuid::from_u128(1), "Sender"));
        let hidden = state.insert(session(Uuid::from_u128(2), "Hidden"));
        state.update_chat_visibility(hidden.session_id, ChatVisibility::Hidden);
        state.insert(session(Uuid::from_u128(3), "Dupe"));
        state.insert(session(Uuid::from_u128(4), "dupe"));

        assert_eq!(
            state
                .plan_private_message(sender.session_id, "Nobody", "hello", "", "")
                .unwrap_err(),
            LobbyPrivateMessageError::UnknownTarget
        );
        assert_eq!(
            state
                .plan_private_message(sender.session_id, "Sender", "hello", "", "")
                .unwrap_err(),
            LobbyPrivateMessageError::SelfMessage
        );
        assert_eq!(
            state
                .plan_private_message(sender.session_id, "Hidden", "hello", "", "")
                .unwrap_err(),
            LobbyPrivateMessageError::HiddenTarget
        );
        assert_eq!(
            state
                .plan_private_message(sender.session_id, "DUPE", "hello", "", "")
                .unwrap_err(),
            LobbyPrivateMessageError::AmbiguousTarget
        );
    }

    #[test]
    fn commands_only_target_receives_private_message() {
        let mut state = LobbyState::new();
        let sender = state.insert(session(Uuid::from_u128(1), "Sender"));
        let target = state.insert(session(Uuid::from_u128(2), "Target"));
        state.update_chat_visibility(target.session_id, ChatVisibility::CommandsOnly);

        let plan = state
            .plan_private_message(sender.session_id, "Target", "hello", "", "")
            .unwrap();

        assert_eq!(plan.recipient_session_id, target.session_id);
    }

    #[test]
    fn successful_private_message_updates_reply_for_both_players() {
        let mut state = LobbyState::new();
        let sender = state.insert(session(Uuid::from_u128(1), "Sender"));
        let target = state.insert(session(Uuid::from_u128(2), "Target"));

        state
            .plan_private_message(sender.session_id, "Target", "hello", "", "")
            .unwrap();
        let reply = state
            .plan_reply_message(target.session_id, "back", "", "")
            .unwrap();

        assert_eq!(reply.sender_session_id, target.session_id);
        assert_eq!(reply.recipient_session_id, sender.session_id);
    }

    #[test]
    fn reply_to_offline_peer_fails_without_changing_reply_state() {
        let mut state = LobbyState::new();
        let sender = state.insert(session(Uuid::from_u128(1), "Sender"));
        let target = state.insert(session(Uuid::from_u128(2), "Target"));
        let other = state.insert(session(Uuid::from_u128(3), "Other"));

        state
            .plan_private_message(sender.session_id, "Target", "hello", "", "")
            .unwrap();
        state.remove_by_session_id(sender.session_id);
        assert_eq!(
            state
                .plan_reply_message(target.session_id, "back", "", "")
                .unwrap_err(),
            LobbyPrivateMessageError::MissingReplyTarget
        );

        state
            .plan_private_message(other.session_id, "Target", "hello", "", "")
            .unwrap();
        let reply = state
            .plan_reply_message(target.session_id, "back", "", "")
            .unwrap();
        assert_eq!(reply.recipient_session_id, other.session_id);
    }

    #[test]
    fn lifecycle_message_plan_uses_chat_visibility_rules() {
        let mut state = LobbyState::new();
        let joining = state.insert(session(Uuid::from_u128(1), "joining"));
        let visible = state.insert(session(Uuid::from_u128(2), "visible"));
        let hidden = state.insert(session(Uuid::from_u128(3), "hidden"));

        state.update_chat_visibility(joining.session_id, ChatVisibility::Full);
        state.update_chat_visibility(visible.session_id, ChatVisibility::CommandsOnly);
        state.update_chat_visibility(hidden.session_id, ChatVisibility::Hidden);

        let plan = state
            .plan_lifecycle_message(joining.session_id, "{player} joined")
            .expect("lifecycle plan");

        assert_eq!(plan.player_username, "joining");
        assert_eq!(plan.template, "{player} joined");
        assert!(
            plan.recipients
                .iter()
                .any(|r| r.session_id == joining.session_id)
        );
        assert!(
            plan.recipients
                .iter()
                .any(|r| r.session_id == visible.session_id)
        );
        assert!(
            !plan
                .recipients
                .iter()
                .any(|r| r.session_id == hidden.session_id)
        );
    }

    #[test]
    fn leave_plan_message_recipients_exclude_departed_player() {
        let mut state = LobbyState::new();
        let departing = state.insert(session(Uuid::from_u128(1), "departing"));
        let remaining = state.insert(session(Uuid::from_u128(2), "remaining"));

        state.update_chat_visibility(remaining.session_id, ChatVisibility::Full);

        let plan = state
            .remove_by_session_id_with_leave_plan(departing.session_id)
            .expect("leave plan");

        assert_eq!(plan.departed_username, "departing");
        assert!(
            !plan
                .lifecycle_message_recipients
                .iter()
                .any(|r| r.session_id == departing.session_id)
        );
        assert_eq!(
            plan.lifecycle_message_recipients[0].session_id,
            remaining.session_id
        );
    }

    #[test]
    fn movement_plan_updates_position_and_excludes_moving_player() {
        let mut state = LobbyState::new();
        let moving = state.insert(session(Uuid::from_u128(1), "moving"));
        let recipient = state.insert(session(Uuid::from_u128(2), "recipient"));
        let new_position = LobbyPosition::new(5.0, 6.0, 7.0, 180.0, 12.0);

        // simple update_position path
        assert!(state.update_position(
            moving.entity_id,
            LobbyPosition::new(0.0, 0.0, 0.0, 0.0, 0.0)
        ));

        // full movement plan path
        let plan = state
            .update_position_with_movement_plan(moving.entity_id, new_position)
            .unwrap();

        assert_eq!(plan.moving_session_id, moving.session_id);
        assert_eq!(plan.moving_entity_id, moving.entity_id);
        assert_eq!(plan.current_position, new_position);
        assert_eq!(
            plan.recipients,
            vec![LobbyRecipient {
                session_id: recipient.session_id,
                uuid: recipient.uuid,
                entity_id: recipient.entity_id,
                protocol_version: ProtocolVersion::V1_20_5,
            }]
        );
        assert_eq!(
            state
                .session_by_entity_id(moving.entity_id)
                .unwrap()
                .position,
            new_position
        );
    }

    #[test]
    fn metadata_plan_crouching_state() {
        let mut state = LobbyState::new();
        let moving = state.insert(session(Uuid::from_u128(1), "moving"));
        let recipient = state.insert(session(Uuid::from_u128(2), "recipient"));

        // no plan when state is unchanged
        assert!(
            state
                .update_crouching_with_metadata_plan(moving.entity_id, false)
                .is_none()
        );

        let plan = state
            .update_crouching_with_metadata_plan(moving.entity_id, true)
            .unwrap();

        assert_eq!(plan.session_id, moving.session_id);
        assert_eq!(plan.entity_id, moving.entity_id);
        assert!(plan.crouching);
        assert_eq!(
            plan.recipients,
            vec![LobbyRecipient {
                session_id: recipient.session_id,
                uuid: recipient.uuid,
                entity_id: recipient.entity_id,
                protocol_version: ProtocolVersion::V1_20_5,
            }]
        );
        assert!(
            state
                .session_by_entity_id(moving.entity_id)
                .unwrap()
                .crouching
        );
    }

    #[test]
    fn swing_plan_excludes_sender_and_preserves_sorted_recipient_versions() {
        let mut state = LobbyState::new();
        let swinger = state.insert(session(Uuid::from_u128(1), "swinger"));
        let mut old_session = session(Uuid::from_u128(2), "old");
        old_session.protocol_version = ProtocolVersion::V1_8;
        let old = state.insert(old_session);
        let modern = state.insert(session(Uuid::from_u128(3), "modern"));

        let plan = state.plan_swing_broadcast(swinger.entity_id).unwrap();

        assert_eq!(plan.swinging_session_id, swinger.session_id);
        assert_eq!(plan.swinging_entity_id, swinger.entity_id);
        assert!(
            !plan
                .recipients
                .iter()
                .any(|r| r.session_id == swinger.session_id)
        );
        assert_eq!(
            plan.recipients,
            vec![
                LobbyRecipient {
                    session_id: old.session_id,
                    uuid: old.uuid,
                    entity_id: old.entity_id,
                    protocol_version: ProtocolVersion::V1_8,
                },
                LobbyRecipient {
                    session_id: modern.session_id,
                    uuid: modern.uuid,
                    entity_id: modern.entity_id,
                    protocol_version: ProtocolVersion::V1_20_5,
                },
            ]
        );
    }

    #[test]
    fn swing_plan_for_unknown_entity_is_empty() {
        assert!(
            LobbyState::new()
                .plan_swing_broadcast(EntityId::new(99))
                .is_none()
        );
    }

    #[test]
    fn join_plan_structure_and_exclusions() {
        // unknown session returns None
        assert!(
            LobbyState::new()
                .plan_join_visibility(LobbySessionId::new(99))
                .is_none()
        );

        // solo player: empty existing lists
        let mut state = LobbyState::new();
        let solo = state.insert(session(Uuid::from_u128(1), "solo"));
        let plan = state.plan_join_visibility(solo.session_id).unwrap();
        assert!(plan.existing_sessions.is_empty());
        assert!(plan.existing_recipients.is_empty());

        // second player sees the solo in existing; solo is in recipients
        let second = state.insert(session(Uuid::from_u128(2), "second"));
        let plan = state.plan_join_visibility(second.session_id).unwrap();
        assert_eq!(plan.new_session.session_id, second.session_id);
        assert_eq!(plan.existing_sessions.len(), 1);
        assert_eq!(plan.existing_sessions[0].session_id, solo.session_id);
        assert_eq!(plan.existing_recipients[0].session_id, solo.session_id);

        // third player: newcomer excluded from existing_recipients
        let third = state.insert(session(Uuid::from_u128(3), "third"));
        let plan = state.plan_join_visibility(second.session_id).unwrap();
        assert_eq!(plan.existing_sessions.len(), 2);
        assert!(
            !plan
                .existing_recipients
                .iter()
                .any(|r| r.session_id == second.session_id)
        );
        let ids: Vec<_> = plan
            .existing_recipients
            .iter()
            .map(|r| r.session_id)
            .collect();
        assert!(ids.contains(&solo.session_id));
        assert!(ids.contains(&third.session_id));
    }
}
