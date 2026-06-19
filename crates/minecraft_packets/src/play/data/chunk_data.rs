use crate::play::data::chunk_context::{VoidChunkContext, WorldContext};
use crate::play::data::chunk_section::ChunkSection;
use crate::play::data::encode_as_bytes::EncodeAsBytes;
use crate::play::data::palette_container::PaletteContainer;
use blocks_report::{BlockEntityTypeLookup, get_block_entity_lookup};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use minecraft_protocol::prelude::*;
use pico_nbt::{IndexMap, Value};
use serde::Serialize;
use std::io::Write;

fn height_maps(protocol_version: ProtocolVersion) -> Value {
    // The MOTION_BLOCKING heightmap stores 256 entries of 9 bits each. Prior to
    // 1.16 these are packed as an uninterrupted bit stream where entries may span
    // long boundaries, using exactly ceil(256 * 9 / 64) = 36 longs. From 1.16 on,
    // entries no longer span longs (7 per long, one padding bit each), needing
    // ceil(256 / 7) = 37 longs. Sending 37 longs to a 1.14/1.15 client makes its
    // BitStorage copy past the end of its 36-long backing array.
    let long_count = if protocol_version.is_before_inclusive(ProtocolVersion::V1_15_2) {
        36
    } else {
        37
    };
    let mut compound = IndexMap::new();
    compound.insert(
        "MOTION_BLOCKING".to_string(),
        Value::LongArray(vec![0; long_count]),
    );
    Value::Compound(compound)
}

pub struct ChunkData {
    v1_21_5_height_maps: LengthPaddedVec<HeightMap>,

    /// Biome IDs, ordered by x then z then y, in 4×4×4 blocks.
    /// Up until 1.17.1 included
    v1_16_2_biomes: LengthPaddedVec<VarInt>,

    /// This array is always of length 1024
    biomes: Vec<i32>,

    data: EncodeAsBytes<Vec<ChunkSection>>,

    // 1.17 and below
    block_entities: LengthPaddedVec<Value>,

    // 1.18+
    v1_18_block_entities: LengthPaddedVec<ChunkBlockEntity>,
}

impl EncodePacket for ChunkData {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        if protocol_version.is_before_inclusive(ProtocolVersion::V1_15_2) {
            self.encode_pre_v1_16(writer, protocol_version)?;
            return Ok(());
        }

        if protocol_version.is_before_inclusive(ProtocolVersion::V1_21_4) {
            height_maps(protocol_version).encode(writer, protocol_version)?;
        } else {
            self.v1_21_5_height_maps.encode(writer, protocol_version)?;
        }

        if protocol_version.between_inclusive(ProtocolVersion::V1_16_2, ProtocolVersion::V1_17_1) {
            self.v1_16_2_biomes.encode(writer, protocol_version)?;
        }

        if protocol_version.is_before_inclusive(ProtocolVersion::V1_16_1) {
            self.biomes.encode(writer, protocol_version)?;
        }

        self.data.encode(writer, protocol_version)?;

        if protocol_version.is_before_inclusive(ProtocolVersion::V1_17_1) {
            self.block_entities.encode(writer, protocol_version)?;
        } else {
            self.v1_18_block_entities.encode(writer, protocol_version)?;
        }

        Ok(())
    }
}

impl ChunkData {
    pub fn void(context: VoidChunkContext) -> Self {
        let section_count = context.dimension_height / ChunkSection::SECTION_SIZE;

        Self {
            v1_21_5_height_maps: LengthPaddedVec::new(vec![HeightMap {
                height_map_type: VarInt::new(4), // Motionblock type
                data: LengthPaddedVec::new(vec![0; 37]),
            }]),
            v1_16_2_biomes: LengthPaddedVec::new(vec![VarInt::new(context.biome_index); 1024]),
            biomes: vec![context.biome_index; 1024],
            data: EncodeAsBytes::new(vec![
                ChunkSection::void(context.biome_index);
                section_count as usize
            ]),
            block_entities: LengthPaddedVec::default(),
            v1_18_block_entities: LengthPaddedVec::default(),
        }
    }

