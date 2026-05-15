use crate::server::packet_registry::PacketRegistry;
use crate::server_state::{
    EntityId, LobbyJoinPlan, LobbyLeavePlan, LobbyMetadataPlan, LobbyMovementPlan, LobbyNpc,
    LobbyNpcKind, LobbyNpcSpawnPlan, LobbyPosition, LobbyRecipient, LobbySession,
};
use minecraft_packets::play::destroy_entities_packet::DestroyEntitiesPacket;
use minecraft_packets::play::move_entity_packet::{
    MoveEntityPosPacket, MoveEntityPosRotPacket, MoveEntityRotPacket, RelativeMoveDelta,
};
use minecraft_packets::play::player_info_remove_packet::PlayerInfoRemovePacket;
use minecraft_packets::play::player_info_update_packet::PlayerInfoUpdatePacket;
use minecraft_packets::play::remove_entities_packet::RemoveEntitiesPacket;
use minecraft_packets::play::rotate_head_packet::RotateHeadPacket;
use minecraft_packets::play::set_entity_data_packet::{EntityBaseFlags, SetEntityMetadataPacket};
use minecraft_packets::play::spawn_entity_packet::SpawnEntityPacket;
use minecraft_packets::play::spawn_player_packet::SpawnPlayerPacket;
use minecraft_packets::play::teleport_entity_packet::{
    EntityPositionSyncPacket, TeleportEntityPacket,
};
use minecraft_protocol::prelude::{ProtocolVersion, Uuid};

pub struct LobbyPacketBatch {
    #[allow(dead_code)]
    pub recipient: LobbyRecipient,
    pub packets: Vec<PacketRegistry>,
}

pub struct LobbyMovementPacketBatch {
    #[allow(dead_code)]
    pub recipient: LobbyRecipient,
    pub packets: Vec<PacketRegistry>,
}

pub struct LobbyMetadataPacketBatch {
    #[allow(dead_code)]
    pub recipient: LobbyRecipient,
    pub packets: Vec<PacketRegistry>,
}

pub fn leave_visibility_batches(plan: &LobbyLeavePlan) -> Vec<LobbyPacketBatch> {
    plan.recipients
        .iter()
        .filter_map(|recipient| {
            let packets = leave_visibility_packets(
                recipient.protocol_version,
                plan.departed_uuid,
                &plan.departed_username,
                plan.departed_entity_id,
            );
            if packets.is_empty() {
                None
            } else {
                Some(LobbyPacketBatch {
                    recipient: recipient.clone(),
                    packets,
                })
            }
        })
        .collect()
}

pub fn leave_visibility_packets(
    recipient_version: ProtocolVersion,
    departed_uuid: Uuid,
    departed_username: &str,
    departed_entity_id: EntityId,
) -> Vec<PacketRegistry> {
    let entity_remove = if recipient_version.is_after_inclusive(ProtocolVersion::V1_21) {
        PacketRegistry::RemoveEntities(RemoveEntitiesPacket::single(departed_entity_id.get()))
    } else {
        PacketRegistry::DestroyEntities(DestroyEntitiesPacket::single(departed_entity_id.get()))
    };

    let player_info_remove = if recipient_version.is_after_inclusive(ProtocolVersion::V1_19_3) {
        Some(PacketRegistry::PlayerInfoRemove(
            PlayerInfoRemovePacket::single(departed_uuid),
        ))
    } else if recipient_version.is_after_inclusive(ProtocolVersion::V1_8) {
        Some(PacketRegistry::PlayerInfoUpdate(
            PlayerInfoUpdatePacket::remove(departed_uuid, departed_username.to_owned()),
        ))
    } else {
        Some(PacketRegistry::PlayerInfoUpdate(
            PlayerInfoUpdatePacket::remove_legacy_name(departed_username.to_owned()),
        ))
    };

    let mut packets = vec![entity_remove];
    packets.extend(player_info_remove);
    packets
}

pub fn movement_visibility_batches(plan: &LobbyMovementPlan) -> Vec<LobbyMovementPacketBatch> {
    plan.recipients
        .iter()
        .filter(|recipient| supports_movement_visibility(recipient.protocol_version))
        .filter_map(|recipient| {
            let packets = movement_visibility_packets(
                recipient.protocol_version,
                plan.moving_entity_id,
                plan.previous_position,
                plan.current_position,
            );
            if packets.is_empty() {
                None
            } else {
                Some(LobbyMovementPacketBatch {
                    recipient: recipient.clone(),
                    packets,
                })
            }
        })
        .collect()
}

fn supports_movement_visibility(protocol_version: ProtocolVersion) -> bool {
    protocol_version.is_after_inclusive(ProtocolVersion::V1_7_2)
}

pub fn movement_visibility_packets(
    recipient_version: ProtocolVersion,
    moving_entity_id: EntityId,
    previous_position: LobbyPosition,
    current_position: LobbyPosition,
) -> Vec<PacketRegistry> {
    let position_changed = position_changed(previous_position, current_position);
    let rotation_changed = rotation_changed(previous_position, current_position);

    if !position_changed && !rotation_changed {
        return Vec::new();
    }

    let delta = if position_changed {
        match RelativeMoveDelta::between_for_version(
            (
                previous_position.x,
                previous_position.y,
                previous_position.z,
            ),
            (current_position.x, current_position.y, current_position.z),
            recipient_version,
        ) {
            Ok(delta) => Some(delta),
            Err(_) => {
                return vec![teleport_visibility_packet(
                    recipient_version,
                    moving_entity_id,
                    current_position,
                )];
            }
        }
    } else {
        None
    };

    let entity_id = moving_entity_id.get();
    let mut packets = Vec::new();
    match (delta, rotation_changed) {
        (Some(delta), true) => {
            packets.push(PacketRegistry::MoveEntityPosRot(
                MoveEntityPosRotPacket::new(
                    entity_id,
                    delta,
                    current_position.yaw,
                    current_position.pitch,
                    true,
                ),
            ));
            packets.push(PacketRegistry::RotateHead(RotateHeadPacket::new(
                entity_id,
                current_position.yaw,
            )));
        }
        (Some(delta), false) => {
            packets.push(PacketRegistry::MoveEntityPos(MoveEntityPosPacket::new(
                entity_id, delta, true,
            )));
        }
        (None, true) => {
            packets.push(PacketRegistry::MoveEntityRot(MoveEntityRotPacket::new(
                entity_id,
                current_position.yaw,
                current_position.pitch,
                true,
            )));
            packets.push(PacketRegistry::RotateHead(RotateHeadPacket::new(
                entity_id,
                current_position.yaw,
            )));
        }
        (None, false) => {}
    }

    packets
}

