use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

use crate::character::CharacterData;
use crate::command::{ClientId, Command, PlayerInput};
use crate::direction::Direction;

const MOCK_CLIENT_ID: ClientId = 1;

/// Simulates a single player session for local development and smoke-testing.
///
/// Registers a per-client output channel with the game loop, sends a scripted
/// command sequence (mirroring what `session::handle_session` would produce),
/// and drains any output that comes back.
pub async fn run_mock_network(
    command_tx: mpsc::Sender<PlayerInput>,
    register_tx: mpsc::Sender<(ClientId, mpsc::Sender<String>)>,
    deregister_tx: mpsc::Sender<ClientId>,
) {
    println!("[Mock] Client {MOCK_CLIENT_ID} connecting.");

    // Create a per-client output channel and register it before sending Connect.
    let (out_tx, mut out_rx) = mpsc::channel::<String>(64);
    register_tx.send((MOCK_CLIENT_ID, out_tx)).await.ok();

    // Send Connect with default character data (no DB lookup in the mock).
    let char_data = CharacterData {
        name: "Tester".to_string(),
        ..Default::default()
    };
    command_tx
        .send(PlayerInput::new(MOCK_CLIENT_ID, Command::Connect(char_data)))
        .await
        .ok();

    // Scripted command sequence — two ticks (400 ms) apart.
    let script: &[(Duration, Command)] = &[
        (Duration::from_millis(400), Command::Look),
        (Duration::from_millis(400), Command::Say("Hello, world!".to_string())),
        (Duration::from_millis(400), Command::Move(Direction::North)),
        (Duration::from_millis(400), Command::Look),
        (Duration::from_millis(400), Command::Unknown("xyzzy".to_string())),
        (Duration::from_millis(400), Command::Quit),
    ];

    for (delay, cmd) in script {
        sleep(*delay).await;
        println!("[Mock] → {:?}", cmd);
        if command_tx
            .send(PlayerInput::new(MOCK_CLIENT_ID, cmd.clone()))
            .await
            .is_err()
        {
            eprintln!("[Mock] Game loop channel closed.");
            break;
        }
    }

    // Give the game loop one tick to process Quit, then drain output.
    sleep(Duration::from_millis(400)).await;
    while let Ok(msg) = out_rx.try_recv() {
        println!("[Mock ←] {msg}");
    }

    deregister_tx.send(MOCK_CLIENT_ID).await.ok();
    println!("[Mock] Session complete.");
}
