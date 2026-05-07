use minecraft_protocol::prelude::*;

#[derive(PacketOut)]
pub struct RemoveEntitiesPacket {
    entity_ids: LengthPaddedVec<VarInt>,
}

impl RemoveEntitiesPacket {
    pub fn new(entity_ids: Vec<i32>) -> Self {
        Self {
            entity_ids: LengthPaddedVec::new(entity_ids.into_iter().map(VarInt::new).collect()),
        }
    }

    pub fn single(entity_id: i32) -> Self {
        Self::new(vec![entity_id])
    }
}
