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
        }
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

    let add_count = slot.display_name.is_some() as i32 + (!slot.lore.is_empty()) as i32;
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
    let nbt = build_display_nbt_modern(&slot.display_name, &slot.lore, version);
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
fn build_display_nbt_modern(
    display_name: &Option<Component>,
    lore: &[Component],
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

    if display.is_empty() {
        return None;
    }

    let mut tag: IndexMap<String, Value> = IndexMap::new();
    tag.insert("display".to_string(), Value::Compound(display));
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
    (0i16).encode(writer, version)?; // damage / meta
    encode_opt_nbt_legacy(slot, writer, version)
}

fn encode_opt_nbt_legacy(
    slot: &LobbySlot,
    writer: &mut BinaryWriter,
    version: ProtocolVersion,
) -> Result<(), BinaryWriterError> {
    let nbt = build_display_nbt_legacy(&slot.display_name, &slot.lore);
    if let Some(nbt) = nbt {
        nbt.encode(writer, version)?;
    } else {
        0u8.encode(writer, version)?;
    }
    Ok(())
}

/// Builds `{display: {Name: "<legacy text>", Lore: ["<legacy text>", ...]}}` for pre-1.13.
fn build_display_nbt_legacy(display_name: &Option<Component>, lore: &[Component]) -> Option<Value> {
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

    if display.is_empty() {
        return None;
    }

    let mut tag: IndexMap<String, Value> = IndexMap::new();
    tag.insert("display".to_string(), Value::Compound(display));
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
    fn non_empty_slot_pre_1_13_encodes_short_id_and_damage() {
        // compass ID 345 = 0x0159; damage field is always 0.
        let bytes = encode(
            &LobbySlot::new(345, 1, None, Vec::new()),
            ProtocolVersion::V1_12_2,
        );
        assert_eq!(&bytes[..6], &[0x01, 0x59, 0x01, 0x00, 0x00, 0x00]);
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
        let nbt = build_display_nbt_modern(&Some(name), &lore, ProtocolVersion::V1_13_2).unwrap();
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
        let nbt2 = build_display_nbt_modern(&None, &lore2, ProtocolVersion::V1_14).unwrap();
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

        let nbt = build_display_nbt_legacy(&Some(name), &lore).unwrap();
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
