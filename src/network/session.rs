use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{
    tcp::{OwnedReadHalf, OwnedWriteHalf},
    TcpStream,
};
use tokio::sync::mpsc;

use super::parser;
use crate::character::CharacterData;
use crate::command::{ClientId, Command, PlayerInput};
use crate::db;
use crate::db::account::AccountData;

// Telnet IAC sequences used to suppress/restore client-side echo during
// password prompts (RFC 857). Not all clients honour these, which is fine —
// the worst case is the password is visible in the terminal.
const IAC_WILL_ECHO: &[u8] = b"\xff\xfb\x01";
const IAC_WONT_ECHO: &[u8] = b"\xff\xfc\x01";

const WELCOME: &str = concat!(
    "\r\n",
    "  <yellow>*** WAYFARER MUD ***</yellow>\r\n",
    "\r\n",
    "  A tick-based engine written in Rust.\r\n",
    "\r\n",
);

/// Drives a single player's connection from first byte to disconnect.
///
/// Phases:
/// 1. Auth gate — welcome screen → login or register → account.
/// 2. Character select — list, play, create, or delete characters.
/// 3. I/O loop — relay typed lines to the game loop and push output back.
/// 4. Cleanup — send `Command::Quit` and deregister with the game loop.
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

    // ── Phase 1: Account auth ─────────────────────────────────────────────────
    let Some(account) = auth_phase(&mut reader, &mut writer, &db).await else {
        return;
    };

    // ── Phase 2: Character select ─────────────────────────────────────────────
    let Some(char_data) = char_select_phase(&mut reader, &mut writer, &db, &account).await else {
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
    let mut line_buf = String::new();
    loop {
        tokio::select! {
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
            msg = output_rx.recv() => {
                match msg {
                    Some(text) => {
                        let framed = format!("{}\r\n", crate::color::render(&text));
                        if writer.write_all(framed.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }

    // ── Phase 4: Cleanup ──────────────────────────────────────────────────────
    println!("[Telnet] Client {client_id} disconnected.");
    let _ = command_tx
        .send(PlayerInput::new(client_id, Command::Quit))
        .await;
    let _ = deregister_tx.send(client_id).await;
}

// ── Auth helpers ──────────────────────────────────────────────────────────────

async fn write_str(writer: &mut OwnedWriteHalf, s: &str) -> Option<()> {
    let rendered = crate::color::render(s);
    writer.write_all(rendered.as_bytes()).await.ok()
}

async fn read_line(reader: &mut BufReader<OwnedReadHalf>) -> Option<String> {
    let mut buf = String::new();
    match reader.read_line(&mut buf).await {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(buf.trim().to_string()),
    }
}

async fn auth_phase(
    reader: &mut BufReader<OwnedReadHalf>,
    writer: &mut OwnedWriteHalf,
    db: &sea_orm::DatabaseConnection,
) -> Option<AccountData> {
    write_str(writer, WELCOME).await?;

    loop {
        write_str(writer, "\r\n<yellow>[L]</yellow>ogin  <yellow>[R]</yellow>egister  <yellow>[Q]</yellow>uit\r\n> ").await?;

        match read_line(reader).await?.to_lowercase().as_str() {
            "l" | "login" => {
                if let Some(account) = login_flow(reader, writer, db).await {
                    return Some(account);
                }
            }
            "r" | "register" => {
                if let Some(account) = register_flow(reader, writer, db).await {
                    return Some(account);
                }
            }
            "q" | "quit" => {
                write_str(writer, "Farewell, traveller.\r\n").await?;
                return None;
            }
            _ => {
                write_str(writer, "Please enter <yellow>L</yellow>, <yellow>R</yellow>, or <yellow>Q</yellow>.\r\n").await?;
            }
        }
    }
}

async fn login_flow(
    reader: &mut BufReader<OwnedReadHalf>,
    writer: &mut OwnedWriteHalf,
    db: &sea_orm::DatabaseConnection,
) -> Option<AccountData> {
    write_str(writer, "\r\nUsername: ").await?;
    let username = read_line(reader).await?;
    if username.is_empty() {
        return None;
    }

    write_str(writer, "Password: ").await?;
    writer.write_all(IAC_WILL_ECHO).await.ok()?;
    let password = read_line(reader).await?;
    writer.write_all(IAC_WONT_ECHO).await.ok()?;
    write_str(writer, "\r\n").await?;

    match db::account::authenticate(db, &username, &password).await {
        Some(account) => {
            write_str(
                writer,
                &format!("Welcome back, <yellow>{}</yellow>!\r\n", account.username),
            )
            .await?;
            Some(account)
        }
        None => {
            write_str(writer, "<red>Invalid username or password.</red>\r\n").await?;
            None
        }
    }
}

async fn register_flow(
    reader: &mut BufReader<OwnedReadHalf>,
    writer: &mut OwnedWriteHalf,
    db: &sea_orm::DatabaseConnection,
) -> Option<AccountData> {
    write_str(
        writer,
        "\r\nChoose a username (3-20 alphanumeric characters): ",
    )
    .await?;
    let username = read_line(reader).await?;

    if username.len() < 3 || username.len() > 20 || !username.chars().all(|c| c.is_alphanumeric()) {
        write_str(
            writer,
            "<red>Username must be 3-20 alphanumeric characters.</red>\r\n",
        )
        .await?;
        return None;
    }

    write_str(writer, "Choose a password (minimum 6 characters): ").await?;
    writer.write_all(IAC_WILL_ECHO).await.ok()?;
    let password = read_line(reader).await?;
    writer.write_all(IAC_WONT_ECHO).await.ok()?;
    write_str(writer, "\r\n").await?;

    if password.len() < 6 {
        write_str(
            writer,
            "<red>Password must be at least 6 characters.</red>\r\n",
        )
        .await?;
        return None;
    }

    write_str(writer, "Confirm password: ").await?;
    writer.write_all(IAC_WILL_ECHO).await.ok()?;
    let password2 = read_line(reader).await?;
    writer.write_all(IAC_WONT_ECHO).await.ok()?;
    write_str(writer, "\r\n").await?;

    if password != password2 {
        write_str(writer, "<red>Passwords do not match.</red>\r\n").await?;
        return None;
    }

    match db::account::register(db, &username, &password).await {
        Ok(account) => {
            let suffix = if account.is_admin {
                " <yellow>[Admin]</yellow>"
            } else {
                ""
            };
            write_str(
                writer,
                &format!(
                    "<green>Account '{}' created!{}</green>\r\n",
                    account.username, suffix
                ),
            )
            .await?;
            Some(account)
        }
        Err(db::account::RegisterError::UsernameTaken) => {
            write_str(writer, "<red>That username is already taken.</red>\r\n").await?;
            None
        }
        Err(db::account::RegisterError::Db(e)) => {
            eprintln!("[Auth] Registration DB error: {e}");
            write_str(writer, "<red>Server error during registration.</red>\r\n").await?;
            None
        }
    }
}

async fn char_select_phase(
    reader: &mut BufReader<OwnedReadHalf>,
    writer: &mut OwnedWriteHalf,
    db: &sea_orm::DatabaseConnection,
    account: &AccountData,
) -> Option<CharacterData> {
    loop {
        let chars = db::character::list_for_account(db, account.id, account.is_admin).await;

        let mut display = String::from("\r\n<yellow>=== Your Characters ===</yellow>\r\n\r\n");
        if chars.is_empty() {
            display.push_str("  (no characters yet)\r\n");
        } else {
            for (i, ch) in chars.iter().enumerate() {
                display.push_str(&format!("  [{}] {}\r\n", i + 1, ch.name));
            }
        }
        display.push_str("\r\n");
        display.push_str("  <yellow>[N]</yellow> New character\r\n");
        display.push_str("  <yellow>[D]</yellow> Delete a character\r\n");
        display.push_str("  <yellow>[Q]</yellow> Disconnect\r\n");
        display.push_str("\r\nChoice: ");

        write_str(writer, &display).await?;

        let input = read_line(reader).await?;
        let lower = input.to_lowercase();

        if let Ok(n) = lower.parse::<usize>() {
            if n >= 1 && n <= chars.len() {
                return Some(chars[n - 1].clone());
            }
            write_str(writer, "<red>Invalid selection.</red>\r\n").await?;
            continue;
        }

        match lower.as_str() {
            "n" | "new" => {
                if let Some(ch) = create_char_flow(reader, writer, db, account).await {
                    return Some(ch);
                }
            }
            "d" | "delete" => {
                // delete_char_flow returns None only on disconnect.
                delete_char_flow(reader, writer, db, account, &chars).await?;
            }
            "q" | "quit" => {
                write_str(writer, "Farewell, traveller.\r\n").await?;
                return None;
            }
            _ => {
                write_str(
                    writer,
                    "Enter a number, <yellow>N</yellow>, <yellow>D</yellow>, or <yellow>Q</yellow>.\r\n",
                )
                .await?;
            }
        }
    }
}

async fn create_char_flow(
    reader: &mut BufReader<OwnedReadHalf>,
    writer: &mut OwnedWriteHalf,
    db: &sea_orm::DatabaseConnection,
    account: &AccountData,
) -> Option<CharacterData> {
    write_str(
        writer,
        "\r\nCharacter name (1-24 alphanumeric characters): ",
    )
    .await?;
    let raw = read_line(reader).await?;

    if raw.is_empty() || raw.len() > 24 || !raw.chars().all(|c| c.is_alphanumeric()) {
        write_str(
            writer,
            "<red>Name must be 1-24 alphanumeric characters.</red>\r\n",
        )
        .await?;
        return None;
    }

    // Capitalise the first letter for a consistent display style.
    let name = {
        let mut chars = raw.chars();
        match chars.next() {
            None => raw,
            Some(c) => c.to_uppercase().to_string() + chars.as_str(),
        }
    };

    match db::character::create_for_account(db, account.id, &name, account.is_admin).await {
        Ok(ch) => {
            write_str(
                writer,
                &format!("<green>Character '{}' created!</green>\r\n", ch.name),
            )
            .await?;
            Some(ch)
        }
        Err(db::character::CreateError::NameTaken) => {
            write_str(writer, "<red>That name is already taken.</red>\r\n").await?;
            None
        }
        Err(db::character::CreateError::Db(e)) => {
            eprintln!("[Auth] Character creation DB error: {e}");
            write_str(writer, "<red>Server error creating character.</red>\r\n").await?;
            None
        }
    }
}

async fn delete_char_flow(
    reader: &mut BufReader<OwnedReadHalf>,
    writer: &mut OwnedWriteHalf,
    db: &sea_orm::DatabaseConnection,
    account: &AccountData,
    chars: &[CharacterData],
) -> Option<()> {
    if chars.is_empty() {
        write_str(writer, "You have no characters to delete.\r\n").await?;
        return Some(());
    }

    write_str(writer, "\r\nEnter the number of the character to delete: ").await?;
    let input = read_line(reader).await?;

    let n: usize = match input.parse() {
        Ok(n) => n,
        Err(_) => {
            write_str(writer, "<red>Invalid number.</red>\r\n").await?;
            return Some(());
        }
    };

    if n < 1 || n > chars.len() {
        write_str(writer, "<red>Invalid selection.</red>\r\n").await?;
        return Some(());
    }

    let target = &chars[n - 1];
    write_str(
        writer,
        &format!("Type '{}' to confirm deletion: ", target.name),
    )
    .await?;
    let confirm = read_line(reader).await?;

    if confirm != target.name {
        write_str(writer, "Name did not match. Deletion cancelled.\r\n").await?;
        return Some(());
    }

    match db::character::delete_by_id(db, target.id, account.id).await {
        Ok(true) => {
            write_str(
                writer,
                &format!("<green>Character '{}' deleted.</green>\r\n", target.name),
            )
            .await?;
        }
        Ok(false) => {
            write_str(
                writer,
                "<red>Character not found or permission denied.</red>\r\n",
            )
            .await?;
        }
        Err(e) => {
            eprintln!("[Auth] Character deletion DB error: {e}");
            write_str(writer, "<red>Server error deleting character.</red>\r\n").await?;
        }
    }

    Some(())
}
