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
    /// Initiate a graceful disconnect and save.
    Quit,
    /// List all online players (admin only).
    AdminWho,
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
