use crate::command::ClientId;
use crate::components::{
    CharacterId, InInventory, ItemId, ItemName, PlayerQuests, RoomContents, Stats, Wallet,
};
use crate::game_state::GameState;
use crate::item::{ItemLocation, ItemLocationSave};
use crate::quest::{PlayerQuestState, QuestObjectiveDef, QuestSave, QuestStatus};
use crate::systems::output::{send_to_client, OutputRegistry};

/// Marks objectives that match `predicate` as done, then checks for phase completion.
/// If all objectives are met and `completion_npc_id` is None, auto-completes the phase.
/// Otherwise sets status to `ReadyToTurnIn`.
pub fn quest_mark_objective(
    state: &mut GameState,
    entity: hecs::Entity,
    client_id: ClientId,
    registry: &OutputRegistry,
    quest_id_filter: Option<i64>,
    predicate: impl Fn(&QuestObjectiveDef) -> bool,
) {
    let to_mark: Vec<(i64, usize)> = {
        let Ok(pq) = state.world.get::<&PlayerQuests>(entity) else {
            return;
        };
        pq.0.iter()
            .filter(|s| s.status == QuestStatus::Active)
            .filter(|s| quest_id_filter.is_none_or(|id| s.quest_id == id))
            .flat_map(|s| {
                let def = state.quest_defs.get(&s.quest_id)?;
                let phase = def.phases.get(s.phase)?;
                Some(
                    phase
                        .objectives
                        .iter()
                        .enumerate()
                        .filter(|(idx, obj)| {
                            !s.objectives_met.get(*idx).copied().unwrap_or(true) && predicate(obj)
                        })
                        .map(|(idx, _)| (s.quest_id, idx))
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .collect()
    };

    if to_mark.is_empty() {
        return;
    }

    let char_id = state
        .world
        .get::<&CharacterId>(entity)
        .ok()
        .map(|c| c.db_id)
        .unwrap_or(0);

    for (quest_id, obj_idx) in to_mark {
        let (all_met, completion_npc_id, obj_description, quest_name) = {
            let Ok(mut pq) = state.world.get::<&mut PlayerQuests>(entity) else {
                continue;
            };
            let Some(qs) = pq.0.iter_mut().find(|s| s.quest_id == quest_id) else {
                continue;
            };
            if let Some(flag) = qs.objectives_met.get_mut(obj_idx) {
                *flag = true;
            }
            let all_met = qs.all_objectives_met();

            let def = state.quest_defs.get(&quest_id);
            let phase = def.and_then(|d| d.phases.get(qs.phase));
            let obj_desc = phase
                .and_then(|p| p.objectives.get(obj_idx))
                .map(|o| o.description().to_string())
                .unwrap_or_default();
            let comp_npc = phase.and_then(|p| p.completion_npc_id);
            let qname = def.map(|d| d.name.clone()).unwrap_or_default();
            (all_met, comp_npc, obj_desc, qname)
        };

        send_to_client(
            registry,
            client_id,
            format!(
                "<yellow>[Quest Update]</yellow> {}: {}",
                quest_name, obj_description
            ),
        );

        if all_met {
            if completion_npc_id.is_none() {
                quest_advance_phase(state, entity, quest_id, char_id, client_id, registry);
            } else {
                if let Ok(mut pq) = state.world.get::<&mut PlayerQuests>(entity) {
                    if let Some(qs) = pq.0.iter_mut().find(|s| s.quest_id == quest_id) {
                        qs.status = QuestStatus::ReadyToTurnIn;
                        let save_state = qs.clone();
                        state.pending_quest_saves.push(QuestSave {
                            char_id,
                            state: save_state,
                        });
                    }
                }
                let quest_name = state
                    .quest_defs
                    .get(&quest_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_default();
                send_to_client(
                    registry,
                    client_id,
                    format!(
                        "<yellow>[Quest Ready]</yellow> {} — find the completion NPC to turn in.",
                        quest_name
                    ),
                );
            }
        } else {
            let save_state: Option<PlayerQuestState> = state
                .world
                .get::<&PlayerQuests>(entity)
                .ok()
                .and_then(|pq| pq.0.iter().find(|s| s.quest_id == quest_id).cloned());
            if let Some(s) = save_state {
                state
                    .pending_quest_saves
                    .push(QuestSave { char_id, state: s });
            }
        }
    }
}

/// Advances a quest to the next phase or marks it Completed.
/// Awards XP, LP, and items for the completed phase. Sends feedback to the player.
pub fn quest_advance_phase(
    state: &mut GameState,
    entity: hecs::Entity,
    quest_id: i64,
    char_id: i64,
    client_id: ClientId,
    registry: &OutputRegistry,
) {
    let (
        current_phase,
        xp_reward,
        lp_reward,
        copper_reward,
        item_reward_ids,
        completion_text,
        quest_name,
        num_phases,
        next_desc,
        next_num_objs,
    ) = {
        let Ok(pq) = state.world.get::<&PlayerQuests>(entity) else {
            return;
        };
        let Some(qs) = pq.0.iter().find(|s| s.quest_id == quest_id) else {
            return;
        };
        let Some(def) = state.quest_defs.get(&quest_id) else {
            return;
        };
        let phase_def = &def.phases[qs.phase];
        let next = def.phases.get(qs.phase + 1);
        (
            qs.phase,
            phase_def.xp_reward,
            phase_def.lp_reward,
            phase_def.copper_reward,
            phase_def.item_rewards.clone(),
            phase_def.completion_text.clone(),
            def.name.clone(),
            def.phases.len(),
            next.map(|p| p.description.clone()),
            next.map(|p| p.objectives.len()).unwrap_or(0),
        )
    };

    let is_last = current_phase + 1 >= num_phases;

    if xp_reward > 0 || lp_reward > 0 {
        if let Ok(mut stats) = state.world.get::<&mut Stats>(entity) {
            if xp_reward > 0 {
                let gained = stats.add_experience(xp_reward);
                if gained > 0 {
                    let level = stats.level;
                    send_to_client(
                        registry,
                        client_id,
                        format!(
                            "<yellow>You feel more powerful! You are now level {level}.</yellow>"
                        ),
                    );
                }
            }
            if lp_reward > 0 {
                stats.learning_points += lp_reward;
            }
        }
    }

    if copper_reward > 0 {
        if let Ok(mut wallet) = state.world.get::<&mut Wallet>(entity) {
            wallet.0 += copper_reward;
        }
    }

    let mut rewarded_item_names: Vec<String> = Vec::new();
    for item_id in item_reward_ids {
        let item_entity = {
            let mut q = state.world.query::<(&ItemId, &ItemName)>();
            q.iter()
                .find(|(_, (id, _))| id.0 == item_id)
                .map(|(e, (_, name))| (e, name.0.clone()))
        };
        let Some((item_entity, item_name)) = item_entity else {
            continue;
        };
        state.world.remove_one::<RoomContents>(item_entity).ok();
        state
            .world
            .insert_one(item_entity, InInventory { owner: entity })
            .ok();
        state.pending_item_saves.push(ItemLocationSave {
            item_id,
            location: ItemLocation::Inventory { char_id },
        });
        send_to_client(
            registry,
            client_id,
            format!("<yellow>You receive: {item_name}.</yellow>"),
        );
        rewarded_item_names.push(item_name);
    }

    if !completion_text.is_empty() {
        send_to_client(registry, client_id, completion_text);
    }

    let new_state: PlayerQuestState = {
        let Ok(mut pq) = state.world.get::<&mut PlayerQuests>(entity) else {
            return;
        };
        let Some(qs) = pq.0.iter_mut().find(|s| s.quest_id == quest_id) else {
            return;
        };
        if is_last {
            qs.status = QuestStatus::Completed;
        } else {
            qs.phase = current_phase + 1;
            qs.objectives_met = vec![false; next_num_objs];
            qs.status = QuestStatus::Active;
        }
        qs.clone()
    };

    state.pending_quest_saves.push(QuestSave {
        char_id,
        state: new_state,
    });

    let mut reward_parts: Vec<String> = Vec::new();
    if xp_reward > 0 {
        reward_parts.push(format!("+{xp_reward} XP"));
    }
    if lp_reward > 0 {
        reward_parts.push(format!("+{lp_reward} LP"));
    }
    if copper_reward > 0 {
        reward_parts.push(format!(
            "+{}",
            crate::currency::format_copper(copper_reward)
        ));
    }
    let reward_suffix = if reward_parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", reward_parts.join(", "))
    };

    if is_last {
        send_to_client(
            registry,
            client_id,
            format!("<yellow>[Quest Complete]</yellow> {quest_name}{reward_suffix}"),
        );
    } else {
        send_to_client(
            registry,
            client_id,
            format!(
                "<yellow>[Quest Updated]</yellow> {quest_name} — Phase {}: {}{}",
                current_phase + 2,
                next_desc.unwrap_or_default(),
                reward_suffix
            ),
        );
    }
}

/// Auto-accepts all quests offered by `npc_db_id` that the player doesn't already have.
pub fn quest_accept_from_npc(
    state: &mut GameState,
    entity: hecs::Entity,
    npc_db_id: i64,
    client_id: ClientId,
    registry: &OutputRegistry,
) {
    let current_ids: Vec<i64> = state
        .world
        .get::<&PlayerQuests>(entity)
        .ok()
        .map(|pq| pq.0.iter().map(|s| s.quest_id).collect())
        .unwrap_or_default();

    let new_quests: Vec<crate::quest::QuestDef> = state
        .quest_defs
        .values()
        .filter(|def| def.giver_npc_id == Some(npc_db_id) && !current_ids.contains(&def.id))
        .cloned()
        .collect();

    let char_id = state
        .world
        .get::<&CharacterId>(entity)
        .ok()
        .map(|c| c.db_id)
        .unwrap_or(0);

    for def in new_quests {
        let num_objs = def.phases.first().map(|p| p.objectives.len()).unwrap_or(0);
        let new_state = PlayerQuestState::new_active(def.id, num_objs);

        {
            let Ok(mut pq) = state.world.get::<&mut PlayerQuests>(entity) else {
                continue;
            };
            pq.0.push(new_state.clone());
        }

        state.pending_quest_saves.push(QuestSave {
            char_id,
            state: new_state,
        });

        let phase_desc = def
            .phases
            .first()
            .map(|p| p.description.as_str())
            .unwrap_or("");
        send_to_client(
            registry,
            client_id,
            format!(
                "<yellow>[Quest Accepted]</yellow> {}\n   {}\n   Objective: {}",
                def.name, def.description, phase_desc
            ),
        );
    }
}

/// Auto-accepts all quests triggered by examining `item_db_id`.
pub fn quest_accept_from_item(
    state: &mut GameState,
    entity: hecs::Entity,
    item_db_id: i64,
    client_id: ClientId,
    registry: &OutputRegistry,
) {
    let current_ids: Vec<i64> = state
        .world
        .get::<&PlayerQuests>(entity)
        .ok()
        .map(|pq| pq.0.iter().map(|s| s.quest_id).collect())
        .unwrap_or_default();

    let new_quests: Vec<crate::quest::QuestDef> = state
        .quest_defs
        .values()
        .filter(|def| def.giver_item_id == Some(item_db_id) && !current_ids.contains(&def.id))
        .cloned()
        .collect();

    let char_id = state
        .world
        .get::<&CharacterId>(entity)
        .ok()
        .map(|c| c.db_id)
        .unwrap_or(0);

    for def in new_quests {
        let num_objs = def.phases.first().map(|p| p.objectives.len()).unwrap_or(0);
        let new_state = PlayerQuestState::new_active(def.id, num_objs);

        {
            let Ok(mut pq) = state.world.get::<&mut PlayerQuests>(entity) else {
                continue;
            };
            pq.0.push(new_state.clone());
        }

        state.pending_quest_saves.push(QuestSave {
            char_id,
            state: new_state,
        });

        let phase_desc = def
            .phases
            .first()
            .map(|p| p.description.as_str())
            .unwrap_or("");
        send_to_client(
            registry,
            client_id,
            format!(
                "<yellow>[Quest Started]</yellow> {}\n   {}\n   Objective: {}",
                def.name, def.description, phase_desc
            ),
        );
    }
}

/// Turns in ready quests whose current phase `completion_npc_id` matches `npc_db_id`.
pub fn quest_turn_in(
    state: &mut GameState,
    entity: hecs::Entity,
    npc_db_id: i64,
    client_id: ClientId,
    registry: &OutputRegistry,
) {
    let char_id = state
        .world
        .get::<&CharacterId>(entity)
        .ok()
        .map(|c| c.db_id)
        .unwrap_or(0);

    let ready_ids: Vec<i64> = {
        let Ok(pq) = state.world.get::<&PlayerQuests>(entity) else {
            return;
        };
        pq.0.iter()
            .filter(|s| s.status == QuestStatus::ReadyToTurnIn)
            .filter(|s| {
                state
                    .quest_defs
                    .get(&s.quest_id)
                    .and_then(|def| def.phases.get(s.phase))
                    .and_then(|p| p.completion_npc_id)
                    == Some(npc_db_id)
            })
            .map(|s| s.quest_id)
            .collect()
    };

    for quest_id in ready_ids {
        quest_advance_phase(state, entity, quest_id, char_id, client_id, registry);
    }
}
