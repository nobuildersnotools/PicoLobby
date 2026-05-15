use crate::login::Property;
use crate::play::move_entity_packet::{encode_angle, encode_entity_id};
use crate::play::set_entity_data_packet::EntityBaseFlags;
use minecraft_protocol::prelude::*;

const LEGACY_ABSOLUTE_POSITION_SCALE: f64 = 32.0;

/// Clientbound "Spawn Player" packet (`minecraft:add_player`), used for 1.8 through 1.20.
/// 1.7.2 uses the same generated packet name in this crate, but its payload includes the
/// player's name and inline profile properties before the fixed-point position.
///
/// In 1.20.2+ the separate spawn-player packet was merged into Spawn Entity. That path
/// is covered by `SpawnEntityPacket` with `minecraft:add_entity` for protocol ≥ 1.21
/// (where the generated reports expose the merged name).
pub struct SpawnPlayerPacket {
    entity_id: i32,
    uuid: Uuid,
    username: Option<String>,
    textures: Option<Property>,
    base_flags: EntityBaseFlags,
    x: f64,
    y: f64,
    z: f64,
    yaw: u8,
    pitch: u8,
}

impl SpawnPlayerPacket {
    pub fn new(entity_id: i32, uuid: Uuid, x: f64, y: f64, z: f64, yaw: f32, pitch: f32) -> Self {
        Self {
            entity_id,
            uuid,
            username: None,
            textures: None,
            base_flags: EntityBaseFlags::empty(),
            x,
            y,
            z,
            yaw: encode_angle(yaw),
            pitch: encode_angle(pitch),
        }
    }

    pub fn lobby_player(
        entity_id: i32,
        uuid: Uuid,
        username: String,
        textures: Option<Property>,
        base_flags: EntityBaseFlags,
        position: (f64, f64, f64),
        rotation: (f32, f32),
    ) -> Self {
        Self {
            entity_id,
            uuid,
            username: Some(username),
            textures,
            base_flags,
            x: position.0,
            y: position.1,
            z: position.2,
            yaw: encode_angle(rotation.0),
            pitch: encode_angle(rotation.1),
        }
    }
}

impl EncodePacket for SpawnPlayerPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        if version.is_before_inclusive(ProtocolVersion::V1_7_6) {
            return self.encode_v1_7_2(writer, version);
        }

        encode_entity_id(&VarInt::new(self.entity_id), writer, version)?;
        UuidAsLongs::new(self.uuid).encode(writer, version)?;

        if version.is_before_inclusive(ProtocolVersion::V1_8) {
            // Fixed-point absolute position: multiply by 32 and truncate to i32.
            encode_legacy_absolute(self.x).encode(writer, version)?;
            encode_legacy_absolute(self.y).encode(writer, version)?;
            encode_legacy_absolute(self.z).encode(writer, version)?;
        } else {
            self.x.encode(writer, version)?;
            self.y.encode(writer, version)?;
            self.z.encode(writer, version)?;
        }

        self.yaw.encode(writer, version)?;
        self.pitch.encode(writer, version)?;

        if version.is_before_inclusive(ProtocolVersion::V1_8) {
            // Current held item slot (always 0 = empty hand).
            0_i16.encode(writer, version)?;
        }

        if version.is_before_inclusive(ProtocolVersion::V1_8) {
            // 1.8 keeps entity metadata inside the spawn-player payload.
            0x7F_u8.encode(writer, version)?;
        } else if version.between_inclusive(ProtocolVersion::V1_9, ProtocolVersion::V1_14_4) {
            // 1.9 through 1.14.4 also carry an inline entity metadata list.
            // The lobby sends actual player metadata separately, so terminate an
            // empty modern metadata list here.
            0xFF_u8.encode(writer, version)?;
        }
        // 1.15 through 1.20 have no embedded metadata field; lobby metadata
        // follows in a separate SetEntityMetadata packet.

        Ok(())
    }
}

