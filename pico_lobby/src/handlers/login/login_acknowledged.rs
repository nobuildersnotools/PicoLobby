use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_brand::SERVER_BRAND;
use crate::server_state::ServerState;
use minecraft_packets::configuration::client_bound_known_packs_packet::ClientBoundKnownPacksPacket;
use minecraft_packets::configuration::configuration_client_bound_plugin_message_packet::ConfigurationClientBoundPluginMessagePacket;
use minecraft_packets::configuration::data::registry_entry::RegistryEntry;
use minecraft_packets::configuration::finish_configuration_packet::FinishConfigurationPacket;
use minecraft_packets::configuration::registry_data_packet::RegistryDataPacket;
use minecraft_packets::configuration::server_bound_known_packs_packet::ServerBoundKnownPacksPacket;
use minecraft_packets::configuration::update_tags_packet::{
    RegistryTag, TaggedRegistry, UpdateTagsPacket,
};
use minecraft_packets::login::login_acknowledged_packet::LoginAcknowledgedPacket;
use minecraft_protocol::prelude::{ProtocolVersion, State, VarInt};
use pico_precomputed_registries::PrecomputedRegistries;
use pico_registries::registry_provider::RegistryProvider;

impl PacketHandler for LoginAcknowledgedPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        _server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        let mut batch = Batch::new();
        let protocol_version = client_state.protocol_version();
        if protocol_version.supports_configuration_state() {
            client_state.set_state(State::Configuration);
            send_configuration_packets(&mut batch, protocol_version)?;
            Ok(batch)
        } else {
            Err(PacketHandlerError::invalid_state(
                "Configuration state not supported for this version",
            ))
        }
    }
}

impl PacketHandler for ServerBoundKnownPacksPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        _server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        let mut batch = Batch::new();
        let protocol_version = client_state.protocol_version();
        send_configuration_registry_packets(&mut batch, protocol_version)?;
        Ok(batch)
    }
}

/// Only for >= 1.20.2
fn send_configuration_packets(
    batch: &mut Batch<PacketRegistry>,
    protocol_version: ProtocolVersion,
) -> Result<(), PacketHandlerError> {
    // Send Server Brand
    let packet = ConfigurationClientBoundPluginMessagePacket::brand(SERVER_BRAND);
    batch.queue(|| PacketRegistry::ConfigurationClientBoundPluginMessage(packet));

    if supports_serverbound_known_packs(protocol_version) {
        // Send Known Packs
        let packet = ClientBoundKnownPacksPacket::new(protocol_version.humanize());
        batch.queue(|| PacketRegistry::ClientBoundKnownPacks(packet));
        return Ok(());
    }

    send_configuration_registry_packets(batch, protocol_version)
}

fn supports_serverbound_known_packs(protocol_version: ProtocolVersion) -> bool {
    protocol_version.is_after_inclusive(ProtocolVersion::V1_21)
}

