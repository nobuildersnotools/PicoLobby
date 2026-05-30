use minecraft_protocol::prelude::*;
use pico_nbt::{IndexMap, Value};
use pico_text_component::prelude::Component;

/// A Minecraft item stack suitable for sending in a hotbar/container slot.
///
/// Encodes to the correct wire format for all supported protocol versions:
/// - Pre-1.13:  `Short item_id, Byte count, Short damage, OptNBT`
/// - 1.13-1.13.1: `Short item_id, Byte count, OptNBT`
/// - 1.13.2-1.20.4: `Bool present, VarInt item_id, Byte count, OptNBT`
/// - 1.20.5+:  `VarInt count, VarInt item_id, components…`
#[derive(Clone)]
pub struct LobbySlot {
    /// Numeric protocol item ID.  Must be ≥ 0 for a non-empty slot.
    item_id: i32,
    count: u8,
    display_name: Option<Component>,
    lore: Vec<Component>,
    /// Pre-1.13 metadata/damage value distinguishing item variants (e.g.
    /// coloured wool).  Only encoded for clients before 1.13; ignored on the
    /// flattened (1.13+) wire formats, which identify variants by item id.
    legacy_damage: i16,
    /// When `true`, the item is rendered with the enchantment glint regardless
    /// of whether it carries a "real" enchantment. Pre-1.20.5 this is achieved
    /// by attaching a hidden dummy enchantment; 1.20.5+ uses the dedicated
    /// `enchantment_glint_override` data component.
    glint: bool,
}

impl LobbySlot {
    /// An empty/absent slot (sent as "no item").
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            item_id: -1,
            count: 0,
            display_name: None,
            lore: Vec::new(),
            legacy_damage: 0,
            glint: false,
        }
    }

    /// Build a slot from the pre-resolved numeric item ID.
    #[must_use]
    pub fn new(
        item_id: i32,
        count: u8,
        display_name: Option<Component>,
        lore: Vec<Component>,
    ) -> Self {
        Self {
            item_id,
            count,
            display_name,
            lore,
            legacy_damage: 0,
            glint: false,
        }
    }

    /// Sets the pre-1.13 metadata/damage value used to select item variants on
    /// clients before the 1.13 Flattening.
    #[must_use]
    pub const fn with_legacy_damage(mut self, legacy_damage: i16) -> Self {
        self.legacy_damage = legacy_damage;
        self
    }

    /// Forces the enchantment glint to render on this slot without adding a
    /// visible enchantment to the tooltip.
    #[must_use]
    pub const fn with_glint(mut self, glint: bool) -> Self {
        self.glint = glint;
        self
    }

    pub const fn item_id(&self) -> i32 {
        self.item_id
    }

    pub const fn count(&self) -> u8 {
        self.count
    }
}

impl EncodePacket for LobbySlot {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        if version.is_after_inclusive(ProtocolVersion::V1_20_5) {
            encode_structured(self, writer, version)
        } else if version.is_after_inclusive(ProtocolVersion::V1_13_2) {
            encode_legacy_var_int(self, writer, version)
        } else if version.is_after_inclusive(ProtocolVersion::V1_13) {
            encode_flat_short(self, writer, version)
        } else {
            encode_legacy_short(self, writer, version)
        }
    }
}

// ── 1.20.5+ structured components ──────────────────────────────────────────

fn encode_structured(
    slot: &LobbySlot,
    writer: &mut BinaryWriter,
    version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    if slot.item_id < 0 || slot.count == 0 {
        VarInt::new(0).encode(writer, version)?;
        return Ok(());
    }

    VarInt::new(i32::from(slot.count)).encode(writer, version)?;
    VarInt::new(slot.item_id).encode(writer, version)?;

    let add_count =
        slot.display_name.is_some() as i32 + (!slot.lore.is_empty()) as i32 + slot.glint as i32;
    VarInt::new(add_count).encode(writer, version)?;
    VarInt::new(0).encode(writer, version)?; // remove count

    if let Some(name) = &slot.display_name {
        VarInt::new(custom_name_component_id(version)).encode(writer, version)?;
        // custom_name data: single TextComponent as nameless NBT compound
        name.to_nbt().encode(writer, version)?;
    }

    if !slot.lore.is_empty() {
        VarInt::new(lore_component_id(version)).encode(writer, version)?;
        // lore data: VarInt length + N × TextComponent (each as nameless NBT compound)
        VarInt::new(slot.lore.len() as i32).encode(writer, version)?;
        for line in &slot.lore {
            line.to_nbt().encode(writer, version)?;
        }
    }

    if slot.glint {
        VarInt::new(glint_override_component_id(version)).encode(writer, version)?;
        // enchantment_glint_override data: a single boolean.
        true.encode(writer, version)?;
    }

    Ok(())
}