impl SpawnPlayerPacket {
    fn encode_v1_7_2(
        &self,
        writer: &mut BinaryWriter,
        version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        VarInt::new(self.entity_id).encode(writer, version)?;
        UuidAsString::new(self.uuid).encode(writer, version)?;
        self.username
            .as_deref()
            .unwrap_or("")
            .to_owned()
            .encode(writer, version)?;
        encode_v1_7_2_properties(self.textures.as_ref(), writer, version)?;
        encode_legacy_absolute(self.x).encode(writer, version)?;
        encode_legacy_absolute(self.y).encode(writer, version)?;
        encode_legacy_absolute(self.z).encode(writer, version)?;
        self.yaw.encode(writer, version)?;
        self.pitch.encode(writer, version)?;
        0_i16.encode(writer, version)?;
        encode_v1_7_2_player_metadata(self.base_flags, writer, version)
    }
}

fn encode_v1_7_2_properties(
    textures: Option<&Property>,
    writer: &mut BinaryWriter,
    version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    let Some(textures) = textures else {
        return VarInt::new(0).encode(writer, version);
    };

    VarInt::new(1).encode(writer, version)?;
    "textures".to_owned().encode(writer, version)?;
    textures.value().to_owned().encode(writer, version)?;
    textures
        .signature()
        .unwrap_or_default()
        .encode(writer, version)
}

fn encode_v1_7_2_player_metadata(
    base_flags: EntityBaseFlags,
    writer: &mut BinaryWriter,
    version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    0_u8.encode(writer, version)?;
    (base_flags.as_metadata_byte() as u8).encode(writer, version)?;
    0x7F_u8.encode(writer, version)
}

