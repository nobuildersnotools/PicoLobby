use crate::handlers::play::click_container::{
    resync_lobby_inventory, selected_hotbar_slot_is_lobby_item,
};
use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::ServerState;
use minecraft_packets::play::player_action_packet::PlayerActionPacket;

impl PacketHandler for PlayerActionPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        let mut batch = Batch::new();
        if self.is_drop_selected_item()
            && selected_hotbar_slot_is_lobby_item(client_state, server_state)
        {
            resync_lobby_inventory(client_state, server_state, true, &mut batch);
        }
        Ok(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::lobby::{SelectorItemConfig, VisibilityToggleConfig};
    use minecraft_protocol::prelude::{BinaryReader, DecodePacket, ProtocolVersion};

    fn server_with_lobby_items() -> ServerState {
        let mut builder = ServerState::builder();
        builder
            .set_lobby_enabled(true)
            .set_lobby_selector(Some(SelectorItemConfig {
                slot: 4,
                item: "minecraft:compass".to_string(),
                display_name: None,
                lore: vec![],
            }))
            .unwrap()
            .set_lobby_visibility_toggle(Some(VisibilityToggleConfig {
                slot: 8,
                item: "minecraft:ender_eye".to_string(),
                display_name_on: None,
                display_name_off: None,
                lore_on: vec![],
                lore_off: vec![],
                message_on: None,
                message_off: None,
            }))
            .unwrap();
        builder.build().unwrap()
    }

    fn client(selected_slot: u8) -> ClientState {
        let mut client = ClientState::default();
        client.set_protocol_version(ProtocolVersion::V1_21);
        client.set_selected_hotbar_slot(selected_slot);
        client
    }

    fn player_action(status: u8) -> PlayerActionPacket {
        let bytes = [status];
        let mut reader = BinaryReader::new(&bytes);
        PlayerActionPacket::decode(&mut reader, ProtocolVersion::V1_21).unwrap()
    }

    #[test]
    fn drop_selected_protected_slot_resyncs_inventory() {
        let server = server_with_lobby_items();
        let mut client = client(4);

        let packets = player_action(4)
            .handle(&mut client, &server)
            .unwrap()
            .into_vec();

        assert!(
            packets
                .iter()
                .any(|packet| matches!(packet, PacketRegistry::SetContainerContent(_)))
        );
    }

    #[test]
    fn drop_stack_selected_protected_slot_resyncs_inventory() {
        let server = server_with_lobby_items();
        let mut client = client(8);

        let packets = player_action(3)
            .handle(&mut client, &server)
            .unwrap()
            .into_vec();

        assert!(
            packets
                .iter()
                .any(|packet| matches!(packet, PacketRegistry::SetContainerContent(_)))
        );
    }

    #[test]
    fn drop_selected_unprotected_slot_is_ignored() {
        let server = server_with_lobby_items();
        let mut client = client(0);

        let packets = player_action(4)
            .handle(&mut client, &server)
            .unwrap()
            .into_vec();

        assert!(packets.is_empty());
    }

    #[test]
    fn non_drop_action_on_protected_slot_is_ignored() {
        let server = server_with_lobby_items();
        let mut client = client(4);

        let packets = player_action(0)
            .handle(&mut client, &server)
            .unwrap()
            .into_vec();

        assert!(packets.is_empty());
    }
}