fn custom_name_component_id(version: ProtocolVersion) -> i32 {
    if version.is_after_inclusive(ProtocolVersion::V1_21_11) {
        6
    } else {
        5
    }
}

fn lore_component_id(version: ProtocolVersion) -> i32 {
    if version.is_after_inclusive(ProtocolVersion::V1_21_11) {
        11
    } else if version.is_after_inclusive(ProtocolVersion::V1_21_2) {
        8
    } else {
        7
    }
}

/// Protocol ID of the `minecraft:enchantment_glint_override` data component.
/// The component-type registry was re-ordered several times across 1.21.x, so
/// the id is non-monotonic: 1.20.5–1.21.1 and 1.21.5–1.21.10 use 18, the
/// 1.21.2–1.21.4 range bumped it to 19, and 1.21.11+ uses 21.
fn glint_override_component_id(version: ProtocolVersion) -> i32 {
    if version.is_after_inclusive(ProtocolVersion::V1_21_11) {
        21
    } else if version.is_after_inclusive(ProtocolVersion::V1_21_5) {
        18
    } else if version.is_after_inclusive(ProtocolVersion::V1_21_2) {
        19
    } else {
        18
    }
}

// ── 1.13.2 – 1.20.4  (VarInt ID + optional NBT) ────────────────────────────

fn encode_legacy_var_int(
    slot: &LobbySlot,
    writer: &mut BinaryWriter,
    version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    if slot.item_id < 0 {
        false.encode(writer, version)?;
        return Ok(());
    }

    true.encode(writer, version)?;
    VarInt::new(slot.item_id).encode(writer, version)?;
    (slot.count as i8).encode(writer, version)?;
    encode_opt_nbt_modern(slot, writer, version)
}

fn encode_opt_nbt_modern(
    slot: &LobbySlot,
    writer: &mut BinaryWriter,
    version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    let nbt = build_display_nbt_modern(&slot.display_name, &slot.lore, slot.glint, version);
    if let Some(nbt) = nbt {
        nbt.encode(writer, version)?;
    } else {
        // TAG_End = no NBT
        0u8.encode(writer, version)?;
    }
    Ok(())
}

/// Builds a display NBT compound for 1.13 – 1.20.4.
///
/// Item names are JSON text components throughout this range. Lore is legacy
/// formatted text on 1.13.x, then JSON text components from 1.14 onward.
///
/// When `glint` is set, a dummy `Enchantments` entry is attached to force the
/// enchantment glint, and `HideFlags` bit `0x1` hides the enchantment line from
/// the tooltip so only the visual glint remains.
fn build_display_nbt_modern(
    display_name: &Option<Component>,
    lore: &[Component],
    glint: bool,
    version: ProtocolVersion,
) -> Option<Value> {
    let mut display: IndexMap<String, Value> = IndexMap::new();

    if let Some(name) = display_name {
        display.insert("Name".to_string(), Value::String(name.to_json()));
    }

    if !lore.is_empty() {
        let lore_values: Vec<Value> = if version.is_before_inclusive(ProtocolVersion::V1_13_2) {
            lore.iter()
                .map(|c| Value::String(c.to_legacy_text()))
                .collect()
        } else {
            lore.iter().map(|c| Value::String(c.to_json())).collect()
        };
        display.insert("Lore".to_string(), Value::List(lore_values));
    }

    let mut tag: IndexMap<String, Value> = IndexMap::new();

    if !display.is_empty() {
        tag.insert("display".to_string(), Value::Compound(display));
    }

    if glint {
        // 1.13+ enchantments are a list of {id: <namespaced string>, lvl: short}.
        let mut enchant: IndexMap<String, Value> = IndexMap::new();
        enchant.insert(
            "id".to_string(),
            Value::String("minecraft:unbreaking".to_string()),
        );
        enchant.insert("lvl".to_string(), Value::Short(1));
        tag.insert(
            "Enchantments".to_string(),
            Value::List(vec![Value::Compound(enchant)]),
        );
        // HideFlags bit 0x1 hides the enchantment tooltip line, keeping the glint.
        tag.insert("HideFlags".to_string(), Value::Int(1));
    }

    if tag.is_empty() {
        return None;
    }

    Some(Value::Compound(tag))
}

