use tokio::sync::mpsc;

use crate::character::CharacterData;
use crate::command::{ClientId, Command, PlayerInput};
use crate::components::{AdminFlag, ClientConnection, Name, Position};
use crate::direction::Direction;
use crate::game_state::GameState;
use crate::systems::{
    movement,
    output::{send_to_client, OutputRegistry},
};

/// Phase 1 of each tick: drains every pending command from the network channel
/// without blocking. No `.await` calls are permitted here.
pub fn process_input(
    state: &mut GameState,
    command_rx: &mut mpsc::Receiver<PlayerInput>,
    output_registry: &OutputRegistry,
) {
    while let Ok(input) = command_rx.try_recv() {
        dispatch(state, input, output_registry);
    }
}

fn dispatch(state: &mut GameState, input: PlayerInput, registry: &OutputRegistry) {
    match input.command {
        Command::Connect(data) => handle_connect(state, input.client_id, data, registry),
        Command::Look => handle_look(state, input.client_id, registry),
        Command::Move(dir) => handle_move(state, input.client_id, dir, registry),
        Command::Say(msg) => handle_say(state, input.client_id, &msg, registry),
        Command::Quit => handle_quit(state, input.client_id, registry),
        Command::AdminWho => handle_admin_who(state, input.client_id, registry),
        Command::Unknown(raw) => handle_unknown(input.client_id, &raw, registry),
    }
}

fn handle_connect(
    state: &mut GameState,
    client_id: ClientId,
    data: CharacterData,
    registry: &OutputRegistry,
) {
    if state.player_registry.is_connected(client_id) {
        return; // duplicate Connect — ignore
    }
    let entity = state.spawn_player_from_data(client_id, &data);
    let room_id = { state.world.get::<&Position>(entity).ok().map(|p| p.room_id) };

    if let Some(room) = room_id.and_then(|id| state.room_registry.get(id)) {
        let welcome = format!("Welcome, {}!\n\n{}", data.name, room.describe());
        send_to_client(registry, client_id, welcome);
    }
}

fn handle_look(state: &GameState, client_id: ClientId, registry: &OutputRegistry) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };

    let room_id = {
        let Ok(pos) = state.world.get::<&Position>(entity) else {
            return;
        };
        pos.room_id
    };

    if let Some(room) = state.room_registry.get(room_id) {
        send_to_client(registry, client_id, room.describe());
    }
}

