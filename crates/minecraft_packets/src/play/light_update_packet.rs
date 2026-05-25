use minecraft_protocol::prelude::*;
use pico_structures::prelude::LightSection;

use crate::play::data::light_data::Light;

#[derive(PacketOut)]
pub struct LightUpdatePacket {
    chunk_x: VarInt,
    chunk_z: VarInt,

    #[pvn(735..757)]
    trust_edges: bool,

    #[pvn(477..757)]
    legacy_light_data: LegacyLightData,
}

impl LightUpdatePacket {
    pub fn new_void(chunk_x: i32, chunk_z: i32, dimension_height: i32) -> Self {
        Self {
            chunk_x: VarInt::new(chunk_x),
            chunk_z: VarInt::new(chunk_z),
            trust_edges: true,
            legacy_light_data: LegacyLightData::new_void(dimension_height),
        }
    }

    pub fn from_light_data(
        chunk_x: i32,
        chunk_z: i32,
        sky_light_sections: &[LightSection],
        block_light_sections: &[LightSection],
        dimension_height: i32,
    ) -> Self {
        Self {
            chunk_x: VarInt::new(chunk_x),
            chunk_z: VarInt::new(chunk_z),
            trust_edges: true,
            legacy_light_data: LegacyLightData::from_light_data(
                sky_light_sections,
                block_light_sections,
                dimension_height,
            ),
        }
    }
}

struct LegacyLightData {
    sky_light_mask: VarInt,
    block_light_mask: VarInt,
    empty_sky_light_mask: VarInt,
    empty_block_light_mask: VarInt,
    sky_light_arrays: LengthPaddedVec<Light>,
    block_light_arrays: LengthPaddedVec<Light>,
}

impl EncodePacket for LegacyLightData {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        if protocol_version.between_inclusive(ProtocolVersion::V1_17, ProtocolVersion::V1_17_1) {
            // 1.17/1.17.1: masks changed to BitSet, arrays gained a count prefix
            let to_bitset = |varint: &VarInt| -> BitSet {
                let val = varint.inner();
                if val == 0 {
                    BitSet::default()
                } else {
                    BitSet::new(vec![val as i64])
                }
            };
            to_bitset(&self.sky_light_mask).encode(writer, protocol_version)?;
            to_bitset(&self.block_light_mask).encode(writer, protocol_version)?;
            to_bitset(&self.empty_sky_light_mask).encode(writer, protocol_version)?;
            to_bitset(&self.empty_block_light_mask).encode(writer, protocol_version)?;
            // 1.17 format: VarInt count then arrays
            self.sky_light_arrays.encode(writer, protocol_version)?;
            self.block_light_arrays.encode(writer, protocol_version)?;
        } else {
            // 1.16.x: VarInt masks; arrays have NO count prefix — ViaVersion reads
            // exactly popcount(mask) arrays directly, then adds the count when rewriting
            // to 1.17 format.
            self.sky_light_mask.encode(writer, protocol_version)?;
            self.block_light_mask.encode(writer, protocol_version)?;
            self.empty_sky_light_mask.encode(writer, protocol_version)?;
            self.empty_block_light_mask
                .encode(writer, protocol_version)?;
            for light in self.sky_light_arrays.inner() {
                light.encode(writer, protocol_version)?;
            }
            for light in self.block_light_arrays.inner() {
                light.encode(writer, protocol_version)?;
            }
        }
        Ok(())
    }
}

impl LegacyLightData {
    fn from_light_data(
        sky_light_sections: &[LightSection],
        block_light_sections: &[LightSection],
        dimension_height: i32,
    ) -> Self {
        let total_light_sections = Self::total_light_sections(dimension_height);
        let all_sections_mask = VarInt::new(Self::all_sections_mask(total_light_sections));

        let mut sky_light_arrays = Vec::with_capacity(total_light_sections as usize);
        sky_light_arrays.push(Light::full_sky());
        for section in sky_light_sections {
            sky_light_arrays.push(Light::new(section.clone()));
        }
        sky_light_arrays.push(Light::full_sky());
        while sky_light_arrays.len() < total_light_sections as usize {
            sky_light_arrays.push(Light::full_sky());
        }

        let mut block_light_arrays = Vec::with_capacity(total_light_sections as usize);
        block_light_arrays.push(Light::no_block());
        for section in block_light_sections {
            block_light_arrays.push(Light::new(section.clone()));
        }
        block_light_arrays.push(Light::no_block());
        while block_light_arrays.len() < total_light_sections as usize {
            block_light_arrays.push(Light::no_block());
        }

        Self {
            sky_light_mask: all_sections_mask.clone(),
            block_light_mask: all_sections_mask,
            empty_sky_light_mask: VarInt::default(),
            empty_block_light_mask: VarInt::default(),
            sky_light_arrays: LengthPaddedVec::new(sky_light_arrays),
            block_light_arrays: LengthPaddedVec::new(block_light_arrays),
        }
    }

