use crate::server::packet_registry::PacketRegistry;
use crate::server_state::{
    EntityId, LobbyJoinPlan, LobbyLeavePlan, LobbyMetadataPlan, LobbyNpc, LobbyNpcKind,
    LobbyNpcSpawnPlan, LobbyPosition, LobbyRecipient, LobbySpawnInfo,
};
#[cfg(test)]
use crate::server_state::{LobbyMovementPlan, LobbySwingPlan};
use minecraft_packets::play::animate_packet::AnimatePacket;
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

#[cfg(test)]
pub struct LobbyMovementPacketBatch {
    #[allow(dead_code)]
    pub recipient: LobbyRecipient,
    pub packets: Vec<PacketRegistry>,
}

#[cfg(test)]
pub struct LobbyMetadataPacketBatch {
    #[allow(dead_code)]
    pub recipient: LobbyRecipient,
    pub packets: Vec<PacketRegistry>,
}

#[cfg(test)]
pub struct LobbySwingPacketBatch {
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
pub fn swing_visibility_batches(plan: &LobbySwingPlan) -> Vec<LobbySwingPacketBatch> {
    plan.recipients
        .iter()
        .filter(|recipient| {
            recipient
                .protocol_version
                .is_after_inclusive(ProtocolVersion::V1_7_2)
        })
        .map(|recipient| LobbySwingPacketBatch {
            recipient: recipient.clone(),
            packets: swing_visibility_packets(plan.swinging_entity_id),
        })
        .collect()
}

pub fn swing_visibility_packets(swinging_entity_id: EntityId) -> Vec<PacketRegistry> {
    vec![PacketRegistry::Animate(AnimatePacket::main_hand(
        swinging_entity_id.get(),
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

    // NPCs keep their player-list entry for the whole session: the client loads
    // a fake player's skin from that entry, and removing it (even a tick later)
    // races against the skin being applied, leaving the NPC with the default
    // Steve/Alex skin. On 1.19.3+ the entry is sent "unlisted" (`listed = false`)
    // so the NPC stays out of the tab list; older clients have no such flag, so
    // the NPC necessarily appears in the tab list there.
    let spawn = LobbySpawnInfo {
        uuid: npc.uuid,
        username: npc.name.clone(),
        textures: npc.textures.clone(),
        entity_id: npc.entity_id,
        position: npc.position,
        crouching: false,
        listed: false,
    };

    if recipient_version.is_after_inclusive(ProtocolVersion::V1_20_2) {
        player_spawn_packets_current(&spawn)
    } else if recipient_version.is_after_inclusive(ProtocolVersion::V1_7_2)
        && recipient_version.is_before_inclusive(ProtocolVersion::V1_20)
    {
        player_spawn_packets_legacy(&spawn, recipient_version)
    } else {
        Vec::new()
    }
}

fn player_info_packet(player: &LobbySpawnInfo) -> PacketRegistry {
    player.textures.as_ref().map_or_else(
        || {
            PacketRegistry::PlayerInfoUpdate(PlayerInfoUpdatePacket::skinless(
                player.username.clone(),
                player.uuid,
                player.listed,
            ))
        },
        |textures| {
            PacketRegistry::PlayerInfoUpdate(PlayerInfoUpdatePacket::skin(
                player.username.clone(),
                player.uuid,
                textures.clone(),
                player.listed,
            ))
        },
    )
}

fn player_spawn_packets_current(player: &LobbySpawnInfo) -> Vec<PacketRegistry> {
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
    player: &LobbySpawnInfo,
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
    use minecraft_packets::login::Property;

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
            lifecycle_message_recipients: recipients.clone(),
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

    fn swing_plan(recipients: Vec<LobbyRecipient>) -> LobbySwingPlan {
        LobbySwingPlan {
            swinging_session_id: LobbySessionId::new(99),
            swinging_entity_id: EntityId::new(300),
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
    fn leave_visibility_packets_encode_per_version_bucket() {
        type LeaveEncodingCase = (ProtocolVersion, u8, &'static [u8], u8, &'static [u8]);

        // (version, entity_remove_id, entity_remove_data, player_info_id, player_info_data)
        const V1_7_2_ENTITY_DATA: &[u8] = &[1, 0, 0, 1, 44]; // int entity_id
        const VARINT_ENTITY_DATA: &[u8] = &[1, 172, 2]; // varint 300
        const V1_7_2_INFO_DATA: &[u8] =
            &[8, b'd', b'e', b'p', b'a', b'r', b't', b'e', b'd', 0, 0, 0];
        const LEGACY_INFO_DATA: &[u8] = &[4, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 99];
        const CURRENT_INFO_DATA: &[u8] = &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 99];

        let cases: &[LeaveEncodingCase] = &[
            (
                ProtocolVersion::V1_7_2,
                19,
                V1_7_2_ENTITY_DATA,
                56,
                V1_7_2_INFO_DATA,
            ),
            (
                ProtocolVersion::V1_8,
                19,
                VARINT_ENTITY_DATA,
                56,
                LEGACY_INFO_DATA,
            ),
            (
                ProtocolVersion::V1_12_2,
                50,
                VARINT_ENTITY_DATA,
                46,
                LEGACY_INFO_DATA,
            ),
            (
                ProtocolVersion::V1_19_4,
                62,
                VARINT_ENTITY_DATA,
                57,
                CURRENT_INFO_DATA,
            ),
            (
                ProtocolVersion::V1_20_5,
                66,
                VARINT_ENTITY_DATA,
                61,
                CURRENT_INFO_DATA,
            ),
            (
                ProtocolVersion::V1_21,
                66,
                VARINT_ENTITY_DATA,
                61,
                CURRENT_INFO_DATA,
            ),
            (
                ProtocolVersion::V26_1,
                77,
                VARINT_ENTITY_DATA,
                69,
                CURRENT_INFO_DATA,
            ),
        ];

        for &(version, remove_id, remove_data, info_id, info_data) in cases {
            let mut packets = leave_visibility_packets(
                version,
                Uuid::from_u128(99),
                DEPARTED_NAME,
                EntityId::new(300),
            );
            let raw = packets.remove(0).encode_packet(version).unwrap();
            assert_eq!(
                raw.packet_id(),
                Some(remove_id),
                "{version:?} entity remove id"
            );
            assert_eq!(raw.data(), remove_data, "{version:?} entity remove data");

            let mut packets = leave_visibility_packets(
                version,
                Uuid::from_u128(99),
                DEPARTED_NAME,
                EntityId::new(300),
            );
            let raw = packets.remove(1).encode_packet(version).unwrap();
            assert_eq!(raw.packet_id(), Some(info_id), "{version:?} player info id");
            assert_eq!(raw.data(), info_data, "{version:?} player info data");
        }
    }

    #[test]
    fn swing_visibility_batches_send_one_animate_per_recipient() {
        let batches = swing_visibility_batches(&swing_plan(vec![
            recipient(1, ProtocolVersion::V1_8),
            recipient(2, ProtocolVersion::V1_19_4),
            recipient(3, ProtocolVersion::V26_1),
        ]));

        assert_eq!(batches.len(), 3);
        assert!(
            batches
                .iter()
                .all(|batch| matches!(batch.packets.as_slice(), [PacketRegistry::Animate(_)]))
        );
    }

    #[test]
    fn swing_visibility_packets_encode_for_legacy_mid_modern_and_latest() {
        for (version, expected_id, expected_data) in [
            (ProtocolVersion::V1_7_2, 0x0b, &[0, 0, 1, 44, 0][..]),
            (ProtocolVersion::V1_8, 0x0b, &[172, 2, 0][..]),
            (ProtocolVersion::V1_12_2, 0x06, &[172, 2, 0][..]),
            (ProtocolVersion::V1_19_4, 0x04, &[172, 2, 0][..]),
            (ProtocolVersion::V1_20_5, 0x03, &[172, 2, 0][..]),
            (ProtocolVersion::V1_21, 0x03, &[172, 2, 0][..]),
            (ProtocolVersion::V1_21_4, 0x03, &[172, 2, 0][..]),
            (ProtocolVersion::V26_1, 0x02, &[172, 2, 0][..]),
        ] {
            let packet = swing_visibility_packets(EntityId::new(300)).pop().unwrap();
            let raw = packet.encode_packet(version).unwrap();
            assert_eq!(raw.packet_id(), Some(expected_id), "{version:?}");
            assert_eq!(raw.data(), expected_data, "{version:?}");
        }
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
        // Verify packet types and wire payload for two representative modern versions.
        // V1_21 and V1_19_4 produce identical data bytes; only the packet IDs differ.
        for (version, posrot_id, head_id) in [
            (ProtocolVersion::V1_21, 47u8, 72u8),
            (ProtocolVersion::V1_19_4, 44, 66),
        ] {
            let mut packets = movement_visibility_packets(
                version,
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
            let raw = packets.remove(0).encode_packet(version).unwrap();
            assert_eq!(raw.packet_id(), Some(posrot_id), "{version:?} posrot id");
            assert_eq!(
                raw.data(),
                &[0xac, 0x02, 0x08, 0x00, 0xfc, 0x00, 0x02, 0x00, 64, 32, 1]
            );
            let raw = packets.remove(0).encode_packet(version).unwrap();
            assert_eq!(raw.packet_id(), Some(head_id), "{version:?} head id");
            assert_eq!(raw.data(), &[0xac, 0x02, 64]);
        }
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
    fn movement_packets_encode_for_v1_8_legacy_byte_delta() {
        let mut packets = movement_visibility_packets(
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
        let raw = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_8)
            .unwrap();
        assert_eq!(raw.packet_id(), Some(23));
        assert_eq!(raw.data(), &[0xac, 0x02, 16, 248, 4, 64, 32, 1]);
        let raw = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_8)
            .unwrap();
        assert_eq!(raw.packet_id(), Some(25));
        assert_eq!(raw.data(), &[0xac, 0x02, 64]);
    }

    #[test]
    fn movement_packets_encode_for_v1_7_2_legacy_int_entity_id() {
        let mut packets = movement_visibility_packets(
            ProtocolVersion::V1_7_2,
            EntityId::new(300),
            LobbyPosition::new(1.0, 2.0, 3.0, 0.0, 0.0),
            LobbyPosition::new(1.5, 1.75, 3.125, 90.0, 45.0),
        );

        let raw = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_7_2)
            .unwrap();
        assert_eq!(raw.packet_id(), Some(23));
        assert_eq!(raw.data(), &[0, 0, 1, 44, 16, 248, 4, 64, 32]);
        let raw = packets
            .remove(0)
            .encode_packet(ProtocolVersion::V1_7_2)
            .unwrap();
        assert_eq!(raw.packet_id(), Some(25));
        assert_eq!(raw.data(), &[0, 0, 1, 44, 64]);
    }

    #[test]
    fn movement_packet_ids_by_version_bucket() {
        // Versions that share payload encoding but differ only by packet ID.
        for (version, posrot_id, head_id) in [
            (ProtocolVersion::V1_18_2, 42u8, 62u8),
            (ProtocolVersion::V1_19_3, 40, 62),
            (ProtocolVersion::V1_20_5, 47, 72),
        ] {
            let mut packets = movement_visibility_packets(
                version,
                EntityId::new(300),
                LobbyPosition::new(1.0, 2.0, 3.0, 0.0, 0.0),
                LobbyPosition::new(1.5, 1.75, 3.125, 90.0, 45.0),
            );
            let raw = packets.remove(0).encode_packet(version).unwrap();
            assert_eq!(raw.packet_id(), Some(posrot_id), "{version:?} posrot id");
            let raw = packets.remove(0).encode_packet(version).unwrap();
            assert_eq!(raw.packet_id(), Some(head_id), "{version:?} head id");
        }
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

    fn make_session(session_id: u64, uuid_val: u128, entity_id: i32) -> SessionForTest {
        SessionForTest {
            session_id: LobbySessionId::new(session_id),
            uuid: Uuid::from_u128(uuid_val),
            username: format!("player{session_id}"),
            entity_id: EntityId::new(entity_id),
            protocol_version: ProtocolVersion::V1_21,
            position: LobbyPosition::new(1.0, 64.0, 2.0, 90.0, 0.0),
            crouching: false,
        }
    }

    struct SessionForTest {
        session_id: LobbySessionId,
        uuid: Uuid,
        username: String,
        entity_id: EntityId,
        protocol_version: ProtocolVersion,
        position: LobbyPosition,
        crouching: bool,
    }

    impl SessionForTest {
        fn to_spawn(&self) -> LobbySpawnInfo {
            LobbySpawnInfo {
                uuid: self.uuid,
                username: self.username.clone(),
                textures: None,
                entity_id: self.entity_id,
                position: self.position,
                crouching: self.crouching,
                listed: true,
            }
        }

        fn to_recipient(&self) -> LobbyRecipient {
            LobbyRecipient {
                session_id: self.session_id,
                uuid: self.uuid,
                entity_id: self.entity_id,
                protocol_version: self.protocol_version,
            }
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    fn join_plan(new: SessionForTest, existing: Vec<SessionForTest>) -> LobbyJoinPlan {
        let existing_recipients = existing.iter().map(SessionForTest::to_recipient).collect();
        let existing_sessions = existing.iter().map(SessionForTest::to_spawn).collect();
        LobbyJoinPlan {
            new_session: new.to_spawn(),
            existing_sessions,
            existing_recipients,
        }
    }

    #[test]
    fn join_visibility_empty_without_counterparts() {
        let plan = join_plan(make_session(1, 1, 10), Vec::new());
        assert!(join_visibility_packets_for_newcomer(&plan, ProtocolVersion::V1_21).is_empty());
        assert!(join_visibility_batches_for_existing(&plan).is_empty());
    }

    #[test]
    fn join_newcomer_current_versions_get_four_spawn_entity_packets() {
        let new = make_session(1, 1, 10);
        let existing = make_session(2, 2, 20);
        let plan = join_plan(new, vec![existing]);
        for version in [ProtocolVersion::V1_20_5, ProtocolVersion::V1_21] {
            let packets = join_visibility_packets_for_newcomer(&plan, version);
            assert_eq!(packets.len(), 4, "{version:?}");
            assert!(matches!(packets[0], PacketRegistry::PlayerInfoUpdate(_)));
            assert!(matches!(packets[1], PacketRegistry::SpawnEntity(_)));
            assert!(matches!(packets[2], PacketRegistry::SetEntityMetadata(_)));
            assert!(matches!(packets[3], PacketRegistry::RotateHead(_)));
        }
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
    fn spawn_entity_packet_version_encoding() {
        // Each entry: (version, existing_entity_id, expected_data_len, type_id_offset, type_id_bytes, tail_offset, tail_bytes)
        struct Case {
            version: ProtocolVersion,
            entity_id: i32,
            len: usize,
            type_offset: usize,
            type_bytes: &'static [u8],
            tail_offset: usize,
            tail_bytes: &'static [u8],
        }
        let cases = [
            Case {
                version: ProtocolVersion::V1_20_2,
                entity_id: 400,
                len: 53,
                type_offset: 18,
                type_bytes: &[0x7a],
                tail_offset: 46,
                tail_bytes: &[0, 0, 0, 0, 0, 0, 0],
            },
            Case {
                version: ProtocolVersion::V1_20_3,
                entity_id: 400,
                len: 53,
                type_offset: 18,
                type_bytes: &[0x7c],
                tail_offset: 45,
                tail_bytes: &[64],
            },
            Case {
                version: ProtocolVersion::V1_20_5,
                entity_id: 400,
                len: 54,
                type_offset: 18,
                type_bytes: &[0x80, 0x01],
                tail_offset: 47,
                tail_bytes: &[0, 0, 0, 0, 0, 0, 0],
            },
            Case {
                version: ProtocolVersion::V1_21,
                entity_id: 400,
                len: 54,
                type_offset: 18,
                type_bytes: &[0x80, 0x01],
                tail_offset: 47,
                tail_bytes: &[0, 0, 0, 0, 0, 0, 0],
            },
            // V26_1: entity_id=1 is a 1-byte VarInt so type lands at offset 17
            Case {
                version: ProtocolVersion::V26_1,
                entity_id: 1,
                len: 48,
                type_offset: 17,
                type_bytes: &[0x9b, 0x01],
                tail_offset: 43,
                tail_bytes: &[0, 0, 64, 64, 0],
            },
        ];

        for case in &cases {
            let plan = join_plan(
                make_session(1, 1, 300),
                vec![make_session(2, 2, case.entity_id)],
            );
            let mut packets = join_visibility_packets_for_newcomer(&plan, case.version);
            let raw = packets.remove(1).encode_packet(case.version).unwrap();
            let data = raw.data();
            assert_eq!(raw.packet_id(), Some(1), "{:?} packet_id", case.version);
            assert_eq!(data.len(), case.len, "{:?} data len", case.version);
            assert_eq!(
                &data[case.type_offset..case.type_offset + case.type_bytes.len()],
                case.type_bytes,
                "{:?} entity type",
                case.version
            );
            assert_eq!(
                &data[case.tail_offset..case.tail_offset + case.tail_bytes.len()],
                case.tail_bytes,
                "{:?} tail",
                case.version
            );
        }
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
    fn join_newcomer_legacy_versions_get_four_spawn_player_packets() {
        let new = make_session(1, 1, 10);
        let existing = make_session(2, 2, 20);
        let plan = join_plan(new, vec![existing]);
        for version in [ProtocolVersion::V1_7_2, ProtocolVersion::V1_8] {
            let packets = join_visibility_packets_for_newcomer(&plan, version);
            assert_eq!(packets.len(), 4, "{version:?}");
            assert!(matches!(packets[0], PacketRegistry::PlayerInfoUpdate(_)));
            assert!(matches!(packets[1], PacketRegistry::SpawnPlayer(_)));
            assert!(matches!(packets[2], PacketRegistry::SetEntityMetadata(_)));
            assert!(matches!(packets[3], PacketRegistry::RotateHead(_)));
        }
    }

    #[test]
    fn legacy_spawn_player_packet_version_encoding() {
        // (version, expected_packet_id, expected_data_len, has_0xff_metadata_terminator)
        for (version, expected_id, expected_len, has_terminator) in [
            (ProtocolVersion::V1_12_2, 5u8, 44usize, true),
            (ProtocolVersion::V1_14_4, 5, 44, true),
            (ProtocolVersion::V1_19_4, 3, 43, false),
            (ProtocolVersion::V1_20, 3, 43, false),
        ] {
            let plan = join_plan(make_session(1, 1, 10), vec![make_session(2, 2, 20)]);
            let mut packets = join_visibility_packets_for_newcomer(&plan, version);
            let raw = packets.remove(1).encode_packet(version).unwrap();
            assert_eq!(raw.packet_id(), Some(expected_id), "{version:?} packet_id");
            assert_eq!(raw.data().len(), expected_len, "{version:?} data len");
            if has_terminator {
                assert_eq!(
                    raw.data()[expected_len - 1],
                    0xFF,
                    "{version:?} metadata terminator"
                );
            }
        }
    }

    #[test]
    fn npc_spawn_never_removes_player_info_entry() {
        // The NPC's player-list entry must persist for the whole session so the
        // client can load its skin; removing it races the skin and drops the NPC
        // to the default Steve/Alex skin. This held across every supported range.
        for version in [
            ProtocolVersion::V1_8,
            ProtocolVersion::V1_9,
            ProtocolVersion::V1_12_2,
            ProtocolVersion::V1_13,
            ProtocolVersion::V1_15_2,
            ProtocolVersion::V1_19_4,
            ProtocolVersion::V1_20,
            ProtocolVersion::V1_20_2,
        ] {
            let packets = npc_spawn_packets_for_join(&npc_spawn_plan(), version);

            assert_eq!(packets.len(), 4, "{version:?}");
            assert!(matches!(packets[0], PacketRegistry::PlayerInfoUpdate(_)));
            assert!(
                !packets[1..]
                    .iter()
                    .any(|p| matches!(p, PacketRegistry::PlayerInfoRemove(_))),
                "{version:?} must not remove the NPC player-list entry"
            );
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
        npc_spawn_plan_with_skin(None)
    }

    fn npc_spawn_plan_with_skin(textures: Option<Property>) -> LobbyNpcSpawnPlan {
        let mut npc = LobbyNpc::player(
            "survival-npc",
            "survival",
            "Survival",
            LobbyPosition::new(1.0, 64.0, 2.0, 90.0, 0.0),
        )
        .with_textures(textures);
        npc.entity_id = EntityId::new(300);
        LobbyNpcSpawnPlan {
            npcs: std::sync::Arc::from(vec![npc]),
        }
    }

    #[test]
    fn npc_spawn_embeds_skin_for_legacy_client() {
        let value = "dGV4dHVyZXM=";
        let textures = Property::textures(value, Some("c2lnbmF0dXJl"));
        let skinned = npc_spawn_packets_for_join(
            &npc_spawn_plan_with_skin(Some(textures)),
            ProtocolVersion::V1_7_2,
        );
        let skinless =
            npc_spawn_packets_for_join(&npc_spawn_plan_with_skin(None), ProtocolVersion::V1_7_2);

        // The textures ride along in the SpawnPlayer payload for legacy clients.
        let skinned_spawn = skinned[1].encode_packet(ProtocolVersion::V1_7_2).unwrap();
        let skinless_spawn = skinless[1].encode_packet(ProtocolVersion::V1_7_2).unwrap();

        assert!(skinned_spawn.data().len() > skinless_spawn.data().len());
        let needle = value.as_bytes();
        assert!(
            skinned_spawn
                .data()
                .windows(needle.len())
                .any(|window| window == needle)
        );
    }

    #[test]
    fn npc_spawn_embeds_skin_for_modern_client() {
        let value = "dGV4dHVyZXM=";
        let textures = Property::textures(value, Some("c2lnbmF0dXJl"));
        let skinned = npc_spawn_packets_for_join(
            &npc_spawn_plan_with_skin(Some(textures)),
            ProtocolVersion::V1_20_2,
        );
        let skinless =
            npc_spawn_packets_for_join(&npc_spawn_plan_with_skin(None), ProtocolVersion::V1_20_2);

        // For modern clients the skin is carried in the player-info update.
        let skinned_info = skinned[0].encode_packet(ProtocolVersion::V1_20_2).unwrap();
        let skinless_info = skinless[0].encode_packet(ProtocolVersion::V1_20_2).unwrap();

        assert!(skinned_info.data().len() > skinless_info.data().len());
        let needle = value.as_bytes();
        assert!(
            skinned_info
                .data()
                .windows(needle.len())
                .any(|window| window == needle)
        );
    }
}
