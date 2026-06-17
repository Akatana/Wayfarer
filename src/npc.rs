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
    pub room_id: u64,
    /// Ordered list of room_ids to patrol; empty means the NPC is stationary.
    #[serde(default)]
    pub patrol: Vec<u64>,
}

/// Queued DB write when a patrolling NPC moves to a new room.
/// Drained between ticks by the game loop.
#[derive(Debug, Clone)]
pub struct NpcRoomSave {
    pub npc_id: i64,
    pub room_id: u64,
}
