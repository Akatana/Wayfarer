use crate::command::ClientId;
use crate::components::{Name, NpcGreeting, NpcId, Position};
use crate::game_state::GameState;
use crate::systems::output::{send_to_client, OutputRegistry};

pub(super) fn handle_talk(
    state: &mut GameState,
    client_id: ClientId,
    target: &str,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    let room_id = match state.world.get::<&Position>(entity).ok() {
        Some(p) => p.room_id,
        None => return,
    };

    let target_lower = target.trim().to_lowercase();
    let found = {
        let mut q = state.world.query::<(&Name, &Position, &NpcId)>();
        q.iter()
            .filter(|(_, (_, pos, _))| pos.room_id == room_id)
            .find(|(_, (name, _, _))| name.0.to_lowercase().contains(&target_lower))
            .map(|(e, (name, _, npc_id))| (e, name.0.clone(), npc_id.0))
    };

    let Some((npc_e, npc_name, npc_db_id)) = found else {
        send_to_client(
            registry,
            client_id,
            format!("You don't see '{}' here.", target),
        );
        return;
    };

    // Quest: turn in any ready quests for this NPC first.
    super::quest_turn_in(state, entity, npc_db_id, client_id, registry);

    // Quest: auto-accept new quests offered by this NPC.
    super::quest_accept_from_npc(state, entity, npc_db_id, client_id, registry);

    // Quest: mark any Talk objectives targeting this NPC as complete.
    super::quest_mark_objective(
        state,
        entity,
        client_id,
        registry,
        |obj| matches!(obj, crate::quest::QuestObjectiveDef::Talk { npc_id, .. } if *npc_id == npc_db_id),
    );

    // Show the NPC's greeting.
    let greeting = state
        .world
        .get::<&NpcGreeting>(npc_e)
        .ok()
        .map(|g| g.0.clone());

    match greeting {
        Some(msg) => send_to_client(
            registry,
            client_id,
            format!("{} says: \"{}\"", npc_name, msg),
        ),
        None => send_to_client(registry, client_id, format!("{} ignores you.", npc_name)),
    }
}
