use std::sync::atomic::Ordering;

use tokio::net::TcpListener;
use tokio::sync::mpsc;

use super::ws_session::handle_ws_session;
use super::NEXT_CLIENT_ID;
use crate::command::{ClientId, PlayerInput};

/// Binds a WebSocket listener on `addr` and accepts connections indefinitely.
///
/// Each connection is upgraded from raw TCP to WebSocket, assigned a unique
/// `ClientId`, and handed to `handle_ws_session` in its own task.
pub async fn run_websocket_server(
    addr: &str,
    command_tx: mpsc::Sender<PlayerInput>,
    register_tx: mpsc::Sender<(ClientId, mpsc::Sender<String>)>,
    deregister_tx: mpsc::Sender<ClientId>,
    db: sea_orm::DatabaseConnection,
) {
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("[WS] Cannot bind {addr}: {e}"));

    println!("[WS] Listening on {addr}");

    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                let client_id = NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed);
                println!("[WS] Client {client_id} connected from {peer_addr}");

                let (out_tx, out_rx) = mpsc::channel::<String>(64);
                register_tx.send((client_id, out_tx)).await.ok();

                tokio::spawn(handle_ws_session(
                    client_id,
                    stream,
                    command_tx.clone(),
                    out_rx,
                    deregister_tx.clone(),
                    db.clone(),
                ));
            }
            Err(e) => eprintln!("[WS] Accept error: {e}"),
        }
    }
}
