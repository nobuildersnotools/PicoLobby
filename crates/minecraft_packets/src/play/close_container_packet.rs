use minecraft_protocol::prelude::*;

/// Close Container — sent in both directions with a single `window_id` byte.
///
/// Clientbound: server forces the container closed.
/// Serverbound: client notifies the server it closed the container.
///
/// Wire format is identical across all supported protocol versions:
/// `Byte window_id`
pub struct CloseContainerPacket {
    pub window_id: u8,
}

impl CloseContainerPacket {
    pub fn new(window_id: u8) -> Self {
        Self { window_id }
    }
}

impl EncodePacket for CloseContainerPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        self.window_id.encode(writer, version)
    }
}

impl DecodePacket for CloseContainerPacket {
    fn decode(
        reader: &mut BinaryReader,
        version: ProtocolVersion,
    ) -> Result<Self, BinaryReaderError> {
        let window_id = u8::decode(reader, version)?;
        Ok(Self { window_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_is_a_single_byte() {
        let mut writer = BinaryWriter::default();
        CloseContainerPacket::new(3)
            .encode(&mut writer, ProtocolVersion::V1_21)
            .unwrap();
        assert_eq!(writer.as_slice(), &[0x03]);

        let decoded =
            CloseContainerPacket::decode(&mut BinaryReader::new(&[0x05]), ProtocolVersion::V1_8)
                .unwrap();
        assert_eq!(decoded.window_id, 5);
    }

    #[test]
    fn round_trips_across_versions() {
        for version in [
            ProtocolVersion::V1_7_2,
            ProtocolVersion::V1_8,
            ProtocolVersion::V1_12_2,
            ProtocolVersion::V1_20_5,
            ProtocolVersion::V1_21,
            ProtocolVersion::V26_1,
        ] {
            let mut writer = BinaryWriter::default();
            CloseContainerPacket::new(7)
                .encode(&mut writer, version)
                .unwrap();
            let bytes = writer.as_slice().to_vec();
            let decoded =
                CloseContainerPacket::decode(&mut BinaryReader::new(&bytes), version).unwrap();
            assert_eq!(decoded.window_id, 7, "failed for {version:?}");
        }
    }
}
