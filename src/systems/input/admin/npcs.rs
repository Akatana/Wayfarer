use crate::command::ClientId;
use crate::components::{
    Hostile, Name, NpcDescription, NpcGreeting, NpcId, NpcRoutine, Passive, PatrolRoute, Position,
};
use crate::game_state::{AdminDbOp, GameState};
use crate::npc::NpcData;
use crate::systems::output::{send_to_client, OutputRegistry};

pub(crate) fn find_npc_entity(world: &hecs::World, npc_id: i64) -> Option<hecs::Entity> {
    let mut q = world.query::<(&NpcId,)>();
    q.iter().find(|(_, (id,))| id.0 == npc_id).map(|(e, _)| e)
}

pub(crate) fn handle_admin_mnpc(
    state: &mut GameState,
    client_id: ClientId,
    spec: String,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }
    let Some(room_id) = super::get_player_room(state, entity) else {
        return;
    };

    let (name, description) = match spec.split_once('/') {
        Some((n, d)) => (n.trim().to_string(), d.trim().to_string()),
        None => (spec.trim().to_string(), String::new()),
    };
    if name.is_empty() {
        send_to_client(
            registry,
            client_id,
            "Usage: @mnpc <name> [/ <description>]".to_string(),
        );
        return;
    }

    let npc_id = state.next_npc_id;
    state.next_npc_id += 1;

    let npc_data = NpcData {
        id: npc_id,
        name: name.clone(),
        description: description.clone(),
        greeting: None,
        hostile: false,
        passive: false,
        room_id,
        patrol: Vec::new(),
        max_hp: 20,
        min_damage: 1,
        max_damage: 4,
        attack_ticks: 10,
        xp_reward: 10,
    };

    state.world.spawn((
        NpcId(npc_id),
        Name(name.clone()),
        NpcDescription(description),
        Position { room_id },
        NpcRoutine {
            last_action_tick: 0,
        },
    ));

    state.pending_admin_ops.push(AdminDbOp::CreateNpc(npc_data));

    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] NPC '{name}' (#{npc_id}) created in this room.</dim>"),
    );
}

pub(crate) fn handle_admin_ndestroy(
    state: &mut GameState,
    client_id: ClientId,
    target: &str,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }
    let target = target.trim();

    // Accept either a numeric id (searches world-wide) or a name (searches current room).
    let found = if let Ok(id) = target.parse::<i64>() {
        let mut q = state.world.query::<(&Name, &NpcId)>();
        q.iter()
            .find(|(_, (_, nid))| nid.0 == id)
            .map(|(e, (n, nid))| (e, n.0.clone(), nid.0))
    } else {
        let Some(room_id) = super::get_player_room(state, entity) else {
            return;
        };
        let target_lower = target.to_lowercase();
        let mut q = state.world.query::<(&Name, &Position, &NpcId)>();
        q.iter()
            .find(|(_, (n, pos, _))| {
                pos.room_id == room_id && n.0.to_lowercase().contains(&target_lower)
            })
            .map(|(e, (n, _, nid))| (e, n.0.clone(), nid.0))
    };

    let Some((npc_ent, npc_name, npc_id)) = found else {
        send_to_client(
            registry,
            client_id,
            format!("No NPC matching '{}' found.", target),
        );
        return;
    };

    state.world.despawn(npc_ent).ok();
    state.pending_admin_ops.push(AdminDbOp::DeleteNpc(npc_id));

    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] NPC '{npc_name}' permanently destroyed.</dim>"),
    );
}

pub(crate) fn handle_admin_nname(
    state: &mut GameState,
    client_id: ClientId,
    npc_id: i64,
    name: String,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }
    let Some(npc_ent) = find_npc_entity(&state.world, npc_id) else {
        send_to_client(
            registry,
            client_id,
            format!("No NPC with id #{npc_id} exists."),
        );
        return;
    };
    if let Ok(mut n) = state.world.get::<&mut Name>(npc_ent) {
        n.0 = name.clone();
    }
    state.pending_admin_ops.push(AdminDbOp::UpdateNpcName {
        id: npc_id,
        name: name.clone(),
    });
    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] NPC #{npc_id} renamed to '{name}'.</dim>"),
    );
}

