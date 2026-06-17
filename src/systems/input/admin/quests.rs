use crate::command::ClientId;
use crate::components::{CharacterId, ClientConnection, Name, PlayerQuests};
use crate::game_state::GameState;
use crate::quest::{PlayerQuestState, QuestSave, QuestStatus};
use crate::systems::output::{send_to_client, OutputRegistry};

pub(crate) fn handle_admin_qlist(
    state: &GameState,
    client_id: ClientId,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    if state.quest_defs.is_empty() {
        send_to_client(
            registry,
            client_id,
            "<yellow>=== Quest Definitions ===</yellow>\n  (none loaded)".to_string(),
        );
        return;
    }

    let mut defs: Vec<_> = state.quest_defs.values().collect();
    defs.sort_by_key(|d| d.id);

    let mut lines = vec![format!(
        "<yellow>=== Quest Definitions ({}) ===</yellow>",
        defs.len()
    )];
    for def in defs {
        let source = match (def.giver_npc_id, def.giver_item_id) {
            (Some(n), _) => format!("NPC #{n}"),
            (_, Some(i)) => format!("item #{i}"),
            _ => "—".to_string(),
        };
        lines.push(format!(
            "  #{:<4} {} [giver: {}] ({} phases)",
            def.id,
            def.name,
            source,
            def.phases.len()
        ));
    }
    send_to_client(registry, client_id, lines.join("\n"));
}

pub(crate) fn handle_admin_qinfo(
    state: &GameState,
    client_id: ClientId,
    quest_id: i64,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    let Some(def) = state.quest_defs.get(&quest_id) else {
        send_to_client(registry, client_id, format!("Quest #{quest_id} not found."));
        return;
    };

    let mut lines = vec![
        format!("<yellow>=== Quest #{}: {} ===</yellow>", def.id, def.name),
        format!("  {}", def.description),
    ];
    if let Some(n) = def.giver_npc_id {
        lines.push(format!("  Giver NPC : #{n}"));
    }
    if let Some(i) = def.giver_item_id {
        lines.push(format!("  Giver item: #{i}"));
    }

    for (pi, phase) in def.phases.iter().enumerate() {
        lines.push(format!(
            "\n  Phase {}/{}: {}",
            pi + 1,
            def.phases.len(),
            phase.description
        ));
        for (oi, obj) in phase.objectives.iter().enumerate() {
            lines.push(format!("    {}. {}", oi + 1, obj.description()));
        }
        if let Some(n) = phase.completion_npc_id {
            lines.push(format!("    → Turn in: NPC #{n}"));
        } else {
            lines.push("    → Auto-completes on all objectives done".to_string());
        }
        lines.push(format!("    XP reward: {}", phase.xp_reward));
    }

    send_to_client(registry, client_id, lines.join("\n"));
}

