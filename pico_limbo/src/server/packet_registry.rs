use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server_state::ServerState;
use macros::PacketReport;
use minecraft_packets::configuration::acknowledge_finish_configuration_packet::AcknowledgeConfigurationPacket;
use minecraft_packets::configuration::client_bound_known_packs_packet::ClientBoundKnownPacksPacket;
use minecraft_packets::configuration::configuration_client_bound_plugin_message_packet::ConfigurationClientBoundPluginMessagePacket;
use minecraft_packets::configuration::finish_configuration_packet::FinishConfigurationPacket;
use minecraft_packets::configuration::registry_data_packet::RegistryDataPacket;
use minecraft_packets::configuration::server_bound_known_packs_packet::ServerBoundKnownPacksPacket;
use minecraft_packets::configuration::update_tags_packet::UpdateTagsPacket;
use minecraft_packets::handshaking::handshake_packet::HandshakePacket;
use minecraft_packets::login::custom_query_answer_packet::CustomQueryAnswerPacket;
use minecraft_packets::login::custom_query_packet::CustomQueryPacket;
use minecraft_packets::login::game_profile_packet::GameProfilePacket;
use minecraft_packets::login::login_acknowledged_packet::LoginAcknowledgedPacket;
use minecraft_packets::login::login_disconnect_packet::LoginDisconnectPacket;
use minecraft_packets::login::login_state_packet::LoginStartPacket;
use minecraft_packets::login::login_success_packet::LoginSuccessPacket;
use minecraft_packets::login::set_compression_packet::SetCompressionPacket;
use minecraft_packets::play::boss_bar_packet::BossBarPacket;
use minecraft_packets::play::chat_command_packet::ChatCommandPacket;
use minecraft_packets::play::chat_message_packet::ChatMessagePacket;
use minecraft_packets::play::chunk_data_and_update_light_packet::ChunkDataAndUpdateLightPacket;
use minecraft_packets::play::client_bound_keep_alive_packet::ClientBoundKeepAlivePacket;
use minecraft_packets::play::client_bound_player_abilities_packet::ClientBoundPlayerAbilitiesPacket;
use minecraft_packets::play::client_bound_plugin_message_packet::PlayClientBoundPluginMessagePacket;
use minecraft_packets::play::commands_packet::CommandsPacket;
use minecraft_packets::play::destroy_entities_packet::DestroyEntitiesPacket;
use minecraft_packets::play::disconnect_packet::DisconnectPacket;
use minecraft_packets::play::game_event_packet::GameEventPacket;
use minecraft_packets::play::legacy_chat_message_packet::LegacyChatMessagePacket;
use minecraft_packets::play::legacy_set_title_packet::LegacySetTitlePacket;
use minecraft_packets::play::light_update_packet::LightUpdatePacket;
use minecraft_packets::play::login_packet::LoginPacket;
use minecraft_packets::play::move_entity_packet::{
    MoveEntityPosPacket, MoveEntityPosRotPacket, MoveEntityRotPacket,
};
use minecraft_packets::play::player_command_packet::PlayerCommandPacket;
use minecraft_packets::play::player_info_remove_packet::PlayerInfoRemovePacket;
use minecraft_packets::play::player_info_update_packet::PlayerInfoUpdatePacket;
use minecraft_packets::play::player_input_packet::PlayerInputPacket;
use minecraft_packets::play::remove_entities_packet::RemoveEntitiesPacket;
use minecraft_packets::play::rotate_head_packet::RotateHeadPacket;
use minecraft_packets::play::server_bound_player_abilities_packet::ServerBoundPlayerAbilitiesPacket;
use minecraft_packets::play::server_data_packet::ServerDataPacket;
use minecraft_packets::play::set_action_bar_text_packet::SetActionBarTextPacket;
use minecraft_packets::play::set_chunk_cache_center_packet::SetCenterChunkPacket;
use minecraft_packets::play::set_default_spawn_position_packet::SetDefaultSpawnPositionPacket;
use minecraft_packets::play::set_entity_data_packet::SetEntityMetadataPacket;
use minecraft_packets::play::set_player_position_and_rotation_packet::SetPlayerPositionAndRotationPacket;
use minecraft_packets::play::set_player_position_packet::SetPlayerPositionPacket;
use minecraft_packets::play::set_subtitle_text_packet::SetSubtitleTextPacket;
use minecraft_packets::play::set_title_text_packet::SetTitleTextPacket;
use minecraft_packets::play::set_titles_animation::SetTitlesAnimationPacket;
use minecraft_packets::play::spawn_entity_packet::SpawnEntityPacket;
use minecraft_packets::play::spawn_player_packet::SpawnPlayerPacket;
use minecraft_packets::play::synchronize_player_position_packet::SynchronizePlayerPositionPacket;
use minecraft_packets::play::system_chat_message_packet::SystemChatMessagePacket;
use minecraft_packets::play::tab_list_packet::TabListPacket;
use minecraft_packets::play::teleport_entity_packet::{
    EntityPositionSyncPacket, TeleportEntityPacket,
};
use minecraft_packets::play::transfer_packet::TransferPacket;
use minecraft_packets::play::update_time_packet::UpdateTimePacket;
use minecraft_packets::status::ping_request_packet::PingRequestPacket;
use minecraft_packets::status::ping_response_packet::PongResponsePacket;
use minecraft_packets::status::status_request_packet::StatusRequestPacket;
use minecraft_packets::status::status_response_packet::StatusResponsePacket;
use minecraft_protocol::prelude::*;
use net::raw_packet::RawPacket;