// ── 1.13 – 1.13.1  (Short ID + optional NBT) ───────────────────────────────

fn encode_flat_short(
    slot: &LobbySlot,
    writer: &mut BinaryWriter,
    version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    if slot.item_id < 0 {
        (-1i16).encode(writer, version)?;
        return Ok(());
    }

    (slot.item_id as i16).encode(writer, version)?;
    (slot.count as i8).encode(writer, version)?;
    encode_opt_nbt_modern(slot, writer, version)
}

// ── Pre-1.13  (Short ID + Short damage + optional NBT) ─────────────────────

fn encode_legacy_short(
    slot: &LobbySlot,
    writer: &mut BinaryWriter,
    version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    if slot.item_id < 0 {
        (-1i16).encode(writer, version)?;
        return Ok(());
    }

    (slot.item_id as i16).encode(writer, version)?;
    (slot.count as i8).encode(writer, version)?;
    slot.legacy_damage.encode(writer, version)?; // damage / meta
    encode_opt_nbt_legacy(slot, writer, version)
}

fn encode_opt_nbt_legacy(
    slot: &LobbySlot,
    writer: &mut BinaryWriter,
    version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    let nbt = build_display_nbt_legacy(&slot.display_name, &slot.lore, slot.glint);
    if let Some(nbt) = nbt {
        nbt.encode(writer, version)?;
    } else {
        0u8.encode(writer, version)?;
    }
    Ok(())
}

