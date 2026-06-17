/// Integration tests exercise the public API of multiple modules together,
/// verifying end-to-end behaviour without mocking internal collaborators.
use std::collections::HashMap;

use tokio::sync::mpsc;

use wayfarer::character::CharacterData;
use wayfarer::command::{Command, PlayerInput};
use wayfarer::components::{NpcRoutine, Position};
use wayfarer::direction::Direction;
use wayfarer::game_state::GameState;
use wayfarer::network::parser;
use wayfarer::systems::input::process_input;
use wayfarer::systems::npc_routine::{npc_routine_system, NPC_ROUTINE_INTERVAL_TICKS};
use wayfarer::systems::output::{send_to_client, OutputRegistry};
use wayfarer::world::seed::STARTING_ROOM_ID;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Set up channels for one or two clients. Returns cmd_tx, cmd_rx, registry, and
/// a per-client map of `ClientId → Receiver<String>` so tests can check output.
fn test_setup(
    client_ids: &[u64],
) -> (
    mpsc::Sender<PlayerInput>,
    mpsc::Receiver<PlayerInput>,
    OutputRegistry,
    HashMap<u64, mpsc::Receiver<String>>,
) {
    let (cmd_tx, cmd_rx) = mpsc::channel(64);
    let mut registry = OutputRegistry::new();
    let mut receivers = HashMap::new();
    for &id in client_ids {
        let (tx, rx) = mpsc::channel(64);
        registry.insert(id, tx);
        receivers.insert(id, rx);
    }
    (cmd_tx, cmd_rx, registry, receivers)
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

// ── GameState lifecycle ───────────────────────────────────────────────────────

#[test]
fn game_state_tick_increments_on_each_advance() {
    let mut state = GameState::new();
    for expected in 1u64..=10 {
        state.advance_tick();
        assert_eq!(state.current_tick, expected);
    }
}

// ── Direction round-trip ──────────────────────────────────────────────────────

#[test]
fn every_direction_survives_opposite_round_trip() {
    let dirs = [
        Direction::North,
        Direction::South,
        Direction::East,
        Direction::West,
        Direction::NorthEast,
        Direction::NorthWest,
        Direction::SouthEast,
        Direction::SouthWest,
        Direction::Up,
        Direction::Down,
    ];
    for dir in dirs {
        assert_eq!(
            dir,
            dir.opposite().opposite(),
            "{dir} failed double-opposite"
        );
        assert_ne!(dir, dir.opposite(), "{dir} must not be its own opposite");
    }
}

// ── NPC routine via GameState ─────────────────────────────────────────────────

#[test]
fn npc_spawned_in_world_triggers_after_interval_ticks() {
    let mut state = GameState::new();
    state.world.spawn((
        NpcRoutine {
            last_action_tick: 0,
        },
        Position { room_id: 1 },
    ));

    for _ in 0..NPC_ROUTINE_INTERVAL_TICKS - 1 {
        state.advance_tick();
    }
    npc_routine_system(&mut state.world, state.current_tick, &mut Vec::new());
    for (_, r) in state.world.query_mut::<&NpcRoutine>() {
        assert_eq!(r.last_action_tick, 0, "Should not have fired yet");
    }

    state.advance_tick();
    npc_routine_system(&mut state.world, state.current_tick, &mut Vec::new());
    for (_, r) in state.world.query_mut::<&NpcRoutine>() {
        assert_eq!(r.last_action_tick, NPC_ROUTINE_INTERVAL_TICKS);
    }
}

#[test]
fn ten_npcs_all_trigger_at_the_interval() {
    let mut state = GameState::new();
    let entities: Vec<_> = (0..10)
        .map(|i| {
            state.world.spawn((
                NpcRoutine {
                    last_action_tick: 0,
                },
                Position { room_id: i },
            ))
        })
        .collect();

    npc_routine_system(
        &mut state.world,
        NPC_ROUTINE_INTERVAL_TICKS,
        &mut Vec::new(),
    );

    for e in entities {
        let r = state.world.get::<&NpcRoutine>(e).unwrap();
        assert_eq!(r.last_action_tick, NPC_ROUTINE_INTERVAL_TICKS);
    }
}

// ── Output registry ───────────────────────────────────────────────────────────

#[test]
fn output_messages_arrive_in_send_order() {
    let (tx, mut rx) = mpsc::channel(32);
    let mut reg = OutputRegistry::new();
    reg.insert(1, tx);
    for tick in [10u64, 20, 30] {
        send_to_client(&reg, 1, format!("tick {tick}"));
    }
    let msgs: Vec<String> = (0..3).map(|_| rx.try_recv().unwrap()).collect();
    assert_eq!(msgs, ["tick 10", "tick 20", "tick 30"]);
}

// ── Command parser ────────────────────────────────────────────────────────────

#[test]
fn parser_handles_all_direction_aliases() {
    assert_eq!(parser::parse("n"), Command::Move(Direction::North));
    assert_eq!(parser::parse("north"), Command::Move(Direction::North));
    assert_eq!(
        parser::parse("northeast"),
        Command::Move(Direction::NorthEast)
    );
    assert_eq!(parser::parse("sw"), Command::Move(Direction::SouthWest));
    assert_eq!(parser::parse("u"), Command::Move(Direction::Up));
}

#[test]
fn parser_produces_unknown_for_gibberish() {
    assert_eq!(
        parser::parse("xyzzy"),
        Command::Unknown("xyzzy".to_string())
    );
}

// ── Room registry ─────────────────────────────────────────────────────────────

#[test]
fn seed_rooms_are_reachable_from_starting_room() {
    let state = GameState::new();
    let reg = &state.room_registry;
    assert!(reg
        .resolve_exit(STARTING_ROOM_ID, Direction::North)
        .is_some());
    assert!(reg
        .resolve_exit(STARTING_ROOM_ID, Direction::East)
        .is_some());
    assert!(reg
        .resolve_exit(STARTING_ROOM_ID, Direction::South)
        .is_none());
}

// ── Player spawn ──────────────────────────────────────────────────────────────

#[test]
fn spawn_player_from_data_uses_character_room() {
    let mut state = GameState::new();
    let data = CharacterData {
        name: "Hero".to_string(),
        room_id: 3,
        ..Default::default()
    };
    let entity = state.spawn_player_from_data(1, &data);
    let pos = state.world.get::<&Position>(entity).unwrap();
    assert_eq!(pos.room_id, 3);
}

// ── Full command pipeline ─────────────────────────────────────────────────────

#[test]
fn command_channel_is_fully_drained_each_tick() {
    let (tx, mut rx, reg, _) = test_setup(&[1]);
    let mut state = GameState::new();

    for cmd in [
        Command::Look,
        Command::Say("hi".into()),
        Command::Move(Direction::West),
        Command::Quit,
    ] {
        tx.try_send(PlayerInput::new(1, cmd)).unwrap();
    }

    process_input(&mut state, &mut rx, &reg);
    assert!(rx.try_recv().is_err());
}

#[test]
fn connect_sends_welcome_with_room_description() {
    let (tx, mut rx, reg, mut receivers) = test_setup(&[1]);
    let mut state = GameState::new();

    tx.try_send(connect(1)).unwrap();
    process_input(&mut state, &mut rx, &reg);

    let msgs = drain(receivers.get_mut(&1).unwrap());
    assert!(
        msgs.iter().any(|m| m.contains("Town Square")),
        "Expected starting room in welcome: {:?}",
        msgs
    );
}

#[test]
fn look_returns_current_room_description() {
    let (tx, mut rx, reg, mut receivers) = test_setup(&[1]);
    let mut state = GameState::new();

    tx.try_send(connect(1)).unwrap();
    tx.try_send(PlayerInput::new(1, Command::Look)).unwrap();
    process_input(&mut state, &mut rx, &reg);

    let msgs = drain(receivers.get_mut(&1).unwrap());
    assert!(
        msgs.iter().filter(|m| m.contains("Town Square")).count() >= 2,
        "Both Connect welcome and Look should show the room"
    );
}

#[test]
fn player_navigates_town_square_to_north_gate() {
    let (tx, mut rx, reg, mut receivers) = test_setup(&[1]);
    let mut state = GameState::new();

    tx.try_send(connect(1)).unwrap();
    tx.try_send(PlayerInput::new(1, Command::Move(Direction::North)))
        .unwrap();
    process_input(&mut state, &mut rx, &reg);

    let entity = state.player_registry.get_entity(1).unwrap();
    let pos = state.world.get::<&Position>(entity).unwrap();
    assert_eq!(pos.room_id, 2, "Player should be in North Gate (room 2)");

    let msgs = drain(receivers.get_mut(&1).unwrap());
    assert!(msgs.iter().any(|m| m.contains("North Gate")));
}

#[test]
fn player_completes_three_room_circuit() {
    let (tx, mut rx, reg, _) = test_setup(&[1]);
    let mut state = GameState::new();

    for cmd in [
        Command::Connect(CharacterData::default()),
        Command::Move(Direction::North), // → room 2
        Command::Move(Direction::North), // → room 4
        Command::Move(Direction::South), // → room 2
    ] {
        tx.try_send(PlayerInput::new(1, cmd)).unwrap();
    }
    process_input(&mut state, &mut rx, &reg);

    let entity = state.player_registry.get_entity(1).unwrap();
    let pos = state.world.get::<&Position>(entity).unwrap();
    assert_eq!(pos.room_id, 2, "Should be back in North Gate");
}

#[test]
fn say_broadcasts_to_players_in_same_room() {
    let (tx, mut rx, reg, mut receivers) = test_setup(&[1, 2]);
    let mut state = GameState::new();

    tx.try_send(connect(1)).unwrap();
    tx.try_send(connect(2)).unwrap();
    tx.try_send(PlayerInput::new(1, Command::Say("greetings".to_string())))
        .unwrap();
    process_input(&mut state, &mut rx, &reg);

    let all_msgs: Vec<String> = receivers.values_mut().flat_map(drain).collect();
    assert!(all_msgs.iter().any(|m| m.contains("You say")));
    assert!(all_msgs.iter().any(|m| m.contains("Someone says")));
}

#[test]
fn say_does_not_reach_player_in_different_room() {
    let (tx, mut rx, reg, mut receivers) = test_setup(&[1, 2]);
    let mut state = GameState::new();

    // Player 1 moves to North Gate.
    tx.try_send(connect(1)).unwrap();
    tx.try_send(PlayerInput::new(1, Command::Move(Direction::North)))
        .unwrap();
    // Player 2 stays in Town Square.
    tx.try_send(connect(2)).unwrap();
    // Player 2 says something.
    tx.try_send(PlayerInput::new(2, Command::Say("hello".to_string())))
        .unwrap();
    process_input(&mut state, &mut rx, &reg);

    // Drain player 1's output and check no say message leaked across rooms.
    let p1_msgs = drain(receivers.get_mut(&1).unwrap());
    let leaked: Vec<_> = p1_msgs
        .iter()
        .filter(|m| m.contains("You say") || m.contains("Someone says"))
        .collect();
    assert!(
        leaked.is_empty(),
        "Player 1 must not hear player 2 across rooms: {:?}",
        leaked
    );
}

#[test]
fn quit_despawns_player_entity_and_queues_save() {
    let (tx, mut rx, reg, _) = test_setup(&[1]);
    let mut state = GameState::new();

    tx.try_send(connect(1)).unwrap();
    process_input(&mut state, &mut rx, &reg);
    assert_eq!(state.world.len(), 1);

    tx.try_send(PlayerInput::new(1, Command::Quit)).unwrap();
    process_input(&mut state, &mut rx, &reg);

    assert!(!state.player_registry.is_connected(1));
    assert_eq!(state.world.len(), 0);
    assert_eq!(
        state.pending_saves.len(),
        1,
        "Character data should be queued for saving"
    );
}

#[test]
fn unknown_command_sends_error_and_does_not_crash() {
    let (tx, mut rx, reg, mut receivers) = test_setup(&[1]);
    let mut state = GameState::new();

    tx.try_send(connect(1)).unwrap();
    tx.try_send(PlayerInput::new(
        1,
        Command::Unknown("frobnicate".to_string()),
    ))
    .unwrap();
    process_input(&mut state, &mut rx, &reg);

    let msgs = drain(receivers.get_mut(&1).unwrap());
    assert!(msgs.iter().any(|m| m.contains("frobnicate")));
}
