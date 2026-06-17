use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::{accept_async, tungstenite::Message};

use super::parser;
use crate::character::CharacterData;
use crate::command::{ClientId, Command, PlayerInput};
use crate::db::character as char_db;

const BANNER: &str = concat!(
    "*** Wayfarer MUD ***\n",
    "A tick-based engine written in Rust.\n",
);

/// Drives a single WebSocket player connection from handshake to disconnect.
///
/// Phases:
/// 1. Auth gate — prompt for name, DB load-or-create, send `Command::Connect`.
/// 2. I/O loop — relay text frames to the game loop and push game output back.
/// 3. Cleanup — send `Command::Quit` and deregister with the game loop.
pub async fn handle_ws_session(
    client_id: ClientId,
    tcp_stream: TcpStream,
    command_tx: mpsc::Sender<PlayerInput>,
    mut output_rx: mpsc::Receiver<String>,
    deregister_tx: mpsc::Sender<ClientId>,
    db: sea_orm::DatabaseConnection,
) {
    let ws_stream = match accept_async(tcp_stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("[WS] Handshake failed for client {client_id}: {e}");
            return;
        }
    };
    let (mut sink, mut stream) = ws_stream.split();

    macro_rules! send_or_return {
        ($text:expr) => {
            if sink.send(Message::Text($text.into())).await.is_err() {
                return;
            }
        };
    }

    // ── Phase 1: Auth gate ────────────────────────────────────────────────────
    send_or_return!(BANNER);
    send_or_return!("By what name do you wish to be known?");

    let name = loop {
        match stream.next().await {
            Some(Ok(Message::Text(text))) => {
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    break trimmed;
                }
            }
            Some(Ok(Message::Close(_))) | None => return,
            _ => continue,
        }
    };

    if name.len() > 24 || !name.chars().all(|c| c.is_alphanumeric()) {
        send_or_return!("Invalid name (1-24 alphanumeric characters). Goodbye.");
        return;
    }

    let char_data: CharacterData = char_db::load_or_create(&db, &name).await;
    let is_returning = char_data.room_id != crate::world::seed::STARTING_ROOM_ID
        || char_data.hp != char_data.max_hp;

    let greeting = if is_returning {
        format!("Welcome back, {name}!")
    } else {
        format!("Welcome to Wayfarer, {name}! A new adventure begins.")
    };
    send_or_return!(&greeting);

    if command_tx
        .send(PlayerInput::new(client_id, Command::Connect(char_data)))
        .await
        .is_err()
    {
        return;
    }

    // ── Phase 2: I/O loop ─────────────────────────────────────────────────────
    loop {
        tokio::select! {
            // Player → game loop
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let cmd = parser::parse(text.trim());
                        let is_quit = cmd == Command::Quit;
                        if command_tx.send(PlayerInput::new(client_id, cmd)).await.is_err() {
                            break;
                        }
                        if is_quit { break; }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            // Game loop → player
            output = output_rx.recv() => {
                match output {
                    Some(text) => {
                        if sink
                            .send(Message::Text(crate::color::render(&text)))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }

    // ── Phase 3: Cleanup ──────────────────────────────────────────────────────
    println!("[WS] Client {client_id} disconnected.");
    let _ = sink.send(Message::Close(None)).await;
    let _ = command_tx
        .send(PlayerInput::new(client_id, Command::Quit))
        .await;
    let _ = deregister_tx.send(client_id).await;
}
