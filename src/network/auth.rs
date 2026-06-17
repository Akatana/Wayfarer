use crate::character::CharacterData;
use crate::db;
use crate::db::account::AccountData;

const WELCOME: &str = concat!(
    "\r\n",
    "  <yellow>*** WAYFARER MUD ***</yellow>\r\n",
    "\r\n",
    "  A tick-based engine written in Rust.\r\n",
    "\r\n",
);

/// Abstraction over the two transport layers (Telnet / WebSocket).
///
/// Implementors must render color tags before writing and must strip
/// protocol framing before returning lines to callers.
pub(super) trait MudSession {
    async fn send(&mut self, s: &str) -> Option<()>;
    async fn recv(&mut self) -> Option<String>;
}

pub(super) async fn auth_phase(
    session: &mut impl MudSession,
    db: &sea_orm::DatabaseConnection,
) -> Option<AccountData> {
    session.send(WELCOME).await?;
    loop {
        session
            .send(
                "\r\n<yellow>[L]</yellow>ogin  <yellow>[R]</yellow>egister  <yellow>[Q]</yellow>uit\r\n> ",
            )
            .await?;
        match session.recv().await?.to_lowercase().as_str() {
            "l" | "login" => {
                if let Some(account) = login_flow(session, db).await {
                    return Some(account);
                }
            }
            "r" | "register" => {
                if let Some(account) = register_flow(session, db).await {
                    return Some(account);
                }
            }
            "q" | "quit" => {
                session.send("Farewell, traveller.\r\n").await?;
                return None;
            }
            _ => {
                session
                    .send(
                        "Please enter <yellow>L</yellow>, <yellow>R</yellow>, or <yellow>Q</yellow>.\r\n",
                    )
                    .await?;
            }
        }
    }
}

async fn login_flow(
    session: &mut impl MudSession,
    db: &sea_orm::DatabaseConnection,
) -> Option<AccountData> {
    session.send("\r\nUsername: ").await?;
    let username = session.recv().await?;
    if username.is_empty() {
        return None;
    }
    session.send("Password: ").await?;
    let password = session.recv().await?;
    match db::account::authenticate(db, &username, &password).await {
        Some(account) => {
            session
                .send(&format!(
                    "Welcome back, <yellow>{}</yellow>!\r\n",
                    account.username
                ))
                .await?;
            Some(account)
        }
        None => {
            session
                .send("<red>Invalid username or password.</red>\r\n")
                .await?;
            None
        }
    }
}

async fn register_flow(
    session: &mut impl MudSession,
    db: &sea_orm::DatabaseConnection,
) -> Option<AccountData> {
    session
        .send("\r\nChoose a username (3-20 alphanumeric characters): ")
        .await?;
    let username = session.recv().await?;
    if username.len() < 3 || username.len() > 20 || !username.chars().all(|c| c.is_alphanumeric()) {
        session
            .send("<red>Username must be 3-20 alphanumeric characters.</red>\r\n")
            .await?;
        return None;
    }

    session
        .send("Choose a password (minimum 6 characters): ")
        .await?;
    let password = session.recv().await?;
    if password.len() < 6 {
        session
            .send("<red>Password must be at least 6 characters.</red>\r\n")
            .await?;
        return None;
    }

    session.send("Confirm password: ").await?;
    let password2 = session.recv().await?;
    if password != password2 {
        session
            .send("<red>Passwords do not match.</red>\r\n")
            .await?;
        return None;
    }

    match db::account::register(db, &username, &password).await {
        Ok(account) => {
            let suffix = if account.is_admin {
                " <yellow>[Admin]</yellow>"
            } else {
                ""
            };
            session
                .send(&format!(
                    "<green>Account '{}' created!{}</green>\r\n",
                    account.username, suffix
                ))
                .await?;
            Some(account)
        }
        Err(db::account::RegisterError::UsernameTaken) => {
            session
                .send("<red>That username is already taken.</red>\r\n")
                .await?;
            None
        }
        Err(db::account::RegisterError::Db(e)) => {
            eprintln!("[Auth] Registration DB error: {e}");
            session
                .send("<red>Server error during registration.</red>\r\n")
                .await?;
            None
        }
    }
}