    pub fn from_schematic(
        chunk_context: VoidChunkContext,
        schematic_context: &WorldContext,
        protocol_version: ProtocolVersion,
    ) -> Self {
        let mut data = Vec::new();
        let negative_section_count =
            chunk_context.dimension_min_y.abs() / ChunkSection::SECTION_SIZE;
        let positive_section_count =
            chunk_context.dimension_height / ChunkSection::SECTION_SIZE - negative_section_count;

        for section_y in -negative_section_count..positive_section_count {
            let coordinates =
                Coordinates::new(chunk_context.chunk_x, section_y, chunk_context.chunk_z);
            let section = ChunkSection::from_schematic(
                schematic_context,
                coordinates,
                chunk_context.biome_index,
            );
            data.push(section);
        }

        let block_entity_lookup = get_block_entity_lookup(protocol_version);

        // Process block entities for this chunk
        let (block_entities_legacy, block_entities) = Self::collect_chunk_block_entities(
            &chunk_context,
            schematic_context,
            &block_entity_lookup,
            protocol_version,
        );

        Self {
            v1_21_5_height_maps: LengthPaddedVec::new(vec![HeightMap {
                height_map_type: VarInt::new(4), // Motionblock type
                data: LengthPaddedVec::new(vec![0; 37]),
            }]),
            v1_16_2_biomes: LengthPaddedVec::new(vec![
                VarInt::new(chunk_context.biome_index);
                1024
            ]),
            biomes: vec![chunk_context.biome_index; 1024],
            data: EncodeAsBytes::new(data),
            block_entities: LengthPaddedVec::new(block_entities_legacy),
            v1_18_block_entities: LengthPaddedVec::new(block_entities),
        }
    }

    fn collect_chunk_block_entities(
        chunk_context: &VoidChunkContext,
        schematic_context: &WorldContext,
        block_entity_lookup: &BlockEntityTypeLookup,
        protocol_version: ProtocolVersion,
    ) -> (Vec<Value>, Vec<ChunkBlockEntity>) {
        let mut block_entities = Vec::new();
        let mut v1_18_block_entities = Vec::new();

        // Get pre-computed block entities for this chunk
        let Some(entities) = schematic_context
            .world
            .get_chunk_block_entities(chunk_context.chunk_x, chunk_context.chunk_z)
        else {
            return (block_entities, v1_18_block_entities);
        };

        // Iterate through all block entities
        for entity_data in entities {
            let Some(protocol_id) =
                block_entity_lookup.get_type_id(&entity_data.get_block_entity_type().to_string())
            else {
                continue;
            };

            let nbt = entity_data.to_nbt(protocol_version);

            let coordinates = entity_data.get_position() + schematic_context.paste_origin;

            if protocol_version.is_after_inclusive(ProtocolVersion::V1_18) {
                v1_18_block_entities.push(ChunkBlockEntity::new(
                    coordinates.x(),
                    coordinates.y(),
                    coordinates.z(),
                    VarInt::new(protocol_id),
                    nbt,
                ));
            } else {
                #[derive(Serialize)]
                struct ChunkBlockEntity {
                    id: String,
                    x: i32,
                    y: i32,
                    z: i32,
                    #[serde(flatten)]
                    data: Value,
                }

                let nbt_fields = ChunkBlockEntity {
                    id: entity_data.block_entity_type.to_string(),
                    x: coordinates.x(),
                    y: coordinates.y(),
                    z: coordinates.z(),
                    data: nbt,
                };

                block_entities.push(
                    pico_nbt::to_value(nbt_fields)
                        .expect("Failed to convert block entity to nbt value"),
                );
            }
        }

        (block_entities, v1_18_block_entities)
    }

