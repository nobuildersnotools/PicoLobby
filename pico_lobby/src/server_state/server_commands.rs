use crate::configuration::commands::CommandsConfig;

#[derive(Default)]
pub struct ServerCommands {
    spawn: String,
    fly: String,
    fly_speed: String,
    transfer: String,
    server: String,
    msg: String,
    reply: String,
    reply_aliases: Vec<String>,
}

impl From<CommandsConfig> for ServerCommands {
    fn from(config: CommandsConfig) -> Self {
        Self {
            spawn: config.spawn,
            fly: config.fly,
            fly_speed: config.fly_speed,
            transfer: config.transfer,
            server: config.server,
            msg: config.msg,
            reply: config.reply,
            reply_aliases: config.reply_aliases,
        }
    }
}

pub enum ServerCommand {
    Disabled,
    Enabled { alias: String },
}

impl ServerCommands {
    pub fn spawn(&self) -> ServerCommand {
        Self::server_command(self.spawn.clone())
    }

    pub fn fly(&self) -> ServerCommand {
        Self::server_command(self.fly.clone())
    }

    pub fn fly_speed(&self) -> ServerCommand {
        Self::server_command(self.fly_speed.clone())
    }

    pub fn transfer(&self) -> ServerCommand {
        Self::server_command(self.transfer.clone())
    }

    pub fn server(&self) -> ServerCommand {
        Self::server_command(self.server.clone())
    }

    pub fn msg(&self) -> ServerCommand {
        Self::server_command(self.msg.clone())
    }

    pub fn reply(&self) -> ServerCommand {
        Self::server_command(self.reply.clone())
    }

    pub fn reply_aliases(&self) -> Vec<ServerCommand> {
        self.reply_aliases
            .iter()
            .map(|alias| Self::server_command(alias.clone()))
            .collect()
    }

    fn server_command(alias: String) -> ServerCommand {
        if alias.is_empty() {
            ServerCommand::Disabled
        } else {
            ServerCommand::Enabled { alias }
        }
    }
}