fn handle_move(
    state: &mut GameState,
    client_id: ClientId,
    direction: Direction,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };

    let new_pos = movement::try_move(&state.world, &state.room_registry, entity, direction);

    match new_pos {
        Some(pos) => {
            let new_room_id = pos.room_id;
            if let Ok(mut current) = state.world.get::<&mut Position>(entity) {
                *current = pos;
            }
            if let Some(room) = state.room_registry.get(new_room_id) {
                send_to_client(
                    registry,
                    client_id,
                    format!("You head {}.\n\n{}", direction, room.describe()),
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

fn handle_say(state: &GameState, client_id: ClientId, message: &str, registry: &OutputRegistry) {
    let Some(sender_entity) = state.player_registry.get_entity(client_id) else {
        return;
    };

    let sender_room_id = {
        let Ok(pos) = state.world.get::<&Position>(sender_entity) else {
            return;
        };
        pos.room_id
    };

    let recipients: Vec<ClientId> = {
        let mut q = state.world.query::<(&Position, &ClientConnection)>();
        q.iter()
            .filter(|(_, (pos, _))| pos.room_id == sender_room_id)
            .map(|(_, (_, conn))| conn.client_id)
            .collect()
    };

    for recipient_id in recipients {
        let msg = if recipient_id == client_id {
            format!("You say: \"{}\"", message)
        } else {
            format!("Someone says: \"{}\"", message)
        };
        send_to_client(registry, recipient_id, msg);
    }
}

fn handle_quit(state: &mut GameState, client_id: ClientId, registry: &OutputRegistry) {
    send_to_client(
        registry,
        client_id,
        "Farewell. The world fades around you...".to_string(),
    );

    if let Some(entity) = state.player_registry.remove(client_id) {
        if let Some(save_data) = state.extract_save_data(entity) {
            state.pending_saves.push(save_data);
        }
        state.world.despawn(entity).ok();
    }
}

fn handle_admin_who(state: &GameState, client_id: ClientId, registry: &OutputRegistry) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if state.world.get::<&AdminFlag>(entity).is_err() {
        send_to_client(
            registry,
            client_id,
            "You don't have that power.".to_string(),
        );
        return;
    }
    let mut lines = vec!["<yellow>=== Online Players ===</yellow>".to_string()];
    let mut q = state.world.query::<(&Name, &ClientConnection)>();
    for (_, (name, _)) in q.iter() {
        lines.push(format!("  {}", name.0));
    }
    if lines.len() == 1 {
        lines.push("  (nobody online)".to_string());
    }
    send_to_client(registry, client_id, lines.join("\n"));
}

fn handle_unknown(client_id: ClientId, raw: &str, registry: &OutputRegistry) {
    send_to_client(
        registry,
        client_id,
        format!("Huh? '{}' isn't something I understand.", raw),
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::CharacterData;
    use crate::command::Command;
    use crate::direction::Direction;
    use crate::game_state::GameState;
    use crate::systems::output::OutputRegistry;
    use tokio::sync::mpsc;

    fn setup() -> (
        GameState,
        mpsc::Sender<PlayerInput>,
        mpsc::Receiver<PlayerInput>,
        OutputRegistry,
        mpsc::Receiver<String>,
    ) {
        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        let (out_tx, out_rx) = mpsc::channel(32);
        let mut registry = OutputRegistry::new();
        registry.insert(1, out_tx.clone());
        registry.insert(2, out_tx);
        (GameState::new(), cmd_tx, cmd_rx, registry, out_rx)
    }

    fn connect(id: u64) -> PlayerInput {
        PlayerInput::new(id, Command::Connect(CharacterData::default()))
    }

    fn drain(rx: &mut mpsc::Receiver<String>) -> Vec<String> {
        let mut v = Vec::new();
        while let Ok(m) = rx.try_recv() {
            v.push(m);
        }
        v
    }

    #[test]
    fn drains_all_pending_commands() {
        let (mut state, tx, mut rx, reg, _) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Look)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn empty_channel_does_not_panic() {
        let (mut state, _tx, mut rx, reg, _) = setup();
        process_input(&mut state, &mut rx, &reg);
    }

    #[test]
    fn does_not_consume_messages_sent_after_drain() {
        let (mut state, tx, mut rx, reg, _) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        tx.try_send(PlayerInput::new(1, Command::Quit)).unwrap();
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn connect_spawns_player_in_world() {
        let (mut state, tx, mut rx, reg, _) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        assert!(state.player_registry.is_connected(1));
    }

    #[test]
    fn look_sends_room_description() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Look)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("Town Square")));
    }

    #[test]
    fn move_valid_exit_updates_entity_position() {
        let (mut state, tx, mut rx, reg, _) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Move(Direction::North)))
            .unwrap();
        process_input(&mut state, &mut rx, &reg);
        let entity = state.player_registry.get_entity(1).unwrap();
        let pos = state.world.get::<&Position>(entity).unwrap();
        assert_eq!(pos.room_id, 2);
    }

    #[test]
    fn move_blocked_exit_sends_error_message() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Move(Direction::Down)))
            .unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("no exit")));
    }

    #[test]
    fn say_delivers_to_sender_and_recipient_in_same_room() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(connect(2)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Say("hi".to_string())))
            .unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("You say")));
        assert!(msgs.iter().any(|m| m.contains("Someone says")));
    }

    #[test]
    fn quit_removes_player_and_queues_save() {
        let (mut state, tx, mut rx, reg, _) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        assert_eq!(state.world.len(), 1);

        tx.try_send(PlayerInput::new(1, Command::Quit)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        assert!(!state.player_registry.is_connected(1));
        assert_eq!(state.world.len(), 0);
        assert_eq!(state.pending_saves.len(), 1);
    }

    #[test]
    fn unknown_command_sends_error_to_client() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Unknown("xyzzy".to_string())))
            .unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("xyzzy")));
    }

    #[test]
    fn admin_who_denied_for_regular_player() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::AdminWho)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs
            .iter()
            .any(|m| m.contains("power") || m.contains("permission")));
    }

    #[test]
    fn admin_who_lists_players_for_admin() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        let admin_data = CharacterData {
            is_admin: true,
            name: "Admin".to_string(),
            ..Default::default()
        };
        tx.try_send(PlayerInput::new(1, Command::Connect(admin_data)))
            .unwrap();
        tx.try_send(PlayerInput::new(1, Command::AdminWho)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("Online Players")));
    }
}
