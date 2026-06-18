use crate::command::ClientId;
use crate::components::{
    BagCapacity, ItemDescription, ItemId, ItemName, ItemSlot, RoomContents, TwoHanded,
};
use crate::game_state::{AdminDbOp, GameState};
use crate::item::{EquipRequirements, EquipSlot, ItemBonuses, ItemData, ItemLocation};
use crate::systems::output::{send_to_client, OutputRegistry};
use crate::systems::queries::find_item_in_room;
use crate::world::loader::ItemDef;

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

    // Create item definition.
    let def_id = state.next_def_id;
    state.next_def_id += 1;

    let def = ItemDef {
        id: def_id,
        name: name.clone(),
        description: description.clone(),
        equip_slot: None,
        two_handed: false,
        bag_capacity: None,
        requirements: EquipRequirements::default(),
        bonuses: ItemBonuses::default(),
    };
    state.item_templates.insert(def_id, def.clone());
    state.pending_admin_ops.push(AdminDbOp::CreateItemDef(def));

    // Spawn one instance of that definition in this room.
    let instance_id = state.next_item_id;
    state.next_item_id += 1;

    let item_data = ItemData {
        id: instance_id,
        def_id,
        name: name.clone(),
        description: description.clone(),
        equip_slot: None,
        two_handed: false,
        bag_capacity: None,
        requirements: EquipRequirements::default(),
        bonuses: ItemBonuses::default(),
        location: ItemLocation::Room(room_id),
    };

    state.world.spawn((
        ItemId(instance_id),
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
        format!("<dim>[Admin] Definition #{def_id} '{name}' created. Instance #{instance_id} placed in this room.</dim>"),
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
    def_id: i64,
    new_name: String,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    if let Some(def) = state.item_templates.get_mut(&def_id) {
        def.name = new_name.clone();
    } else {
        send_to_client(
            registry,
            client_id,
            format!("No item definition #{def_id}. Use @idefs to list all definitions."),
        );
        return;
    }

    state.pending_admin_ops.push(AdminDbOp::UpdateDefName {
        id: def_id,
        name: new_name.clone(),
    });
    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] Definition #{def_id} renamed to '{new_name}'.</dim>"),
    );
}

pub(crate) fn handle_admin_idesc(
    state: &mut GameState,
    client_id: ClientId,
    def_id: i64,
    new_desc: String,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    if let Some(def) = state.item_templates.get_mut(&def_id) {
        def.description = new_desc.clone();
    } else {
        send_to_client(
            registry,
            client_id,
            format!("No item definition #{def_id}. Use @idefs to list all definitions."),
        );
        return;
    }

    state.pending_admin_ops.push(AdminDbOp::UpdateDefDesc {
        id: def_id,
        description: new_desc,
    });
    send_to_client(
        registry,
        client_id,
        format!("<dim>[Admin] Definition #{def_id} description updated.</dim>"),
    );
}

