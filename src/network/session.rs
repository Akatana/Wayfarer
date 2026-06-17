use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{
    tcp::{OwnedReadHalf, OwnedWriteHalf},
    TcpStream,
};
use tokio::sync::mpsc;

use super::{auth, parser};
use crate::command::{ClientId, Command, PlayerInput};

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

    // Auth and character select happen through the session adapter, then drop it
    // so reader/writer are available for the I/O loop below.
    let account = {
        let mut s = TelnetSession {
            reader: &mut reader,
            writer: &mut writer,
        };
        auth::auth_phase(&mut s, &db).await
    };
    let Some(account) = account else { return };

    let char_data = {
        let mut s = TelnetSession {
            reader: &mut reader,
            writer: &mut writer,
        };
        auth::char_select_phase(&mut s, &db, &account).await
    };
    let Some(char_data) = char_data else { return };

    if command_tx
        .send(PlayerInput::new(client_id, Command::Connect(char_data)))
        .await
        .is_err()
    {
        return;
    }

    // ── I/O loop ──────────────────────────────────────────────────────────────
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

    println!("[Telnet] Client {client_id} disconnected.");
    let _ = command_tx
        .send(PlayerInput::new(client_id, Command::Quit))
        .await;
    let _ = deregister_tx.send(client_id).await;
}

struct TelnetSession<'a> {
    reader: &'a mut BufReader<OwnedReadHalf>,
    writer: &'a mut OwnedWriteHalf,
}

impl auth::MudSession for TelnetSession<'_> {
    async fn send(&mut self, s: &str) -> Option<()> {
        let rendered = crate::color::render(s);
        self.writer.write_all(rendered.as_bytes()).await.ok()
    }

    async fn recv(&mut self) -> Option<String> {
        let mut buf = String::new();
        match self.reader.read_line(&mut buf).await {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(buf.trim().to_string()),
        }
    }
}
