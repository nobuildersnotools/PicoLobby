use minecraft_protocol::prelude::*;

pub struct PlayerInputPacket {
    flags: Option<u8>,
}

impl PlayerInputPacket {
    const SHIFT: u8 = 0x20;

    pub const fn shift(&self) -> Option<bool> {
        match self.flags {
            Some(flags) => Some((flags & Self::SHIFT) != 0),
            None => None,
        }
    }
}

impl DecodePacket for PlayerInputPacket {
    fn decode(
        reader: &mut BinaryReader,
        protocol_version: ProtocolVersion,
    ) -> Result<Self, BinaryReaderError> {
        if protocol_version.is_after_inclusive(ProtocolVersion::V1_21_9) {
            let flags = u8::decode(reader, protocol_version)?;
            return Ok(Self { flags: Some(flags) });
        }

        let _sideways = f32::decode(reader, protocol_version)?;
        let _forward = f32::decode(reader, protocol_version)?;
        let _legacy_flags = u8::decode(reader, protocol_version)?;
        Ok(Self { flags: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(data: &[u8], protocol_version: ProtocolVersion) -> PlayerInputPacket {
        let mut reader = BinaryReader::new(data);
        PlayerInputPacket::decode(&mut reader, protocol_version).unwrap()
    }

    #[test]
    fn modern_shift_flag_decoded_for_v1_21_9_and_later() {
        // Both V1_21_9 and V26_1 use the compact flags byte; 0x20 = shift bit set.
        for version in [ProtocolVersion::V1_21_9, ProtocolVersion::V26_1] {
            assert_eq!(decode(&[0x20], version).shift(), Some(true), "{version:?}");
            assert_eq!(decode(&[0x40], version).shift(), Some(false), "{version:?}");
        }
    }

    #[test]
    fn legacy_vehicle_input_does_not_map_to_shift() {
        let packet = decode(
            &[0x3f, 0x80, 0, 0, 0xbf, 0x80, 0, 0, 0x03],
            ProtocolVersion::V1_21,
        );
        assert_eq!(packet.shift(), None);
    }
}
