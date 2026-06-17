use std::sync::atomic::Ordering;

use tokio::net::TcpListener;
use tokio::sync::mpsc;

use super::session::handle_session;
use super::NEXT_CLIENT_ID;
use crate::command::{ClientId, PlayerInput};

/// Binds a TCP listener on `addr` and accepts connections indefinitely.
///
/// Each accepted connection gets a unique `ClientId`, a dedicated output
/// channel, and its own tokio task running `handle_session`.
pub async fn run_telnet_server(
    addr: &str,
    command_tx: mpsc::Sender<PlayerInput>,
    register_tx: mpsc::Sender<(ClientId, mpsc::Sender<String>)>,
    deregister_tx: mpsc::Sender<ClientId>,
    db: sea_orm::DatabaseConnection,
) {
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("[Telnet] Cannot bind {addr}: {e}"));

    println!("[Telnet] Listening on {addr}");

    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                let client_id = NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed);
                println!("[Telnet] Client {client_id} connected from {peer_addr}");

                let (out_tx, out_rx) = mpsc::channel::<String>(64);

                // Register the output sender with the game loop before the
                // session task starts sending commands, so output is never lost.
                register_tx.send((client_id, out_tx)).await.ok();

                tokio::spawn(handle_session(
                    client_id,
                    stream,
                    command_tx.clone(),
                    out_rx,
                    deregister_tx.clone(),
                    db.clone(),
                ));
            }
            Err(e) => eprintln!("[Telnet] Accept error: {e}"),
        }
    }
}
