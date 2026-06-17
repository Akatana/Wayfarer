use tokio::sync::mpsc;

use crate::character::CharacterData;
use crate::command::{ClientId, Command, PlayerInput};
use crate::components::{
    AdminFlag, BagCapacity, ClientConnection, Equipped, InInventory, ItemDescription, ItemName,
    ItemSlot, Name, Position, RoomContents, Stats, TwoHanded,
};
use crate::direction::Direction;
use crate::game_state::GameState;
use crate::item::EquipSlot;
use crate::systems::{
    movement,
    output::{send_to_client, OutputRegistry},
};

const BASE_INVENTORY_LIMIT: usize = 20;

/// Phase 1 of each tick: drains every pending command from the network channel
/// without blocking. No `.await` calls are permitted here.
pub fn process_input(
    state: &mut GameState,
    command_rx: &mut mpsc::Receiver<PlayerInput>,
    output_registry: &OutputRegistry,
) {
    while let Ok(input) = command_rx.try_recv() {
        dispatch(state, input, output_registry);
    }
}

fn dispatch(state: &mut GameState, input: PlayerInput, registry: &OutputRegistry) {
    match input.command {
        Command::Connect(data) => handle_connect(state, input.client_id, data, registry),
        Command::Look => handle_look(state, input.client_id, registry),
        Command::Move(dir) => handle_move(state, input.client_id, dir, registry),
        Command::Say(msg) => handle_say(state, input.client_id, &msg, registry),
        Command::Get(target) => handle_get(state, input.client_id, &target, registry),
        Command::Drop(target) => handle_drop(state, input.client_id, &target, registry),
        Command::Inventory => handle_inventory(state, input.client_id, registry),
        Command::Equip(target) => handle_equip(state, input.client_id, &target, registry),
        Command::Unequip(target) => handle_unequip(state, input.client_id, &target, registry),
        Command::Examine(target) => handle_examine(state, input.client_id, &target, registry),
        Command::Score => handle_score(state, input.client_id, registry),
        Command::Quit => handle_quit(state, input.client_id, registry),
        Command::AdminWho => handle_admin_who(state, input.client_id, registry),
        Command::Unknown(raw) => handle_unknown(input.client_id, &raw, registry),
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Builds the full room description including floor items.
fn describe_room(state: &GameState, room_id: u64) -> Option<String> {
    let room = state.room_registry.get(room_id)?;
    let mut desc = room.describe();

    let mut floor_items: Vec<String> = {
        let mut q = state.world.query::<(&ItemName, &RoomContents)>();
        q.iter()
            .filter(|(_, (_, rc))| rc.room_id == room_id)
            .map(|(_, (n, _))| n.0.clone())
            .collect()
    };
    floor_items.sort_unstable();

    if !floor_items.is_empty() {
        desc.push_str(&format!("\n[ Items: {} ]", floor_items.join(", ")));
    }
    Some(desc)
}

fn inventory_count(world: &hecs::World, owner: hecs::Entity) -> usize {
    let mut q = world.query::<(&InInventory,)>();
    q.iter().filter(|(_, (inv,))| inv.owner == owner).count()
}

fn find_equipped_in_slot(
    world: &hecs::World,
    owner: hecs::Entity,
    slot: EquipSlot,
) -> Option<hecs::Entity> {
    let mut q = world.query::<(&Equipped,)>();
    q.iter()
        .find(|(_, (eq,))| eq.owner == owner && eq.slot == slot)
        .map(|(e, _)| e)
}

fn has_two_handed(world: &hecs::World, owner: hecs::Entity) -> bool {
    find_equipped_in_slot(world, owner, EquipSlot::LeftHand)
        .map(|e| world.get::<&TwoHanded>(e).is_ok())
        .unwrap_or(false)
}

/// Base limit plus the sum of BagCapacity for every bag the player has equipped.
fn effective_inventory_limit(world: &hecs::World, owner: hecs::Entity) -> usize {
    let bonus: usize = {
        let mut q = world.query::<(&BagCapacity, &Equipped)>();
        q.iter()
            .filter(|(_, (_, eq))| eq.owner == owner)
            .map(|(_, (cap, _))| cap.0)
            .sum()
    };
    BASE_INVENTORY_LIMIT + bonus
}

// ── Command handlers ──────────────────────────────────────────────────────────

fn handle_connect(
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
        if let Some(desc) = describe_room(state, rid) {
            send_to_client(registry, client_id, format!("Welcome, {}!\n\n{}", data.name, desc));
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
            send_to_client(registry, id, format!("{} has entered the world.", data.name));
        }
    }
}

fn handle_look(state: &GameState, client_id: ClientId, registry: &OutputRegistry) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    let room_id = {
        let Ok(pos) = state.world.get::<&Position>(entity) else { return };
        pos.room_id
    };
    if let Some(desc) = describe_room(state, room_id) {
        send_to_client(registry, client_id, desc);
    }
}

