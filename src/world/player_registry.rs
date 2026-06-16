use std::collections::HashMap;

use crate::command::ClientId;

/// Bidirectional map between connected network clients and their ECS entities.
///
/// Entries are inserted on player spawn and removed on `Quit` or disconnect.
/// All lookups are O(1).
#[derive(Default)]
pub struct PlayerRegistry {
    clients: HashMap<ClientId, hecs::Entity>,
}

impl PlayerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, client_id: ClientId, entity: hecs::Entity) {
        self.clients.insert(client_id, entity);
    }

    pub fn get_entity(&self, client_id: ClientId) -> Option<hecs::Entity> {
        self.clients.get(&client_id).copied()
    }

    /// Removes the mapping and returns the entity so the caller can despawn it.
    pub fn remove(&mut self, client_id: ClientId) -> Option<hecs::Entity> {
        self.clients.remove(&client_id)
    }

    pub fn is_connected(&self, client_id: ClientId) -> bool {
        self.clients.contains_key(&client_id)
    }

    pub fn len(&self) -> usize {
        self.clients.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn() -> (hecs::World, hecs::Entity) {
        let mut world = hecs::World::new();
        let entity = world.spawn(());
        (world, entity)
    }

    #[test]
    fn register_then_lookup_returns_same_entity() {
        let (_, entity) = spawn();
        let mut reg = PlayerRegistry::new();
        reg.register(1, entity);
        assert_eq!(reg.get_entity(1), Some(entity));
    }

    #[test]
    fn lookup_missing_client_returns_none() {
        let reg = PlayerRegistry::new();
        assert_eq!(reg.get_entity(99), None);
    }

    #[test]
    fn is_connected_reflects_registration_state() {
        let (_, entity) = spawn();
        let mut reg = PlayerRegistry::new();
        assert!(!reg.is_connected(1));
        reg.register(1, entity);
        assert!(reg.is_connected(1));
    }

    #[test]
    fn remove_returns_entity_and_clears_mapping() {
        let (_, entity) = spawn();
        let mut reg = PlayerRegistry::new();
        reg.register(5, entity);
        assert_eq!(reg.remove(5), Some(entity));
        assert!(!reg.is_connected(5));
    }

    #[test]
    fn remove_missing_returns_none() {
        let mut reg = PlayerRegistry::new();
        assert_eq!(reg.remove(77), None);
    }

    #[test]
    fn len_tracks_active_connections() {
        let (mut world, _) = spawn();
        let mut reg = PlayerRegistry::new();
        reg.register(1, world.spawn(()));
        reg.register(2, world.spawn(()));
        assert_eq!(reg.len(), 2);
        reg.remove(1);
        assert_eq!(reg.len(), 1);
    }
}
