use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::character::CharacterData;
use crate::world::seed::STARTING_ROOM_ID;

// ── SeaORM entity ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "characters")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub room_id: i64,
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
    pub copper: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

// ── Error types ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum CreateError {
    NameTaken,
    Db(DbErr),
}

// ── CRUD ──────────────────────────────────────────────────────────────────────

/// Returns all characters belonging to the given account.
pub async fn list_for_account(
    db: &DatabaseConnection,
    account_id: i64,
    is_admin: bool,
) -> Vec<CharacterData> {
    Entity::find()
        .filter(Column::AccountId.eq(account_id))
        .all(db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| model_to_data(m, is_admin))
        .collect()
}

/// Creates a new character for the given account.
/// Character names are globally unique across all accounts.
pub async fn create_for_account(
    db: &DatabaseConnection,
    account_id: i64,
    name: &str,
    is_admin: bool,
) -> Result<CharacterData, CreateError> {
    if Entity::find()
        .filter(Column::Name.eq(name))
        .one(db)
        .await
        .map_err(CreateError::Db)?
        .is_some()
    {
        return Err(CreateError::NameTaken);
    }

    let active = ActiveModel {
        account_id: Set(account_id),
        name: Set(name.to_string()),
        room_id: Set(STARTING_ROOM_ID as i64),
        hp: Set(100),
        max_hp: Set(100),
        mp: Set(10),
        max_mp: Set(10),
        strength: Set(0),
        dexterity: Set(0),
        knowledge: Set(0),
        level: Set(1),
        experience: Set(0),
        learning_points: Set(0),
        ..Default::default()
    };

    let model = active.insert(db).await.map_err(CreateError::Db)?;
    Ok(model_to_data(model, is_admin))
}

/// Deletes a character, verifying that it belongs to `account_id`.
/// Returns `true` if a row was deleted, `false` if not found or ownership mismatch.
pub async fn delete_by_id(
    db: &DatabaseConnection,
    char_id: i64,
    account_id: i64,
) -> Result<bool, DbErr> {
    let result = Entity::delete_many()
        .filter(Column::Id.eq(char_id))
        .filter(Column::AccountId.eq(account_id))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}

/// Persists the current in-game state of a character back to the database.
/// Skips characters with id == 0 (mock / test characters not backed by the DB).
pub async fn save(db: &DatabaseConnection, data: CharacterData) -> Result<(), DbErr> {
    if data.id == 0 {
        return Ok(());
    }
    if let Some(model) = Entity::find_by_id(data.id).one(db).await? {
        let mut active: ActiveModel = model.into();
        active.room_id = Set(data.room_id as i64);
        active.hp = Set(data.hp);
        active.max_hp = Set(data.max_hp);
        active.mp = Set(data.mp);
        active.max_mp = Set(data.max_mp);
        active.strength = Set(data.strength);
        active.dexterity = Set(data.dexterity);
        active.knowledge = Set(data.knowledge);
        active.level = Set(data.level);
        active.experience = Set(data.experience);
        active.learning_points = Set(data.learning_points);
        active.copper = Set(data.copper);
        active.update(db).await?;
    }
    Ok(())
}

fn model_to_data(m: Model, is_admin: bool) -> CharacterData {
    CharacterData {
        id: m.id,
        account_id: m.account_id,
        is_admin,
        name: m.name,
        room_id: m.room_id as u64,
        hp: m.hp,
        max_hp: m.max_hp,
        mp: m.mp,
        max_mp: m.max_mp,
        strength: m.strength,
        dexterity: m.dexterity,
        knowledge: m.knowledge,
        level: m.level,
        experience: m.experience,
        learning_points: m.learning_points,
        copper: m.copper,
        items: Vec::new(),
        quests: Vec::new(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{account, connect, schema};

    async fn test_db() -> DatabaseConnection {
        let db = connect("sqlite::memory:").await.unwrap();
        schema::create_tables(&db).await.unwrap();
        db
    }

    async fn make_account(db: &DatabaseConnection) -> account::AccountData {
        account::register(db, "testuser", "password").await.unwrap()
    }

    #[tokio::test]
    async fn list_is_empty_for_new_account() {
        let db = test_db().await;
        let acct = make_account(&db).await;
        let chars = list_for_account(&db, acct.id, false).await;
        assert!(chars.is_empty());
    }

    #[tokio::test]
    async fn create_for_account_inserts_character() {
        let db = test_db().await;
        let acct = make_account(&db).await;
        create_for_account(&db, acct.id, "Aldric", false)
            .await
            .unwrap();
        let chars = list_for_account(&db, acct.id, false).await;
        assert_eq!(chars.len(), 1);
        assert_eq!(chars[0].name, "Aldric");
    }

    #[tokio::test]
    async fn created_character_starts_at_full_health() {
        let db = test_db().await;
        let acct = make_account(&db).await;
        let ch = create_for_account(&db, acct.id, "Thane", false)
            .await
            .unwrap();
        assert_eq!(ch.hp, ch.max_hp);
        assert_eq!(ch.mp, ch.max_mp);
    }

    #[tokio::test]
    async fn duplicate_name_returns_error() {
        let db = test_db().await;
        let acct = make_account(&db).await;
        create_for_account(&db, acct.id, "Zara", false)
            .await
            .unwrap();
        let err = create_for_account(&db, acct.id, "Zara", false)
            .await
            .unwrap_err();
        assert!(matches!(err, CreateError::NameTaken));
    }

    #[tokio::test]
    async fn delete_by_id_removes_character() {
        let db = test_db().await;
        let acct = make_account(&db).await;
        let ch = create_for_account(&db, acct.id, "Mira", false)
            .await
            .unwrap();
        let deleted = delete_by_id(&db, ch.id, acct.id).await.unwrap();
        assert!(deleted);
        let chars = list_for_account(&db, acct.id, false).await;
        assert!(chars.is_empty());
    }

    #[tokio::test]
    async fn delete_by_id_requires_matching_account() {
        let db = test_db().await;
        let acct = make_account(&db).await;
        let ch = create_for_account(&db, acct.id, "Kira", false)
            .await
            .unwrap();
        let deleted = delete_by_id(&db, ch.id, acct.id + 99).await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn save_persists_position_and_stats() {
        let db = test_db().await;
        let acct = make_account(&db).await;
        let ch = create_for_account(&db, acct.id, "Legolas", false)
            .await
            .unwrap();

        let updated = CharacterData {
            id: ch.id,
            room_id: 4,
            hp: 75,
            max_hp: 100,
            mp: 30,
            max_mp: 50,
            ..ch.clone()
        };
        save(&db, updated).await.unwrap();

        let chars = list_for_account(&db, acct.id, false).await;
        let reloaded = chars.into_iter().find(|c| c.name == "Legolas").unwrap();
        assert_eq!(reloaded.room_id, 4);
        assert_eq!(reloaded.hp, 75);
        assert_eq!(reloaded.mp, 30);
    }

    #[tokio::test]
    async fn save_with_id_zero_is_no_op() {
        let db = test_db().await;
        let result = save(&db, CharacterData::default()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn is_admin_flag_propagates_from_list() {
        let db = test_db().await;
        let acct = make_account(&db).await;
        create_for_account(&db, acct.id, "Boss", acct.is_admin)
            .await
            .unwrap();
        let chars = list_for_account(&db, acct.id, acct.is_admin).await;
        assert_eq!(chars[0].is_admin, acct.is_admin);
    }
}
