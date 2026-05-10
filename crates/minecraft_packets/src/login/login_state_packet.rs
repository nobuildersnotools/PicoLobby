use minecraft_protocol::prelude::*;

#[derive(Default, PacketIn)]
pub struct LoginStartPacket {
    pub name: String,
    #[pvn(759..761)]
    #[allow(dead_code)]
    sig_data: Optional<SigData>,
    #[pvn(760..764)]
    profile_uuid: Optional<Uuid>,
    #[pvn(764..)]
    v1_20_2_player_uuid: Uuid,
}

impl LoginStartPacket {
    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn uuid(&self) -> Uuid {
        self.profile_uuid.unwrap_or(self.v1_20_2_player_uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn encode_login_start(
        protocol_version: ProtocolVersion,
        name: &str,
        public_key: Option<(&[i8], &[i8])>,
        optional_uuid: Option<Uuid>,
        plain_uuid: Option<Uuid>,
    ) -> Vec<u8> {
        let mut writer = BinaryWriter::new();
        name.to_string()
            .encode(&mut writer, protocol_version)
            .expect("encode name");

        if protocol_version.between_inclusive(ProtocolVersion::V1_19, ProtocolVersion::V1_19_1) {
            match public_key {
                Some((public_key, signature)) => {
                    true.encode(&mut writer, protocol_version)
                        .expect("encode public key presence");
                    0_i64
                        .encode(&mut writer, protocol_version)
                        .expect("encode key timestamp");
                    LengthPaddedVec::new(public_key.to_vec())
                        .encode(&mut writer, protocol_version)
                        .expect("encode public key");
                    LengthPaddedVec::new(signature.to_vec())
                        .encode(&mut writer, protocol_version)
                        .expect("encode key signature");
                }
                None => false
                    .encode(&mut writer, protocol_version)
                    .expect("encode public key absence"),
            }
        }

        if protocol_version.between_inclusive(ProtocolVersion::V1_19_1, ProtocolVersion::V1_20) {
            match optional_uuid {
                Some(uuid) => {
                    true.encode(&mut writer, protocol_version)
                        .expect("encode uuid presence");
                    writer.write(&uuid).expect("encode uuid");
                }
                None => false
                    .encode(&mut writer, protocol_version)
                    .expect("encode uuid absence"),
            }
        }

        if protocol_version.is_after_inclusive(ProtocolVersion::V1_20_2) {
            writer
                .write(&plain_uuid.unwrap_or_default())
                .expect("encode uuid");
        }

        writer.into_inner()
    }

    #[test]
    fn decodes_v1_19_1_profile_uuid_after_public_key() {
        let expected_uuid =
            Uuid::from_str("01234567-89ab-cdef-0123-456789abcdef").expect("valid uuid");
        let payload = encode_login_start(
            ProtocolVersion::V1_19_1,
            "PlayerName",
            Some((&[1, 2, 3], &[4, 5, 6])),
            Some(expected_uuid),
            None,
        );
        let mut reader = BinaryReader::new(&payload);

        let packet = LoginStartPacket::decode(&mut reader, ProtocolVersion::V1_19_1)
            .expect("decode login start");

        assert_eq!(packet.name(), "PlayerName");
        assert_eq!(packet.uuid(), expected_uuid);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn decodes_v1_19_1_absent_profile_uuid_as_nil() {
        let payload = encode_login_start(ProtocolVersion::V1_19_1, "PlayerName", None, None, None);
        let mut reader = BinaryReader::new(&payload);

        let packet = LoginStartPacket::decode(&mut reader, ProtocolVersion::V1_19_1)
            .expect("decode login start");

        assert_eq!(packet.uuid(), Uuid::nil());
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn decodes_v1_19_without_profile_uuid() {
        let payload = encode_login_start(ProtocolVersion::V1_19, "PlayerName", None, None, None);
        let mut reader = BinaryReader::new(&payload);

        let packet = LoginStartPacket::decode(&mut reader, ProtocolVersion::V1_19)
            .expect("decode login start");

        assert_eq!(packet.uuid(), Uuid::nil());
        assert_eq!(reader.remaining(), 0);
    }
}

#[derive(Default, PacketIn)]
#[allow(dead_code)]
struct SigData {
    /// When the key data will expire.
    timestamp: i64,
    /// Length of Public Key.
    public_key: LengthPaddedVec<i8>,
    /// The bytes of the public key signature the client received from Mojang.
    signature: LengthPaddedVec<i8>,
}
