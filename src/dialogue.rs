use serde::Deserialize;

/// Full dialogue tree for one NPC, loaded from `assets/dialogues.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct NpcDialogue {
    pub npc_id: i64,
    pub nodes: Vec<DialogueNode>,
}

impl NpcDialogue {
    pub fn find_node(&self, id: &str) -> Option<&DialogueNode> {
        self.nodes.iter().find(|n| n.id == id)
    }
}

/// A single state in a dialogue tree.
#[derive(Debug, Clone, Deserialize)]
pub struct DialogueNode {
    pub id: String,
    /// What the NPC says when this node is shown.
    pub text: String,
    /// Player-selectable responses. Empty list = conversation ends after text.
    #[serde(default)]
    pub options: Vec<DialogueOption>,
}

/// One player-selectable response inside a `DialogueNode`.
#[derive(Debug, Clone, Deserialize)]
pub struct DialogueOption {
    /// The text the player "says" / the button label.
    pub text: String,
    /// ID of the node to navigate to, or `null` to end the conversation.
    pub goto: Option<String>,
    /// Side-effects applied when this option is chosen, in order.
    #[serde(default)]
    pub effects: Vec<DialogueEffect>,
    /// All conditions must hold for this option to be visible.
    #[serde(default)]
    pub conditions: Vec<DialogueCondition>,
}

/// A side-effect that fires when a `DialogueOption` is chosen.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DialogueEffect {
    /// Accept a specific quest (no-op if already in log).
    AcceptQuest { quest_id: i64 },
    /// Mark all Talk objectives for this NPC on the specified quest.
    MarkObjective { quest_id: i64 },
    /// Turn in any quest that is `ReadyToTurnIn` for this NPC.
    TurnInQuest,
    /// Move a world item into the player's inventory.
    GiveItem { item_id: i64 },
}

/// A condition that must be true for a `DialogueOption` to appear.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DialogueCondition {
    /// Player does not have this quest in their log at all.
    QuestNotStarted { quest_id: i64 },
    /// Player has this quest with status Active.
    QuestActive { quest_id: i64 },
    /// Player is on the given 0-based phase of this quest (Active).
    QuestPhase { quest_id: i64, phase: usize },
    /// Player has this quest with status ReadyToTurnIn.
    QuestReady { quest_id: i64 },
    /// Player has this quest ReadyToTurnIn specifically at the given 0-based phase.
    QuestReadyAtPhase { quest_id: i64, phase: usize },
    /// Player has completed this quest.
    QuestComplete { quest_id: i64 },
    /// Player's level is at least this value.
    MinLevel { level: i32 },
}
