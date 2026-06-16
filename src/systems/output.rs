use std::collections::HashMap;

use tokio::sync::mpsc;

use crate::command::ClientId;

/// Per-client output senders owned by the game loop.
/// The game loop inserts/removes entries when clients connect and disconnect.
pub type OutputRegistry = HashMap<ClientId, mpsc::Sender<String>>;

/// Phase 5 of each tick: enqueues `message` for delivery to `client_id`.
///
/// Uses `try_send` intentionally — a full buffer means the client is processing
/// too slowly. The message is dropped rather than stalling the tick.
pub fn send_to_client(registry: &OutputRegistry, client_id: ClientId, message: String) {
    if let Some(tx) = registry.get(&client_id) {
        let _ = tx.try_send(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_client(id: ClientId) -> (OutputRegistry, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(16);
        let mut reg = OutputRegistry::new();
        reg.insert(id, tx);
        (reg, rx)
    }

    #[test]
    fn message_arrives_on_receiver() {
        let (reg, mut rx) = single_client(7);
        send_to_client(&reg, 7, "You see a dark forest.".to_string());
        assert_eq!(rx.try_recv().unwrap(), "You see a dark forest.");
    }

    #[test]
    fn multiple_messages_are_ordered() {
        let (reg, mut rx) = single_client(1);
        send_to_client(&reg, 1, "first".to_string());
        send_to_client(&reg, 1, "second".to_string());
        assert_eq!(rx.try_recv().unwrap(), "first");
        assert_eq!(rx.try_recv().unwrap(), "second");
    }

    #[test]
    fn unknown_client_id_is_silently_ignored() {
        let (reg, _rx) = single_client(1);
        // client 99 is not in the registry
        send_to_client(&reg, 99, "this should not panic".to_string());
    }

    #[test]
    fn full_channel_does_not_panic() {
        let (tx, _rx) = mpsc::channel(1);
        let mut reg = OutputRegistry::new();
        reg.insert(1, tx);
        send_to_client(&reg, 1, "fills the channel".to_string());
        send_to_client(&reg, 1, "dropped silently".to_string());
    }
}