    fn encode_pre_v1_16(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        if protocol_version.is_after_inclusive(ProtocolVersion::V1_14) {
            height_maps(protocol_version).encode(writer, protocol_version)?;
        }

        if protocol_version.is_after_inclusive(ProtocolVersion::V1_15) {
            self.biomes.encode(writer, protocol_version)?;
        }

        let mut payload_writer = BinaryWriter::default();
        if protocol_version == ProtocolVersion::V1_8 {
            encode_v1_8_sections(self.data.inner(), &mut payload_writer)?;
        } else {
            for section in self.data.inner() {
                encode_pre_v1_16_section(section, &mut payload_writer, protocol_version)?;
            }
        }

        if protocol_version.between_inclusive(ProtocolVersion::V1_13, ProtocolVersion::V1_14_4) {
            // 1.13–1.14: 256 biome integers (one per column, z * 16 | x), appended
            // to the chunk data. Before 1.13 these were single bytes; 1.15 moved to
            // a 1024-int 3D array encoded ahead of the section data instead.
            for biome_id in self.biomes.iter().take(256) {
                biome_id.encode(&mut payload_writer, protocol_version)?;
            }
        } else if protocol_version.is_before_inclusive(ProtocolVersion::V1_12_2) {
            let biome_id = self.biomes.first().copied().unwrap_or(1).clamp(0, 255) as u8;
            payload_writer.write_bytes(&vec![biome_id; 256])?;
        }

        if protocol_version.is_before_inclusive(ProtocolVersion::V1_7_6) {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(payload_writer.as_slice())?;
            let compressed = encoder.finish()?;
            i32::try_from(compressed.len())?.encode(writer, protocol_version)?;
            writer.write_bytes(&compressed)?;
        } else {
            VarInt::new(i32::try_from(payload_writer.len())?).encode(writer, protocol_version)?;
            writer.write_bytes(payload_writer.as_slice())?;
        }

        if protocol_version.is_after_inclusive(ProtocolVersion::V1_9_3) {
            self.block_entities.encode(writer, protocol_version)?;
        }

        Ok(())
    }
}

fn encode_pre_v1_16_section(
    section: &ChunkSection,
    writer: &mut BinaryWriter,
    protocol_version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    if protocol_version == ProtocolVersion::V1_8 {
        encode_v1_8_sections(std::slice::from_ref(section), writer)?;
        return Ok(());
    }

    if protocol_version.is_before_inclusive(ProtocolVersion::V1_7_6) {
        encode_pre_flattening_section(section, writer)?;
        return Ok(());
    }

    if protocol_version.is_before_inclusive(ProtocolVersion::V1_12_2) {
        encode_v1_9_to_v1_12_section(section, writer, protocol_version)?;
        return Ok(());
    }

    if protocol_version.is_after_inclusive(ProtocolVersion::V1_14) {
        section.block_count.encode(writer, protocol_version)?;
        encode_flattened_section_blocks(section, writer, protocol_version)?;
        return Ok(());
    }

    encode_flattened_section_blocks(section, writer, protocol_version)?;
    writer.write_bytes(&vec![0; 2048])?;
    writer.write_bytes(&vec![0xFF; 2048])?;
    Ok(())
}

fn encode_flattened_section_blocks(
    section: &ChunkSection,
    writer: &mut BinaryWriter,
    protocol_version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    let ids = section_block_ids(section);
    let mut palette = Vec::<u32>::new();
    let mut palette_indices = Vec::with_capacity(ids.len());

    for id in ids {
        let index = palette
            .iter()
            .position(|entry| *entry == id)
            .unwrap_or_else(|| {
                palette.push(id);
                palette.len() - 1
            });
        palette_indices.push(index as u32);
    }

    let bits_per_block = bits_needed(palette.len()).clamp(4, 8) as u8;
    bits_per_block.encode(writer, protocol_version)?;
    VarInt::new(i32::try_from(palette.len())?).encode(writer, protocol_version)?;
    for id in palette {
        VarInt::new(i32::try_from(id)?).encode(writer, protocol_version)?;
    }

    let data = pack_compact(palette_indices.into_iter(), bits_per_block);
    VarInt::new(i32::try_from(data.len())?).encode(writer, protocol_version)?;
    for word in data {
        word.encode(writer, protocol_version)?;
    }

    Ok(())
}

fn encode_v1_8_sections(
    sections: &[ChunkSection],
    writer: &mut BinaryWriter,
) -> Result<(), BinaryWriterError> {
    if sections
        .iter()
        .all(|section| is_single_block_section(section, 0))
    {
        const EMPTY_BLOCK_STATES: [u8; 8192] = [0; 8192];
        const NO_BLOCK_LIGHT: [u8; 2048] = [0; 2048];
        const FULL_SKY_LIGHT: [u8; 2048] = [0xFF; 2048];

        for _ in sections {
            writer.write_bytes(&EMPTY_BLOCK_STATES)?;
        }
        for _ in sections {
            writer.write_bytes(&NO_BLOCK_LIGHT)?;
        }
        for _ in sections {
            writer.write_bytes(&FULL_SKY_LIGHT)?;
        }
        return Ok(());
    }

    let mut block_lights = Vec::with_capacity(sections.len());
    let mut sky_lights = Vec::with_capacity(sections.len());

    for section in sections {
        for state in legacy_block_states(section) {
            write_v1_8_block_state(writer, state)?;
        }
        block_lights.push(vec![0x00; 2048]);
        sky_lights.push(vec![0xFF; 2048]);
    }

    for light in block_lights {
        writer.write_bytes(&light)?;
    }
    for light in sky_lights {
        writer.write_bytes(&light)?;
    }

    Ok(())
}

