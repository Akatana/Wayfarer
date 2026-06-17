use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

use crate::command::{ClientId, Command, PlayerInput};
use crate::db;
use crate::direction::Direction;

const MOCK_CLIENT_ID: ClientId = 1;

/// Simulates a single player session for local development and smoke-testing.
///
/// Loads or creates a "mock_tester" account and a "Tester" character via the DB,
/// then runs a scripted command sequence, mirroring what a real session handler
/// would produce without requiring a live network connection.
pub async fn run_mock_network(
    command_tx: mpsc::Sender<PlayerInput>,
    register_tx: mpsc::Sender<(ClientId, mpsc::Sender<String>)>,
    deregister_tx: mpsc::Sender<ClientId>,
    db: sea_orm::DatabaseConnection,
) {
    println!("[Mock] Client {MOCK_CLIENT_ID} connecting.");

    // Create a per-client output channel and register it before sending Connect.
    let (out_tx, mut out_rx) = mpsc::channel::<String>(64);
    register_tx.send((MOCK_CLIENT_ID, out_tx)).await.ok();

    // Ensure the mock account and character exist, then connect with real data.
    let account = db::account::find_or_create_mock(&db).await;
    let chars = db::character::list_for_account(&db, account.id, account.is_admin).await;
    let char_data = if let Some(ch) = chars.into_iter().find(|c| c.name == "Tester") {
        ch
    } else {
        db::character::create_for_account(&db, account.id, "Tester", account.is_admin)
            .await
            .unwrap_or_else(|_| crate::character::CharacterData {
                name: "Tester".to_string(),
                account_id: account.id,
                is_admin: account.is_admin,
                ..Default::default()
            })
    };

    command_tx
        .send(PlayerInput::new(
            MOCK_CLIENT_ID,
            Command::Connect(char_data),
        ))
        .await
        .ok();

    // Scripted command sequence — two ticks (400 ms) apart.
    let script: &[(Duration, Command)] = &[
        (Duration::from_millis(400), Command::Look),
        (
            Duration::from_millis(400),
            Command::Say("Hello, world!".to_string()),
        ),
        (Duration::from_millis(400), Command::Move(Direction::North)),
        (Duration::from_millis(400), Command::Look),
        (
            Duration::from_millis(400),
            Command::Unknown("xyzzy".to_string()),
        ),
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
