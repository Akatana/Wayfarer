mod admin;
mod items;
mod movement;
mod npcs;
mod world;

use tokio::sync::mpsc;

use crate::command::{Command, PlayerInput};
use crate::components::{
    BagCapacity, Equipped, Hostile, InInventory, ItemName, Name, NpcId, Position, RoomContents,
    TwoHanded,
};
use crate::game_state::GameState;
use crate::item::EquipSlot;
use crate::systems::output::OutputRegistry;

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
    let id = input.client_id;
    match input.command {
        Command::Connect(data) => world::handle_connect(state, id, data, registry),
        Command::Look => world::handle_look(state, id, registry),
        Command::Move(dir) => movement::handle_move(state, id, dir, registry),
        Command::Say(msg) => world::handle_say(state, id, &msg, registry),
        Command::Get(target) => items::handle_get(state, id, &target, registry),
        Command::Drop(target) => items::handle_drop(state, id, &target, registry),
        Command::Inventory => items::handle_inventory(state, id, registry),
        Command::Equip(target) => items::handle_equip(state, id, &target, registry),
        Command::Unequip(target) => items::handle_unequip(state, id, &target, registry),
        Command::Examine(target) => world::handle_examine(state, id, &target, registry),
        Command::Score => world::handle_score(state, id, registry),
        Command::Quit => world::handle_quit(state, id, registry),
        Command::Talk(target) => npcs::handle_talk(state, id, &target, registry),
        Command::AdminWho => admin::handle_admin_who(state, id, registry),
        Command::AdminGoto(room_id) => admin::handle_admin_goto(state, id, room_id, registry),
        Command::AdminDig(dir, name) => admin::handle_admin_dig(state, id, dir, name, registry),
        Command::AdminLink(dir, dest) => admin::handle_admin_link(state, id, dir, dest, registry),
        Command::AdminUnlink(dir) => admin::handle_admin_unlink(state, id, dir, registry),
        Command::AdminRename(name) => admin::handle_admin_rename(state, id, name, registry),
        Command::AdminRedesc(desc) => admin::handle_admin_redesc(state, id, desc, registry),
        Command::AdminRoomInfo => admin::handle_admin_roominfo(state, id, registry),
        Command::AdminMitem(spec) => admin::handle_admin_mitem(state, id, spec, registry),
        Command::AdminDestroy(target) => admin::handle_admin_destroy(state, id, &target, registry),
        Command::AdminIname(item_id, name) => {
            admin::handle_admin_iname(state, id, item_id, name, registry)
        }
        Command::AdminIdesc(item_id, desc) => {
            admin::handle_admin_idesc(state, id, item_id, desc, registry)
        }
        Command::AdminIslot(item_id, slot) => {
            admin::handle_admin_islot(state, id, item_id, slot, registry)
        }
        Command::AdminIreq(item_id, stat, val) => {
            admin::handle_admin_ireq(state, id, item_id, stat, val, registry)
        }
        Command::AdminMnpc(spec) => admin::handle_admin_mnpc(state, id, spec, registry),
        Command::AdminNdestroy(target) => {
            admin::handle_admin_ndestroy(state, id, &target, registry)
        }
        Command::AdminNname(npc_id, name) => {
            admin::handle_admin_nname(state, id, npc_id, name, registry)
        }
        Command::AdminNdesc(npc_id, desc) => {
            admin::handle_admin_ndesc(state, id, npc_id, desc, registry)
        }
        Command::AdminNgreet(npc_id, text) => {
            admin::handle_admin_ngreet(state, id, npc_id, text, registry)
        }
        Command::AdminNhostile(npc_id, hostile) => {
            admin::handle_admin_nhostile(state, id, npc_id, hostile, registry)
        }
        Command::AdminNpatrol(npc_id, spec) => {
            admin::handle_admin_npatrol(state, id, npc_id, spec, registry)
        }
        Command::AdminNlist => admin::handle_admin_nlist(state, id, registry),
        Command::AdminNinfo(npc_id) => admin::handle_admin_ninfo(state, id, npc_id, registry),
        Command::Unknown(raw) => world::handle_unknown(id, &raw, registry),
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

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

    let npc_pairs: Vec<(hecs::Entity, String)> = {
        let mut q = state.world.query::<(&Name, &Position, &NpcId)>();
        q.iter()
            .filter(|(_, (_, pos, _))| pos.room_id == room_id)
            .map(|(e, (name, _, _))| (e, name.0.clone()))
            .collect()
    };
    if !npc_pairs.is_empty() {
        let mut labels: Vec<String> = npc_pairs
            .into_iter()
            .map(|(e, name)| {
                if state.world.get::<&Hostile>(e).is_ok() {
                    format!("{} <red>(hostile)</red>", name)
                } else {
                    name
                }
            })
            .collect();
        labels.sort_unstable();
        desc.push_str(&format!("\n[ NPCs: {} ]", labels.join(", ")));
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

    fn spawn_floor_item(state: &mut GameState, room_id: u64, name: &str) -> hecs::Entity {
        state.world.spawn((
            ItemName(name.to_string()),
            ItemDescription("A test item.".to_string()),
            ItemSlot(EquipSlot::LeftHand),
            RoomContents { room_id },
        ))
    }

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
        tx.try_send(PlayerInput::new(1, Command::Move(Direction::North)))
            .unwrap();
        process_input(&mut state, &mut rx, &reg);
        let entity = state.player_registry.get_entity(1).unwrap();
        let pos = state.world.get::<&Position>(entity).unwrap();
        assert_eq!(pos.room_id, 2);
    }

    #[test]
    fn move_blocked_exit_sends_error_message() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Move(Direction::Down)))
            .unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("no exit")));
    }

    #[test]
    fn say_delivers_to_sender_and_recipient_in_same_room() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(connect(2)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::Say("hi".to_string())))
            .unwrap();
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
        tx.try_send(PlayerInput::new(1, Command::Unknown("xyzzy".to_string())))
            .unwrap();
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
        tx.try_send(PlayerInput::new(
            1,
            Command::Connect(CharacterData {
                name: "Mover".to_string(),
                ..Default::default()
            }),
        ))
        .unwrap();
        tx.try_send(connect(2)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx1);
        drain(&mut out_rx2);

        tx.try_send(PlayerInput::new(1, Command::Move(Direction::North)))
            .unwrap();
        process_input(&mut state, &mut rx, &reg);

        let msgs2 = drain(&mut out_rx2);
        assert!(msgs2
            .iter()
            .any(|m| m.contains("Mover") && m.contains("leaves")));
    }

    #[test]
    fn move_broadcasts_arrival_to_destination_room() {
        let (mut state, tx, mut rx, reg, mut out_rx1, mut out_rx2) = setup_two();
        tx.try_send(PlayerInput::new(
            1,
            Command::Connect(CharacterData {
                name: "Mover".to_string(),
                ..Default::default()
            }),
        ))
        .unwrap();
        tx.try_send(PlayerInput::new(
            2,
            Command::Connect(CharacterData {
                name: "Watcher".to_string(),
                room_id: 2,
                ..Default::default()
            }),
        ))
        .unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx1);
        drain(&mut out_rx2);

        tx.try_send(PlayerInput::new(1, Command::Move(Direction::North)))
            .unwrap();
        process_input(&mut state, &mut rx, &reg);

        let msgs2 = drain(&mut out_rx2);
        assert!(msgs2
            .iter()
            .any(|m| m.contains("Mover") && m.contains("arrives")));
    }

    #[test]
    fn admin_who_denied_for_regular_player() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        tx.try_send(PlayerInput::new(1, Command::AdminWho)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs
            .iter()
            .any(|m| m.contains("power") || m.contains("permission")));
    }

    #[test]
    fn admin_who_lists_players_for_admin() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(PlayerInput::new(
            1,
            Command::Connect(CharacterData {
                is_admin: true,
                name: "Admin".to_string(),
                ..Default::default()
            }),
        ))
        .unwrap();
        tx.try_send(PlayerInput::new(1, Command::AdminWho)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs.iter().any(|m| m.contains("Online Players")));
    }

    #[test]
    fn look_shows_floor_items() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

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

        tx.try_send(PlayerInput::new(1, Command::Get("dagger".to_string())))
            .unwrap();
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

        tx.try_send(PlayerInput::new(1, Command::Get("dragon".to_string())))
            .unwrap();
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

        for i in 0..BASE_INVENTORY_LIMIT {
            spawn_bag_item(
                &mut state,
                entity,
                &format!("item {i}"),
                EquipSlot::LeftHand,
            );
        }
        spawn_floor_item(&mut state, starting_room, "the straw that breaks");

        tx.try_send(PlayerInput::new(1, Command::Get("straw".to_string())))
            .unwrap();
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

        tx.try_send(PlayerInput::new(1, Command::Drop("coin".to_string())))
            .unwrap();
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

        tx.try_send(PlayerInput::new(1, Command::Drop("nothing".to_string())))
            .unwrap();
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

        tx.try_send(PlayerInput::new(1, Command::Inventory))
            .unwrap();
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

        tx.try_send(PlayerInput::new(1, Command::Equip("sword".to_string())))
            .unwrap();
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

        tx.try_send(PlayerInput::new(1, Command::Equip("sword one".to_string())))
            .unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        tx.try_send(PlayerInput::new(1, Command::Equip("sword two".to_string())))
            .unwrap();
        process_input(&mut state, &mut rx, &reg);
        let msgs = drain(&mut out_rx);
        assert!(msgs
            .iter()
            .any(|m| m.contains("occupied") || m.contains("Unequip")));
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

        tx.try_send(PlayerInput::new(
            1,
            Command::Equip("ring alpha".to_string()),
        ))
        .unwrap();
        tx.try_send(PlayerInput::new(1, Command::Equip("ring beta".to_string())))
            .unwrap();
        process_input(&mut state, &mut rx, &reg);

        assert_eq!(
            state.world.get::<&Equipped>(ring1).unwrap().slot,
            EquipSlot::Ring1
        );
        assert_eq!(
            state.world.get::<&Equipped>(ring2).unwrap().slot,
            EquipSlot::Ring2
        );
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
            Equipped {
                owner: entity,
                slot: EquipSlot::Head,
            },
        ));

        tx.try_send(PlayerInput::new(1, Command::Unequip("head".to_string())))
            .unwrap();
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
            RoomContents {
                room_id: starting_room,
            },
        ));

        tx.try_send(PlayerInput::new(1, Command::Examine("tome".to_string())))
            .unwrap();
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
            Equipped {
                owner: entity,
                slot: EquipSlot::Head,
            },
        ));
        assert_eq!(state.world.len(), 3);

        tx.try_send(PlayerInput::new(1, Command::Quit)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        assert_eq!(state.world.len(), 0);
    }

    #[test]
    fn equipping_bag_raises_inventory_limit() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        let entity = state.player_registry.get_entity(1).unwrap();
        assert_eq!(
            effective_inventory_limit(&state.world, entity),
            BASE_INVENTORY_LIMIT
        );

        let bag = state.world.spawn((
            ItemName("a small pouch".to_string()),
            ItemDescription("Adds 5 slots.".to_string()),
            ItemSlot(EquipSlot::Bag1),
            BagCapacity(5),
            InInventory { owner: entity },
        ));

        tx.try_send(PlayerInput::new(1, Command::Equip("pouch".to_string())))
            .unwrap();
        process_input(&mut state, &mut rx, &reg);

        assert_eq!(
            state.world.get::<&Equipped>(bag).unwrap().slot,
            EquipSlot::Bag1
        );
        assert_eq!(
            effective_inventory_limit(&state.world, entity),
            BASE_INVENTORY_LIMIT + 5
        );

        tx.try_send(PlayerInput::new(1, Command::Unequip("bag".to_string())))
            .unwrap();
        process_input(&mut state, &mut rx, &reg);

        assert!(state.world.get::<&InInventory>(bag).is_ok());
        assert_eq!(
            effective_inventory_limit(&state.world, entity),
            BASE_INVENTORY_LIMIT
        );
    }

    #[test]
    fn bags_auto_fill_bag1_through_bag4() {
        let (mut state, tx, mut rx, reg, mut out_rx) = setup();
        tx.try_send(connect(1)).unwrap();
        process_input(&mut state, &mut rx, &reg);
        drain(&mut out_rx);

        let entity = state.player_registry.get_entity(1).unwrap();
        let expected_slots = [
            EquipSlot::Bag1,
            EquipSlot::Bag2,
            EquipSlot::Bag3,
            EquipSlot::Bag4,
        ];

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
            tx.try_send(PlayerInput::new(1, Command::Equip(format!("bag {i}"))))
                .unwrap();
        }
        process_input(&mut state, &mut rx, &reg);

        for (bag, &slot) in bags.iter().zip(expected_slots.iter()) {
            assert_eq!(state.world.get::<&Equipped>(*bag).unwrap().slot, slot);
        }
        drain(&mut out_rx);
    }
}
