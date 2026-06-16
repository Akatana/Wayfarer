use std::collections::HashMap;

use crate::direction::Direction;

/// A one-way directional link between two rooms.
pub struct Exit {
    pub destination_room_id: u64,
}

/// A discrete location in the game world. The world is a graph of rooms
/// connected by `Exit`s — there are no continuous coordinates.
pub struct Room {
    pub id: u64,
    pub name: String,
    pub description: String,
    /// Direction → exit. A missing entry means that passage is sealed.
    pub exits: HashMap<Direction, Exit>,
}

impl Room {
    /// Generates the player-visible room description including available exits.
    pub fn describe(&self) -> String {
        let mut labels: Vec<String> = self.exits.keys().map(|d| d.to_string()).collect();
        labels.sort_unstable(); // deterministic display order
        format!(
            "{}\n   {}\n[ Exits: {} ]",
            self.name,
            self.description,
            if labels.is_empty() { "none".to_string() } else { labels.join(", ") }
        )
    }
}

/// In-memory store of all rooms, built from the database (or seed data) at startup.
///
/// The registry is immutable after startup and requires no locking — all
/// runtime lookups take `&self`.
#[derive(Default)]
pub struct RoomRegistry {
    rooms: HashMap<u64, Room>,
}

impl RoomRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a room, replacing any existing entry with the same id.
    pub fn insert(&mut self, room: Room) {
        self.rooms.insert(room.id, room);
    }

    pub fn get(&self, room_id: u64) -> Option<&Room> {
        self.rooms.get(&room_id)
    }

    /// Returns the destination room_id reachable via `direction` from `room_id`,
    /// or `None` if no such exit exists.
    pub fn resolve_exit(&self, room_id: u64, direction: Direction) -> Option<u64> {
        self.rooms
            .get(&room_id)?
            .exits
            .get(&direction)
            .map(|e| e.destination_room_id)
    }

    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Room> {
        self.rooms.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry() -> RoomRegistry {
        let mut reg = RoomRegistry::new();
        reg.insert(Room {
            id: 1,
            name: "Room A".to_string(),
            description: "A plain room.".to_string(),
            exits: HashMap::from([
                (Direction::North, Exit { destination_room_id: 2 }),
                (Direction::East, Exit { destination_room_id: 3 }),
            ]),
        });
        reg.insert(Room {
            id: 2,
            name: "Room B".to_string(),
            description: "A second room.".to_string(),
            exits: HashMap::from([(Direction::South, Exit { destination_room_id: 1 })]),
        });
        reg
    }

    #[test]
    fn resolves_existing_exit() {
        let reg = make_registry();
        assert_eq!(reg.resolve_exit(1, Direction::North), Some(2));
        assert_eq!(reg.resolve_exit(1, Direction::East), Some(3));
    }

    #[test]
    fn returns_none_for_missing_exit() {
        let reg = make_registry();
        assert_eq!(reg.resolve_exit(1, Direction::South), None);
    }

    #[test]
    fn returns_none_for_unknown_room() {
        let reg = make_registry();
        assert_eq!(reg.resolve_exit(999, Direction::North), None);
    }

    #[test]
    fn get_returns_room_by_id() {
        let reg = make_registry();
        assert!(reg.get(1).is_some());
        assert!(reg.get(999).is_none());
    }

    #[test]
    fn describe_contains_room_name_and_exits() {
        let reg = make_registry();
        let desc = reg.get(1).unwrap().describe();
        assert!(desc.contains("Room A"));
        assert!(desc.contains("north"));
        assert!(desc.contains("east"));
    }

    #[test]
    fn describe_shows_none_when_no_exits() {
        let mut reg = RoomRegistry::new();
        reg.insert(Room {
            id: 10,
            name: "Dead End".to_string(),
            description: "Walls on all sides.".to_string(),
            exits: HashMap::new(),
        });
        assert!(reg.get(10).unwrap().describe().contains("none"));
    }
}
