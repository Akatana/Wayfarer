use crate::character::CharacterData;
use crate::command::ClientId;
use crate::components::{
    AdminFlag, CharacterId, ClientConnection, Equipped, InInventory, ItemDescription, ItemId,
    ItemName, Name, NpcDescription, NpcId, PlayerQuests, Position, RoomContents, Stats, Wallet,
};
use crate::game_state::GameState;
use crate::item::{ItemLocation, ItemLocationSave};
use crate::quest::QuestStatus;
use crate::systems::output::{send_to_client, OutputRegistry};

pub(super) fn handle_connect(
    state: &mut GameState,
    client_id: ClientId,
    data: CharacterData,
    registry: &OutputRegistry,
) {
    if state.player_registry.is_connected(client_id) {
        return;
    }
    let entity = state.spawn_player_from_data(client_id, &data);
    let room_id = state.world.get::<&Position>(entity).ok().map(|p| p.room_id);

    if let Some(rid) = room_id {
        if let Some(desc) = super::describe_room(state, rid, entity) {
            send_to_client(
                registry,
                client_id,
                format!("Welcome, {}!\n\n{}", data.name, desc),
            );
        }
    }

    if let Some(rid) = room_id {
        let others: Vec<ClientId> = {
            let mut q = state.world.query::<(&Position, &ClientConnection)>();
            q.iter()
                .filter(|(e, (p, _))| *e != entity && p.room_id == rid)
                .map(|(_, (_, conn))| conn.client_id)
                .collect()
        };
        for id in others {
            send_to_client(
                registry,
                id,
                format!("{} has entered the world.", data.name),
            );
        }
    }
}

pub(super) fn handle_look(state: &GameState, client_id: ClientId, registry: &OutputRegistry) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    let room_id = {
        let Ok(pos) = state.world.get::<&Position>(entity) else {
            return;
        };
        pos.room_id
    };
    if let Some(desc) = super::describe_room(state, room_id, entity) {
        send_to_client(registry, client_id, desc);
    }
}

