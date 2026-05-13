use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::ServerState;
use minecraft_packets::play::legacy_use_item_packet::LegacyUseItemPacket;
use minecraft_packets::play::server_bound_set_held_item_packet::ServerBoundSetHeldItemPacket;
use minecraft_packets::play::use_item_packet::UseItemPacket;
use tracing::info;

impl PacketHandler for ServerBoundSetHeldItemPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        _server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        client_state.set_selected_hotbar_slot(self.selected_slot());
        Ok(Batch::new())
    }
}

impl PacketHandler for UseItemPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        if !self.is_main_hand() {
            return Ok(Batch::new());
        }

        open_selector_for_selected_slot(client_state, server_state)
    }
}

impl PacketHandler for LegacyUseItemPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        open_selector_for_selected_slot(client_state, server_state)
    }
}

fn open_selector_for_selected_slot(
    client_state: &ClientState,
    server_state: &ServerState,
) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
    let Some(selector) = server_state.lobby_selector() else {
        return Ok(Batch::new());
    };

    if client_state.selected_hotbar_slot() != selector.hotbar_slot {
        return Ok(Batch::new());
    }

    info!(
        "{} opened server selector via hotbar slot {}",
        client_state.get_username(),
        selector.hotbar_slot
    );

    Ok(Batch::new())
}