fn handle_move(
    state: &mut GameState,
    client_id: ClientId,
    direction: Direction,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };

    let old_room_id = {
        let Ok(pos) = state.world.get::<&Position>(entity) else { return };
        pos.room_id
    };
    let player_name = state.world.get::<&Name>(entity).map(|n| n.0.clone()).unwrap_or_default();

    let new_pos = movement::try_move(&state.world, &state.room_registry, entity, direction);

    match new_pos {
        Some(pos) => {
            let new_room_id = pos.room_id;

            let old_occupants: Vec<ClientId> = {
                let mut q = state.world.query::<(&Position, &ClientConnection)>();
                q.iter()
                    .filter(|(e, (p, _))| *e != entity && p.room_id == old_room_id)
                    .map(|(_, (_, conn))| conn.client_id)
                    .collect()
            };

            if let Ok(mut current) = state.world.get::<&mut Position>(entity) {
                *current = pos;
            }

            let new_occupants: Vec<ClientId> = {
                let mut q = state.world.query::<(&Position, &ClientConnection)>();
                q.iter()
                    .filter(|(e, (p, _))| *e != entity && p.room_id == new_room_id)
                    .map(|(_, (_, conn))| conn.client_id)
                    .collect()
            };

            for id in old_occupants {
                send_to_client(registry, id, format!("{} leaves {}.", player_name, direction));
            }
            let from = direction.opposite();
            for id in new_occupants {
                send_to_client(registry, id, format!("{} arrives from the {}.", player_name, from));
            }

            if let Some(desc) = describe_room(state, new_room_id) {
                send_to_client(registry, client_id, format!("You head {}.\n\n{}", direction, desc));
            }
        }
        None => {
            send_to_client(registry, client_id, "There's no exit in that direction.".to_string());
        }
    }
}