pub(super) fn handle_say(
    state: &GameState,
    client_id: ClientId,
    message: &str,
    registry: &OutputRegistry,
) {
    let Some(sender_entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    let sender_room_id = {
        let Ok(pos) = state.world.get::<&Position>(sender_entity) else {
            return;
        };
        pos.room_id
    };
    let recipients: Vec<ClientId> = {
        let mut q = state.world.query::<(&Position, &ClientConnection)>();
        q.iter()
            .filter(|(_, (pos, _))| pos.room_id == sender_room_id)
            .map(|(_, (_, conn))| conn.client_id)
            .collect()
    };
    for recipient_id in recipients {
        let msg = if recipient_id == client_id {
            format!("You say: \"{}\"", message)
        } else {
            format!("Someone says: \"{}\"", message)
        };
        send_to_client(registry, recipient_id, msg);
    }
}

pub(super) fn handle_examine(
    state: &mut GameState,
    client_id: ClientId,
    target: &str,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    let target = target.trim();
    if target.is_empty() {
        send_to_client(registry, client_id, "Examine what?".to_string());
        return;
    }

    let room_id = state.world.get::<&Position>(entity).ok().map(|p| p.room_id);
    let target_lower = target.to_lowercase();

    // Search floor → bag → equipped, in that priority order.
    let item: Option<hecs::Entity> = {
        let floor = room_id.and_then(|rid| {
            let mut q = state.world.query::<(&ItemName, &RoomContents)>();
            q.iter()
                .find(|(_, (n, rc))| {
                    rc.room_id == rid && n.0.to_lowercase().contains(&target_lower)
                })
                .map(|(e, _)| e)
        });
        if floor.is_some() {
            floor
        } else {
            let inv = {
                let mut q = state.world.query::<(&ItemName, &InInventory)>();
                q.iter()
                    .find(|(_, (n, inv))| {
                        inv.owner == entity && n.0.to_lowercase().contains(&target_lower)
                    })
                    .map(|(e, _)| e)
            };
            if inv.is_some() {
                inv
            } else {
                let mut q = state.world.query::<(&ItemName, &Equipped)>();
                q.iter()
                    .find(|(_, (n, eq))| {
                        eq.owner == entity && n.0.to_lowercase().contains(&target_lower)
                    })
                    .map(|(e, _)| e)
            }
        }
    };

    if let Some(item) = item {
        let name = state
            .world
            .get::<&ItemName>(item)
            .map(|n| n.0.clone())
            .unwrap_or_default();
        let desc = state
            .world
            .get::<&ItemDescription>(item)
            .map(|d| d.0.clone())
            .unwrap_or_else(|_| "Nothing remarkable.".to_string());
        send_to_client(
            registry,
            client_id,
            format!("<yellow>{}</yellow>\n   {}", name, desc),
        );

        // Check for item-triggered quest start or Examine objective.
        if let Ok(item_id) = state.world.get::<&ItemId>(item).map(|id| id.0) {
            super::quest_accept_from_item(state, entity, item_id, client_id, registry);
            super::quest_mark_objective(
                state,
                entity,
                client_id,
                registry,
                |obj| matches!(obj, crate::quest::QuestObjectiveDef::Examine { item_id: id, .. } if *id == item_id),
            );
        }
        return;
    }

    // Also check NPCs in the current room.
    if let Some(rid) = room_id {
        let npc = {
            let mut q = state.world.query::<(&Name, &Position, &NpcId)>();
            q.iter()
                .find(|(_, (n, pos, _))| {
                    pos.room_id == rid && n.0.to_lowercase().contains(&target_lower)
                })
                .map(|(e, (n, _, _))| (e, n.0.clone()))
        };
        if let Some((npc_e, npc_name)) = npc {
            let desc = state
                .world
                .get::<&NpcDescription>(npc_e)
                .map(|d| d.0.clone())
                .unwrap_or_else(|_| "Nothing remarkable.".to_string());
            send_to_client(
                registry,
                client_id,
                format!("<yellow>{}</yellow>\n   {}", npc_name, desc),
            );
            return;
        }
    }

    send_to_client(
        registry,
        client_id,
        format!("You don't see '{}' anywhere.", target),
    );
}

pub(super) fn handle_score(state: &GameState, client_id: ClientId, registry: &OutputRegistry) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };

    let name = state
        .world
        .get::<&Name>(entity)
        .map(|n| n.0.clone())
        .unwrap_or_default();
    let is_admin = state.world.get::<&AdminFlag>(entity).is_ok();
    let room_name = {
        let room_id = state.world.get::<&Position>(entity).ok().map(|p| p.room_id);
        room_id
            .and_then(|id| state.room_registry.get(id))
            .map(|r| r.name.clone())
            .unwrap_or_else(|| "Unknown".to_string())
    };

    let admin_tag = if is_admin {
        " <yellow>[Admin]</yellow>"
    } else {
        ""
    };
    let mut lines = vec![format!("<yellow>=== {} ===</yellow>{}", name, admin_tag)];

    if let Ok(s) = state.world.get::<&Stats>(entity) {
        lines.push(format!(
            "Level {}  |  XP: {}/{}  |  LP: {}",
            s.level,
            s.experience,
            s.xp_to_next_level(),
            s.learning_points
        ));
        lines.push(format!(
            "HP:  {}/{}   MP:  {}/{}",
            s.hp, s.max_hp, s.mp, s.max_mp
        ));
        lines.push(format!(
            "STR: {}   DEX: {}   KNW: {}",
            s.strength, s.dexterity, s.knowledge
        ));
    }
    if let Ok(w) = state.world.get::<&Wallet>(entity) {
        lines.push(format!("Wallet: {}", crate::currency::format_copper(w.0)));
    }
    lines.push(format!("Location: {}", room_name));

    send_to_client(registry, client_id, lines.join("\n"));
}

pub(super) fn handle_balance(state: &GameState, client_id: ClientId, registry: &OutputRegistry) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    let copper = state
        .world
        .get::<&Wallet>(entity)
        .ok()
        .map(|w| w.0)
        .unwrap_or(0);
    send_to_client(
        registry,
        client_id,
        format!("Wallet: {}", crate::currency::format_copper(copper)),
    );
}

