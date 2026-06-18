use std::collections::{HashMap, VecDeque};

use crate::character::CharacterData;
use crate::command::{ClientId, Command};
use crate::components::{
    AdminFlag, BagCapacity, CharacterId, ClientConnection, Equipped, Hostile, InInventory,
    ItemDescription, ItemId, ItemName, ItemSlot, Name, PlayerQuests, Position, Stats, TwoHanded,
    Wallet,
};
use crate::dialogue::NpcDialogue;
use crate::direction::Direction;
use crate::item::{ItemData, ItemLocation, ItemLocationSave};
use crate::npc::{NpcData, NpcRoomSave};
use crate::quest::{QuestDef, QuestSave};
use crate::world::loader::ItemDef;
use crate::world::player_registry::PlayerRegistry;
use crate::world::room::{Room, RoomRegistry};
use crate::world::seed::build_starting_rooms;

/// A deferred database write queued by an admin command.
/// Drained between ticks by the game loop — analogous to `pending_saves`.
pub enum AdminDbOp {
    CreateRoom(Room),
    UpdateRoom {
        id: u64,
        name: String,
        description: String,
    },
    UpsertExit {
        room_id: u64,
        dir: Direction,
        dest_id: u64,
    },
    DeleteExit {
        room_id: u64,
        dir: Direction,
    },
    CreateItemDef(crate::world::loader::ItemDef),
    CreateItem(ItemData),
    DeleteItem(i64),
    UpdateDefName {
        id: i64,
        name: String,
    },
    UpdateDefDesc {
        id: i64,
        description: String,
    },
    /// `equip_slot` is the raw slot string (e.g. "lefthand"); `None` clears the slot.
    UpdateDefSlot {
        id: i64,
        equip_slot: Option<String>,
    },
    UpdateDefReq {
        id: i64,
        level: i32,
        strength: i32,
        dexterity: i32,
        knowledge: i32,
    },
    UpdateDefBonuses {
        id: i64,
        bonuses: crate::item::ItemBonuses,
    },
    CreateNpc(NpcData),
    DeleteNpc(i64),
    UpdateNpcName {
        id: i64,
        name: String,
    },
    UpdateNpcDesc {
        id: i64,
        description: String,
    },
    /// `greeting` is `None` to clear it (NPC becomes silent).
    UpdateNpcGreet {
        id: i64,
        greeting: Option<String>,
    },
    UpdateNpcHostile {
        id: i64,
        hostile: bool,
    },
    UpdateNpcPassive {
        id: i64,
        passive: bool,
    },
    /// Replaces the patrol route; empty `rooms` clears it.
    SetNpcPatrol {
        id: i64,
        rooms: Vec<u64>,
    },
}

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
    /// Item location changes waiting to be persisted on the next inter-tick drain.
    pub pending_item_saves: Vec<ItemLocationSave>,
    /// Admin world/item/npc operations waiting to be persisted on the next inter-tick drain.
    pub pending_admin_ops: Vec<AdminDbOp>,
    /// NPC patrol room changes waiting to be persisted on the next inter-tick drain.
    pub pending_npc_saves: Vec<NpcRoomSave>,
    /// Next available id for admin-created rooms. Set from DB max at startup.
    pub next_room_id: u64,
    /// Next available id for admin-created items. Set from DB max at startup.
    pub next_item_id: i64,
    /// Next available id for admin-created NPCs. Set from DB max at startup.
    pub next_npc_id: i64,
    /// Next available id for admin-created item definitions. Set from DB max at startup.
    pub next_def_id: i64,
    /// All loaded quest definitions keyed by quest id. Immutable after startup.
    pub quest_defs: HashMap<i64, QuestDef>,
    /// All loaded NPC dialogue trees keyed by npc_db_id. Immutable after startup.
    pub dialogue_defs: HashMap<i64, NpcDialogue>,
    /// Player quest state changes waiting to be persisted on the next inter-tick drain.
    pub pending_quest_saves: Vec<QuestSave>,
    /// Per-player command queues. Each tick processes exactly one command per player.
    pub pending_commands: HashMap<ClientId, VecDeque<Command>>,
    /// NPCs scheduled to respawn. Checked every tick by the combat system.
    pub pending_respawns: Vec<crate::npc::NpcRespawn>,
    /// Item definitions keyed by id, used by the combat system to spawn loot copies.
    pub item_templates: HashMap<i64, ItemDef>,
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
            pending_item_saves: Vec::new(),
            pending_admin_ops: Vec::new(),
            pending_npc_saves: Vec::new(),
            next_room_id: 1001,
            next_item_id: 1001,
            next_npc_id: 1001,
            next_def_id: 1001,
            quest_defs: HashMap::new(),
            dialogue_defs: HashMap::new(),
            pending_quest_saves: Vec::new(),
            pending_commands: HashMap::new(),
            pending_respawns: Vec::new(),
            item_templates: HashMap::new(),
        }
    }

    pub fn is_admin(&self, entity: hecs::Entity) -> bool {
        self.world.get::<&AdminFlag>(entity).is_ok()
    }

    pub fn get_player_room(&self, entity: hecs::Entity) -> Option<u64> {
        self.world.get::<&Position>(entity).ok().map(|p| p.room_id)
    }

    pub fn is_hostile(&self, entity: hecs::Entity) -> bool {
        self.world.get::<&Hostile>(entity).is_ok()
    }

    /// Advances the engine clock by one tick.
    /// Uses saturating addition so the server can run indefinitely.
    pub fn advance_tick(&mut self) {
        self.current_tick = self.current_tick.saturating_add(1);
    }

    /// Spawns a player entity from DB-loaded (or default) character data
    /// and registers the `ClientId → Entity` mapping.
    /// Also spawns ECS entities for each item in `data.items` (inventory + equipped).
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
                strength: data.strength,
                dexterity: data.dexterity,
                knowledge: data.knowledge,
                level: data.level,
                experience: data.experience,
                learning_points: data.learning_points,
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

        self.world
            .insert(entity, (PlayerQuests(data.quests.clone()),))
            .ok();
        self.world.insert(entity, (Wallet(data.copper),)).ok();

        // Spawn persistent items (inventory + equipped).
        for item in &data.items {
            let mut builder = hecs::EntityBuilder::new();
            builder.add(ItemId(item.id));
            builder.add(ItemName(item.name.clone()));
            builder.add(ItemDescription(item.description.clone()));

            match &item.location {
                ItemLocation::Inventory { .. } => {
                    builder.add(InInventory { owner: entity });
                }
                ItemLocation::Equipped { slot, .. } => {
                    builder.add(Equipped {
                        owner: entity,
                        slot: *slot,
                    });
                }
                ItemLocation::Room(_) => {} // shouldn't happen for player items
            }

            if let Some(slot) = item.equip_slot {
                builder.add(ItemSlot(slot));
            }
            if item.two_handed {
                builder.add(TwoHanded);
            }
            if let Some(cap) = item.bag_capacity {
                builder.add(BagCapacity(cap));
            }
            if item.requirements.has_any() {
                builder.add(item.requirements);
            }
            if item.bonuses.has_any() {
                builder.add(item.bonuses);
            }

            self.world.spawn(builder.build());
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
    /// Items are NOT included — they are persisted via `pending_item_saves`.
    pub fn extract_save_data(&self, entity: hecs::Entity) -> Option<CharacterData> {
        let room_id = self.world.get::<&Position>(entity).ok()?.room_id;
        let name = self.world.get::<&Name>(entity).ok()?.0.clone();
        let (
            hp,
            max_hp,
            mp,
            max_mp,
            strength,
            dexterity,
            knowledge,
            level,
            experience,
            learning_points,
        ) = {
            let s = self.world.get::<&Stats>(entity).ok()?;
            (
                s.hp,
                s.max_hp,
                s.mp,
                s.max_mp,
                s.strength,
                s.dexterity,
                s.knowledge,
                s.level,
                s.experience,
                s.learning_points,
            )
        };
        let (id, account_id) = {
            let c = self.world.get::<&CharacterId>(entity).ok()?;
            (c.db_id, c.account_id)
        };
        let is_admin = self.world.get::<&AdminFlag>(entity).is_ok();
        let copper = self
            .world
            .get::<&Wallet>(entity)
            .ok()
            .map(|w| w.0)
            .unwrap_or(0);
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
            strength,
            dexterity,
            knowledge,
            level,
            experience,
            learning_points,
            copper,
            items: Vec::new(),
            quests: Vec::new(),
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
