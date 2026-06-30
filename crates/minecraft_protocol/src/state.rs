use std::fmt;
use std::fmt::{Display, Formatter};

#[derive(Copy, Debug, PartialEq, Clone, Default, Eq, Hash)]
pub enum State {
    #[default]
    Handshake,
    Status,
    Login,
    Configuration,
    Play,
    Transfer,
}

impl Display for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Handshake => f.write_str("handshake"),
            Self::Status => f.write_str("status"),
            Self::Login => f.write_str("login"),
            Self::Configuration => f.write_str("configuration"),
            Self::Play => f.write_str("play"),
            Self::Transfer => f.write_str("transfer"),
        }
    }
}
