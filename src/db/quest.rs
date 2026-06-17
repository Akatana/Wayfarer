use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};

use crate::quest::{PlayerQuestState, QuestDef, QuestObjectiveDef, QuestStatus};

// ── Seeding ───────────────────────────────────────────────────────────────────

/// Seeds quest definitions on first boot. Skips if the table already has rows.
pub async fn seed_if_empty(db: &DatabaseConnection, defs: &[QuestDef]) -> Result<(), DbErr> {
    let count: i64 = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS n FROM quests".to_string(),
        ))
        .await?
        .and_then(|r| r.try_get::<i64>("", "n").ok())
        .unwrap_or(0);

    if count > 0 {
        return Ok(());
    }

    for def in defs {
        db.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT OR IGNORE INTO quests (id, name, description, giver_npc_id, giver_item_id) VALUES (?,?,?,?,?)",
            [
                def.id.into(),
                def.name.clone().into(),
                def.description.clone().into(),
                def.giver_npc_id.into(),
                def.giver_item_id.into(),
            ],
        ))
        .await?;

        for (pi, phase) in def.phases.iter().enumerate() {
            db.execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT OR IGNORE INTO quest_phases (quest_id, phase_index, description, completion_npc_id, completion_text, xp_reward) VALUES (?,?,?,?,?,?)",
                [
                    def.id.into(),
                    (pi as i64).into(),
                    phase.description.clone().into(),
                    phase.completion_npc_id.into(),
                    phase.completion_text.clone().into(),
                    (phase.xp_reward as i64).into(),
                ],
            ))
            .await?;

            for (oi, obj) in phase.objectives.iter().enumerate() {
                let (obj_type, target_id, obj_desc): (&str, i64, String) = match obj {
                    QuestObjectiveDef::Talk {
                        npc_id,
                        description,
                    } => ("talk", *npc_id, description.clone()),
                    QuestObjectiveDef::Examine {
                        item_id,
                        description,
                    } => ("examine", *item_id, description.clone()),
                    QuestObjectiveDef::Reach {
                        room_id,
                        description,
                    } => ("reach", *room_id as i64, description.clone()),
                };
                db.execute(Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    "INSERT OR IGNORE INTO quest_objectives (quest_id, phase_index, obj_index, obj_type, target_id, description) VALUES (?,?,?,?,?,?)",
                    [
                        def.id.into(),
                        (pi as i64).into(),
                        (oi as i64).into(),
                        obj_type.into(),
                        target_id.into(),
                        obj_desc.into(),
                    ],
                ))
                .await?;
            }
        }
    }

    Ok(())
}

// ── Player quest state ────────────────────────────────────────────────────────

/// Loads all quest states for a character from the database.
pub async fn load_player_quests(
    db: &DatabaseConnection,
    char_id: i64,
) -> Result<Vec<PlayerQuestState>, DbErr> {
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT quest_id, phase_index, objectives_met, status FROM player_quests WHERE char_id = ?",
            [char_id.into()],
        ))
        .await?;

    let mut states = Vec::new();
    for row in rows {
        let quest_id: i64 = row.try_get("", "quest_id")?;
        let phase_index: i64 = row.try_get("", "phase_index")?;
        let objectives_json: String = row.try_get("", "objectives_met")?;
        let status_str: String = row.try_get("", "status")?;

        let objectives_met: Vec<bool> = serde_json::from_str(&objectives_json).unwrap_or_default();
        let status = match status_str.as_str() {
            "ready_to_turn_in" => QuestStatus::ReadyToTurnIn,
            "completed" => QuestStatus::Completed,
            _ => QuestStatus::Active,
        };

        states.push(PlayerQuestState {
            quest_id,
            phase: phase_index as usize,
            objectives_met,
            status,
        });
    }

    Ok(states)
}

/// Upserts a single player quest state. Skips characters with id == 0 (test actors).
pub async fn save_player_quest(
    db: &DatabaseConnection,
    char_id: i64,
    state: &PlayerQuestState,
) -> Result<(), DbErr> {
    if char_id == 0 {
        return Ok(());
    }
    let objectives_json =
        serde_json::to_string(&state.objectives_met).unwrap_or_else(|_| "[]".to_string());
    let status_str = match state.status {
        QuestStatus::Active => "active",
        QuestStatus::ReadyToTurnIn => "ready_to_turn_in",
        QuestStatus::Completed => "completed",
    };

    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT OR REPLACE INTO player_quests (char_id, quest_id, phase_index, objectives_met, status) VALUES (?,?,?,?,?)",
        [
            char_id.into(),
            state.quest_id.into(),
            (state.phase as i64).into(),
            objectives_json.into(),
            status_str.into(),
        ],
    ))
    .await?;

    Ok(())
}