pub fn metadata_visibility_batches(plan: &LobbyMetadataPlan) -> Vec<LobbyMetadataPacketBatch> {
    plan.recipients
        .iter()
        .filter(|recipient| {
            recipient
                .protocol_version
                .is_after_inclusive(ProtocolVersion::V1_7_2)
        })
        .filter_map(|recipient| {
            let packets = metadata_visibility_packets(plan);
            if packets.is_empty() {
                None
            } else {
                Some(LobbyMetadataPacketBatch {
                    recipient: recipient.clone(),
                    packets,
                })
            }
        })
        .collect()
}

pub fn metadata_visibility_packets(plan: &LobbyMetadataPlan) -> Vec<PacketRegistry> {
    vec![PacketRegistry::SetEntityMetadata(player_metadata_packet(
        plan.entity_id.get(),
        plan.crouching,
    ))]
}

fn teleport_visibility_packet(
    recipient_version: ProtocolVersion,
    moving_entity_id: EntityId,
    current_position: LobbyPosition,
) -> PacketRegistry {
    let entity_id = moving_entity_id.get();
    if recipient_version.is_after_inclusive(ProtocolVersion::V1_21_2) {
        PacketRegistry::EntityPositionSync(EntityPositionSyncPacket::absolute(
            entity_id,
            current_position.x,
            current_position.y,
            current_position.z,
            current_position.yaw,
            current_position.pitch,
            true,
        ))
    } else {
        PacketRegistry::TeleportEntity(TeleportEntityPacket::absolute(
            entity_id,
            current_position.x,
            current_position.y,
            current_position.z,
            current_position.yaw,
            current_position.pitch,
            true,
        ))
    }
}

pub fn join_visibility_packets_for_newcomer(
    plan: &LobbyJoinPlan,
    newcomer_version: ProtocolVersion,
) -> Vec<PacketRegistry> {
    if newcomer_version.is_after_inclusive(ProtocolVersion::V1_20_2) {
        return plan
            .existing_sessions
            .iter()
            .flat_map(player_spawn_packets_current)
            .collect();
    }
    if newcomer_version.is_after_inclusive(ProtocolVersion::V1_7_2)
        && newcomer_version.is_before_inclusive(ProtocolVersion::V1_20)
    {
        return plan
            .existing_sessions
            .iter()
            .flat_map(|s| player_spawn_packets_legacy(s, newcomer_version))
            .collect();
    }
    Vec::new()
}

pub fn join_visibility_batches_for_existing(plan: &LobbyJoinPlan) -> Vec<LobbyPacketBatch> {
    plan.existing_recipients
        .iter()
        .filter_map(|recipient| {
            let version = recipient.protocol_version;
            let packets = if version.is_after_inclusive(ProtocolVersion::V1_20_2) {
                player_spawn_packets_current(&plan.new_session)
            } else if version.is_after_inclusive(ProtocolVersion::V1_7_2)
                && version.is_before_inclusive(ProtocolVersion::V1_20)
            {
                player_spawn_packets_legacy(&plan.new_session, version)
            } else {
                Vec::new()
            };
            if packets.is_empty() {
                None
            } else {
                Some(LobbyPacketBatch {
                    recipient: recipient.clone(),
                    packets,
                })
            }
        })
        .collect()
}

pub fn npc_spawn_packets_for_join(
    plan: &LobbyNpcSpawnPlan,
    recipient_version: ProtocolVersion,
) -> Vec<PacketRegistry> {
    plan.npcs
        .iter()
        .flat_map(|npc| npc_spawn_packets(npc, recipient_version))
        .collect()
}

fn npc_spawn_packets(npc: &LobbyNpc, recipient_version: ProtocolVersion) -> Vec<PacketRegistry> {
    match npc.kind {
        LobbyNpcKind::Player => {}
    }

    let mut session = LobbySession::new(
        npc.uuid,
        npc.name.clone(),
        None,
        recipient_version,
        npc.position,
    );
    session.entity_id = npc.entity_id;

    let mut packets = if recipient_version.is_after_inclusive(ProtocolVersion::V1_20_2) {
        player_spawn_packets_current(&session)
    } else if recipient_version.is_after_inclusive(ProtocolVersion::V1_7_2)
        && recipient_version.is_before_inclusive(ProtocolVersion::V1_20)
    {
        player_spawn_packets_legacy(&session, recipient_version)
    } else {
        Vec::new()
    };

    if npc_player_info_remove_after_spawn(recipient_version) {
        packets.push(npc_player_info_remove_packet(
            recipient_version,
            npc.uuid,
            &npc.name,
        ));
    }
    packets
}

fn npc_player_info_remove_after_spawn(recipient_version: ProtocolVersion) -> bool {
    !recipient_version.between_inclusive(ProtocolVersion::V1_9, ProtocolVersion::V1_12_2)
}

fn npc_player_info_remove_packet(
    recipient_version: ProtocolVersion,
    uuid: Uuid,
    username: &str,
) -> PacketRegistry {
    if recipient_version.is_after_inclusive(ProtocolVersion::V1_19_3) {
        PacketRegistry::PlayerInfoRemove(PlayerInfoRemovePacket::single(uuid))
    } else if recipient_version.is_after_inclusive(ProtocolVersion::V1_8) {
        PacketRegistry::PlayerInfoUpdate(PlayerInfoUpdatePacket::remove(uuid, username.to_owned()))
    } else {
        PacketRegistry::PlayerInfoUpdate(PlayerInfoUpdatePacket::remove_legacy_name(
            username.to_owned(),
        ))
    }
}

fn player_info_packet(player: &LobbySession) -> PacketRegistry {
    player.textures.as_ref().map_or_else(
        || {
            PacketRegistry::PlayerInfoUpdate(PlayerInfoUpdatePacket::skinless(
                player.username.clone(),
                player.uuid,
                true,
            ))
        },
        |textures| {
            PacketRegistry::PlayerInfoUpdate(PlayerInfoUpdatePacket::skin(
                player.username.clone(),
                player.uuid,
                textures.clone(),
                true,
            ))
        },
    )
}

fn player_spawn_packets_current(player: &LobbySession) -> Vec<PacketRegistry> {
    let player_info = player_info_packet(player);
    let spawn = PacketRegistry::SpawnEntity(SpawnEntityPacket::spawn_player(
        player.entity_id.get(),
        player.uuid,
        player.position.x,
        player.position.y,
        player.position.z,
        player.position.yaw,
        player.position.pitch,
    ));
    let metadata = PacketRegistry::SetEntityMetadata(player_metadata_packet(
        player.entity_id.get(),
        player.crouching,
    ));
    let head_rotation = PacketRegistry::RotateHead(RotateHeadPacket::new(
        player.entity_id.get(),
        player.position.yaw,
    ));
    vec![player_info, spawn, metadata, head_rotation]
}

