use minecraft_protocol::prelude::*;

pub struct SetEntityMetadataPacket {
    entity_id: VarInt,
    entity_metadata: Vec<EntityMetadata>,
}

impl SetEntityMetadataPacket {
    pub fn player_baseline(entity_id: i32) -> Self {
        Self::player(entity_id, EntityBaseFlags::default())
    }

    pub fn player(entity_id: i32, base_flags: EntityBaseFlags) -> Self {
        let entity_metadata = vec![
            EntityMetadata::BaseFlags(Metadata::Byte(base_flags.as_metadata_byte())),
            EntityMetadata::Pose(Metadata::Pose(EntityPose::from_base_flags(base_flags))),
            EntityMetadata::SkinParts(Metadata::Byte(
                0x01 | 0x02 | 0x04 | 0x08 | 0x10 | 0x20 | 0x40,
            )),
            EntityMetadata::End,
        ];

        Self {
            entity_id: entity_id.into(),
            entity_metadata,
        }
    }

    pub fn skin_layers(entity_id: i32) -> Self {
        Self::player_baseline(entity_id)
    }
}

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct EntityBaseFlags {
    bits: u8,
}

impl EntityBaseFlags {
    const CROUCHING: u8 = 0x02;

    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub const fn crouching() -> Self {
        Self {
            bits: Self::CROUCHING,
        }
    }

    const fn is_crouching(self) -> bool {
        (self.bits & Self::CROUCHING) != 0
    }

    pub(crate) const fn as_metadata_byte(self) -> i8 {
        self.bits as i8
    }
}

impl EncodePacket for SetEntityMetadataPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        crate::play::move_entity_packet::encode_entity_id(
            &self.entity_id,
            writer,
            protocol_version,
        )?;
        self.entity_metadata.encode(writer, protocol_version)
    }
}

enum EntityMetadata {
    BaseFlags(Metadata),
    Pose(Metadata),
    SkinParts(Metadata),
    End,
}

impl EntityMetadata {
    fn get_index(&self, protocol_version: ProtocolVersion) -> u8 {
        match self {
            Self::BaseFlags(_) => 0,
            Self::Pose(_) => 6,
            Self::SkinParts(_) => {
                if protocol_version.is_after_inclusive(ProtocolVersion::V1_21_9) {
                    16
                } else if protocol_version.is_after_inclusive(ProtocolVersion::V1_17) {
                    17
                } else if protocol_version.is_after_inclusive(ProtocolVersion::V1_15) {
                    16
                } else if protocol_version.is_after_inclusive(ProtocolVersion::V1_14) {
                    15
                } else if protocol_version.is_after_inclusive(ProtocolVersion::V1_12) {
                    13
                } else if protocol_version.is_after_inclusive(ProtocolVersion::V1_9) {
                    15
                } else if protocol_version.is_after_inclusive(ProtocolVersion::V1_8) {
                    10
                } else {
                    0
                }
            }
            Self::End => {
                if protocol_version.is_after_inclusive(ProtocolVersion::V1_9) {
                    255
                } else {
                    127
                }
            }
        }
    }
}

impl EncodePacket for EntityMetadata {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        if matches!(self, EntityMetadata::SkinParts(_))
            && protocol_version.is_before_inclusive(ProtocolVersion::V1_7_6)
        {
            return Ok(());
        }
        if matches!(self, EntityMetadata::Pose(_))
            && protocol_version.is_before_inclusive(ProtocolVersion::V1_20_3)
        {
            return Ok(());
        }

        self.get_index(protocol_version)
            .encode(writer, protocol_version)?;
        match self {
            EntityMetadata::BaseFlags(metadata)
            | EntityMetadata::Pose(metadata)
            | EntityMetadata::SkinParts(metadata) => {
                metadata.encode(writer, protocol_version)?;
            }
            EntityMetadata::End => {}
        }
        Ok(())
    }
}

#[derive(Clone)]
enum Metadata {
    Byte(i8),
    Pose(EntityPose),
}

#[derive(Copy, Clone)]
enum EntityPose {
    Standing,
    Crouching,
}

impl EntityPose {
    const STANDING_ID: i32 = 0;
    const CROUCHING_ID: i32 = 5;

