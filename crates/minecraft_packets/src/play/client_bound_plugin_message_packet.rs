use minecraft_protocol::prelude::*;

#[derive(PacketOut)]
pub struct PlayClientBoundPluginMessagePacket {
    channel: Identifier,
    data: LengthPaddedVec<i8>,
}

impl PlayClientBoundPluginMessagePacket {
    pub fn brand(brand: impl ToString) -> Self {
        Self {
            channel: Identifier::vanilla_unchecked("brand"),
            data: LengthPaddedVec::new(
                brand
                    .to_string()
                    .as_bytes()
                    .iter()
                    .map(|&b| b as i8)
                    .collect::<Vec<_>>(),
            ),
        }
    }

    /// Constructs a BungeeCord `Connect` plugin message that instructs the Velocity
    /// proxy to move the player to the named downstream server.
    pub fn bungeecord_connect(server_name: &str) -> Self {
        Self {
            channel: Identifier::new_unchecked("bungeecord", "main"),
            data: LengthPaddedVec::new(bungeecord_payload("Connect", server_name)),
        }
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

        assert_eq!(packet.channel.namespace, "bungeecord");
        assert_eq!(packet.channel.thing, "main");

        let data: Vec<u8> = packet.data.inner().iter().map(|&b| b as u8).collect();
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
        let data: Vec<u8> = packet.data.inner().iter().map(|&b| b as u8).collect();
        // "Connect" header then 0x00 0x00 for empty name
        assert_eq!(&data[9..], &[0x00, 0x00]);
    }
}
