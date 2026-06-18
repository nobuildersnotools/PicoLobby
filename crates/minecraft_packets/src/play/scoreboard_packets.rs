use minecraft_protocol::prelude::*;
use pico_text_component::prelude::Component;

const TEAM_COLOR_RESET_LEGACY: i8 = -1;
const TEAM_COLOR_RESET_MODERN: i32 = 21;
const TEAM_VISIBILITY_ALWAYS: i32 = 0;
const TEAM_COLLISION_ALWAYS: i32 = 0;

#[derive(Clone)]
pub struct SetObjectivePacket {
    name: String,
    title: Component,
    mode: i8,
}

impl SetObjectivePacket {
    pub fn create(name: impl Into<String>, title: Component) -> Self {
        Self {
            name: name.into(),
            title,
            mode: 0,
        }
    }

    pub fn update(name: impl Into<String>, title: Component) -> Self {
        Self {
            name: name.into(),
            title,
            mode: 2,
        }
    }
}

impl EncodePacket for SetObjectivePacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        self.name.encode(writer, protocol_version)?;
        if protocol_version.is_before_inclusive(ProtocolVersion::V1_7_6) {
            truncate_chars(&self.title.to_legacy_text(), 32).encode(writer, protocol_version)?;
            self.mode.encode(writer, protocol_version)?;
        } else {
            self.mode.encode(writer, protocol_version)?;
            if protocol_version.is_before_inclusive(ProtocolVersion::V1_12_2) {
                truncate_chars(&self.title.to_legacy_text(), 32)
                    .encode(writer, protocol_version)?;
                "integer".to_string().encode(writer, protocol_version)?;
            } else {
                self.title.encode(writer, protocol_version)?;
                VarInt::new(0).encode(writer, protocol_version)?;
                if protocol_version.is_after_inclusive(ProtocolVersion::V1_20_3) {
                    Optional::<VarInt>::None.encode(writer, protocol_version)?;
                }
            }
        }
        Ok(())
    }
}

pub struct SetDisplayObjectivePacket {
    slot: i32,
    objective_name: String,
}

impl SetDisplayObjectivePacket {
    pub fn sidebar(objective_name: impl Into<String>) -> Self {
        Self {
            slot: 1,
            objective_name: objective_name.into(),
        }
    }
}

impl EncodePacket for SetDisplayObjectivePacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        if protocol_version.is_before_inclusive(ProtocolVersion::V1_12_2) {
            i8::try_from(self.slot)
                .unwrap_or(1)
                .encode(writer, protocol_version)?;
        } else {
            VarInt::new(self.slot).encode(writer, protocol_version)?;
        }
        self.objective_name.encode(writer, protocol_version)
    }
}

pub struct SetScorePacket {
    entry: String,
    objective_name: String,
    value: i32,
}

impl SetScorePacket {
    pub fn change(entry: impl Into<String>, objective_name: impl Into<String>, value: i32) -> Self {
        Self {
            entry: entry.into(),
            objective_name: objective_name.into(),
            value,
        }
    }
}

impl EncodePacket for SetScorePacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        self.entry.encode(writer, protocol_version)?;
        if protocol_version.is_before_inclusive(ProtocolVersion::V1_7_6) {
            0_i8.encode(writer, protocol_version)?;
            self.objective_name.encode(writer, protocol_version)?;
            self.value.encode(writer, protocol_version)?;
        } else if protocol_version.is_before_inclusive(ProtocolVersion::V1_20_2) {
            VarInt::new(0).encode(writer, protocol_version)?;
            self.objective_name.encode(writer, protocol_version)?;
            VarInt::new(self.value).encode(writer, protocol_version)?;
        } else {
            self.objective_name.encode(writer, protocol_version)?;
            VarInt::new(self.value).encode(writer, protocol_version)?;
            Optional::<Component>::None.encode(writer, protocol_version)?;
            Optional::<VarInt>::None.encode(writer, protocol_version)?;
        }
        Ok(())
    }
}

pub struct ResetScorePacket {
    entry: String,
    objective_name: String,
}

