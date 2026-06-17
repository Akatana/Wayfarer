use crate::command::ClientId;
use crate::components::{ClientConnection, Name, Position};
use crate::direction::Direction;
use crate::game_state::GameState;
use crate::systems::{
    movement,
    output::{send_to_client, OutputRegistry},
};

pub(super) fn handle_move(
    state: &mut GameState,
    client_id: ClientId,
    direction: Direction,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };

    let old_room_id = {
        let Ok(pos) = state.world.get::<&Position>(entity) else {
            return;
        };
        pos.room_id
    };
    let player_name = state
        .world
        .get::<&Name>(entity)
        .map(|n| n.0.clone())
        .unwrap_or_default();

    let new_pos = movement::try_move(&state.world, &state.room_registry, entity, direction);

    match new_pos {
        Some(pos) => {
            let new_room_id = pos.room_id;

            let old_occupants: Vec<ClientId> = {
                let mut q = state.world.query::<(&Position, &ClientConnection)>();
                q.iter()
                    .filter(|(e, (p, _))| *e != entity && p.room_id == old_room_id)
                    .map(|(_, (_, conn))| conn.client_id)
                    .collect()
            };

            if let Ok(mut current) = state.world.get::<&mut Position>(entity) {
                *current = pos;
            }

            let new_occupants: Vec<ClientId> = {
                let mut q = state.world.query::<(&Position, &ClientConnection)>();
                q.iter()
                    .filter(|(e, (p, _))| *e != entity && p.room_id == new_room_id)
                    .map(|(_, (_, conn))| conn.client_id)
                    .collect()
            };

            for id in old_occupants {
                send_to_client(
                    registry,
                    id,
                    format!("{} leaves {}.", player_name, direction),
                );
            }
            let from = direction.opposite();
            for id in new_occupants {
                send_to_client(
                    registry,
                    id,
                    format!("{} arrives from the {}.", player_name, from),
                );
            }

            if let Some(desc) = super::describe_room(state, new_room_id) {
                send_to_client(
                    registry,
                    client_id,
                    format!("You head {}.\n\n{}", direction, desc),
                );
            }
        }
        None => {
            send_to_client(
                registry,
                client_id,
                "There's no exit in that direction.".to_string(),
            );
        }
    }
}