fn encode_legacy_absolute(position: f64) -> i32 {
    (position * LEGACY_ABSOLUTE_POSITION_SCALE).floor() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_packet() -> SpawnPlayerPacket {
        SpawnPlayerPacket::new(300, Uuid::from_u128(99), 10.5, 64.0, -5.0, 90.0, 0.0)
    }

    fn encode(packet: SpawnPlayerPacket, version: ProtocolVersion) -> Vec<u8> {
        let mut writer = BinaryWriter::new();
        packet.encode(&mut writer, version).unwrap();
        writer.into_inner()
    }

    #[test]
    fn encodes_spawn_player_for_v1_8() {
        let data = encode(make_packet(), ProtocolVersion::V1_8);

        // entity_id VarInt(300) = [0xac, 0x02]
        // uuid as longs: 0x0000_0000_0000_0000_0000_0000_0000_0063
        // x = (10.5 * 32).floor() = 336 = 0x00_00_01_50
        // y = (64.0 * 32).floor() = 2048 = 0x00_00_08_00
        // z = (-5.0 * 32).floor() = -160 = 0xFF_FF_FF_60
        // yaw = 90° → 64, pitch = 0° → 0
        // current_item = 0 (i16)
        // metadata terminator = 0x7f
        assert_eq!(data[0..2], [0xac, 0x02]); // entity_id
        assert_eq!(&data[18..22], [0x00, 0x00, 0x01, 0x50]); // x fixed-point
        assert_eq!(data[22..26], [0x00, 0x00, 0x08, 0x00]); // y fixed-point
        // z = -160 = 0xFFFFFF60
        assert_eq!(data[26..30], [0xFF, 0xFF, 0xFF, 0x60]); // z fixed-point
        assert_eq!(data[30], 64); // yaw = 90°
        assert_eq!(data[31], 0); // pitch = 0°
        assert_eq!(&data[32..34], [0, 0]); // current_item = 0 (i16)
        assert_eq!(data[34], 0x7F); // metadata terminator
    }

    #[test]
    fn encodes_spawn_player_for_v1_7_2_named_entity_shape() {
        let packet = SpawnPlayerPacket::lobby_player(
            300,
            Uuid::from_u128(99),
            "player2".to_owned(),
            None,
            EntityBaseFlags::crouching(),
            (10.5, 64.0, -5.0),
            (90.0, 0.0),
        );
        let data = encode(packet, ProtocolVersion::V1_7_2);

        assert_eq!(data[0..2], [0xac, 0x02]); // entity_id VarInt
        assert_eq!(data[2], 32); // dashless UUID string length
        assert_eq!(&data[28..35], b"0000063"); // end of dashless UUID
        assert_eq!(data[35], 7); // username length
        assert_eq!(&data[36..43], b"player2");
        assert_eq!(data[43], 0); // property count
        assert_eq!(&data[44..48], [0x00, 0x00, 0x01, 0x50]); // x fixed-point
        assert_eq!(&data[48..52], [0x00, 0x00, 0x08, 0x00]); // y fixed-point
        assert_eq!(&data[52..56], [0xFF, 0xFF, 0xFF, 0x60]); // z fixed-point
        assert_eq!(data[56], 64); // yaw
        assert_eq!(data[57], 0); // pitch
        assert_eq!(&data[58..60], [0, 0]); // current_item
        assert_eq!(&data[60..63], [0, 0x02, 0x7f]); // crouching base flags + terminator
    }

    #[test]
    fn encodes_spawn_player_for_v1_12_2() {
        let data = encode(make_packet(), ProtocolVersion::V1_12_2);

        // entity_id VarInt(300) = [0xac, 0x02]
        // uuid as 2 longs (16 bytes)
        // x, y, z as f64 (24 bytes)
        // yaw, pitch (2 bytes)
        // empty modern metadata list terminator in 1.9 through 1.14.4
        assert_eq!(data[0..2], [0xac, 0x02]); // entity_id
        let x_bytes = f64::to_be_bytes(10.5);
        assert_eq!(data[18..26], x_bytes); // x as f64
        assert_eq!(data[42], 64); // yaw = 90°
        assert_eq!(data[43], 0); // pitch = 0°
        assert_eq!(data[44], 0xFF); // metadata terminator
        assert_eq!(data.len(), 45);
    }

    #[test]
    fn encodes_spawn_player_for_v1_14_4_with_empty_metadata_list() {
        let data = encode(make_packet(), ProtocolVersion::V1_14_4);

        assert_eq!(data[0..2], [0xac, 0x02]); // entity_id
        assert_eq!(data[42], 64); // yaw = 90°
        assert_eq!(data[43], 0); // pitch = 0°
        assert_eq!(data[44], 0xFF); // metadata terminator
        assert_eq!(data.len(), 45);
    }

    #[test]
    fn encodes_spawn_player_for_v1_15_2_without_metadata_terminator() {
        let data = encode(make_packet(), ProtocolVersion::V1_15_2);

        assert_eq!(data[0..2], [0xac, 0x02]); // entity_id
        assert_eq!(data[42], 64); // yaw = 90°
        assert_eq!(data[43], 0); // pitch = 0°
        assert_eq!(data.len(), 44); // no metadata terminator
    }

    #[test]
    fn encodes_spawn_player_for_v1_19_4() {
        let data = encode(make_packet(), ProtocolVersion::V1_19_4);

        // entity_id VarInt(300) = [0xac, 0x02]
        // uuid as 2 longs (16 bytes)
        // x, y, z as f64 (24 bytes)
        // yaw, pitch (2 bytes)
        // NO metadata terminator in 1.19.4+
        assert_eq!(data[0..2], [0xac, 0x02]);
        let x_bytes = f64::to_be_bytes(10.5);
        assert_eq!(data[18..26], x_bytes);
        assert_eq!(data[42], 64); // yaw
        assert_eq!(data[43], 0); // pitch
        assert_eq!(data.len(), 44); // no metadata terminator
    }

    #[test]
    fn encodes_spawn_player_for_v1_20() {
        let data = encode(make_packet(), ProtocolVersion::V1_20);

        // Same as 1.19.4 - no metadata terminator
        assert_eq!(data.len(), 44);
        assert_eq!(data[42], 64); // yaw
        assert_eq!(data[43], 0); // pitch
    }
}
