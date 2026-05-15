use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::{ServerState, build_selector_menu};
use minecraft_packets::play::close_container_packet::CloseContainerPacket;
use minecraft_packets::play::legacy_use_item_packet::LegacyUseItemPacket;
use minecraft_packets::play::open_screen_packet::OpenScreenPacket;
use minecraft_packets::play::server_bound_set_held_item_packet::ServerBoundSetHeldItemPacket;
use minecraft_packets::play::set_container_content_packet::SetContainerContentPacket;
use minecraft_packets::play::use_item_packet::UseItemPacket;
use pico_text_component::prelude::Component;
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

        Ok(open_selector_for_selected_slot(client_state, server_state))
    }
}

impl PacketHandler for LegacyUseItemPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        Ok(open_selector_for_selected_slot(client_state, server_state))
    }
}

fn open_selector_for_selected_slot(
    client_state: &mut ClientState,
    server_state: &ServerState,
) -> Batch<PacketRegistry> {
    let Some(selector) = server_state.lobby_selector() else {
        return Batch::new();
    };

    if client_state.selected_hotbar_slot() != selector.hotbar_slot {
        return Batch::new();
    }

    let destinations = server_state.lobby_destinations();
    if destinations.is_empty() {
        return Batch::new();
    }

    info!(
        "{} opened server selector via hotbar slot {}",
        client_state.get_username(),
        selector.hotbar_slot
    );

    let mut batch = Batch::new();
    let version = client_state.protocol_version();

    // Close any already-open selector window before opening a new one.
    if let Some(old) = client_state.take_open_selector() {
        let wid = old.window_id;
        batch.queue(move || {
            PacketRegistry::ClientBoundCloseContainer(CloseContainerPacket::new(wid))
        });
    }

    let window_id = client_state.allocate_window_id();
    let state = build_selector_menu(window_id, destinations, version);

    let title = selector
        .display_name()
        .cloned()
        .unwrap_or_else(|| Component::new("Server Selector"));

    let open_pkt = OpenScreenPacket::new(window_id, title);
    batch.queue(|| PacketRegistry::OpenScreen(open_pkt));

    let content_pkt =
        SetContainerContentPacket::new(window_id, state.state_id, state.slots.clone());
    batch.queue(|| PacketRegistry::SetContainerContent(content_pkt));

    client_state.set_open_selector(state);

    batch
}
