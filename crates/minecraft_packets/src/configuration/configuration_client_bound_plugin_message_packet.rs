use minecraft_protocol::prelude::*;

#[derive(PacketOut)]
pub struct ConfigurationClientBoundPluginMessagePacket {
    channel: Identifier,
    data: Vec<i8>,
}

impl ConfigurationClientBoundPluginMessagePacket {
    pub fn brand(brand: impl ToString) -> Self {
        Self {
            channel: Identifier::vanilla_unchecked("brand"),
            data: minecraft_string_payload(&brand.to_string()),
        }
    }
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
