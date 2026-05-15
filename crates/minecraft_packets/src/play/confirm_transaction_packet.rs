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
    fn round_trips_legacy_rejection() {
        let wire = [0x03u8, 0x00, 0x2a, 0x00]; // window=3, action=42, accepted=false

        let mut writer = BinaryWriter::default();
        ConfirmTransactionPacket::new(3, 42, false)
            .encode(&mut writer, ProtocolVersion::V1_12_2)
            .unwrap();
        assert_eq!(writer.as_slice(), &wire);

        let decoded = ConfirmTransactionPacket::decode(
            &mut BinaryReader::new(&wire),
            ProtocolVersion::V1_12_2,
        )
        .unwrap();
        assert_eq!(decoded.window_id, 3);
        assert_eq!(decoded.action_number, 42);
        assert!(!decoded.accepted);
    }
}
