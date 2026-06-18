mod admin;
mod combat;
mod items;
mod movement;
mod npcs;
mod world;

use tokio::sync::mpsc;

use crate::command::{ClientId, Command, PlayerInput};
use crate::components::{
    BagCapacity, CharacterId, Equipped, InDialogue, InInventory, ItemId, ItemName, Name, NpcId,
    PlayerQuests, Position, RoomContents, Stats, TwoHanded,
};
use crate::dialogue::{DialogueCondition, DialogueEffect};
use crate::game_state::GameState;
use crate::item::{EquipSlot, ItemLocation, ItemLocationSave};
use crate::quest::{PlayerQuestState, QuestDef, QuestObjectiveDef, QuestSave, QuestStatus};
use crate::systems::output::{send_to_client, OutputRegistry};
use crate::systems::quest::{quest_mark_objective, quest_turn_in};

const BASE_INVENTORY_LIMIT: usize = 20;

/// Phase 1 of each tick: drains the network channel into per-player queues,
/// then dispatches exactly one command per player. No `.await` calls permitted.
pub fn process_input(
    state: &mut GameState,
    command_rx: &mut mpsc::Receiver<PlayerInput>,
    output_registry: &OutputRegistry,
) {
    // Drain channel into per-player queues.
    while let Ok(input) = command_rx.try_recv() {
        state
            .pending_commands
            .entry(input.client_id)
            .or_default()
            .push_back(input.command);
    }

    // Pop one command per player (collect releases the borrow before dispatch).
    let to_dispatch: Vec<PlayerInput> = state
        .pending_commands
        .iter_mut()
        .filter_map(|(&client_id, queue)| {
            queue
                .pop_front()
                .map(|command| PlayerInput { client_id, command })
        })
        .collect();

    // Drop empty queues.
    state.pending_commands.retain(|_, q| !q.is_empty());

    // Dispatch exactly one command per player this tick.
    for input in to_dispatch {
        dispatch(state, input, output_registry);
    }
}

