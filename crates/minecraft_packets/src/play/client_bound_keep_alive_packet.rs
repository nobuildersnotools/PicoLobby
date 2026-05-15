use minecraft_protocol::prelude::*;
use rand::RngExt;
use std::num::TryFromIntError;

/// This packet exists for all versions of the game from 1.7.2 to the latest at the time (1.21.4).
#[derive(PacketOut)]
pub struct ClientBoundKeepAlivePacket {
    #[pvn(340..)]
    v1_12_2_id: i64,
    #[pvn(47..340)]
    v1_8_id: VarInt,
    #[pvn(..47)]
    id: i32,
}

impl ClientBoundKeepAlivePacket {
    pub fn new(id: i32) -> Result<Self, TryFromIntError> {
        Ok(Self {
            v1_12_2_id: id.into(),
            v1_8_id: id.into(),
            id,
        })
    }

    pub fn random() -> Result<Self, TryFromIntError> {
        Self::new(get_random_i32())
    }
}

fn get_random_i32() -> i32 {
    let mut rng = rand::rng();
    rng.random()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_id_0_with_correct_wire_width_per_era() {
        // pre-1.8: i32 (4 bytes), 1.8–1.12.1: VarInt, 1.12.2+: i64 (8 bytes)
        let cases: &[(ProtocolVersion, &[u8])] = &[
            (ProtocolVersion::V1_7_2, &[0x00, 0x00, 0x00, 0x00]),
            (ProtocolVersion::V1_8, &[0x00]),
            (
                ProtocolVersion::V1_12_2,
                &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            ),
        ];
        for &(version, expected) in cases {
            let packet = ClientBoundKeepAlivePacket::new(0).unwrap();
            let mut writer = BinaryWriter::new();
            packet.encode(&mut writer, version).unwrap();
            assert_eq!(
                writer.into_inner(),
                expected,
                "wrong encoding for {version:?}"
            );
        }
    }
}