fn send_configuration_registry_packets(
    batch: &mut Batch<PacketRegistry>,
    protocol_version: ProtocolVersion,
) -> Result<(), PacketHandlerError> {
    let registry_provider = PrecomputedRegistries::new(protocol_version);

    // Send Registry Data
    if protocol_version.is_after_inclusive(ProtocolVersion::V1_20_5) {
        // Since 1.20.5, each registry is sent in its own packet
        batch.chain_iter(
            registry_provider
                .get_registry_data_v1_20_5()?
                .into_iter()
                .map(move |(registry_id, registry_entries)| {
                    let entries = registry_entries
                        .iter()
                        .map(|entry| {
                            RegistryEntry::new(entry.entry_id.clone(), entry.nbt_bytes.clone())
                        })
                        .collect();
                    let packet = RegistryDataPacket::registry(registry_id, entries);
                    PacketRegistry::RegistryData(packet)
                }),
        );
    } else if protocol_version.is_after_inclusive(ProtocolVersion::V1_20_2) {
        // Since 1.19, all registries are sent as a single NBT tag
        // Since 1.20.2, all registries are sent in their own packet during the configuration state, still as a single NBT tag
        let registry_codec = registry_provider.get_registry_codec_v1_16()?;
        let packet = RegistryDataPacket::codec(registry_codec);
        batch.queue(|| PacketRegistry::RegistryData(packet));
    } else {
        // Registries are sent in the Join Game packet for versions prior to 1.20.2 since configuration state does not exist
        unreachable!();
    }

    // Send tags
    if protocol_version.is_after_inclusive(ProtocolVersion::V1_21_6) {
        // Since 1.21.6, the Dialog tags should be sent to have server links working
        // Since 1.21.11, the Timeline tags should be sent to get the time of day working
        // All tags are sent in a single packet
        // TODO: `wolf_variant` tags should probably be sent too?
        let tagged_registries = registry_provider
            .get_tagged_registries()?
            .iter()
            .map(|tagged_registry| {
                TaggedRegistry::new(
                    tagged_registry.registry_id.clone(),
                    tagged_registry
                        .tags
                        .iter()
                        .map(|registry_tag| {
                            RegistryTag::new(
                                registry_tag.identifier.clone(),
                                registry_tag.ids.iter().map(VarInt::from).collect(),
                            )
                        })
                        .collect(),
                )
            })
            .collect();

        let packet = UpdateTagsPacket::new(tagged_registries);
        batch.queue(|| PacketRegistry::UpdateTags(packet));
    }

    // Send Finished Configuration
    let packet = FinishConfigurationPacket {};
    batch.queue(|| PacketRegistry::FinishConfiguration(packet));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use minecraft_protocol::prelude::ProtocolVersion;

    fn server_state() -> ServerState {
        ServerState::builder().build().unwrap()
    }

    fn client(protocol: ProtocolVersion) -> ClientState {
        let mut cs = ClientState::default();
        cs.set_protocol_version(protocol);
        cs.set_state(State::Login);
        cs
    }

    fn packet() -> LoginAcknowledgedPacket {
        LoginAcknowledgedPacket::default()
    }

    #[tokio::test]
    async fn test_login_ack_supported_protocol() {
        // Given
        let mut client_state = client(ProtocolVersion::V1_20_2);
        let server_state = server_state();
        let pkt = packet();

        // When
        let batch = pkt.handle(&mut client_state, &server_state).unwrap();
        let mut batch = batch.into_stream();

        // Then
        assert_eq!(client_state.state(), State::Configuration);
        assert!(batch.next().await.is_some());
    }

    #[test]
    fn test_login_ack_unsupported_protocol() {
        // Given
        let mut client_state = client(ProtocolVersion::V1_20);
        let server_state = server_state();
        let pkt = packet();

        // When
        let result = pkt.handle(&mut client_state, &server_state);

        // Then
        assert!(matches!(
            result,
            Err(PacketHandlerError::InvalidState(_, _))
        ));
    }

    #[tokio::test]
    async fn test_configuration_packets_v1_20_2() {
        // Given
        let mut batch = Batch::new();

        // When
        send_configuration_packets(&mut batch, ProtocolVersion::V1_20_2).unwrap();
        let mut batch = batch.into_stream();

        // Then
        assert!(matches!(
            batch.next().await.unwrap(),
            PacketRegistry::ConfigurationClientBoundPluginMessage(_)
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            PacketRegistry::RegistryData(_)
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            PacketRegistry::FinishConfiguration(_)
        ));
        assert!(batch.next().await.is_none());
    }

    #[tokio::test]
    async fn test_configuration_packets_v1_20_5() {
        // Given
        let mut batch = Batch::new();

        // When
        send_configuration_packets(&mut batch, ProtocolVersion::V1_20_5).unwrap();
        let mut batch = batch.into_stream();

        // Then
        assert!(matches!(
            batch.next().await.unwrap(),
            PacketRegistry::ConfigurationClientBoundPluginMessage(_)
        ));
        for _ in 0..5 {
            assert!(matches!(
                batch.next().await.unwrap(),
                PacketRegistry::RegistryData(_)
            ));
        }
        assert!(matches!(
            batch.next().await.unwrap(),
            PacketRegistry::FinishConfiguration(_)
        ));
        assert!(batch.next().await.is_none());
    }

    #[tokio::test]
    async fn test_configuration_packets_v1_21_request_known_packs() {
        // Given
        let mut batch = Batch::new();

        // When
        send_configuration_packets(&mut batch, ProtocolVersion::V1_21).unwrap();
        let mut batch = batch.into_stream();

        // Then
        assert!(matches!(
            batch.next().await.unwrap(),
            PacketRegistry::ConfigurationClientBoundPluginMessage(_)
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            PacketRegistry::ClientBoundKnownPacks(_)
        ));
        assert!(batch.next().await.is_none());
    }

    #[tokio::test]
    async fn test_known_packs_response_sends_registry_data_v1_20_5() {
        // Given
        let mut batch = Batch::new();

        // When
        send_configuration_registry_packets(&mut batch, ProtocolVersion::V1_20_5).unwrap();
        let mut batch = batch.into_stream();

        // Then
        for _ in 0..5 {
            assert!(matches!(
                batch.next().await.unwrap(),
                PacketRegistry::RegistryData(_)
            ));
        }
        assert!(matches!(
            batch.next().await.unwrap(),
            PacketRegistry::FinishConfiguration(_)
        ));
        assert!(batch.next().await.is_none());
    }

    #[tokio::test]
    async fn test_known_packs_response_sends_full_registry_data_v1_21_4() {
        // Given
        let mut batch = Batch::new();
        let registry_provider = PrecomputedRegistries::new(ProtocolVersion::V1_21_4);
        let registry_count = registry_provider.get_registry_data_v1_20_5().unwrap().len();

        // When
        send_configuration_registry_packets(&mut batch, ProtocolVersion::V1_21_4).unwrap();
        let packets = batch.into_stream().collect::<Vec<_>>().await;

        // Then
        assert_eq!(
            packets
                .iter()
                .filter(|packet| matches!(packet, PacketRegistry::RegistryData(_)))
                .count(),
            registry_count,
            "known-pack responses should send PicoLobby's full configured registry set"
        );
        assert!(matches!(
            packets.last(),
            Some(PacketRegistry::FinishConfiguration(_))
        ));
    }

    #[test]
    fn test_registry_data_v1_21_2_and_later_includes_instruments() {
        for protocol_version in [
            ProtocolVersion::V1_21_2,
            ProtocolVersion::V1_21_4,
            ProtocolVersion::V1_21_5,
            ProtocolVersion::V1_21_6,
            ProtocolVersion::V1_21_7,
            ProtocolVersion::V1_21_9,
            ProtocolVersion::V1_21_11,
        ] {
            // Given
            let registry_provider = PrecomputedRegistries::new(protocol_version);

            // When
            let registries = registry_provider.get_registry_data_v1_20_5().unwrap();

            // Then
            let instruments = registries
                .iter()
                .find(|(registry_id, _)| registry_id.to_string() == "minecraft:instrument")
                .map_or_else(
                    || {
                        panic!(
                            "instrument registry should be sent to {} clients",
                            protocol_version.humanize()
                        )
                    },
                    |(_, entries)| entries,
                );

            assert!(
                instruments
                    .iter()
                    .any(|entry| entry.entry_id.to_string() == "minecraft:ponder_goat_horn"),
                "ponder goat horn should be sent to {} clients",
                protocol_version.humanize()
            );
        }
    }

    #[test]
    fn test_registry_data_before_v1_21_2_omits_instruments() {
        for protocol_version in [ProtocolVersion::V1_20_5, ProtocolVersion::V1_21] {
            // Given
            let registry_provider = PrecomputedRegistries::new(protocol_version);

            // When
            let registries = registry_provider.get_registry_data_v1_20_5().unwrap();

            // Then
            let instruments = registries
                .iter()
                .find(|(registry_id, _)| registry_id.to_string() == "minecraft:instrument")
                .map(|(_, entries)| entries);
            assert!(
                instruments.is_none(),
                "instrument registry should not be sent to {} clients",
                protocol_version.humanize()
            );
        }
    }

    #[test]
    fn known_pack_response_uses_picolobby_registry_payloads() {
        for protocol_version in [
            ProtocolVersion::V1_21,
            ProtocolVersion::V1_21_2,
            ProtocolVersion::V1_21_4,
            ProtocolVersion::V1_21_5,
            ProtocolVersion::V1_21_6,
            ProtocolVersion::V1_21_7,
            ProtocolVersion::V1_21_9,
            ProtocolVersion::V1_21_11,
        ] {
            let registry_provider = PrecomputedRegistries::new(protocol_version);
            let registries = registry_provider.get_registry_data_v1_20_5().unwrap();

            let dimension_type = registries
                .iter()
                .find(|(registry_id, _)| registry_id.to_string() == "minecraft:dimension_type")
                .expect("dimension_type registry should be present");
            let biome = registries
                .iter()
                .find(|(registry_id, _)| registry_id.to_string() == "minecraft:worldgen/biome")
                .expect("biome registry should be present");
            let painting_variant = registries
                .iter()
                .find(|(registry_id, _)| registry_id.to_string() == "minecraft:painting_variant")
                .expect("painting_variant registry should be present");
            let wolf_variant = registries
                .iter()
                .find(|(registry_id, _)| registry_id.to_string() == "minecraft:wolf_variant")
                .expect("wolf_variant registry should be present");

            assert!(
                dimension_type
                    .1
                    .iter()
                    .all(|entry| entry.nbt_bytes.is_some()),
                "dimension_type payloads must be sent to {} clients",
                protocol_version.humanize()
            );
            assert!(
                biome.1.iter().all(|entry| entry.nbt_bytes.is_some()),
                "biome payloads must come from PicoLobby's registry data for {} clients",
                protocol_version.humanize()
            );
            assert!(
                !painting_variant.1.is_empty()
                    && painting_variant
                        .1
                        .iter()
                        .all(|entry| entry.nbt_bytes.is_some()),
                "painting_variant payloads must stay non-empty for {} clients",
                protocol_version.humanize()
            );
            assert!(
                !wolf_variant.1.is_empty()
                    && wolf_variant.1.iter().all(|entry| entry.nbt_bytes.is_some()),
                "wolf_variant payloads must stay non-empty for {} clients",
                protocol_version.humanize()
            );
        }
    }
}