fn dispatch(state: &mut GameState, input: PlayerInput, registry: &OutputRegistry) {
    let id = input.client_id;

    // ── Dialogue intercept ────────────────────────────────────────────────────
    // If the player is mid-conversation, most commands are blocked. Movement
    // and combat exit dialogue and fall through to normal handling.
    if !matches!(input.command, Command::Connect(_)) {
        if let Some(entity) = state.player_registry.get_entity(id) {
            let in_dialogue = state
                .world
                .get::<&InDialogue>(entity)
                .ok()
                .map(|d| (d.npc_entity, d.npc_db_id, d.node_id.clone()));
            if in_dialogue.is_some() {
                match &input.command {
                    Command::DialogueChoice(n) => {
                        let n = *n;
                        handle_dialogue_choice(state, id, n, registry);
                        return;
                    }
                    Command::Quit => {
                        // Treat quit/bye as "end conversation" while in dialogue.
                        state.world.remove_one::<InDialogue>(entity).ok();
                        send_to_client(
                            registry,
                            id,
                            "You say goodbye and end the conversation.".to_string(),
                        );
                        return;
                    }
                    Command::Move(_) | Command::Flee | Command::Attack(_) => {
                        // Exit dialogue; fall through to process the command normally.
                        state.world.remove_one::<InDialogue>(entity).ok();
                    }
                    _ => {
                        send_to_client(
                            registry,
                            id,
                            "You are in a conversation. Enter a number to respond, or type 'bye' to leave.".to_string(),
                        );
                        return;
                    }
                }
            }
        }
    }

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
        Command::Help(topic) => world::handle_help(state, id, topic.as_deref(), registry),
        Command::Attack(target) => combat::handle_attack(state, id, &target, registry),
        Command::Flee => combat::handle_flee(state, id, registry),
        Command::Talk(target) => npcs::handle_talk(state, id, &target, registry),
        Command::Balance => world::handle_balance(state, id, registry),
        Command::QuestLog => world::handle_quest_log(state, id, registry),
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
        Command::AdminIspawn(def_id) => admin::handle_admin_ispawn(state, id, def_id, registry),
        Command::AdminIdefs => admin::handle_admin_idefs(state, id, registry),
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
        Command::AdminNpassive(npc_id, passive) => {
            admin::handle_admin_npassive(state, id, npc_id, passive, registry)
        }
        Command::AdminNpatrol(npc_id, spec) => {
            admin::handle_admin_npatrol(state, id, npc_id, spec, registry)
        }
        Command::AdminNlist => admin::handle_admin_nlist(state, id, registry),
        Command::AdminNinfo(npc_id) => admin::handle_admin_ninfo(state, id, npc_id, registry),
        Command::AdminQlist => admin::handle_admin_qlist(state, id, registry),
        Command::AdminQinfo(quest_id) => admin::handle_admin_qinfo(state, id, quest_id, registry),
        Command::AdminQgive(name, quest_id) => {
            admin::handle_admin_qgive(state, id, name, quest_id, registry)
        }
        Command::AdminQreset(name, quest_id) => {
            admin::handle_admin_qreset(state, id, name, quest_id, registry)
        }
        Command::DialogueChoice(_) => {
            // Typed a number while not in dialogue — treat as unknown.
            send_to_client(
                registry,
                id,
                "You aren't in a conversation right now.".to_string(),
            );
        }
        Command::Unknown(raw) => world::handle_unknown(id, &raw, registry),
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn describe_room(state: &GameState, room_id: u64, viewer: hecs::Entity) -> Option<String> {
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

    // Collect viewer quest data for NPC markers (borrows released after this block).
    let viewer_quest_ids: Vec<i64> = state
        .world
        .get::<&PlayerQuests>(viewer)
        .ok()
        .map(|pq| pq.0.iter().map(|s| s.quest_id).collect())
        .unwrap_or_default();
    let ready_turn_ins: Vec<(i64, usize)> = state
        .world
        .get::<&PlayerQuests>(viewer)
        .ok()
        .map(|pq| {
            pq.0.iter()
                .filter(|s| s.status == QuestStatus::ReadyToTurnIn)
                .map(|s| (s.quest_id, s.phase))
                .collect()
        })
        .unwrap_or_default();

    let npc_pairs: Vec<(hecs::Entity, i64, String)> = {
        let mut q = state.world.query::<(&Name, &Position, &NpcId)>();
        q.iter()
            .filter(|(_, (_, pos, _))| pos.room_id == room_id)
            .map(|(e, (name, _, npc_id))| (e, npc_id.0, name.0.clone()))
            .collect()
    };

    if !npc_pairs.is_empty() {
        let mut labels: Vec<String> = npc_pairs
            .into_iter()
            .map(|(e, npc_db_id, name)| {
                let marker = if ready_turn_ins.iter().any(|(qid, phase)| {
                    state
                        .quest_defs
                        .get(qid)
                        .and_then(|def| def.phases.get(*phase))
                        .and_then(|p| p.completion_npc_id)
                        == Some(npc_db_id)
                }) {
                    " <yellow>[?]</yellow>"
                } else if state.quest_defs.values().any(|def| {
                    def.giver_npc_id == Some(npc_db_id) && !viewer_quest_ids.contains(&def.id)
                }) {
                    " <yellow>[!]</yellow>"
                } else {
                    ""
                };
                if state.is_hostile(e) {
                    format!("{}{} <red>(hostile)</red>", name, marker)
                } else {
                    format!("{}{}", name, marker)
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

// ── Dialogue helpers ──────────────────────────────────────────────────────────

/// Displays a dialogue node to the player: NPC speech then numbered options.
/// Removes `InDialogue` and prints "(The conversation ends.)" if no options are visible.
pub(super) fn show_dialogue_node(
    state: &mut GameState,
    client_id: ClientId,
    npc_entity: hecs::Entity,
    node_id: &str,
    registry: &OutputRegistry,
) {
    let Some(player_entity) = state.player_registry.get_entity(client_id) else {
        return;
    };

    let npc_name = state
        .world
        .get::<&Name>(npc_entity)
        .ok()
        .map(|n| n.0.clone())
        .unwrap_or_else(|| "???".to_string());

    let npc_db_id = state
        .world
        .get::<&NpcId>(npc_entity)
        .ok()
        .map(|n| n.0)
        .unwrap_or(0);

    let player_quests = state
        .world
        .get::<&PlayerQuests>(player_entity)
        .ok()
        .map(|pq| pq.0.clone())
        .unwrap_or_default();
    let player_level = state
        .world
        .get::<&Stats>(player_entity)
        .ok()
        .map(|s| s.level)
        .unwrap_or(1);

    let dialogue = match state.dialogue_defs.get(&npc_db_id) {
        Some(d) => d,
        None => return,
    };
    let node = match dialogue.find_node(node_id) {
        Some(n) => n,
        None => return,
    };

    let visible: Vec<&crate::dialogue::DialogueOption> = node
        .options
        .iter()
        .filter(|opt| {
            check_conditions(
                &opt.conditions,
                &player_quests,
                player_level,
                &state.quest_defs,
            )
        })
        .collect();

    send_to_client(
        registry,
        client_id,
        format!("{} says: \"{}\"", npc_name, node.text),
    );

    if visible.is_empty() {
        state.world.remove_one::<InDialogue>(player_entity).ok();
        send_to_client(registry, client_id, "(The conversation ends.)".to_string());
    } else {
        for (i, opt) in visible.iter().enumerate() {
            send_to_client(registry, client_id, format!("  [{}] {}", i + 1, opt.text));
        }
    }
}

/// Handles the player typing a numeric dialogue choice.
fn handle_dialogue_choice(
    state: &mut GameState,
    client_id: ClientId,
    choice: usize,
    registry: &OutputRegistry,
) {
    let Some(player_entity) = state.player_registry.get_entity(client_id) else {
        return;
    };

    // Clone dialogue state to release the borrow.
    let (npc_entity, npc_db_id, node_id) = {
        let Ok(dlg) = state.world.get::<&InDialogue>(player_entity) else {
            return;
        };
        (dlg.npc_entity, dlg.npc_db_id, dlg.node_id.clone())
    };

    // Ensure NPC is still in the same room.
    let player_room = state
        .world
        .get::<&Position>(player_entity)
        .ok()
        .map(|p| p.room_id);
    let npc_room = state
        .world
        .get::<&Position>(npc_entity)
        .ok()
        .map(|p| p.room_id);
    if player_room.is_none() || player_room != npc_room {
        state.world.remove_one::<InDialogue>(player_entity).ok();
        send_to_client(registry, client_id, "The conversation ends.".to_string());
        return;
    }

    // Clone what we need to avoid borrow conflicts.
    let dialogue = match state.dialogue_defs.get(&npc_db_id).cloned() {
        Some(d) => d,
        None => {
            state.world.remove_one::<InDialogue>(player_entity).ok();
            return;
        }
    };
    let player_quests = state
        .world
        .get::<&PlayerQuests>(player_entity)
        .ok()
        .map(|pq| pq.0.clone())
        .unwrap_or_default();
    let player_level = state
        .world
        .get::<&Stats>(player_entity)
        .ok()
        .map(|s| s.level)
        .unwrap_or(1);

    let node = match dialogue.find_node(&node_id) {
        Some(n) => n,
        None => {
            state.world.remove_one::<InDialogue>(player_entity).ok();
            return;
        }
    };

    // Determine which options are visible (cloned indices).
    let visible_indices: Vec<usize> = node
        .options
        .iter()
        .enumerate()
        .filter(|(_, opt)| {
            check_conditions(
                &opt.conditions,
                &player_quests,
                player_level,
                &state.quest_defs,
            )
        })
        .map(|(i, _)| i)
        .collect();

    let idx = choice.saturating_sub(1);
    let Some(&real_idx) = visible_indices.get(idx) else {
        let max = visible_indices.len();
        send_to_client(
            registry,
            client_id,
            format!("Please enter a number between 1 and {max}."),
        );
        return;
    };

    let option = node.options[real_idx].clone();

    // Apply effects (needs &mut state).
    for effect in &option.effects {
        apply_dialogue_effect(
            state,
            player_entity,
            client_id,
            npc_db_id,
            npc_entity,
            effect,
            registry,
        );
    }

    // Navigate to next node or end conversation.
    match option.goto {
        Some(next_id) => {
            // Re-read updated quest state for the next node's condition checks.
            let updated_quests = state
                .world
                .get::<&PlayerQuests>(player_entity)
                .ok()
                .map(|pq| pq.0.clone())
                .unwrap_or_default();
            let updated_level = state
                .world
                .get::<&Stats>(player_entity)
                .ok()
                .map(|s| s.level)
                .unwrap_or(1);

            let dialogue = match state.dialogue_defs.get(&npc_db_id).cloned() {
                Some(d) => d,
                None => {
                    state.world.remove_one::<InDialogue>(player_entity).ok();
                    return;
                }
            };
            let next_node = match dialogue.find_node(&next_id) {
                Some(n) => n.clone(),
                None => {
                    state.world.remove_one::<InDialogue>(player_entity).ok();
                    return;
                }
            };

            // Update the player's dialogue state.
            if let Ok(mut dlg) = state.world.get::<&mut InDialogue>(player_entity) {
                dlg.node_id = next_id.clone();
            }

            // Show the next node.
            let npc_name = state
                .world
                .get::<&Name>(npc_entity)
                .ok()
                .map(|n| n.0.clone())
                .unwrap_or_default();

            let next_visible: Vec<&crate::dialogue::DialogueOption> = next_node
                .options
                .iter()
                .filter(|opt| {
                    check_conditions(
                        &opt.conditions,
                        &updated_quests,
                        updated_level,
                        &state.quest_defs,
                    )
                })
                .collect();

            send_to_client(
                registry,
                client_id,
                format!("{} says: \"{}\"", npc_name, next_node.text),
            );
            if next_visible.is_empty() {
                state.world.remove_one::<InDialogue>(player_entity).ok();
                send_to_client(registry, client_id, "(The conversation ends.)".to_string());
            } else {
                for (i, opt) in next_visible.iter().enumerate() {
                    send_to_client(registry, client_id, format!("  [{}] {}", i + 1, opt.text));
                }
            }
        }
        None => {
            state.world.remove_one::<InDialogue>(player_entity).ok();
            let npc_name = state
                .world
                .get::<&Name>(npc_entity)
                .ok()
                .map(|n| n.0.clone())
                .unwrap_or_default();
            send_to_client(
                registry,
                client_id,
                format!("You end your conversation with {}.", npc_name),
            );
        }
    }
}

/// Evaluates all conditions for a dialogue option against the player's current state.
fn check_conditions(
    conditions: &[DialogueCondition],
    player_quests: &[PlayerQuestState],
    player_level: i32,
    _quest_defs: &std::collections::HashMap<i64, QuestDef>,
) -> bool {
    conditions.iter().all(|cond| match cond {
        DialogueCondition::QuestNotStarted { quest_id } => {
            !player_quests.iter().any(|q| q.quest_id == *quest_id)
        }
        DialogueCondition::QuestActive { quest_id } => player_quests
            .iter()
            .any(|q| q.quest_id == *quest_id && q.status == QuestStatus::Active),
        DialogueCondition::QuestPhase { quest_id, phase } => player_quests.iter().any(|q| {
            q.quest_id == *quest_id && q.phase == *phase && q.status == QuestStatus::Active
        }),
        DialogueCondition::QuestReady { quest_id } => player_quests
            .iter()
            .any(|q| q.quest_id == *quest_id && q.status == QuestStatus::ReadyToTurnIn),
        DialogueCondition::QuestReadyAtPhase { quest_id, phase } => player_quests.iter().any(|q| {
            q.quest_id == *quest_id && q.phase == *phase && q.status == QuestStatus::ReadyToTurnIn
        }),
        DialogueCondition::QuestComplete { quest_id } => player_quests
            .iter()
            .any(|q| q.quest_id == *quest_id && q.status == QuestStatus::Completed),
        DialogueCondition::MinLevel { level } => player_level >= *level,
    })
}

/// Applies a single dialogue effect. Called per-effect when an option is chosen.
fn apply_dialogue_effect(
    state: &mut GameState,
    player_entity: hecs::Entity,
    client_id: ClientId,
    npc_db_id: i64,
    _npc_entity: hecs::Entity,
    effect: &DialogueEffect,
    registry: &OutputRegistry,
) {
    let char_id = state
        .world
        .get::<&CharacterId>(player_entity)
        .ok()
        .map(|c| c.db_id)
        .unwrap_or(0);

    match effect {
        DialogueEffect::AcceptQuest { quest_id } => {
            // Accept a specific quest (skip if already in log).
            let already_has = state
                .world
                .get::<&PlayerQuests>(player_entity)
                .ok()
                .map(|pq| pq.0.iter().any(|s| s.quest_id == *quest_id))
                .unwrap_or(false);
            if already_has {
                return;
            }
            let quest_def = state.quest_defs.get(quest_id).cloned();
            if let Some(def) = quest_def {
                let num_objs = def.phases.first().map(|p| p.objectives.len()).unwrap_or(0);
                let new_state = PlayerQuestState::new_active(def.id, num_objs);
                {
                    let Ok(mut pq) = state.world.get::<&mut PlayerQuests>(player_entity) else {
                        return;
                    };
                    pq.0.push(new_state.clone());
                }
                state.pending_quest_saves.push(QuestSave {
                    char_id,
                    state: new_state,
                });
                let phase_desc = def
                    .phases
                    .first()
                    .map(|p| p.description.as_str())
                    .unwrap_or("");
                send_to_client(
                    registry,
                    client_id,
                    format!(
                        "<yellow>[Quest Accepted]</yellow> {}\n   {}\n   Objective: {}",
                        def.name, def.description, phase_desc
                    ),
                );
            }
        }
        DialogueEffect::MarkObjective { quest_id } => {
            let qid = *quest_id;
            quest_mark_objective(
                state,
                player_entity,
                client_id,
                registry,
                Some(qid),
                |obj| matches!(obj, QuestObjectiveDef::Talk { npc_id, .. } if *npc_id == npc_db_id),
            );
        }
        DialogueEffect::TurnInQuest => {
            quest_turn_in(state, player_entity, npc_db_id, client_id, registry);
        }
        DialogueEffect::GiveItem { item_id } => {
            let item_entity = {
                let mut q = state.world.query::<(&ItemId, &ItemName)>();
                q.iter()
                    .find(|(_, (id, _))| id.0 == *item_id)
                    .map(|(e, (_, name))| (e, name.0.clone()))
            };
            if let Some((item_entity, item_name)) = item_entity {
                state.world.remove_one::<RoomContents>(item_entity).ok();
                state
                    .world
                    .insert_one(
                        item_entity,
                        InInventory {
                            owner: player_entity,
                        },
                    )
                    .ok();
                state.pending_item_saves.push(ItemLocationSave {
                    item_id: *item_id,
                    location: ItemLocation::Inventory { char_id },
                });
                send_to_client(
                    registry,
                    client_id,
                    format!("<yellow>You receive: {item_name}.</yellow>"),
                );
            }
        }
    }
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
        process_input(&mut state, &mut rx, &reg); // tick 1: connect
        process_input(&mut state, &mut rx, &reg); // tick 2: move
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
        process_input(&mut state, &mut rx, &reg); // tick 1: connect
        process_input(&mut state, &mut rx, &reg); // tick 2: move
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
        process_input(&mut state, &mut rx, &reg); // tick 1: connect(1) + connect(2)
        process_input(&mut state, &mut rx, &reg); // tick 2: say
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
        process_input(&mut state, &mut rx, &reg); // tick 1: connect
        process_input(&mut state, &mut rx, &reg); // tick 2: unknown
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
        process_input(&mut state, &mut rx, &reg); // tick 1: connect
        process_input(&mut state, &mut rx, &reg); // tick 2: score
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
        process_input(&mut state, &mut rx, &reg); // tick 1: connect
        process_input(&mut state, &mut rx, &reg); // tick 2: @who
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
        process_input(&mut state, &mut rx, &reg); // tick 1: connect
        process_input(&mut state, &mut rx, &reg); // tick 2: @who
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
        process_input(&mut state, &mut rx, &reg); // tick 1: equip ring alpha
        process_input(&mut state, &mut rx, &reg); // tick 2: equip ring beta

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
        // One command dispatched per player per tick.
        for _ in 0..4 {
            process_input(&mut state, &mut rx, &reg);
        }

        for (bag, &slot) in bags.iter().zip(expected_slots.iter()) {
            assert_eq!(state.world.get::<&Equipped>(*bag).unwrap().slot, slot);
        }
        drain(&mut out_rx);
    }
}