pub(crate) fn handle_admin_qgive(
    state: &mut GameState,
    client_id: ClientId,
    target_name: String,
    quest_id: i64,
    registry: &OutputRegistry,
) {
    let Some(admin_entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, admin_entity, registry) {
        return;
    }

    let Some(def) = state.quest_defs.get(&quest_id).cloned() else {
        send_to_client(registry, client_id, format!("Quest #{quest_id} not found."));
        return;
    };

    let name_lower = target_name.to_lowercase();
    let target_entity = {
        let mut q = state.world.query::<(&Name, &ClientConnection)>();
        q.iter()
            .find(|(_, (n, _))| n.0.to_lowercase() == name_lower)
            .map(|(e, _)| e)
    };

    let Some(target) = target_entity else {
        send_to_client(
            registry,
            client_id,
            format!("No online player named '{}'.", target_name),
        );
        return;
    };

    // Check if already has this quest.
    let already_has = state
        .world
        .get::<&PlayerQuests>(target)
        .ok()
        .map(|pq| pq.0.iter().any(|s| s.quest_id == quest_id))
        .unwrap_or(false);

    if already_has {
        send_to_client(
            registry,
            client_id,
            format!("{} already has quest '{}'.", target_name, def.name),
        );
        return;
    }

    let num_objs = def.phases.first().map(|p| p.objectives.len()).unwrap_or(0);
    let new_state = PlayerQuestState::new_active(quest_id, num_objs);

    let char_id = state
        .world
        .get::<&CharacterId>(target)
        .ok()
        .map(|c| c.db_id)
        .unwrap_or(0);

    {
        let Ok(mut pq) = state.world.get::<&mut PlayerQuests>(target) else {
            return;
        };
        pq.0.push(new_state.clone());
    }

    state.pending_quest_saves.push(QuestSave {
        char_id,
        state: new_state,
    });

    send_to_client(
        registry,
        client_id,
        format!(
            "<dim>[Admin] Gave quest '{}' to {}.</dim>",
            def.name, target_name
        ),
    );

    // Notify the target player.
    let target_client_id = state
        .world
        .get::<&crate::components::ClientConnection>(target)
        .map(|c| c.client_id)
        .unwrap_or(0);
    if target_client_id != 0 {
        let phase_desc = def
            .phases
            .first()
            .map(|p| p.description.as_str())
            .unwrap_or("");
        send_to_client(
            registry,
            target_client_id,
            format!(
                "<yellow>[Quest Granted]</yellow> {}\n   {}\n   Objective: {}",
                def.name, def.description, phase_desc
            ),
        );
    }
}

pub(crate) fn handle_admin_qreset(
    state: &mut GameState,
    client_id: ClientId,
    target_name: String,
    quest_id: i64,
    registry: &OutputRegistry,
) {
    let Some(admin_entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, admin_entity, registry) {
        return;
    }

    let Some(def) = state.quest_defs.get(&quest_id).cloned() else {
        send_to_client(registry, client_id, format!("Quest #{quest_id} not found."));
        return;
    };

    let name_lower = target_name.to_lowercase();
    let target_entity = {
        let mut q = state.world.query::<(&Name, &ClientConnection)>();
        q.iter()
            .find(|(_, (n, _))| n.0.to_lowercase() == name_lower)
            .map(|(e, _)| e)
    };

    let Some(target) = target_entity else {
        send_to_client(
            registry,
            client_id,
            format!("No online player named '{}'.", target_name),
        );
        return;
    };

    let num_objs = def.phases.first().map(|p| p.objectives.len()).unwrap_or(0);
    let reset_state = PlayerQuestState::new_active(quest_id, num_objs);

    let char_id = state
        .world
        .get::<&CharacterId>(target)
        .ok()
        .map(|c| c.db_id)
        .unwrap_or(0);

    let found = {
        let Ok(mut pq) = state.world.get::<&mut PlayerQuests>(target) else {
            send_to_client(
                registry,
                client_id,
                format!("{} does not have quest '{}' active.", target_name, def.name),
            );
            return;
        };
        if let Some(qs) = pq.0.iter_mut().find(|s| s.quest_id == quest_id) {
            *qs = reset_state.clone();
            true
        } else {
            false
        }
    };

    if !found {
        send_to_client(
            registry,
            client_id,
            format!("{} does not have quest '{}' active.", target_name, def.name),
        );
        return;
    }

    state.pending_quest_saves.push(QuestSave {
        char_id,
        state: reset_state,
    });

    send_to_client(
        registry,
        client_id,
        format!(
            "<dim>[Admin] Reset quest '{}' for {}.</dim>",
            def.name, target_name
        ),
    );

    let target_client_id = state
        .world
        .get::<&crate::components::ClientConnection>(target)
        .map(|c| c.client_id)
        .unwrap_or(0);
    if target_client_id != 0 {
        send_to_client(
            registry,
            target_client_id,
            format!(
                "<yellow>[Quest Reset]</yellow> {} has been reset to its start.",
                def.name
            ),
        );
    }
}

/// Helper: used by qgive/qreset to mark `QuestStatus` as Active for the save.
#[allow(dead_code)]
fn _status_active() -> QuestStatus {
    QuestStatus::Active
}
