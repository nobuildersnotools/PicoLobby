use crate::play::move_entity_packet::{encode_angle, encode_entity_id};
use minecraft_protocol::prelude::*;

pub struct RotateHeadPacket {
    entity_id: VarInt,
    head_yaw: u8,
}

impl RotateHeadPacket {
    pub fn new(entity_id: i32, head_yaw: f32) -> Self {
        Self {
            entity_id: VarInt::new(entity_id),
            head_yaw: encode_angle(head_yaw),
        }
    }
}

impl EncodePacket for RotateHeadPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        encode_entity_id(&self.entity_id, writer, protocol_version)?;
        self.head_yaw.encode(writer, protocol_version)?;
        Ok(())
    }
}