fn handle_say(state: &GameState, client_id: ClientId, message: &str, registry: &OutputRegistry) {
    let Some(sender_entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    let sender_room_id = {
        let Ok(pos) = state.world.get::<&Position>(sender_entity) else { return };
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

fn handle_get(state: &mut GameState, client_id: ClientId, target: &str, registry: &OutputRegistry) {
    let Some(entity) = state.player_registry.get_entity(client_id) else { return };
    let target = target.trim();
    if target.is_empty() {
        send_to_client(registry, client_id, "Get what?".to_string());
        return;
    }

    let room_id = {
        let Ok(pos) = state.world.get::<&Position>(entity) else { return };
        pos.room_id
    };

    let limit = effective_inventory_limit(&state.world, entity);
    if inventory_count(&state.world, entity) >= limit {
        send_to_client(registry, client_id, format!("Your bag is full ({limit}/{limit})."));
        return;
    }

    let target_lower = target.to_lowercase();
    let found = {
        let mut q = state.world.query::<(&ItemName, &RoomContents)>();
        q.iter()
            .find(|(_, (n, rc))| rc.room_id == room_id && n.0.to_lowercase().contains(&target_lower))
            .map(|(e, (n, _))| (e, n.0.clone()))
    };

    let Some((item, item_name)) = found else {
        send_to_client(registry, client_id, format!("You don't see '{}' here.", target));
        return;
    };

    state.world.remove::<(RoomContents,)>(item).ok();
    state.world.insert(item, (InInventory { owner: entity },)).ok();

    send_to_client(registry, client_id, format!("You pick up {}.", item_name));

    let player_name = state.world.get::<&Name>(entity).map(|n| n.0.clone()).unwrap_or_default();
    let others: Vec<ClientId> = {
        let mut q = state.world.query::<(&Position, &ClientConnection)>();
        q.iter()
            .filter(|(e, (p, _))| *e != entity && p.room_id == room_id)
            .map(|(_, (_, conn))| conn.client_id)
            .collect()
    };
    for id in others {
        send_to_client(registry, id, format!("{} picks up {}.", player_name, item_name));
    }
}

fn handle_drop(
    state: &mut GameState,
    client_id: ClientId,
    target: &str,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else { return };
    let target = target.trim();
    if target.is_empty() {
        send_to_client(registry, client_id, "Drop what?".to_string());
        return;
    }

    let room_id = {
        let Ok(pos) = state.world.get::<&Position>(entity) else { return };
        pos.room_id
    };

    let target_lower = target.to_lowercase();
    let found = {
        let mut q = state.world.query::<(&ItemName, &InInventory)>();
        q.iter()
            .find(|(_, (n, inv))| inv.owner == entity && n.0.to_lowercase().contains(&target_lower))
            .map(|(e, (n, _))| (e, n.0.clone()))
    };

    let Some((item, item_name)) = found else {
        send_to_client(registry, client_id, format!("You don't have '{}' in your bag.", target));
        return;
    };

    state.world.remove::<(InInventory,)>(item).ok();
    state.world.insert(item, (RoomContents { room_id },)).ok();

    send_to_client(registry, client_id, format!("You drop {}.", item_name));

    let player_name = state.world.get::<&Name>(entity).map(|n| n.0.clone()).unwrap_or_default();
    let others: Vec<ClientId> = {
        let mut q = state.world.query::<(&Position, &ClientConnection)>();
        q.iter()
            .filter(|(e, (p, _))| *e != entity && p.room_id == room_id)
            .map(|(_, (_, conn))| conn.client_id)
            .collect()
    };
    for id in others {
        send_to_client(registry, id, format!("{} drops {}.", player_name, item_name));
    }
}

fn handle_inventory(state: &GameState, client_id: ClientId, registry: &OutputRegistry) {
    let Some(entity) = state.player_registry.get_entity(client_id) else { return };

    let equipped_map: std::collections::HashMap<EquipSlot, String> = {
        let mut q = state.world.query::<(&ItemName, &Equipped)>();
        q.iter()
            .filter(|(_, (_, eq))| eq.owner == entity)
            .map(|(_, (n, eq))| (eq.slot, n.0.clone()))
            .collect()
    };

    let mut lines = vec!["<yellow>=== Equipment ===</yellow>".to_string()];
    for slot in EquipSlot::all() {
        let item_str = equipped_map.get(&slot).map(String::as_str).unwrap_or("(empty)");
        lines.push(format!("  <cyan>{:<12}</cyan> : {}", slot.label(), item_str));
    }

    let bag: Vec<String> = {
        let mut q = state.world.query::<(&ItemName, &InInventory)>();
        q.iter()
            .filter(|(_, (_, inv))| inv.owner == entity)
            .map(|(_, (n, _))| n.0.clone())
            .collect()
    };

    let limit = effective_inventory_limit(&state.world, entity);
    lines.push(format!("\n<yellow>=== Bag ({}/{limit}) ===</yellow>", bag.len()));
    if bag.is_empty() {
        lines.push("  (empty)".to_string());
    } else {
        for (i, name) in bag.iter().enumerate() {
            lines.push(format!("  {}. {}", i + 1, name));
        }
    }

    send_to_client(registry, client_id, lines.join("\n"));
}

fn handle_equip(
    state: &mut GameState,
    client_id: ClientId,
    target: &str,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else { return };
    let target = target.trim();
    if target.is_empty() {
        send_to_client(registry, client_id, "Equip what?".to_string());
        return;
    }

    let target_lower = target.to_lowercase();
    let found = {
        let mut q = state.world.query::<(&ItemName, &InInventory)>();
        q.iter()
            .find(|(_, (n, inv))| inv.owner == entity && n.0.to_lowercase().contains(&target_lower))
            .map(|(e, (n, _))| (e, n.0.clone()))
    };

    let Some((item, item_name)) = found else {
        send_to_client(registry, client_id, format!("You don't have '{}' in your bag.", target));
        return;
    };

    let item_slot = match state.world.get::<&ItemSlot>(item) {
        Ok(s) => s.0,
        Err(_) => {
            send_to_client(registry, client_id, format!("{} cannot be equipped.", item_name));
            return;
        }
    };
    let is_two_handed = state.world.get::<&TwoHanded>(item).is_ok();

    // Rings and bags auto-pick the first free slot of their type.
    let target_slot = if item_slot == EquipSlot::Ring1 {
        if find_equipped_in_slot(&state.world, entity, EquipSlot::Ring1).is_none() {
            EquipSlot::Ring1
        } else if find_equipped_in_slot(&state.world, entity, EquipSlot::Ring2).is_none() {
            EquipSlot::Ring2
        } else {
            send_to_client(registry, client_id, "Both ring slots are full. Unequip a ring first.".to_string());
            return;
        }
    } else if item_slot == EquipSlot::Bag1 {
        match [EquipSlot::Bag1, EquipSlot::Bag2, EquipSlot::Bag3, EquipSlot::Bag4]
            .into_iter()
            .find(|&s| find_equipped_in_slot(&state.world, entity, s).is_none())
        {
            Some(s) => s,
            None => {
                send_to_client(registry, client_id, "All bag slots are full. Unequip a bag first.".to_string());
                return;
            }
        }
    } else {
        item_slot
    };

    if find_equipped_in_slot(&state.world, entity, target_slot).is_some() {
        send_to_client(
            registry,
            client_id,
            format!("{} is already occupied. Unequip it first.", target_slot.label()),
        );
        return;
    }

    if is_two_handed && find_equipped_in_slot(&state.world, entity, EquipSlot::RightHand).is_some() {
        send_to_client(registry, client_id, "Two-handed weapons need both hands free. Unequip your off-hand first.".to_string());
        return;
    }

    if target_slot == EquipSlot::RightHand && has_two_handed(&state.world, entity) {
        send_to_client(registry, client_id, "You can't use an off-hand while wielding a two-handed weapon.".to_string());
        return;
    }

    state.world.remove::<(InInventory,)>(item).ok();
    state.world.insert(item, (Equipped { owner: entity, slot: target_slot },)).ok();

    send_to_client(
        registry,
        client_id,
        format!("You equip {} <cyan>[{}]</cyan>.", item_name, target_slot.label()),
    );
}

fn handle_unequip(
    state: &mut GameState,
    client_id: ClientId,
    target: &str,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else { return };
    let target = target.trim();
    if target.is_empty() {
        send_to_client(registry, client_id, "Unequip what? (slot name or item name)".to_string());
        return;
    }

    if inventory_count(&state.world, entity) >= effective_inventory_limit(&state.world, entity) {
        send_to_client(registry, client_id, "Your bag is full. Drop something first.".to_string());
        return;
    }

    // Try slot name first, then item name.
    let found: Option<(hecs::Entity, EquipSlot)> = if let Some(slot) = EquipSlot::from_str(target) {
        // For "ring", try Ring1 then Ring2.
        if slot == EquipSlot::Ring1 {
            find_equipped_in_slot(&state.world, entity, EquipSlot::Ring1)
                .map(|e| (e, EquipSlot::Ring1))
                .or_else(|| {
                    find_equipped_in_slot(&state.world, entity, EquipSlot::Ring2)
                        .map(|e| (e, EquipSlot::Ring2))
                })
        } else {
            find_equipped_in_slot(&state.world, entity, slot).map(|e| (e, slot))
        }
    } else {
        let target_lower = target.to_lowercase();
        let mut q = state.world.query::<(&ItemName, &Equipped)>();
        q.iter()
            .find(|(_, (n, eq))| eq.owner == entity && n.0.to_lowercase().contains(&target_lower))
            .map(|(e, (_, eq))| (e, eq.slot))
    };

    let Some((item, slot)) = found else {
        send_to_client(registry, client_id, format!("Nothing equipped matching '{}'.", target));
        return;
    };

    let item_name = state.world.get::<&ItemName>(item).map(|n| n.0.clone()).unwrap_or_default();

    state.world.remove::<(Equipped,)>(item).ok();
    state.world.insert(item, (InInventory { owner: entity },)).ok();

    send_to_client(
        registry,
        client_id,
        format!("You unequip {} from {}.", item_name, slot.label()),
    );
}

fn handle_examine(state: &GameState, client_id: ClientId, target: &str, registry: &OutputRegistry) {
    let Some(entity) = state.player_registry.get_entity(client_id) else { return };
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
                .find(|(_, (n, rc))| rc.room_id == rid && n.0.to_lowercase().contains(&target_lower))
                .map(|(e, _)| e)
        });
        if floor.is_some() {
            floor
        } else {
            let inv = {
                let mut q = state.world.query::<(&ItemName, &InInventory)>();
                q.iter()
                    .find(|(_, (n, inv))| inv.owner == entity && n.0.to_lowercase().contains(&target_lower))
                    .map(|(e, _)| e)
            };
            if inv.is_some() {
                inv
            } else {
                let mut q = state.world.query::<(&ItemName, &Equipped)>();
                q.iter()
                    .find(|(_, (n, eq))| eq.owner == entity && n.0.to_lowercase().contains(&target_lower))
                    .map(|(e, _)| e)
            }
        }
    };

    let Some(item) = item else {
        send_to_client(registry, client_id, format!("You don't see '{}' anywhere.", target));
        return;
    };

    let name = state.world.get::<&ItemName>(item).map(|n| n.0.clone()).unwrap_or_default();
    let desc = state.world.get::<&ItemDescription>(item)
        .map(|d| d.0.clone())
        .unwrap_or_else(|_| "Nothing remarkable.".to_string());

    send_to_client(registry, client_id, format!("<yellow>{}</yellow>\n   {}", name, desc));
}

