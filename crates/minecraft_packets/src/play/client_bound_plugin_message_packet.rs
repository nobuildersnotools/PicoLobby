use minecraft_protocol::prelude::*;

pub struct PlayClientBoundPluginMessagePacket {
    modern_channel: Identifier,
    legacy_channel: String,
    data: Vec<i8>,
}

impl PlayClientBoundPluginMessagePacket {
    pub fn brand(brand: impl ToString) -> Self {
        Self {
            modern_channel: Identifier::vanilla_unchecked("brand"),
            legacy_channel: "MC|Brand".to_owned(),
            data: minecraft_string_payload(&brand.to_string()),
        }
    }

    /// Constructs a BungeeCord `Connect` plugin message that instructs the Velocity
    /// proxy to move the player to the named downstream server.
    pub fn bungeecord_connect(server_name: &str) -> Self {
        Self {
            modern_channel: Identifier::new_unchecked("bungeecord", "main"),
            legacy_channel: "BungeeCord".to_owned(),
            data: bungeecord_payload("Connect", server_name),
        }
    }

    fn channel_name(&self, version: ProtocolVersion) -> String {
        if version.is_before_inclusive(ProtocolVersion::V1_12_2) {
            self.legacy_channel.clone()
        } else {
            format!(
                "{}:{}",
                self.modern_channel.namespace, self.modern_channel.thing
            )
        }
    }
}

impl EncodePacket for PlayClientBoundPluginMessagePacket {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        self.channel_name(version).encode(writer, version)?;
        self.data.encode(writer, version)
    }
}

/// Encodes a BungeeCord two-field plugin message payload.
/// Each field is written as Java `DataOutputStream.writeUTF()`:
/// a big-endian u16 byte-length followed by the UTF-8 bytes.
fn bungeecord_payload(subchannel: &str, argument: &str) -> Vec<i8> {
    let mut buf: Vec<i8> = Vec::new();
    write_java_utf8(&mut buf, subchannel);
    write_java_utf8(&mut buf, argument);
    buf
}

fn minecraft_string_payload(s: &str) -> Vec<i8> {
    let mut buf = Vec::with_capacity(varint_size(s.len()) + s.len());
    write_varint_usize(&mut buf, s.len());
    buf.extend(s.as_bytes().iter().map(|&b| b as i8));
    buf
}

fn write_varint_usize(buf: &mut Vec<i8>, value: usize) {
    let mut value = value as u32;
    loop {
        if value & !0x7F == 0 {
            buf.push(value as i8);
            return;
        }
        buf.push(((value & 0x7F) | 0x80) as u8 as i8);
        value >>= 7;
    }
}

const fn varint_size(value: usize) -> usize {
    match value {
        0..=0x7F => 1,
        0x80..=0x3FFF => 2,
        0x4000..=0x1F_FFFF => 3,
        0x20_0000..=0xFFF_FFFF => 4,
        _ => 5,
    }
}

fn write_java_utf8(buf: &mut Vec<i8>, s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len() as u16;
    buf.push((len >> 8) as i8);
    buf.push(len as u8 as i8);
    for &b in bytes {
        buf.push(b as i8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bungeecord_connect_channel_and_payload() {
        let packet = PlayClientBoundPluginMessagePacket::bungeecord_connect("survival");

        assert_eq!(
            packet.channel_name(ProtocolVersion::V1_21),
            "bungeecord:main"
        );
        assert_eq!(packet.channel_name(ProtocolVersion::V1_12_2), "BungeeCord");

        let data: Vec<u8> = packet.data.iter().map(|&b| b as u8).collect();
        let mut expected: Vec<u8> = Vec::new();
        expected.extend_from_slice(&[0x00, 0x07]); // "Connect" length
        expected.extend_from_slice(b"Connect");
        expected.extend_from_slice(&[0x00, 0x08]); // "survival" length
        expected.extend_from_slice(b"survival");
        assert_eq!(data, expected);
    }

    #[test]
    fn bungeecord_connect_empty_server_name_produces_zero_length_field() {
        let packet = PlayClientBoundPluginMessagePacket::bungeecord_connect("");
        let data: Vec<u8> = packet.data.iter().map(|&b| b as u8).collect();
        // "Connect" header then 0x00 0x00 for empty name
        assert_eq!(&data[9..], &[0x00, 0x00]);
    }

    #[test]
    fn modern_bungeecord_connect_packet_has_no_outer_payload_length() {
        let packet = PlayClientBoundPluginMessagePacket::bungeecord_connect("auth");
        let mut writer = BinaryWriter::default();
        packet.encode(&mut writer, ProtocolVersion::V1_21).unwrap();

        let mut expected = Vec::new();
        expected.push(15); // channel string length
        expected.extend_from_slice(b"bungeecord:main");
        expected.extend_from_slice(&[0x00, 0x07]);
        expected.extend_from_slice(b"Connect");
        expected.extend_from_slice(&[0x00, 0x04]);
        expected.extend_from_slice(b"auth");

        assert_eq!(writer.as_slice(), expected);
    }

    #[test]
    fn legacy_bungeecord_connect_uses_bungeecord_channel() {
        let packet = PlayClientBoundPluginMessagePacket::bungeecord_connect("auth");
        let mut writer = BinaryWriter::default();
        packet
            .encode(&mut writer, ProtocolVersion::V1_12_2)
            .unwrap();

        let mut expected = Vec::new();
        expected.push(10); // channel string length
        expected.extend_from_slice(b"BungeeCord");
        expected.extend_from_slice(&[0x00, 0x07]);
        expected.extend_from_slice(b"Connect");
        expected.extend_from_slice(&[0x00, 0x04]);
        expected.extend_from_slice(b"auth");

        assert_eq!(writer.as_slice(), expected);
    }

    #[test]
    fn brand_payload_still_encodes_minecraft_string() {
        let packet = PlayClientBoundPluginMessagePacket::brand("PicoLobby");
        let data: Vec<u8> = packet.data.iter().map(|&b| b as u8).collect();

        let mut expected = Vec::new();
        expected.push(9);
        expected.extend_from_slice(b"PicoLobby");

        assert_eq!(data, expected);
    }
}
