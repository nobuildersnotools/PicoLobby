use crate::play::move_entity_packet::encode_entity_id;
use minecraft_protocol::prelude::*;

pub struct AnimatePacket {
    entity_id: VarInt,
    animation_id: u8,
}

impl AnimatePacket {
    const MAIN_HAND_SWING: u8 = 0;

    pub fn main_hand(entity_id: i32) -> Self {
        Self {
            entity_id: VarInt::new(entity_id),
            animation_id: Self::MAIN_HAND_SWING,
        }
    }
}

impl EncodePacket for AnimatePacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        encode_entity_id(&self.entity_id, writer, protocol_version)?;
        self.animation_id.encode(writer, protocol_version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(version: ProtocolVersion) -> Vec<u8> {
        let mut writer = BinaryWriter::new();
        AnimatePacket::main_hand(300)
            .encode(&mut writer, version)
            .unwrap();
        writer.into_inner()
    }

    #[test]
    fn encodes_legacy_entity_id_as_int() {
        assert_eq!(encode(ProtocolVersion::V1_7_2), [0, 0, 1, 44, 0]);
    }

    #[test]
    fn encodes_newer_entity_id_as_varint() {
        assert_eq!(encode(ProtocolVersion::V1_8), [172, 2, 0]);
        assert_eq!(encode(ProtocolVersion::V26_1), [172, 2, 0]);
    }
}