fn handle_score(state: &GameState, client_id: ClientId, registry: &OutputRegistry) {
    let Some(entity) = state.player_registry.get_entity(client_id) else { return };

    let name = state.world.get::<&Name>(entity).map(|n| n.0.clone()).unwrap_or_default();
    let is_admin = state.world.get::<&AdminFlag>(entity).is_ok();
    let room_name = {
        let room_id = state.world.get::<&Position>(entity).ok().map(|p| p.room_id);
        room_id
            .and_then(|id| state.room_registry.get(id))
            .map(|r| r.name.clone())
            .unwrap_or_else(|| "Unknown".to_string())
    };

    let admin_tag = if is_admin { " <yellow>[Admin]</yellow>" } else { "" };
    let mut lines = vec![format!("<yellow>=== {} ===</yellow>{}", name, admin_tag)];

    if let Ok(s) = state.world.get::<&Stats>(entity) {
        lines.push(format!("HP: {}/{}   MP: {}/{}", s.hp, s.max_hp, s.mp, s.max_mp));
    }
    lines.push(format!("Location: {}", room_name));

    send_to_client(registry, client_id, lines.join("\n"));
}

fn handle_quit(state: &mut GameState, client_id: ClientId, registry: &OutputRegistry) {
    send_to_client(registry, client_id, "Farewell. The world fades around you...".to_string());

    if let Some(entity) = state.player_registry.remove(client_id) {
        let room_id = state.world.get::<&Position>(entity).ok().map(|p| p.room_id);
        let player_name = state.world.get::<&Name>(entity).map(|n| n.0.clone()).unwrap_or_default();

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

        // Despawn all items owned by this player (bag + equipped).
        let owned: Vec<hecs::Entity> = {
            let inv: Vec<_> = {
                let mut q = state.world.query::<(&InInventory,)>();
                q.iter().filter(|(_, (inv,))| inv.owner == entity).map(|(e, _)| e).collect()
            };
            let eq: Vec<_> = {
                let mut q = state.world.query::<(&Equipped,)>();
                q.iter().filter(|(_, (eq,))| eq.owner == entity).map(|(e, _)| e).collect()
            };
            inv.into_iter().chain(eq).collect()
        };
        for item in owned {
            state.world.despawn(item).ok();
        }

        if let Some(save_data) = state.extract_save_data(entity) {
            state.pending_saves.push(save_data);
        }
        state.world.despawn(entity).ok();
    }
}

