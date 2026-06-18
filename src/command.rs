use crate::character::CharacterData;
use crate::direction::Direction;

/// Opaque identifier for a connected network client, assigned at accept time.
pub type ClientId = u64;

/// A parsed command paired with the client that issued it.
#[derive(Debug, Clone)]
pub struct PlayerInput {
    pub client_id: ClientId,
    pub command: Command,
}

impl PlayerInput {
    pub fn new(client_id: ClientId, command: Command) -> Self {
        PlayerInput { client_id, command }
    }
}

/// All actions a connected client can send to the game engine.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Auth: the session handler has loaded/created the character and is
    /// handing it to the game loop to spawn the ECS entity.
    Connect(CharacterData),
    /// Attempt to move the player's character in the given direction.
    Move(Direction),
    /// Broadcast a message to all entities in the same room.
    Say(String),
    /// Request a description of the current room.
    Look,
    /// Pick up a named item from the floor of the current room.
    Get(String),
    /// Drop a named item from the bag onto the floor.
    Drop(String),
    /// List bag contents and equipped items.
    Inventory,
    /// Equip a named item from the bag.
    Equip(String),
    /// Move an equipped item back into the bag (by slot name or item name).
    Unequip(String),
    /// Read the description of an item (floor, bag, or equipped).
    Examine(String),
    /// Display the player's own stats (HP, MP, location).
    Score,
    /// Initiate a graceful disconnect and save.
    Quit,
    /// Show command help. `None` = full listing; `Some(topic)` = detail for that command.
    Help(Option<String>),
    /// List all online players (admin only).
    AdminWho,
    /// Teleport to a room by id (admin only).
    AdminGoto(u64),
    /// Carve a new room in a direction and link it bidirectionally (admin only).
    AdminDig(Direction, String),
    /// Add an exit from the current room to an existing room (admin only).
    AdminLink(Direction, u64),
    /// Remove an exit from the current room (admin only).
    AdminUnlink(Direction),
    /// Rename the current room (admin only).
    AdminRename(String),
    /// Change the description of the current room (admin only).
    AdminRedesc(String),
    /// Show the room id, name, desc, and exits with destination ids (admin only).
    AdminRoomInfo,
    /// Materialize a new item in the current room (admin only): "name / description".
    AdminMitem(String),
    /// Remove and permanently delete a named item from the current room (admin only).
    AdminDestroy(String),
    /// Rename an item by id (admin only).
    AdminIname(i64, String),
    /// Set an item's description by id (admin only).
    AdminIdesc(i64, String),
    /// Set or clear an item's equip slot by id — "none" clears it (admin only).
    AdminIslot(i64, String),
    /// Set one stat requirement on an item by id: stat ∈ {str, dex, knw, level} (admin only).
    AdminIreq(i64, String, i32),
    /// Initiate combat with a named NPC in the current room.
    Attack(String),
    /// Break off combat and flee to a random exit.
    Flee,
    /// Start a conversation with a named NPC in the current room.
    Talk(String),
    /// Create an NPC in the current room (admin): "name [/ description]".
    AdminMnpc(String),
    /// Destroy a named NPC in the current room (admin).
    AdminNdestroy(String),
    /// Rename an NPC by id (admin).
    AdminNname(i64, String),
    /// Set an NPC's description by id (admin).
    AdminNdesc(i64, String),
    /// Set an NPC's greeting by id — "none" clears it (admin).
    AdminNgreet(i64, String),
    /// Toggle an NPC's hostile flag by id (admin): "true" or "false".
    AdminNhostile(i64, bool),
    /// Toggle an NPC's passive flag by id (admin): "true" or "false".
    AdminNpassive(i64, bool),
    /// Set an NPC's patrol route by id — comma-separated room ids or "none" to clear (admin).
    AdminNpatrol(i64, String),
    /// List all NPCs with their ids and current rooms (admin).
    AdminNlist,
    /// Show detailed info on an NPC by id (admin).
    AdminNinfo(i64),
    /// Show the player's current wallet balance.
    Balance,
    /// Display the player's active quests.
    QuestLog,
    /// List all loaded quest definitions (admin).
    AdminQlist,
    /// Show full details of a quest definition by id (admin).
    AdminQinfo(i64),
    /// Give a quest to an online player by name (admin).
    AdminQgive(String, i64),
    /// Reset an online player's quest to its start (admin).
    AdminQreset(String, i64),
    /// A numeric dialogue choice (1–9) typed while in conversation with an NPC.
    DialogueChoice(usize),
    /// Input that could not be mapped to a known command.
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_input_stores_client_id_and_command() {
        let input = PlayerInput::new(42, Command::Look);
        assert_eq!(input.client_id, 42);
        assert_eq!(input.command, Command::Look);
    }

    #[test]
    fn move_command_carries_direction() {
        let cmd = Command::Move(Direction::North);
        assert_eq!(cmd, Command::Move(Direction::North));
        assert_ne!(cmd, Command::Move(Direction::South));
    }

    #[test]
    fn say_command_carries_message() {
        assert_eq!(
            Command::Say("hello".to_string()),
            Command::Say("hello".to_string())
        );
    }

    #[test]
    fn connect_command_carries_character_data() {
        let data = CharacterData::default();
        let cmd = Command::Connect(data.clone());
        assert_eq!(cmd, Command::Connect(data));
    }

    #[test]
    fn unknown_command_carries_raw_input() {
        assert_eq!(
            Command::Unknown("xyzzy".to_string()),
            Command::Unknown("xyzzy".to_string())
        );
    }
}
