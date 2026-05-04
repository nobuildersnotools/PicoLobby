use crate::handlers::configuration::send_message;
use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::{Boundaries, ServerState};
use minecraft_packets::play::set_player_position_and_rotation_packet::SetPlayerPositionAndRotationPacket;
use minecraft_packets::play::synchronize_player_position_packet::SynchronizePlayerPositionPacket;

const FALL_SPEED: f64 = 3.8855;

impl PacketHandler for SetPlayerPositionAndRotationPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        Ok(teleport_player_to_spawn_out_of_bounds(
            client_state,
            server_state,
            (self.x, self.feet_y, self.z),
            Some((self.yaw, self.pitch)),
        ))
    }
}

pub fn teleport_player_to_spawn_out_of_bounds(
    client_state: &mut ClientState,
    server_state: &ServerState,
    position: (f64, f64, f64),
    rotation: Option<(f32, f32)>,
) -> Batch<PacketRegistry> {
    let mut batch = Batch::new();
    let previous_position = client_state.get_y_position();
    client_state.set_position(position);
    if let Some(rotation) = rotation {
        client_state.set_rotation(rotation);
    }
    server_state.update_lobby_position(client_state);

    if let Some(Boundaries {
        teleport_message,
        min_y,
    }) = server_state.boundaries()
    {
        let feet_y = position.1;

        if feet_y < f64::from(*min_y) {
            let difference = (previous_position - feet_y).abs();

            if previous_position >= f64::from(*min_y) && difference <= FALL_SPEED {
                teleport_player_to_spawn(client_state, server_state, &mut batch);

                if let Some(content) = teleport_message {
                    send_message(&mut batch, content, client_state.protocol_version());
                }
            }
        }
    }
    batch
}

pub fn teleport_player_to_spawn(
    client_state: &mut ClientState,
    server_state: &ServerState,
    batch: &mut Batch<PacketRegistry>,
) {
    let (x, y, z) = server_state.spawn_position();
    let (yaw, pitch) = server_state.spawn_rotation();
    let packet = SynchronizePlayerPositionPacket::new(x, y, z, yaw, pitch);
    batch.queue(|| PacketRegistry::SynchronizePlayerPosition(packet));

    client_state.set_position((x, y, z));
    client_state.set_rotation((yaw, pitch));
    server_state.update_lobby_position(client_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use minecraft_protocol::prelude::{ProtocolVersion, State};

    fn server_state_with_min_y(min_y: i32, message: Option<String>) -> ServerState {
        let mut builder = ServerState::builder();
        builder.spawn_position((0.0, 100.0, 0.0));
        if let Some(content) = message {
            builder.boundaries(min_y, content).unwrap();
        } else {
            builder.boundaries(min_y, "").unwrap();
        }

        builder.build().unwrap()
    }

    fn client_state() -> ClientState {
        let mut cs = ClientState::default();
        cs.set_protocol_version(ProtocolVersion::V1_20_5);
        cs.set_state(State::Play);
        cs
    }

    #[tokio::test]
    async fn test_should_teleport_and_message_player() {
        // Given
        let mut client_state = client_state();
        let server_state = server_state_with_min_y(0, Some("Direct teleport test".to_string()));

        // When
        let batch = teleport_player_to_spawn_out_of_bounds(
            &mut client_state,
            &server_state,
            (0.0, -1.0, 0.0),
            None,
        );
        let mut batch = batch.into_stream();

        // Then
        assert!(matches!(
            batch.next().await.unwrap(),
            PacketRegistry::SynchronizePlayerPosition(_)
        ));
        assert!(matches!(
            batch.next().await.unwrap(),
            PacketRegistry::SystemChatMessage(_) | PacketRegistry::LegacyChatMessage(_)
        ));
        assert!(batch.next().await.is_none());
    }

    #[tokio::test]
    async fn test_should_teleport_player() {
        // Given
        let mut client_state = client_state();
        let server_state = server_state_with_min_y(0, None);

        // When
        let batch = teleport_player_to_spawn_out_of_bounds(
            &mut client_state,
            &server_state,
            (0.0, -1.0, 0.0),
            None,
        );
        let mut batch = batch.into_stream();

        // Then
        assert!(matches!(
            batch.next().await.unwrap(),
            PacketRegistry::SynchronizePlayerPosition(_)
        ));
        assert!(batch.next().await.is_none());
    }

    #[tokio::test]
    async fn test_should_do_nothing() {
        // Given
        let mut client_state = client_state();
        let server_state = server_state_with_min_y(0, None);

        // When
        let batch = teleport_player_to_spawn_out_of_bounds(
            &mut client_state,
            &server_state,
            (0.0, 10.0, 0.0),
            None,
        );
        let mut batch = batch.into_stream();

        // Then
        assert!(batch.next().await.is_none());
    }

    #[tokio::test]
    async fn test_should_teleport_once_two_packets() {
        // Given
        const STARTING_POSITION: f64 = 1.0;
        let mut client_state = client_state();
        client_state.set_position((0.0, STARTING_POSITION, 0.0));
        let server_state = server_state_with_min_y(0, None);

        // When
        let mut stream1 = teleport_player_to_spawn_out_of_bounds(
            &mut client_state,
            &server_state,
            (0.0, STARTING_POSITION, 0.0),
            None,
        )
        .into_stream();

        let mut stream2 = teleport_player_to_spawn_out_of_bounds(
            &mut client_state,
            &server_state,
            (0.0, STARTING_POSITION - FALL_SPEED, 0.0),
            None,
        )
        .into_stream();

        let subsequent_streams = (2..=10).map(|i| {
            teleport_player_to_spawn_out_of_bounds(
                &mut client_state,
                &server_state,
                (
                    0.0,
                    FALL_SPEED.mul_add(-f64::from(i), STARTING_POSITION),
                    0.0,
                ),
                None,
            )
            .into_stream()
        });

        // Then
        assert!(
            stream1.next().await.is_none(),
            "First packet should do nothing"
        );

        assert!(
            matches!(
                stream2.next().await.unwrap(),
                PacketRegistry::SynchronizePlayerPosition(_)
            ),
            "Second packet should trigger a teleport"
        );
        assert!(stream2.next().await.is_none());

        for (i, mut stream) in subsequent_streams.enumerate() {
            assert!(
                stream.next().await.is_none(),
                "Subsequent packet #{} should not trigger another teleport",
                i + 3
            );
        }
    }
}
