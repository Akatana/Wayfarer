use std::collections::HashMap;

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};

use crate::item::{
    EquipRequirements, EquipSlot, ItemBonuses, ItemData, ItemLocation, ItemLocationSave,
};
use crate::world::loader::ItemDef;

// ── Item definition seeding / loading ────────────────────────────────────────

/// Seeds the item_definitions table from `items.json` on first boot.
pub async fn seed_defs_if_empty(
    db: &DatabaseConnection,
    item_defs: &[ItemDef],
) -> Result<(), DbErr> {
    let count: i64 = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS n FROM item_definitions".to_string(),
        ))
        .await?
        .and_then(|r| r.try_get::<i64>("", "n").ok())
        .unwrap_or(0);

    if count > 0 {
        return Ok(());
    }

    for def in item_defs {
        db.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO item_definitions (
                id, name, description, equip_slot, two_handed, bag_capacity,
                req_level, req_strength, req_dexterity, req_knowledge,
                bonus_strength, bonus_dexterity, bonus_knowledge, bonus_max_hp,
                bonus_min_damage, bonus_max_damage, bonus_armor
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            [
                def.id.into(),
                def.name.clone().into(),
                def.description.clone().into(),
                def.equip_slot.clone().into(),
                (def.two_handed as i64).into(),
                def.bag_capacity.map(|c| c as i64).into(),
                (def.requirements.level as i64).into(),
                (def.requirements.strength as i64).into(),
                (def.requirements.dexterity as i64).into(),
                (def.requirements.knowledge as i64).into(),
                (def.bonuses.bonus_strength as i64).into(),
                (def.bonuses.bonus_dexterity as i64).into(),
                (def.bonuses.bonus_knowledge as i64).into(),
                (def.bonuses.bonus_max_hp as i64).into(),
                (def.bonuses.bonus_min_damage as i64).into(),
                (def.bonuses.bonus_max_damage as i64).into(),
                (def.bonuses.bonus_armor as i64).into(),
            ],
        ))
        .await?;
    }

    Ok(())
}

/// Loads all item definitions from the DB (both built-in and admin-created).
pub async fn load_all_defs(db: &DatabaseConnection) -> Result<Vec<ItemDef>, DbErr> {
    let rows = db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT * FROM item_definitions ORDER BY id".to_string(),
        ))
        .await?;

    rows.into_iter().map(|r| row_to_item_def(&r)).collect()
}

/// Persists a new admin-created item definition.
pub async fn create_def(db: &DatabaseConnection, def: &ItemDef) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO item_definitions (
            id, name, description, equip_slot, two_handed, bag_capacity,
            req_level, req_strength, req_dexterity, req_knowledge,
            bonus_strength, bonus_dexterity, bonus_knowledge, bonus_max_hp,
            bonus_min_damage, bonus_max_damage, bonus_armor
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        [
            def.id.into(),
            def.name.clone().into(),
            def.description.clone().into(),
            def.equip_slot.clone().into(),
            (def.two_handed as i64).into(),
            def.bag_capacity.map(|c| c as i64).into(),
            (def.requirements.level as i64).into(),
            (def.requirements.strength as i64).into(),
            (def.requirements.dexterity as i64).into(),
            (def.requirements.knowledge as i64).into(),
            (def.bonuses.bonus_strength as i64).into(),
            (def.bonuses.bonus_dexterity as i64).into(),
            (def.bonuses.bonus_knowledge as i64).into(),
            (def.bonuses.bonus_max_hp as i64).into(),
            (def.bonuses.bonus_min_damage as i64).into(),
            (def.bonuses.bonus_max_damage as i64).into(),
            (def.bonuses.bonus_armor as i64).into(),
        ],
    ))
    .await?;
    Ok(())
}

/// Returns the highest id in item_definitions, or 0 if empty.
pub async fn max_def_id(db: &DatabaseConnection) -> Result<i64, DbErr> {
    let n: i64 = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COALESCE(MAX(id), 0) AS n FROM item_definitions".to_string(),
        ))
        .await?
        .and_then(|r| r.try_get::<i64>("", "n").ok())
        .unwrap_or(0);
    Ok(n)
}

// ── Seeding ───────────────────────────────────────────────────────────────────

