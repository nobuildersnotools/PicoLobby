use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::lobby_chat::chat_feedback_packet;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::{SelectorClick, ServerState};
use minecraft_packets::play::click_container_packet::ClickContainerPacket;
use minecraft_packets::play::client_bound_plugin_message_packet::PlayClientBoundPluginMessagePacket;
use minecraft_packets::play::close_container_packet::CloseContainerPacket;
use minecraft_packets::play::confirm_transaction_packet::ConfirmTransactionPacket;
use minecraft_packets::play::set_container_content_packet::SetContainerContentPacket;
use minecraft_protocol::prelude::ProtocolVersion;
use tracing::{info, warn};

impl PacketHandler for ClickContainerPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        let Some(selector) = client_state.open_selector() else {
            return Ok(Batch::new());
        };

        if selector.window_id != self.window_id {
            return Ok(Batch::new());
        }

        let version = client_state.protocol_version();
        let click = selector.classify(self, version);

        match click {
            SelectorClick::Select { slot_index } => {
                // SAFETY: slot_map has exactly 27 entries; slot_index is 0..26.
                let destination_id = client_state
                    .open_selector()
                    .and_then(|s| s.slot_map[slot_index].clone());

                let Some(destination_id) = destination_id else {
                    return Ok(resync(client_state, self));
                };

                match server_state.resolve_lobby_destination(&destination_id) {
                    Ok(dest) => {
                        info!(
                            "Sending {} to Velocity server '{}' via selector",
                            client_state.get_username(),
                            dest.server,
                        );
                        let window_id = client_state
                            .take_open_selector()
                            .map_or(self.window_id, |s| s.window_id);

                        let mut batch = Batch::new();
                        let packet =
                            PlayClientBoundPluginMessagePacket::bungeecord_connect(&dest.server);
                        if let Some(confirm) = legacy_reject_click(version, self) {
                            batch.queue(|| PacketRegistry::ClientBoundConfirmTransaction(confirm));
                        }
                        batch.queue(|| PacketRegistry::PlayClientBoundPluginMessage(packet));
                        batch.queue(move || {
                            PacketRegistry::ClientBoundCloseContainer(CloseContainerPacket::new(
                                window_id,
                            ))
                        });
                        Ok(batch)
                    }
                    Err(err) => {
                        warn!("{}: {}", client_state.get_username(), err);
                        let msg = format!("Unknown server: {destination_id}");
                        let mut batch = resync(client_state, self);
                        batch.queue(move || chat_feedback_packet(version, &msg));
                        Ok(batch)
                    }
                }
            }
            SelectorClick::RequiresResync => Ok(resync(client_state, self)),
            SelectorClick::Ignored => Ok(Batch::new()),
        }
    }
}

impl PacketHandler for ConfirmTransactionPacket {
    fn handle(
        &self,
        _client_state: &mut ClientState,
        _server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        Ok(Batch::new())
    }
}

fn resync(client_state: &mut ClientState, click: &ClickContainerPacket) -> Batch<PacketRegistry> {
    let version = client_state.protocol_version();
    let Some(selector) = client_state.open_selector_mut() else {
        return Batch::new();
    };
    selector.state_id += 1;
    let pkt = SetContainerContentPacket::new(
        selector.window_id,
        selector.state_id,
        selector.slots.clone(),
    );
    let mut batch = Batch::new();
    if let Some(confirm) = legacy_reject_click(version, click) {
        batch.queue(|| PacketRegistry::ClientBoundConfirmTransaction(confirm));
    }
    batch.queue(|| PacketRegistry::SetContainerContent(pkt));
    batch
}

fn legacy_reject_click(
    version: ProtocolVersion,
    click: &ClickContainerPacket,
) -> Option<ConfirmTransactionPacket> {
    version
        .is_before_inclusive(ProtocolVersion::V1_12_2)
        .then(|| ConfirmTransactionPacket::new(click.window_id, click.action_number, false))
}
