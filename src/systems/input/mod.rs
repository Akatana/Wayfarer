mod admin;
mod items;
mod movement;
mod npcs;
mod world;

use tokio::sync::mpsc;

use crate::command::{ClientId, Command, PlayerInput};
use crate::components::{
    BagCapacity, CharacterId, Equipped, Hostile, InInventory, ItemId, ItemName, Name, NpcId,
    PlayerQuests, Position, RoomContents, Stats, TwoHanded,
};
use crate::game_state::GameState;
use crate::item::{EquipSlot, ItemLocation, ItemLocationSave};
use crate::quest::{PlayerQuestState, QuestObjectiveDef, QuestSave, QuestStatus};
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
        Command::AdminQlist => admin::handle_admin_qlist(state, id, registry),
        Command::AdminQinfo(quest_id) => admin::handle_admin_qinfo(state, id, quest_id, registry),
        Command::AdminQgive(name, quest_id) => {
            admin::handle_admin_qgive(state, id, name, quest_id, registry)
        }
        Command::AdminQreset(name, quest_id) => {
            admin::handle_admin_qreset(state, id, name, quest_id, registry)
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
                if state.world.get::<&Hostile>(e).is_ok() {
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

/// Marks objectives that match `predicate` as done, then checks for phase completion.
/// If all objectives are met and `completion_npc_id` is None, auto-completes the phase.
/// Otherwise sets status to `ReadyToTurnIn`.
fn quest_mark_objective(
    state: &mut GameState,
    entity: hecs::Entity,
    client_id: ClientId,
    registry: &OutputRegistry,
    predicate: impl Fn(&QuestObjectiveDef) -> bool,
) {
    use crate::systems::output::send_to_client;

    // Collect (quest_id, obj_idx) pairs to mark, respecting borrow rules.
    let to_mark: Vec<(i64, usize)> = {
        let Ok(pq) = state.world.get::<&PlayerQuests>(entity) else {
            return;
        };
        pq.0.iter()
            .filter(|s| s.status == QuestStatus::Active)
            .flat_map(|s| {
                let def = state.quest_defs.get(&s.quest_id)?;
                let phase = def.phases.get(s.phase)?;
                Some(
                    phase
                        .objectives
                        .iter()
                        .enumerate()
                        .filter(|(idx, obj)| {
                            !s.objectives_met.get(*idx).copied().unwrap_or(true) && predicate(obj)
                        })
                        .map(|(idx, _)| (s.quest_id, idx))
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .collect()
    };

    if to_mark.is_empty() {
        return;
    }

    let char_id = state
        .world
        .get::<&CharacterId>(entity)
        .ok()
        .map(|c| c.db_id)
        .unwrap_or(0);

    for (quest_id, obj_idx) in to_mark {
        // Mark objective and check completion — do this in a scoped borrow.
        let (all_met, completion_npc_id, obj_description, quest_name) = {
            let Ok(mut pq) = state.world.get::<&mut PlayerQuests>(entity) else {
                continue;
            };
            let Some(qs) = pq.0.iter_mut().find(|s| s.quest_id == quest_id) else {
                continue;
            };
            if let Some(flag) = qs.objectives_met.get_mut(obj_idx) {
                *flag = true;
            }
            let all_met = qs.all_objectives_met();

            let def = state.quest_defs.get(&quest_id);
            let phase = def.and_then(|d| d.phases.get(qs.phase));
            let obj_desc = phase
                .and_then(|p| p.objectives.get(obj_idx))
                .map(|o| o.description().to_string())
                .unwrap_or_default();
            let comp_npc = phase.and_then(|p| p.completion_npc_id);
            let qname = def.map(|d| d.name.clone()).unwrap_or_default();
            (all_met, comp_npc, obj_desc, qname)
        };

        send_to_client(
            registry,
            client_id,
            format!(
                "<yellow>[Quest Update]</yellow> {}: {}",
                quest_name, obj_description
            ),
        );

        if all_met {
            if completion_npc_id.is_none() {
                // Auto-complete — advance or finish.
                quest_advance_phase(state, entity, quest_id, char_id, client_id, registry);
            } else {
                // Require turn-in.
                if let Ok(mut pq) = state.world.get::<&mut PlayerQuests>(entity) {
                    if let Some(qs) = pq.0.iter_mut().find(|s| s.quest_id == quest_id) {
                        qs.status = QuestStatus::ReadyToTurnIn;
                        let save_state = qs.clone();
                        state.pending_quest_saves.push(QuestSave {
                            char_id,
                            state: save_state,
                        });
                    }
                }
                let quest_name = state
                    .quest_defs
                    .get(&quest_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_default();
                send_to_client(
                    registry,
                    client_id,
                    format!(
                        "<yellow>[Quest Ready]</yellow> {} — find the completion NPC to turn in.",
                        quest_name
                    ),
                );
            }
        } else {
            // Save partial progress.
            let save_state: Option<PlayerQuestState> = state
                .world
                .get::<&PlayerQuests>(entity)
                .ok()
                .and_then(|pq| pq.0.iter().find(|s| s.quest_id == quest_id).cloned());
            if let Some(s) = save_state {
                state
                    .pending_quest_saves
                    .push(QuestSave { char_id, state: s });
            }
        }
    }
}

/// Advances a quest to the next phase or marks it Completed.
/// Awards XP, LP, and items for the completed phase. Sends feedback to the player.
fn quest_advance_phase(
    state: &mut GameState,
    entity: hecs::Entity,
    quest_id: i64,
    char_id: i64,
    client_id: ClientId,
    registry: &OutputRegistry,
) {
    use crate::systems::output::send_to_client;

    let (
        current_phase,
        xp_reward,
        lp_reward,
        copper_reward,
        item_reward_ids,
        completion_text,
        quest_name,
        num_phases,
        next_desc,
        next_num_objs,
    ) = {
        let Ok(pq) = state.world.get::<&PlayerQuests>(entity) else {
            return;
        };
        let Some(qs) = pq.0.iter().find(|s| s.quest_id == quest_id) else {
            return;
        };
        let Some(def) = state.quest_defs.get(&quest_id) else {
            return;
        };
        let phase_def = &def.phases[qs.phase];
        let next = def.phases.get(qs.phase + 1);
        (
            qs.phase,
            phase_def.xp_reward,
            phase_def.lp_reward,
            phase_def.copper_reward,
            phase_def.item_rewards.clone(),
            phase_def.completion_text.clone(),
            def.name.clone(),
            def.phases.len(),
            next.map(|p| p.description.clone()),
            next.map(|p| p.objectives.len()).unwrap_or(0),
        )
    };

    let is_last = current_phase + 1 >= num_phases;

    // Grant XP and LP.
    if xp_reward > 0 || lp_reward > 0 {
        if let Ok(mut stats) = state.world.get::<&mut Stats>(entity) {
            if xp_reward > 0 {
                let gained = stats.add_experience(xp_reward);
                if gained > 0 {
                    let level = stats.level;
                    send_to_client(
                        registry,
                        client_id,
                        format!(
                            "<yellow>You feel more powerful! You are now level {level}.</yellow>"
                        ),
                    );
                }
            }
            if lp_reward > 0 {
                stats.learning_points += lp_reward;
            }
        }
    }

    // Grant copper.
    if copper_reward > 0 {
        if let Ok(mut wallet) = state.world.get::<&mut crate::components::Wallet>(entity) {
            wallet.0 += copper_reward;
        }
    }

    // Transfer item rewards to the player's inventory.
    let mut rewarded_item_names: Vec<String> = Vec::new();
    for item_id in item_reward_ids {
        let item_entity = {
            let mut q = state.world.query::<(&ItemId, &ItemName)>();
            q.iter()
                .find(|(_, (id, _))| id.0 == item_id)
                .map(|(e, (_, name))| (e, name.0.clone()))
        };
        let Some((item_entity, item_name)) = item_entity else {
            continue;
        };
        // Remove from room if present, add to inventory.
        state.world.remove_one::<RoomContents>(item_entity).ok();
        state
            .world
            .insert_one(item_entity, InInventory { owner: entity })
            .ok();
        state.pending_item_saves.push(ItemLocationSave {
            item_id,
            location: ItemLocation::Inventory { char_id },
        });
        send_to_client(
            registry,
            client_id,
            format!("<yellow>You receive: {item_name}.</yellow>"),
        );
        rewarded_item_names.push(item_name);
    }

    // Emit completion text, then update quest state.
    if !completion_text.is_empty() {
        send_to_client(registry, client_id, completion_text);
    }

    let new_state: PlayerQuestState = {
        let Ok(mut pq) = state.world.get::<&mut PlayerQuests>(entity) else {
            return;
        };
        let Some(qs) = pq.0.iter_mut().find(|s| s.quest_id == quest_id) else {
            return;
        };
        if is_last {
            qs.status = QuestStatus::Completed;
        } else {
            qs.phase = current_phase + 1;
            qs.objectives_met = vec![false; next_num_objs];
            qs.status = QuestStatus::Active;
        }
        qs.clone()
    };

    state.pending_quest_saves.push(QuestSave {
        char_id,
        state: new_state,
    });

    // Build reward summary for the completion line.
    let mut reward_parts: Vec<String> = Vec::new();
    if xp_reward > 0 {
        reward_parts.push(format!("+{xp_reward} XP"));
    }
    if lp_reward > 0 {
        reward_parts.push(format!("+{lp_reward} LP"));
    }
    if copper_reward > 0 {
        reward_parts.push(format!(
            "+{}",
            crate::currency::format_copper(copper_reward)
        ));
    }
    let reward_suffix = if reward_parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", reward_parts.join(", "))
    };

    if is_last {
        send_to_client(
            registry,
            client_id,
            format!("<yellow>[Quest Complete]</yellow> {quest_name}{reward_suffix}"),
        );
    } else {
        send_to_client(
            registry,
            client_id,
            format!(
                "<yellow>[Quest Updated]</yellow> {quest_name} — Phase {}: {}{}",
                current_phase + 2,
                next_desc.unwrap_or_default(),
                reward_suffix
            ),
        );
    }
}

/// Auto-accepts all quests offered by `npc_db_id` that the player doesn't already have.
fn quest_accept_from_npc(
    state: &mut GameState,
    entity: hecs::Entity,
    npc_db_id: i64,
    client_id: ClientId,
    registry: &OutputRegistry,
) {
    use crate::systems::output::send_to_client;

    let current_ids: Vec<i64> = state
        .world
        .get::<&PlayerQuests>(entity)
        .ok()
        .map(|pq| pq.0.iter().map(|s| s.quest_id).collect())
        .unwrap_or_default();

    let new_quests: Vec<crate::quest::QuestDef> = state
        .quest_defs
        .values()
        .filter(|def| def.giver_npc_id == Some(npc_db_id) && !current_ids.contains(&def.id))
        .cloned()
        .collect();

    let char_id = state
        .world
        .get::<&CharacterId>(entity)
        .ok()
        .map(|c| c.db_id)
        .unwrap_or(0);

    for def in new_quests {
        let num_objs = def.phases.first().map(|p| p.objectives.len()).unwrap_or(0);
        let new_state = PlayerQuestState::new_active(def.id, num_objs);

        {
            let Ok(mut pq) = state.world.get::<&mut PlayerQuests>(entity) else {
                continue;
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

/// Auto-accepts all quests triggered by examining `item_db_id`.
fn quest_accept_from_item(
    state: &mut GameState,
    entity: hecs::Entity,
    item_db_id: i64,
    client_id: ClientId,
    registry: &OutputRegistry,
) {
    use crate::systems::output::send_to_client;

    let current_ids: Vec<i64> = state
        .world
        .get::<&PlayerQuests>(entity)
        .ok()
        .map(|pq| pq.0.iter().map(|s| s.quest_id).collect())
        .unwrap_or_default();

    let new_quests: Vec<crate::quest::QuestDef> = state
        .quest_defs
        .values()
        .filter(|def| def.giver_item_id == Some(item_db_id) && !current_ids.contains(&def.id))
        .cloned()
        .collect();

    let char_id = state
        .world
        .get::<&CharacterId>(entity)
        .ok()
        .map(|c| c.db_id)
        .unwrap_or(0);

    for def in new_quests {
        let num_objs = def.phases.first().map(|p| p.objectives.len()).unwrap_or(0);
        let new_state = PlayerQuestState::new_active(def.id, num_objs);

        {
            let Ok(mut pq) = state.world.get::<&mut PlayerQuests>(entity) else {
                continue;
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
                "<yellow>[Quest Started]</yellow> {}\n   {}\n   Objective: {}",
                def.name, def.description, phase_desc
            ),
        );
    }
}

/// Turns in ready quests whose current phase `completion_npc_id` matches `npc_db_id`.
fn quest_turn_in(
    state: &mut GameState,
    entity: hecs::Entity,
    npc_db_id: i64,
    client_id: ClientId,
    registry: &OutputRegistry,
) {
    let char_id = state
        .world
        .get::<&CharacterId>(entity)
        .ok()
        .map(|c| c.db_id)
        .unwrap_or(0);

    let ready_ids: Vec<i64> = {
        let Ok(pq) = state.world.get::<&PlayerQuests>(entity) else {
            return;
        };
        pq.0.iter()
            .filter(|s| s.status == QuestStatus::ReadyToTurnIn)
            .filter(|s| {
                state
                    .quest_defs
                    .get(&s.quest_id)
                    .and_then(|def| def.phases.get(s.phase))
                    .and_then(|p| p.completion_npc_id)
                    == Some(npc_db_id)
            })
            .map(|s| s.quest_id)
            .collect()
    };

    for quest_id in ready_ids {
        quest_advance_phase(state, entity, quest_id, char_id, client_id, registry);
    }
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