fn is_single_block_section(section: &ChunkSection, expected_id: i32) -> bool {
    matches!(
        &section.block_states,
        PaletteContainer::SingleValued { value, .. } if value.inner() == expected_id
    )
}

fn write_v1_8_block_state(writer: &mut BinaryWriter, state: u16) -> Result<(), BinaryWriterError> {
    writer.write_bytes(&state.to_le_bytes())?;
    Ok(())
}

fn encode_pre_flattening_section(
    section: &ChunkSection,
    writer: &mut BinaryWriter,
) -> Result<(), BinaryWriterError> {
    let mut block_ids = Vec::with_capacity(4096);
    let mut metas = Vec::with_capacity(4096);
    for state in legacy_block_states(section) {
        block_ids.push((state >> 4) as u8);
        metas.push((state & 0xF) as u8);
    }

    writer.write_bytes(&block_ids)?;
    writer.write_bytes(&pack_nibbles(&metas))?;
    writer.write_bytes(&vec![0x00; 2048])?;
    writer.write_bytes(&vec![0xFF; 2048])?;
    Ok(())
}

fn encode_v1_9_to_v1_12_section(
    section: &ChunkSection,
    writer: &mut BinaryWriter,
    protocol_version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    // The section already holds this version's native pre-Flattening ids
    // (block_id << 4 | metadata), so they can be paletted directly.
    let legacy_ids: Vec<u32> = section_block_ids(section);

    let mut palette = Vec::<u32>::new();
    let mut palette_indices = Vec::with_capacity(legacy_ids.len());
    for &id in &legacy_ids {
        let index = palette
            .iter()
            .position(|&entry| entry == id)
            .unwrap_or_else(|| {
                palette.push(id);
                palette.len() - 1
            });
        palette_indices.push(index as u32);
    }

    let bits_per_block = bits_needed(palette.len()).clamp(4, 8) as u8;
    bits_per_block.encode(writer, protocol_version)?;
    VarInt::new(i32::try_from(palette.len())?).encode(writer, protocol_version)?;
    for id in &palette {
        VarInt::new(i32::try_from(*id)?).encode(writer, protocol_version)?;
    }

    let data = pack_compact(palette_indices.into_iter(), bits_per_block);
    VarInt::new(i32::try_from(data.len())?).encode(writer, protocol_version)?;
    for word in data {
        word.encode(writer, protocol_version)?;
    }

    writer.write_bytes(&vec![0x00; 2048])?;
    writer.write_bytes(&vec![0xFF; 2048])?;

    Ok(())
}

/// The section's per-version native block ids as `u16`. For pre-Flattening
/// targets these are already `block_id << 4 | metadata`, so no remap is needed.
fn legacy_block_states(section: &ChunkSection) -> Vec<u16> {
    section_block_ids(section)
        .into_iter()
        .map(|sid| sid as u16)
        .collect()
}

fn pack_nibbles(values: &[u8]) -> Vec<u8> {
    values
        .chunks(2)
        .map(|pair| (pair[0] & 0xF) | (pair.get(1).copied().unwrap_or(0) << 4))
        .collect()
}

fn section_block_ids(section: &ChunkSection) -> Vec<u32> {
    match &section.block_states {
        PaletteContainer::SingleValued { value, .. } => vec![value.inner() as u32; 4096],
        PaletteContainer::Indirect {
            bits_per_entry,
            palette,
            data,
        } => {
            let indices = unpack_padded(data.inner(), *bits_per_entry, 4096);
            indices
                .into_iter()
                .map(|index| {
                    palette
                        .inner()
                        .get(index as usize)
                        .map_or(0, |value| value.inner() as u32)
                })
                .collect()
        }
        PaletteContainer::Direct {
            bits_per_entry,
            data,
        } => unpack_padded(data.inner(), *bits_per_entry, 4096),
    }
}