pub(crate) fn handle_admin_ndesc(
    state: &mut GameState,
    client_id: ClientId,
    npc_id: i64,
    description: String,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }
    let Some(npc_ent) = find_npc_entity(&state.world, npc_id) else {
        send_to_client(
            registry,
            client_id,
            format!("No NPC with id #{npc_id} exists."),
        );
        return;
    };
    state
        .world
        .insert(npc_ent, (NpcDescription(description.clone()),))
        .ok();
    state.pending_admin_ops.push(AdminDbOp::UpdateNpcDesc {
        id: npc_id,
        description,
    });
    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] NPC #{npc_id} description updated.</dim>"),
    );
}

pub(crate) fn handle_admin_ngreet(
    state: &mut GameState,
    client_id: ClientId,
    npc_id: i64,
    text: String,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }
    let Some(npc_ent) = find_npc_entity(&state.world, npc_id) else {
        send_to_client(
            registry,
            client_id,
            format!("No NPC with id #{npc_id} exists."),
        );
        return;
    };

    let lower = text.trim().to_lowercase();
    if lower == "none" {
        state.world.remove::<(NpcGreeting,)>(npc_ent).ok();
        state.pending_admin_ops.push(AdminDbOp::UpdateNpcGreet {
            id: npc_id,
            greeting: None,
        });
        send_to_client(
            registry,
            client_id,
            format!("<dim>[Admin] NPC #{npc_id} greeting cleared.</dim>"),
        );
    } else {
        state
            .world
            .insert(npc_ent, (NpcGreeting(text.clone()),))
            .ok();
        state.pending_admin_ops.push(AdminDbOp::UpdateNpcGreet {
            id: npc_id,
            greeting: Some(text.clone()),
        });
        send_to_client(
            registry,
            client_id,
            format!("<dim>[Admin] NPC #{npc_id} greeting set.</dim>"),
        );
    }
}

pub(crate) fn handle_admin_nhostile(
    state: &mut GameState,
    client_id: ClientId,
    npc_id: i64,
    hostile: bool,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }
    let Some(npc_ent) = find_npc_entity(&state.world, npc_id) else {
        send_to_client(
            registry,
            client_id,
            format!("No NPC with id #{npc_id} exists."),
        );
        return;
    };

    if hostile {
        state.world.insert(npc_ent, (Hostile,)).ok();
    } else {
        state.world.remove::<(Hostile,)>(npc_ent).ok();
    }
    state.pending_admin_ops.push(AdminDbOp::UpdateNpcHostile {
        id: npc_id,
        hostile,
    });
    let label = if hostile { "hostile" } else { "friendly" };
    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] NPC #{npc_id} is now {label}.</dim>"),
    );
}

pub(crate) fn handle_admin_npassive(
    state: &mut GameState,
    client_id: ClientId,
    npc_id: i64,
    passive: bool,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }
    let Some(npc_ent) = find_npc_entity(&state.world, npc_id) else {
        send_to_client(
            registry,
            client_id,
            format!("No NPC with id #{npc_id} exists."),
        );
        return;
    };

    if passive {
        state.world.insert(npc_ent, (Passive,)).ok();
    } else {
        state.world.remove::<(Passive,)>(npc_ent).ok();
    }
    state.pending_admin_ops.push(AdminDbOp::UpdateNpcPassive {
        id: npc_id,
        passive,
    });
    let label = if passive { "passive" } else { "retaliating" };
    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] NPC #{npc_id} is now {label}.</dim>"),
    );
}