#[derive(PacketReport)]
pub enum PacketRegistry {
    // Handshake packets
    #[protocol_id(
        state = "handshake",
        bound = "serverbound",
        name = "minecraft:intention"
    )]
    Handshake(HandshakePacket),

    // Status packets
    #[protocol_id(
        state = "status",
        bound = "serverbound",
        name = "minecraft:status_request"
    )]
    StatusRequest(StatusRequestPacket),

    #[protocol_id(
        state = "status",
        bound = "clientbound",
        name = "minecraft:status_response"
    )]
    StatusResponse(StatusResponsePacket),

    #[protocol_id(
        state = "status",
        bound = "serverbound",
        name = "minecraft:ping_request"
    )]
    PingRequest(PingRequestPacket),

    #[protocol_id(
        state = "status",
        bound = "clientbound",
        name = "minecraft:pong_response"
    )]
    PongResponse(PongResponsePacket),

    // Login packets
    #[protocol_id(state = "login", bound = "serverbound", name = "minecraft:hello")]
    LoginStart(LoginStartPacket),

    #[protocol_id(
        state = "login",
        bound = "serverbound",
        name = "minecraft:login_acknowledged"
    )]
    LoginAcknowledged(LoginAcknowledgedPacket),

    #[protocol_id(
        state = "login",
        bound = "serverbound",
        name = "minecraft:custom_query_answer"
    )]
    CustomQueryAnswer(CustomQueryAnswerPacket),

    #[protocol_id(
        state = "login",
        bound = "clientbound",
        name = "minecraft:custom_query"
    )]
    CustomQuery(CustomQueryPacket),

    #[protocol_id(
        state = "login",
        bound = "clientbound",
        name = "minecraft:login_finished"
    )]
    LoginSuccess(LoginSuccessPacket),

    #[protocol_id(
        state = "login",
        bound = "clientbound",
        name = "minecraft:game_profile"
    )]
    GameProfile(GameProfilePacket),

    #[protocol_id(
        state = "login",
        bound = "clientbound",
        name = "minecraft:login_disconnect"
    )]
    LoginDisconnect(LoginDisconnectPacket),

    #[protocol_id(
        state = "login",
        bound = "clientbound",
        name = "minecraft:login_compression"
    )]
    SetCompression(SetCompressionPacket),

    // Configuration packets
    #[protocol_id(
        state = "configuration",
        bound = "serverbound",
        name = "minecraft:finish_configuration"
    )]
    AcknowledgeConfiguration(AcknowledgeConfigurationPacket),

    #[protocol_id(
        state = "configuration",
        bound = "clientbound",
        name = "minecraft:custom_payload"
    )]
    ConfigurationClientBoundPluginMessage(ConfigurationClientBoundPluginMessagePacket),

    #[protocol_id(
        state = "configuration",
        bound = "clientbound",
        name = "minecraft:select_known_packs"
    )]
    ClientBoundKnownPacks(ClientBoundKnownPacksPacket),

    #[protocol_id(
        state = "configuration",
        bound = "serverbound",
        name = "minecraft:select_known_packs"
    )]
    ServerBoundKnownPacks(ServerBoundKnownPacksPacket),

    #[protocol_id(
        state = "configuration",
        bound = "clientbound",
        name = "minecraft:registry_data"
    )]
    RegistryData(RegistryDataPacket),

    #[protocol_id(
        state = "configuration",
        bound = "clientbound",
        name = "minecraft:update_tags"
    )]
    UpdateTags(UpdateTagsPacket),

    #[protocol_id(
        state = "configuration",
        bound = "clientbound",
        name = "minecraft:finish_configuration"
    )]
    FinishConfiguration(FinishConfigurationPacket),

    #[protocol_id(
        state = "configuration",
        bound = "clientbound",
        name = "minecraft:disconnect"
    )]
    ConfigurationDisconnect(DisconnectPacket),

    // Play packets
    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:login")]
    Login(Box<LoginPacket>),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:player_position"
    )]
    SynchronizePlayerPosition(SynchronizePlayerPositionPacket),

    #[protocol_id(
        state = "play",
        bound = "serverbound",
        name = "minecraft:move_player_pos"
    )]
    SetPlayerPosition(SetPlayerPositionPacket),

    #[protocol_id(
        state = "play",
        bound = "serverbound",
        name = "minecraft:move_player_pos_rot"
    )]
    SetPlayerPositionAndRotation(SetPlayerPositionAndRotationPacket),

    #[protocol_id(state = "play", bound = "serverbound", name = "minecraft:chat_command")]
    ChatCommand(ChatCommandPacket),

    #[protocol_id(state = "play", bound = "serverbound", name = "minecraft:chat")]
    ChatMessage(ChatMessagePacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:set_default_spawn_position"
    )]
    SetDefaultSpawnPosition(SetDefaultSpawnPositionPacket),

    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:commands")]
    Commands(CommandsPacket),

    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:game_event")]
    GameEvent(GameEventPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:set_chunk_cache_center"
    )]
    SetCenterChunk(SetCenterChunkPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:level_chunk_with_light"
    )]
    ChunkDataAndUpdateLight(Box<ChunkDataAndUpdateLightPacket>),

    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:light_update")]
    LightUpdate(Box<LightUpdatePacket>),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:custom_payload"
    )]
    PlayClientBoundPluginMessage(PlayClientBoundPluginMessagePacket),

    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:system_chat")]
    SystemChatMessage(SystemChatMessagePacket),

    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:server_data")]
    ServerData(ServerDataPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:legacy_chat_message"
    )]
    LegacyChatMessage(LegacyChatMessagePacket),

    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:keep_alive")]
    ClientBoundKeepAlive(ClientBoundKeepAlivePacket),

    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:disconnect")]
    PlayDisconnect(DisconnectPacket),

    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:set_time")]
    UpdateTime(UpdateTimePacket),

    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:tab_list")]
    TabList(TabListPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:player_info_update"
    )]
    PlayerInfoUpdate(PlayerInfoUpdatePacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:player_info_remove"
    )]
    PlayerInfoRemove(PlayerInfoRemovePacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:remove_entities"
    )]
    RemoveEntities(RemoveEntitiesPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:destroy_entities"
    )]
    DestroyEntities(DestroyEntitiesPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:move_entity_pos"
    )]
    MoveEntityPos(MoveEntityPosPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:move_entity_pos_rot"
    )]
    MoveEntityPosRot(MoveEntityPosRotPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:move_entity_rot"
    )]
    MoveEntityRot(MoveEntityRotPacket),

    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:rotate_head")]
    RotateHead(RotateHeadPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:set_entity_data"
    )]
    SetEntityMetadata(SetEntityMetadataPacket),

    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:add_entity")]
    SpawnEntity(SpawnEntityPacket),

    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:add_player")]
    SpawnPlayer(SpawnPlayerPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:entity_position_sync"
    )]
    EntityPositionSync(EntityPositionSyncPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:teleport_entity"
    )]
    TeleportEntity(TeleportEntityPacket),

    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:boss_event")]
    BossBar(BossBarPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:set_title_text"
    )]
    SetTitleText(SetTitleTextPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:set_titles_animation"
    )]
    SetTitlesAnimation(SetTitlesAnimationPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:set_subtitle_text"
    )]
    SetSubtitleText(SetSubtitleTextPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:legacy_set_title"
    )]
    LegacySetTitle(LegacySetTitlePacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:set_action_bar_text"
    )]
    SetActionBarText(SetActionBarTextPacket),

    #[protocol_id(state = "play", bound = "clientbound", name = "minecraft:transfer")]
    Transfer(TransferPacket),

    #[protocol_id(
        state = "play",
        bound = "clientbound",
        name = "minecraft:player_abilities"
    )]
    ClientBoundPlayerAbilities(ClientBoundPlayerAbilitiesPacket),

    #[protocol_id(
        state = "play",
        bound = "serverbound",
        name = "minecraft:player_abilities"
    )]
    ServerBoundPlayerAbilities(ServerBoundPlayerAbilitiesPacket),

    #[protocol_id(
        state = "play",
        bound = "serverbound",
        name = "minecraft:player_command"
    )]
    PlayerCommand(PlayerCommandPacket),

    #[protocol_id(state = "play", bound = "serverbound", name = "minecraft:player_input")]
    PlayerInput(PlayerInputPacket),
}