pub(super) async fn char_select_phase(
    session: &mut impl MudSession,
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
        session.send(&display).await?;

        let input = session.recv().await?;
        let lower = input.to_lowercase();

        if let Ok(n) = lower.parse::<usize>() {
            if n >= 1 && n <= chars.len() {
                return Some(chars[n - 1].clone());
            }
            session.send("<red>Invalid selection.</red>\r\n").await?;
            continue;
        }

        match lower.as_str() {
            "n" | "new" => {
                if let Some(ch) = create_char_flow(session, db, account).await {
                    return Some(ch);
                }
            }
            "d" | "delete" => {
                delete_char_flow(session, db, account, &chars).await?;
            }
            "q" | "quit" => {
                session.send("Farewell, traveller.\r\n").await?;
                return None;
            }
            _ => {
                session
                    .send(
                        "Enter a number, <yellow>N</yellow>, <yellow>D</yellow>, or <yellow>Q</yellow>.\r\n",
                    )
                    .await?;
            }
        }
    }
}

async fn create_char_flow(
    session: &mut impl MudSession,
    db: &sea_orm::DatabaseConnection,
    account: &AccountData,
) -> Option<CharacterData> {
    session
        .send("\r\nCharacter name (1-24 alphanumeric characters): ")
        .await?;
    let raw = session.recv().await?;
    if raw.is_empty() || raw.len() > 24 || !raw.chars().all(|c| c.is_alphanumeric()) {
        session
            .send("<red>Name must be 1-24 alphanumeric characters.</red>\r\n")
            .await?;
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
            session
                .send(&format!(
                    "<green>Character '{}' created!</green>\r\n",
                    ch.name
                ))
                .await?;
            Some(ch)
        }
        Err(db::character::CreateError::NameTaken) => {
            session
                .send("<red>That name is already taken.</red>\r\n")
                .await?;
            None
        }
        Err(db::character::CreateError::Db(e)) => {
            eprintln!("[Auth] Character creation DB error: {e}");
            session
                .send("<red>Server error creating character.</red>\r\n")
                .await?;
            None
        }
    }
}

async fn delete_char_flow(
    session: &mut impl MudSession,
    db: &sea_orm::DatabaseConnection,
    account: &AccountData,
    chars: &[CharacterData],
) -> Option<()> {
    if chars.is_empty() {
        session
            .send("You have no characters to delete.\r\n")
            .await?;
        return Some(());
    }

    session
        .send("\r\nEnter the number of the character to delete: ")
        .await?;
    let input = session.recv().await?;
    let n: usize = match input.parse() {
        Ok(n) => n,
        Err(_) => {
            session.send("<red>Invalid number.</red>\r\n").await?;
            return Some(());
        }
    };

    if n < 1 || n > chars.len() {
        session.send("<red>Invalid selection.</red>\r\n").await?;
        return Some(());
    }

    let target = &chars[n - 1];
    session
        .send(&format!("Type '{}' to confirm deletion: ", target.name))
        .await?;
    let confirm = session.recv().await?;
    if confirm != target.name {
        session
            .send("Name did not match. Deletion cancelled.\r\n")
            .await?;
        return Some(());
    }

    match db::character::delete_by_id(db, target.id, account.id).await {
        Ok(true) => {
            session
                .send(&format!(
                    "<green>Character '{}' deleted.</green>\r\n",
                    target.name
                ))
                .await?;
        }
        Ok(false) => {
            session
                .send("<red>Character not found or permission denied.</red>\r\n")
                .await?;
        }
        Err(e) => {
            eprintln!("[Auth] Character deletion DB error: {e}");
            session
                .send("<red>Server error deleting character.</red>\r\n")
                .await?;
        }
    }
    Some(())
}
