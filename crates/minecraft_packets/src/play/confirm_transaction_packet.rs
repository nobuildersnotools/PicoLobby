use minecraft_protocol::prelude::*;

/// Legacy Confirm Transaction packet used for inventory click acknowledgements
/// before the container state-id protocol replaced it.
pub struct ConfirmTransactionPacket {
    pub window_id: u8,
    pub action_number: i16,
    pub accepted: bool,
}

impl ConfirmTransactionPacket {
    pub const fn new(window_id: u8, action_number: i16, accepted: bool) -> Self {
        Self {
            window_id,
            action_number,
            accepted,
        }
    }
}

impl EncodePacket for ConfirmTransactionPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        self.window_id.encode(writer, version)?;
        self.action_number.encode(writer, version)?;
        self.accepted.encode(writer, version)
    }
}

impl DecodePacket for ConfirmTransactionPacket {
    fn decode(
        reader: &mut BinaryReader,
        version: ProtocolVersion,
    ) -> Result<Self, BinaryReaderError> {
        let window_id = u8::decode(reader, version)?;
        let action_number = i16::decode(reader, version)?;
        let accepted = bool::decode(reader, version)?;
        Ok(Self {
            window_id,
            action_number,
            accepted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_legacy_rejection() {
        let packet = ConfirmTransactionPacket::new(3, 42, false);
        let mut writer = BinaryWriter::default();
        packet
            .encode(&mut writer, ProtocolVersion::V1_12_2)
            .unwrap();

        assert_eq!(writer.as_slice(), &[0x03, 0x00, 0x2a, 0x00]);
    }

    #[test]
    fn decodes_legacy_acknowledgement() {
        let bytes = [0x03, 0x00, 0x2a, 0x00];
        let mut reader = BinaryReader::new(&bytes);
        let packet = ConfirmTransactionPacket::decode(&mut reader, ProtocolVersion::V1_12_2)
            .expect("valid packet");

        assert_eq!(packet.window_id, 3);
        assert_eq!(packet.action_number, 42);
        assert!(!packet.accepted);
    }
}
