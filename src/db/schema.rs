use sea_orm::ConnectionTrait;
use sea_orm::DatabaseConnection;
use sea_orm::DbBackend;
use sea_orm::Statement;

/// Creates all tables if they do not already exist.
/// Safe to call on every startup — uses `IF NOT EXISTS` throughout.
/// Also runs additive column migrations for schema upgrades on existing DBs.
pub async fn create_tables(db: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE IF NOT EXISTS accounts (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            username      TEXT    NOT NULL UNIQUE,
            password_hash TEXT    NOT NULL,
            is_admin      INTEGER NOT NULL DEFAULT 0
        )"
        .to_string(),
    ))
    .await?;

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE IF NOT EXISTS characters (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id INTEGER NOT NULL DEFAULT 0,
            name       TEXT    NOT NULL UNIQUE,
            room_id    INTEGER NOT NULL DEFAULT 1,
            hp         INTEGER NOT NULL DEFAULT 100,
            max_hp     INTEGER NOT NULL DEFAULT 100,
            mp         INTEGER NOT NULL DEFAULT 50,
            max_mp     INTEGER NOT NULL DEFAULT 50
        )"
        .to_string(),
    ))
    .await?;

    // Migration: add account_id to existing characters tables that predate this column.
    // Fails silently on fresh DBs where the column is already in the CREATE TABLE DDL.
    let _ = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "ALTER TABLE characters ADD COLUMN account_id INTEGER NOT NULL DEFAULT 0".to_string(),
        ))
        .await;

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE IF NOT EXISTS rooms (
            id          INTEGER PRIMARY KEY,
            name        TEXT    NOT NULL,
            description TEXT    NOT NULL
        )"
        .to_string(),
    ))
    .await?;

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE IF NOT EXISTS exits (
            room_id             INTEGER NOT NULL,
            direction           TEXT    NOT NULL,
            destination_room_id INTEGER NOT NULL,
            PRIMARY KEY (room_id, direction),
            FOREIGN KEY (room_id) REFERENCES rooms(id)
        )"
        .to_string(),
    ))
    .await?;

    Ok(())
}
