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
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id      INTEGER NOT NULL DEFAULT 0,
            name            TEXT    NOT NULL UNIQUE,
            room_id         INTEGER NOT NULL DEFAULT 1,
            hp              INTEGER NOT NULL DEFAULT 100,
            max_hp          INTEGER NOT NULL DEFAULT 100,
            mp              INTEGER NOT NULL DEFAULT 10,
            max_mp          INTEGER NOT NULL DEFAULT 10,
            strength        INTEGER NOT NULL DEFAULT 0,
            dexterity       INTEGER NOT NULL DEFAULT 0,
            knowledge       INTEGER NOT NULL DEFAULT 0,
            level           INTEGER NOT NULL DEFAULT 1,
            experience      INTEGER NOT NULL DEFAULT 0,
            learning_points INTEGER NOT NULL DEFAULT 0,
            copper          INTEGER NOT NULL DEFAULT 0
        )"
        .to_string(),
    ))
    .await?;

    // Additive migrations — fail silently on fresh DBs that already have the column.
    for sql in [
        "ALTER TABLE items ADD COLUMN def_id       INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE npcs ADD COLUMN max_hp       INTEGER NOT NULL DEFAULT 20",
        "ALTER TABLE npcs ADD COLUMN min_damage   INTEGER NOT NULL DEFAULT 1",
        "ALTER TABLE npcs ADD COLUMN max_damage   INTEGER NOT NULL DEFAULT 4",
        "ALTER TABLE npcs ADD COLUMN attack_ticks INTEGER NOT NULL DEFAULT 10",
        "ALTER TABLE npcs ADD COLUMN xp_reward    INTEGER NOT NULL DEFAULT 10",
        "ALTER TABLE npcs ADD COLUMN passive      INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE characters ADD COLUMN account_id      INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE characters ADD COLUMN strength        INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE characters ADD COLUMN dexterity       INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE characters ADD COLUMN knowledge       INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE characters ADD COLUMN level           INTEGER NOT NULL DEFAULT 1",
        "ALTER TABLE characters ADD COLUMN experience      INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE characters ADD COLUMN learning_points INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE characters ADD COLUMN copper          INTEGER NOT NULL DEFAULT 0",
    ] {
        let _ = db
            .execute(Statement::from_string(DbBackend::Sqlite, sql.to_string()))
            .await;
    }

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE IF NOT EXISTS items (
            id              INTEGER PRIMARY KEY,
            def_id          INTEGER NOT NULL DEFAULT 0,
            name            TEXT    NOT NULL,
            description     TEXT    NOT NULL,
            equip_slot      TEXT,
            two_handed      INTEGER NOT NULL DEFAULT 0,
            bag_capacity    INTEGER,
            req_level       INTEGER NOT NULL DEFAULT 0,
            req_strength    INTEGER NOT NULL DEFAULT 0,
            req_dexterity   INTEGER NOT NULL DEFAULT 0,
            req_knowledge   INTEGER NOT NULL DEFAULT 0,
            location        TEXT    NOT NULL DEFAULT 'room',
            room_id         INTEGER,
            char_id         INTEGER,
            equipped_slot   TEXT
        )"
        .to_string(),
    ))
    .await?;

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE IF NOT EXISTS npcs (
            id           INTEGER PRIMARY KEY,
            name         TEXT    NOT NULL,
            description  TEXT    NOT NULL DEFAULT '',
            greeting     TEXT,
            hostile      INTEGER NOT NULL DEFAULT 0,
            passive      INTEGER NOT NULL DEFAULT 0,
            room_id      INTEGER NOT NULL DEFAULT 1,
            max_hp       INTEGER NOT NULL DEFAULT 20,
            min_damage   INTEGER NOT NULL DEFAULT 1,
            max_damage   INTEGER NOT NULL DEFAULT 4,
            attack_ticks INTEGER NOT NULL DEFAULT 10,
            xp_reward    INTEGER NOT NULL DEFAULT 10
        )"
        .to_string(),
    ))
    .await?;

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE IF NOT EXISTS npc_patrol_routes (
            npc_id  INTEGER NOT NULL,
            step    INTEGER NOT NULL,
            room_id INTEGER NOT NULL,
            PRIMARY KEY (npc_id, step),
            FOREIGN KEY (npc_id) REFERENCES npcs(id)
        )"
        .to_string(),
    ))
    .await?;

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE IF NOT EXISTS npc_loot_table (
            npc_id  INTEGER NOT NULL,
            step    INTEGER NOT NULL,
            item_id INTEGER NOT NULL,
            chance  REAL    NOT NULL DEFAULT 1.0,
            PRIMARY KEY (npc_id, step),
            FOREIGN KEY (npc_id) REFERENCES npcs(id)
        )"
        .to_string(),
    ))
    .await?;

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE IF NOT EXISTS item_definitions (
            id            INTEGER PRIMARY KEY,
            name          TEXT    NOT NULL,
            description   TEXT    NOT NULL DEFAULT '',
            equip_slot    TEXT,
            two_handed    INTEGER NOT NULL DEFAULT 0,
            bag_capacity  INTEGER,
            req_level     INTEGER NOT NULL DEFAULT 0,
            req_strength  INTEGER NOT NULL DEFAULT 0,
            req_dexterity INTEGER NOT NULL DEFAULT 0,
            req_knowledge INTEGER NOT NULL DEFAULT 0
        )"
        .to_string(),
    ))
    .await?;

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

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE IF NOT EXISTS quests (
            id            INTEGER PRIMARY KEY,
            name          TEXT    NOT NULL,
            description   TEXT    NOT NULL,
            giver_npc_id  INTEGER,
            giver_item_id INTEGER
        )"
        .to_string(),
    ))
    .await?;

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE IF NOT EXISTS quest_phases (
            quest_id          INTEGER NOT NULL,
            phase_index       INTEGER NOT NULL,
            description       TEXT    NOT NULL,
            completion_npc_id INTEGER,
            completion_text   TEXT    NOT NULL DEFAULT '',
            xp_reward         INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (quest_id, phase_index)
        )"
        .to_string(),
    ))
    .await?;

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE IF NOT EXISTS quest_objectives (
            quest_id    INTEGER NOT NULL,
            phase_index INTEGER NOT NULL,
            obj_index   INTEGER NOT NULL,
            obj_type    TEXT    NOT NULL,
            target_id   INTEGER NOT NULL,
            description TEXT    NOT NULL,
            PRIMARY KEY (quest_id, phase_index, obj_index)
        )"
        .to_string(),
    ))
    .await?;

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE IF NOT EXISTS player_quests (
            char_id        INTEGER NOT NULL,
            quest_id       INTEGER NOT NULL,
            phase_index    INTEGER NOT NULL DEFAULT 0,
            objectives_met TEXT    NOT NULL DEFAULT '[]',
            status         TEXT    NOT NULL DEFAULT 'active',
            PRIMARY KEY (char_id, quest_id)
        )"
        .to_string(),
    ))
    .await?;

    Ok(())
}
