use crate::command::ClientId;

/// The room an entity currently occupies. All spatial queries use room_id,
/// not continuous coordinates — the world is a graph of discrete rooms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub room_id: u64,
}

/// Human-readable identifier used for all player-facing output.
#[derive(Debug, Clone)]
pub struct Name(pub String);

/// Tracks when an NPC last executed its scheduled behaviour so the
/// npc_routine_system can fire it at the correct interval.
#[derive(Debug, Clone)]
pub struct NpcRoutine {
    pub last_action_tick: u64,
}

/// Core combat and resource statistics.
#[derive(Debug, Clone)]
pub struct Stats {
    pub hp: i32,
    pub max_hp: i32,
    pub mp: i32,
    pub max_mp: i32,
}

impl Stats {
    pub fn new(max_hp: i32, max_mp: i32) -> Self {
        Stats {
            hp: max_hp,
            max_hp,
            mp: max_mp,
            max_mp,
        }
    }

    pub fn is_alive(&self) -> bool {
        self.hp > 0
    }
}

/// Links an ECS entity to the network client driving it.
/// Only player-controlled entities carry this component.
#[derive(Debug, Clone, Copy)]
pub struct ClientConnection {
    pub client_id: ClientId,
}

/// Stores the database identifiers for a player entity so saves can be keyed
/// by id rather than name, and so the account relationship is preserved.
pub struct CharacterId {
    pub db_id: i64,
    pub account_id: i64,
}

/// Marker component present only on entities whose account has `is_admin = true`.
/// Guards access to privileged in-game commands.
pub struct AdminFlag;

// ── Item components ───────────────────────────────────────────────────────────

/// Player-facing name of an item entity (e.g. "a rusty sword").
pub struct ItemName(pub String);

/// Long-form text shown by the `examine` command.
pub struct ItemDescription(pub String);

/// The slot this item occupies when equipped. Items without this component
/// cannot be equipped (they are junk / currency / quest items).
pub struct ItemSlot(pub crate::item::EquipSlot);

/// Marker: weapon requires both hands; blocks the RightHand slot while equipped.
pub struct TwoHanded;

/// Item location — lying on the floor of a room.
pub struct RoomContents {
    pub room_id: u64,
}

/// Item location — in a player's bag (not equipped, counts toward the 20-slot limit).
pub struct InInventory {
    pub owner: hecs::Entity,
}

/// Item location — worn or wielded by a player in a specific equipment slot.
pub struct Equipped {
    pub owner: hecs::Entity,
    pub slot: crate::item::EquipSlot,
}

/// How many extra inventory slots this bag grants when equipped.
pub struct BagCapacity(pub usize);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_new_initialises_at_full() {
        let s = Stats::new(100, 50);
        assert_eq!(s.hp, s.max_hp);
        assert_eq!(s.mp, s.max_mp);
    }

    #[test]
    fn stats_is_alive_returns_false_at_zero_hp() {
        let s = Stats {
            hp: 0,
            max_hp: 100,
            mp: 50,
            max_mp: 50,
        };
        assert!(!s.is_alive());
    }

    #[test]
    fn stats_is_alive_returns_true_when_hp_positive() {
        let s = Stats::new(1, 0);
        assert!(s.is_alive());
    }

    #[test]
    fn position_stores_room_id() {
        let p = Position { room_id: 99 };
        assert_eq!(p.room_id, 99);
    }
}
