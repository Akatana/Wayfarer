/// Runtime representation of a character's persistent state.
///
/// Created by the session handler after auth + character selection, then sent
/// to the game loop via `Command::Connect(CharacterData)` so the ECS entity
/// can be spawned without any async work inside the tick body.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterData {
    pub id: i64,
    pub account_id: i64,
    pub is_admin: bool,
    pub name: String,
    pub room_id: u64,
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
    /// Items in the character's inventory or equipped — loaded from DB at login.
    pub items: Vec<crate::item::ItemData>,
}

impl Default for CharacterData {
    fn default() -> Self {
        CharacterData {
            id: 0,
            account_id: 0,
            is_admin: false,
            name: "Adventurer".to_string(),
            room_id: crate::world::seed::STARTING_ROOM_ID,
            hp: 100,
            max_hp: 100,
            mp: 10,
            max_mp: 10,
            strength: 0,
            dexterity: 0,
            knowledge: 0,
            level: 1,
            experience: 0,
            learning_points: 0,
            items: Vec::new(),
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

    #[test]
    fn default_is_not_admin() {
        assert!(!CharacterData::default().is_admin);
    }

    #[test]
    fn default_starts_at_level_one_with_zero_base_stats() {
        let d = CharacterData::default();
        assert_eq!(d.level, 1);
        assert_eq!(d.strength, 0);
        assert_eq!(d.dexterity, 0);
        assert_eq!(d.knowledge, 0);
        assert_eq!(d.experience, 0);
        assert_eq!(d.learning_points, 0);
    }
}
