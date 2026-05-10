use crate::RegistryManager;
use crate::registry_provider::shared::{encode_nameless_compound_to_bytes, get_registry_keys};
use pico_nbt::{IndexMap, Value};
use protocol_version::protocol_version::ProtocolVersion;
use serde::Serialize;
use std::borrow::Cow;

#[derive(Serialize)]
struct RegistryCodec {
    #[serde(rename = "type")]
    registry_type: String,
    value: Vec<RegistryCodecEntry>,
}

#[derive(Serialize)]
struct RegistryCodecEntry {
    name: String,
    id: i32,
    element: Value,
}

pub fn get_registry_codec_bytes_v1_16_2(
    registry_manager: &RegistryManager,
    protocol_version: ProtocolVersion,
) -> crate::Result<Cow<'static, [u8]>> {
    crate::Error::incompatible_version(
        protocol_version,
        ProtocolVersion::V1_16_2,
        ProtocolVersion::V1_20_3,
    )?;
    let registries = get_registry_keys(protocol_version)?;

    let registries = registries
        .iter()
        .filter_map(|registry_keys| registry_manager.try_get(registry_keys));

    let mut final_registries = IndexMap::new();
    for registry in registries {
        let registry_type = registry.get_registry_key().get_value().to_string();
        final_registries.insert(
            registry_type.clone(),
            RegistryCodec {
                registry_type,
                value: registry
                    .get_entries()
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| -> crate::Result<RegistryCodecEntry> {
                        Ok(RegistryCodecEntry {
                            name: entry.get_registry_key().get_value().to_string(),
                            id: i32::try_from(index)
                                .map_err(|_| crate::Error::UnknownRegistryEntry)?,
                            element: entry
                                .get_raw_value()
                                .cloned()
                                .ok_or(crate::Error::UnknownRegistryEntry)?,
                        })
                    })
                    .collect::<crate::Result<_>>()?,
            },
        );
    }

    Ok(encode_nameless_compound_to_bytes(
        protocol_version,
        &final_registries,
    )?)
}

#[cfg(test)]
mod tests {
    use crate::registry_provider::RegistryProvider;
    use crate::registry_provider::RuntimeRegistryProvider;
    use pico_nbt::{NbtOptions, Value};
    use protocol_version::protocol_version::ProtocolVersion;
    use std::error::Error;
    use std::io;
    use std::path::PathBuf;

    fn test_error(message: &'static str) -> io::Error {
        io::Error::other(message)
    }

    fn generated_data_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/generated")
    }

    fn codec_root(protocol_version: ProtocolVersion) -> Result<Value, Box<dyn Error>> {
        let provider = RuntimeRegistryProvider::new(&generated_data_path(), protocol_version)?;
        let bytes = provider.get_registry_codec_v1_16()?;
        let (_, root) = pico_nbt::from_slice_with_options(
            &bytes,
            NbtOptions::new()
                .nameless_root(protocol_version.is_after_inclusive(ProtocolVersion::V1_20_2)),
        )?;
        Ok(root)
    }

    fn contains_trim_material_redstone(root: &Value) -> Result<bool, Box<dyn Error>> {
        let root = root
            .get_compound()
            .ok_or_else(|| test_error("registry codec root should be a compound"))?;
        let trim_material = root
            .get("minecraft:trim_material")
            .and_then(Value::get_compound)
            .ok_or_else(|| test_error("registry codec should include trim materials"))?;
        let entries = trim_material
            .get("value")
            .and_then(Value::get_list)
            .ok_or_else(|| test_error("trim material registry should have entries"))?;

        Ok(entries.iter().any(|entry| {
            entry
                .get_compound()
                .and_then(|entry| entry.get("name"))
                .and_then(Value::get_str)
                == Some("minecraft:redstone")
        }))
    }

    #[test]
    fn registry_codec_includes_trim_materials_for_affected_versions() -> Result<(), Box<dyn Error>>
    {
        for protocol_version in [
            ProtocolVersion::V1_19_4,
            ProtocolVersion::V1_20,
            ProtocolVersion::V1_20_2,
            ProtocolVersion::V1_20_3,
        ] {
            let root = codec_root(protocol_version)?;
            if !contains_trim_material_redstone(&root)? {
                return Err(io::Error::other(format!(
                    "registry codec should include minecraft:trim_material/minecraft:redstone for {}",
                    protocol_version.humanize(),
                ))
                .into());
            }
        }

        Ok(())
    }
}