pub(crate) fn handle_admin_islot(
    state: &mut GameState,
    client_id: ClientId,
    def_id: i64,
    slot_str: String,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    let lower = slot_str.to_lowercase();

    if lower == "none" {
        if let Some(def) = state.item_templates.get_mut(&def_id) {
            def.equip_slot = None;
        } else {
            send_to_client(
                registry,
                client_id,
                format!("No item definition #{def_id}. Use @idefs to list all definitions."),
            );
            return;
        }
        state.pending_admin_ops.push(AdminDbOp::UpdateDefSlot {
            id: def_id,
            equip_slot: None,
        });
        send_to_client(
            registry,
            client_id,
            format!("<dim>[Admin] Definition #{def_id} equip slot cleared.</dim>"),
        );
    } else if let Some(slot) = EquipSlot::parse(&lower) {
        if let Some(def) = state.item_templates.get_mut(&def_id) {
            def.equip_slot = Some(lower.clone());
        } else {
            send_to_client(
                registry,
                client_id,
                format!("No item definition #{def_id}. Use @idefs to list all definitions."),
            );
            return;
        }
        state.pending_admin_ops.push(AdminDbOp::UpdateDefSlot {
            id: def_id,
            equip_slot: Some(lower),
        });
        send_to_client(
            registry,
            client_id,
            format!(
                "<dim>[Admin] Definition #{def_id} slot set to {}.</dim>",
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
    def_id: i64,
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

    let Some(def) = state.item_templates.get_mut(&def_id) else {
        send_to_client(
            registry,
            client_id,
            format!("No item definition #{def_id}. Use @idefs to list all definitions."),
        );
        return;
    };

    match stat.to_lowercase().as_str() {
        "str" | "strength" => def.requirements.strength = value,
        "dex" | "dexterity" => def.requirements.dexterity = value,
        "knw" | "knowledge" => def.requirements.knowledge = value,
        "level" | "lv" => def.requirements.level = value,
        _ => {
            send_to_client(
                registry,
                client_id,
                "Unknown stat. Use: str, dex, knw, level".to_string(),
            );
            return;
        }
    }

    let reqs = def.requirements;
    state.pending_admin_ops.push(AdminDbOp::UpdateDefReq {
        id: def_id,
        level: reqs.level,
        strength: reqs.strength,
        dexterity: reqs.dexterity,
        knowledge: reqs.knowledge,
    });

    send_to_client(
        registry,
        client_id,
        format!(
            "<dim>[Admin] Definition #{def_id} requirements: Lv {} STR {} DEX {} KNW {}.</dim>",
            reqs.level, reqs.strength, reqs.dexterity, reqs.knowledge
        ),
    );
}

pub(crate) fn handle_admin_ispawn(
    state: &mut GameState,
    client_id: ClientId,
    def_id: i64,
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

    let Some(def) = state.item_templates.get(&def_id).cloned() else {
        send_to_client(
            registry,
            client_id,
            format!("No item definition #{def_id}. Use @idefs to list all definitions."),
        );
        return;
    };

    let instance_id = state.next_item_id;
    state.next_item_id += 1;

    let equip_slot = def.equip_slot.as_deref().and_then(EquipSlot::parse);

    let item_data = ItemData {
        id: instance_id,
        def_id,
        name: def.name.clone(),
        description: def.description.clone(),
        equip_slot,
        two_handed: def.two_handed,
        bag_capacity: def.bag_capacity,
        requirements: def.requirements,
        bonuses: def.bonuses,
        location: ItemLocation::Room(room_id),
    };

    let mut builder = hecs::EntityBuilder::new();
    builder.add(ItemId(instance_id));
    builder.add(ItemName(def.name.clone()));
    builder.add(ItemDescription(def.description.clone()));
    builder.add(RoomContents { room_id });
    if let Some(slot) = equip_slot {
        builder.add(ItemSlot(slot));
    }
    if def.two_handed {
        builder.add(TwoHanded);
    }
    if let Some(cap) = def.bag_capacity {
        builder.add(BagCapacity(cap));
    }
    if def.requirements.has_any() {
        builder.add(def.requirements);
    }
    if def.bonuses.has_any() {
        builder.add(def.bonuses);
    }
    state.world.spawn(builder.build());

    state
        .pending_admin_ops
        .push(AdminDbOp::CreateItem(item_data));

    send_to_client(
        registry,
        client_id,
        format!(
            "<dim>[Admin] Spawned '{}' (def #{def_id}) as instance #{instance_id} in this room.</dim>",
            def.name
        ),
    );
}

pub(crate) fn handle_admin_ibonus(
    state: &mut GameState,
    client_id: ClientId,
    def_id: i64,
    field: String,
    value: i32,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    let Some(def) = state.item_templates.get_mut(&def_id) else {
        send_to_client(
            registry,
            client_id,
            format!("No item definition #{def_id}. Use @idefs to list all definitions."),
        );
        return;
    };

    match field.to_lowercase().as_str() {
        "str" | "strength" => def.bonuses.bonus_strength = value,
        "dex" | "dexterity" => def.bonuses.bonus_dexterity = value,
        "knw" | "knowledge" => def.bonuses.bonus_knowledge = value,
        "hp" | "maxhp" => def.bonuses.bonus_max_hp = value,
        "mindmg" | "mindamage" => def.bonuses.bonus_min_damage = value,
        "maxdmg" | "maxdamage" => def.bonuses.bonus_max_damage = value,
        "armor" | "armour" | "ac" => def.bonuses.bonus_armor = value,
        _ => {
            send_to_client(
                registry,
                client_id,
                "Unknown field. Use: str, dex, knw, hp, mindmg, maxdmg, armor".to_string(),
            );
            return;
        }
    }

    let bonuses = def.bonuses;
    state.pending_admin_ops.push(AdminDbOp::UpdateDefBonuses {
        id: def_id,
        bonuses,
    });

    send_to_client(
        registry,
        client_id,
        format!(
            "<dim>[Admin] Definition #{def_id} bonuses: STR+{} DEX+{} KNW+{} HP+{} DMG {}-{} ARM {}.</dim>",
            bonuses.bonus_strength,
            bonuses.bonus_dexterity,
            bonuses.bonus_knowledge,
            bonuses.bonus_max_hp,
            bonuses.bonus_min_damage,
            bonuses.bonus_max_damage,
            bonuses.bonus_armor,
        ),
    );
}

pub(crate) fn handle_admin_idefs(
    state: &mut GameState,
    client_id: ClientId,
    registry: &OutputRegistry,
) {
    let Some(entity) = state.player_registry.get_entity(client_id) else {
        return;
    };
    if !super::require_admin(state, client_id, entity, registry) {
        return;
    }

    let mut defs: Vec<(i64, &ItemDef)> = state
        .item_templates
        .iter()
        .map(|(&id, d)| (id, d))
        .collect();
    defs.sort_by_key(|(id, _)| *id);

    if defs.is_empty() {
        send_to_client(
            registry,
            client_id,
            "No item definitions loaded.".to_string(),
        );
        return;
    }

    let mut lines = vec!["<yellow>=== Item Definitions ===</yellow>".to_string()];
    for (id, def) in defs {
        let slot_label = match &def.equip_slot {
            Some(s) => format!(" [{}]", s),
            None => String::new(),
        };
        lines.push(format!("  #{id:<6} {}{}", def.name, slot_label));
    }
    send_to_client(registry, client_id, lines.join("\n"));
}