fn handle_admin_who(state: &GameState, client_id: ClientId, registry: &OutputRegistry) {
    let Some(entity) = state.player_registry.get_entity(client_id) else { return };
    if state.world.get::<&AdminFlag>(entity).is_err() {
        send_to_client(registry, client_id, "You don't have that power.".to_string());
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

fn handle_unknown(client_id: ClientId, raw: &str, registry: &OutputRegistry) {
    send_to_client(registry, client_id, format!("Huh? '{}' isn't something I understand.", raw));
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::CharacterData;
    use crate::command::Command;
    use crate::components::{BagCapacity, ItemDescription, ItemName, ItemSlot, RoomContents};
    use crate::direction::Direction;
    use crate::game_state::GameState;
    use crate::item::EquipSlot;
    use crate::systems::output::OutputRegistry;
    use tokio::sync::mpsc;

    fn setup() -> (
        GameState,
        mpsc::Sender<PlayerInput>,
        mpsc::Receiver<PlayerInput>,
        OutputRegistry,
        mpsc::Receiver<String>,
    ) {
        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        let (out_tx, out_rx) = mpsc::channel(32);
        let mut registry = OutputRegistry::new();
        registry.insert(1, out_tx.clone());
        registry.insert(2, out_tx);
        (GameState::new(), cmd_tx, cmd_rx, registry, out_rx)
    }

    fn setup_two() -> (
        GameState,
        mpsc::Sender<PlayerInput>,
        mpsc::Receiver<PlayerInput>,
        OutputRegistry,
        mpsc::Receiver<String>,
        mpsc::Receiver<String>,
    ) {
        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        let (out_tx1, out_rx1) = mpsc::channel(32);
        let (out_tx2, out_rx2) = mpsc::channel(32);
        let mut registry = OutputRegistry::new();
        registry.insert(1, out_tx1);
        registry.insert(2, out_tx2);
        (GameState::new(), cmd_tx, cmd_rx, registry, out_rx1, out_rx2)
    }

    fn connect(id: u64) -> PlayerInput {
        PlayerInput::new(id, Command::Connect(CharacterData::default()))
    }

    fn drain(rx: &mut mpsc::Receiver<String>) -> Vec<String> {
        let mut v = Vec::new();
        while let Ok(m) = rx.try_recv() {
            v.push(m);
        }
        v
    }

    /// Spawns an item entity on the floor of a room.
    fn spawn_floor_item(state: &mut GameState, room_id: u64, name: &str) -> hecs::Entity {
        state.world.spawn((
            ItemName(name.to_string()),
            ItemDescription("A test item.".to_string()),
            ItemSlot(EquipSlot::LeftHand),
            RoomContents { room_id },
        ))
    }

    /// Spawns an item entity in a player's bag.
    fn spawn_bag_item(
        state: &mut GameState,
        owner: hecs::Entity,
        name: &str,
        slot: EquipSlot,
    ) -> hecs::Entity {
        state.world.spawn((
            ItemName(name.to_string()),
            ItemDescription("A test item.".to_string()),
            ItemSlot(slot),
            InInventory { owner },
        ))
    }

    // ── Existing tests ────────────────────────────────────────────────────────

    #[test]
    fn drains_all_pending_commands() {
        let (mut state, tx, mut rx, reg, _) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Look)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn empty_channel_does_not_panic() {
        let (mut state, _tx, mut rx, reg, _) = setup();
        process_input(&mut state, &mut rx, &reg);
    }

    #[test]
    fn does_not_consume_messages_sent_after_drain() {
        let (mut state, tx, mut rx, reg, _) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        tx.try_send(PlayerInput::new(1, Command::Quit)).unwrap();
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn connect_spawns_player_in_world() {
        let (mut state, tx, mut rx, reg, _) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        assert!(state.player_registry.is_connected(1));
    }

    #[test]
    fn look_sends_room_description() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Look)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("Town Square")));
    }

    #[test]
    fn move_valid_exit_updates_entity_position() {
        let (mut state, tx, mut rx, reg, _) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Move(Direction::North))).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let entity = state.player_registry.get_entity(1).unwrap();
        let pos = state.world.get::<&Position>(entity).unwrap();
        assert_eq!(pos.room_id, 2);
    }

    #[test]
    fn move_blocked_exit_sends_error_message() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Move(Direction::Down))).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("no exit")));
    }

    #[test]
    fn say_delivers_to_sender_and_recipient_in_same_room() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(connect(2)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Say("hi".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("You say")));
        assert!(msgs.iter().any(|m| m.contains("Someone says")));
    }

    #[test]
    fn quit_removes_player_and_queues_save() {
        let (mut state, tx, mut rx, reg, _) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        assert_eq!(state.world.len(), 1);

        tx.try_send(PlayerInput::new(1, Command::Quit)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        assert!(!state.player_registry.is_connected(1));
        assert_eq!(state.world.len(), 0);
        assert_eq!(state.pending_saves.len(), 1);
    }

    #[test]
    fn unknown_command_sends_error_to_client() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Unknown("xyzzy".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("xyzzy")));
    }

    #[test]
    fn connect_notifies_players_already_in_room() {
        let (mut state, tx, mut rx, reg, mut out_rx1, _out_rx2) = setup_two();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx1);

        tx.try_send(connect(2)).unwrap();
        process_input(&mut state, &mut rx, &reg);

        let msgs1 = drain(&mut out_rx1);
        assert!(msgs1.iter().any(|m| m.contains("entered the world")));
    }

    #[test]
    fn quit_notifies_remaining_players_in_room() {
        let (mut state, tx, mut rx, reg, mut out_rx1, mut out_rx2) = setup_two();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(connect(2)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx1);
        drain(&mut out_rx2);

        tx.try_send(PlayerInput::new(2, Command::Quit)).unwrap();
        process_input(&mut state, &mut rx, &reg);

        let msgs1 = drain(&mut out_rx1);
        assert!(msgs1.iter().any(|m| m.contains("left the world")));
    }

    #[test]
    fn score_shows_name_hp_and_location() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Score)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        let combined = msgs.join("\n");
        assert!(combined.contains("Adventurer"));
        assert!(combined.contains("HP"));
        assert!(combined.contains("Town Square"));
    }

    #[test]
    fn move_broadcasts_departure_to_same_room() {
        let (mut state, tx, mut rx, reg, mut out_rx1, mut out_rx2) = setup_two();
        tx.try_send(PlayerInput::new(1, Command::Connect(CharacterData { name: "Mover".to_string(), ..Default::default() }))).unwrap();
        tx.try_send(connect(2)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx1);
        drain(&mut out_rx2);

        tx.try_send(PlayerInput::new(1, Command::Move(Direction::North))).unwrap();
        process_input(&mut state, &mut rx, &reg);

        let msgs2 = drain(&mut out_rx2);
        assert!(msgs2.iter().any(|m| m.contains("Mover") && m.contains("leaves")));
    }

    #[test]
    fn move_broadcasts_arrival_to_destination_room() {
        let (mut state, tx, mut rx, reg, mut out_rx1, mut out_rx2) = setup_two();
        tx.try_send(PlayerInput::new(1, Command::Connect(CharacterData { name: "Mover".to_string(), ..Default::default() }))).unwrap();
        tx.try_send(PlayerInput::new(2, Command::Connect(CharacterData { name: "Watcher".to_string(), room_id: 2, ..Default::default() }))).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx1);
        drain(&mut out_rx2);

        tx.try_send(PlayerInput::new(1, Command::Move(Direction::North))).unwrap();
        process_input(&mut state, &mut rx, &reg);

        let msgs2 = drain(&mut out_rx2);
        assert!(msgs2.iter().any(|m| m.contains("Mover") && m.contains("arrives")));
    }

    #[test]
    fn admin_who_denied_for_regular_player() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::AdminWho)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("power") || m.contains("permission")));
    }

    #[test]
    fn admin_who_lists_players_for_admin() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(PlayerInput::new(1, Command::Connect(CharacterData { is_admin: true, name: "Admin".to_string(), ..Default::default() }))).unwrap();
        tx.try_send(PlayerInput::new(1, Command::AdminWho)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("Online Players")));
    }

    // ── Item tests ────────────────────────────────────────────────────────────

    #[test]
    fn look_shows_floor_items() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        // Spawn an item in the starting room.
        let starting_room = crate::world::seed::STARTING_ROOM_ID;
        spawn_floor_item(&mut state, starting_room, "a shiny penny");

        tx.try_send(PlayerInput::new(1, Command::Look)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("a shiny penny")));
    }

    #[test]
    fn get_picks_up_item_from_room() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        let starting_room = crate::world::seed::STARTING_ROOM_ID;
        let item = spawn_floor_item(&mut state, starting_room, "a rusty dagger");

        tx.try_send(PlayerInput::new(1, Command::Get("dagger".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);

        assert!(state.world.get::<&InInventory>(item).is_ok());
        assert!(state.world.get::<&RoomContents>(item).is_err());
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("pick up")));
    }

    #[test]
    fn get_fails_when_item_not_in_room() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        tx.try_send(PlayerInput::new(1, Command::Get("dragon".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("don't see")));
    }

    #[test]
    fn get_fails_when_inventory_full() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        let entity = state.player_registry.get_entity(1).unwrap();
        let starting_room = crate::world::seed::STARTING_ROOM_ID;

        // Fill the bag to capacity.
        for i in 0..BASE_INVENTORY_LIMIT {
            spawn_bag_item(&mut state, entity, &format!("item {i}"), EquipSlot::LeftHand);
        }
        // Place one more on the floor.
        spawn_floor_item(&mut state, starting_room, "the straw that breaks");

        tx.try_send(PlayerInput::new(1, Command::Get("straw".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("full")));
    }

    #[test]
    fn drop_puts_item_in_room() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        let entity = state.player_registry.get_entity(1).unwrap();
        let item = spawn_bag_item(&mut state, entity, "a copper coin", EquipSlot::LeftHand);

        tx.try_send(PlayerInput::new(1, Command::Drop("coin".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);

        assert!(state.world.get::<&RoomContents>(item).is_ok());
        assert!(state.world.get::<&InInventory>(item).is_err());
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("drop")));
    }

    #[test]
    fn drop_fails_when_item_not_in_bag() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        tx.try_send(PlayerInput::new(1, Command::Drop("nothing".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("don't have")));
    }

    #[test]
    fn inventory_lists_bag_and_equipment() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        let entity = state.player_registry.get_entity(1).unwrap();
        spawn_bag_item(&mut state, entity, "a blue gem", EquipSlot::Necklace);

        tx.try_send(PlayerInput::new(1, Command::Inventory)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        let combined = msgs.join("\n");
        assert!(combined.contains("Equipment"));
        assert!(combined.contains("Bag"));
        assert!(combined.contains("a blue gem"));
    }

    #[test]
    fn equip_moves_item_from_bag_to_slot() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        let entity = state.player_registry.get_entity(1).unwrap();
        let item = spawn_bag_item(&mut state, entity, "a rusty sword", EquipSlot::LeftHand);

        tx.try_send(PlayerInput::new(1, Command::Equip("sword".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);

        let eq = state.world.get::<&Equipped>(item).unwrap();
        assert_eq!(eq.slot, EquipSlot::LeftHand);
        assert!(state.world.get::<&InInventory>(item).is_err());
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("equip")));
    }

    #[test]
    fn equip_fails_when_slot_is_occupied() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        let entity = state.player_registry.get_entity(1).unwrap();
        spawn_bag_item(&mut state, entity, "sword one", EquipSlot::LeftHand);
        spawn_bag_item(&mut state, entity, "sword two", EquipSlot::LeftHand);

        tx.try_send(PlayerInput::new(1, Command::Equip("sword one".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        tx.try_send(PlayerInput::new(1, Command::Equip("sword two".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("occupied") || m.contains("Unequip")));
    }

    #[test]
    fn rings_auto_fill_ring1_then_ring2() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        let entity = state.player_registry.get_entity(1).unwrap();
        let ring1 = spawn_bag_item(&mut state, entity, "ring alpha", EquipSlot::Ring1);
        let ring2 = spawn_bag_item(&mut state, entity, "ring beta", EquipSlot::Ring1);

        tx.try_send(PlayerInput::new(1, Command::Equip("ring alpha".to_string()))).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Equip("ring beta".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);

        assert_eq!(state.world.get::<&Equipped>(ring1).unwrap().slot, EquipSlot::Ring1);
        assert_eq!(state.world.get::<&Equipped>(ring2).unwrap().slot, EquipSlot::Ring2);
        drain(&mut out_rx);
    }

    #[test]
    fn unequip_moves_item_back_to_bag() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        let entity = state.player_registry.get_entity(1).unwrap();
        let item = state.world.spawn((
            ItemName("a helm".to_string()),
            ItemDescription("A helm.".to_string()),
            ItemSlot(EquipSlot::Head),
            Equipped { owner: entity, slot: EquipSlot::Head },
        ));

        tx.try_send(PlayerInput::new(1, Command::Unequip("head".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);

        assert!(state.world.get::<&InInventory>(item).is_ok());
        assert!(state.world.get::<&Equipped>(item).is_err());
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("unequip")));
    }

    #[test]
    fn examine_shows_item_description() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        let starting_room = crate::world::seed::STARTING_ROOM_ID;
        state.world.spawn((
            ItemName("an ancient tome".to_string()),
            ItemDescription("Its pages are filled with forgotten lore.".to_string()),
            RoomContents { room_id: starting_room },
        ));

        tx.try_send(PlayerInput::new(1, Command::Examine("tome".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        let combined = msgs.join("\n");
        assert!(combined.contains("forgotten lore"));
    }

    #[test]
    fn quit_despawns_owned_items() {
        let (mut state, tx, mut rx, reg, _) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);

        let entity = state.player_registry.get_entity(1).unwrap();
        spawn_bag_item(&mut state, entity, "item in bag", EquipSlot::Necklace);
        state.world.spawn((
            ItemName("item equipped".to_string()),
            ItemDescription("Equipped.".to_string()),
            ItemSlot(EquipSlot::Head),
            Equipped { owner: entity, slot: EquipSlot::Head },
        ));
        assert_eq!(state.world.len(), 3); // player + 2 items

        tx.try_send(PlayerInput::new(1, Command::Quit)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        assert_eq!(state.world.len(), 0); // all despawned
    }

    #[test]
    fn equipping_bag_raises_inventory_limit() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        let entity = state.player_registry.get_entity(1).unwrap();
        assert_eq!(effective_inventory_limit(&state.world, entity), BASE_INVENTORY_LIMIT);

        // Put a bag in the inventory.
        let bag = state.world.spawn((
            ItemName("a small pouch".to_string()),
            ItemDescription("Adds 5 slots.".to_string()),
            ItemSlot(EquipSlot::Bag1),
            BagCapacity(5),
            InInventory { owner: entity },
        ));

        // Equip it.
        tx.try_send(PlayerInput::new(1, Command::Equip("pouch".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);

        assert_eq!(state.world.get::<&Equipped>(bag).unwrap().slot, EquipSlot::Bag1);
        assert_eq!(effective_inventory_limit(&state.world, entity), BASE_INVENTORY_LIMIT + 5);

        // Unequipping puts it back in the bag and drops the limit.
        tx.try_send(PlayerInput::new(1, Command::Unequip("bag".to_string()))).unwrap();
        process_input(&mut state, &mut rx, &reg);

        assert!(state.world.get::<&InInventory>(bag).is_ok());
        assert_eq!(effective_inventory_limit(&state.world, entity), BASE_INVENTORY_LIMIT);
    }

    #[test]
    fn bags_auto_fill_bag1_through_bag4() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        let entity = state.player_registry.get_entity(1).unwrap();
        let expected_slots = [EquipSlot::Bag1, EquipSlot::Bag2, EquipSlot::Bag3, EquipSlot::Bag4];

        let mut bags = Vec::new();
        for i in 0..4 {
            let b = state.world.spawn((
                ItemName(format!("bag {i}")),
                ItemDescription("A bag.".to_string()),
                ItemSlot(EquipSlot::Bag1),
                BagCapacity(5),
                InInventory { owner: entity },
            ));
            bags.push(b);
            tx.try_send(PlayerInput::new(1, Command::Equip(format!("bag {i}")))).unwrap();
        }
        process_input(&mut state, &mut rx, &reg);

        for (bag, &slot) in bags.iter().zip(expected_slots.iter()) {
            assert_eq!(state.world.get::<&Equipped>(*bag).unwrap().slot, slot);
        }
        drain(&mut out_rx);
    }
}