impl PacketHandler for PacketRegistry {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        match self {
            Self::Handshake(packet) => packet.handle(client_state, server_state),
            Self::StatusRequest(packet) => packet.handle(client_state, server_state),
            Self::PingRequest(packet) => packet.handle(client_state, server_state),
            Self::LoginStart(packet) => packet.handle(client_state, server_state),
            Self::CustomQueryAnswer(packet) => packet.handle(client_state, server_state),
            Self::LoginAcknowledged(packet) => packet.handle(client_state, server_state),
            Self::AcknowledgeConfiguration(packet) => packet.handle(client_state, server_state),
            Self::ServerBoundKnownPacks(packet) => packet.handle(client_state, server_state),
            Self::SetPlayerPositionAndRotation(packet) => packet.handle(client_state, server_state),
            Self::SetPlayerPosition(packet) => packet.handle(client_state, server_state),
            Self::ChatCommand(packet) => packet.handle(client_state, server_state),
            Self::ChatMessage(packet) => packet.handle(client_state, server_state),
            Self::ServerBoundPlayerAbilities(packet) => packet.handle(client_state, server_state),
            Self::PlayerCommand(packet) => packet.handle(client_state, server_state),
            Self::PlayerInput(packet) => packet.handle(client_state, server_state),
            _ => Err(PacketHandlerError::custom("Unhandled packet")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minecraft_packets::play::VoidChunkContext;
    use minecraft_protocol::prelude::{BinaryReader, DecodePacket, Uuid, VarInt};

    #[test]
    fn decodes_current_player_command_packet() {
        let raw_packet = RawPacket::from_bytes(37, &[0xac, 0x02, 0, 0]);
        let packet =
            PacketRegistry::decode_packet(ProtocolVersion::V1_21, State::Play, raw_packet).unwrap();

        match packet {
            PacketRegistry::PlayerCommand(packet) => {
                assert_eq!(packet.entity_id(), 300);
                assert_eq!(packet.crouching_change(), Some(true));
            }
            _ => panic!("expected player command packet"),
        }
    }

    #[test]
    fn decodes_older_player_command_packet_ids() {
        for (version, packet_id) in [
            (ProtocolVersion::V1_7_2, 11),
            (ProtocolVersion::V1_8, 11),
            (ProtocolVersion::V1_12_2, 21),
            (ProtocolVersion::V1_19_4, 30),
            (ProtocolVersion::V1_20_5, 37),
        ] {
            let raw_packet = RawPacket::from_bytes(packet_id, &[0xac, 0x02, 0, 0]);
            let packet = PacketRegistry::decode_packet(version, State::Play, raw_packet).unwrap();

            match packet {
                PacketRegistry::PlayerCommand(packet) => {
                    assert_eq!(packet.entity_id(), 300);
                    assert_eq!(packet.crouching_change(), Some(true));
                }
                _ => panic!("expected player command packet for {version:?}"),
            }
        }
    }

    #[test]
    fn decodes_latest_player_command_packet_id() {
        let raw_packet = RawPacket::from_bytes(42, &[0xac, 0x02, 1, 0]);
        let packet =
            PacketRegistry::decode_packet(ProtocolVersion::V26_1, State::Play, raw_packet).unwrap();

        match packet {
            PacketRegistry::PlayerCommand(packet) => {
                assert_eq!(packet.entity_id(), 300);
                assert_eq!(packet.crouching_change(), Some(false));
            }
            _ => panic!("expected player command packet"),
        }
    }

    #[test]
    fn decodes_latest_player_input_packet_id() {
        let raw_packet = RawPacket::from_bytes(43, &[0x20]);
        let packet =
            PacketRegistry::decode_packet(ProtocolVersion::V26_1, State::Play, raw_packet).unwrap();

        match packet {
            PacketRegistry::PlayerInput(packet) => {
                assert_eq!(packet.shift(), Some(true));
            }
            _ => panic!("expected player input packet"),
        }
    }

    #[test]
    fn decodes_v1_21_9_player_input_packet_id() {
        let raw_packet = RawPacket::from_bytes(42, &[0x20]);
        let packet =
            PacketRegistry::decode_packet(ProtocolVersion::V1_21_9, State::Play, raw_packet)
                .unwrap();

        match packet {
            PacketRegistry::PlayerInput(packet) => {
                assert_eq!(packet.shift(), Some(true));
            }
            _ => panic!("expected player input packet"),
        }
    }

    #[test]
    fn encodes_current_player_info_remove_packet() {
        let uuid = Uuid::from_u128(1);
        let packet = PacketRegistry::PlayerInfoRemove(PlayerInfoRemovePacket::single(uuid));

        let raw_packet = packet.encode_packet(ProtocolVersion::V1_21).unwrap();

        assert_eq!(
            raw_packet.bytes(),
            &[61, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,]
        );
    }

    #[test]
    fn encodes_v1_19_server_data_with_secure_profiles_disabled() {
        for (protocol_version, packet_id, data) in [
            (ProtocolVersion::V1_19, 63, vec![0, 0, 0]),
            (ProtocolVersion::V1_19_1, 66, vec![0, 0, 0, 0]),
        ] {
            let packet =
                PacketRegistry::ServerData(ServerDataPacket::disable_secure_profile_enforcement());
            let raw_packet = packet.encode_packet(protocol_version).unwrap();

            assert_eq!(raw_packet.packet_id(), Some(packet_id));
            assert_eq!(raw_packet.data(), data);
        }
    }

    #[test]
    fn encodes_latest_player_info_remove_packet_id() {
        let uuid = Uuid::from_u128(1);
        let packet = PacketRegistry::PlayerInfoRemove(PlayerInfoRemovePacket::single(uuid));

        let raw_packet = packet.encode_packet(ProtocolVersion::V26_1).unwrap();

        assert_eq!(raw_packet.packet_id(), Some(69));
        assert_eq!(
            raw_packet.data(),
            &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]
        );
    }

