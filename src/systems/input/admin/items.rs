use crate::command::ClientId;
use crate::components::{ItemDescription, ItemId, ItemName, ItemSlot, RoomContents};
use crate::game_state::{AdminDbOp, GameState};
use crate::item::{EquipRequirements, EquipSlot, ItemData, ItemLocation};
use crate::systems::output::{send_to_client, OutputRegistry};
use crate::systems::queries::find_item_in_room;

pub(crate) fn find_item_entity(world: &hecs::World, item_id: i64) -> Option<hecs::Entity> {
    let mut q = world.query::<(&ItemId,)>();
    q.iter().find(|(_, (id,))| id.0 == item_id).map(|(e, _)| e)
}

pub(crate) fn handle_admin_mitem(
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

    let Some(room_id) = state.get_player_room(entity) else {
        return;
    };

    let (name, description) = match spec.split_once('/') {
        Some((n, d)) => (n.trim().to_string(), d.trim().to_string()),
        None => (spec.trim().to_string(), "No description.".to_string()),
    };

    if name.is_empty() {
        send_to_client(
            registry,
            client_id,
            "Usage: @mitem <name> [/ <description>]".to_string(),
        );
        return;
    }

    let item_id = state.next_item_id;
    state.next_item_id += 1;

    let item_data = ItemData {
        id: item_id,
        name: name.clone(),
        description: description.clone(),
        equip_slot: None,
        two_handed: false,
        bag_capacity: None,
        requirements: EquipRequirements::default(),
        location: ItemLocation::Room(room_id),
    };

    state.world.spawn((
        ItemId(item_id),
        ItemName(name.clone()),
        ItemDescription(description),
        RoomContents { room_id },
    ));

    state
        .pending_admin_ops
        .push(AdminDbOp::CreateItem(item_data));

    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] '{name}' (#{item_id}) created in this room.</dim>"),
    );
}

pub(crate) fn handle_admin_destroy(
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

    let Some(room_id) = state.get_player_room(entity) else {
        return;
    };

    let target_lower = target.trim().to_lowercase();
    let found = find_item_in_room(&state.world, room_id, &target_lower);

    let Some((item_ent, item_name)) = found else {
        send_to_client(
            registry,
            client_id,
            format!("No item matching '{}' on the floor here.", target),
        );
        return;
    };

    let item_id = state.world.get::<&ItemId>(item_ent).ok().map(|id| id.0);

    state.world.despawn(item_ent).ok();

    if let Some(id) = item_id {
        state.pending_admin_ops.push(AdminDbOp::DeleteItem(id));
    }

    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] '{item_name}' permanently destroyed.</dim>"),
    );
}

pub(crate) fn handle_admin_iname(
    state: &mut GameState,
    client_id: ClientId,
    item_id: i64,
    new_name: String,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    let Some(item_ent) = find_item_entity(&state.world, item_id) else {
        send_to_client(
            registry,
            client_id,
            format!("No item with id #{item_id} is in the game right now."),
        );
        return;
    };

    if let Ok(mut n) = state.world.get::<&mut ItemName>(item_ent) {
        n.0 = new_name.clone();
    } else {
        return;
    }

    state.pending_admin_ops.push(AdminDbOp::UpdateItemName {
        id: item_id,
        name: new_name.clone(),
    });
    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] Item #{item_id} renamed to '{new_name}'.</dim>"),
    );
}

pub(crate) fn handle_admin_idesc(
    state: &mut GameState,
    client_id: ClientId,
    item_id: i64,
    new_desc: String,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    let Some(item_ent) = find_item_entity(&state.world, item_id) else {
        send_to_client(
            registry,
            client_id,
            format!("No item with id #{item_id} is in the game right now."),
        );
        return;
    };

    if let Ok(mut d) = state.world.get::<&mut ItemDescription>(item_ent) {
        d.0 = new_desc.clone();
    } else {
        return;
    }

    state.pending_admin_ops.push(AdminDbOp::UpdateItemDesc {
        id: item_id,
        description: new_desc,
    });
    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] Item #{item_id} description updated.</dim>"),
    );
}

pub(crate) fn handle_admin_islot(
    state: &mut GameState,
    client_id: ClientId,
    item_id: i64,
    slot_str: String,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    let Some(item_ent) = find_item_entity(&state.world, item_id) else {
        send_to_client(
            registry,
            client_id,
            format!("No item with id #{item_id} is in the game right now."),
        );
        return;
    };

    let lower = slot_str.to_lowercase();
    if lower == "none" {
        state.world.remove::<(ItemSlot,)>(item_ent).ok();
        state.pending_admin_ops.push(AdminDbOp::UpdateItemSlot {
            id: item_id,
            equip_slot: None,
        });
        send_to_client(
            registry,
            client_id,
            format!("<dim>[Admin] Item #{item_id} equip slot cleared.</dim>"),
        );
    } else if let Some(slot) = EquipSlot::parse(&lower) {
        state.world.insert(item_ent, (ItemSlot(slot),)).ok();
        state.pending_admin_ops.push(AdminDbOp::UpdateItemSlot {
            id: item_id,
            equip_slot: Some(slot.to_string()),
        });
        send_to_client(
            registry,
            client_id,
            format!(
                "<dim>[Admin] Item #{item_id} slot set to {}.</dim>",
                slot.label()
            ),
        );
    } else {
        send_to_client(
            registry,
            client_id,
            format!("Unknown slot '{}'. Use a slot name or 'none'.", slot_str),
        );
    }
}

pub(crate) fn handle_admin_ireq(
    state: &mut GameState,
    client_id: ClientId,
    item_id: i64,
    stat: String,
    value: i32,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    let Some(item_ent) = find_item_entity(&state.world, item_id) else {
        send_to_client(
            registry,
            client_id,
            format!("No item with id #{item_id} is in the game right now."),
        );
        return;
    };

    let mut reqs = state
        .world
        .get::<&EquipRequirements>(item_ent)
        .map(|r| *r)
        .unwrap_or_default();

    match stat.to_lowercase().as_str() {
        "str" | "strength" => reqs.strength = value,
        "dex" | "dexterity" => reqs.dexterity = value,
        "knw" | "knowledge" => reqs.knowledge = value,
        "level" | "lv" => reqs.level = value,
        _ => {
            send_to_client(
                registry,
                client_id,
                "Unknown stat. Use: str, dex, knw, level".to_string(),
            );
            return;
        }
    }

    state.world.insert(item_ent, (reqs,)).ok();
    state.pending_admin_ops.push(AdminDbOp::UpdateItemReq {
        id: item_id,
        level: reqs.level,
        strength: reqs.strength,
        dexterity: reqs.dexterity,
        knowledge: reqs.knowledge,
    });

    send_to_client(
        registry,
        client_id,
        format!(
            "<dim>[Admin] Item #{item_id} requirements: Lv {} STR {} DEX {} KNW {}.</dim>",
            reqs.level, reqs.strength, reqs.dexterity, reqs.knowledge
        ),
    );
}
