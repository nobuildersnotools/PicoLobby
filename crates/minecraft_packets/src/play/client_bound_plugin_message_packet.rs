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
    fn bungeecord_connect_payload_and_channel_names() {
        let packet = PlayClientBoundPluginMessagePacket::bungeecord_connect("survival");

        assert_eq!(
            packet.channel_name(ProtocolVersion::V1_21),
            "bungeecord:main"
        );
        assert_eq!(packet.channel_name(ProtocolVersion::V1_12_2), "BungeeCord");

        let data: Vec<u8> = packet.data.iter().map(|&b| b as u8).collect();
        let mut expected: Vec<u8> = Vec::new();
        expected.extend_from_slice(&[0x00, 0x07]);
        expected.extend_from_slice(b"Connect");
        expected.extend_from_slice(&[0x00, 0x08]);
        expected.extend_from_slice(b"survival");
        assert_eq!(data, expected);

        // Empty server name: two zero bytes for the length field.
        let empty = PlayClientBoundPluginMessagePacket::bungeecord_connect("");
        let data: Vec<u8> = empty.data.iter().map(|&b| b as u8).collect();
        assert_eq!(&data[9..], &[0x00, 0x00]);
    }

    #[test]
    fn full_encode_uses_correct_channel_per_era() {
        // modern: "bungeecord:main" (15 bytes), legacy: "BungeeCord" (10 bytes)
        let cases: &[(ProtocolVersion, u8, &[u8])] = &[
            (ProtocolVersion::V1_21, 15, b"bungeecord:main"),
            (ProtocolVersion::V1_12_2, 10, b"BungeeCord"),
        ];
        for &(version, chan_len, chan_bytes) in cases {
            let packet = PlayClientBoundPluginMessagePacket::bungeecord_connect("auth");
            let mut writer = BinaryWriter::default();
            packet.encode(&mut writer, version).unwrap();
            let bytes = writer.as_slice();
            assert_eq!(bytes[0], chan_len, "channel len for {version:?}");
            assert_eq!(
                &bytes[1..1 + chan_len as usize],
                chan_bytes,
                "channel for {version:?}"
            );
        }
    }

    #[test]
    fn brand_payload_encodes_minecraft_varint_prefixed_string() {
        let packet = PlayClientBoundPluginMessagePacket::brand("PicoLobby");
        let data: Vec<u8> = packet.data.iter().map(|&b| b as u8).collect();
        let mut expected = Vec::new();
        expected.push(9);
        expected.extend_from_slice(b"PicoLobby");
        assert_eq!(data, expected);
    }
}
