mod items;
mod npcs;
mod quests;
mod rooms;

pub(super) use items::{
    handle_admin_destroy, handle_admin_idesc, handle_admin_iname, handle_admin_ireq,
    handle_admin_islot, handle_admin_mitem,
};
pub(super) use npcs::{
    handle_admin_mnpc, handle_admin_ndesc, handle_admin_ndestroy, handle_admin_ngreet,
    handle_admin_nhostile, handle_admin_ninfo, handle_admin_nlist, handle_admin_nname,
    handle_admin_npatrol,
};
pub(super) use quests::{
    handle_admin_qgive, handle_admin_qinfo, handle_admin_qlist, handle_admin_qreset,
};
pub(super) use rooms::{
    handle_admin_dig, handle_admin_goto, handle_admin_link, handle_admin_redesc,
    handle_admin_rename, handle_admin_unlink,
};

use crate::command::ClientId;
use crate::components::{
    AdminFlag, ClientConnection, ItemId, ItemName, Name, Position, RoomContents,
};
use crate::game_state::GameState;
use crate::systems::output::{send_to_client, OutputRegistry};

pub(super) fn require_admin(
    state: &GameState,
    client_id: ClientId,
    entity: hecs::Entity,
    registry: &OutputRegistry,
) -> bool {
    if state.world.get::<&AdminFlag>(entity).is_err() {
        send_to_client(
            registry,
            client_id,
            "You don't have that power.".to_string(),
        );
        false
    } else {
        true
    }
}

pub(super) fn get_player_room(state: &GameState, entity: hecs::Entity) -> Option<u64> {
    state.world.get::<&Position>(entity).ok().map(|p| p.room_id)
}

pub(super) fn handle_admin_who(state: &GameState, client_id: ClientId, registry: &OutputRegistry) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if state.world.get::<&AdminFlag>(entity).is_err() {
        send_to_client(
            registry,
            client_id,
            "You don't have that power.".to_string(),
        );
        return;
    }
    let mut lines = vec!["<yellow>=== Online Players ===</yellow>".to_string()];
    let mut q = state.world.query::<(&Name, &ClientConnection)>();
    for (_, (name, _)) in q.iter() {
        lines.push(format!("  {}", name.0));
    }
    if lines.len() == 1 {
        lines.push("  (nobody online)".to_string());
    }
    send_to_client(registry, client_id, lines.join("\n"));
}

pub(super) fn handle_admin_roominfo(
    state: &GameState,
    client_id: ClientId,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if state.world.get::<&AdminFlag>(entity).is_err() {
        send_to_client(
            registry,
            client_id,
            "You don't have that power.".to_string(),
        );
        return;
    }

    let Some(room_id) = get_player_room(state, entity) else {
        return;
    };
    let Some(room) = state.room_registry.get(room_id) else {
        return;
    };

    let mut lines = vec![
        "<yellow>=== Room Info ===</yellow>".to_string(),
        format!("  ID   : {room_id}"),
        format!("  Name : {}", room.name),
        format!("  Desc : {}", room.description),
        "  Exits:".to_string(),
    ];
    let mut dirs: Vec<_> = room.exits.iter().collect();
    dirs.sort_by_key(|(d, _)| d.to_string());
    for (dir, exit) in dirs {
        lines.push(format!(
            "    {:<12} → room {}",
            dir, exit.destination_room_id
        ));
    }
    if room.exits.is_empty() {
        lines.push("    (none)".to_string());
    }

    let mut floor_items: Vec<(i64, String)> = {
        let mut q = state.world.query::<(&ItemId, &ItemName, &RoomContents)>();
        q.iter()
            .filter(|(_, (_, _, rc))| rc.room_id == room_id)
            .map(|(_, (id, n, _))| (id.0, n.0.clone()))
            .collect()
    };
    floor_items.sort_by_key(|(id, _)| *id);

    if !floor_items.is_empty() {
        lines.push("  Items:".to_string());
        for (id, name) in floor_items {
            lines.push(format!("    #{id:<6} {name}"));
        }
    }

    send_to_client(registry, client_id, lines.join("\n"));
}
