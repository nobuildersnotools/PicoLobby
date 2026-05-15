use minecraft_protocol::prelude::*;

pub struct AttackPacket {
    target_entity_id: i32,
}

impl AttackPacket {
    pub const fn target_entity_id(&self) -> i32 {
        self.target_entity_id
    }
}

impl DecodePacket for AttackPacket {
    fn decode(
        reader: &mut BinaryReader,
        version: ProtocolVersion,
    ) -> Result<Self, BinaryReaderError> {
        Ok(Self {
            target_entity_id: VarInt::decode(reader, version)?.inner(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_target_entity_id() {
        let mut reader = BinaryReader::new(&[0xac, 0x02]);
        let packet = AttackPacket::decode(&mut reader, ProtocolVersion::V26_1).unwrap();

        assert_eq!(packet.target_entity_id(), 300);
    }
}