    fn new_void(dimension_height: i32) -> Self {
        let total_light_sections = Self::total_light_sections(dimension_height);
        let all_sections_mask = VarInt::new(Self::all_sections_mask(total_light_sections));

        Self {
            sky_light_mask: all_sections_mask.clone(),
            block_light_mask: all_sections_mask,
            empty_sky_light_mask: VarInt::default(),
            empty_block_light_mask: VarInt::default(),
            sky_light_arrays: LengthPaddedVec::new(vec![
                Light::full_sky();
                total_light_sections as usize
            ]),
            block_light_arrays: LengthPaddedVec::new(vec![
                Light::no_block();
                total_light_sections as usize
            ]),
        }
    }

    fn total_light_sections(dimension_height: i32) -> i32 {
        dimension_height / 16 + 2
    }

    fn all_sections_mask(total_light_sections: i32) -> i32 {
        (1i32 << total_light_sections) - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_16_light_update_uses_var_int_masks_and_no_array_count() {
        // 1.16.x has no count prefix on light arrays; ViaVersion reads popcount(mask)
        // arrays implicitly, then adds the count prefix when rewriting to 1.17 format.
        let packet = LightUpdatePacket::new_void(3, -2, 256);
        let mut bytes = BinaryWriter::default();

        packet.encode(&mut bytes, ProtocolVersion::V1_16_2).unwrap();

        let bytes = bytes.into_inner();
        // chunk_x=3, chunk_z=-2, trust_edges=true
        assert_eq!(&[0x03, 0xFE, 0xFF, 0xFF, 0xFF, 0x0F, 0x01], &bytes[..7]);
        // sky_light_mask=0x3FFFF, block_light_mask=0x3FFFF, empty_sky=0, empty_block=0
        assert_eq!(
            &[0xFF, 0xFF, 0x0F, 0xFF, 0xFF, 0x0F, 0x00, 0x00],
            &bytes[7..15]
        );
        // No count prefix: 15 header bytes + 2 * (18 arrays * 2050 bytes each)
        assert_eq!(15 + (2 * (18 * 2050)), bytes.len());
    }

    #[test]
    fn v1_14_light_update_has_no_trust_edges() {
        let packet = LightUpdatePacket::new_void(3, -2, 256);
        let mut bytes = BinaryWriter::default();

        packet.encode(&mut bytes, ProtocolVersion::V1_14_4).unwrap();

        let bytes = bytes.into_inner();
        // chunk_x=3, chunk_z=-2, then masks immediately; no trust_edges field.
        assert_eq!(
            &[0x03, 0xFE, 0xFF, 0xFF, 0xFF, 0x0F, 0xFF, 0xFF, 0x0F],
            &bytes[..9]
        );
        assert_eq!(14 + (2 * (18 * 2050)), bytes.len());
    }

    #[test]
    fn v1_17_light_update_uses_bitset_masks() {
        // 1.17 changed light masks from VarInt to BitSet (VarInt length + i64 array).
        // For 256-height: 18 sections, mask = (1<<18)-1 = 0x3FFFF = 262143.
        // BitSet(262143) → VarInt(1) + i64(262143 big-endian).
        let packet = LightUpdatePacket::new_void(0, 0, 256);
        let mut bytes = BinaryWriter::default();

        packet.encode(&mut bytes, ProtocolVersion::V1_17).unwrap();

        let bytes = bytes.into_inner();
        // chunk_x=0, chunk_z=0, trust_edges=true
        assert_eq!(&[0x00, 0x00, 0x01], &bytes[..3]);
        // sky_light_mask as BitSet: VarInt(1)=0x01, then i64(262143)
        assert_eq!(
            &[0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xFF, 0xFF],
            &bytes[3..12]
        );
        // block_light_mask identical
        assert_eq!(
            &[0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xFF, 0xFF],
            &bytes[12..21]
        );
        // empty_sky_light_mask as BitSet: VarInt(0)
        assert_eq!(0x00, bytes[21]);
        // empty_block_light_mask as BitSet: VarInt(0)
        assert_eq!(0x00, bytes[22]);
        // Header: 3 + 9 + 9 + 1 + 1 = 23 bytes; arrays have count prefix in 1.17 format.
        // 2 * (VarInt(18) + 18 * 2050) = 2 * (1 + 36900)
        assert_eq!(23 + (2 * (1 + 18 * 2050)), bytes.len());
    }
}
