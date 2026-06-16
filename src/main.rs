use tokio::sync::mpsc;
use wayfarer::{command, db, game_loop, network};

#[tokio::main]
async fn main() {
    // ── Database ──────────────────────────────────────────────────────────────
    let db = db::connect("sqlite://./wayfarer.db?mode=rwc")
        .await
        .expect("[DB] Failed to connect to SQLite");

    db::schema::create_tables(&db)
        .await
        .expect("[DB] Failed to create schema");

    println!("[Wayfarer] Database ready.");

    // ── Channels ──────────────────────────────────────────────────────────────
    // Network → game loop: player commands.
    let (command_tx, command_rx) = mpsc::channel::<command::PlayerInput>(256);
    // Sessions → game loop: new per-client output sender.
    let (register_tx, register_rx) = mpsc::channel::<(command::ClientId, mpsc::Sender<String>)>(64);
    // Sessions → game loop: disconnected client ID.
    let (deregister_tx, deregister_rx) = mpsc::channel::<command::ClientId>(64);

    println!("[Wayfarer] Booting engine...");

    // ── Tasks ─────────────────────────────────────────────────────────────────
    // Mock fires once at startup for smoke-testing; its completion does not
    // affect server lifetime.
    tokio::spawn(network::mock::run_mock_network(
        command_tx.clone(),
        register_tx.clone(),
        deregister_tx.clone(),
    ));

    let tcp_handle = tokio::spawn(network::telnet::run_telnet_server(
        "0.0.0.0:4000",
        command_tx.clone(),
        register_tx.clone(),
        deregister_tx.clone(),
        db.clone(),
    ));

    let ws_handle = tokio::spawn(network::websocket::run_websocket_server(
        "0.0.0.0:4001",
        command_tx,
        register_tx,
        deregister_tx,
        db.clone(),
    ));

    let game_handle = tokio::spawn(game_loop::run(command_rx, register_rx, deregister_rx, db));

    tokio::select! {
        _ = tcp_handle   => eprintln!("[Wayfarer] Telnet server exited."),
        _ = ws_handle    => eprintln!("[Wayfarer] WebSocket server exited."),
        _ = game_handle  => eprintln!("[Wayfarer] Game loop exited."),
    }

    println!("[Wayfarer] Engine shutdown complete.");
}
