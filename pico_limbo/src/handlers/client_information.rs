use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::{ChatVisibility, ServerState};
use minecraft_packets::configuration::client_information_packet::ClientInformationPacket;

impl PacketHandler for ClientInformationPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        client_state.set_chat_visibility(ChatVisibility::from_client_mode(self.chat_mode()));
        server_state.update_lobby_chat_visibility(client_state);
        Ok(Batch::new())
    }
}
