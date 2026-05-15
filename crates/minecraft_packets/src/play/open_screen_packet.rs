use minecraft_protocol::prelude::*;
use pico_text_component::prelude::Component;

/// Clientbound Open Screen — instructs the client to open a container GUI.
///
/// Wire format by version:
/// - ≤ 1.8: `UByte window_id, String "minecraft:chest", Chat title, UByte 27`
/// - 1.9–1.13.x: `VarInt window_id, String "minecraft:chest", Chat title, UByte 27`
/// - 1.14+: `VarInt window_id, VarInt 2 (generic_9x3), Chat title`
pub struct OpenScreenPacket {
    pub window_id: u8,
    pub title: Component,
}

impl OpenScreenPacket {
    pub fn new(window_id: u8, title: Component) -> Self {
        Self { window_id, title }
    }
}

impl EncodePacket for OpenScreenPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        if version.is_before_inclusive(ProtocolVersion::V1_8) {
            self.window_id.encode(writer, version)?;
            "minecraft:chest".to_string().encode(writer, version)?;
            self.title.encode(writer, version)?;
            27u8.encode(writer, version)?;
        } else if version.is_before_inclusive(ProtocolVersion::V1_13_2) {
            // 1.9–1.13.x: VarInt window_id + String type + Chat title + UByte slots
            VarInt::new(i32::from(self.window_id)).encode(writer, version)?;
            "minecraft:chest".to_string().encode(writer, version)?;
            self.title.encode(writer, version)?;
            27u8.encode(writer, version)?;
        } else {
            // 1.14+: VarInt window_id + VarInt type_id (2 = generic_9x3) + Chat title
            VarInt::new(i32::from(self.window_id)).encode(writer, version)?;
            VarInt::new(2).encode(writer, version)?;
            self.title.encode(writer, version)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pico_text_component::prelude::parse_mini_message;

    fn make_packet() -> OpenScreenPacket {
        let title = parse_mini_message("Server Selector").unwrap();
        OpenScreenPacket::new(1, title)
    }

    fn encode(pkt: OpenScreenPacket, version: ProtocolVersion) -> Vec<u8> {
        let mut writer = BinaryWriter::default();
        pkt.encode(&mut writer, version).unwrap();
        writer.as_slice().to_vec()
    }

    #[test]
    fn pre_1_14_appends_string_type_and_num_slots_27() {
        // 1.8 uses u8 window_id; 1.9–1.13 use VarInt — both end with the 27-slot count byte.
        for version in [ProtocolVersion::V1_8, ProtocolVersion::V1_12_2] {
            let bytes = encode(make_packet(), version);
            assert_eq!(bytes[0], 0x01, "window_id for {version:?}");
            assert_eq!(
                bytes[bytes.len() - 1],
                27,
                "trailing slot count for {version:?}"
            );
        }
    }

    #[test]
    fn v1_14_and_later_uses_varint_type_id_without_slot_count() {
        // 1.14+: VarInt window_id, VarInt(2) = generic_9x3, Chat title — no trailing slot count.
        for version in [ProtocolVersion::V1_14, ProtocolVersion::V1_20_5] {
            let bytes = encode(make_packet(), version);
            assert_eq!(bytes[0], 0x01, "window_id for {version:?}");
            assert_eq!(bytes[1], 0x02, "generic_9x3 type id for {version:?}");
            assert_ne!(
                bytes[bytes.len() - 1],
                27,
                "must not have slot count for {version:?}"
            );
        }
    }
}
