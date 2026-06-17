use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};

use super::parser;
use crate::character::CharacterData;
use crate::command::{ClientId, Command, PlayerInput};
use crate::db;
use crate::db::account::AccountData;

type WsSink = SplitSink<WebSocketStream<TcpStream>, Message>;
type WsStream = SplitStream<WebSocketStream<TcpStream>>;

const WELCOME: &str = concat!(
    "\r\n",
    "  *** WAYFARER MUD ***\r\n",
    "\r\n",
    "  A tick-based engine written in Rust.\r\n",
    "\r\n",
);

/// Drives a single WebSocket player connection from handshake to disconnect.
///
/// Phases:
/// 1. Auth gate — welcome screen → login or register → account.
/// 2. Character select — list, play, create, or delete characters.
/// 3. I/O loop — relay text frames to the game loop and push output back.
/// 4. Cleanup — send `Command::Quit` and deregister with the game loop.
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

    // ── Phase 1: Account auth ─────────────────────────────────────────────────
    let Some(account) = ws_auth_phase(&mut sink, &mut stream, &db).await else {
        return;
    };

    // ── Phase 2: Character select ─────────────────────────────────────────────
    let Some(char_data) = ws_char_select_phase(&mut sink, &mut stream, &db, &account).await else {
        return;
    };

    if command_tx
        .send(PlayerInput::new(client_id, Command::Connect(char_data)))
        .await
        .is_err()
    {
        return;
    }

    // ── Phase 3: I/O loop ─────────────────────────────────────────────────────
    loop {
        tokio::select! {
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

    // ── Phase 4: Cleanup ──────────────────────────────────────────────────────
    println!("[WS] Client {client_id} disconnected.");
    let _ = sink.send(Message::Close(None)).await;
    let _ = command_tx
        .send(PlayerInput::new(client_id, Command::Quit))
        .await;
    let _ = deregister_tx.send(client_id).await;
}

// ── Auth helpers ──────────────────────────────────────────────────────────────

async fn send_ws(sink: &mut WsSink, s: &str) -> Option<()> {
    sink.send(Message::Text(crate::color::render(s))).await.ok()
}

async fn read_ws(stream: &mut WsStream) -> Option<String> {
    loop {
        match stream.next().await {
            Some(Ok(Message::Text(t))) => {
                let t = t.trim().to_string();
                if !t.is_empty() {
                    return Some(t);
                }
            }
            Some(Ok(Message::Close(_))) | None => return None,
            _ => continue,
        }
    }
}

async fn ws_auth_phase(
    sink: &mut WsSink,
    stream: &mut WsStream,
    db: &sea_orm::DatabaseConnection,
) -> Option<AccountData> {
    send_ws(sink, WELCOME).await?;

    loop {
        send_ws(sink, "\r\n[L]ogin  [R]egister  [Q]uit\r\n").await?;

        match read_ws(stream).await?.to_lowercase().as_str() {
            "l" | "login" => {
                if let Some(account) = ws_login_flow(sink, stream, db).await {
                    return Some(account);
                }
            }
            "r" | "register" => {
                if let Some(account) = ws_register_flow(sink, stream, db).await {
                    return Some(account);
                }
            }
            "q" | "quit" => {
                send_ws(sink, "Farewell, traveller.\r\n").await?;
                return None;
            }
            _ => {
                send_ws(sink, "Please enter L, R, or Q.\r\n").await?;
            }
        }
    }
}

async fn ws_login_flow(
    sink: &mut WsSink,
    stream: &mut WsStream,
    db: &sea_orm::DatabaseConnection,
) -> Option<AccountData> {
    send_ws(sink, "Username:").await?;
    let username = read_ws(stream).await?;
    if username.is_empty() {
        return None;
    }

    send_ws(sink, "Password:").await?;
    let password = read_ws(stream).await?;

    match db::account::authenticate(db, &username, &password).await {
        Some(account) => {
            send_ws(sink, &format!("Welcome back, {}!\r\n", account.username)).await?;
            Some(account)
        }
        None => {
            send_ws(sink, "Invalid username or password.\r\n").await?;
            None
        }
    }
}

async fn ws_register_flow(
    sink: &mut WsSink,
    stream: &mut WsStream,
    db: &sea_orm::DatabaseConnection,
) -> Option<AccountData> {
    send_ws(sink, "Choose a username (3-20 alphanumeric characters):").await?;
    let username = read_ws(stream).await?;

    if username.len() < 3 || username.len() > 20 || !username.chars().all(|c| c.is_alphanumeric()) {
        send_ws(sink, "Username must be 3-20 alphanumeric characters.\r\n").await?;
        return None;
    }

    send_ws(sink, "Choose a password (minimum 6 characters):").await?;
    let password = read_ws(stream).await?;

    if password.len() < 6 {
        send_ws(sink, "Password must be at least 6 characters.\r\n").await?;
        return None;
    }

    send_ws(sink, "Confirm password:").await?;
    let password2 = read_ws(stream).await?;

    if password != password2 {
        send_ws(sink, "Passwords do not match.\r\n").await?;
        return None;
    }

    match db::account::register(db, &username, &password).await {
        Ok(account) => {
            let suffix = if account.is_admin { " [Admin]" } else { "" };
            send_ws(
                sink,
                &format!("Account '{}' created!{}\r\n", account.username, suffix),
            )
            .await?;
            Some(account)
        }
        Err(db::account::RegisterError::UsernameTaken) => {
            send_ws(sink, "That username is already taken.\r\n").await?;
            None
        }
        Err(db::account::RegisterError::Db(e)) => {
            eprintln!("[Auth] Registration DB error: {e}");
            send_ws(sink, "Server error during registration.\r\n").await?;
            None
        }
    }
}

async fn ws_char_select_phase(
    sink: &mut WsSink,
    stream: &mut WsStream,
    db: &sea_orm::DatabaseConnection,
    account: &AccountData,
) -> Option<CharacterData> {
    loop {
        let chars = db::character::list_for_account(db, account.id, account.is_admin).await;

        let mut display = String::from("\r\n=== Your Characters ===\r\n\r\n");
        if chars.is_empty() {
            display.push_str("  (no characters yet)\r\n");
        } else {
            for (i, ch) in chars.iter().enumerate() {
                display.push_str(&format!("  [{}] {}\r\n", i + 1, ch.name));
            }
        }
        display.push_str("\r\n  [N] New character\r\n  [D] Delete\r\n  [Q] Disconnect\r\n");

        send_ws(sink, &display).await?;

        let input = read_ws(stream).await?;
        let lower = input.to_lowercase();

        if let Ok(n) = lower.parse::<usize>() {
            if n >= 1 && n <= chars.len() {
                return Some(chars[n - 1].clone());
            }
            send_ws(sink, "Invalid selection.\r\n").await?;
            continue;
        }

        match lower.as_str() {
            "n" | "new" => {
                if let Some(ch) = ws_create_char_flow(sink, stream, db, account).await {
                    return Some(ch);
                }
            }
            "d" | "delete" => {
                ws_delete_char_flow(sink, stream, db, account, &chars).await?;
            }
            "q" | "quit" => {
                send_ws(sink, "Farewell, traveller.\r\n").await?;
                return None;
            }
            _ => {
                send_ws(sink, "Enter a number, N, D, or Q.\r\n").await?;
            }
        }
    }
}

async fn ws_create_char_flow(
    sink: &mut WsSink,
    stream: &mut WsStream,
    db: &sea_orm::DatabaseConnection,
    account: &AccountData,
) -> Option<CharacterData> {
    send_ws(sink, "Character name (1-24 alphanumeric characters):").await?;
    let raw = read_ws(stream).await?;

    if raw.is_empty() || raw.len() > 24 || !raw.chars().all(|c| c.is_alphanumeric()) {
        send_ws(sink, "Name must be 1-24 alphanumeric characters.\r\n").await?;
        return None;
    }

    let name = {
        let mut chars = raw.chars();
        match chars.next() {
            None => raw,
            Some(c) => c.to_uppercase().to_string() + chars.as_str(),
        }
    };

    match db::character::create_for_account(db, account.id, &name, account.is_admin).await {
        Ok(ch) => {
            send_ws(sink, &format!("Character '{}' created!\r\n", ch.name)).await?;
            Some(ch)
        }
        Err(db::character::CreateError::NameTaken) => {
            send_ws(sink, "That name is already taken.\r\n").await?;
            None
        }
        Err(db::character::CreateError::Db(e)) => {
            eprintln!("[Auth] Character creation DB error: {e}");
            send_ws(sink, "Server error creating character.\r\n").await?;
            None
        }
    }
}

async fn ws_delete_char_flow(
    sink: &mut WsSink,
    stream: &mut WsStream,
    db: &sea_orm::DatabaseConnection,
    account: &AccountData,
    chars: &[CharacterData],
) -> Option<()> {
    if chars.is_empty() {
        send_ws(sink, "You have no characters to delete.\r\n").await?;
        return Some(());
    }

    send_ws(sink, "Enter the number of the character to delete:").await?;
    let input = read_ws(stream).await?;
    let n: usize = match input.parse() {
        Ok(n) => n,
        Err(_) => {
            send_ws(sink, "Invalid number.\r\n").await?;
            return Some(());
        }
    };

    if n < 1 || n > chars.len() {
        send_ws(sink, "Invalid selection.\r\n").await?;
        return Some(());
    }

    let target = &chars[n - 1];
    send_ws(
        sink,
        &format!("Type '{}' to confirm deletion:", target.name),
    )
    .await?;
    let confirm = read_ws(stream).await?;

    if confirm != target.name {
        send_ws(sink, "Name did not match. Deletion cancelled.\r\n").await?;
        return Some(());
    }

    match db::character::delete_by_id(db, target.id, account.id).await {
        Ok(true) => {
            send_ws(sink, &format!("Character '{}' deleted.\r\n", target.name)).await?;
        }
        Ok(false) => {
            send_ws(sink, "Character not found or permission denied.\r\n").await?;
        }
        Err(e) => {
            eprintln!("[Auth] Character deletion DB error: {e}");
            send_ws(sink, "Server error deleting character.\r\n").await?;
        }
    }
    Some(())
}
