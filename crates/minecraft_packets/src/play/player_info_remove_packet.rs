use minecraft_protocol::prelude::*;

#[derive(PacketOut)]
pub struct PlayerInfoRemovePacket {
    uuids: LengthPaddedVec<UuidAsLongs>,
}

impl PlayerInfoRemovePacket {
    pub fn new(uuids: Vec<Uuid>) -> Self {
        Self {
            uuids: LengthPaddedVec::new(uuids.into_iter().map(UuidAsLongs::new).collect()),
        }
    }

    pub fn single(uuid: Uuid) -> Self {
        Self::new(vec![uuid])
    }
}
