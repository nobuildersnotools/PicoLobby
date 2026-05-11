use crate::prelude::EncodePacket;
use pico_binutils::prelude::{BinaryWriter, BinaryWriterError};
use pico_nbt::{NbtOptions, Value};
use protocol_version::protocol_version::ProtocolVersion;

impl EncodePacket for Value {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        let nbt_bytes = self
            .to_byte(
                pico_nbt::CompressionType::None,
                from_protocol_version(protocol_version),
                None,
            )
            .map_err(|_| BinaryWriterError::UnsupportedOperation)?;
        writer.write_bytes(&nbt_bytes)?;
        Ok(())
    }
}

fn from_protocol_version(value: ProtocolVersion) -> NbtOptions {
    NbtOptions::new()
        .nameless_root(value.is_after_inclusive(ProtocolVersion::V1_20_2))
        .dynamic_lists(value.is_after_inclusive(ProtocolVersion::V1_21_5))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pico_nbt::IndexMap;

    #[test]
    fn value_encoder_preserves_long_array_tags() {
        let mut compound = IndexMap::new();
        compound.insert("MOTION_BLOCKING".to_string(), Value::LongArray(vec![0; 37]));
        let value = Value::Compound(compound);

        let mut writer = BinaryWriter::default();
        value
            .encode(&mut writer, ProtocolVersion::V1_15_2)
            .expect("heightmap NBT should encode");

        let bytes = writer.as_slice();
        assert_eq!(bytes[0], 10, "root tag must be a compound");
        assert_eq!(
            bytes[3], 12,
            "heightmap entry must stay TAG_Long_Array, not TAG_List"
        );
    }
}
