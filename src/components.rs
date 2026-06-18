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
    pub strength: i32,
    pub dexterity: i32,
    pub knowledge: i32,
    pub level: i32,
    pub experience: i32,
    pub learning_points: i32,
}

impl Stats {
    pub fn new(max_hp: i32, max_mp: i32) -> Self {
        Stats {
            hp: max_hp,
            max_hp,
            mp: max_mp,
            max_mp,
            strength: 0,
            dexterity: 0,
            knowledge: 0,
            level: 1,
            experience: 0,
            learning_points: 0,
        }
    }

    pub fn is_alive(&self) -> bool {
        self.hp > 0
    }

    /// XP required to go from the current level to the next.
    pub fn xp_to_next_level(&self) -> i32 {
        self.level * 100
    }

    /// Adds experience and processes any level-ups.
    /// Returns the number of levels gained (usually 0 or 1, rarely more).
    /// On each level-up: +5 max HP, +10 Learning Points. XP carries over.
    pub fn add_experience(&mut self, xp: i32) -> i32 {
        self.experience += xp;
        let mut levels_gained = 0;
        loop {
            let needed = self.xp_to_next_level();
            if self.experience >= needed {
                self.experience -= needed;
                self.level += 1;
                self.max_hp += 5;
                self.learning_points += 10;
                levels_gained += 1;
            } else {
                break;
            }
        }
        levels_gained
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

/// The database primary key of an item entity.
/// Required so location changes can be persisted back to the DB.
pub struct ItemId(pub i64);

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

// ── NPC components ────────────────────────────────────────────────────────────

/// The database primary key of an NPC entity.
pub struct NpcId(pub i64);

/// What an NPC says when a player uses the `talk` command on them.
pub struct NpcGreeting(pub String);

/// Long-form description shown when a player examines the NPC.
pub struct NpcDescription(pub String);

/// Marker: NPC is hostile to players. No combat effect yet — used for display.
pub struct Hostile;

/// Marker: NPC will not retaliate when attacked by a player.
pub struct Passive;

/// Ordered patrol route for an NPC — cycles through room_ids on each routine tick.
pub struct PatrolRoute {
    pub rooms: Vec<u64>,
    /// Current position in `rooms`; advances on each routine fire.
    pub index: usize,
}

// ── Currency component ────────────────────────────────────────────────────────

/// A player's current wealth stored as raw copper (1g = 100s = 10,000c).
pub struct Wallet(pub i64);

// ── Combat components ─────────────────────────────────────────────────────────

/// Items this NPC may drop on death, loaded from the NPC definition.
pub struct NpcLootTable(pub Vec<crate::npc::LootEntry>);

/// Runtime health and combat stats for an NPC entity.
/// Players use the `Stats` component instead.
pub struct NpcCombatStats {
    pub hp: i32,
    pub max_hp: i32,
    pub min_damage: i32,
    pub max_damage: i32,
    /// Ticks between each NPC attack (200ms/tick → 10 ticks = 2 s).
    pub attack_ticks: u64,
    /// XP awarded to the player who delivers the killing blow.
    pub xp_reward: i32,
}

/// Present on any entity currently in active combat.
pub struct InCombat {
    pub target: hecs::Entity,
    pub last_attack_tick: u64,
    /// Ticks between this entity's attacks.
    pub attack_interval: u64,
    /// False when the entity is being attacked but hasn't chosen to fight back yet.
    /// Pass 1 of the combat system skips entities with attacking = false.
    pub attacking: bool,
}

// ── Quest components ──────────────────────────────────────────────────────────

/// All quest states for a player entity. Always present on spawned players.
pub struct PlayerQuests(pub Vec<crate::quest::PlayerQuestState>);

// ── Dialogue components ───────────────────────────────────────────────────────

/// Marks a player as currently mid-conversation with an NPC.
/// Removed automatically when the conversation ends or the player moves.
pub struct InDialogue {
    pub npc_entity: hecs::Entity,
    pub npc_db_id: i64,
    pub node_id: String,
}

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
        let mut s = Stats::new(100, 10);
        s.hp = 0;
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

    #[test]
    fn stats_starts_at_level_one_with_zero_base_stats() {
        let s = Stats::new(100, 10);
        assert_eq!(s.level, 1);
        assert_eq!(s.strength, 0);
        assert_eq!(s.dexterity, 0);
        assert_eq!(s.knowledge, 0);
        assert_eq!(s.experience, 0);
        assert_eq!(s.learning_points, 0);
    }

    #[test]
    fn xp_to_next_level_scales_with_level() {
        let s = Stats::new(100, 10);
        assert_eq!(s.xp_to_next_level(), 100); // level 1 → 2 costs 100
        let mut s2 = s.clone();
        s2.level = 5;
        assert_eq!(s2.xp_to_next_level(), 500);
    }

    #[test]
    fn add_experience_levels_up_and_grants_lp() {
        let mut s = Stats::new(100, 10);
        let gained = s.add_experience(100);
        assert_eq!(gained, 1);
        assert_eq!(s.level, 2);
        assert_eq!(s.learning_points, 10);
        assert_eq!(s.max_hp, 105);
        assert_eq!(s.experience, 0);
    }

    #[test]
    fn add_experience_carries_over_excess_xp() {
        let mut s = Stats::new(100, 10);
        s.add_experience(250); // 100 for lv2, 200 for lv3, leftover = 250 - 100 - 200 = -50... wait
                               // level 1→2: need 100, level 2→3: need 200 → total 300
                               // 250 only covers level 1→2 (100 used, 150 leftover < 200)
        assert_eq!(s.level, 2);
        assert_eq!(s.experience, 150);
    }

    #[test]
    fn add_experience_can_gain_multiple_levels_at_once() {
        let mut s = Stats::new(100, 10);
        s.add_experience(300); // 100 (lv1→2) + 200 (lv2→3) = 300 exactly
        assert_eq!(s.level, 3);
        assert_eq!(s.experience, 0);
        assert_eq!(s.learning_points, 20);
        assert_eq!(s.max_hp, 110);
    }
}