    const fn from_base_flags(base_flags: EntityBaseFlags) -> Self {
        if base_flags.is_crouching() {
            Self::Crouching
        } else {
            Self::Standing
        }
    }

    const fn protocol_id(self) -> i32 {
        match self {
            Self::Standing => Self::STANDING_ID,
            Self::Crouching => Self::CROUCHING_ID,
        }
    }
}

impl Metadata {
    fn get_type_id(&self, protocol_version: ProtocolVersion) -> u8 {
        match self {
            Metadata::Byte(_) => 0,
            Metadata::Pose(_) => {
                if protocol_version.is_after_inclusive(ProtocolVersion::V1_21_9) {
                    20
                } else {
                    21
                }
            }
        }
    }
}

impl EncodePacket for Metadata {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        let type_id = self.get_type_id(protocol_version);
        let final_byte = if protocol_version.is_after_inclusive(ProtocolVersion::V1_9) {
            type_id.encode(writer, protocol_version)?;
            match self {
                Self::Byte(value) => *value as u8,
                Self::Pose(value) => {
                    VarInt::new(value.protocol_id()).encode(writer, protocol_version)?;
                    return Ok(());
                }
            }
        } else {
            match self {
                Self::Byte(value) => (type_id << 5) | (*value as u8),
                Self::Pose(_) => return Ok(()),
            }
        };
        final_byte.encode(writer, protocol_version)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(packet: SetEntityMetadataPacket, version: ProtocolVersion) -> Vec<u8> {
        let mut writer = BinaryWriter::new();
        packet.encode(&mut writer, version).unwrap();
        writer.into_inner()
    }

    #[test]
    fn baseline_metadata_skin_parts_index_shifts_at_v1_21_9() {
        // V1_21 and V1_20_5 both use skin-parts index 17, pose type-id 21.
        // V26_1 (≥1.21.9) uses index 16, pose type-id 20.
        let cases: &[(ProtocolVersion, &[u8])] = &[
            (
                ProtocolVersion::V1_21,
                &[0xac, 0x02, 0, 0, 0, 6, 21, 0, 17, 0, 0x7f, 0xff],
            ),
            (
                ProtocolVersion::V26_1,
                &[0xac, 0x02, 0, 0, 0, 6, 20, 0, 16, 0, 0x7f, 0xff],
            ),
        ];
        for &(version, expected) in cases {
            assert_eq!(
                encode(SetEntityMetadataPacket::player_baseline(300), version),
                expected,
                "{version:?}"
            );
        }
    }

    #[test]
    fn crouching_metadata_encodes_base_flag_and_pose() {
        // V1_20_5 and V1_21 share the same encoding; V26_1 shifts the indices.
        let cases: &[(ProtocolVersion, &[u8])] = &[
            (
                ProtocolVersion::V1_20_5,
                &[0xac, 0x02, 0, 0, 0x02, 6, 21, 5, 17, 0, 0x7f, 0xff],
            ),
            (
                ProtocolVersion::V1_21,
                &[0xac, 0x02, 0, 0, 0x02, 6, 21, 5, 17, 0, 0x7f, 0xff],
            ),
            (
                ProtocolVersion::V26_1,
                &[0xac, 0x02, 0, 0, 0x02, 6, 20, 5, 16, 0, 0x7f, 0xff],
            ),
        ];
        for &(version, expected) in cases {
            assert_eq!(
                encode(
                    SetEntityMetadataPacket::player(300, EntityBaseFlags::crouching()),
                    version
                ),
                expected,
                "{version:?}"
            );
        }
    }

    #[test]
    fn v1_8_uses_legacy_byte_metadata() {
        let data = encode(
            SetEntityMetadataPacket::player_baseline(300),
            ProtocolVersion::V1_8,
        );
        assert_eq!(data, &[0xac, 0x02, 0, 0, 10, 0x7f, 0x7f]);
    }

    #[test]
    fn v1_7_uses_big_endian_i32_entity_id() {
        let data = encode(
            SetEntityMetadataPacket::player(300, EntityBaseFlags::crouching()),
            ProtocolVersion::V1_7_2,
        );
        assert_eq!(data, &[0, 0, 1, 44, 0, 0x02, 0x7f]);
    }
}
