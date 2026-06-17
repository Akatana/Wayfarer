use crate::components::Position;
use crate::direction::Direction;
use crate::world::room::RoomRegistry;

/// Phase 2 of each tick: resolves whether `entity` can exit via `direction`.
///
/// Returns the new `Position` on success, or `None` if the exit is blocked,
/// absent from the room graph, or the entity has no `Position` component.
/// The caller is responsible for writing the returned position back to the ECS.
///
/// No `.await` calls are permitted here.
pub fn try_move(
    world: &hecs::World,
    room_registry: &RoomRegistry,
    entity: hecs::Entity,
    direction: Direction,
) -> Option<Position> {
    // Scope the Ref so the world borrow is released before we return.
    let current_room_id = {
        let pos = world.get::<&Position>(entity).ok()?;
        pos.room_id
    };
    let new_room_id = room_registry.resolve_exit(current_room_id, direction)?;
    Some(Position {
        room_id: new_room_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::room::{Exit, Room, RoomRegistry};
    use std::collections::HashMap;

    fn two_room_registry() -> RoomRegistry {
        let mut reg = RoomRegistry::new();
        reg.insert(Room {
            id: 1,
            name: "Start".to_string(),
            description: "A room.".to_string(),
            exits: HashMap::from([(
                Direction::North,
                Exit {
                    destination_room_id: 2,
                },
            )]),
        });
        reg.insert(Room {
            id: 2,
            name: "North Room".to_string(),
            description: "Another room.".to_string(),
            exits: HashMap::from([(
                Direction::South,
                Exit {
                    destination_room_id: 1,
                },
            )]),
        });
        reg
    }

    #[test]
    fn returns_new_position_for_valid_exit() {
        let mut world = hecs::World::new();
        let reg = two_room_registry();
        let entity = world.spawn((Position { room_id: 1 },));

        assert_eq!(
            try_move(&world, &reg, entity, Direction::North),
            Some(Position { room_id: 2 })
        );
    }

    #[test]
    fn returns_none_for_blocked_direction() {
        let mut world = hecs::World::new();
        let reg = two_room_registry();
        let entity = world.spawn((Position { room_id: 1 },));

        assert!(try_move(&world, &reg, entity, Direction::South).is_none());
    }

    #[test]
    fn returns_none_when_entity_has_no_position() {
        let mut world = hecs::World::new();
        let reg = two_room_registry();
        let entity = world.spawn(());

        assert!(try_move(&world, &reg, entity, Direction::North).is_none());
    }

    #[test]
    fn round_trip_north_then_south_returns_to_origin() {
        let mut world = hecs::World::new();
        let reg = two_room_registry();
        let entity = world.spawn((Position { room_id: 1 },));

        let north = try_move(&world, &reg, entity, Direction::North).unwrap();
        assert_eq!(north.room_id, 2);

        // Apply the position, then move south.
        if let Ok(mut pos) = world.get::<&mut Position>(entity) {
            *pos = north;
        }
        let south = try_move(&world, &reg, entity, Direction::South).unwrap();
        assert_eq!(south.room_id, 1);
    }
}
