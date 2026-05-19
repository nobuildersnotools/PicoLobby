use crate::server::packet_registry::PacketRegistry;
use crate::server_state::{
    LobbyChatPlan, LobbyLifecycleMessagePlan, LobbyPrivateMessagePlan, LobbyRecipient,
};
use minecraft_packets::play::legacy_chat_message_packet::LegacyChatMessagePacket;
use minecraft_packets::play::system_chat_message_packet::SystemChatMessagePacket;
use minecraft_protocol::prelude::ProtocolVersion;
use pico_text_component::prelude::{Component, parse_mini_message};

pub const MAX_CHAT_MESSAGE_CHARS: usize = 256;

pub fn chat_packets_for_plan(plan: &LobbyChatPlan) -> Vec<(LobbyRecipient, PacketRegistry)> {
    let component = format_lobby_chat(&plan.sender_username, &plan.message, &plan.format);
    plan.recipients
        .iter()
        .cloned()
        .map(|recipient| {
            let packet = chat_packet_for_version(recipient.protocol_version, &component);
            (recipient, packet)
        })
        .collect()
}

pub fn lifecycle_message_packets_for_plan(
    plan: &LobbyLifecycleMessagePlan,
) -> Vec<(LobbyRecipient, PacketRegistry)> {
    let component = format_lobby_lifecycle_message(&plan.player_username, &plan.template);
    plan.recipients
        .iter()
        .cloned()
        .map(|recipient| {
            let packet = chat_packet_for_version(recipient.protocol_version, &component);
            (recipient, packet)
        })
        .collect()
}

pub fn private_message_packets_for_plan(
    plan: &LobbyPrivateMessagePlan,
) -> Vec<(LobbyRecipient, PacketRegistry)> {
    let sender_component = format_private_message_for_sender(plan);
    let recipient_component = format_private_message_for_recipient(plan);
    vec![
        (
            plan.sender_recipient.clone(),
            chat_packet_for_version(plan.sender_recipient.protocol_version, &sender_component),
        ),
        (
            plan.message_recipient.clone(),
            chat_packet_for_version(
                plan.message_recipient.protocol_version,
                &recipient_component,
            ),
        ),
    ]
}

pub fn chat_packet_for_version(version: ProtocolVersion, component: &Component) -> PacketRegistry {
    if version.is_after_inclusive(ProtocolVersion::V1_19) {
        PacketRegistry::SystemChatMessage(SystemChatMessagePacket::component(component))
    } else {
        PacketRegistry::LegacyChatMessage(LegacyChatMessagePacket::system(component))
    }
}

pub fn chat_feedback_packet(version: ProtocolVersion, message: &str) -> PacketRegistry {
    let component = feedback_component(message);
    chat_packet_for_version(version, &component)
}

pub fn plain_chat_feedback_packet(version: ProtocolVersion, message: &str) -> PacketRegistry {
    let component = Component::new(message);
    chat_packet_for_version(version, &component)
}

fn feedback_component(message: &str) -> Component {
    parse_mini_message(message).unwrap_or_else(|_| Component::new(message))
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

#[allow(clippy::literal_string_with_formatting_args)]
fn format_private_message_for_sender(plan: &LobbyPrivateMessagePlan) -> Component {
    let template = plan
        .sender_format
        .replace("{sender}", &escape_minimessage_text(&plan.sender_username))
        .replace(
            "{recipient}",
            &escape_minimessage_text(&plan.recipient_username),
        )
        .replace(
            "{target}",
            &escape_minimessage_text(&plan.recipient_username),
        )
        .replace("{message}", &escape_minimessage_text(&plan.message));
    parse_mini_message(&template).unwrap_or_else(|_| {
        Component::new(format!("To {}: {}", plan.recipient_username, plan.message))
    })
}

#[allow(clippy::literal_string_with_formatting_args)]
fn format_private_message_for_recipient(plan: &LobbyPrivateMessagePlan) -> Component {
    let template = plan
        .recipient_format
        .replace("{sender}", &escape_minimessage_text(&plan.sender_username))
        .replace(
            "{recipient}",
            &escape_minimessage_text(&plan.recipient_username),
        )
        .replace(
            "{target}",
            &escape_minimessage_text(&plan.recipient_username),
        )
        .replace("{message}", &escape_minimessage_text(&plan.message));
    parse_mini_message(&template).unwrap_or_else(|_| {
        Component::new(format!("From {}: {}", plan.sender_username, plan.message))
    })
}

pub fn escape_minimessage_text(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_state::{EntityId, LobbySessionId};
    use minecraft_protocol::prelude::Uuid;

    fn private_message_plan() -> LobbyPrivateMessagePlan {
        LobbyPrivateMessagePlan {
            sender_session_id: LobbySessionId::new(1),
            recipient_session_id: LobbySessionId::new(2),
            sender_username: "Sender".to_string(),
            recipient_username: "Recipient".to_string(),
            message: "hello".to_string(),
            sender_format: "<gray>[me -> {recipient}]</gray> <white>{message}</white>".to_string(),
            recipient_format: "<gray>[{sender} -> me]</gray> <white>{message}</white>".to_string(),
            sender_recipient: LobbyRecipient {
                session_id: LobbySessionId::new(1),
                uuid: Uuid::from_u128(1),
                entity_id: EntityId::new(1),
                protocol_version: ProtocolVersion::V1_18_2,
            },
            message_recipient: LobbyRecipient {
                session_id: LobbySessionId::new(2),
                uuid: Uuid::from_u128(2),
                entity_id: EntityId::new(2),
                protocol_version: ProtocolVersion::V1_19,
            },
        }
    }

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
    fn private_message_views_contain_expected_names_and_message() {
        let plan = private_message_plan();

        let sender_json = format_private_message_for_sender(&plan).to_json();
        let recipient_json = format_private_message_for_recipient(&plan).to_json();

        assert!(sender_json.contains("Recipient"));
        assert!(sender_json.contains("hello"));
        assert!(recipient_json.contains("Sender"));
        assert!(recipient_json.contains("hello"));
    }

    #[test]
    fn private_message_user_text_does_not_become_minimessage_markup() {
        let mut plan = private_message_plan();
        plan.sender_username = "<red>Sender</red>".to_string();
        plan.message = "<red>hello</red>".to_string();

        let sender_json = format_private_message_for_sender(&plan).to_json();
        let recipient_json = format_private_message_for_recipient(&plan).to_json();

        assert!(!sender_json.contains("\"color\":\"red\""));
        assert!(!recipient_json.contains("\"color\":\"red\""));
    }

    #[test]
    fn private_message_packets_use_recipient_versions() {
        let packets = private_message_packets_for_plan(&private_message_plan());

        assert!(matches!(packets[0].1, PacketRegistry::LegacyChatMessage(_)));
        assert!(matches!(packets[1].1, PacketRegistry::SystemChatMessage(_)));
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

    #[test]
    fn feedback_message_supports_minimessage() {
        let component = feedback_component("<red>Slow down.</red>");
        let json = component.to_json();

        assert!(json.contains("Slow down."));
        assert!(json.contains("\"color\":\"red\""));
        assert!(!json.contains("<red>"));
    }
}
