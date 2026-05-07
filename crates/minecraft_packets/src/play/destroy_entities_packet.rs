use minecraft_protocol::prelude::*;

pub struct DestroyEntitiesPacket {
    entity_ids: Vec<i32>,
}

impl DestroyEntitiesPacket {
    pub fn new(entity_ids: Vec<i32>) -> Self {
        Self { entity_ids }
    }

    pub fn single(entity_id: i32) -> Self {
        Self::new(vec![entity_id])
    }
}

impl EncodePacket for DestroyEntitiesPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        if protocol_version.is_before_inclusive(ProtocolVersion::V1_7_6) {
            let count = u8::try_from(self.entity_ids.len())?;
            count.encode(writer, protocol_version)?;
            for entity_id in &self.entity_ids {
                entity_id.encode(writer, protocol_version)?;
            }
            return Ok(());
        }

        VarInt::new(i32::try_from(self.entity_ids.len())?).encode(writer, protocol_version)?;
        for entity_id in &self.entity_ids {
            VarInt::new(*entity_id).encode(writer, protocol_version)?;
        }
        Ok(())
    }
}
