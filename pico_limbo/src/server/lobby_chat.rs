use crate::server::packet_registry::PacketRegistry;
use crate::server_state::{LobbyChatPlan, LobbyRecipient};
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

fn format_lobby_chat(sender: &str, message: &str, format: &str) -> Component {
    let template = format
        .replace("{sender}", &escape_minimessage_text(sender))
        .replace("{message}", &escape_minimessage_text(message));
    parse_mini_message(&template).unwrap_or_else(|_| Component::new(format!("{sender}: {message}")))
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
    fn selects_legacy_chat_before_1_19() {
        let packet = chat_packet_for_version(ProtocolVersion::V1_18_2, &Component::new("hello"));

        assert!(matches!(packet, PacketRegistry::LegacyChatMessage(_)));
    }

    #[test]
    fn selects_system_chat_from_1_19() {
        let packet = chat_packet_for_version(ProtocolVersion::V1_19, &Component::new("hello"));

        assert!(matches!(packet, PacketRegistry::SystemChatMessage(_)));
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
}