    #[test]
    fn encodes_current_remove_entities_packet() {
        let packet = PacketRegistry::RemoveEntities(RemoveEntitiesPacket::single(300));

        let raw_packet = packet.encode_packet(ProtocolVersion::V1_21).unwrap();

        assert_eq!(raw_packet.bytes(), &[66, 1, 0xac, 0x02]);
    }

    #[test]
    fn encodes_latest_remove_entities_packet_id() {
        let packet = PacketRegistry::RemoveEntities(RemoveEntitiesPacket::single(300));

        let raw_packet = packet.encode_packet(ProtocolVersion::V26_1).unwrap();

        assert_eq!(raw_packet.packet_id(), Some(77));
        assert_eq!(raw_packet.data(), &[1, 0xac, 0x02]);
    }

    #[test]
    fn encodes_current_relative_move_packets() {
        let delta = minecraft_packets::play::move_entity_packet::RelativeMoveDelta::new_unchecked(
            2048, -1024, 512,
        );

        let raw_packet = PacketRegistry::MoveEntityPos(MoveEntityPosPacket::new(300, delta, true))
            .encode_packet(ProtocolVersion::V1_21)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(46));
        assert_eq!(
            raw_packet.data(),
            &[0xac, 0x02, 0x08, 0x00, 0xfc, 0x00, 0x02, 0x00, 1]
        );

