use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::direction::Direction;
use crate::item::EquipRequirements;
use crate::npc::NpcData;
use crate::world::room::{Exit, Room, RoomRegistry};

// ── Serde DTOs ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RoomFile {
    id: u64,
    name: String,
    description: String,
    #[serde(default)]
    exits: HashMap<String, u64>,
    /// Item IDs that start in this room.
    #[serde(default)]
    items: Vec<i64>,
}

/// A single item definition from `assets/items.json`.
///
/// Items carry a stable `id` — the same value is used as the DB primary key
/// so that room files can reference items by ID rather than by position.
#[derive(Deserialize, Debug, Clone)]
pub struct ItemDef {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub equip_slot: Option<String>,
    #[serde(default)]
    pub two_handed: bool,
    pub bag_capacity: Option<usize>,
    #[serde(default)]
    pub requirements: EquipRequirements,
}

/// Output of `load_seed` — everything needed to bootstrap the world on first boot.
pub struct WorldSeed {
    pub rooms: RoomRegistry,
    /// room_id → list of item IDs that start in that room.
    pub room_items: HashMap<u64, Vec<i64>>,
    pub item_defs: Vec<ItemDef>,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Loads rooms + item placement from JSON files in a single pass.
///
/// Call this once at startup; use the returned `WorldSeed` to seed the DB
/// and build the initial ECS world.
pub fn load_seed(rooms_dir: &Path, items_path: &Path) -> WorldSeed {
    let (rooms, room_items) = load_rooms_internal(rooms_dir);
    let item_defs = load_items(items_path);
    WorldSeed {
        rooms,
        room_items,
        item_defs,
    }
}

/// Loads all `*.json` files from `rooms_dir` into a `RoomRegistry`.
///
/// Files are processed in filename order so load order is deterministic.
/// Panics on malformed JSON or an unreadable directory.
pub fn load_rooms(rooms_dir: &Path) -> RoomRegistry {
    load_rooms_internal(rooms_dir).0
}

/// Loads the item list from `items_path` (a JSON array of `ItemDef`).
///
/// Panics on malformed JSON or an unreadable file.
pub fn load_items(items_path: &Path) -> Vec<ItemDef> {
    let src = std::fs::read_to_string(items_path)
        .unwrap_or_else(|e| panic!("Cannot read {:?}: {e}", items_path));
    serde_json::from_str(&src).unwrap_or_else(|e| panic!("Invalid JSON in {:?}: {e}", items_path))
}

/// Loads NPC definitions from `npcs_path` (a JSON array of `NpcData`).
///
/// Returns an empty list if the file does not exist; panics on malformed JSON.
pub fn load_npcs(npcs_path: &Path) -> Vec<NpcData> {
    if !npcs_path.exists() {
        return Vec::new();
    }
    let src = std::fs::read_to_string(npcs_path)
        .unwrap_or_else(|e| panic!("Cannot read {:?}: {e}", npcs_path));
    serde_json::from_str(&src).unwrap_or_else(|e| panic!("Invalid JSON in {:?}: {e}", npcs_path))
}

/// Loads quest definitions from `quests_path` (a JSON array of `QuestDef`).
///
/// Returns an empty list if the file does not exist; panics on malformed JSON.
pub fn load_quests(quests_path: &Path) -> Vec<crate::quest::QuestDef> {
    if !quests_path.exists() {
        return Vec::new();
    }
    let src = std::fs::read_to_string(quests_path)
        .unwrap_or_else(|e| panic!("Cannot read {:?}: {e}", quests_path));
    serde_json::from_str(&src).unwrap_or_else(|e| panic!("Invalid JSON in {:?}: {e}", quests_path))
}

// ── Internals ─────────────────────────────────────────────────────────────────

fn load_rooms_internal(rooms_dir: &Path) -> (RoomRegistry, HashMap<u64, Vec<i64>>) {
    let mut entries: Vec<_> = std::fs::read_dir(rooms_dir)
        .unwrap_or_else(|e| panic!("Cannot read rooms directory {:?}: {e}", rooms_dir))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
        .collect();

    entries.sort_by_key(|e| e.path());

    let mut registry = RoomRegistry::new();
    let mut room_items: HashMap<u64, Vec<i64>> = HashMap::new();

    for entry in entries {
        let path = entry.path();
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Cannot read {:?}: {e}", path));
        let def: RoomFile = serde_json::from_str(&src)
            .unwrap_or_else(|e| panic!("Invalid JSON in {:?}: {e}", path));

        let exits = def
            .exits
            .into_iter()
            .filter_map(|(dir_str, dest)| {
                parse_direction(&dir_str).map(|d| {
                    (
                        d,
                        Exit {
                            destination_room_id: dest,
                        },
                    )
                })
            })
            .collect();

        if !def.items.is_empty() {
            room_items.insert(def.id, def.items);
        }

        registry.insert(Room {
            id: def.id,
            name: def.name,
            description: def.description,
            exits,
        });
    }

    (registry, room_items)
}

fn parse_direction(s: &str) -> Option<Direction> {
    match s.to_lowercase().as_str() {
        "north" | "n" => Some(Direction::North),
        "south" | "s" => Some(Direction::South),
        "east" | "e" => Some(Direction::East),
        "west" | "w" => Some(Direction::West),
        "northeast" | "ne" => Some(Direction::NorthEast),
        "northwest" | "nw" => Some(Direction::NorthWest),
        "southeast" | "se" => Some(Direction::SouthEast),
        "southwest" | "sw" => Some(Direction::SouthWest),
        "up" | "u" => Some(Direction::Up),
        "down" | "d" => Some(Direction::Down),
        _ => None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rooms_dir() -> std::path::PathBuf {
        std::path::PathBuf::from("assets/rooms")
    }

    fn items_path() -> std::path::PathBuf {
        std::path::PathBuf::from("assets/items.json")
    }

    #[test]
    fn loads_four_rooms() {
        assert_eq!(load_rooms(&rooms_dir()).len(), 4);
    }

    #[test]
    fn starting_room_has_correct_name() {
        assert_eq!(load_rooms(&rooms_dir()).get(1).unwrap().name, "Town Square");
    }

    #[test]
    fn all_exits_point_to_existing_rooms() {
        let registry = load_rooms(&rooms_dir());
        for room_id in 1u64..=4 {
            let room = registry.get(room_id).unwrap();
            for (dir, exit) in &room.exits {
                assert!(
                    registry.get(exit.destination_room_id).is_some(),
                    "Room {room_id} exit {dir} → {} not found",
                    exit.destination_room_id
                );
            }
        }
    }

    #[test]
    fn loads_eight_items() {
        assert_eq!(load_items(&items_path()).len(), 8);
    }

    #[test]
    fn items_have_stable_ids() {
        let items = load_items(&items_path());
        assert!(
            items.iter().all(|i| i.id > 0),
            "all items must have a positive id"
        );
    }

    #[test]
    fn load_seed_room_items_reference_valid_ids() {
        let seed = load_seed(&rooms_dir(), &items_path());
        let known_ids: std::collections::HashSet<i64> =
            seed.item_defs.iter().map(|d| d.id).collect();
        for (room_id, ids) in &seed.room_items {
            for item_id in ids {
                assert!(
                    known_ids.contains(item_id),
                    "Room {room_id} references unknown item id {item_id}"
                );
            }
        }
    }

    #[test]
    fn load_seed_rooms_match_room_items_keys() {
        let seed = load_seed(&rooms_dir(), &items_path());
        for room_id in seed.room_items.keys() {
            assert!(
                seed.rooms.get(*room_id).is_some(),
                "room_items key {room_id} has no matching room"
            );
        }
    }
}