fn unpack_padded(data: &[u64], bits_per_entry: u8, entry_count: usize) -> Vec<u32> {
    let entries_per_long = 64 / usize::from(bits_per_entry);
    let mask = (1u64 << bits_per_entry) - 1;
    let mut values = Vec::with_capacity(entry_count);

    for word in data {
        for index in 0..entries_per_long {
            if values.len() == entry_count {
                return values;
            }
            values.push(((word >> (index * usize::from(bits_per_entry))) & mask) as u32);
        }
    }

    values.resize(entry_count, 0);
    values
}

fn pack_compact(entries: impl Iterator<Item = u32>, bits_per_entry: u8) -> Vec<u64> {
    let mask = (1u64 << bits_per_entry) - 1;
    let mut data = vec![0u64; (4096 * usize::from(bits_per_entry)).div_ceil(64)];

    for (index, entry) in entries.enumerate() {
        let bit_index = index * usize::from(bits_per_entry);
        let start_long = bit_index / 64;
        let start_offset = bit_index % 64;
        data[start_long] |= ((u64::from(entry)) & mask) << start_offset;

        let end_offset = start_offset + usize::from(bits_per_entry);
        if end_offset > 64 {
            data[start_long + 1] |= ((u64::from(entry)) & mask) >> (64 - start_offset);
        }
    }

    data
}

