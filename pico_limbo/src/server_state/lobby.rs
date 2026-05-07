use minecraft_packets::login::Property;
use minecraft_protocol::prelude::{ProtocolVersion, Uuid};
use net::raw_packet::RawPacket;
use std::collections::HashMap;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LobbyLeavePlan {
    pub departed_uuid: Uuid,
    pub departed_username: String,
    pub departed_entity_id: EntityId,
    pub recipients: Vec<LobbyRecipient>,
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

#[derive(Clone)]
pub struct LobbyJoinPlan {
    pub new_session: LobbySession,
    pub existing_sessions: Vec<LobbySession>,
    pub existing_recipients: Vec<LobbyRecipient>,
}

pub struct LobbyState {
    next_session_id: u64,
    next_entity_id: i32,
    sessions_by_uuid: HashMap<Uuid, LobbySession>,
    entity_to_uuid: HashMap<EntityId, Uuid>,
    session_to_uuid: HashMap<LobbySessionId, Uuid>,
    broadcast_senders: HashMap<LobbySessionId, mpsc::UnboundedSender<RawPacket>>,
}

impl LobbyState {
    pub fn new() -> Self {
        Self {
            next_session_id: LobbySessionId::FIRST,
            next_entity_id: EntityId::FIRST_PLAYER_ID,
            sessions_by_uuid: HashMap::new(),
            entity_to_uuid: HashMap::new(),
            session_to_uuid: HashMap::new(),
            broadcast_senders: HashMap::new(),
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
        self.broadcast_senders.remove(&session.session_id);
        Some(session)
    }

    pub fn remove_by_session_id(&mut self, session_id: LobbySessionId) -> Option<LobbySession> {
        let uuid = self.session_to_uuid.remove(&session_id)?;
        let session = self.sessions_by_uuid.remove(&uuid)?;
        self.entity_to_uuid.remove(&session.entity_id);
        self.broadcast_senders.remove(&session_id);
        Some(session)
    }

    pub fn remove_by_session_id_with_leave_plan(
        &mut self,
        session_id: LobbySessionId,
    ) -> Option<LobbyLeavePlan> {
        let removed = self.remove_by_session_id(session_id)?;
        let recipients = self.plan_recipients(None);
        Some(LobbyLeavePlan {
            departed_uuid: removed.uuid,
            departed_username: removed.username,
            departed_entity_id: removed.entity_id,
            recipients,
        })
    }

    #[allow(dead_code)]
    pub fn remove_by_entity_id(&mut self, entity_id: EntityId) -> Option<LobbySession> {
        let uuid = self.entity_to_uuid.remove(&entity_id)?;
        let session = self.sessions_by_uuid.remove(&uuid)?;
        self.session_to_uuid.remove(&session.session_id);
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

    #[test]
    fn movement_plan_updates_position_and_excludes_moving_player() {
        let mut state = LobbyState::new();
        let moving = state.insert(session(Uuid::from_u128(1), "moving"));
        let recipient = state.insert(session(Uuid::from_u128(2), "recipient"));
        let new_position = LobbyPosition::new(5.0, 6.0, 7.0, 180.0, 12.0);

        let plan = state
            .update_position_with_movement_plan(moving.entity_id, new_position)
            .unwrap();

        assert_eq!(plan.moving_session_id, moving.session_id);
        assert_eq!(plan.moving_entity_id, moving.entity_id);
        assert_eq!(plan.previous_position, moving.position);
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
    fn metadata_plan_updates_crouching_and_excludes_sender() {
        let mut state = LobbyState::new();
        let moving = state.insert(session(Uuid::from_u128(1), "moving"));
        let recipient = state.insert(session(Uuid::from_u128(2), "recipient"));

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
    fn metadata_plan_is_empty_when_crouching_state_does_not_change() {
        let mut state = LobbyState::new();
        let moving = state.insert(session(Uuid::from_u128(1), "moving"));

        assert!(
            state
                .update_crouching_with_metadata_plan(moving.entity_id, false)
                .is_none()
        );
    }

    #[test]
    fn join_plan_returns_none_for_unknown_session() {
        let state = LobbyState::new();
        assert!(
            state
                .plan_join_visibility(LobbySessionId::new(99))
                .is_none()
        );
    }

    #[test]
    fn join_plan_contains_new_session_and_existing_sessions() {
        let mut state = LobbyState::new();
        let first = state.insert(session(Uuid::from_u128(1), "first"));
        let second = state.insert(session(Uuid::from_u128(2), "second"));

        let plan = state
            .plan_join_visibility(second.session_id)
            .expect("plan should exist");

        assert_eq!(plan.new_session.session_id, second.session_id);
        assert_eq!(plan.existing_sessions.len(), 1);
        assert_eq!(plan.existing_sessions[0].session_id, first.session_id);
        assert_eq!(plan.existing_recipients.len(), 1);
        assert_eq!(plan.existing_recipients[0].session_id, first.session_id);
    }

    #[test]
    fn join_plan_excludes_new_session_from_existing_recipients() {
        let mut state = LobbyState::new();
        let first = state.insert(session(Uuid::from_u128(1), "first"));
        let second = state.insert(session(Uuid::from_u128(2), "second"));
        let third = state.insert(session(Uuid::from_u128(3), "third"));

        let plan = state
            .plan_join_visibility(second.session_id)
            .expect("plan should exist");

        assert_eq!(plan.new_session.session_id, second.session_id);
        assert_eq!(plan.existing_sessions.len(), 2);
        assert!(
            !plan
                .existing_recipients
                .iter()
                .any(|r| r.session_id == second.session_id)
        );
        let session_ids: Vec<_> = plan
            .existing_recipients
            .iter()
            .map(|r| r.session_id)
            .collect();
        assert!(session_ids.contains(&first.session_id));
        assert!(session_ids.contains(&third.session_id));
    }

    #[test]
    fn join_plan_for_solo_player_has_empty_existing() {
        let mut state = LobbyState::new();
        let solo = state.insert(session(Uuid::from_u128(1), "solo"));

        let plan = state
            .plan_join_visibility(solo.session_id)
            .expect("plan should exist");

        assert!(plan.existing_sessions.is_empty());
        assert!(plan.existing_recipients.is_empty());
    }
}