/// Seeds the items table on first boot.
///
/// Creates one instance per room placement. Items not placed in any room are
/// not seeded — they only enter the world via loot drops or admin creation.
///
/// `item_defs` — parsed from `assets/items.json` (templates)
/// `room_items` — room_id → [def_id, ...] from room JSON files
pub async fn seed_if_empty(
    db: &DatabaseConnection,
    item_defs: &[ItemDef],
    room_items: &HashMap<u64, Vec<i64>>,
) -> Result<(), DbErr> {
    let count: i64 = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS n FROM items".to_string(),
        ))
        .await?
        .and_then(|r| r.try_get::<i64>("", "n").ok())
        .unwrap_or(0);

    if count > 0 {
        return Ok(());
    }

    let def_map: HashMap<i64, &ItemDef> = item_defs.iter().map(|d| (d.id, d)).collect();

    for (&room_id, def_ids) in room_items {
        for &def_id in def_ids {
            let Some(def) = def_map.get(&def_id) else {
                continue;
            };
            db.execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO items (
                    def_id, name, description, equip_slot, two_handed, bag_capacity,
                    req_level, req_strength, req_dexterity, req_knowledge,
                    bonus_strength, bonus_dexterity, bonus_knowledge, bonus_max_hp,
                    bonus_min_damage, bonus_max_damage, bonus_armor,
                    location, room_id
                 ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                [
                    def_id.into(),
                    def.name.clone().into(),
                    def.description.clone().into(),
                    def.equip_slot.clone().into(),
                    (def.two_handed as i64).into(),
                    def.bag_capacity.map(|c| c as i64).into(),
                    (def.requirements.level as i64).into(),
                    (def.requirements.strength as i64).into(),
                    (def.requirements.dexterity as i64).into(),
                    (def.requirements.knowledge as i64).into(),
                    (def.bonuses.bonus_strength as i64).into(),
                    (def.bonuses.bonus_dexterity as i64).into(),
                    (def.bonuses.bonus_knowledge as i64).into(),
                    (def.bonuses.bonus_max_hp as i64).into(),
                    (def.bonuses.bonus_min_damage as i64).into(),
                    (def.bonuses.bonus_max_damage as i64).into(),
                    (def.bonuses.bonus_armor as i64).into(),
                    "room".into(),
                    (room_id as i64).into(),
                ],
            ))
            .await?;
        }
    }

    Ok(())
}

// ── Loading ───────────────────────────────────────────────────────────────────

/// Loads all items whose location is a room (for ECS spawn at startup).
pub async fn load_in_rooms(db: &DatabaseConnection) -> Result<Vec<ItemData>, DbErr> {
    let rows = db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT * FROM items WHERE location = 'room'".to_string(),
        ))
        .await?;

    rows.into_iter().map(|r| row_to_item_data(&r)).collect()
}

/// Loads all items belonging to a character (inventory + equipped).
pub async fn load_for_character(
    db: &DatabaseConnection,
    char_id: i64,
) -> Result<Vec<ItemData>, DbErr> {
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT * FROM items WHERE char_id = ? AND location IN ('inventory','equipped')",
            [char_id.into()],
        ))
        .await?;

    rows.into_iter().map(|r| row_to_item_data(&r)).collect()
}

// ── Admin item operations ─────────────────────────────────────────────────────

/// Inserts a brand-new item instance. `item.id` must be pre-assigned by the caller.
pub async fn create(db: &DatabaseConnection, item: &ItemData) -> Result<(), DbErr> {
    let room_id: Option<i64> = match &item.location {
        ItemLocation::Room(id) => Some(*id as i64),
        _ => None,
    };
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT OR IGNORE INTO items (
            id, def_id, name, description, equip_slot, two_handed, bag_capacity,
            req_level, req_strength, req_dexterity, req_knowledge,
            bonus_strength, bonus_dexterity, bonus_knowledge, bonus_max_hp,
            bonus_min_damage, bonus_max_damage, bonus_armor,
            location, room_id
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        [
            item.id.into(),
            item.def_id.into(),
            item.name.clone().into(),
            item.description.clone().into(),
            item.equip_slot.map(|s| s.to_string()).into(),
            (item.two_handed as i64).into(),
            item.bag_capacity.map(|c| c as i64).into(),
            (item.requirements.level as i64).into(),
            (item.requirements.strength as i64).into(),
            (item.requirements.dexterity as i64).into(),
            (item.requirements.knowledge as i64).into(),
            (item.bonuses.bonus_strength as i64).into(),
            (item.bonuses.bonus_dexterity as i64).into(),
            (item.bonuses.bonus_knowledge as i64).into(),
            (item.bonuses.bonus_max_hp as i64).into(),
            (item.bonuses.bonus_min_damage as i64).into(),
            (item.bonuses.bonus_max_damage as i64).into(),
            (item.bonuses.bonus_armor as i64).into(),
            item.location.as_db_str().into(),
            room_id.into(),
        ],
    ))
    .await?;
    Ok(())
}

