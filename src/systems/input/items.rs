use crate::command::ClientId;
use crate::components::{
    CharacterId, Equipped, InInventory, ItemName, ItemSlot, Name, RoomContents, Stats, TwoHanded,
};
use crate::game_state::GameState;
use crate::item::{EquipRequirements, EquipSlot, ItemLocation, ItemLocationSave};
use crate::systems::output::{send_to_client, OutputRegistry};
use crate::systems::queries::{clients_in_room_except, find_item_in_inventory, find_item_in_room};

pub(super) fn handle_get(
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
        send_to_client(registry, client_id, "Get what?".to_string());
        return;
    }

    let Some(room_id) = state.get_player_room(entity) else {
        return;
    };

    let limit = super::effective_inventory_limit(&state.world, entity);
    if super::inventory_count(&state.world, entity) >= limit {
        send_to_client(
            registry,
            client_id,
            format!("Your bag is full ({limit}/{limit})."),
        );
        return;
    }

    let target_lower = target.to_lowercase();
    let found = find_item_in_room(&state.world, room_id, &target_lower);

    let Some((item, item_name)) = found else {
        send_to_client(
            registry,
            client_id,
            format!("You don't see '{}' here.", target),
        );
        return;
    };

    state.world.remove::<(RoomContents,)>(item).ok();
    state
        .world
        .insert(item, (InInventory { owner: entity },))
        .ok();

    if let (Ok(item_id), Ok(char_id)) = (
        state
            .world
            .get::<&crate::components::ItemId>(item)
            .map(|id| id.0),
        state.world.get::<&CharacterId>(entity).map(|c| c.db_id),
    ) {
        state.pending_item_saves.push(ItemLocationSave {
            item_id,
            location: ItemLocation::Inventory { char_id },
        });
    }

    send_to_client(registry, client_id, format!("You pick up {}.", item_name));

    let player_name = state
        .world
        .get::<&Name>(entity)
        .map(|n| n.0.clone())
        .unwrap_or_default();
    for id in clients_in_room_except(&state.world, room_id, entity) {
        send_to_client(
            registry,
            id,
            format!("{} picks up {}.", player_name, item_name),
        );
    }
}

pub(super) fn handle_drop(
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
        send_to_client(registry, client_id, "Drop what?".to_string());
        return;
    }

    let Some(room_id) = state.get_player_room(entity) else {
        return;
    };

    let target_lower = target.to_lowercase();
    let found = find_item_in_inventory(&state.world, entity, &target_lower);

    let Some((item, item_name)) = found else {
        send_to_client(
            registry,
            client_id,
            format!("You don't have '{}' in your bag.", target),
        );
        return;
    };

    state.world.remove::<(InInventory,)>(item).ok();
    state.world.insert(item, (RoomContents { room_id },)).ok();

    if let Ok(item_id) = state
        .world
        .get::<&crate::components::ItemId>(item)
        .map(|id| id.0)
    {
        state.pending_item_saves.push(ItemLocationSave {
            item_id,
            location: ItemLocation::Room(room_id),
        });
    }

    send_to_client(registry, client_id, format!("You drop {}.", item_name));

    let player_name = state
        .world
        .get::<&Name>(entity)
        .map(|n| n.0.clone())
        .unwrap_or_default();
    for id in clients_in_room_except(&state.world, room_id, entity) {
        send_to_client(
            registry,
            id,
            format!("{} drops {}.", player_name, item_name),
        );
    }
}

pub(super) fn handle_inventory(state: &GameState, client_id: ClientId, registry: &OutputRegistry) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };

    let equipped_map: std::collections::HashMap<EquipSlot, String> = {
        let mut q = state.world.query::<(&ItemName, &Equipped)>();
        q.iter()
            .filter(|(_, (_, eq))| eq.owner == entity)
            .map(|(_, (n, eq))| (eq.slot, n.0.clone()))
            .collect()
    };

    let mut lines = vec!["<yellow>=== Equipment ===</yellow>".to_string()];
    for slot in EquipSlot::all() {
        let item_str = equipped_map
            .get(&slot)
            .map(String::as_str)
            .unwrap_or("(empty)");
        lines.push(format!(
            "  <cyan>{:<12}</cyan> : {}",
            slot.label(),
            item_str
        ));
    }

    let bag: Vec<String> = {
        let mut q = state.world.query::<(&ItemName, &InInventory)>();
        q.iter()
            .filter(|(_, (_, inv))| inv.owner == entity)
            .map(|(_, (n, _))| n.0.clone())
            .collect()
    };

    let limit = super::effective_inventory_limit(&state.world, entity);
    lines.push(format!(
        "\n<yellow>=== Bag ({}/{limit}) ===</yellow>",
        bag.len()
    ));
    if bag.is_empty() {
        lines.push("  (empty)".to_string());
    } else {
        for (i, name) in bag.iter().enumerate() {
            lines.push(format!("  {}. {}", i + 1, name));
        }
    }

    send_to_client(registry, client_id, lines.join("\n"));
}

