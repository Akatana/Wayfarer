use std::collections::HashMap;

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};

use crate::npc::NpcData;

// ── Seeding ───────────────────────────────────────────────────────────────────

/// Seeds the npcs table on first boot from `assets/npcs.json` data.
pub async fn seed_if_empty(db: &DatabaseConnection, npcs: &[NpcData]) -> Result<(), DbErr> {
    let count: i64 = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS n FROM npcs".to_string(),
        ))
        .await?
        .and_then(|r| r.try_get::<i64>("", "n").ok())
        .unwrap_or(0);

    if count > 0 {
        return Ok(());
    }

    for npc in npcs {
        create(db, npc).await?;
    }
    Ok(())
}

// ── Loading ───────────────────────────────────────────────────────────────────

/// Loads all NPCs from the database, including their patrol routes.
pub async fn load_all(db: &DatabaseConnection) -> Result<Vec<NpcData>, DbErr> {
    let npc_rows = db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT id, name, description, greeting, hostile, room_id FROM npcs ORDER BY id"
                .to_string(),
        ))
        .await?;

    let patrol_rows = db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT npc_id, room_id FROM npc_patrol_routes ORDER BY npc_id, step".to_string(),
        ))
        .await?;

    let mut patrol_map: HashMap<i64, Vec<u64>> = HashMap::new();
    for row in patrol_rows {
        let npc_id: i64 = row.try_get("", "npc_id")?;
        let room_id: i64 = row.try_get("", "room_id")?;
        patrol_map.entry(npc_id).or_default().push(room_id as u64);
    }

    let mut result = Vec::new();
    for row in npc_rows {
        let id: i64 = row.try_get("", "id")?;
        let name: String = row.try_get("", "name")?;
        let description: String = row.try_get("", "description")?;
        let greeting: Option<String> = row.try_get("", "greeting")?;
        let hostile: i64 = row.try_get("", "hostile")?;
        let room_id: i64 = row.try_get("", "room_id")?;
        let patrol = patrol_map.remove(&id).unwrap_or_default();
        result.push(NpcData {
            id,
            name,
            description,
            greeting,
            hostile: hostile != 0,
            room_id: room_id as u64,
            patrol,
        });
    }
    Ok(result)
}

// ── Admin operations ──────────────────────────────────────────────────────────

/// Inserts a new NPC row and its patrol route.
pub async fn create(db: &DatabaseConnection, npc: &NpcData) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT OR IGNORE INTO npcs (id, name, description, greeting, hostile, room_id)
         VALUES (?,?,?,?,?,?)",
        [
            npc.id.into(),
            npc.name.clone().into(),
            npc.description.clone().into(),
            npc.greeting.clone().into(),
            (npc.hostile as i64).into(),
            (npc.room_id as i64).into(),
        ],
    ))
    .await?;
    set_patrol(db, npc.id, &npc.patrol).await?;
    Ok(())
}

/// Permanently removes an NPC and its patrol route.
pub async fn delete(db: &DatabaseConnection, npc_id: i64) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "DELETE FROM npc_patrol_routes WHERE npc_id=?",
        [npc_id.into()],
    ))
    .await?;
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "DELETE FROM npcs WHERE id=?",
        [npc_id.into()],
    ))
    .await?;
    Ok(())
}

/// Returns the highest NPC id in the database, or 0 if the table is empty.
pub async fn max_id(db: &DatabaseConnection) -> Result<i64, DbErr> {
    let n: i64 = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COALESCE(MAX(id), 0) AS n FROM npcs".to_string(),
        ))
        .await?
        .and_then(|r| r.try_get::<i64>("", "n").ok())
        .unwrap_or(0);
    Ok(n)
}

pub async fn update_name(db: &DatabaseConnection, id: i64, name: &str) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE npcs SET name=? WHERE id=?",
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
        "UPDATE npcs SET description=? WHERE id=?",
        [description.into(), id.into()],
    ))
    .await?;
    Ok(())
}

/// Sets or clears the NPC's greeting. `None` removes it (NPC becomes silent).
pub async fn update_greeting(
    db: &DatabaseConnection,
    id: i64,
    greeting: Option<&str>,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE npcs SET greeting=? WHERE id=?",
        [greeting.map(str::to_string).into(), id.into()],
    ))
    .await?;
    Ok(())
}

pub async fn update_hostile(db: &DatabaseConnection, id: i64, hostile: bool) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE npcs SET hostile=? WHERE id=?",
        [(hostile as i64).into(), id.into()],
    ))
    .await?;
    Ok(())
}

