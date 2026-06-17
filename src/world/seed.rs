use crate::components::{
    BagCapacity, ItemDescription, ItemId, ItemName, ItemSlot, RoomContents, TwoHanded,
};
use crate::item::{ItemData, ItemLocation};
use crate::world::loader;
use crate::world::room::RoomRegistry;

/// The room_id where newly spawned players first appear.
pub const STARTING_ROOM_ID: u64 = 1;

/// Returns the room registry built from the JSON room files.
///
/// Delegates to `loader::load_rooms`; kept as a named function so callers
/// (including tests) have a stable import path.
pub fn build_starting_rooms() -> RoomRegistry {
    loader::load_rooms(std::path::Path::new("assets/rooms"))
}

/// Spawns room-item entities into the ECS world from DB-loaded `ItemData`.
///
/// Called once from `game_loop::run()` after the DB is seeded and room items
/// are fetched. Only items whose location is `Room(_)` are expected here.
pub fn spawn_items(world: &mut hecs::World, items: &[ItemData]) {
    for item in items {
        let room_id = match item.location {
            ItemLocation::Room(id) => id,
            _ => continue, // skip non-room items (shouldn't happen at startup)
        };

        let mut builder = hecs::EntityBuilder::new();
        builder.add(ItemId(item.id));
        builder.add(ItemName(item.name.clone()));
        builder.add(ItemDescription(item.description.clone()));
        builder.add(RoomContents { room_id });

        if let Some(slot) = item.equip_slot {
            builder.add(ItemSlot(slot));
        }
        if item.two_handed {
            builder.add(TwoHanded);
        }
        if let Some(cap) = item.bag_capacity {
            builder.add(BagCapacity(cap));
        }
        if item.requirements.has_any() {
            builder.add(item.requirements.clone());
        }

        world.spawn(builder.build());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::direction::Direction;
    use crate::item::{EquipRequirements, EquipSlot, ItemLocation};

    fn make_item(id: i64, room_id: u64) -> ItemData {
        ItemData {
            id,
            name: format!("item {id}"),
            description: "A test item.".to_string(),
            equip_slot: None,
            two_handed: false,
            bag_capacity: None,
            requirements: EquipRequirements::default(),
            location: ItemLocation::Room(room_id),
        }
    }

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

    #[test]
    fn spawn_items_creates_entities_for_each_room_item() {
        let items = vec![make_item(1, 1), make_item(2, 1), make_item(3, 2)];
        let mut world = hecs::World::new();
        spawn_items(&mut world, &items);
        assert_eq!(world.len(), 3);
    }

    #[test]
    fn spawn_items_attaches_item_id_component() {
        let items = vec![make_item(42, 1)];
        let mut world = hecs::World::new();
        spawn_items(&mut world, &items);
        let id = world
            .query::<(&ItemId,)>()
            .iter()
            .next()
            .map(|(_, (id,))| id.0)
            .unwrap();
        assert_eq!(id, 42);
    }

    #[test]
    fn spawn_items_attaches_equip_slot() {
        let mut item = make_item(1, 1);
        item.equip_slot = Some(EquipSlot::LeftHand);
        let mut world = hecs::World::new();
        spawn_items(&mut world, &[item]);
        let has_slot = world.query::<(&ItemSlot,)>().iter().next().is_some();
        assert!(has_slot);
    }

    #[test]
    fn spawn_items_skips_non_room_items() {
        let item = ItemData {
            id: 1,
            name: "carried".to_string(),
            description: "In inventory.".to_string(),
            equip_slot: None,
            two_handed: false,
            bag_capacity: None,
            requirements: EquipRequirements::default(),
            location: ItemLocation::Inventory { char_id: 1 },
        };
        let mut world = hecs::World::new();
        spawn_items(&mut world, &[item]);
        assert_eq!(world.len(), 0);
    }
}