fn player_spawn_packets_legacy(
    player: &LobbySession,
    version: ProtocolVersion,
) -> Vec<PacketRegistry> {
    let player_info = player_info_packet(player);
    let spawn = PacketRegistry::SpawnPlayer(SpawnPlayerPacket::lobby_player(
        player.entity_id.get(),
        player.uuid,
        player.username.clone(),
        player.textures.clone(),
        player_base_flags(player.crouching),
        (player.position.x, player.position.y, player.position.z),
        (player.position.yaw, player.position.pitch),
    ));
    let head_rotation = PacketRegistry::RotateHead(RotateHeadPacket::new(
        player.entity_id.get(),
        player.position.yaw,
    ));
    // Metadata is embedded only in the 1.7.2/1.8 spawn-player payloads. For 1.9+
    // it must be sent separately, which is also safe for the older clients.
    let _ = version;
    let metadata = PacketRegistry::SetEntityMetadata(player_metadata_packet(
        player.entity_id.get(),
        player.crouching,
    ));
    vec![player_info, spawn, metadata, head_rotation]
}

fn player_metadata_packet(entity_id: i32, crouching: bool) -> SetEntityMetadataPacket {
    SetEntityMetadataPacket::player(entity_id, player_base_flags(crouching))
}

const fn player_base_flags(crouching: bool) -> EntityBaseFlags {
    if crouching {
        EntityBaseFlags::crouching()
    } else {
        EntityBaseFlags::empty()
    }
}

fn position_changed(previous_position: LobbyPosition, current_position: LobbyPosition) -> bool {
    (previous_position.x - current_position.x).abs() > f64::EPSILON
        || (previous_position.y - current_position.y).abs() > f64::EPSILON
        || (previous_position.z - current_position.z).abs() > f64::EPSILON
}

