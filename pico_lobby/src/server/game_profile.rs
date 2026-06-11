use md5::{Digest, Md5};
use minecraft_packets::login::Property;
use minecraft_packets::login::login_state_packet::LoginStartPacket;
use minecraft_protocol::prelude::*;
use uuid::Builder as UuidBuilder;

#[derive(Clone)]
pub struct GameProfile {
    username: String,
    uuid: Uuid,
    textures: Option<Property>,
}

impl GameProfile {
    pub fn new(username: &str, uuid: Uuid, textures: Option<Property>) -> Self {
        Self {
            username: sanitize_username(username),
            uuid,
            textures,
        }
    }

    pub const fn anonymous(uuid: Uuid, textures: Option<Property>) -> Self {
        Self {
            username: String::new(),
            uuid,
            textures,
        }
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub const fn is_anonymous(&self) -> bool {
        self.username.is_empty()
    }

    pub fn set_name<S>(&mut self, name: &S)
    where
        S: ToString,
    {
        self.username = name.to_string();
    }

    pub const fn uuid(&self) -> Uuid {
        self.uuid
    }

    pub const fn textures(&self) -> Option<&Property> {
        self.textures.as_ref()
    }
}

impl From<&LoginStartPacket> for GameProfile {
    fn from(value: &LoginStartPacket) -> Self {
        let username = sanitize_username(&value.name());
        let uuid = {
            let login_uuid = value.uuid();
            if login_uuid.is_nil() {
                offline_uuid_from_username(&username)
            } else {
                login_uuid
            }
        };

        Self {
            username,
            uuid,
            textures: None,
        }
    }
}

/// Maximum length of a Minecraft username, in characters.
const MAX_USERNAME_LEN: usize = 16;

/// Normalises a client-supplied username before it is stored, logged, or
/// broadcast to other players.
///
/// Control characters and the legacy section sign (`§`) are stripped to prevent
/// log/chat injection and colour spoofing, and the result is capped at
/// [`MAX_USERNAME_LEN`] characters. Behind a Velocity proxy the forwarded name
/// is already validated, so this is a no-op there; it is a safety net for any
/// direct (non-forwarded) connection where the name is fully client-controlled.
fn sanitize_username(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control() && *c != '\u{00a7}')
        .take(MAX_USERNAME_LEN)
        .collect()
}

fn offline_uuid_from_username(username: &str) -> Uuid {
    // Matches Java's UUID.nameUUIDFromBytes("OfflinePlayer:<username>" UTF-8 bytes).
    let mut hasher = Md5::new();
    hasher.update(b"OfflinePlayer:");
    hasher.update(username.as_bytes());
    let digest: [u8; 16] = hasher.finalize().into();
    UuidBuilder::from_md5_bytes(digest).into_uuid()
}

#[cfg(test)]
mod tests {
    use super::*;
    use minecraft_packets::login::login_state_packet::LoginStartPacket;
    use minecraft_protocol::prelude::{BinaryReader, BinaryWriter, DecodePacket, EncodePacket};
    use std::str::FromStr;

    #[test]
    fn login_start_with_uuid_keeps_packet_uuid() {
        let expected_uuid =
            Uuid::from_str("01234567-89ab-cdef-0123-456789abcdef").expect("valid uuid");
        let packet = build_login_start_packet("PlayerName", expected_uuid);
        let profile = GameProfile::from(&packet);

        assert_eq!(profile.uuid(), expected_uuid);
    }

    #[test]
    fn login_start_with_nil_uuid_uses_expected_offline_uuid_and_is_idempotent() {
        let packet = build_login_start_packet("PlayerName", Uuid::nil());
        let first = GameProfile::from(&packet);
        let second = GameProfile::from(&packet);
        let expected = Uuid::from_str("823dfbec-453f-3a13-bc3b-1afd172427d6").expect("valid uuid");

        assert_eq!(first.uuid(), expected);
        assert_eq!(second.uuid(), expected);
        assert_eq!(first.uuid(), second.uuid());
    }

    #[test]
    fn new_strips_control_and_section_chars_and_caps_length() {
        let profile = GameProfile::new("a\nb\u{00a7}c", Uuid::nil(), None);
        assert_eq!(profile.username(), "abc");

        let long = "x".repeat(40);
        let profile = GameProfile::new(&long, Uuid::nil(), None);
        assert_eq!(profile.username().chars().count(), 16);
    }

    fn build_login_start_packet(name: &str, uuid: Uuid) -> LoginStartPacket {
        let protocol_version = ProtocolVersion::V1_20_2;
        let mut writer = BinaryWriter::new();
        name.to_string()
            .encode(&mut writer, protocol_version)
            .expect("encode name");
        writer.write(&uuid).expect("encode uuid");
        let payload = writer.into_inner();
        let mut reader = BinaryReader::new(&payload);
        let packet =
            LoginStartPacket::decode(&mut reader, protocol_version).expect("decode login start");

        assert_eq!(packet.name(), name);
        packet
    }
}
