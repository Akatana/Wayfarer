use crate::command::ClientId;
use crate::direction::Direction;
use crate::game_state::{AdminDbOp, GameState};
use crate::systems::output::{send_to_client, OutputRegistry};
use crate::world::room::{Exit, Room};
use std::collections::HashMap;

pub(crate) fn handle_admin_goto(
    state: &mut GameState,
    client_id: ClientId,
    room_id: u64,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    if state.room_registry.get(room_id).is_none() {
        send_to_client(
            registry,
            client_id,
            format!("Room {room_id} does not exist."),
        );
        return;
    }

    if let Ok(mut pos) = state.world.get::<&mut crate::components::Position>(entity) {
        pos.room_id = room_id;
    }

    if let Some(desc) = super::super::describe_room(state, room_id, entity) {
        send_to_client(
            registry,
            client_id,
            format!("<dim>[Admin] Teleported to room {room_id}.</dim>\n\n{desc}"),
        );
    }
}

pub(crate) fn handle_admin_dig(
    state: &mut GameState,
    client_id: ClientId,
    dir: Direction,
    room_name: String,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    let Some(current_room_id) = state.get_player_room(entity) else {
        return;
    };

    if state
        .room_registry
        .resolve_exit(current_room_id, dir)
        .is_some()
    {
        send_to_client(
            registry,
            client_id,
            format!("There is already an exit to the {} from here.", dir),
        );
        return;
    }

    let new_id = state.next_room_id;
    state.next_room_id += 1;

    let return_dir = dir.opposite();
    let new_room = Room {
        id: new_id,
        name: room_name.clone(),
        description: "No description yet.".to_string(),
        exits: HashMap::from([(
            return_dir,
            Exit {
                destination_room_id: current_room_id,
            },
        )]),
    };

    if let Some(current) = state.room_registry.get_mut(current_room_id) {
        current.exits.insert(
            dir,
            Exit {
                destination_room_id: new_id,
            },
        );
    }
    state.room_registry.insert(new_room.clone());

    state.pending_admin_ops.push(AdminDbOp::UpsertExit {
        room_id: current_room_id,
        dir,
        dest_id: new_id,
    });
    state
        .pending_admin_ops
        .push(AdminDbOp::CreateRoom(new_room));

    send_to_client(
        registry,
        client_id,
        format!(
            "<dim>[Admin] Room #{new_id} '{room_name}' created to the {dir}. Return exit: {return_dir}.</dim>"
        ),
    );
}

pub(crate) fn handle_admin_link(
    state: &mut GameState,
    client_id: ClientId,
    dir: Direction,
    dest_id: u64,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    let Some(current_room_id) = state.get_player_room(entity) else {
        return;
    };

    if state.room_registry.get(dest_id).is_none() {
        send_to_client(
            registry,
            client_id,
            format!("Room {dest_id} does not exist."),
        );
        return;
    }
    if state
        .room_registry
        .resolve_exit(current_room_id, dir)
        .is_some()
    {
        send_to_client(
            registry,
            client_id,
            format!("There is already an exit to the {} from here.", dir),
        );
        return;
    }

    if let Some(current) = state.room_registry.get_mut(current_room_id) {
        current.exits.insert(
            dir,
            Exit {
                destination_room_id: dest_id,
            },
        );
    }
    state.pending_admin_ops.push(AdminDbOp::UpsertExit {
        room_id: current_room_id,
        dir,
        dest_id,
    });

    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] Exit {} â†’ room {dest_id} added.</dim>", dir),
    );
}

pub(crate) fn handle_admin_unlink(
    state: &mut GameState,
    client_id: ClientId,
    dir: Direction,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    let Some(current_room_id) = state.get_player_room(entity) else {
        return;
    };

    if state
        .room_registry
        .resolve_exit(current_room_id, dir)
        .is_none()
    {
        send_to_client(
            registry,
            client_id,
            format!("No exit to the {} from here.", dir),
        );
        return;
    }

    if let Some(current) = state.room_registry.get_mut(current_room_id) {
        current.exits.remove(&dir);
    }
    state.pending_admin_ops.push(AdminDbOp::DeleteExit {
        room_id: current_room_id,
        dir,
    });

    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] Exit {} removed.</dim>", dir),
    );
}

pub(crate) fn handle_admin_rename(
    state: &mut GameState,
    client_id: ClientId,
    new_name: String,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    let Some(room_id) = state.get_player_room(entity) else {
        return;
    };

    let description = if let Some(room) = state.room_registry.get_mut(room_id) {
        let old_name = std::mem::replace(&mut room.name, new_name.clone());
        let _ = old_name;
        room.description.clone()
    } else {
        return;
    };

    state.pending_admin_ops.push(AdminDbOp::UpdateRoom {
        id: room_id,
        name: new_name.clone(),
        description,
    });

    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] Room renamed to '{new_name}'.</dim>"),
    );
}

pub(crate) fn handle_admin_redesc(
    state: &mut GameState,
    client_id: ClientId,
    new_desc: String,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    let Some(room_id) = state.get_player_room(entity) else {
        return;
    };

    let name = if let Some(room) = state.room_registry.get_mut(room_id) {
        room.description = new_desc.clone();
        room.name.clone()
    } else {
        return;
    };

    state.pending_admin_ops.push(AdminDbOp::UpdateRoom {
        id: room_id,
        name,
        description: new_desc,
    });

    send_to_client(
        registry,
        client_id,
        "<dim>[Admin] Room description updated.</dim>".to_string(),
    );
}
