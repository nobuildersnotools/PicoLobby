use minecraft_protocol::prelude::*;

#[derive(PacketIn)]
pub struct SetPlayerRotationPacket {
    pub yaw: f32,
    pub pitch: f32,
    #[pvn(769..)]
    pub v1_21_4_flags: u8,
    #[pvn(..769)]
    pub on_ground: bool,
}
