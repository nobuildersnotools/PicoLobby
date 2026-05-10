use minecraft_protocol::prelude::*;

#[derive(PacketOut)]
pub struct ServerDataPacket {
    motd: Optional<String>,
    icon: Optional<String>,
    #[pvn(759..761)]
    previews_chat: bool,
    #[pvn(760..)]
    enforces_secure_chat: bool,
}

impl ServerDataPacket {
    pub fn disable_secure_profile_enforcement() -> Self {
        Self {
            motd: Optional::None,
            icon: Optional::None,
            previews_chat: false,
            enforces_secure_chat: false,
        }
    }
}