pub(super) fn handle_quest_log(state: &GameState, client_id: ClientId, registry: &OutputRegistry) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };

    let Ok(pq) = state.world.get::<&PlayerQuests>(entity) else {
        send_to_client(
            registry,
            client_id,
            "<yellow>=== Quest Log ===</yellow>\n  (no quests)".to_string(),
        );
        return;
    };

    let active: Vec<_> =
        pq.0.iter()
            .filter(|s| s.status != QuestStatus::Completed)
            .collect();

    if active.is_empty() {
        send_to_client(
            registry,
            client_id,
            "<yellow>=== Quest Log ===</yellow>\n  (no active quests)".to_string(),
        );
        return;
    }

    let mut lines = vec!["<yellow>=== Quest Log ===</yellow>".to_string()];

    for qs in &active {
        let Some(def) = state.quest_defs.get(&qs.quest_id) else {
            continue;
        };
        let status_label = match qs.status {
            QuestStatus::Active => "<cyan>[Active]</cyan>",
            QuestStatus::ReadyToTurnIn => "<yellow>[Ready to Turn In]</yellow>",
            QuestStatus::Completed => "<green>[Complete]</green>",
        };
        lines.push(format!("\n{} {}", status_label, def.name));
        lines.push(format!("  {}", def.description));

        if let Some(phase) = def.phases.get(qs.phase) {
            lines.push(format!(
                "  Phase {}/{}: {}",
                qs.phase + 1,
                def.phases.len(),
                phase.description
            ));
            for (i, obj) in phase.objectives.iter().enumerate() {
                let met = qs.objectives_met.get(i).copied().unwrap_or(false);
                let tick = if met { "<green>[x]</green>" } else { "[ ]" };
                lines.push(format!("    {} {}", tick, obj.description()));
            }
            if qs.status == QuestStatus::ReadyToTurnIn {
                if let Some(npc_id) = phase.completion_npc_id {
                    lines.push(format!(
                        "  <yellow>→ Return to NPC #{} to complete.</yellow>",
                        npc_id
                    ));
                }
            }
        }
    }

    send_to_client(registry, client_id, lines.join("\n"));
}

pub(super) fn handle_quit(state: &mut GameState, client_id: ClientId, registry: &OutputRegistry) {
    send_to_client(
        registry,
        client_id,
        "Farewell. The world fades around you...".to_string(),
    );

    if let Some(entity) = state.player_registry.remove(client_id) {
        let room_id = state.world.get::<&Position>(entity).ok().map(|p| p.room_id);
        let player_name = state
            .world
            .get::<&Name>(entity)
            .map(|n| n.0.clone())
            .unwrap_or_default();

        if let Some(rid) = room_id {
            let others: Vec<ClientId> = {
                let mut q = state.world.query::<(&Position, &ClientConnection)>();
                q.iter()
                    .filter(|(e, (p, _))| *e != entity && p.room_id == rid)
                    .map(|(_, (_, conn))| conn.client_id)
                    .collect()
            };
            for id in others {
                send_to_client(registry, id, format!("{} has left the world.", player_name));
            }
        }

        // Queue item location saves then despawn all items owned by this player.
        let char_id = state
            .world
            .get::<&CharacterId>(entity)
            .map(|c| c.db_id)
            .unwrap_or(0);
        let owned: Vec<(hecs::Entity, Option<i64>)> = {
            let inv: Vec<_> = {
                let mut q = state.world.query::<(&InInventory, Option<&ItemId>)>();
                q.iter()
                    .filter(|(_, (inv, _))| inv.owner == entity)
                    .map(|(e, (_, id))| (e, id.map(|i| i.0)))
                    .collect()
            };
            let eq: Vec<_> = {
                let mut q = state.world.query::<(&Equipped, Option<&ItemId>)>();
                q.iter()
                    .filter(|(_, (eq, _))| eq.owner == entity)
                    .map(|(e, (_, id))| (e, id.map(|i| i.0)))
                    .collect()
            };
            inv.into_iter().chain(eq).collect()
        };
        for (item_ent, item_id) in owned {
            if let Some(id) = item_id {
                state.pending_item_saves.push(ItemLocationSave {
                    item_id: id,
                    location: ItemLocation::Inventory { char_id },
                });
            }
            state.world.despawn(item_ent).ok();
        }

        if let Some(save_data) = state.extract_save_data(entity) {
            state.pending_saves.push(save_data);
        }
        state.world.despawn(entity).ok();
    }
}

pub(super) fn handle_unknown(client_id: ClientId, raw: &str, registry: &OutputRegistry) {
    send_to_client(
        registry,
        client_id,
        format!("Huh? '{}' isn't something I understand.", raw),
    );
}
