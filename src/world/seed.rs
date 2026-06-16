use std::collections::HashMap;

use crate::direction::Direction;
use crate::world::room::{Exit, Room, RoomRegistry};

/// The room_id where newly spawned players first appear.
pub const STARTING_ROOM_ID: u64 = 1;

/// Builds the initial room graph used during development.
///
/// In production this will be replaced by a database load on startup.
/// The layout is intentionally kept small so all exits can be verified
/// in tests without external fixtures.
///
/// ```text
///   [4: Old Road]
///        |  S/N
///   [2: North Gate]
///        |  S/N
///   [1: Town Square] --E/W-- [3: Market Street]
/// ```
pub fn build_starting_rooms() -> RoomRegistry {
    let mut registry = RoomRegistry::new();

    registry.insert(Room {
        id: 1,
        name: "Town Square".to_string(),
        description:
            "A cobblestone square at the heart of the village. An old well sits at its centre."
                .to_string(),
        exits: HashMap::from([
            (Direction::North, Exit { destination_room_id: 2 }),
            (Direction::East, Exit { destination_room_id: 3 }),
        ]),
    });

    registry.insert(Room {
        id: 2,
        name: "North Gate".to_string(),
        description:
            "Tall oak gates mark the northern boundary. A drowsy guard leans on his halberd."
                .to_string(),
        exits: HashMap::from([
            (Direction::South, Exit { destination_room_id: 1 }),
            (Direction::North, Exit { destination_room_id: 4 }),
        ]),
    });

    registry.insert(Room {
        id: 3,
        name: "Market Street".to_string(),
        description:
            "Stalls line the narrow street — bread, leather, and curiosities jostle for space."
                .to_string(),
        exits: HashMap::from([(Direction::West, Exit { destination_room_id: 1 })]),
    });

    registry.insert(Room {
        id: 4,
        name: "Old Road".to_string(),
        description:
            "A dirt road winds north through tall grass. The village bell fades behind you."
                .to_string(),
        exits: HashMap::from([(Direction::South, Exit { destination_room_id: 2 })]),
    });

    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_contains_at_least_four_rooms() {
        assert!(build_starting_rooms().len() >= 4);
    }

    #[test]
    fn starting_room_exists_in_registry() {
        let registry = build_starting_rooms();
        assert!(
            registry.get(STARTING_ROOM_ID).is_some(),
            "STARTING_ROOM_ID {STARTING_ROOM_ID} must be present"
        );
    }

    #[test]
    fn every_exit_points_to_an_existing_room() {
        let registry = build_starting_rooms();
        for room_id in [1u64, 2, 3, 4] {
            let room = registry.get(room_id).unwrap();
            for (dir, exit) in &room.exits {
                assert!(
                    registry.get(exit.destination_room_id).is_some(),
                    "Room {room_id} exit {dir} → {} does not exist",
                    exit.destination_room_id
                );
            }
        }
    }

    #[test]
    fn north_from_town_square_reaches_north_gate() {
        let registry = build_starting_rooms();
        assert_eq!(
            registry.resolve_exit(STARTING_ROOM_ID, Direction::North),
            Some(2)
        );
    }
}
