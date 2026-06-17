pub mod account;
pub mod character;
pub mod item;
pub mod room;
pub mod schema;

use sea_orm::{Database, DatabaseConnection};

/// Opens a SeaORM connection to the given SQLite URL.
///
/// For a file-based DB: `"sqlite://./wayfarer.db?mode=rwc"`
/// For an in-memory DB (tests): `"sqlite::memory:"`
pub async fn connect(url: &str) -> Result<DatabaseConnection, sea_orm::DbErr> {
    Database::connect(url).await
}
