use crate::command::ClientId;
use crate::components::{InCombat, Name, NpcCombatStats, NpcId, Passive, Position, Stats};
use crate::game_state::GameState;
use crate::systems::combat::{clear_combat, player_attack_interval, send_combat_status};
use crate::systems::output::{send_to_client, OutputRegistry};

pub(super) fn handle_attack(
    state: &mut GameState,
    client_id: ClientId,
    target_name: &str,
    registry: &OutputRegistry,
) {
    let Some(player) = state.player_registry.get_entity(client_id) else {
        return;
    };
    let target_lower = target_name.trim().to_lowercase();
    if target_lower.is_empty() {
        send_to_client(registry, client_id, "Attack what?".to_string());
        return;
    }

    let room_id = state.world.get::<&Position>(player).ok().map(|p| p.room_id);
    let Some(room_id) = room_id else { return };

    // Find NPC in the same room whose name matches.
    let npc_entity: Option<hecs::Entity> = {
        let mut q = state
            .world
            .query::<(&Name, &Position, &NpcId, &NpcCombatStats)>();
        q.iter()
            .find(|(_, (n, pos, _, _))| {
                pos.room_id == room_id && n.0.to_lowercase().contains(&target_lower)
            })
            .map(|(e, _)| e)
    };

    let Some(npc) = npc_entity else {
        send_to_client(
            registry,
            client_id,
            format!("You don't see '{}' here.", target_name),
        );
        return;
    };

    let tick = state.current_tick;

    // Check for an existing InCombat on the player before any mutable work.
    let existing = state
        .world
        .get::<&InCombat>(player)
        .ok()
        .map(|c| (c.target, c.attacking));

    if let Some((existing_target, is_attacking)) = existing {
        if existing_target == npc {
            if is_attacking {
                let npc_name = state
                    .world
                    .get::<&Name>(npc)
                    .ok()
                    .map(|n| n.0.clone())
                    .unwrap_or_default();
                send_to_client(
                    registry,
                    client_id,
                    format!("You are already fighting {}.", npc_name),
                );
                return;
            }
            // Being attacked by this NPC but not yet fighting back — activate.
            let player_interval = state
                .world
                .get::<&Stats>(player)
                .ok()
                .map(|s| player_attack_interval(s.dexterity))
                .unwrap_or(10);
            if let Ok(mut c) = state.world.get::<&mut InCombat>(player) {
                c.attacking = true;
                c.last_attack_tick = tick.saturating_sub(player_interval);
            }
            send_combat_status(&state.world, registry, player, npc);
            return;
        }
        send_to_client(
            registry,
            client_id,
            "You're already in combat! Flee first.".to_string(),
        );
        return;
    }

    let player_interval = state
        .world
        .get::<&Stats>(player)
        .ok()
        .map(|s| player_attack_interval(s.dexterity))
        .unwrap_or(10);
    let npc_interval = state
        .world
        .get::<&NpcCombatStats>(npc)
        .ok()
        .map(|ns| ns.attack_ticks)
        .unwrap_or(10);

    // Start combat on the player (first attack fires immediately).
    state
        .world
        .insert_one(
            player,
            InCombat {
                target: npc,
                last_attack_tick: tick.saturating_sub(player_interval),
                attack_interval: player_interval,
                attacking: true,
            },
        )
        .ok();

    // NPC retaliates unless it is passive.
    if state.world.get::<&InCombat>(npc).is_err() && state.world.get::<&Passive>(npc).is_err() {
        state
            .world
            .insert_one(
                npc,
                InCombat {
                    target: player,
                    last_attack_tick: tick.saturating_sub(npc_interval),
                    attack_interval: npc_interval,
                    attacking: true,
                },
            )
            .ok();
    }

    send_combat_status(&state.world, registry, player, npc);
}

pub(super) fn handle_flee(state: &mut GameState, client_id: ClientId, registry: &OutputRegistry) {
    let Some(player) = state.player_registry.get_entity(client_id) else {
        return;
    };

    if state.world.get::<&InCombat>(player).is_err() {
        send_to_client(registry, client_id, "You're not in combat.".to_string());
        return;
    }

    let room_id = state.world.get::<&Position>(player).ok().map(|p| p.room_id);
    let Some(room_id) = room_id else { return };

    // Pick first available exit deterministically.
    let exit_dir = state
        .room_registry
        .get(room_id)
        .and_then(|r| r.exits.keys().next().copied());

    let Some(dir) = exit_dir else {
        send_to_client(registry, client_id, "There's nowhere to run!".to_string());
        return;
    };

    // Break combat first.
    clear_combat(&mut state.world, player);

    // Then move the player.
    send_to_client(registry, client_id, format!("You flee {} in a panic!", dir));
    super::movement::handle_move(state, client_id, dir, registry);
}