/// Persists the room an NPC moved into during a patrol step.
pub async fn update_room(db: &DatabaseConnection, npc_id: i64, room_id: u64) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE npcs SET room_id=? WHERE id=?",
        [(room_id as i64).into(), npc_id.into()],
    ))
    .await?;
    Ok(())
}

/// Replaces the patrol route for an NPC. Pass an empty slice to clear it.
pub async fn set_patrol(db: &DatabaseConnection, npc_id: i64, rooms: &[u64]) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "DELETE FROM npc_patrol_routes WHERE npc_id=?",
        [npc_id.into()],
    ))
    .await?;
    for (step, &room_id) in rooms.iter().enumerate() {
        db.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO npc_patrol_routes (npc_id, step, room_id) VALUES (?,?,?)",
            [npc_id.into(), (step as i64).into(), (room_id as i64).into()],
        ))
        .await?;
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connect, schema};

    async fn test_db() -> DatabaseConnection {
        let db = connect("sqlite::memory:").await.unwrap();
        schema::create_tables(&db).await.unwrap();
        db
    }

    fn make_npc(id: i64) -> NpcData {
        NpcData {
            id,
            name: format!("npc {id}"),
            description: "A test NPC.".to_string(),
            greeting: None,
            hostile: false,
            room_id: 1,
            patrol: Vec::new(),
        }
    }

    #[tokio::test]
    async fn seed_if_empty_inserts_npcs() {
        let db = test_db().await;
        let npcs = vec![make_npc(1), make_npc(2)];
        seed_if_empty(&db, &npcs).await.unwrap();
        let loaded = load_all(&db).await.unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[tokio::test]
    async fn seed_if_empty_does_not_duplicate() {
        let db = test_db().await;
        let npcs = vec![make_npc(1)];
        seed_if_empty(&db, &npcs).await.unwrap();
        seed_if_empty(&db, &npcs).await.unwrap();
        let loaded = load_all(&db).await.unwrap();
        assert_eq!(loaded.len(), 1);
    }

    #[tokio::test]
    async fn create_and_delete_npc() {
        let db = test_db().await;
        create(&db, &make_npc(5)).await.unwrap();
        assert_eq!(load_all(&db).await.unwrap().len(), 1);
        delete(&db, 5).await.unwrap();
        assert_eq!(load_all(&db).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn patrol_route_round_trips() {
        let db = test_db().await;
        let mut npc = make_npc(1);
        npc.patrol = vec![1, 2, 3, 2];
        create(&db, &npc).await.unwrap();
        let loaded = load_all(&db).await.unwrap();
        assert_eq!(loaded[0].patrol, vec![1, 2, 3, 2]);
    }

    #[tokio::test]
    async fn set_patrol_replaces_route() {
        let db = test_db().await;
        let mut npc = make_npc(1);
        npc.patrol = vec![1, 2];
        create(&db, &npc).await.unwrap();
        set_patrol(&db, 1, &[3, 4, 5]).await.unwrap();
        let loaded = load_all(&db).await.unwrap();
        assert_eq!(loaded[0].patrol, vec![3, 4, 5]);
    }

    #[tokio::test]
    async fn max_id_returns_zero_on_empty() {
        let db = test_db().await;
        assert_eq!(max_id(&db).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn update_hostile_persists() {
        let db = test_db().await;
        create(&db, &make_npc(1)).await.unwrap();
        update_hostile(&db, 1, true).await.unwrap();
        let npc = &load_all(&db).await.unwrap()[0];
        assert!(npc.hostile);
    }

    #[tokio::test]
    async fn update_greeting_persists() {
        let db = test_db().await;
        create(&db, &make_npc(1)).await.unwrap();
        update_greeting(&db, 1, Some("Hello, traveller."))
            .await
            .unwrap();
        let npc = &load_all(&db).await.unwrap()[0];
        assert_eq!(npc.greeting.as_deref(), Some("Hello, traveller."));
        update_greeting(&db, 1, None).await.unwrap();
        let npc = &load_all(&db).await.unwrap()[0];
        assert!(npc.greeting.is_none());
    }

    #[tokio::test]
    async fn update_room_persists() {
        let db = test_db().await;
        create(&db, &make_npc(1)).await.unwrap();
        update_room(&db, 1, 42).await.unwrap();
        let npc = &load_all(&db).await.unwrap()[0];
        assert_eq!(npc.room_id, 42);
    }
}
