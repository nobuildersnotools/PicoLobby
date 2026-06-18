use crate::login::Property;
use minecraft_protocol::prelude::*;

/// This packet was introduced in 1.21.2, previous versions uses the GameProfilePacket.
#[derive(PacketOut)]
pub struct LoginSuccessPacket {
    uuid: UuidAsString,
    username: String,
    #[pvn(735..)]
    properties: LengthPaddedVec<Property>,
    /// Introduced in 26.2 (protocol 776): a per-session UUID.
    #[pvn(776..)]
    session_id: UuidAsLongs,
}

impl LoginSuccessPacket {
    pub fn new(uuid: Uuid, username: impl ToString) -> Self {
        Self {
            uuid: uuid.into(),
            username: username.to_string(),
            properties: LengthPaddedVec::default(),
            session_id: Uuid::new_v4().into(),
        }
    }
}
