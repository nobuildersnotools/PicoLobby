# Commands

Representing the `[commands]` section in `server.toml`.

## Spawn Command

The `/spawn` command teleports the player to the server's spawn location. You can customize the command alias or disable it entirely.

:::code-group
```toml [server.toml] {2}
[commands]
spawn = "spawn"
```
:::

## Fly Command

The `/fly` command allows players to toggle flight on and off. This command is not affected by the Allow Flight setting.

:::code-group
```toml [server.toml] {2}
[commands]
fly = "fly"
```
:::

## Fly Speed Command

The `/flyspeed` command allows players to adjust their flight speed with a `speed` float argument. The speed value must be between `0.0` and `1.0`.

:::code-group
```toml [server.toml] {2}
[commands]
fly_speed = "flyspeed"
```
:::

## Transfer Command <Badge type="warning" text="1.20.5+" />

The `/transfer` command allows players to transfer to another server by specifying its `hostname` and optionally a `port`. If a port is not specified the Minecraft default of 25565 is used. 

> [!NOTE]
> The destination server must have [accepts-transfers](https://minecraft.wiki/w/Server.properties#Keys) set to `true` in its server.properties.

:::code-group
```toml [server.toml] {2}
[commands]
transfer = "transfer"
```
:::

## Server Command

The `/server` command sends a player to a configured lobby destination through the proxy plugin message flow.

:::code-group
```toml [server.toml] {2}
[commands]
server = "server"
```
:::

## Private Message Commands

The `/msg <player> <message>` command sends a private message to one online player in the lobby. Player matching is a full username match and ignores case. `/reply <message>` replies to the last private-message peer in either direction, and `/r <message>` is the default reply alias.

Private messages are lobby-only. Recipients with Full or Commands Only chat visibility can receive them; recipients with Hidden Chat cannot.

:::code-group
```toml [server.toml]
[commands]
msg = "msg"
reply = "reply"
reply_aliases = ["r"]
```
:::

Use an empty alias list to disable `/r` and any other reply aliases.

:::code-group
```toml [server.toml]
[commands]
reply_aliases = []
```
:::

Private-message output and feedback are configured under `[lobby.private_messages]`. The format fields support `{sender}`, `{recipient}`, `{target}`, and `{message}` placeholders where applicable. Player-provided text is escaped before MiniMessage parsing.

:::code-group
```toml [server.toml]
[lobby.private_messages]
sender_format = "<gray>[me -> {recipient}]</gray> <white>{message}</white>"
recipient_format = "<gray>[{sender} -> me]</gray> <white>{message}</white>"
unknown_target = "<red>Player '{target}' is not online in the lobby.</red>"
ambiguous_target = "<red>More than one online player matches '{target}'.</red>"
hidden_target = "<red>{target} cannot receive private messages with hidden chat.</red>"
missing_reply_target = "<red>You have nobody to reply to.</red>"
self_message = "<red>You cannot send a private message to yourself.</red>"
empty_message = "<red>Private message cannot be empty.</red>"
too_long = "<red>Private message is too long.</red>"
rate_limit = "<red>You are sending messages too quickly.</red>"
unavailable = "<red>Private messages are only available in the lobby.</red>"
```
:::

## Disabling Commands

Any command can be disabled by setting its value to an empty string `""`. This prevents players from using that command entirely.

:::code-group
```toml [server.toml] {2}
[commands]
spawn = ""
fly = "fly"
fly_speed = ""
transfer = ""
server = ""
msg = ""
reply = ""
```
:::

## Renaming Commands

You can rename any command to a custom alias by changing its value. For example, you could rename multiple commands for your server's theme or language preferences.

:::code-group
```toml [server.toml] {2}
[commands]
spawn = "home"
fly = "soar"
fly_speed = "speed"
transfer = "server"
server = "join"
msg = "tell"
reply = "respond"
```
:::
