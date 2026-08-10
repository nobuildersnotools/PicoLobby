use crate::RegistryManager;
use crate::registry_provider::shared::{get_registry_keys, registry_entry_value_for_protocol};
use pico_identifier::Identifier;
use pico_nbt::{CompressionType, NbtOptions};
use protocol_version::protocol_version::ProtocolVersion;
use std::borrow::Cow;

pub struct RegistryDataEntry {
    pub entry_id: Identifier,
    pub nbt_bytes: Option<Cow<'static, [u8]>>,
}

impl RegistryDataEntry {
    #[must_use]
    pub const fn new(entry_id: Identifier, nbt_bytes: Option<Cow<'static, [u8]>>) -> Self {
        Self {
            entry_id,
            nbt_bytes,
        }
    }
}

pub fn get_registry_data_v1_20_5(
    registry_manager: &RegistryManager,
    protocol_version: ProtocolVersion,
) -> crate::Result<Vec<(Identifier, Vec<RegistryDataEntry>)>> {
    crate::Error::incompatible_version(
        protocol_version,
        ProtocolVersion::V1_20_5,
        ProtocolVersion::latest(),
    )?;
    let registries = get_registry_keys(protocol_version)?;

    Ok(registries
        .iter()
        .filter_map(|registry_keys| registry_manager.try_get(registry_keys))
        .map(|registry| {
            let registry_id = registry.get_registry_key().get_value().clone();
            let registry_entries = registry
                .get_entries()
                .iter()
                .flat_map(|entry| -> crate::Result<RegistryDataEntry> {
                    let entry_id = entry.get_registry_key().get_value().clone();
                    let bytes = entry
                        .get_raw_value()
                        .map(|raw_value| {
                            registry_entry_value_for_protocol(
                                protocol_version,
                                &registry_id,
                                &entry_id,
                                raw_value,
                            )
                            .to_byte(
                                CompressionType::None,
                                NbtOptions::new().nameless_root(true).dynamic_lists(true),
                                None,
                            )
                        })
                        .transpose()?
                        .map(Cow::Owned);
                    Ok(RegistryDataEntry::new(entry_id, bytes))
                })
                .collect();
            (registry_id, registry_entries)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use crate::registry_provider::{RegistryProvider, RuntimeRegistryProvider};
    use pico_nbt::{NbtOptions, Value};
    use protocol_version::protocol_version::ProtocolVersion;
    use std::error::Error;
    use std::io;
    use std::path::PathBuf;

    fn generated_data_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/generated")
    }

    fn test_error(message: &'static str) -> io::Error {
        io::Error::other(message)
    }

    fn trim_material_entry(
        protocol_version: ProtocolVersion,
        entry_id: &str,
    ) -> Result<Value, Box<dyn Error>> {
        registry_entry(protocol_version, "minecraft:trim_material", entry_id)
    }

    fn registry_entry(
        protocol_version: ProtocolVersion,
        registry_id: &str,
        entry_id: &str,
    ) -> Result<Value, Box<dyn Error>> {
        let provider = RuntimeRegistryProvider::new(&generated_data_path(), protocol_version)?;
        let registries = provider.get_registry_data_v1_20_5()?;
        let (_, entries) = registries
            .iter()
            .find(|(candidate, _)| candidate.to_string() == registry_id)
            .ok_or_else(|| test_error("registry should be present"))?;
        let entry = entries
            .iter()
            .find(|entry| entry.entry_id.to_string() == entry_id)
            .ok_or_else(|| test_error("registry entry should be present"))?;
        let bytes = entry
            .nbt_bytes
            .as_ref()
            .ok_or_else(|| test_error("registry entry should have NBT data"))?;
        let (_, value) =
            pico_nbt::from_slice_with_options(bytes, NbtOptions::new().nameless_root(true))?;

        Ok(value)
    }

    #[test]
    fn dimension_type_uses_codec_numeric_types() -> Result<(), Box<dyn Error>> {
        let value = registry_entry(
            ProtocolVersion::V26_2,
            "minecraft:dimension_type",
            "minecraft:overworld",
        )?;
        let fields = value
            .get_compound()
            .ok_or_else(|| test_error("dimension type should be a compound"))?;

        assert_eq!(fields.get("height"), Some(&Value::Int(256)));
        assert_eq!(fields.get("logical_height"), Some(&Value::Int(256)));
        assert_eq!(fields.get("min_y"), Some(&Value::Int(0)));
        assert_eq!(
            fields.get("monster_spawn_block_light_limit"),
            Some(&Value::Int(0))
        );
        assert_eq!(fields.get("coordinate_scale"), Some(&Value::Double(1.0)));

        Ok(())
    }

    #[test]
    fn trim_material_data_v1_21_4_includes_required_ingredient() -> Result<(), Box<dyn Error>> {
        let value = trim_material_entry(ProtocolVersion::V1_21_4, "minecraft:resin")?;
        let fields = value
            .get_compound()
            .ok_or_else(|| test_error("trim material should be a compound"))?;

        assert_eq!(
            fields.get("ingredient").and_then(Value::get_str),
            Some("minecraft:resin_brick")
        );
        assert_eq!(
            fields.get("item_model_index"),
            None,
            "1.21.4 trim material data no longer carries item_model_index",
        );

        Ok(())
    }

    #[test]
    fn trim_material_data_v1_20_5_uses_legacy_model_fields() -> Result<(), Box<dyn Error>> {
        let value = trim_material_entry(ProtocolVersion::V1_20_5, "minecraft:diamond")?;
        let fields = value
            .get_compound()
            .ok_or_else(|| test_error("trim material should be a compound"))?;

        assert_eq!(
            fields.get("ingredient").and_then(Value::get_str),
            Some("minecraft:diamond")
        );
        assert_eq!(
            fields.get("item_model_index").and_then(Value::get_float),
            Some(0.8)
        );
        assert_eq!(
            fields.get("override_armor_assets"),
            None,
            "pre-1.21.4 clients expect override_armor_materials",
        );
        assert_ne!(
            fields.get("override_armor_materials"),
            None,
            "pre-1.21.4 clients need legacy armor material override keys",
        );

        Ok(())
    }

    #[test]
    fn trim_material_data_v1_20_5_removes_unknown_copper_armor_override()
    -> Result<(), Box<dyn Error>> {
        for protocol_version in [ProtocolVersion::V1_20_5, ProtocolVersion::V1_21] {
            let value = trim_material_entry(protocol_version, "minecraft:copper")?;
            let fields = value
                .get_compound()
                .ok_or_else(|| test_error("trim material should be a compound"))?;

            assert_eq!(
                fields.get("ingredient").and_then(Value::get_str),
                Some("minecraft:copper_ingot"),
                "{}",
                protocol_version.humanize()
            );
            assert_eq!(
                fields.get("override_armor_materials"),
                None,
                "{} clients do not know minecraft:copper as an armor material",
                protocol_version.humanize()
            );
        }

        Ok(())
    }
}