fn bits_needed(n: usize) -> usize {
    if n <= 1 {
        1
    } else {
        usize::BITS as usize - (n - 1).leading_zeros() as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_14_and_v1_15_sections_include_block_count_without_inline_light() {
        for protocol_version in [ProtocolVersion::V1_14_4, ProtocolVersion::V1_15_2] {
            let section = ChunkSection::void(1);
            let mut writer = BinaryWriter::default();

            encode_pre_v1_16_section(&section, &mut writer, protocol_version).unwrap();

            let bytes = writer.into_inner();
            assert_eq!(
                bytes.len(),
                2055,
                "unexpected section size for {protocol_version:?}"
            );
            assert_eq!(
                &[0x00, 0x00, 0x04, 0x01, 0x00, 0x80, 0x02],
                &bytes[..7],
                "1.14+ sections must start with non-air count before palette data"
            );
            assert!(
                bytes[7..].iter().all(|byte| *byte == 0),
                "1.14+ chunk sections should not contain inline block/sky light data"
            );
        }
    }

    #[test]
    fn v1_14_full_chunk_payload_encodes_biomes_as_ints() {
        let chunk_data = ChunkData::void(VoidChunkContext {
            chunk_x: 0,
            chunk_z: 0,
            biome_index: 1,
            dimension_height: 256,
            dimension_min_y: 0,
        });
        let mut writer = BinaryWriter::default();

        chunk_data
            .encode_pre_v1_16(&mut writer, ProtocolVersion::V1_14_4)
            .unwrap();

        let bytes = writer.into_inner();
        let expected_payload_len = (16 * 2055) + (256 * 4);
        assert_eq!(&bytes[0..5], &[0x0A, 0x00, 0x00, 0x0C, 0x00]);
        let payload_len_offset = bytes
            .windows(3)
            .position(|window| window == [0xF0, 0x88, 0x02])
            .expect("missing 1.14 chunk payload length");
        let payload_offset = payload_len_offset + 3;
        assert_eq!(bytes.len(), payload_offset + expected_payload_len + 1);
        assert_eq!(
            &bytes[payload_offset + (16 * 2055)..payload_offset + (16 * 2055) + 4],
            &[0, 0, 0, 1]
        );
    }

    #[test]
    fn v1_9_to_v1_13_void_section_uses_palette_format_with_inline_light() {
        // 1.9–1.13: palette format with inline block and sky light, no block count.
        // Total: bpe(1) + palette_len(1) + air_entry(1) + data_len(2) +
        //        256 longs(2048) + block_light(2048) + sky_light(2048) = 6149 bytes.
        for version in [ProtocolVersion::V1_12_2, ProtocolVersion::V1_13_2] {
            let section = ChunkSection::void(1);
            let mut writer = BinaryWriter::default();
            encode_pre_v1_16_section(&section, &mut writer, version).unwrap();
            let bytes = writer.into_inner();

            assert_eq!(bytes.len(), 6149, "wrong section size for {version:?}");
            // bpe=4, palette=[1, 0 (air)], data_len=256
            assert_eq!(
                &bytes[..5],
                &[0x04, 0x01, 0x00, 0x80, 0x02],
                "palette header for {version:?}"
            );
            assert!(
                bytes[5..2053].iter().all(|&b| b == 0),
                "block data must be air for {version:?}"
            );
            assert!(
                bytes[2053..4101].iter().all(|&b| b == 0),
                "block light must be dark for {version:?}"
            );
            assert!(
                bytes[4101..].iter().all(|&b| b == 0xFF),
                "sky light must be bright for {version:?}"
            );
        }
    }

    #[test]
    fn v1_8_void_section_encodes_to_packed_state_format() {
        let section = ChunkSection::void(1);
        let mut writer = BinaryWriter::default();

        encode_pre_v1_16_section(&section, &mut writer, ProtocolVersion::V1_8).unwrap();

        let bytes = writer.into_inner();
        assert_eq!(
            bytes.len(),
            12288,
            "1.8 section data must use packed state entries plus block and sky light"
        );
        assert!(
            bytes[..8192].iter().all(|&b| b == 0),
            "void section: all packed block states must be air"
        );
        assert!(
            bytes[8192..10240].iter().all(|&b| b == 0),
            "void section: block light must be dark"
        );
        assert!(
            bytes[10240..].iter().all(|&b| b == 0xFF),
            "void section: sky light must be fully bright"
        );
    }

    #[test]
    fn v1_8_block_states_are_little_endian() {
        let mut writer = BinaryWriter::default();

        write_v1_8_block_state(&mut writer, 0x0123).unwrap();

        assert_eq!(writer.into_inner(), [0x23, 0x01]);
    }

    #[test]
    fn pre_v1_8_void_section_keeps_separate_id_and_metadata_arrays() {
        let section = ChunkSection::void(1);
        let mut writer = BinaryWriter::default();

        encode_pre_v1_16_section(&section, &mut writer, ProtocolVersion::V1_7_6).unwrap();

        let bytes = writer.into_inner();
        assert_eq!(
            bytes.len(),
            10240,
            "1.7 section data must use separate ID and metadata arrays"
        );
        assert!(
            bytes[..4096].iter().all(|&b| b == 0),
            "void section: all block IDs must be air"
        );
        assert!(
            bytes[4096..6144].iter().all(|&b| b == 0),
            "void section: all metadata must be zero"
        );
        assert!(
            bytes[6144..8192].iter().all(|&b| b == 0),
            "void section: block light must be dark"
        );
        assert!(
            bytes[8192..].iter().all(|&b| b == 0xFF),
            "void section: sky light must be fully bright"
        );
    }

    #[test]
    fn v1_8_full_chunk_payload_uses_packed_state_arrays_then_light() {
        let chunk_data = ChunkData::void(VoidChunkContext {
            chunk_x: 0,
            chunk_z: 0,
            biome_index: 1,
            dimension_height: 256,
            dimension_min_y: 0,
        });
        let mut writer = BinaryWriter::default();

        chunk_data
            .encode_pre_v1_16(&mut writer, ProtocolVersion::V1_8)
            .unwrap();

        let bytes = writer.into_inner();
        let payload_len = (16 * 8192) + (16 * 2048) + (16 * 2048) + 256;
        assert_eq!(&bytes[0..3], &[0x80, 0x82, 0x0C]);
        assert_eq!(bytes.len(), 3 + payload_len);

        let payload = &bytes[3..];
        assert!(
            payload[..16 * 8192].iter().all(|&byte| byte == 0),
            "void 1.8 block state arrays must be air"
        );
        assert!(
            payload[16 * 8192..(16 * 8192) + (16 * 2048)]
                .iter()
                .all(|&byte| byte == 0),
            "void 1.8 block light arrays must be dark"
        );
        assert!(
            payload[(16 * 8192) + (16 * 2048)..(16 * 8192) + (32 * 2048)]
                .iter()
                .all(|&byte| byte == 0xFF),
            "void 1.8 sky light arrays must be fully bright"
        );
        assert_eq!(payload[payload_len - 256], 1);
    }

    #[test]
    fn pack_nibbles_pairs_low_then_high() {
        let values = vec![0x3u8, 0xAu8, 0x1u8, 0x5u8];
        let nibbles = pack_nibbles(&values);
        assert_eq!(nibbles.len(), 2);
        assert_eq!(nibbles[0], 0x3 | (0xA << 4));
        assert_eq!(nibbles[1], 0x1 | (0x5 << 4));
    }

    #[test]
    fn motion_blocking_heightmap_length_matches_packing_format() {
        // Pre-1.16 clients pack the heightmap as a spanning bit stream (36 longs);
        // 1.16+ clients pad each long (37 longs). A length mismatch makes the
        // client BitStorage copy out of bounds.
        for (version, expected_longs) in [
            (ProtocolVersion::V1_14_4, 36usize),
            (ProtocolVersion::V1_15_2, 36),
            (ProtocolVersion::V1_16, 37),
        ] {
            match height_maps(version) {
                Value::Compound(compound) => match compound.get("MOTION_BLOCKING") {
                    Some(Value::LongArray(data)) => assert_eq!(
                        data.len(),
                        expected_longs,
                        "wrong MOTION_BLOCKING long count for {version:?}"
                    ),
                    other => panic!("MOTION_BLOCKING must be a long array, got {other:?}"),
                },
                other => panic!("heightmaps must be a compound, got {other:?}"),
            }
        }
    }

    #[test]
    fn v1_13_full_chunk_payload_encodes_biomes_as_ints() {
        // 1.13 changed the trailing biome array from 256 bytes to 256 ints. Sending
        // bytes leaves the client reading past the end of the chunk data buffer.
        let chunk_data = ChunkData::void(VoidChunkContext {
            chunk_x: 0,
            chunk_z: 0,
            biome_index: 1,
            dimension_height: 256,
            dimension_min_y: 0,
        });
        let mut writer = BinaryWriter::default();

        chunk_data
            .encode_pre_v1_16(&mut writer, ProtocolVersion::V1_13_2)
            .unwrap();

        let bytes = writer.into_inner();
        let expected_payload_len = (16 * 6149) + (256 * 4);
        assert_eq!(&bytes[0..3], &[0xD0, 0x88, 0x06]);
        assert_eq!(bytes.len(), 3 + expected_payload_len + 1);
        assert_eq!(
            &bytes[3 + (16 * 6149)..3 + (16 * 6149) + 4],
            &[0, 0, 0, 1],
            "first biome must be encoded as a big-endian int"
        );
    }
}

#[derive(PacketOut)]
struct HeightMap {
    /// 1: WORLD_SURFACE
    /// All blocks other than air, cave air and void air. To determine if a beacon beam is obstructed.
    /// 4: MOTION_BLOCKING
    /// "Solid" blocks, except bamboo saplings and cacti; fluids. To determine where to display rain and snow.
    /// 5: MOTION_BLOCKING_NO_LEAVES
    /// Same as MOTION_BLOCKING, excluding leaf blocks.
    height_map_type: VarInt,
    data: LengthPaddedVec<i64>,
}

#[derive(PacketOut)]
pub struct ChunkBlockEntity {
    /// Packed XZ coordinates within the chunk section (X: 4 bits, Z: 4 bits)
    /// Calculated as: ((x & 15) << 4) | (z & 15)
    packed_xz: u8,
    /// Y coordinate within the chunk section (0-15 for normal sections)
    y: i16,
    /// Type of block entity (VarInt registry ID)
    block_entity_type: VarInt,
    /// NBT data for the block entity
    data: Value,
}

impl ChunkBlockEntity {
    /// Creates a new BlockEntity from world coordinates and NBT data
    pub fn new(
        world_x: i32,
        world_y: i32,
        world_z: i32,
        block_entity_type: VarInt,
        data: Value,
    ) -> Self {
        // Pack X and Z coordinates (each only needs 4 bits since chunk is 16x16)
        let chunk_x = (world_x & 15) as u8;
        let chunk_z = (world_z & 15) as u8;
        let packed_xz = (chunk_x << 4) | chunk_z;

        Self {
            packed_xz,
            y: world_y as i16,
            block_entity_type,
            data,
        }
    }
}
