use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};

// ── SeaORM entity ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "accounts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub is_admin: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

// ── Public DTO ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AccountData {
    pub id: i64,
    pub username: String,
    pub is_admin: bool,
}

#[derive(Debug)]
pub enum RegisterError {
    UsernameTaken,
    Db(DbErr),
}

// ── CRUD ──────────────────────────────────────────────────────────────────────

/// Creates a new account. The very first account registered on the server
/// automatically receives admin privileges.
pub async fn register(
    db: &DatabaseConnection,
    username: &str,
    password: &str,
) -> Result<AccountData, RegisterError> {
    if Entity::find()
        .filter(Column::Username.eq(username))
        .one(db)
        .await
        .map_err(RegisterError::Db)?
        .is_some()
    {
        return Err(RegisterError::UsernameTaken);
    }

    let count = Entity::find().count(db).await.map_err(RegisterError::Db)?;
    let is_admin = count == 0;

    let hash = bcrypt::hash(password, HASH_COST)
        .map_err(|e| RegisterError::Db(DbErr::Custom(e.to_string())))?;

    let active = ActiveModel {
        username: Set(username.to_string()),
        password_hash: Set(hash),
        is_admin: Set(is_admin),
        ..Default::default()
    };

    let model = active.insert(db).await.map_err(RegisterError::Db)?;
    Ok(model_to_data(model))
}

/// Verifies credentials and returns the account if they are correct.
pub async fn authenticate(
    db: &DatabaseConnection,
    username: &str,
    password: &str,
) -> Option<AccountData> {
    let model = Entity::find()
        .filter(Column::Username.eq(username))
        .one(db)
        .await
        .ok()??;

    let valid = bcrypt::verify(password, &model.password_hash).unwrap_or(false);
    if valid {
        Some(model_to_data(model))
    } else {
        None
    }
}

/// Looks up an account by username without verifying a password.
/// Used by the mock session to find or seed its test account.
pub async fn find_by_username(db: &DatabaseConnection, username: &str) -> Option<AccountData> {
    Entity::find()
        .filter(Column::Username.eq(username))
        .one(db)
        .await
        .ok()?
        .map(model_to_data)
}

/// Returns the mock account, creating it on first run.
pub async fn find_or_create_mock(db: &DatabaseConnection) -> AccountData {
    if let Some(acct) = find_by_username(db, "mock_tester").await {
        return acct;
    }
    match register(db, "mock_tester", "test_password").await {
        Ok(acct) => acct,
        Err(RegisterError::UsernameTaken) => {
            find_by_username(db, "mock_tester")
                .await
                .unwrap_or(AccountData {
                    id: 0,
                    username: "mock_tester".to_string(),
                    is_admin: false,
                })
        }
        Err(RegisterError::Db(e)) => {
            eprintln!("[Mock] Failed to create mock account: {e}");
            AccountData {
                id: 0,
                username: "mock_tester".to_string(),
                is_admin: false,
            }
        }
    }
}

fn model_to_data(m: Model) -> AccountData {
    AccountData {
        id: m.id,
        username: m.username,
        is_admin: m.is_admin,
    }
}

#[cfg(not(test))]
const HASH_COST: u32 = bcrypt::DEFAULT_COST;

#[cfg(test)]
const HASH_COST: u32 = 4;

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

    #[tokio::test]
    async fn register_creates_account() {
        let db = test_db().await;
        let acct = register(&db, "alice", "password123").await.unwrap();
        assert_eq!(acct.username, "alice");
    }

    #[tokio::test]
    async fn first_account_is_admin() {
        let db = test_db().await;
        let acct = register(&db, "firstuser", "password123").await.unwrap();
        assert!(acct.is_admin);
    }

    #[tokio::test]
    async fn second_account_is_not_admin() {
        let db = test_db().await;
        register(&db, "firstuser", "password123").await.unwrap();
        let acct = register(&db, "seconduser", "password123").await.unwrap();
        assert!(!acct.is_admin);
    }

    #[tokio::test]
    async fn duplicate_username_returns_error() {
        let db = test_db().await;
        register(&db, "alice", "password123").await.unwrap();
        let err = register(&db, "alice", "different").await.unwrap_err();
        assert!(matches!(err, RegisterError::UsernameTaken));
    }

    #[tokio::test]
    async fn authenticate_returns_account_on_correct_password() {
        let db = test_db().await;
        register(&db, "bob", "hunter2").await.unwrap();
        let acct = authenticate(&db, "bob", "hunter2").await;
        assert!(acct.is_some());
        assert_eq!(acct.unwrap().username, "bob");
    }

    #[tokio::test]
    async fn authenticate_returns_none_on_wrong_password() {
        let db = test_db().await;
        register(&db, "bob", "hunter2").await.unwrap();
        assert!(authenticate(&db, "bob", "wrongpassword").await.is_none());
    }

    #[tokio::test]
    async fn authenticate_returns_none_for_unknown_username() {
        let db = test_db().await;
        assert!(authenticate(&db, "nobody", "whatever").await.is_none());
    }

    #[tokio::test]
    async fn find_by_username_returns_account_without_password_check() {
        let db = test_db().await;
        register(&db, "carol", "secret").await.unwrap();
        let acct = find_by_username(&db, "carol").await;
        assert!(acct.is_some());
    }

    #[tokio::test]
    async fn find_or_create_mock_is_idempotent() {
        let db = test_db().await;
        let first = find_or_create_mock(&db).await;
        let second = find_or_create_mock(&db).await;
        assert_eq!(first.id, second.id);
        assert_eq!(first.username, second.username);
    }
}