/// Permanently removes an item row.
pub async fn delete(db: &DatabaseConnection, item_id: i64) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "DELETE FROM items WHERE id=?",
        [item_id.into()],
    ))
    .await?;
    Ok(())
}

/// Returns the highest item id currently in the database, or 0 if the table is empty.
pub async fn max_id(db: &DatabaseConnection) -> Result<i64, DbErr> {
    let n: i64 = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COALESCE(MAX(id), 0) AS n FROM items".to_string(),
        ))
        .await?
        .and_then(|r| r.try_get::<i64>("", "n").ok())
        .unwrap_or(0);
    Ok(n)
}

// ── Admin item field updates ──────────────────────────────────────────────────

pub async fn update_name(db: &DatabaseConnection, id: i64, name: &str) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE items SET name=? WHERE id=?",
        [name.into(), id.into()],
    ))
    .await?;
    Ok(())
}

pub async fn update_description(
    db: &DatabaseConnection,
    id: i64,
    description: &str,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE items SET description=? WHERE id=?",
        [description.into(), id.into()],
    ))
    .await?;
    Ok(())
}

/// Sets or clears the equip slot. `None` writes NULL (unequippable item).
pub async fn update_slot(
    db: &DatabaseConnection,
    id: i64,
    equip_slot: Option<&str>,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE items SET equip_slot=? WHERE id=?",
        [equip_slot.map(|s| s.to_string()).into(), id.into()],
    ))
    .await?;
    Ok(())
}

pub async fn update_requirements(
    db: &DatabaseConnection,
    id: i64,
    level: i32,
    strength: i32,
    dexterity: i32,
    knowledge: i32,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE items SET req_level=?, req_strength=?, req_dexterity=?, req_knowledge=? WHERE id=?",
        [
            (level as i64).into(),
            (strength as i64).into(),
            (dexterity as i64).into(),
            (knowledge as i64).into(),
            id.into(),
        ],
    ))
    .await?;
    Ok(())
}

// ── Saving ────────────────────────────────────────────────────────────────────

/// Persists a single item's location to the database.
pub async fn save_location(db: &DatabaseConnection, save: &ItemLocationSave) -> Result<(), DbErr> {
    match &save.location {
        ItemLocation::Room(room_id) => {
            db.execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE items SET location='room', room_id=?, char_id=NULL, equipped_slot=NULL WHERE id=?",
                [(*room_id as i64).into(), save.item_id.into()],
            ))
            .await?;
        }
        ItemLocation::Inventory { char_id } => {
            db.execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE items SET location='inventory', char_id=?, room_id=NULL, equipped_slot=NULL WHERE id=?",
                [(*char_id).into(), save.item_id.into()],
            ))
            .await?;
        }
        ItemLocation::Equipped { char_id, slot } => {
            db.execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE items SET location='equipped', char_id=?, equipped_slot=?, room_id=NULL WHERE id=?",
                [(*char_id).into(), slot.to_string().into(), save.item_id.into()],
            ))
            .await?;
        }
    }
    Ok(())
}

// ── Row mapping ───────────────────────────────────────────────────────────────

