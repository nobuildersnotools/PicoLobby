use minecraft_protocol::prelude::*;

/// Serverbound Set Held Item — sent when the player changes their selected
/// hotbar slot (0–8) via the scroll wheel or 1–9 number keys.
pub struct ServerBoundSetHeldItemPacket {
    slot: i16,
}

impl ServerBoundSetHeldItemPacket {
    /// Slot index in 0–8 range, or -1 if not valid.
    pub const fn selected_slot(&self) -> u8 {
        if self.slot >= 0 && self.slot <= 8 {
            self.slot as u8
        } else {
            0
        }
    }
}

impl DecodePacket for ServerBoundSetHeldItemPacket {
    fn decode(
        reader: &mut BinaryReader,
        _version: ProtocolVersion,
    ) -> Result<Self, BinaryReaderError> {
        let slot = i16::decode(reader, _version)?;
        Ok(Self { slot })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_slot_4() {
        let bytes = [0x00, 0x04];
        let mut reader = BinaryReader::new(&bytes);
        let pkt = ServerBoundSetHeldItemPacket::decode(&mut reader, ProtocolVersion::V1_20_5)
            .expect("decode");
        assert_eq!(pkt.selected_slot(), 4);
    }

    #[test]
    fn clamps_out_of_range_slot() {
        let bytes = [0x00, 0x09]; // slot 9 is invalid
        let mut reader = BinaryReader::new(&bytes);
        let pkt = ServerBoundSetHeldItemPacket::decode(&mut reader, ProtocolVersion::V1_20_5)
            .expect("decode");
        assert_eq!(pkt.selected_slot(), 0);
    }
}
