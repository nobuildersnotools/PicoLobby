use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::ServerState;
use minecraft_packets::play::close_container_packet::CloseContainerPacket;

impl PacketHandler for CloseContainerPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        _server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        if let Some(selector) = client_state.open_selector()
            && selector.window_id == self.window_id
        {
            client_state.take_open_selector();
        }
        Ok(Batch::new())
    }
}
