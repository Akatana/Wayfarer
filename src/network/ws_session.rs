use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};

use super::{auth, parser};
use crate::command::{ClientId, Command, PlayerInput};

type WsSink = SplitSink<WebSocketStream<TcpStream>, Message>;
type WsStream = SplitStream<WebSocketStream<TcpStream>>;

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

    let account = {
        let mut s = WsSession {
            sink: &mut sink,
            stream: &mut stream,
        };
        auth::auth_phase(&mut s, &db).await
    };
    let Some(account) = account else { return };

    let char_data = {
        let mut s = WsSession {
            sink: &mut sink,
            stream: &mut stream,
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

    println!("[WS] Client {client_id} disconnected.");
    let _ = sink.send(Message::Close(None)).await;
    let _ = command_tx
        .send(PlayerInput::new(client_id, Command::Quit))
        .await;
    let _ = deregister_tx.send(client_id).await;
}

struct WsSession<'a> {
    sink: &'a mut WsSink,
    stream: &'a mut WsStream,
}

impl auth::MudSession for WsSession<'_> {
    async fn send(&mut self, s: &str) -> Option<()> {
        self.sink
            .send(Message::Text(crate::color::render(s)))
            .await
            .ok()
    }

    async fn recv(&mut self) -> Option<String> {
        loop {
            match self.stream.next().await {
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
}
