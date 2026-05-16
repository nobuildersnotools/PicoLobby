use crate::server::packet_registry::PacketRegistry;
use crate::server_state::{LobbyChatPlan, LobbyLifecycleMessagePlan, LobbyRecipient};
use minecraft_packets::play::legacy_chat_message_packet::LegacyChatMessagePacket;
use minecraft_packets::play::system_chat_message_packet::SystemChatMessagePacket;
use minecraft_protocol::prelude::ProtocolVersion;
use pico_text_component::prelude::{Component, parse_mini_message};

pub const MAX_CHAT_MESSAGE_CHARS: usize = 256;

pub fn chat_packets_for_plan(plan: &LobbyChatPlan) -> Vec<(LobbyRecipient, PacketRegistry)> {
    plan.recipients
        .iter()
        .cloned()
        .map(|recipient| {
            let component = format_lobby_chat(&plan.sender_username, &plan.message, &plan.format);
            let packet = chat_packet_for_version(recipient.protocol_version, &component);
            (recipient, packet)
        })
        .collect()
}

pub fn lifecycle_message_packets_for_plan(
    plan: &LobbyLifecycleMessagePlan,
) -> Vec<(LobbyRecipient, PacketRegistry)> {
    plan.recipients
        .iter()
        .cloned()
        .map(|recipient| {
            let component = format_lobby_lifecycle_message(&plan.player_username, &plan.template);
            let packet = chat_packet_for_version(recipient.protocol_version, &component);
            (recipient, packet)
        })
        .collect()
}

pub fn chat_packet_for_version(version: ProtocolVersion, component: &Component) -> PacketRegistry {
    if version.is_after_inclusive(ProtocolVersion::V1_19) {
        PacketRegistry::SystemChatMessage(SystemChatMessagePacket::component(component))
    } else {
        PacketRegistry::LegacyChatMessage(LegacyChatMessagePacket::system(component))
    }
}

pub fn chat_feedback_packet(version: ProtocolVersion, message: &str) -> PacketRegistry {
    let component = Component::new(message);
    chat_packet_for_version(version, &component)
}

#[allow(clippy::literal_string_with_formatting_args)]
fn format_lobby_chat(sender: &str, message: &str, format: &str) -> Component {
    let template = format
        .replace("{sender}", &escape_minimessage_text(sender))
        .replace("{message}", &escape_minimessage_text(message));
    parse_mini_message(&template).unwrap_or_else(|_| Component::new(format!("{sender}: {message}")))
}

fn format_lobby_lifecycle_message(player: &str, template: &str) -> Component {
    #[allow(clippy::literal_string_with_formatting_args)]
    let template = template.replace("{player}", &escape_minimessage_text(player));
    parse_mini_message(&template).unwrap_or_else(|_| Component::new(player))
}

fn escape_minimessage_text(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_packet_version_dispatch() {
        let msg = Component::new("hello");
        assert!(matches!(
            chat_packet_for_version(ProtocolVersion::V1_18_2, &msg),
            PacketRegistry::LegacyChatMessage(_)
        ));
        assert!(matches!(
            chat_packet_for_version(ProtocolVersion::V1_19, &msg),
            PacketRegistry::SystemChatMessage(_)
        ));
    }

    #[test]
    fn sender_name_appears_in_chat_output() {
        let component = format_lobby_chat(
            "Steve",
            "hello",
            "<white>&lt;{sender}&gt; {message}</white>",
        );
        let json = component.to_json();

        assert!(json.contains("Steve"), "sender name must appear in output");
        assert!(json.contains("hello"), "message must appear in output");
    }

    #[test]
    fn user_text_does_not_become_minimessage_markup() {
        let component = format_lobby_chat(
            "sender",
            "<red>hello</red>",
            "<white>&lt;{sender}&gt; {message}</white>",
        );
        let json = component.to_json();

        assert!(!json.contains("\"color\":\"red\""));
    }

    #[test]
    fn lifecycle_player_name_does_not_become_minimessage_markup() {
        let component = format_lobby_lifecycle_message(
            "<red>Steve</red>",
            "<yellow>{player} joined the game</yellow>",
        );
        let json = component.to_json();

        assert!(json.contains("Steve"));
        assert!(!json.contains("\"color\":\"red\""));
    }
}