fn rotation_changed(previous_position: LobbyPosition, current_position: LobbyPosition) -> bool {
    (previous_position.yaw - current_position.yaw).abs() > f32::EPSILON
        || (previous_position.pitch - current_position.pitch).abs() > f32::EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_state::{LobbyRecipient, LobbySessionId};

    const UUID_BYTES: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 99];
    const LEGACY_UUID_LONGS: [u8; 16] = UUID_BYTES;
    const DEPARTED_NAME: &str = "departed";

    fn recipient(session_id: u64, protocol_version: ProtocolVersion) -> LobbyRecipient {
        LobbyRecipient {
            session_id: LobbySessionId::new(session_id),
            uuid: Uuid::from_u128(session_id.into()),
            entity_id: EntityId::new(i32::try_from(session_id).unwrap()),
            protocol_version,
        }
    }

    fn plan(recipients: Vec<LobbyRecipient>) -> LobbyLeavePlan {
        LobbyLeavePlan {
            departed_uuid: Uuid::from_u128(99),
            departed_username: DEPARTED_NAME.to_owned(),
            departed_entity_id: EntityId::new(300),
            recipients,
        }
    }

    fn movement_plan(
        previous_position: LobbyPosition,
        current_position: LobbyPosition,
        recipients: Vec<LobbyRecipient>,
    ) -> LobbyMovementPlan {
        LobbyMovementPlan {
            moving_session_id: LobbySessionId::new(99),
            moving_entity_id: EntityId::new(300),
            previous_position,
            current_position,
            recipients,
        }
    }

    #[test]
    fn empty_recipient_list_yields_no_packet_batches() {
        assert!(leave_visibility_batches(&plan(Vec::new())).is_empty());
    }

    #[test]
    fn old_mid_and_current_recipients_get_compatible_packet_variants() {
        let batches = leave_visibility_batches(&plan(vec![
            recipient(1, ProtocolVersion::V1_8),
            recipient(2, ProtocolVersion::V1_19_4),
            recipient(3, ProtocolVersion::V1_21),
        ]));

        assert_eq!(batches.len(), 3);
        assert!(matches!(
            batches[0].packets.as_slice(),
            [
                PacketRegistry::DestroyEntities(_),
                PacketRegistry::PlayerInfoUpdate(_)
            ]
        ));
        assert!(matches!(
            batches[1].packets.as_slice(),
            [
                PacketRegistry::DestroyEntities(_),
                PacketRegistry::PlayerInfoRemove(_)
            ]
        ));
        assert!(matches!(
            batches[2].packets.as_slice(),
            [
                PacketRegistry::RemoveEntities(_),
                PacketRegistry::PlayerInfoRemove(_)
            ]
        ));
    }

    #[test]
    fn encodes_leave_visibility_for_oldest_supported_report() {
        let packets = leave_visibility_packets(
            ProtocolVersion::V1_7_2,
            Uuid::from_u128(99),
            DEPARTED_NAME,
            EntityId::new(300),
        );

        assert_destroy_entities_packet(packets, ProtocolVersion::V1_7_2, 19, &[1, 0, 0, 1, 44]);
        let packets = leave_visibility_packets(
            ProtocolVersion::V1_7_2,
            Uuid::from_u128(99),
            DEPARTED_NAME,
            EntityId::new(300),
        );
        assert_player_info_update_packet(
            packets,
            ProtocolVersion::V1_7_2,
            56,
            &[8, b'd', b'e', b'p', b'a', b'r', b't', b'e', b'd', 0, 0, 0],
        );
    }

    #[test]
    fn encodes_leave_visibility_for_v1_8() {
        assert_legacy_remove_bucket(ProtocolVersion::V1_8, 19, 56);
    }

    #[test]
    fn encodes_leave_visibility_for_v1_12_2_nearest_report() {
        assert_legacy_remove_bucket(ProtocolVersion::V1_12_2, 50, 46);
    }

    #[test]
    fn encodes_leave_visibility_for_v1_19_4() {
        assert_destroy_with_current_player_info_remove_bucket(ProtocolVersion::V1_19_4, 62, 57);
    }

    #[test]
    fn encodes_leave_visibility_for_v1_20_5() {
        assert_destroy_with_current_player_info_remove_bucket(ProtocolVersion::V1_20_5, 66, 61);
    }

    #[test]
    fn encodes_leave_visibility_for_v1_21_plus() {
        assert_current_remove_bucket(ProtocolVersion::V1_21, 66, 61);
    }

    #[test]
    fn encodes_leave_visibility_for_latest_report() {
        assert_current_remove_bucket(ProtocolVersion::V26_1, 77, 69);
    }

    #[test]
    fn movement_batches_include_old_mid_and_current_protocol_slices() {
        let batches = movement_visibility_batches(&movement_plan(
            LobbyPosition::new(1.0, 2.0, 3.0, 0.0, 0.0),
            LobbyPosition::new(1.5, 2.0, 3.0, 0.0, 0.0),
            vec![
                recipient(1, ProtocolVersion::V1_18_2),
                recipient(2, ProtocolVersion::V1_19_4),
                recipient(3, ProtocolVersion::V1_20_5),
                recipient(4, ProtocolVersion::V1_21),
            ],
        ));

        assert_eq!(batches.len(), 4);
        assert_eq!(
            batches
                .iter()
                .map(|batch| batch.recipient.protocol_version)
                .collect::<Vec<_>>(),
            vec![
                ProtocolVersion::V1_18_2,
                ProtocolVersion::V1_19_4,
                ProtocolVersion::V1_20_5,
                ProtocolVersion::V1_21,
            ]
        );
        assert!(
            batches.iter().all(|batch| matches!(
                batch.packets.as_slice(),
                [PacketRegistry::MoveEntityPos(_)]
            ))
        );
    }

    #[test]
    fn movement_packets_use_pos_rot_and_head_rotation_when_position_and_look_change() {
        let packets = movement_visibility_packets(
            ProtocolVersion::V1_21,
            EntityId::new(300),
            LobbyPosition::new(1.0, 2.0, 3.0, 0.0, 0.0),
            LobbyPosition::new(1.5, 1.75, 3.125, 90.0, 45.0),
        );

        assert!(matches!(
            packets.as_slice(),
            [
                PacketRegistry::MoveEntityPosRot(_),
                PacketRegistry::RotateHead(_)
            ]
        ));

        let mut packets = packets;
        let raw_packet = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_21)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(47));
        assert_eq!(
            raw_packet.data(),
            &[0xac, 0x02, 0x08, 0x00, 0xfc, 0x00, 0x02, 0x00, 64, 32, 1]
        );
        let raw_packet = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_21)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(72));
        assert_eq!(raw_packet.data(), &[0xac, 0x02, 64]);
    }

    #[test]
    fn movement_packets_use_rotation_only_when_only_look_changes() {
        let packets = movement_visibility_packets(
            ProtocolVersion::V1_21,
            EntityId::new(300),
            LobbyPosition::new(1.0, 2.0, 3.0, 0.0, 0.0),
            LobbyPosition::new(1.0, 2.0, 3.0, 180.0, -90.0),
        );

        assert!(matches!(
            packets.as_slice(),
            [
                PacketRegistry::MoveEntityRot(_),
                PacketRegistry::RotateHead(_)
            ]
        ));
    }

    #[test]
    fn movement_packets_encode_for_v1_19_4() {
        let packets = movement_visibility_packets(
            ProtocolVersion::V1_19_4,
            EntityId::new(300),
            LobbyPosition::new(1.0, 2.0, 3.0, 0.0, 0.0),
            LobbyPosition::new(1.5, 1.75, 3.125, 90.0, 45.0),
        );

        assert!(matches!(
            packets.as_slice(),
            [
                PacketRegistry::MoveEntityPosRot(_),
                PacketRegistry::RotateHead(_)
            ]
        ));

        let mut packets = packets;
        let raw_packet = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_19_4)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(44));
        assert_eq!(
            raw_packet.data(),
            &[0xac, 0x02, 0x08, 0x00, 0xfc, 0x00, 0x02, 0x00, 64, 32, 1]
        );

        let raw_packet = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_19_4)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(66));
        assert_eq!(raw_packet.data(), &[0xac, 0x02, 64]);
    }

    #[test]
    fn movement_packets_encode_for_v1_8_legacy_byte_delta() {
        let packets = movement_visibility_packets(
            ProtocolVersion::V1_8,
            EntityId::new(300),
            LobbyPosition::new(1.0, 2.0, 3.0, 0.0, 0.0),
            LobbyPosition::new(1.5, 1.75, 3.125, 90.0, 45.0),
        );

        assert!(matches!(
            packets.as_slice(),
            [
                PacketRegistry::MoveEntityPosRot(_),
                PacketRegistry::RotateHead(_)
            ]
        ));

        let mut packets = packets;
        let raw_packet = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_8)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(23));
        assert_eq!(raw_packet.data(), &[0xac, 0x02, 16, 248, 4, 64, 32, 1]);

        let raw_packet = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_8)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(25));
        assert_eq!(raw_packet.data(), &[0xac, 0x02, 64]);
    }

    #[test]
    fn movement_packets_encode_for_v1_7_2_legacy_int_entity_id() {
        let packets = movement_visibility_packets(
            ProtocolVersion::V1_7_2,
            EntityId::new(300),
            LobbyPosition::new(1.0, 2.0, 3.0, 0.0, 0.0),
            LobbyPosition::new(1.5, 1.75, 3.125, 90.0, 45.0),
        );

        let mut packets = packets;
        let raw_packet = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_7_2)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(23));
        assert_eq!(raw_packet.data(), &[0, 0, 1, 44, 16, 248, 4, 64, 32]);

        let raw_packet = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_7_2)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(25));
        assert_eq!(raw_packet.data(), &[0, 0, 1, 44, 64]);
    }

    #[test]
    fn movement_packets_encode_for_v1_18_2_nearest_report() {
        let packets = movement_visibility_packets(
            ProtocolVersion::V1_18_2,
            EntityId::new(300),
            LobbyPosition::new(1.0, 2.0, 3.0, 0.0, 0.0),
            LobbyPosition::new(1.5, 1.75, 3.125, 90.0, 45.0),
        );

        let mut packets = packets;
        let raw_packet = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_18_2)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(42));

        let raw_packet = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_18_2)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(62));
    }

    #[test]
    fn movement_packets_encode_for_v1_19_3() {
        let packets = movement_visibility_packets(
            ProtocolVersion::V1_19_3,
            EntityId::new(300),
            LobbyPosition::new(1.0, 2.0, 3.0, 0.0, 0.0),
            LobbyPosition::new(1.5, 1.75, 3.125, 90.0, 45.0),
        );

        let mut packets = packets;
        let raw_packet = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_19_3)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(40));

        let raw_packet = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_19_3)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(62));
    }

    #[test]
    fn movement_packets_encode_for_v1_20_5() {
        let packets = movement_visibility_packets(
            ProtocolVersion::V1_20_5,
            EntityId::new(300),
            LobbyPosition::new(1.0, 2.0, 3.0, 0.0, 0.0),
            LobbyPosition::new(1.5, 1.75, 3.125, 90.0, 45.0),
        );

        let mut packets = packets;
        let raw_packet = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_20_5)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(47));

        let raw_packet = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_20_5)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(72));
    }

    #[test]
    fn movement_packets_use_legacy_teleport_for_large_delta_on_v1_21() {
        let packets = movement_visibility_packets(
            ProtocolVersion::V1_21,
            EntityId::new(300),
            LobbyPosition::new(0.0, 0.0, 0.0, 0.0, 0.0),
            LobbyPosition::new(8.1, 64.0, -2.25, 90.0, 45.0),
        );

        assert!(matches!(
            packets.as_slice(),
            [PacketRegistry::TeleportEntity(_)]
        ));

        let raw_packet = packets
            .into_iter()
            .next()
            .unwrap()
            .encode_packet(ProtocolVersion::V1_21)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(112));
        assert_eq!(raw_packet.data().len(), 29);
        assert_eq!(&raw_packet.data()[0..2], &[0xac, 0x02]);
        assert_eq!(&raw_packet.data()[26..29], &[64, 32, 1]);
    }

    #[test]
    fn movement_packets_use_position_sync_for_large_delta_after_v1_21_2() {
        let packets = movement_visibility_packets(
            ProtocolVersion::V26_1,
            EntityId::new(300),
            LobbyPosition::new(0.0, 0.0, 0.0, 0.0, 0.0),
            LobbyPosition::new(8.1, 64.0, -2.25, 90.0, 45.0),
        );

        assert!(matches!(
            packets.as_slice(),
            [PacketRegistry::EntityPositionSync(_)]
        ));

        let raw_packet = packets
            .into_iter()
            .next()
            .unwrap()
            .encode_packet(ProtocolVersion::V26_1)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(35));
        assert_eq!(raw_packet.data().len(), 63);
        assert_eq!(&raw_packet.data()[0..2], &[0xac, 0x02]);
        assert_eq!(&raw_packet.data()[58..63], &[0, 0, 0, 0, 1]);
    }

    #[test]
    fn metadata_batches_include_v1_7_2_through_current() {
        let plan = LobbyMetadataPlan {
            session_id: LobbySessionId::new(99),
            entity_id: EntityId::new(300),
            crouching: true,
            recipients: vec![
                recipient(1, ProtocolVersion::V1_7_2),
                recipient(2, ProtocolVersion::V1_8),
                recipient(3, ProtocolVersion::V1_20_5),
                recipient(4, ProtocolVersion::V1_21),
            ],
        };

        let batches = metadata_visibility_batches(&plan);

        assert_eq!(batches.len(), 4);
        assert_eq!(
            batches[0].recipient.protocol_version,
            ProtocolVersion::V1_7_2
        );
        assert_eq!(batches[1].recipient.protocol_version, ProtocolVersion::V1_8);
        assert_eq!(
            batches[2].recipient.protocol_version,
            ProtocolVersion::V1_20_5
        );
        assert_eq!(
            batches[3].recipient.protocol_version,
            ProtocolVersion::V1_21
        );
        assert!(
            batches
                .iter()
                .all(|b| matches!(b.packets.as_slice(), [PacketRegistry::SetEntityMetadata(_)]))
        );
    }

    #[test]
    fn metadata_packets_encode_crouching_base_flags() {
        let plan = LobbyMetadataPlan {
            session_id: LobbySessionId::new(99),
            entity_id: EntityId::new(300),
            crouching: true,
            recipients: Vec::new(),
        };
        let raw_packet = metadata_visibility_packets(&plan)
            .remove(0)
            .encode_packet(ProtocolVersion::V1_21)
            .unwrap();

        assert_eq!(raw_packet.packet_id(), Some(88));
        assert_eq!(
            raw_packet.data(),
            &[0xac, 0x02, 0, 0, 0x02, 6, 21, 5, 17, 0, 0x7f, 0xff]
        );
    }

    fn assert_legacy_remove_bucket(
        protocol_version: ProtocolVersion,
        destroy_packet_id: u8,
        player_info_packet_id: u8,
    ) {
        let packets = leave_visibility_packets(
            protocol_version,
            Uuid::from_u128(99),
            DEPARTED_NAME,
            EntityId::new(300),
        );
        assert_destroy_entities_packet(packets, protocol_version, destroy_packet_id, &[1, 172, 2]);

        let mut player_info_data = vec![4, 1];
        player_info_data.extend_from_slice(&LEGACY_UUID_LONGS);
        let packets = leave_visibility_packets(
            protocol_version,
            Uuid::from_u128(99),
            DEPARTED_NAME,
            EntityId::new(300),
        );
        assert_player_info_update_packet(
            packets,
            protocol_version,
            player_info_packet_id,
            &player_info_data,
        );
    }

    fn assert_current_remove_bucket(
        protocol_version: ProtocolVersion,
        remove_entities_packet_id: u8,
        player_info_remove_packet_id: u8,
    ) {
        let packets = leave_visibility_packets(
            protocol_version,
            Uuid::from_u128(99),
            DEPARTED_NAME,
            EntityId::new(300),
        );
        assert_remove_entities_packet(packets, protocol_version, remove_entities_packet_id);

        let mut player_info_data = vec![1];
        player_info_data.extend_from_slice(&UUID_BYTES);
        let packets = leave_visibility_packets(
            protocol_version,
            Uuid::from_u128(99),
            DEPARTED_NAME,
            EntityId::new(300),
        );
        assert_player_info_remove_packet(
            packets,
            protocol_version,
            player_info_remove_packet_id,
            &player_info_data,
        );
    }

    fn assert_destroy_with_current_player_info_remove_bucket(
        protocol_version: ProtocolVersion,
        destroy_packet_id: u8,
        player_info_remove_packet_id: u8,
    ) {
        let packets = leave_visibility_packets(
            protocol_version,
            Uuid::from_u128(99),
            DEPARTED_NAME,
            EntityId::new(300),
        );
        assert_destroy_entities_packet(packets, protocol_version, destroy_packet_id, &[1, 172, 2]);

        let mut player_info_data = vec![1];
        player_info_data.extend_from_slice(&UUID_BYTES);
        let packets = leave_visibility_packets(
            protocol_version,
            Uuid::from_u128(99),
            DEPARTED_NAME,
            EntityId::new(300),
        );
        assert_player_info_remove_packet(
            packets,
            protocol_version,
            player_info_remove_packet_id,
            &player_info_data,
        );
    }

    fn assert_destroy_entities_packet(
        mut packets: Vec<PacketRegistry>,
        protocol_version: ProtocolVersion,
        packet_id: u8,
        data: &[u8],
    ) {
        let raw_packet = packets.remove(0).encode_packet(protocol_version).unwrap();
        assert_eq!(raw_packet.packet_id(), Some(packet_id));
        assert_eq!(raw_packet.data(), data);
    }

    fn assert_remove_entities_packet(
        mut packets: Vec<PacketRegistry>,
        protocol_version: ProtocolVersion,
        packet_id: u8,
    ) {
        let raw_packet = packets.remove(0).encode_packet(protocol_version).unwrap();
        assert_eq!(raw_packet.packet_id(), Some(packet_id));
        assert_eq!(raw_packet.data(), &[1, 172, 2]);
    }

    fn assert_player_info_update_packet(
        mut packets: Vec<PacketRegistry>,
        protocol_version: ProtocolVersion,
        packet_id: u8,
        data: &[u8],
    ) {
        let raw_packet = packets.remove(1).encode_packet(protocol_version).unwrap();
        assert_eq!(raw_packet.packet_id(), Some(packet_id));
        assert_eq!(raw_packet.data(), data);
    }

    fn assert_player_info_remove_packet(
        mut packets: Vec<PacketRegistry>,
        protocol_version: ProtocolVersion,
        packet_id: u8,
        data: &[u8],
    ) {
        let raw_packet = packets.remove(1).encode_packet(protocol_version).unwrap();
        assert_eq!(raw_packet.packet_id(), Some(packet_id));
        assert_eq!(raw_packet.data(), data);
    }

    fn make_session(session_id: u64, uuid_val: u128, entity_id: i32) -> LobbySession {
        let mut session = LobbySession::new(
            Uuid::from_u128(uuid_val),
            format!("player{session_id}"),
            None,
            ProtocolVersion::V1_21,
            LobbyPosition::new(1.0, 64.0, 2.0, 90.0, 0.0),
        );
        session.session_id = LobbySessionId::new(session_id);
        session.entity_id = EntityId::new(entity_id);
        session
    }

    fn join_plan(new: LobbySession, existing: Vec<LobbySession>) -> LobbyJoinPlan {
        let existing_recipients = existing
            .iter()
            .map(|s| LobbyRecipient {
                session_id: s.session_id,
                uuid: s.uuid,
                entity_id: s.entity_id,
                protocol_version: s.protocol_version,
            })
            .collect();
        LobbyJoinPlan {
            new_session: new,
            existing_sessions: existing,
            existing_recipients,
        }
    }

    #[test]
    fn join_newcomer_packets_empty_when_no_existing_players() {
        let new = make_session(1, 1, 10);
        let plan = join_plan(new, Vec::new());
        let packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_21);
        assert!(packets.is_empty());
    }

    #[test]
    fn join_newcomer_v1_20_5_gets_current_spawn_entity_packets() {
        let new = make_session(1, 1, 10);
        let existing = make_session(2, 2, 20);
        let plan = join_plan(new, vec![existing]);
        let packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_20_5);
        assert_eq!(packets.len(), 4);
        assert!(matches!(packets[0], PacketRegistry::PlayerInfoUpdate(_)));
        assert!(matches!(packets[1], PacketRegistry::SpawnEntity(_)));
        assert!(matches!(packets[2], PacketRegistry::SetEntityMetadata(_)));
        assert!(matches!(packets[3], PacketRegistry::RotateHead(_)));
    }

    #[test]
    fn join_newcomer_gets_four_packets_per_existing_player() {
        let new = make_session(1, 1, 10);
        let existing = make_session(2, 2, 20);
        let plan = join_plan(new, vec![existing]);
        let packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_21);
        assert_eq!(
            packets.len(),
            4,
            "expected PlayerInfo + Spawn + Metadata + HeadRotation"
        );
        assert!(matches!(packets[0], PacketRegistry::PlayerInfoUpdate(_)));
        assert!(matches!(packets[1], PacketRegistry::SpawnEntity(_)));
        assert!(matches!(packets[2], PacketRegistry::SetEntityMetadata(_)));
        assert!(matches!(packets[3], PacketRegistry::RotateHead(_)));
    }

    #[test]
    fn join_newcomer_packets_scale_with_existing_player_count() {
        let new = make_session(1, 1, 10);
        let existing = vec![make_session(2, 2, 20), make_session(3, 3, 30)];
        let plan = join_plan(new, existing);
        let packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_21);
        assert_eq!(
            packets.len(),
            8,
            "4 packets per existing player * 2 players"
        );
    }

    #[test]
    fn join_existing_batches_empty_when_no_existing_recipients() {
        let new = make_session(1, 1, 10);
        let plan = join_plan(new, Vec::new());
        let batches = join_visibility_batches_for_existing(&plan);
        assert!(batches.is_empty());
    }

    #[test]
    fn join_existing_batches_include_v1_20_5_recipients() {
        let new = make_session(1, 1, 10);
        let mut existing = make_session(2, 2, 20);
        existing.protocol_version = ProtocolVersion::V1_20_5;
        let plan = join_plan(new, vec![existing]);
        let batches = join_visibility_batches_for_existing(&plan);
        assert_eq!(batches.len(), 1);
        assert!(matches!(
            batches[0].packets[1],
            PacketRegistry::SpawnEntity(_)
        ));
    }

    #[test]
    fn join_existing_batches_include_four_packets_for_1_21_recipients() {
        let new = make_session(1, 1, 10);
        let existing = make_session(2, 2, 20);
        let plan = join_plan(new, vec![existing]);
        let batches = join_visibility_batches_for_existing(&plan);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].packets.len(), 4);
        assert!(matches!(
            batches[0].packets[0],
            PacketRegistry::PlayerInfoUpdate(_)
        ));
        assert!(matches!(
            batches[0].packets[1],
            PacketRegistry::SpawnEntity(_)
        ));
    }

    #[test]
    fn spawn_entity_packet_encodes_for_v1_21() {
        let new = make_session(1, 1, 300);
        let existing = make_session(2, 2, 400);
        let plan = join_plan(new, vec![existing]);
        let mut packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_21);
        let spawn = packets.remove(1);
        let raw = spawn.encode_packet(ProtocolVersion::V1_21).unwrap();
        assert_eq!(raw.packet_id(), Some(1));
        let data = raw.data();
        assert_eq!(data.len(), 54);
        // entity_id 400 = VarInt [0x90, 0x03]
        assert_eq!(&data[0..2], &[0x90, 0x03]);
        // entity type 128 = VarInt [0x80, 0x01]
        assert_eq!(&data[18..20], &[0x80, 0x01]);
        // data VarInt + legacy zero velocity as three shorts
        assert_eq!(&data[47..54], &[0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn spawn_entity_packet_encodes_for_v1_20_2() {
        let new = make_session(1, 1, 300);
        let existing = make_session(2, 2, 400);
        let plan = join_plan(new, vec![existing]);
        let mut packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_20_2);
        let spawn = packets.remove(1);
        let raw = spawn.encode_packet(ProtocolVersion::V1_20_2).unwrap();
        assert_eq!(raw.packet_id(), Some(1));
        let data = raw.data();
        assert_eq!(data.len(), 53);
        // entity type 122 = VarInt [0x7a]
        assert_eq!(data[18], 0x7a);
        assert_eq!(data[45], 64); // head yaw
        // data VarInt + legacy zero velocity as three shorts
        assert_eq!(&data[46..53], &[0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn spawn_entity_packet_encodes_for_v1_20_3() {
        let new = make_session(1, 1, 300);
        let existing = make_session(2, 2, 400);
        let plan = join_plan(new, vec![existing]);
        let mut packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_20_3);
        let spawn = packets.remove(1);
        let raw = spawn.encode_packet(ProtocolVersion::V1_20_3).unwrap();
        assert_eq!(raw.packet_id(), Some(1));
        let data = raw.data();
        assert_eq!(data.len(), 53);
        // entity type 124 = VarInt [0x7c]
        assert_eq!(data[18], 0x7c);
        assert_eq!(data[45], 64); // head yaw
    }

    #[test]
    fn spawn_entity_packet_encodes_for_v1_20_5() {
        let new = make_session(1, 1, 300);
        let existing = make_session(2, 2, 400);
        let plan = join_plan(new, vec![existing]);
        let mut packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_20_5);
        let spawn = packets.remove(1);
        let raw = spawn.encode_packet(ProtocolVersion::V1_20_5).unwrap();
        assert_eq!(raw.packet_id(), Some(1));
        let data = raw.data();
        assert_eq!(data.len(), 54);
        // entity type 128 = VarInt [0x80, 0x01]
        assert_eq!(&data[18..20], &[0x80, 0x01]);
    }

    #[test]
    fn spawn_entity_packet_encodes_for_v26_1() {
        let new = make_session(1, 1, 300);
        // entity_id=1 encodes as 1-byte VarInt [0x01], uuid=16 bytes, entity type at offset 17
        let existing = make_session(2, 2, 1);
        let plan = join_plan(new, vec![existing]);
        let mut packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V26_1);
        let spawn = packets.remove(1);
        let raw = spawn.encode_packet(ProtocolVersion::V26_1).unwrap();
        assert_eq!(raw.packet_id(), Some(1));
        let data = raw.data();
        assert_eq!(data.len(), 48);
        // entity type 155 = VarInt [0x9b, 0x01]
        assert_eq!(&data[17..19], &[0x9b, 0x01]);
        // 1.21.9+ moved low-precision Vec3 velocity before rotations and keeps data as a VarInt.
        assert_eq!(&data[43..48], &[0, 0, 64, 64, 0]);
    }

    #[test]
    fn join_spawn_metadata_reflects_current_crouching_state() {
        let new = make_session(1, 1, 300);
        let mut existing = make_session(2, 2, 400);
        existing.crouching = true;
        let plan = join_plan(new, vec![existing]);
        let mut packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_21);
        let raw = packets
            .remove(2)
            .encode_packet(ProtocolVersion::V1_21)
            .unwrap();

        assert_eq!(raw.packet_id(), Some(88));
        assert_eq!(
            raw.data(),
            &[0x90, 0x03, 0, 0, 0x02, 6, 21, 5, 17, 0, 0x7f, 0xff]
        );
    }

    // --- Legacy join visibility (V1_8 through V1_20) ---

    #[test]
    fn join_newcomer_v1_8_gets_four_packets_with_spawn_player() {
        let new = make_session(1, 1, 10);
        let existing = make_session(2, 2, 20);
        let plan = join_plan(new, vec![existing]);
        let packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_8);
        assert_eq!(packets.len(), 4);
        assert!(matches!(packets[0], PacketRegistry::PlayerInfoUpdate(_)));
        assert!(matches!(packets[1], PacketRegistry::SpawnPlayer(_)));
        assert!(matches!(packets[2], PacketRegistry::SetEntityMetadata(_)));
        assert!(matches!(packets[3], PacketRegistry::RotateHead(_)));
    }

    #[test]
    fn join_newcomer_v1_12_2_gets_spawn_player_packet() {
        let new = make_session(1, 1, 10);
        let existing = make_session(2, 2, 20);
        let plan = join_plan(new, vec![existing]);
        let mut packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_12_2);
        let spawn = packets.remove(1);
        let raw = spawn.encode_packet(ProtocolVersion::V1_12_2).unwrap();
        // minecraft:add_player for V1_12 = ID 5
        assert_eq!(raw.packet_id(), Some(5));
        // data = entity_id (VarInt 20=[0x14]) + uuid (16 bytes) + x,y,z (3*f64=24 bytes) +
        //        yaw, pitch (2 bytes) + empty metadata list terminator.
        assert_eq!(raw.data().len(), 44);
        assert_eq!(raw.data()[43], 0xFF);
    }

    #[test]
    fn join_newcomer_v1_14_4_gets_spawn_player_packet_with_empty_metadata_list() {
        let new = make_session(1, 1, 10);
        let existing = make_session(2, 2, 20);
        let plan = join_plan(new, vec![existing]);
        let mut packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_14_4);
        let spawn = packets.remove(1);
        let raw = spawn.encode_packet(ProtocolVersion::V1_14_4).unwrap();
        // minecraft:add_player for V1_14 = ID 5
        assert_eq!(raw.packet_id(), Some(5));
        assert_eq!(raw.data().len(), 44);
        assert_eq!(raw.data()[43], 0xFF);
    }

    #[test]
    fn join_newcomer_v1_19_4_gets_spawn_player_packet_without_metadata_terminator() {
        let new = make_session(1, 1, 10);
        let existing = make_session(2, 2, 20);
        let plan = join_plan(new, vec![existing]);
        let mut packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_19_4);
        let spawn = packets.remove(1);
        let raw = spawn.encode_packet(ProtocolVersion::V1_19_4).unwrap();
        // minecraft:add_player for V1_19_4 = ID 3
        assert_eq!(raw.packet_id(), Some(3));
        // data = entity_id (VarInt 20=[0x14]) + uuid (16 bytes) + x,y,z (3×f64=24 bytes) +
        //        yaw, pitch (2 bytes) — no metadata terminator in 1.19.4+
        assert_eq!(raw.data().len(), 43);
    }

    #[test]
    fn join_newcomer_v1_20_gets_spawn_player_packet() {
        let new = make_session(1, 1, 10);
        let existing = make_session(2, 2, 20);
        let plan = join_plan(new, vec![existing]);
        let mut packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_20);
        let spawn = packets.remove(1);
        let raw = spawn.encode_packet(ProtocolVersion::V1_20).unwrap();
        // minecraft:add_player for V1_20 = ID 3
        assert_eq!(raw.packet_id(), Some(3));
        assert_eq!(raw.data().len(), 43);
    }

    #[test]
    fn join_newcomer_v1_7_2_gets_legacy_spawn_player_packet() {
        let new = make_session(1, 1, 10);
        let existing = make_session(2, 2, 20);
        let plan = join_plan(new, vec![existing]);
        let packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_7_2);

        assert_eq!(packets.len(), 4);
        assert!(matches!(packets[0], PacketRegistry::PlayerInfoUpdate(_)));
        assert!(matches!(packets[1], PacketRegistry::SpawnPlayer(_)));
        assert!(matches!(packets[2], PacketRegistry::SetEntityMetadata(_)));
        assert!(matches!(packets[3], PacketRegistry::RotateHead(_)));
    }

    #[test]
    fn npc_spawn_keeps_player_info_for_v1_9_through_v1_12_2_interactions() {
        for version in [
            ProtocolVersion::V1_9,
            ProtocolVersion::V1_9_3,
            ProtocolVersion::V1_10,
            ProtocolVersion::V1_11,
            ProtocolVersion::V1_12_2,
        ] {
            let packets = npc_spawn_packets_for_join(&npc_spawn_plan(), version);

            assert_eq!(packets.len(), 4, "{version:?}");
            assert!(matches!(packets[0], PacketRegistry::PlayerInfoUpdate(_)));
            assert!(matches!(packets[1], PacketRegistry::SpawnPlayer(_)));
            assert!(matches!(packets[2], PacketRegistry::SetEntityMetadata(_)));
            assert!(matches!(packets[3], PacketRegistry::RotateHead(_)));
        }
    }

    #[test]
    fn npc_spawn_still_removes_player_info_outside_v1_9_through_v1_12_2() {
        for version in [
            ProtocolVersion::V1_8,
            ProtocolVersion::V1_13,
            ProtocolVersion::V1_14_4,
            ProtocolVersion::V1_15_2,
        ] {
            let packets = npc_spawn_packets_for_join(&npc_spawn_plan(), version);

            assert_eq!(packets.len(), 5, "{version:?}");
            assert!(matches!(
                packets.last(),
                Some(PacketRegistry::PlayerInfoUpdate(_))
            ));
        }
    }

    #[test]
    fn join_existing_batches_include_legacy_v1_8_through_v1_20() {
        let new = make_session(1, 1, 10);
        let mut r1 = make_session(2, 2, 20);
        r1.protocol_version = ProtocolVersion::V1_7_2;
        let mut r2 = make_session(3, 3, 30);
        r2.protocol_version = ProtocolVersion::V1_8;
        let mut r3 = make_session(4, 4, 40);
        r3.protocol_version = ProtocolVersion::V1_19_4;
        let mut r4 = make_session(5, 5, 50);
        r4.protocol_version = ProtocolVersion::V1_20;
        let mut r5 = make_session(6, 6, 60);
        r5.protocol_version = ProtocolVersion::V1_20_2;
        let mut r6 = make_session(7, 7, 70);
        r6.protocol_version = ProtocolVersion::V1_21;

        let plan = join_plan(new, vec![r1, r2, r3, r4, r5, r6]);
        let batches = join_visibility_batches_for_existing(&plan);

        // V1_7_2, V1_8, V1_19_4, V1_20, V1_20_2, V1_21 → 6 batches
        assert_eq!(batches.len(), 6);
        assert_eq!(
            batches[0].recipient.protocol_version,
            ProtocolVersion::V1_7_2
        );
        assert_eq!(batches[1].recipient.protocol_version, ProtocolVersion::V1_8);
        assert_eq!(
            batches[2].recipient.protocol_version,
            ProtocolVersion::V1_19_4
        );
        assert_eq!(
            batches[3].recipient.protocol_version,
            ProtocolVersion::V1_20
        );
        assert_eq!(
            batches[4].recipient.protocol_version,
            ProtocolVersion::V1_20_2
        );
        assert_eq!(
            batches[5].recipient.protocol_version,
            ProtocolVersion::V1_21
        );

        // V1_7_2, V1_8, V1_19_4, and V1_20 use SpawnPlayer; V1_20_2+ use SpawnEntity.
        assert!(matches!(
            batches[0].packets[1],
            PacketRegistry::SpawnPlayer(_)
        ));
        assert!(matches!(
            batches[1].packets[1],
            PacketRegistry::SpawnPlayer(_)
        ));
        assert!(matches!(
            batches[2].packets[1],
            PacketRegistry::SpawnPlayer(_)
        ));
        assert!(matches!(
            batches[3].packets[1],
            PacketRegistry::SpawnPlayer(_)
        ));
        assert!(matches!(
            batches[4].packets[1],
            PacketRegistry::SpawnEntity(_)
        ));
        assert!(matches!(
            batches[5].packets[1],
            PacketRegistry::SpawnEntity(_)
        ));
    }

    #[test]
    fn join_newcomer_v1_8_spawn_player_uses_fixed_point_coordinates() {
        let new = make_session(1, 1, 10);
        let mut existing = make_session(2, 2, 20);
        existing.position = LobbyPosition::new(10.5, 64.0, -5.0, 90.0, 0.0);
        let plan = join_plan(new, vec![existing]);
        let mut packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_8);
        let spawn = packets.remove(1);
        let raw = spawn.encode_packet(ProtocolVersion::V1_8).unwrap();
        // minecraft:add_player for V1_8 = ID 12
        assert_eq!(raw.packet_id(), Some(12));
        let data = raw.data();
        // entity_id = 20 → VarInt [0x14], uuid = 16 bytes
        // x = (10.5*32).floor() = 336 = 0x00_00_01_50
        assert_eq!(&data[17..21], [0x00, 0x00, 0x01, 0x50]);
        // y = (64.0*32).floor() = 2048 = 0x00_00_08_00
        assert_eq!(&data[21..25], [0x00, 0x00, 0x08, 0x00]);
        // z = (-5.0*32).floor() = -160 = 0xFF_FF_FF_60
        assert_eq!(&data[25..29], [0xFF, 0xFF, 0xFF, 0x60]);
        // yaw = 90° → 64
        assert_eq!(data[29], 64);
        // current_item = 0 (i16)
        assert_eq!(&data[31..33], [0, 0]);
        // metadata terminator = 0x7F for 1.8
        assert_eq!(data[33], 0x7F);
    }

    #[test]
    fn join_newcomer_v1_7_2_spawn_player_uses_named_entity_shape() {
        let new = make_session(1, 1, 10);
        let mut existing = make_session(2, 2, 20);
        existing.position = LobbyPosition::new(10.5, 64.0, -5.0, 90.0, 0.0);
        existing.crouching = true;
        let plan = join_plan(new, vec![existing]);
        let mut packets = join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_7_2);
        let spawn = packets.remove(1);
        let raw = spawn.encode_packet(ProtocolVersion::V1_7_2).unwrap();

        assert_eq!(raw.packet_id(), Some(12));
        let data = raw.data();
        assert_eq!(data[0], 20); // entity_id VarInt
        assert_eq!(data[1], 32); // dashless UUID string length
        assert_eq!(data[34], 7); // username length
        assert_eq!(&data[35..42], b"player2");
        assert_eq!(data[42], 0); // profile property count
        assert_eq!(&data[43..47], [0x00, 0x00, 0x01, 0x50]);
        assert_eq!(&data[47..51], [0x00, 0x00, 0x08, 0x00]);
        assert_eq!(&data[51..55], [0xFF, 0xFF, 0xFF, 0x60]);
        assert_eq!(&data[59..62], [0, 0x02, 0x7f]);
    }

    fn npc_spawn_plan() -> LobbyNpcSpawnPlan {
        let mut npc = LobbyNpc::player(
            "survival-npc",
            "survival",
            "Survival",
            LobbyPosition::new(1.0, 64.0, 2.0, 90.0, 0.0),
        );
        npc.entity_id = EntityId::new(300);
        LobbyNpcSpawnPlan { npcs: vec![npc] }
    }
}