        let raw_packet = PacketRegistry::MoveEntityPosRot(MoveEntityPosRotPacket::new(
            300, delta, 90.0, 45.0, false,
        ))
        .encode_packet(ProtocolVersion::V1_21)
        .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(47));
        assert_eq!(
            raw_packet.data(),
            &[0xac, 0x02, 0x08, 0x00, 0xfc, 0x00, 0x02, 0x00, 64, 32, 0]
        );

        let raw_packet =
            PacketRegistry::MoveEntityRot(MoveEntityRotPacket::new(300, 180.0, -90.0, true))
                .encode_packet(ProtocolVersion::V1_21)
                .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(48));
        assert_eq!(raw_packet.data(), &[0xac, 0x02, 128, 192, 1]);
    }

    #[test]
    fn encodes_latest_relative_move_packet_ids() {
        let delta = minecraft_packets::play::move_entity_packet::RelativeMoveDelta::new_unchecked(
            2048, -1024, 512,
        );

        assert_eq!(
            PacketRegistry::MoveEntityPos(MoveEntityPosPacket::new(300, delta, true))
                .encode_packet(ProtocolVersion::V26_1)
                .unwrap()
                .packet_id(),
            Some(53)
        );
        assert_eq!(
            PacketRegistry::MoveEntityPosRot(MoveEntityPosRotPacket::new(
                300, delta, 90.0, 45.0, false,
            ))
            .encode_packet(ProtocolVersion::V26_1)
            .unwrap()
            .packet_id(),
            Some(54)
        );
        assert_eq!(
            PacketRegistry::MoveEntityRot(MoveEntityRotPacket::new(300, 180.0, -90.0, true))
                .encode_packet(ProtocolVersion::V26_1)
                .unwrap()
                .packet_id(),
            Some(56)
        );
    }

    #[test]
    fn encodes_mid_current_relative_move_packet_ids() {
        let delta = minecraft_packets::play::move_entity_packet::RelativeMoveDelta::new_unchecked(
            2048, -1024, 512,
        );

        for (protocol_version, pos_id, pos_rot_id, rot_id, head_id, teleport_id) in [
            (ProtocolVersion::V1_19_4, 43, 44, 45, 66, 104),
            (ProtocolVersion::V1_20, 43, 44, 45, 66, 104),
            (ProtocolVersion::V1_20_2, 44, 45, 46, 68, 107),
            (ProtocolVersion::V1_20_3, 44, 45, 46, 70, 109),
            (ProtocolVersion::V1_20_5, 46, 47, 48, 72, 112),
        ] {
            assert_eq!(
                PacketRegistry::MoveEntityPos(MoveEntityPosPacket::new(300, delta, true))
                    .encode_packet(protocol_version)
                    .unwrap()
                    .packet_id(),
                Some(pos_id)
            );
            assert_eq!(
                PacketRegistry::MoveEntityPosRot(MoveEntityPosRotPacket::new(
                    300, delta, 90.0, 45.0, false,
                ))
                .encode_packet(protocol_version)
                .unwrap()
                .packet_id(),
                Some(pos_rot_id)
            );
            assert_eq!(
                PacketRegistry::MoveEntityRot(MoveEntityRotPacket::new(300, 180.0, -90.0, true))
                    .encode_packet(protocol_version)
                    .unwrap()
                    .packet_id(),
                Some(rot_id)
            );
            assert_eq!(
                PacketRegistry::RotateHead(RotateHeadPacket::new(300, 90.0))
                    .encode_packet(protocol_version)
                    .unwrap()
                    .packet_id(),
                Some(head_id)
            );
            assert_eq!(
                PacketRegistry::TeleportEntity(TeleportEntityPacket::absolute(
                    300, 8.1, 64.0, -2.25, 90.0, 45.0, true,
                ))
                .encode_packet(protocol_version)
                .unwrap()
                .packet_id(),
                Some(teleport_id)
            );
        }
    }

    #[test]
    fn encodes_old_and_mid_relative_move_packet_ids() {
        let delta = minecraft_packets::play::move_entity_packet::RelativeMoveDelta::new_unchecked(
            2048, -1024, 512,
        );

        for (protocol_version, pos_id, pos_rot_id, rot_id, head_id, teleport_id) in [
            (ProtocolVersion::V1_7_2, 21, 23, 22, 25, 24),
            (ProtocolVersion::V1_8, 21, 23, 22, 25, 24),
            (ProtocolVersion::V1_12_2, 38, 39, 40, 54, 76),
            (ProtocolVersion::V1_18_2, 41, 42, 43, 62, 98),
            (ProtocolVersion::V1_19_3, 39, 40, 41, 62, 100),
        ] {
            assert_eq!(
                PacketRegistry::MoveEntityPos(MoveEntityPosPacket::new(300, delta, true))
                    .encode_packet(protocol_version)
                    .unwrap()
                    .packet_id(),
                Some(pos_id)
            );
            assert_eq!(
                PacketRegistry::MoveEntityPosRot(MoveEntityPosRotPacket::new(
                    300, delta, 90.0, 45.0, false,
                ))
                .encode_packet(protocol_version)
                .unwrap()
                .packet_id(),
                Some(pos_rot_id)
            );
            assert_eq!(
                PacketRegistry::MoveEntityRot(MoveEntityRotPacket::new(300, 180.0, -90.0, true))
                    .encode_packet(protocol_version)
                    .unwrap()
                    .packet_id(),
                Some(rot_id)
            );
            assert_eq!(
                PacketRegistry::RotateHead(RotateHeadPacket::new(300, 90.0))
                    .encode_packet(protocol_version)
                    .unwrap()
                    .packet_id(),
                Some(head_id)
            );
            assert_eq!(
                PacketRegistry::TeleportEntity(TeleportEntityPacket::absolute(
                    300, 8.1, 64.0, -2.25, 90.0, 45.0, true,
                ))
                .encode_packet(protocol_version)
                .unwrap()
                .packet_id(),
                Some(teleport_id)
            );
        }
    }

    #[test]
    fn encodes_current_and_latest_head_rotation_packets() {
        let raw_packet = PacketRegistry::RotateHead(RotateHeadPacket::new(300, 90.0))
            .encode_packet(ProtocolVersion::V1_21)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(72));
        assert_eq!(raw_packet.data(), &[0xac, 0x02, 64]);

        let raw_packet = PacketRegistry::RotateHead(RotateHeadPacket::new(300, 90.0))
            .encode_packet(ProtocolVersion::V26_1)
            .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(83));
        assert_eq!(raw_packet.data(), &[0xac, 0x02, 64]);
    }

    #[test]
    fn encodes_current_legacy_entity_teleport_packet() {
        let raw_packet = PacketRegistry::TeleportEntity(TeleportEntityPacket::absolute(
            300, 8.1, 64.0, -2.25, 90.0, 45.0, true,
        ))
        .encode_packet(ProtocolVersion::V1_21)
        .unwrap();

        assert_eq!(raw_packet.packet_id(), Some(112));
        assert_eq!(raw_packet.data().len(), 29);
        assert_eq!(&raw_packet.data()[0..2], &[0xac, 0x02]);
        assert_eq!(&raw_packet.data()[26..29], &[64, 32, 1]);
    }

    #[test]
    fn encodes_position_sync_packet_after_v1_21_2() {
        let raw_packet = PacketRegistry::EntityPositionSync(EntityPositionSyncPacket::absolute(
            300, 8.1, 64.0, -2.25, 90.0, 45.0, true,
        ))
        .encode_packet(ProtocolVersion::V1_21_2)
        .unwrap();

        assert_eq!(raw_packet.packet_id(), Some(32));
        assert_eq!(raw_packet.data().len(), 63);
        assert_eq!(&raw_packet.data()[0..2], &[0xac, 0x02]);
        assert_eq!(&raw_packet.data()[58..63], &[0, 0, 0, 0, 1]);

        let raw_packet = PacketRegistry::EntityPositionSync(EntityPositionSyncPacket::absolute(
            300, 8.1, 64.0, -2.25, 90.0, 45.0, true,
        ))
        .encode_packet(ProtocolVersion::V26_1)
        .unwrap();
        assert_eq!(raw_packet.packet_id(), Some(35));
        assert_eq!(raw_packet.data().len(), 63);
    }

    #[test]
    fn encodes_legacy_light_update_packet_ids() {
        for (protocol_version, packet_id) in [
            (ProtocolVersion::V1_14, 36),
            (ProtocolVersion::V1_15, 37),
            (ProtocolVersion::V1_16, 36),
            (ProtocolVersion::V1_16_2, 35),
            (ProtocolVersion::V1_17, 37),
        ] {
            let raw_packet =
                PacketRegistry::LightUpdate(Box::new(LightUpdatePacket::new_void(0, 0, 256)))
                    .encode_packet(protocol_version)
                    .unwrap();

            assert_eq!(raw_packet.packet_id(), Some(packet_id));
        }
    }

    #[test]
    fn encodes_pre_v1_16_chunk_packet_ids() {
        for (protocol_version, packet_id) in [
            (ProtocolVersion::V1_7_2, 33),
            (ProtocolVersion::V1_8, 33),
            (ProtocolVersion::V1_12_2, 32),
            (ProtocolVersion::V1_13, 34),
            (ProtocolVersion::V1_13_2, 34),
            (ProtocolVersion::V1_14_4, 33),
            (ProtocolVersion::V1_15_2, 34),
        ] {
            let packet = ChunkDataAndUpdateLightPacket::void(VoidChunkContext {
                chunk_x: 0,
                chunk_z: 0,
                biome_index: 1,
                dimension_height: 256,
                dimension_min_y: 0,
            });
            let raw_packet = PacketRegistry::ChunkDataAndUpdateLight(Box::new(packet))
                .encode_packet(protocol_version)
                .unwrap();

            assert_eq!(raw_packet.packet_id(), Some(packet_id));
            assert!(!raw_packet.data().is_empty());
        }
    }

    #[test]
    fn v1_8_chunk_header_uses_varint_data_length_after_primary_mask() {
        let packet = ChunkDataAndUpdateLightPacket::void(VoidChunkContext {
            chunk_x: 0,
            chunk_z: 0,
            biome_index: 1,
            dimension_height: 256,
            dimension_min_y: 0,
        });
        let raw_packet = PacketRegistry::ChunkDataAndUpdateLight(Box::new(packet))
            .encode_packet(ProtocolVersion::V1_8)
            .unwrap();
        let data = raw_packet.data();

        assert_eq!(raw_packet.packet_id(), Some(33));
        assert_eq!(&data[0..11], &[0, 0, 0, 0, 0, 0, 0, 0, 1, 0xFF, 0xFF]);

        let mut reader = BinaryReader::new(&data[11..]);
        let payload_len = VarInt::decode(&mut reader, ProtocolVersion::V1_8)
            .unwrap()
            .inner() as usize;

        assert_eq!(payload_len, reader.remaining());
    }

    #[test]
    fn v1_9_to_v1_12_chunk_payload_has_no_trailing_block_entity_count() {
        for protocol_version in [
            ProtocolVersion::V1_9,
            ProtocolVersion::V1_10,
            ProtocolVersion::V1_11,
            ProtocolVersion::V1_12_2,
        ] {
            let packet = ChunkDataAndUpdateLightPacket::void(VoidChunkContext {
                chunk_x: 0,
                chunk_z: 0,
                biome_index: 1,
                dimension_height: 256,
                dimension_min_y: 0,
            });
            let raw_packet = PacketRegistry::ChunkDataAndUpdateLight(Box::new(packet))
                .encode_packet(protocol_version)
                .unwrap();
            let data = raw_packet.data();

            let mut reader = BinaryReader::new(&data[9..]);
            VarInt::decode(&mut reader, protocol_version).unwrap();
            let payload_len = VarInt::decode(&mut reader, protocol_version)
                .unwrap()
                .inner() as usize;

            assert_eq!(
                payload_len,
                reader.remaining(),
                "unexpected trailing bytes for {protocol_version:?}"
            );
        }
    }

    #[test]
    fn v1_13_chunk_payload_keeps_block_entity_count() {
        let packet = ChunkDataAndUpdateLightPacket::void(VoidChunkContext {
            chunk_x: 0,
            chunk_z: 0,
            biome_index: 1,
            dimension_height: 256,
            dimension_min_y: 0,
        });
        let raw_packet = PacketRegistry::ChunkDataAndUpdateLight(Box::new(packet))
            .encode_packet(ProtocolVersion::V1_13)
            .unwrap();
        let data = raw_packet.data();

        let mut reader = BinaryReader::new(&data[9..]);
        VarInt::decode(&mut reader, ProtocolVersion::V1_13).unwrap();
        let payload_len = VarInt::decode(&mut reader, ProtocolVersion::V1_13)
            .unwrap()
            .inner() as usize;

        assert_eq!(payload_len + 1, reader.remaining());
    }
}
