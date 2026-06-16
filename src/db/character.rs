use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::character::CharacterData;

// ── SeaORM entity ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "characters")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub room_id: i64,
    pub hp: i32,
    pub max_hp: i32,
    pub mp: i32,
    pub max_mp: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

// ── CRUD ──────────────────────────────────────────────────────────────────────

/// Loads an existing character by name, or creates a fresh one if none exists.
///
/// Called from the session handler (network layer) so the game loop tick body
/// never blocks on I/O.
pub async fn load_or_create(db: &DatabaseConnection, name: &str) -> CharacterData {
    if let Ok(Some(model)) = Entity::find()
        .filter(Column::Name.eq(name))
        .one(db)
        .await
    {
        return model_to_data(model);
    }

    // New character
    let active = ActiveModel {
        name:   Set(name.to_string()),
        room_id: Set(1),
        hp:     Set(100),
        max_hp: Set(100),
        mp:     Set(50),
        max_mp: Set(50),
        ..Default::default()
    };

    match active.insert(db).await {
        Ok(model) => model_to_data(model),
        Err(e) => {
            eprintln!("[DB] Failed to create character '{name}': {e}");
            CharacterData { name: name.to_string(), ..Default::default() }
        }
    }
}

/// Persists the current in-game state of a character back to the database.
///
/// Called from async tasks spawned by the game loop — never inside the tick body.
pub async fn save(db: &DatabaseConnection, data: CharacterData) -> Result<(), DbErr> {
    if let Some(model) = Entity::find()
        .filter(Column::Name.eq(&data.name))
        .one(db)
        .await?
    {
        let mut active: ActiveModel = model.into();
        active.room_id = Set(data.room_id as i64);
        active.hp      = Set(data.hp);
        active.max_hp  = Set(data.max_hp);
        active.mp      = Set(data.mp);
        active.max_mp  = Set(data.max_mp);
        active.update(db).await?;
    }
    Ok(())
}

fn model_to_data(m: Model) -> CharacterData {
    CharacterData {
        name:   m.name,
        room_id: m.room_id as u64,
        hp:     m.hp,
        max_hp: m.max_hp,
        mp:     m.mp,
        max_mp: m.max_mp,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::Database;
    use crate::db::schema;

    async fn in_memory_db() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        schema::create_tables(&db).await.unwrap();
        db
    }

    #[tokio::test]
    async fn creates_new_character_on_first_login() {
        let db = in_memory_db().await;
        let data = load_or_create(&db, "Gandalf").await;
        assert_eq!(data.name, "Gandalf");
        assert_eq!(data.room_id, 1);
        assert_eq!(data.hp, 100);
    }

    #[tokio::test]
    async fn loads_existing_character_on_second_login() {
        let db = in_memory_db().await;
        load_or_create(&db, "Frodo").await; // first login

        // Change something in DB
        let model = Entity::find()
            .filter(Column::Name.eq("Frodo"))
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        let mut active: ActiveModel = model.into();
        active.room_id = Set(3);
        active.hp      = Set(42);
        active.update(&db).await.unwrap();

        let data = load_or_create(&db, "Frodo").await;
        assert_eq!(data.room_id, 3);
        assert_eq!(data.hp, 42);
    }

    #[tokio::test]
    async fn save_persists_position_and_stats() {
        let db = in_memory_db().await;
        load_or_create(&db, "Legolas").await;

        let updated = CharacterData {
            name: "Legolas".to_string(),
            room_id: 4,
            hp: 75,
            max_hp: 100,
            mp: 30,
            max_mp: 50,
        };
        save(&db, updated).await.unwrap();

        let reloaded = load_or_create(&db, "Legolas").await;
        assert_eq!(reloaded.room_id, 4);
        assert_eq!(reloaded.hp, 75);
        assert_eq!(reloaded.mp, 30);
    }

    #[tokio::test]
    async fn two_different_characters_are_independent() {
        let db = in_memory_db().await;
        load_or_create(&db, "Sam").await;
        load_or_create(&db, "Pippin").await;

        let sam = load_or_create(&db, "Sam").await;
        let pip = load_or_create(&db, "Pippin").await;
        assert_ne!(sam.name, pip.name);
    }
}