/// Builds `{display: {Name: "<legacy text>", Lore: ["<legacy text>", ...]}}` for pre-1.13.
///
/// When `glint` is set, a dummy `ench` entry forces the enchantment glint and
/// `HideFlags` bit `0x1` hides the enchantment tooltip line (1.8+; on 1.7.x
/// `HideFlags` is ignored and the dummy enchantment line shows).
fn build_display_nbt_legacy(
    display_name: &Option<Component>,
    lore: &[Component],
    glint: bool,
) -> Option<Value> {
    let mut display: IndexMap<String, Value> = IndexMap::new();

    if let Some(name) = display_name {
        // Pre-flattening (pre-1.13): Name and Lore are raw legacy-formatted strings, not JSON.
        display.insert("Name".to_string(), Value::String(name.to_legacy_text()));
    }

    if !lore.is_empty() {
        let lore_values: Vec<Value> = lore
            .iter()
            .map(|line| Value::String(line.to_legacy_text()))
            .collect();
        display.insert("Lore".to_string(), Value::List(lore_values));
    }

    let mut tag: IndexMap<String, Value> = IndexMap::new();

    if !display.is_empty() {
        tag.insert("display".to_string(), Value::Compound(display));
    }

    if glint {
        // Pre-1.13 enchantments are a list of {id: short, lvl: short}; id 0 is
        // Protection. The value is irrelevant — any entry triggers the glint.
        let mut enchant: IndexMap<String, Value> = IndexMap::new();
        enchant.insert("id".to_string(), Value::Short(0));
        enchant.insert("lvl".to_string(), Value::Short(1));
        tag.insert(
            "ench".to_string(),
            Value::List(vec![Value::Compound(enchant)]),
        );
        tag.insert("HideFlags".to_string(), Value::Int(1));
    }

    if tag.is_empty() {
        return None;
    }

    Some(Value::Compound(tag))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pico_text_component::prelude::parse_mini_message;

    fn encode(slot: &LobbySlot, version: ProtocolVersion) -> Vec<u8> {
        let mut writer = BinaryWriter::default();
        slot.encode(&mut writer, version).expect("encode failed");
        writer.as_slice().to_vec()
    }

    fn display_tag(value: &Value) -> &IndexMap<String, Value> {
        value
            .get_compound()
            .expect("root compound")
            .get("display")
            .expect("display tag")
            .get_compound()
            .expect("display compound")
    }

    #[test]
    fn empty_slot_wire_format_per_era() {
        // Pre-1.13 and 1.13/1.13.1: Short -1.  1.13.2–1.20.4: bool false.  1.20.5+: VarInt 0.
        let cases: &[(ProtocolVersion, &[u8])] = &[
            (ProtocolVersion::V1_12_2, &[0xFF, 0xFF]),
            (ProtocolVersion::V1_13_1, &[0xFF, 0xFF]),
            (ProtocolVersion::V1_13_2, &[0x00]),
            (ProtocolVersion::V1_20_5, &[0x00]),
        ];
        let slot = LobbySlot::empty();
        for &(version, expected) in cases {
            assert_eq!(&encode(&slot, version), expected, "{version:?}");
        }
    }

    #[test]
    fn glint_structured_appends_glint_override_component() {
        // 1.21: count 1, item 888, add_count 1, remove 0, glint id 18, bool true.
        let bytes = encode(
            &LobbySlot::new(888, 1, None, Vec::new()).with_glint(true),
            ProtocolVersion::V1_21,
        );
        assert_eq!(bytes, &[0x01, 0xF8, 0x06, 0x01, 0x00, 0x12, 0x01]);

        // 1.21.2 re-ordered the registry: glint override is id 19.
        let bytes = encode(
            &LobbySlot::new(888, 1, None, Vec::new()).with_glint(true),
            ProtocolVersion::V1_21_2,
        );
        assert_eq!(bytes, &[0x01, 0xF8, 0x06, 0x01, 0x00, 0x13, 0x01]);
    }

    #[test]
    fn glint_override_component_id_per_bucket() {
        for (version, expected) in [
            (ProtocolVersion::V1_20_5, 18),
            (ProtocolVersion::V1_21, 18),
            (ProtocolVersion::V1_21_2, 19),
            (ProtocolVersion::V1_21_5, 18),
            (ProtocolVersion::V1_21_9, 18),
            (ProtocolVersion::V1_21_11, 21),
        ] {
            assert_eq!(
                glint_override_component_id(version),
                expected,
                "{version:?}"
            );
        }
    }

    #[test]
    fn glint_modern_nbt_adds_hidden_dummy_enchantment() {
        // 1.13.2–1.20.4: glint is faked with an Enchantments list + HideFlags.
        let nbt = build_display_nbt_modern(&None, &[], true, ProtocolVersion::V1_20).unwrap();
        let root = nbt.get_compound().expect("root compound");
        assert!(root.get("display").is_none(), "no display data expected");
        assert_eq!(root.get("HideFlags").and_then(Value::get_int), Some(1));
        let ench = root.get("Enchantments").and_then(Value::get_list).unwrap();
        let entry = ench[0].get_compound().unwrap();
        assert_eq!(
            entry.get("id").and_then(Value::get_str),
            Some("minecraft:unbreaking")
        );

        // Without glint and without display data, no NBT is produced.
        assert!(build_display_nbt_modern(&None, &[], false, ProtocolVersion::V1_20).is_none());
    }

    #[test]
    fn glint_legacy_nbt_adds_hidden_dummy_enchantment() {
        // Pre-1.13: glint uses the numeric `ench` list + HideFlags.
        let nbt = build_display_nbt_legacy(&None, &[], true).unwrap();
        let root = nbt.get_compound().expect("root compound");
        assert_eq!(root.get("HideFlags").and_then(Value::get_int), Some(1));
        let ench = root.get("ench").and_then(Value::get_list).unwrap();
        let entry = ench[0].get_compound().unwrap();
        assert_eq!(entry.get("id").and_then(Value::get_short), Some(0));

        assert!(build_display_nbt_legacy(&None, &[], false).is_none());
    }

    #[test]
    fn non_empty_slot_pre_1_13_encodes_short_id_and_damage() {
        // compass ID 345 = 0x0159; damage field is always 0.
        let bytes = encode(
            &LobbySlot::new(345, 1, None, Vec::new()),
            ProtocolVersion::V1_12_2,
        );
        assert_eq!(&bytes[..6], &[0x01, 0x59, 0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn non_empty_slot_pre_1_13_encodes_variant_metadata_as_damage() {
        // Red wool: item 35 = 0x0023, count 1, metadata 14 = 0x000E in the
        // pre-Flattening damage field. NBT byte 0x00 (no display data).
        let bytes = encode(
            &LobbySlot::new(35, 1, None, Vec::new()).with_legacy_damage(14),
            ProtocolVersion::V1_12_2,
        );
        assert_eq!(&bytes[..6], &[0x00, 0x23, 0x01, 0x00, 0x0E, 0x00]);
    }

    #[test]
    fn legacy_damage_is_ignored_on_flattened_wire_formats() {
        // 1.13+ identifies variants by item id, so the damage value never
        // appears on the wire.
        let bytes = encode(
            &LobbySlot::new(562, 1, None, Vec::new()).with_legacy_damage(14),
            ProtocolVersion::V1_13_1,
        );
        assert_eq!(&bytes[..4], &[0x02, 0x32, 0x01, 0x00]);
        assert_eq!(bytes.len(), 4);
    }

    #[test]
    fn non_empty_slot_v1_13_flat_short_without_damage() {
        // 1.13/1.13.1: flattened IDs, Short item_id, no damage field.
        let bytes = encode(
            &LobbySlot::new(562, 1, None, Vec::new()),
            ProtocolVersion::V1_13_1,
        );
        // 562 = 0x0232; no damage field → 4 bytes total.
        assert_eq!(&bytes[..4], &[0x02, 0x32, 0x01, 0x00]);
        assert_eq!(bytes.len(), 4);
    }

    #[test]
    fn non_empty_slot_modern_bool_present_varint_id() {
        // 1.13.2–1.20.4: bool true, VarInt item_id, i8 count.
        // item 888: VarInt → [0xF8, 0x06]
        let bytes = encode(
            &LobbySlot::new(888, 1, None, Vec::new()),
            ProtocolVersion::V1_20,
        );
        assert_eq!(&bytes[..5], &[0x01, 0xF8, 0x06, 0x01, 0x00]);
    }

    #[test]
    fn non_empty_slot_structured_encodes_count_then_item_id() {
        // 1.20.5+: VarInt count, VarInt item_id, add/remove component counts.
        let bytes = encode(
            &LobbySlot::new(888, 1, None, Vec::new()),
            ProtocolVersion::V1_20_5,
        );
        assert_eq!(&bytes[..5], &[0x01, 0xF8, 0x06, 0x00, 0x00]);
    }

    #[test]
    fn nbt_lore_format_switches_from_legacy_to_json_at_v1_14() {
        let name = parse_mini_message("<bold><gold>Server Selector").unwrap();
        let lore = vec![parse_mini_message("<gray>Right-click").unwrap()];

        // 1.13.2: name is JSON, lore is legacy text.
        let nbt =
            build_display_nbt_modern(&Some(name), &lore, false, ProtocolVersion::V1_13_2).unwrap();
        let display = display_tag(&nbt);
        assert!(
            display
                .get("Name")
                .and_then(Value::get_str)
                .unwrap()
                .contains("\"color\":\"gold\"")
        );
        assert_eq!(
            display.get("Lore").and_then(Value::get_list).unwrap()[0].get_str(),
            Some("\u{00a7}r\u{00a7}7Right-click")
        );

        // 1.14+: both name and lore are JSON.
        let lore2 = vec![parse_mini_message("<gray>Right-click").unwrap()];
        let nbt2 = build_display_nbt_modern(&None, &lore2, false, ProtocolVersion::V1_14).unwrap();
        let display2 = display_tag(&nbt2);
        assert!(
            display2.get("Lore").and_then(Value::get_list).unwrap()[0]
                .get_str()
                .unwrap()
                .contains("\"color\":\"gray\"")
        );
    }

    #[test]
    fn pre_1_13_name_and_lore_use_legacy_text() {
        let name = parse_mini_message("<bold><gold>Server Selector").unwrap();
        let lore = vec![parse_mini_message("<gray>Right-click").unwrap()];

        let nbt = build_display_nbt_legacy(&Some(name), &lore, false).unwrap();
        let display = display_tag(&nbt);
        assert_eq!(
            display.get("Name").and_then(Value::get_str),
            Some("\u{00a7}r\u{00a7}6\u{00a7}lServer Selector")
        );
        assert_eq!(
            display.get("Lore").and_then(Value::get_list).unwrap()[0].get_str(),
            Some("\u{00a7}r\u{00a7}7Right-click")
        );
    }
}
