use minecraft_packets::login::Property;
use minecraft_protocol::prelude::{ProtocolVersion, Uuid};
use std::collections::HashMap;

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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

#[derive(Clone)]
pub struct LobbySession {
    pub session_id: LobbySessionId,
    pub uuid: Uuid,
    #[allow(dead_code)]
    pub username: String,
    #[allow(dead_code)]
    pub textures: Option<Property>,
    #[allow(dead_code)]
    pub protocol_version: ProtocolVersion,
    pub entity_id: EntityId,
    pub position: LobbyPosition,
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
            lifecycle: LobbySessionLifecycle::Joined,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct LobbyRecipient {
    pub session_id: LobbySessionId,
    pub uuid: Uuid,
    pub entity_id: EntityId,
    pub protocol_version: ProtocolVersion,
}

pub struct LobbyState {
    next_session_id: u64,
    next_entity_id: i32,
    sessions_by_uuid: HashMap<Uuid, LobbySession>,
    entity_to_uuid: HashMap<EntityId, Uuid>,
    session_to_uuid: HashMap<LobbySessionId, Uuid>,
}

impl LobbyState {
    pub fn new() -> Self {
        Self {
            next_session_id: LobbySessionId::FIRST,
            next_entity_id: EntityId::FIRST_PLAYER_ID,
            sessions_by_uuid: HashMap::new(),
            entity_to_uuid: HashMap::new(),
            session_to_uuid: HashMap::new(),
        }
    }

    pub fn insert(&mut self, mut session: LobbySession) -> LobbySession {
        if let Some(previous) = self.sessions_by_uuid.remove(&session.uuid) {
            self.entity_to_uuid.remove(&previous.entity_id);
            self.session_to_uuid.remove(&previous.session_id);
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
        Some(session)
    }

    pub fn remove_by_session_id(&mut self, session_id: LobbySessionId) -> Option<LobbySession> {
        let uuid = self.session_to_uuid.remove(&session_id)?;
        let session = self.sessions_by_uuid.remove(&uuid)?;
        self.entity_to_uuid.remove(&session.entity_id);
        Some(session)
    }

    #[allow(dead_code)]
    pub fn remove_by_entity_id(&mut self, entity_id: EntityId) -> Option<LobbySession> {
        let uuid = self.entity_to_uuid.remove(&entity_id)?;
        let session = self.sessions_by_uuid.remove(&uuid)?;
        self.session_to_uuid.remove(&session.session_id);
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
        let Some(uuid) = self.entity_to_uuid.get(&entity_id) else {
            return false;
        };
        let Some(session) = self.sessions_by_uuid.get_mut(uuid) else {
            return false;
        };
        session.position = position;
        true
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
        self.sessions_by_uuid
            .values()
            .filter(|session| Some(session.session_id) != exclude_session_id)
            .map(|session| LobbyRecipient {
                session_id: session.session_id,
                uuid: session.uuid,
                entity_id: session.entity_id,
                protocol_version: session.protocol_version,
            })
            .collect()
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
    fn removal_cleans_uuid_and_entity_indexes() {
        let mut state = LobbyState::new();
        let uuid = Uuid::from_u128(1);
        let inserted = state.insert(session(uuid, "player"));

        let removed = state.remove_by_uuid(uuid);

        assert!(removed.is_some());
        assert!(state.session_by_uuid(uuid).is_none());
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
    fn removing_by_session_id_cleans_all_indexes() {
        let mut state = LobbyState::new();
        let inserted = state.insert(session(Uuid::from_u128(1), "player"));

        let removed = state.remove_by_session_id(inserted.session_id);

        assert_eq!(removed.unwrap().uuid, inserted.uuid);
        assert!(state.session_by_uuid(inserted.uuid).is_none());
        assert!(state.session_by_entity_id(inserted.entity_id).is_none());
        assert!(state.session_by_session_id(inserted.session_id).is_none());
        assert!(state.is_empty());
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
    fn updates_position_without_exposing_registry_lock() {
        let mut state = LobbyState::new();
        let inserted = state.insert(session(Uuid::from_u128(1), "player"));
        let position = LobbyPosition::new(5.0, 6.0, 7.0, 180.0, 12.0);

        assert!(state.update_position(inserted.entity_id, position));
        assert_eq!(
            state
                .session_by_entity_id(inserted.entity_id)
                .unwrap()
                .position,
            position
        );
    }
}
