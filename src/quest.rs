use serde::Deserialize;

/// Static definition of a quest loaded from `assets/quests.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct QuestDef {
    pub id: i64,
    pub name: String,
    pub description: String,
    /// NPC that gives this quest (shows [!] marker). `None` = item-triggered only.
    pub giver_npc_id: Option<i64>,
    /// Item that triggers this quest when examined. `None` = NPC-given only.
    pub giver_item_id: Option<i64>,
    pub phases: Vec<QuestPhaseDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuestPhaseDef {
    pub description: String,
    pub objectives: Vec<QuestObjectiveDef>,
    /// NPC the player must talk to in order to complete this phase.
    /// `None` = auto-completes when all objectives are met.
    pub completion_npc_id: Option<i64>,
    /// Text the NPC says when the phase is turned in.
    #[serde(default)]
    pub completion_text: String,
    #[serde(default)]
    pub xp_reward: i32,
    #[serde(default)]
    pub lp_reward: i32,
    /// Copper awarded on phase completion (1g = 100s = 10,000c).
    #[serde(default)]
    pub copper_reward: i64,
    /// IDs of existing items to transfer to the player's inventory on phase completion.
    #[serde(default)]
    pub item_rewards: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QuestObjectiveDef {
    Talk { npc_id: i64, description: String },
    Examine { item_id: i64, description: String },
    Reach { room_id: u64, description: String },
}

impl QuestObjectiveDef {
    pub fn description(&self) -> &str {
        match self {
            QuestObjectiveDef::Talk { description, .. } => description,
            QuestObjectiveDef::Examine { description, .. } => description,
            QuestObjectiveDef::Reach { description, .. } => description,
        }
    }
}

/// A single player's runtime progress through one quest.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerQuestState {
    pub quest_id: i64,
    pub phase: usize,
    /// One bool per objective in the current phase; `true` = completed.
    pub objectives_met: Vec<bool>,
    pub status: QuestStatus,
}

impl PlayerQuestState {
    pub fn new_active(quest_id: i64, num_objectives: usize) -> Self {
        Self {
            quest_id,
            phase: 0,
            objectives_met: vec![false; num_objectives],
            status: QuestStatus::Active,
        }
    }

    pub fn all_objectives_met(&self) -> bool {
        self.objectives_met.is_empty() || self.objectives_met.iter().all(|&m| m)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum QuestStatus {
    Active,
    /// All objectives done; player needs to talk to the completion NPC.
    ReadyToTurnIn,
    Completed,
}

/// Pending DB write for a changed player quest state. Drained between ticks.
pub struct QuestSave {
    pub char_id: i64,
    pub state: PlayerQuestState,
}