pub(super) fn handle_equip(
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
        send_to_client(registry, client_id, "Equip what?".to_string());
        return;
    }

    let target_lower = target.to_lowercase();
    let found = find_item_in_inventory(&state.world, entity, &target_lower);

    let Some((item, item_name)) = found else {
        send_to_client(
            registry,
            client_id,
            format!("You don't have '{}' in your bag.", target),
        );
        return;
    };

    let item_slot = match state.world.get::<&ItemSlot>(item) {
        Ok(s) => s.0,
        Err(_) => {
            send_to_client(
                registry,
                client_id,
                format!("{} cannot be equipped.", item_name),
            );
            return;
        }
    };
    let is_two_handed = state.world.get::<&TwoHanded>(item).is_ok();

    if let Ok(reqs) = state.world.get::<&EquipRequirements>(item) {
        let stats = match state.world.get::<&Stats>(entity) {
            Ok(s) => s,
            Err(_) => return,
        };
        if !reqs.is_met_by(&stats) {
            send_to_client(
                registry,
                client_id,
                format!(
                    "You don't meet the requirements to equip {item_name}. Needs: {}",
                    reqs.display()
                ),
            );
            return;
        }
    }

    // Rings and bags auto-pick the first free slot of their type.
    let target_slot = if item_slot == EquipSlot::Ring1 {
        if super::find_equipped_in_slot(&state.world, entity, EquipSlot::Ring1).is_none() {
            EquipSlot::Ring1
        } else if super::find_equipped_in_slot(&state.world, entity, EquipSlot::Ring2).is_none() {
            EquipSlot::Ring2
        } else {
            send_to_client(
                registry,
                client_id,
                "Both ring slots are full. Unequip a ring first.".to_string(),
            );
            return;
        }
    } else if item_slot == EquipSlot::Bag1 {
        match [
            EquipSlot::Bag1,
            EquipSlot::Bag2,
            EquipSlot::Bag3,
            EquipSlot::Bag4,
        ]
        .into_iter()
        .find(|&s| super::find_equipped_in_slot(&state.world, entity, s).is_none())
        {
            Some(s) => s,
            None => {
                send_to_client(
                    registry,
                    client_id,
                    "All bag slots are full. Unequip a bag first.".to_string(),
                );
                return;
            }
        }
    } else {
        item_slot
    };

    if super::find_equipped_in_slot(&state.world, entity, target_slot).is_some() {
        send_to_client(
            registry,
            client_id,
            format!(
                "{} is already occupied. Unequip it first.",
                target_slot.label()
            ),
        );
        return;
    }

    if is_two_handed
        && super::find_equipped_in_slot(&state.world, entity, EquipSlot::RightHand).is_some()
    {
        send_to_client(
            registry,
            client_id,
            "Two-handed weapons need both hands free. Unequip your off-hand first.".to_string(),
        );
        return;
    }

    if target_slot == EquipSlot::RightHand && super::has_two_handed(&state.world, entity) {
        send_to_client(
            registry,
            client_id,
            "You can't use an off-hand while wielding a two-handed weapon.".to_string(),
        );
        return;
    }

    state.world.remove::<(InInventory,)>(item).ok();
    state
        .world
        .insert(
            item,
            (Equipped {
                owner: entity,
                slot: target_slot,
            },),
        )
        .ok();

    if let (Ok(item_id), Ok(char_id)) = (
        state
            .world
            .get::<&crate::components::ItemId>(item)
            .map(|id| id.0),
        state.world.get::<&CharacterId>(entity).map(|c| c.db_id),
    ) {
        state.pending_item_saves.push(ItemLocationSave {
            item_id,
            location: ItemLocation::Equipped {
                char_id,
                slot: target_slot,
            },
        });
    }

    send_to_client(
        registry,
        client_id,
        format!(
            "You equip {} <cyan>[{}]</cyan>.",
            item_name,
            target_slot.label()
        ),
    );
}

pub(super) fn handle_unequip(
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
        send_to_client(
            registry,
            client_id,
            "Unequip what? (slot name or item name)".to_string(),
        );
        return;
    }

    if super::inventory_count(&state.world, entity)
        >= super::effective_inventory_limit(&state.world, entity)
    {
        send_to_client(
            registry,
            client_id,
            "Your bag is full. Drop something first.".to_string(),
        );
        return;
    }

    // Try slot name first, then item name.
    let found: Option<(hecs::Entity, EquipSlot)> = if let Some(slot) = EquipSlot::parse(target) {
        // For "ring", try Ring1 then Ring2.
        if slot == EquipSlot::Ring1 {
            super::find_equipped_in_slot(&state.world, entity, EquipSlot::Ring1)
                .map(|e| (e, EquipSlot::Ring1))
                .or_else(|| {
                    super::find_equipped_in_slot(&state.world, entity, EquipSlot::Ring2)
                        .map(|e| (e, EquipSlot::Ring2))
                })
        } else {
            super::find_equipped_in_slot(&state.world, entity, slot).map(|e| (e, slot))
        }
    } else {
        let target_lower = target.to_lowercase();
        let mut q = state.world.query::<(&ItemName, &Equipped)>();
        q.iter()
            .find(|(_, (n, eq))| eq.owner == entity && n.0.to_lowercase().contains(&target_lower))
            .map(|(e, (_, eq))| (e, eq.slot))
    };

    let Some((item, slot)) = found else {
        send_to_client(
            registry,
            client_id,
            format!("Nothing equipped matching '{}'.", target),
        );
        return;
    };

    let item_name = state
        .world
        .get::<&ItemName>(item)
        .map(|n| n.0.clone())
        .unwrap_or_default();

    state.world.remove::<(Equipped,)>(item).ok();
    state
        .world
        .insert(item, (InInventory { owner: entity },))
        .ok();

    if let (Ok(item_id), Ok(char_id)) = (
        state
            .world
            .get::<&crate::components::ItemId>(item)
            .map(|id| id.0),
        state.world.get::<&CharacterId>(entity).map(|c| c.db_id),
    ) {
        state.pending_item_saves.push(ItemLocationSave {
            item_id,
            location: ItemLocation::Inventory { char_id },
        });
    }

    send_to_client(
        registry,
        client_id,
        format!("You unequip {} from {}.", item_name, slot.label()),
    );
}