fn row_to_item_data(r: &sea_orm::QueryResult) -> Result<ItemData, DbErr> {
    let id: i64 = r.try_get("", "id")?;
    let def_id: i64 = r.try_get("", "def_id")?;
    let name: String = r.try_get("", "name")?;
    let description: String = r.try_get("", "description")?;
    let equip_slot_str: Option<String> = r.try_get("", "equip_slot")?;
    let two_handed: i64 = r.try_get("", "two_handed")?;
    let bag_capacity: Option<i64> = r.try_get("", "bag_capacity")?;
    let req_level: i64 = r.try_get("", "req_level")?;
    let req_strength: i64 = r.try_get("", "req_strength")?;
    let req_dexterity: i64 = r.try_get("", "req_dexterity")?;
    let req_knowledge: i64 = r.try_get("", "req_knowledge")?;
    let bonus_strength: i64 = r.try_get("", "bonus_strength").unwrap_or(0);
    let bonus_dexterity: i64 = r.try_get("", "bonus_dexterity").unwrap_or(0);
    let bonus_knowledge: i64 = r.try_get("", "bonus_knowledge").unwrap_or(0);
    let bonus_max_hp: i64 = r.try_get("", "bonus_max_hp").unwrap_or(0);
    let bonus_min_damage: i64 = r.try_get("", "bonus_min_damage").unwrap_or(0);
    let bonus_max_damage: i64 = r.try_get("", "bonus_max_damage").unwrap_or(0);
    let bonus_armor: i64 = r.try_get("", "bonus_armor").unwrap_or(0);
    let location_str: String = r.try_get("", "location")?;
    let room_id: Option<i64> = r.try_get("", "room_id")?;
    let char_id: Option<i64> = r.try_get("", "char_id")?;
    let equipped_slot_str: Option<String> = r.try_get("", "equipped_slot")?;

    let equip_slot = equip_slot_str.as_deref().and_then(EquipSlot::parse);

    let location = match location_str.as_str() {
        "inventory" => ItemLocation::Inventory {
            char_id: char_id.unwrap_or(0),
        },
        "equipped" => ItemLocation::Equipped {
            char_id: char_id.unwrap_or(0),
            slot: equipped_slot_str
                .as_deref()
                .and_then(EquipSlot::parse)
                .unwrap_or(EquipSlot::LeftHand),
        },
        _ => ItemLocation::Room(room_id.unwrap_or(1) as u64),
    };

    Ok(ItemData {
        id,
        def_id,
        name,
        description,
        equip_slot,
        two_handed: two_handed != 0,
        bag_capacity: bag_capacity.map(|c| c as usize),
        requirements: EquipRequirements {
            level: req_level as i32,
            strength: req_strength as i32,
            dexterity: req_dexterity as i32,
            knowledge: req_knowledge as i32,
        },
        bonuses: ItemBonuses {
            bonus_strength: bonus_strength as i32,
            bonus_dexterity: bonus_dexterity as i32,
            bonus_knowledge: bonus_knowledge as i32,
            bonus_max_hp: bonus_max_hp as i32,
            bonus_min_damage: bonus_min_damage as i32,
            bonus_max_damage: bonus_max_damage as i32,
            bonus_armor: bonus_armor as i32,
        },
        location,
    })
}

pub async fn update_def_name(db: &DatabaseConnection, id: i64, name: &str) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE item_definitions SET name=? WHERE id=?",
        [name.into(), id.into()],
    ))
    .await?;
    Ok(())
}

pub async fn update_def_description(
    db: &DatabaseConnection,
    id: i64,
    description: &str,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE item_definitions SET description=? WHERE id=?",
        [description.into(), id.into()],
    ))
    .await?;
    Ok(())
}

pub async fn update_def_slot(
    db: &DatabaseConnection,
    id: i64,
    equip_slot: Option<&str>,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE item_definitions SET equip_slot=? WHERE id=?",
        [equip_slot.map(|s| s.to_string()).into(), id.into()],
    ))
    .await?;
    Ok(())
}

pub async fn update_def_requirements(
    db: &DatabaseConnection,
    id: i64,
    level: i32,
    strength: i32,
    dexterity: i32,
    knowledge: i32,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE item_definitions SET req_level=?, req_strength=?, req_dexterity=?, req_knowledge=? WHERE id=?",
        [
            (level as i64).into(),
            (strength as i64).into(),
            (dexterity as i64).into(),
            (knowledge as i64).into(),
            id.into(),
        ],
    ))
    .await?;
    Ok(())
}

pub async fn update_def_bonuses(
    db: &DatabaseConnection,
    id: i64,
    bonuses: &ItemBonuses,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE item_definitions SET
            bonus_strength=?, bonus_dexterity=?, bonus_knowledge=?, bonus_max_hp=?,
            bonus_min_damage=?, bonus_max_damage=?, bonus_armor=?
         WHERE id=?",
        [
            (bonuses.bonus_strength as i64).into(),
            (bonuses.bonus_dexterity as i64).into(),
            (bonuses.bonus_knowledge as i64).into(),
            (bonuses.bonus_max_hp as i64).into(),
            (bonuses.bonus_min_damage as i64).into(),
            (bonuses.bonus_max_damage as i64).into(),
            (bonuses.bonus_armor as i64).into(),
            id.into(),
        ],
    ))
    .await?;
    Ok(())
}

