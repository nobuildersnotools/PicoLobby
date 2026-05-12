use minecraft_protocol::prelude::*;

#[derive(PacketIn)]
pub struct ClientInformationPacket {
    locale: String,
    view_distance: i8,
    chat_mode: VarInt,
    chat_colors: bool,
    displayed_skin_parts: u8,
    main_hand: VarInt,
    #[pvn(755..)]
    text_filtering_enabled: bool,
    #[pvn(757..)]
    allows_server_listings: bool,
}

impl ClientInformationPacket {
    pub fn chat_mode(&self) -> i32 {
        self.chat_mode.inner()
    }

    #[allow(dead_code)]
    pub fn locale(&self) -> &str {
        &self.locale
    }

    #[allow(dead_code)]
    pub const fn view_distance(&self) -> i8 {
        self.view_distance
    }

    #[allow(dead_code)]
    pub const fn chat_colors(&self) -> bool {
        self.chat_colors
    }

    #[allow(dead_code)]
    pub const fn displayed_skin_parts(&self) -> u8 {
        self.displayed_skin_parts
    }

    #[allow(dead_code)]
    pub fn main_hand(&self) -> i32 {
        self.main_hand.inner()
    }

    #[allow(dead_code)]
    pub const fn text_filtering_enabled(&self) -> bool {
        self.text_filtering_enabled
    }

    #[allow(dead_code)]
    pub const fn allows_server_listings(&self) -> bool {
        self.allows_server_listings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(data: &[u8], version: ProtocolVersion) -> ClientInformationPacket {
        let mut reader = BinaryReader::new(data);
        ClientInformationPacket::decode(&mut reader, version).unwrap()
    }

    #[test]
    fn decodes_chat_mode() {
        let packet = decode(
            &[5, b'e', b'n', b'_', b'u', b's', 10, 2, 1, 0x7f, 1, 0, 1],
            ProtocolVersion::V1_20_5,
        );

        assert_eq!(packet.locale(), "en_us");
        assert_eq!(packet.view_distance(), 10);
        assert_eq!(packet.chat_mode(), 2);
        assert!(packet.chat_colors());
        assert_eq!(packet.displayed_skin_parts(), 0x7f);
        assert_eq!(packet.main_hand(), 1);
        assert!(!packet.text_filtering_enabled());
        assert!(packet.allows_server_listings());
    }

    #[test]
    fn older_versions_omit_filtering_and_listings() {
        let packet = decode(
            &[5, b'e', b'n', b'_', b'u', b's', 8, 1, 1, 0x7f, 1],
            ProtocolVersion::V1_16_4,
        );

        assert_eq!(packet.chat_mode(), 1);
        assert!(!packet.text_filtering_enabled());
        assert!(!packet.allows_server_listings());
    }
}
