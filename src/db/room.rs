use std::collections::HashMap;

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};

use crate::direction::Direction;
use crate::world::loader;
use crate::world::room::{Exit, Room, RoomRegistry};

/// Loads the world from the database.
///
/// On first run (empty `rooms` table) the hardcoded seed world is inserted
/// so operators can then edit rooms directly in the DB without recompiling.
pub async fn load_or_seed(db: &DatabaseConnection) -> Result<RoomRegistry, DbErr> {
    let count_row = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS n FROM rooms".to_string(),
        ))
        .await?;

    let is_empty = count_row
        .and_then(|r| r.try_get::<i64>("", "n").ok())
        .unwrap_or(0)
        == 0;

    if is_empty {
        seed(db).await?;
    }

    load(db).await
}

// ── Admin world-building operations ──────────────────────────────────────────

/// Inserts a new room and all its exits. Does nothing if the id already exists.
pub async fn create(db: &DatabaseConnection, room: &Room) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT OR IGNORE INTO rooms (id, name, description) VALUES (?, ?, ?)",
        [
            (room.id as i64).into(),
            room.name.clone().into(),
            room.description.clone().into(),
        ],
    ))
    .await?;

    for (dir, exit) in &room.exits {
        upsert_exit(db, room.id, *dir, exit.destination_room_id).await?;
    }
    Ok(())
}

/// Updates the name and description of an existing room.
pub async fn update(
    db: &DatabaseConnection,
    id: u64,
    name: &str,
    description: &str,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE rooms SET name=?, description=? WHERE id=?",
        [name.into(), description.into(), (id as i64).into()],
    ))
    .await?;
    Ok(())
}

/// Inserts or replaces a single exit.
pub async fn upsert_exit(
    db: &DatabaseConnection,
    room_id: u64,
    dir: Direction,
    dest_id: u64,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT OR REPLACE INTO exits (room_id, direction, destination_room_id) VALUES (?, ?, ?)",
        [
            (room_id as i64).into(),
            dir.to_string().into(),
            (dest_id as i64).into(),
        ],
    ))
    .await?;
    Ok(())
}

/// Removes a single exit.
pub async fn delete_exit(
    db: &DatabaseConnection,
    room_id: u64,
    dir: Direction,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "DELETE FROM exits WHERE room_id=? AND direction=?",
        [(room_id as i64).into(), dir.to_string().into()],
    ))
    .await?;
    Ok(())
}

/// Returns the highest room id currently in the database, or 0 if the table is empty.
pub async fn max_id(db: &DatabaseConnection) -> Result<u64, DbErr> {
    let n: i64 = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COALESCE(MAX(id), 0) AS n FROM rooms".to_string(),
        ))
        .await?
        .and_then(|r| r.try_get::<i64>("", "n").ok())
        .unwrap_or(0);
    Ok(n as u64)
}

async fn seed(db: &DatabaseConnection) -> Result<(), DbErr> {
    let registry = loader::load_rooms(std::path::Path::new("assets/rooms"));
    for room in registry.iter() {
        db.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT OR IGNORE INTO rooms (id, name, description) VALUES (?, ?, ?)",
            [
                (room.id as i64).into(),
                room.name.clone().into(),
                room.description.clone().into(),
            ],
        ))
        .await?;

        for (direction, exit) in &room.exits {
            db.execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT OR IGNORE INTO exits (room_id, direction, destination_room_id) VALUES (?, ?, ?)",
                [
                    (room.id as i64).into(),
                    direction.to_string().into(),
                    (exit.destination_room_id as i64).into(),
                ],
            ))
            .await?;
        }
    }
    Ok(())
}

async fn load(db: &DatabaseConnection) -> Result<RoomRegistry, DbErr> {
    let room_rows = db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT id, name, description FROM rooms ORDER BY id".to_string(),
        ))
        .await?;

    let exit_rows = db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT room_id, direction, destination_room_id FROM exits".to_string(),
        ))
        .await?;

    // Group exits by room_id.
    let mut exit_map: HashMap<u64, Vec<(String, u64)>> = HashMap::new();
    for row in exit_rows {
        let room_id: i64 = row.try_get("", "room_id")?;
        let direction: String = row.try_get("", "direction")?;
        let dest_id: i64 = row.try_get("", "destination_room_id")?;
        exit_map
            .entry(room_id as u64)
            .or_default()
            .push((direction, dest_id as u64));
    }

    let mut registry = RoomRegistry::new();
    for row in room_rows {
        let id: i64 = row.try_get("", "id")?;
        let name: String = row.try_get("", "name")?;
        let description: String = row.try_get("", "description")?;

        let exits = exit_map
            .remove(&(id as u64))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(dir_str, dest_id)| {
                dir_str.parse::<Direction>().ok().map(|d| {
                    (
                        d,
                        Exit {
                            destination_room_id: dest_id,
                        },
                    )
                })
            })
            .collect();

        registry.insert(Room {
            id: id as u64,
            name,
            description,
            exits,
        });
    }

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connect, schema};

    async fn test_db() -> DatabaseConnection {
        let db = connect("sqlite::memory:").await.unwrap();
        schema::create_tables(&db).await.unwrap();
        db
    }

    #[tokio::test]
    async fn seeds_four_rooms_on_empty_db() {
        let db = test_db().await;
        let registry = load_or_seed(&db).await.unwrap();
        assert_eq!(registry.len(), 4);
    }

    #[tokio::test]
    async fn second_call_does_not_duplicate_rooms() {
        let db = test_db().await;
        load_or_seed(&db).await.unwrap();
        let registry = load_or_seed(&db).await.unwrap();
        assert_eq!(registry.len(), 4);
    }

    #[tokio::test]
    async fn exits_resolve_correctly_after_round_trip() {
        let db = test_db().await;
        let registry = load_or_seed(&db).await.unwrap();
        assert_eq!(registry.resolve_exit(1, Direction::North), Some(2));
        assert_eq!(registry.resolve_exit(2, Direction::South), Some(1));
        assert_eq!(registry.resolve_exit(1, Direction::West), None);
    }

    #[tokio::test]
    async fn room_names_survive_round_trip() {
        let db = test_db().await;
        let registry = load_or_seed(&db).await.unwrap();
        assert_eq!(registry.get(1).unwrap().name, "Town Square");
        assert_eq!(registry.get(2).unwrap().name, "North Gate");
    }

    #[tokio::test]
    async fn manually_inserted_room_is_returned_by_load() {
        let db = test_db().await;
        load_or_seed(&db).await.unwrap();

        db.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO rooms (id, name, description) VALUES (?, ?, ?)",
            [5i64.into(), "Hidden Cave".into(), "Darkness echoes.".into()],
        ))
        .await
        .unwrap();

        let registry = load(&db).await.unwrap();
        assert_eq!(registry.get(5).unwrap().name, "Hidden Cave");
    }
}