fn row_to_item_def(r: &sea_orm::QueryResult) -> Result<ItemDef, DbErr> {
    let id: i64 = r.try_get("", "id")?;
    let name: String = r.try_get("", "name")?;
    let description: String = r.try_get("", "description")?;
    let equip_slot: Option<String> = r.try_get("", "equip_slot")?;
    let two_handed: i64 = r.try_get("", "two_handed")?;
    let bag_capacity: Option<i64> = r.try_get("", "bag_capacity")?;
    let req_level: i64 = r.try_get("", "req_level")?;
    let req_strength: i64 = r.try_get("", "req_strength")?;
    let req_dexterity: i64 = r.try_get("", "req_dexterity")?;
    let req_knowledge: i64 = r.try_get("", "req_knowledge")?;
    let bonus_strength: i64 = r.try_get("", "bonus_strength").unwrap_or(0);
    let bonus_dexterity: i64 = r.try_get("", "bonus_dexterity").unwrap_or(0);
    let bonus_knowledge: i64 = r.try_get("", "bonus_knowledge").unwrap_or(0);
    let bonus_max_hp: i64 = r.try_get("", "bonus_max_hp").unwrap_or(0);
    let bonus_min_damage: i64 = r.try_get("", "bonus_min_damage").unwrap_or(0);
    let bonus_max_damage: i64 = r.try_get("", "bonus_max_damage").unwrap_or(0);
    let bonus_armor: i64 = r.try_get("", "bonus_armor").unwrap_or(0);

    Ok(ItemDef {
        id,
        name,
        description,
        equip_slot,
        two_handed: two_handed != 0,
        bag_capacity: bag_capacity.map(|c| c as usize),
        requirements: EquipRequirements {
            level: req_level as i32,
            strength: req_strength as i32,
            dexterity: req_dexterity as i32,
            knowledge: req_knowledge as i32,
        },
        bonuses: ItemBonuses {
            bonus_strength: bonus_strength as i32,
            bonus_dexterity: bonus_dexterity as i32,
            bonus_knowledge: bonus_knowledge as i32,
            bonus_max_hp: bonus_max_hp as i32,
            bonus_min_damage: bonus_min_damage as i32,
            bonus_max_damage: bonus_max_damage as i32,
            bonus_armor: bonus_armor as i32,
        },
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connect, schema};
    use crate::world::loader;

    async fn test_db() -> DatabaseConnection {
        let db = connect("sqlite::memory:").await.unwrap();
        schema::create_tables(&db).await.unwrap();
        db
    }

    fn seed_data() -> (Vec<ItemDef>, HashMap<u64, Vec<i64>>) {
        let item_defs = loader::load_items(std::path::Path::new("assets/items.json"));
        let seed = loader::load_seed(
            std::path::Path::new("assets/rooms"),
            std::path::Path::new("assets/items.json"),
        );
        (item_defs, seed.room_items)
    }

    #[tokio::test]
    async fn seeds_eight_items_on_empty_db() {
        let db = test_db().await;
        let (defs, room_items) = seed_data();
        seed_if_empty(&db, &defs, &room_items).await.unwrap();
        let items = load_in_rooms(&db).await.unwrap();
        assert_eq!(items.len(), 8);
    }

    #[tokio::test]
    async fn seed_is_idempotent() {
        let db = test_db().await;
        let (defs, room_items) = seed_data();
        seed_if_empty(&db, &defs, &room_items).await.unwrap();
        seed_if_empty(&db, &defs, &room_items).await.unwrap();
        let items = load_in_rooms(&db).await.unwrap();
        assert_eq!(items.len(), 8);
    }

    #[tokio::test]
    async fn save_location_moves_item_to_inventory() {
        let db = test_db().await;
        let (defs, room_items) = seed_data();
        seed_if_empty(&db, &defs, &room_items).await.unwrap();

        save_location(
            &db,
            &ItemLocationSave {
                item_id: 1,
                location: ItemLocation::Inventory { char_id: 42 },
            },
        )
        .await
        .unwrap();

        let char_items = load_for_character(&db, 42).await.unwrap();
        assert_eq!(char_items.len(), 1);
        assert_eq!(char_items[0].id, 1);
        assert!(matches!(
            char_items[0].location,
            ItemLocation::Inventory { char_id: 42 }
        ));
    }

    #[tokio::test]
    async fn save_location_equips_item() {
        let db = test_db().await;
        let (defs, room_items) = seed_data();
        seed_if_empty(&db, &defs, &room_items).await.unwrap();

        save_location(
            &db,
            &ItemLocationSave {
                item_id: 1,
                location: ItemLocation::Equipped {
                    char_id: 99,
                    slot: EquipSlot::LeftHand,
                },
            },
        )
        .await
        .unwrap();

        let char_items = load_for_character(&db, 99).await.unwrap();
        assert!(matches!(
            char_items[0].location,
            ItemLocation::Equipped {
                slot: EquipSlot::LeftHand,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn requirements_survive_round_trip() {
        let db = test_db().await;
        let (defs, room_items) = seed_data();
        seed_if_empty(&db, &defs, &room_items).await.unwrap();

        let items = load_in_rooms(&db).await.unwrap();
        let sword = items.iter().find(|i| i.name.contains("sword")).unwrap();
        assert_eq!(sword.requirements.strength, 5);
    }
}
