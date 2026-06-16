use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::character::CharacterData;
use crate::command::{ClientId, Command, PlayerInput};
use crate::db::character as char_db;
use super::parser;

const BANNER: &str = concat!(
    "\r\n",
    "  *** Wayfarer MUD ***\r\n",
    "  A tick-based engine written in Rust.\r\n",
    "\r\n",
);

/// Drives a single player's connection from first byte to disconnect.
///
/// Phases:
/// 1. Auth gate — prompt for name, DB load-or-create, send `Command::Connect`.
/// 2. I/O loop — relay typed lines to the game loop and push game output back.
/// 3. Cleanup — send `Command::Quit` and deregister with the game loop.
pub async fn handle_session(
    client_id: ClientId,
    stream: TcpStream,
    command_tx: mpsc::Sender<PlayerInput>,
    mut output_rx: mpsc::Receiver<String>,
    deregister_tx: mpsc::Sender<ClientId>,
    db: sea_orm::DatabaseConnection,
) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // ── Phase 1: Auth gate ────────────────────────────────────────────────────
    macro_rules! send_or_return {
        ($bytes:expr) => {
            if writer.write_all($bytes).await.is_err() {
                return;
            }
        };
    }

    send_or_return!(BANNER.as_bytes());
    send_or_return!(b"By what name do you wish to be known? ");

    let mut name_buf = String::new();
    match reader.read_line(&mut name_buf).await {
        Ok(0) | Err(_) => return,
        Ok(_) => {}
    }

    let name = name_buf.trim().to_string();
    if name.is_empty() || name.len() > 24 || !name.chars().all(|c| c.is_alphanumeric()) {
        send_or_return!(b"Invalid name (1-24 alphanumeric characters). Goodbye.\r\n");
        return;
    }

    // DB lookup happens here in the session task — game loop tick body stays sync.
    let char_data: CharacterData = char_db::load_or_create(&db, &name).await;
    let is_returning = char_data.room_id != crate::world::seed::STARTING_ROOM_ID
        || char_data.hp != char_data.max_hp;

    let greeting = if is_returning {
        format!("\r\nWelcome back, {name}!\r\n\r\n")
    } else {
        format!("\r\nWelcome to Wayfarer, {name}! A new adventure begins.\r\n\r\n")
    };
    send_or_return!(greeting.as_bytes());

    // Hand character data to the game loop — no async work needed inside the tick.
    if command_tx
        .send(PlayerInput::new(client_id, Command::Connect(char_data)))
        .await
        .is_err()
    {
        return;
    }

    // ── Phase 2: I/O loop ─────────────────────────────────────────────────────
    let mut line_buf = String::new();
    loop {
        tokio::select! {
            // Player → game loop
            result = reader.read_line(&mut line_buf) => {
                match result {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let cmd = parser::parse(line_buf.trim());
                        let is_quit = cmd == Command::Quit;

                        if command_tx.send(PlayerInput::new(client_id, cmd)).await.is_err() {
                            break;
                        }
                        line_buf.clear();
                        if is_quit { break; }
                    }
                }
            }
            // Game loop → player
            msg = output_rx.recv() => {
                match msg {
                    Some(text) => {
                        let framed = format!("{}\r\n", text);
                        if writer.write_all(framed.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }

    // ── Phase 3: Cleanup ──────────────────────────────────────────────────────
    println!("[Telnet] Client {client_id} disconnected.");
    // Send Quit in case the client dropped the connection without typing it.
    let _ = command_tx.send(PlayerInput::new(client_id, Command::Quit)).await;
    let _ = deregister_tx.send(client_id).await;
}
