/// One entry in an NPC's loot table.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct LootEntry {
    /// Template item id — a copy of this item is spawned on drop.
    pub item_id: i64,
    /// Drop probability per kill (0.0 = never, 1.0 = always).
    #[serde(default = "default_chance")]
    pub chance: f32,
}

fn default_chance() -> f32 {
    1.0
}

/// Persistent NPC record — matches the `npcs` DB schema and serves as the
/// seed format (deserialized from `assets/npcs.json`).
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct NpcData {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// What the NPC says when a player talks to them. `None` means silence.
    pub greeting: Option<String>,
    #[serde(default)]
    pub hostile: bool,
    #[serde(default)]
    pub passive: bool,
    pub room_id: u64,
    /// Ordered list of room_ids to patrol; empty means the NPC is stationary.
    #[serde(default)]
    pub patrol: Vec<u64>,
    // ── Combat stats ─────────────────────────────────────────────────────────
    #[serde(default = "default_max_hp")]
    pub max_hp: i32,
    #[serde(default = "default_min_damage")]
    pub min_damage: i32,
    #[serde(default = "default_max_damage")]
    pub max_damage: i32,
    /// Ticks between attacks (200ms/tick → 10 ticks = 2 s).
    #[serde(default = "default_attack_ticks")]
    pub attack_ticks: u64,
    #[serde(default = "default_xp_reward")]
    pub xp_reward: i32,
    /// Items that may drop when this NPC is killed.
    #[serde(default)]
    pub loot_table: Vec<LootEntry>,
}

fn default_max_hp() -> i32 {
    20
}
fn default_min_damage() -> i32 {
    1
}
fn default_max_damage() -> i32 {
    4
}
fn default_attack_ticks() -> u64 {
    10
}
fn default_xp_reward() -> i32 {
    10
}

/// Queued DB write when a patrolling NPC moves to a new room.
/// Drained between ticks by the game loop.
#[derive(Debug, Clone)]
pub struct NpcRoomSave {
    pub npc_id: i64,
    pub room_id: u64,
}

/// Queued NPC respawn. When `respawn_at_tick` is reached the combat system
/// re-spawns the NPC using the original `data`.
pub struct NpcRespawn {
    pub data: NpcData,
    pub respawn_at_tick: u64,
}
