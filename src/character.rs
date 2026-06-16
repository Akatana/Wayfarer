/// Runtime representation of a character's persistent state.
///
/// Created by the session handler after a DB load-or-create, then sent to the
/// game loop via `Command::Connect(CharacterData)` so the ECS entity can be
/// spawned without any async work inside the tick body.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterData {
    pub name: String,
    pub room_id: u64,
    pub hp: i32,
    pub max_hp: i32,
    pub mp: i32,
    pub max_mp: i32,
}

impl Default for CharacterData {
    fn default() -> Self {
        CharacterData {
            name: "Adventurer".to_string(),
            room_id: crate::world::seed::STARTING_ROOM_ID,
            hp: 100,
            max_hp: 100,
            mp: 50,
            max_mp: 50,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_starts_at_full_health() {
        let d = CharacterData::default();
        assert_eq!(d.hp, d.max_hp);
        assert_eq!(d.mp, d.max_mp);
    }

    #[test]
    fn default_spawns_at_starting_room() {
        use crate::world::seed::STARTING_ROOM_ID;
        assert_eq!(CharacterData::default().room_id, STARTING_ROOM_ID);
    }
}
