use crate::data::registry_entry::RegistryEntry;
use crate::registry_provider::Dimension;
use crate::{RegistryKeys, RegistryManager};
use pico_identifier::Identifier;
use pico_nbt::NbtOptions;
use pico_nbt::Value;
use protocol_version::protocol_version::ProtocolVersion;
use serde::Serialize;
use std::borrow::Cow;
use std::path::Path;

pub fn load_registry_manager(
    base_path: &Path,
    protocol_version: ProtocolVersion,
    registries: &[RegistryKeys],
) -> crate::Result<RegistryManager> {
    crate::Error::incompatible_version(
        protocol_version,
        ProtocolVersion::V1_16,
        ProtocolVersion::latest(),
    )?;

    let resource_root = base_path.join(protocol_version.data().to_string());

    RegistryManager::builder()
        .register_all(registries)
        .load_from_resource_path(&resource_root)
}

pub fn get_registry_keys(protocol_version: ProtocolVersion) -> crate::Result<Vec<RegistryKeys>> {
    crate::Error::incompatible_version(
        protocol_version,
        ProtocolVersion::V1_16,
        ProtocolVersion::latest(),
    )?;
    Ok(RegistryKeys::ALL_REGISTRIES
        .iter()
        .filter(|key| {
            key.is_mandatory()
                && key.get_minimum_version().is_some_and(|minimum_version| {
                    protocol_version.is_after_inclusive(minimum_version)
                })
        })
        .cloned()
        .collect())
}

pub fn get_dimension(
    registry_manager: &RegistryManager,
    dimension_identifier: Dimension,
) -> crate::Result<&RegistryEntry> {
    registry_manager
        .get(&RegistryKeys::DimensionType)?
        .try_get(&dimension_identifier.identifier())
        .ok_or(crate::Error::UnknownRegistryEntry)
}

pub fn encode_nameless_compound_to_bytes<T: Serialize>(
    protocol_version: ProtocolVersion,
    value: &T,
) -> pico_nbt::Result<Cow<'static, [u8]>> {
    let is_nameless = protocol_version.is_after_inclusive(ProtocolVersion::V1_20_2);
    let options = NbtOptions::new().nameless_root(is_nameless);
    let name = if is_nameless { None } else { Some("") };
    let mut bytes = Vec::new();
    pico_nbt::to_writer_with_options(&mut bytes, &value, name, options)?;
    Ok(Cow::Owned(bytes))
}

#[must_use]
pub fn registry_entry_value_for_protocol(
    protocol_version: ProtocolVersion,
    registry_id: &Identifier,
    entry_id: &Identifier,
    raw_value: &Value,
) -> Value {
    let mut value = raw_value.clone();

    if registry_id.namespace == "minecraft" {
        match registry_id.thing.as_str() {
            "dimension_type" => normalize_dimension_type(&mut value),
            "trim_material" => {
                normalize_trim_material(protocol_version, &entry_id.thing, &mut value);
            }
            _ => {}
        }
    }

    value
}

fn normalize_dimension_type(value: &mut Value) {
    let Value::Compound(fields) = value else {
        return;
    };

    // Mojang's JSON reports do not retain NBT numeric types. The generic JSON
    // converter consequently stores small integral values as TAG_Byte/TAG_Short,
    // but dimension codecs require these fields to be TAG_Int. Consumers such as
    // Geyser use NbtMap#getInt and otherwise see a zero-height dimension.
    for field in [
        "height",
        "logical_height",
        "min_y",
        "monster_spawn_block_light_limit",
    ] {
        normalize_i32_field(fields, field);
    }

    if let Some(Value::Compound(light_level)) = fields.get_mut("monster_spawn_light_level") {
        normalize_i32_field(light_level, "min_inclusive");
        normalize_i32_field(light_level, "max_inclusive");
    }

    if let Some(coordinate_scale @ Value::Float(_)) = fields.get_mut("coordinate_scale")
        && let Value::Float(number) = *coordinate_scale
    {
        *coordinate_scale = Value::Double(f64::from(number));
    }
}

fn normalize_i32_field(fields: &mut pico_nbt::IndexMap<String, Value>, field: &str) {
    let Some(value) = fields.get_mut(field) else {
        return;
    };

    let normalized = match value {
        Value::Byte(number) => Some(i32::from(*number)),
        Value::Short(number) => Some(i32::from(*number)),
        Value::Long(number) => i32::try_from(*number).ok(),
        _ => None,
    };
    if let Some(number) = normalized {
        *value = Value::Int(number);
    }
}

fn normalize_trim_material(
    protocol_version: ProtocolVersion,
    trim_material: &str,
    value: &mut Value,
) {
    let Value::Compound(fields) = value else {
        return;
    };

    if protocol_version.is_before_inclusive(ProtocolVersion::V1_21_4)
        && let Some(ingredient) = trim_material_ingredient(trim_material)
    {
        fields
            .entry("ingredient".to_string())
            .or_insert_with(|| Value::String(ingredient.to_string()));
    }

    if protocol_version.is_before_inclusive(ProtocolVersion::V1_21_2) {
        if let Some(item_model_index) = trim_material_item_model_index(trim_material) {
            fields
                .entry("item_model_index".to_string())
                .or_insert(Value::Float(item_model_index));
        }

        if let Some(mut override_armor_assets) = fields.shift_remove("override_armor_assets") {
            remove_unsupported_armor_material_overrides(
                protocol_version,
                &mut override_armor_assets,
            );

            if !is_empty_compound(&override_armor_assets) {
                fields
                    .entry("override_armor_materials".to_string())
                    .or_insert(override_armor_assets);
            }
        }
    }
}

fn remove_unsupported_armor_material_overrides(
    protocol_version: ProtocolVersion,
    value: &mut Value,
) {
    if protocol_version.is_after_inclusive(ProtocolVersion::V1_21_4) {
        return;
    }

    let Value::Compound(fields) = value else {
        return;
    };

    fields.shift_remove("minecraft:copper");
}

fn is_empty_compound(value: &Value) -> bool {
    matches!(value, Value::Compound(fields) if fields.is_empty())
}

fn trim_material_ingredient(trim_material: &str) -> Option<&'static str> {
    match trim_material {
        "amethyst" => Some("minecraft:amethyst_shard"),
        "copper" => Some("minecraft:copper_ingot"),
        "diamond" => Some("minecraft:diamond"),
        "emerald" => Some("minecraft:emerald"),
        "gold" => Some("minecraft:gold_ingot"),
        "iron" => Some("minecraft:iron_ingot"),
        "lapis" => Some("minecraft:lapis_lazuli"),
        "netherite" => Some("minecraft:netherite_ingot"),
        "quartz" => Some("minecraft:quartz"),
        "redstone" => Some("minecraft:redstone"),
        "resin" => Some("minecraft:resin_brick"),
        _ => None,
    }
}

fn trim_material_item_model_index(trim_material: &str) -> Option<f32> {
    match trim_material {
        "quartz" => Some(0.1),
        "iron" => Some(0.2),
        "netherite" => Some(0.3),
        "redstone" => Some(0.4),
        "copper" => Some(0.5),
        "gold" => Some(0.6),
        "emerald" => Some(0.7),
        "diamond" => Some(0.8),
        "lapis" => Some(0.9),
        "amethyst" => Some(1.0),
        _ => None,
    }
}