pub(crate) fn handle_admin_npatrol(
    state: &mut GameState,
    client_id: ClientId,
    npc_id: i64,
    spec: String,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }
    let Some(npc_ent) = find_npc_entity(&state.world, npc_id) else {
        send_to_client(
            registry,
            client_id,
            format!("No NPC with id #{npc_id} exists."),
        );
        return;
    };

    let lower = spec.trim().to_lowercase();
    let rooms: Vec<u64> = if lower == "none" {
        Vec::new()
    } else {
        let parsed: Result<Vec<u64>, _> =
            lower.split(',').map(|s| s.trim().parse::<u64>()).collect();
        match parsed {
            Ok(v) => v,
            Err(_) => {
                send_to_client(
                    registry,
                    client_id,
                    "Invalid patrol spec. Use comma-separated room ids or 'none'.".to_string(),
                );
                return;
            }
        }
    };

    if rooms.is_empty() {
        state.world.remove::<(PatrolRoute,)>(npc_ent).ok();
        send_to_client(
            registry,
            client_id,
            format!("<dim>[Admin] NPC #{npc_id} patrol cleared â€” now stationary.</dim>"),
        );
    } else {
        let current_room = state
            .world
            .get::<&Position>(npc_ent)
            .ok()
            .map(|p| p.room_id)
            .unwrap_or(0);
        let index = rooms.iter().position(|&r| r == current_room).unwrap_or(0);
        state
            .world
            .insert(
                npc_ent,
                (PatrolRoute {
                    rooms: rooms.clone(),
                    index,
                },),
            )
            .ok();
        send_to_client(
            registry,
            client_id,
            format!(
                "<dim>[Admin] NPC #{npc_id} patrol set: {}.</dim>",
                rooms
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<_>>()
                    .join(" â†’ ")
            ),
        );
    }
    state
        .pending_admin_ops
        .push(AdminDbOp::SetNpcPatrol { id: npc_id, rooms });
}

pub(crate) fn handle_admin_nlist(
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

    let mut entries: Vec<(i64, String, u64, bool, bool)> = {
        let mut q = state.world.query::<(&NpcId, &Name, &Position)>();
        q.iter()
            .map(|(e, (id, name, pos))| {
                let hostile = state.world.get::<&Hostile>(e).is_ok();
                let patrol = state.world.get::<&PatrolRoute>(e).is_ok();
                (id.0, name.0.clone(), pos.room_id, hostile, patrol)
            })
            .collect()
    };
    entries.sort_by_key(|e| e.0);

    if entries.is_empty() {
        send_to_client(registry, client_id, "[Admin] No NPCs in world.".to_string());
        return;
    }

    let mut lines = vec![format!("[Admin] NPCs ({} total):", entries.len())];
    for (id, name, room_id, hostile, patrol) in entries {
        let tags = match (hostile, patrol) {
            (true, true) => " [hostile] [patrol]",
            (true, false) => " [hostile]",
            (false, true) => " [patrol]",
            _ => "",
        };
        lines.push(format!("  #{id:<6} {name:<30} room {room_id}{tags}"));
    }
    send_to_client(registry, client_id, lines.join("\n"));
}

pub(crate) fn handle_admin_ninfo(
    state: &GameState,
    client_id: ClientId,
    npc_id: i64,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }
    let Some(npc_ent) = find_npc_entity(&state.world, npc_id) else {
        send_to_client(
            registry,
            client_id,
            format!("No NPC with id #{npc_id} exists."),
        );
        return;
    };

    let name = state
        .world
        .get::<&Name>(npc_ent)
        .map(|n| n.0.clone())
        .unwrap_or_default();
    let desc = state
        .world
        .get::<&NpcDescription>(npc_ent)
        .map(|d| d.0.clone())
        .unwrap_or_default();
    let greeting = state
        .world
        .get::<&NpcGreeting>(npc_ent)
        .ok()
        .map(|g| format!("\"{}\"", g.0))
        .unwrap_or_else(|| "(none)".to_string());
    let hostile = state.world.get::<&Hostile>(npc_ent).is_ok();
    let passive = state.world.get::<&Passive>(npc_ent).is_ok();
    let room_id = state
        .world
        .get::<&Position>(npc_ent)
        .map(|p| p.room_id)
        .unwrap_or(0);
    let room_name = state
        .room_registry
        .get(room_id)
        .map(|r| r.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());
    let patrol_str = state
        .world
        .get::<&PatrolRoute>(npc_ent)
        .ok()
        .map(|pr| {
            pr.rooms
                .iter()
                .map(|r| r.to_string())
                .collect::<Vec<_>>()
                .join(" â†’ ")
        })
        .unwrap_or_else(|| "(stationary)".to_string());

    let lines = [
        format!("[Admin] NPC #{npc_id} â€” {name}"),
        format!("  Description: {desc}"),
        format!("  Greeting:    {greeting}"),
        format!("  Hostile:     {}", if hostile { "yes" } else { "no" }),
        format!("  Passive:     {}", if passive { "yes" } else { "no" }),
        format!("  Room:        {room_id} ({room_name})"),
        format!("  Patrol:      {patrol_str}"),
    ];
    send_to_client(registry, client_id, lines.join("\n"));
}