impl ResetScorePacket {
    pub fn new(entry: impl Into<String>, objective_name: impl Into<String>) -> Self {
        Self {
            entry: entry.into(),
            objective_name: objective_name.into(),
        }
    }
}

impl EncodePacket for ResetScorePacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        self.entry.encode(writer, protocol_version)?;
        Optional::Some(self.objective_name.clone()).encode(writer, protocol_version)
    }
}

pub struct SetPlayerTeamPacket {
    name: String,
    mode: i8,
    display_name: Component,
    prefix: Component,
    suffix: Component,
    entries: Vec<String>,
}

impl SetPlayerTeamPacket {
    pub fn create(
        name: impl Into<String>,
        display_name: Component,
        prefix: Component,
        suffix: Component,
        entries: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            mode: 0,
            display_name,
            prefix,
            suffix,
            entries,
        }
    }

    pub fn update(
        name: impl Into<String>,
        display_name: Component,
        prefix: Component,
        suffix: Component,
        entries: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            mode: 2,
            display_name,
            prefix,
            suffix,
            entries,
        }
    }
}

impl EncodePacket for SetPlayerTeamPacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        self.name.encode(writer, protocol_version)?;
        self.mode.encode(writer, protocol_version)?;

        if protocol_version.is_before_inclusive(ProtocolVersion::V1_12_2) {
            truncate_chars(&self.display_name.to_legacy_text(), 32)
                .encode(writer, protocol_version)?;
            truncate_chars(&self.prefix.to_legacy_text(), 16).encode(writer, protocol_version)?;
            truncate_chars(&self.suffix.to_legacy_text(), 16).encode(writer, protocol_version)?;
            0_i8.encode(writer, protocol_version)?;
            if protocol_version.is_after_inclusive(ProtocolVersion::V1_8) {
                "always".to_string().encode(writer, protocol_version)?;
                if protocol_version.is_after_inclusive(ProtocolVersion::V1_9) {
                    "always".to_string().encode(writer, protocol_version)?;
                }
                TEAM_COLOR_RESET_LEGACY.encode(writer, protocol_version)?;
                if self.mode == 0 {
                    LengthPaddedVec::new(self.entries.clone()).encode(writer, protocol_version)?;
                }
            } else {
                if self.mode == 0 {
                    i16::try_from(self.entries.len())
                        .unwrap_or(i16::MAX)
                        .encode(writer, protocol_version)?;
                    for entry in &self.entries {
                        entry.encode(writer, protocol_version)?;
                    }
                }
            }
        } else if protocol_version.is_after_inclusive(ProtocolVersion::V26_2) {
            // 26.2 reworked the team parameters block: prefix/suffix now follow
            // the display name directly, the team color became an
            // `Optional<TeamColor>` (a new 16-value enum with no RESET sentinel,
            // so "no color" is encoded as an empty optional), and the packed
            // friendly-flags byte moved to the end.
            self.display_name.encode(writer, protocol_version)?;
            self.prefix.encode(writer, protocol_version)?;
            self.suffix.encode(writer, protocol_version)?;
            VarInt::new(TEAM_VISIBILITY_ALWAYS).encode(writer, protocol_version)?;
            VarInt::new(TEAM_COLLISION_ALWAYS).encode(writer, protocol_version)?;
            Optional::<VarInt>::None.encode(writer, protocol_version)?;
            0_i8.encode(writer, protocol_version)?;
            if self.mode == 0 {
                LengthPaddedVec::new(self.entries.clone()).encode(writer, protocol_version)?;
            }
        } else {
            self.display_name.encode(writer, protocol_version)?;
            0_i8.encode(writer, protocol_version)?;
            if protocol_version.is_after_inclusive(ProtocolVersion::V1_21_5) {
                VarInt::new(TEAM_VISIBILITY_ALWAYS).encode(writer, protocol_version)?;
                VarInt::new(TEAM_COLLISION_ALWAYS).encode(writer, protocol_version)?;
            } else {
                "always".to_string().encode(writer, protocol_version)?;
                "always".to_string().encode(writer, protocol_version)?;
            }
            VarInt::new(TEAM_COLOR_RESET_MODERN).encode(writer, protocol_version)?;
            self.prefix.encode(writer, protocol_version)?;
            self.suffix.encode(writer, protocol_version)?;
            if self.mode == 0 {
                LengthPaddedVec::new(self.entries.clone()).encode(writer, protocol_version)?;
            }
        }
        Ok(())
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode<T: EncodePacket>(packet: T, protocol_version: ProtocolVersion) {
        let mut writer = BinaryWriter::default();
        packet.encode(&mut writer, protocol_version).unwrap();
        assert!(!writer.as_slice().is_empty());
    }

    #[test]
    fn scoreboard_packets_encode_for_representative_versions() {
        for version in [
            ProtocolVersion::V1_7_2,
            ProtocolVersion::V1_8,
            ProtocolVersion::V1_12_2,
            ProtocolVersion::V1_13,
            ProtocolVersion::V1_17,
            ProtocolVersion::V1_20_5,
            ProtocolVersion::V26_1,
        ] {
            encode(
                SetObjectivePacket::create("picolobby", Component::new("PicoLobby")),
                version,
            );
            encode(SetDisplayObjectivePacket::sidebar("picolobby"), version);
            encode(SetScorePacket::change("\u{00a7}0", "picolobby", 1), version);
            encode(
                SetPlayerTeamPacket::create(
                    "plsb00",
                    Component::new(""),
                    Component::new("Line"),
                    Component::new(""),
                    vec!["\u{00a7}0".to_string()],
                ),
                version,
            );
        }
    }

    #[test]
    fn team_packet_uses_valid_modern_reset_color() {
        let packet = SetPlayerTeamPacket::create(
            "plsb00",
            Component::new(""),
            Component::new("Line"),
            Component::new(""),
            vec!["\u{00a7}0".to_string()],
        );
        let mut writer = BinaryWriter::default();
        packet.encode(&mut writer, ProtocolVersion::V1_13).unwrap();

        let mut reader = BinaryReader::new(writer.as_slice());
        let _team_name = String::decode(&mut reader, ProtocolVersion::V1_13).unwrap();
        let _mode = i8::decode(&mut reader, ProtocolVersion::V1_13).unwrap();
        let _display_name = String::decode(&mut reader, ProtocolVersion::V1_13).unwrap();
        let _friendly_flags = i8::decode(&mut reader, ProtocolVersion::V1_13).unwrap();
        let _name_tag_visibility = String::decode(&mut reader, ProtocolVersion::V1_13).unwrap();
        let _collision_rule = String::decode(&mut reader, ProtocolVersion::V1_13).unwrap();
        let color = VarInt::decode(&mut reader, ProtocolVersion::V1_13)
            .unwrap()
            .inner();

        assert_eq!(color, TEAM_COLOR_RESET_MODERN);
    }

    #[test]
    fn team_packet_omits_collision_rule_for_v1_8() {
        let packet = SetPlayerTeamPacket::create(
            "plsb00",
            Component::new(""),
            Component::new("\u{00a7}r\u{00a7}7Player: \u{00a7}r\u{00a7}f"),
            Component::new("\u{00a7}fpr0jectmarch"),
            vec!["\u{00a7}0".to_string()],
        );
        let mut writer = BinaryWriter::default();
        packet.encode(&mut writer, ProtocolVersion::V1_8).unwrap();

        let mut reader = BinaryReader::new(writer.as_slice());
        let _team_name = String::decode(&mut reader, ProtocolVersion::V1_8).unwrap();
        let _mode = i8::decode(&mut reader, ProtocolVersion::V1_8).unwrap();
        let _display_name = String::decode(&mut reader, ProtocolVersion::V1_8).unwrap();
        let _prefix = String::decode(&mut reader, ProtocolVersion::V1_8).unwrap();
        let _suffix = String::decode(&mut reader, ProtocolVersion::V1_8).unwrap();
        let _friendly_flags = i8::decode(&mut reader, ProtocolVersion::V1_8).unwrap();
        let name_tag_visibility = String::decode(&mut reader, ProtocolVersion::V1_8).unwrap();
        let color = i8::decode(&mut reader, ProtocolVersion::V1_8).unwrap();
        let entries = LengthPaddedVec::<String>::decode(&mut reader, ProtocolVersion::V1_8)
            .unwrap()
            .into_inner();

        assert_eq!(name_tag_visibility, "always");
        assert_eq!(color, TEAM_COLOR_RESET_LEGACY);
        assert_eq!(entries, vec!["\u{00a7}0".to_string()]);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn team_packet_uses_enum_visibility_and_collision_from_v1_21_5() {
        let packet = SetPlayerTeamPacket::create(
            "plsb00",
            Component::new(""),
            Component::new("Line"),
            Component::new(""),
            vec!["\u{00a7}0".to_string()],
        );
        let mut writer = BinaryWriter::default();
        packet
            .encode(&mut writer, ProtocolVersion::V1_21_5)
            .unwrap();

        let mut expected = BinaryWriter::default();
        "plsb00"
            .to_string()
            .encode(&mut expected, ProtocolVersion::V1_21_5)
            .unwrap();
        0_i8.encode(&mut expected, ProtocolVersion::V1_21_5)
            .unwrap();
        Component::new("")
            .encode(&mut expected, ProtocolVersion::V1_21_5)
            .unwrap();
        0_i8.encode(&mut expected, ProtocolVersion::V1_21_5)
            .unwrap();
        VarInt::new(TEAM_VISIBILITY_ALWAYS)
            .encode(&mut expected, ProtocolVersion::V1_21_5)
            .unwrap();
        VarInt::new(TEAM_COLLISION_ALWAYS)
            .encode(&mut expected, ProtocolVersion::V1_21_5)
            .unwrap();
        VarInt::new(TEAM_COLOR_RESET_MODERN)
            .encode(&mut expected, ProtocolVersion::V1_21_5)
            .unwrap();
        Component::new("Line")
            .encode(&mut expected, ProtocolVersion::V1_21_5)
            .unwrap();
        Component::new("")
            .encode(&mut expected, ProtocolVersion::V1_21_5)
            .unwrap();
        LengthPaddedVec::new(vec!["\u{00a7}0".to_string()])
            .encode(&mut expected, ProtocolVersion::V1_21_5)
            .unwrap();

        assert_eq!(writer.as_slice(), expected.as_slice());
    }

    #[test]
    fn team_packet_reorders_parameters_and_uses_optional_color_from_v26_2() {
        let packet = SetPlayerTeamPacket::create(
            "plsb00",
            Component::new(""),
            Component::new("Line"),
            Component::new(""),
            vec!["\u{00a7}0".to_string()],
        );
        let mut writer = BinaryWriter::default();
        packet.encode(&mut writer, ProtocolVersion::V26_2).unwrap();

        // 26.2 order: name, mode, display name, prefix, suffix, visibility,
        // collision, Optional<TeamColor>, friendly-flags byte, then entries.
        let mut expected = BinaryWriter::default();
        "plsb00"
            .to_string()
            .encode(&mut expected, ProtocolVersion::V26_2)
            .unwrap();
        0_i8.encode(&mut expected, ProtocolVersion::V26_2).unwrap();
        Component::new("")
            .encode(&mut expected, ProtocolVersion::V26_2)
            .unwrap();
        Component::new("Line")
            .encode(&mut expected, ProtocolVersion::V26_2)
            .unwrap();
        Component::new("")
            .encode(&mut expected, ProtocolVersion::V26_2)
            .unwrap();
        VarInt::new(TEAM_VISIBILITY_ALWAYS)
            .encode(&mut expected, ProtocolVersion::V26_2)
            .unwrap();
        VarInt::new(TEAM_COLLISION_ALWAYS)
            .encode(&mut expected, ProtocolVersion::V26_2)
            .unwrap();
        Optional::<VarInt>::None
            .encode(&mut expected, ProtocolVersion::V26_2)
            .unwrap();
        0_i8.encode(&mut expected, ProtocolVersion::V26_2).unwrap();
        LengthPaddedVec::new(vec!["\u{00a7}0".to_string()])
            .encode(&mut expected, ProtocolVersion::V26_2)
            .unwrap();

        assert_eq!(writer.as_slice(), expected.as_slice());
    }
}
