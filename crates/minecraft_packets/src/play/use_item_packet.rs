use minecraft_protocol::prelude::*;

/// Serverbound Use Item — sent when the player right-clicks with an item in
/// hand without targeting a block.  Introduced in 1.9; pre-1.9 clients use
/// the legacy block placement interaction packet instead.
///
/// Hand: 0 = main hand, 1 = off hand.
pub struct UseItemPacket {
    hand: i32,
}

impl UseItemPacket {
    pub const fn is_main_hand(&self) -> bool {
        self.hand == 0
    }
}

impl DecodePacket for UseItemPacket {
    fn decode(
        reader: &mut BinaryReader,
        version: ProtocolVersion,
    ) -> Result<Self, BinaryReaderError> {
        let hand = VarInt::decode(reader, version)?.inner();
        // Versions 1.19+ also send a sequence VarInt.
        if version.is_after_inclusive(ProtocolVersion::V1_19) {
            let _sequence = VarInt::decode(reader, version)?;
        }
        Ok(Self { hand })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_main_hand() {
        let bytes = [0x00]; // VarInt 0 = main hand
        let mut reader = BinaryReader::new(&bytes);
        let pkt = UseItemPacket::decode(&mut reader, ProtocolVersion::V1_18_2).expect("decode");
        assert!(pkt.is_main_hand());
    }

    #[test]
    fn decodes_off_hand() {
        let bytes = [0x01]; // VarInt 1 = off hand
        let mut reader = BinaryReader::new(&bytes);
        let pkt = UseItemPacket::decode(&mut reader, ProtocolVersion::V1_18_2).expect("decode");
        assert!(!pkt.is_main_hand());
    }

    #[test]
    fn decodes_v1_19_sequence() {
        let bytes = [0x00, 0x2a]; // hand = main hand, sequence = 42
        let mut reader = BinaryReader::new(&bytes);
        let pkt = UseItemPacket::decode(&mut reader, ProtocolVersion::V1_19).expect("decode");
        assert!(pkt.is_main_hand());
    }
}
