use crate::character::CharacterData;
use crate::command::ClientId;
use crate::components::{AdminFlag, CharacterId, ClientConnection, Name, Position, Stats};
use crate::world::player_registry::PlayerRegistry;
use crate::world::room::RoomRegistry;
use crate::world::seed::build_starting_rooms;

/// The single authoritative runtime state of the server.
/// Lives entirely inside the game loop task — never shared across threads.
pub struct GameState {
    pub world: hecs::World,
    pub current_tick: u64,
    /// Immutable after startup: all room data loaded from seed / database.
    pub room_registry: RoomRegistry,
    /// Tracks which ECS entity corresponds to each connected client.
    pub player_registry: PlayerRegistry,
    /// Characters waiting to be saved on the next inter-tick async drain.
    pub pending_saves: Vec<CharacterData>,
}

impl GameState {
    /// Builds state using the hardcoded seed world — used by unit tests.
    pub fn new() -> Self {
        Self::with_rooms(build_starting_rooms())
    }

    /// Builds state from a pre-loaded registry — used by the game loop after
    /// loading rooms from the database.
    pub fn with_rooms(room_registry: RoomRegistry) -> Self {
        GameState {
            world: hecs::World::new(),
            current_tick: 0,
            room_registry,
            player_registry: PlayerRegistry::new(),
            pending_saves: Vec::new(),
        }
    }

    /// Advances the engine clock by one tick.
    /// Uses saturating addition so the server can run indefinitely.
    pub fn advance_tick(&mut self) {
        self.current_tick = self.current_tick.saturating_add(1);
    }

    /// Spawns a player entity from DB-loaded (or default) character data
    /// and registers the `ClientId → Entity` mapping.
    pub fn spawn_player_from_data(
        &mut self,
        client_id: ClientId,
        data: &CharacterData,
    ) -> hecs::Entity {
        let entity = self.world.spawn((
            Position {
                room_id: data.room_id,
            },
            Name(data.name.clone()),
            Stats {
                hp: data.hp,
                max_hp: data.max_hp,
                mp: data.mp,
                max_mp: data.max_mp,
            },
            ClientConnection { client_id },
            CharacterId {
                db_id: data.id,
                account_id: data.account_id,
            },
        ));
        if data.is_admin {
            self.world.insert(entity, (AdminFlag,)).ok();
        }
        self.player_registry.register(client_id, entity);
        entity
    }

    /// Convenience wrapper that spawns with default stats at the starting room.
    pub fn spawn_player(&mut self, client_id: ClientId) -> hecs::Entity {
        self.spawn_player_from_data(client_id, &CharacterData::default())
    }

    /// Extracts the current ECS state into a `CharacterData` ready for DB persistence.
    /// Returns `None` if any required component is missing (should not happen in practice).
    pub fn extract_save_data(&self, entity: hecs::Entity) -> Option<CharacterData> {
        let room_id = self.world.get::<&Position>(entity).ok()?.room_id;
        let name = self.world.get::<&Name>(entity).ok()?.0.clone();
        let (hp, max_hp, mp, max_mp) = {
            let s = self.world.get::<&Stats>(entity).ok()?;
            (s.hp, s.max_hp, s.mp, s.max_mp)
        };
        let (id, account_id) = {
            let c = self.world.get::<&CharacterId>(entity).ok()?;
            (c.db_id, c.account_id)
        };
        let is_admin = self.world.get::<&AdminFlag>(entity).is_ok();
        Some(CharacterData {
            id,
            account_id,
            is_admin,
            name,
            room_id,
            hp,
            max_hp,
            mp,
            max_mp,
        })
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Position;
    use crate::world::seed::STARTING_ROOM_ID;

    #[test]
    fn initial_tick_is_zero() {
        assert_eq!(GameState::new().current_tick, 0);
    }

    #[test]
    fn advance_tick_increments_by_one() {
        let mut state = GameState::new();
        state.advance_tick();
        assert_eq!(state.current_tick, 1);
        state.advance_tick();
        assert_eq!(state.current_tick, 2);
    }

    #[test]
    fn advance_tick_saturates_at_u64_max() {
        let mut state = GameState::new();
        state.current_tick = u64::MAX;
        state.advance_tick();
        assert_eq!(state.current_tick, u64::MAX);
    }

    #[test]
    fn world_starts_empty() {
        assert_eq!(GameState::new().world.len(), 0);
    }

    #[test]
    fn room_registry_contains_seed_rooms() {
        let state = GameState::new();
        assert!(state.room_registry.get(STARTING_ROOM_ID).is_some());
    }

    #[test]
    fn spawn_player_from_data_places_entity_at_given_room() {
        let mut state = GameState::new();
        let data = CharacterData {
            name: "Tester".to_string(),
            room_id: 3,
            ..Default::default()
        };
        let entity = state.spawn_player_from_data(1, &data);
        let pos = state.world.get::<&Position>(entity).unwrap();
        assert_eq!(pos.room_id, 3);
    }

    #[test]
    fn spawn_player_registers_client_id() {
        let mut state = GameState::new();
        let entity = state.spawn_player(42);
        assert_eq!(state.player_registry.get_entity(42), Some(entity));
    }

    #[test]
    fn spawn_player_increments_world_len() {
        let mut state = GameState::new();
        state.spawn_player(1);
        state.spawn_player(2);
        assert_eq!(state.world.len(), 2);
    }

    #[test]
    fn extract_save_data_round_trips_character() {
        let mut state = GameState::new();
        let original = CharacterData {
            name: "Hero".to_string(),
            room_id: 2,
            hp: 80,
            max_hp: 100,
            mp: 40,
            max_mp: 50,
            ..Default::default()
        };
        let entity = state.spawn_player_from_data(1, &original);
        let extracted = state.extract_save_data(entity).unwrap();
        assert_eq!(extracted, original);
    }
}
