use std::collections::HashSet;

use crate::components::{
    BagCapacity, ClientConnection, Equipped, Hostile, InCombat, ItemDescription, ItemId, ItemName,
    ItemSlot, Name, NpcCombatStats, NpcId, NpcLootTable, Passive, Position, RoomContents, Stats,
    TwoHanded,
};
use crate::game_state::{AdminDbOp, GameState};
use crate::item::{EquipSlot, ItemBonuses};
use crate::item::{ItemData, ItemLocation};
use crate::npc::NpcRespawn;
use crate::systems::output::{send_to_client, OutputRegistry};
use crate::world::seed::{spawn_single_npc, STARTING_ROOM_ID};

/// Ticks before a dead NPC respawns (300 ticks × 200 ms = 60 s).
pub const NPC_RESPAWN_TICKS: u64 = 300;

/// Player attack interval formula: base 10 ticks, reduced by DEX/5, minimum 3.
pub fn player_attack_interval(dexterity: i32) -> u64 {
    10u64.saturating_sub((dexterity / 5) as u64).max(3)
}

/// Returns `true` with probability `chance` (0.0–1.0) using a lightweight LCG.
fn roll_chance(seed: u64, chance: f32) -> bool {
    if chance >= 1.0 {
        return true;
    }
    if chance <= 0.0 {
        return false;
    }
    let h = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    (h >> 32) % 10_000 < (chance * 10_000.0) as u64
}

/// Deterministic pseudo-random value in `[min, max)` using a lightweight LCG.
fn roll(seed: u64, min: i32, max: i32) -> i32 {
    if max <= min {
        return min;
    }
    let h = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    min + ((h >> 33) % (max - min) as u64) as i32
}

/// Phase 4 of each tick: drive all active combats, handle deaths,
/// auto-aggro hostile NPCs, and process pending NPC respawns.
/// No `.await` calls permitted here.
pub fn combat_system(state: &mut GameState, registry: &OutputRegistry) {
    let tick = state.current_tick;

    // ── Pre-pass: collect gear bonuses for combat calculations ───────────────
    // weapon_damage: player entity → (min_dmg, max_dmg) from equipped LeftHand item
    let weapon_damage: std::collections::HashMap<hecs::Entity, (i32, i32)> = {
        let mut map = std::collections::HashMap::new();
        let mut q = state.world.query::<(&Equipped, &ItemBonuses)>();
        for (_, (eq, b)) in q.iter() {
            if eq.slot == EquipSlot::LeftHand {
                map.insert(eq.owner, (b.bonus_min_damage, b.bonus_max_damage));
            }
        }
        map
    };

    // player_armor: player entity → total flat damage reduction from all equipped items
    let player_armor: std::collections::HashMap<hecs::Entity, i32> = {
        let mut map: std::collections::HashMap<hecs::Entity, i32> =
            std::collections::HashMap::new();
        let mut q = state.world.query::<(&Equipped, &ItemBonuses)>();
        for (_, (eq, b)) in q.iter() {
            if b.bonus_armor > 0 {
                *map.entry(eq.owner).or_insert(0) += b.bonus_armor;
            }
        }
        map
    };

    // ── Pass 1: collect attack events ────────────────────────────────────────
    // (attacker, target, damage, is_player_attacker)
    struct AttackEvent {
        attacker: hecs::Entity,
        target: hecs::Entity,
        damage: i32,
        is_player_attacker: bool,
    }

    let attacks: Vec<AttackEvent> = {
        let mut result = Vec::new();
        let mut q = state
            .world
            .query::<(&InCombat, Option<&Stats>, Option<&NpcCombatStats>)>();
        for (attacker, (combat, stats, npc_stats)) in q.iter() {
            if !combat.attacking {
                continue;
            }
            let elapsed = tick.saturating_sub(combat.last_attack_tick);
            if elapsed < combat.attack_interval {
                continue;
            }
            let (damage, is_player) = match (stats, npc_stats) {
                (Some(s), _) => {
                    // Use weapon damage range if a weapon with bonuses is equipped,
                    // otherwise fall back to barehanded (1-5).
                    let (wmin, wmax) = weapon_damage
                        .get(&attacker)
                        .copied()
                        .map(|(mn, mx)| (mn.max(1), mx.max(1)))
                        .unwrap_or((1, 5));
                    let base = (s.strength / 2).max(1);
                    let rnd = roll(tick ^ u64::from(attacker.to_bits()), wmin, wmax + 1);
                    (base + rnd, true)
                }
                (_, Some(ns)) => {
                    let raw = roll(
                        tick ^ u64::from(attacker.to_bits()),
                        ns.min_damage,
                        ns.max_damage + 1,
                    );
                    // Apply target player's armor rating.
                    let armor = player_armor.get(&combat.target).copied().unwrap_or(0);
                    let d = (raw - armor).max(1);
                    (d, false)
                }
                _ => continue,
            };
            result.push(AttackEvent {
                attacker,
                target: combat.target,
                damage,
                is_player_attacker: is_player,
            });
        }
        result
    };

    // ── Pass 2: apply damage, update timers, collect deaths ──────────────────
    let mut dead: HashSet<hecs::Entity> = HashSet::new();
    // Maps dead entity → its killer
    let mut kills: Vec<(hecs::Entity, hecs::Entity, bool)> = Vec::new();

    for ev in &attacks {
        // Update attacker's last_attack_tick.
        if let Ok(mut c) = state.world.get::<&mut InCombat>(ev.attacker) {
            c.last_attack_tick = tick;
        }

        // Apply damage to target.
        let target_died = if ev.is_player_attacker {
            if let Ok(mut ns) = state.world.get::<&mut NpcCombatStats>(ev.target) {
                ns.hp -= ev.damage;
                ns.hp <= 0
            } else {
                continue;
            }
        } else {
            if let Ok(mut s) = state.world.get::<&mut Stats>(ev.target) {
                s.hp -= ev.damage;
                s.hp <= 0
            } else {
                continue;
            }
        };

        // Send hit message to the player involved.
        let player_entity = if ev.is_player_attacker {
            ev.attacker
        } else {
            ev.target
        };
        let other_name = if ev.is_player_attacker {
            state
                .world
                .get::<&Name>(ev.target)
                .ok()
                .map(|n| n.0.clone())
                .unwrap_or_default()
        } else {
            state
                .world
                .get::<&Name>(ev.attacker)
                .ok()
                .map(|n| n.0.clone())
                .unwrap_or_default()
        };

        let client_id = state
            .world
            .get::<&ClientConnection>(player_entity)
            .ok()
            .map(|conn| conn.client_id);
        if let Some(cid) = client_id {
            let msg = if ev.is_player_attacker {
                format!("You hit {} for {} damage.", other_name, ev.damage)
            } else {
                format!("{} hits you for {} damage.", other_name, ev.damage)
            };
            send_to_client(registry, cid, msg);

            let npc_entity = if ev.is_player_attacker {
                ev.target
            } else {
                ev.attacker
            };
            let player_hp = state
                .world
                .get::<&Stats>(player_entity)
                .map(|s| format!("{}/{}", s.hp, s.max_hp))
                .unwrap_or_default();
            let npc_hp = state
                .world
                .get::<&NpcCombatStats>(npc_entity)
                .ok()
                .map(|ns| format!("{}/{}", ns.hp.max(0), ns.max_hp));
            if let Some(nh) = npc_hp {
                send_to_client(
                    registry,
                    cid,
                    format!("[Your HP: {}  |  {} HP: {}]", player_hp, other_name, nh),
                );
            }
        }

        if target_died && dead.insert(ev.target) {
            kills.push((ev.target, ev.attacker, ev.is_player_attacker));
        }
    }

    // ── Pass 3: handle deaths ────────────────────────────────────────────────
    for (dead_entity, killer, killer_is_player) in kills {
        if killer_is_player {
            // NPC died.
            let xp = state
                .world
                .get::<&NpcCombatStats>(dead_entity)
                .ok()
                .map(|ns| ns.xp_reward)
                .unwrap_or(0);
            let npc_name = state
                .world
                .get::<&Name>(dead_entity)
                .ok()
                .map(|n| n.0.clone())
                .unwrap_or_else(|| "the creature".to_string());
            let room_id = state
                .world
                .get::<&Position>(dead_entity)
                .ok()
                .map(|p| p.room_id);

            // Collect loot entries before the entity is despawned.
            let loot: Vec<crate::npc::LootEntry> = state
                .world
                .get::<&NpcLootTable>(dead_entity)
                .ok()
                .map(|lt| lt.0.clone())
                .unwrap_or_default();

            // Award XP and notify the killer.
            if let Ok(mut stats) = state.world.get::<&mut Stats>(killer) {
                let levels = stats.add_experience(xp);
                if let Ok(conn) = state.world.get::<&ClientConnection>(killer) {
                    send_to_client(
                        registry,
                        conn.client_id,
                        format!("You have slain {}! (+{} XP)", npc_name, xp),
                    );
                    if levels > 0 {
                        let lvl = stats.level;
                        send_to_client(
                            registry,
                            conn.client_id,
                            format!(
                                "<yellow>You have gained {} level{}! You are now level {}.</yellow>",
                                levels,
                                if levels == 1 { "" } else { "s" },
                                lvl
                            ),
                        );
                    }
                }
            }

            // Notify others in the room.
            if let Some(rid) = room_id {
                let watchers: Vec<(u64, String)> = {
                    let mut q = state.world.query::<(&Position, &ClientConnection, &Name)>();
                    q.iter()
                        .filter(|(e, (p, _, _))| *e != killer && p.room_id == rid)
                        .map(|(_, (_, conn, name))| (conn.client_id, name.0.clone()))
                        .collect()
                };
                let killer_name = state
                    .world
                    .get::<&Name>(killer)
                    .ok()
                    .map(|n| n.0.clone())
                    .unwrap_or_default();
                for (cid, _) in watchers {
                    send_to_client(
                        registry,
                        cid,
                        format!("{} has slain {}!", killer_name, npc_name),
                    );
                }
            }

            // Remove combat from the killer.
            state.world.remove_one::<InCombat>(killer).ok();

            // Queue respawn.
            if let Some(npc_id) = state.world.get::<&NpcId>(dead_entity).ok().map(|n| n.0) {
                let npc_data = build_respawn_data(&state.world, dead_entity, npc_id);
                state.pending_respawns.push(NpcRespawn {
                    data: npc_data,
                    respawn_at_tick: tick + NPC_RESPAWN_TICKS,
                });
            }

            state.world.despawn(dead_entity).ok();

            // Spawn loot copies in the kill room.
            if let Some(rid) = room_id {
                for (i, entry) in loot.iter().enumerate() {
                    let loot_seed = tick
                        ^ u64::from(dead_entity.to_bits())
                        ^ (i as u64).wrapping_mul(2_654_435_761);
                    if !roll_chance(loot_seed, entry.chance) {
                        continue;
                    }
                    let Some(tmpl) = state.item_templates.get(&entry.item_id).cloned() else {
                        continue;
                    };
                    let instance_id = state.next_item_id;
                    state.next_item_id += 1;
                    let equip_slot = tmpl
                        .equip_slot
                        .as_deref()
                        .and_then(crate::item::EquipSlot::parse);
                    let item_data = ItemData {
                        id: instance_id,
                        def_id: entry.item_id,
                        name: tmpl.name.clone(),
                        description: tmpl.description.clone(),
                        equip_slot,
                        two_handed: tmpl.two_handed,
                        bag_capacity: tmpl.bag_capacity,
                        requirements: tmpl.requirements,
                        bonuses: tmpl.bonuses,
                        location: ItemLocation::Room(rid),
                    };
                    let mut builder = hecs::EntityBuilder::new();
                    builder.add(ItemId(instance_id));
                    builder.add(ItemName(tmpl.name.clone()));
                    builder.add(ItemDescription(tmpl.description.clone()));
                    builder.add(RoomContents { room_id: rid });
                    if let Some(slot) = equip_slot {
                        builder.add(ItemSlot(slot));
                    }
                    if tmpl.two_handed {
                        builder.add(TwoHanded);
                    }
                    if let Some(cap) = tmpl.bag_capacity {
                        builder.add(BagCapacity(cap));
                    }
                    if tmpl.requirements.has_any() {
                        builder.add(tmpl.requirements);
                    }
                    if tmpl.bonuses.has_any() {
                        builder.add(tmpl.bonuses);
                    }
                    state.world.spawn(builder.build());
                    state
                        .pending_admin_ops
                        .push(AdminDbOp::CreateItem(item_data));

                    let drop_msg = format!("{} falls to the ground.", capitalize_first(&tmpl.name));
                    let players_in_room: Vec<u64> = {
                        let mut q = state.world.query::<(&Position, &ClientConnection)>();
                        q.iter()
                            .filter(|(_, (p, _))| p.room_id == rid)
                            .map(|(_, (_, conn))| conn.client_id)
                            .collect()
                    };
                    for cid in players_in_room {
                        send_to_client(registry, cid, drop_msg.clone());
                    }
                }
            }
        } else {
            // Player died.
            let npc_name = state
                .world
                .get::<&Name>(killer)
                .ok()
                .map(|n| n.0.clone())
                .unwrap_or_default();

            // Stop the NPC's combat.
            state.world.remove_one::<InCombat>(killer).ok();

            // Restore player to half HP and teleport to start.
            if let Ok(mut s) = state.world.get::<&mut Stats>(dead_entity) {
                s.hp = (s.max_hp / 2).max(1);
            }
            if let Ok(mut pos) = state.world.get::<&mut Position>(dead_entity) {
                pos.room_id = STARTING_ROOM_ID;
            }
            state.world.remove_one::<InCombat>(dead_entity).ok();

            if let Ok(conn) = state.world.get::<&ClientConnection>(dead_entity) {
                send_to_client(
                    registry,
                    conn.client_id,
                    format!(
                        "<red>You have been slain by {}!</red> You wake up back in town, battered but alive.",
                        npc_name
                    ),
                );
            }
        }
    }

    // ── Pass 4: auto-aggro ───────────────────────────────────────────────────
    // Hostile NPCs not in combat attack any player in their room.
    let hostile_idle: Vec<(hecs::Entity, u64)> = {
        let mut q = state
            .world
            .query::<(&Position, &Hostile, &NpcCombatStats)>();
        q.iter().map(|(e, (pos, _, _))| (e, pos.room_id)).collect()
    };
    let hostile_idle: Vec<(hecs::Entity, u64)> = hostile_idle
        .into_iter()
        .filter(|(e, _)| state.world.get::<&InCombat>(*e).is_err())
        .collect();

    let players_in_rooms: Vec<(hecs::Entity, u64)> = {
        let mut q = state
            .world
            .query::<(&Position, &ClientConnection, &Stats)>();
        q.iter().map(|(e, (pos, _, _))| (e, pos.room_id)).collect()
    };

    for (npc, npc_room) in &hostile_idle {
        let npc = *npc;
        let npc_room = *npc_room;
        for &(player, player_room) in &players_in_rooms {
            if player_room != npc_room {
                continue;
            }
            if state.world.get::<&InCombat>(player).is_ok() {
                continue;
            }

            let npc_interval = state
                .world
                .get::<&NpcCombatStats>(npc)
                .ok()
                .map(|ns| ns.attack_ticks)
                .unwrap_or(10);
            let player_interval = state
                .world
                .get::<&Stats>(player)
                .ok()
                .map(|s| player_attack_interval(s.dexterity))
                .unwrap_or(10);
            let npc_name = state
                .world
                .get::<&Name>(npc)
                .ok()
                .map(|n| n.0.clone())
                .unwrap_or_default();

            state
                .world
                .insert_one(
                    npc,
                    InCombat {
                        target: player,
                        last_attack_tick: tick.saturating_sub(npc_interval),
                        attack_interval: npc_interval,
                        attacking: true,
                    },
                )
                .ok();
            // Player is tracked as in-combat (so flee works) but not attacking yet.
            state
                .world
                .insert_one(
                    player,
                    InCombat {
                        target: npc,
                        last_attack_tick: tick,
                        attack_interval: player_interval,
                        attacking: false,
                    },
                )
                .ok();

            if let Ok(conn) = state.world.get::<&ClientConnection>(player) {
                send_to_client(
                    registry,
                    conn.client_id,
                    format!(
                        "<red>{} attacks you! You fight back!</red> (type 'flee' to run)",
                        npc_name
                    ),
                );
            }
            break; // one player per NPC per tick
        }
    }

    // ── Pass 5: respawns ─────────────────────────────────────────────────────
    let mut remaining = Vec::new();
    let mut due = Vec::new();
    for r in state.pending_respawns.drain(..) {
        if r.respawn_at_tick <= tick {
            due.push(r);
        } else {
            remaining.push(r);
        }
    }
    state.pending_respawns = remaining;

    for r in due {
        let room_id = r.data.room_id;
        let display_name = capitalize_first(&r.data.name);
        spawn_single_npc(&mut state.world, &r.data);
        let watchers: Vec<u64> = {
            let mut q = state.world.query::<(&Position, &ClientConnection)>();
            q.iter()
                .filter(|(_, (pos, _))| pos.room_id == room_id)
                .map(|(_, (_, conn))| conn.client_id)
                .collect()
        };
        for cid in watchers {
            send_to_client(registry, cid, format!("{} appears!", display_name));
        }
    }
}

/// Removes `InCombat` from `entity` and from any entity targeting it.
/// Called by the movement and quit handlers.
pub fn clear_combat(world: &mut hecs::World, entity: hecs::Entity) {
    world.remove_one::<InCombat>(entity).ok();
    let targeting: Vec<hecs::Entity> = {
        let mut q = world.query::<(&InCombat,)>();
        q.iter()
            .filter(|(_, (c,))| c.target == entity)
            .map(|(e, _)| e)
            .collect()
    };
    for e in targeting {
        world.remove_one::<InCombat>(e).ok();
    }
}

/// Reconstructs a minimal `NpcData` from an NPC entity's ECS components
/// so it can be stored in `pending_respawns`.
fn build_respawn_data(
    world: &hecs::World,
    entity: hecs::Entity,
    npc_id: i64,
) -> crate::npc::NpcData {
    use crate::components::{NpcDescription, NpcGreeting, PatrolRoute};

    let name = world
        .get::<&Name>(entity)
        .ok()
        .map(|n| n.0.clone())
        .unwrap_or_default();
    let description = world
        .get::<&NpcDescription>(entity)
        .ok()
        .map(|d| d.0.clone())
        .unwrap_or_default();
    let greeting = world.get::<&NpcGreeting>(entity).ok().map(|g| g.0.clone());
    let hostile = world.get::<&Hostile>(entity).is_ok();
    let passive = world.get::<&Passive>(entity).is_ok();
    let patrol = world
        .get::<&PatrolRoute>(entity)
        .ok()
        .map(|pr| pr.rooms.clone())
        .unwrap_or_default();
    // Respawn at the start of the patrol route so the NPC resets to its home position.
    let room_id = if !patrol.is_empty() {
        patrol[0]
    } else {
        world
            .get::<&Position>(entity)
            .ok()
            .map(|p| p.room_id)
            .unwrap_or(STARTING_ROOM_ID)
    };
    let cs = world.get::<&NpcCombatStats>(entity);
    let (max_hp, min_damage, max_damage, attack_ticks, xp_reward) = cs
        .as_ref()
        .map(|ns| {
            (
                ns.max_hp,
                ns.min_damage,
                ns.max_damage,
                ns.attack_ticks,
                ns.xp_reward,
            )
        })
        .unwrap_or((20, 1, 4, 10, 10));
    let loot_table = world
        .get::<&NpcLootTable>(entity)
        .ok()
        .map(|lt| lt.0.clone())
        .unwrap_or_default();

    crate::npc::NpcData {
        id: npc_id,
        name,
        description,
        greeting,
        hostile,
        passive,
        room_id,
        patrol,
        max_hp,
        min_damage,
        max_damage,
        attack_ticks,
        xp_reward,
        loot_table,
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

/// Sends the current HP bar for an entity in combat.
/// Used by the `handle_attack` command to give immediate feedback.
pub fn send_combat_status(
    world: &hecs::World,
    registry: &OutputRegistry,
    player: hecs::Entity,
    npc: hecs::Entity,
) {
    let player_hp = world
        .get::<&Stats>(player)
        .map(|s| format!("{}/{}", s.hp, s.max_hp))
        .unwrap_or_default();
    let npc_name = world
        .get::<&Name>(npc)
        .ok()
        .map(|n| n.0.clone())
        .unwrap_or_default();
    let npc_hp_pct = world.get::<&NpcCombatStats>(npc).map(|ns| {
        let pct = (ns.hp * 100 / ns.max_hp.max(1)).clamp(0, 100);
        match pct {
            76..=100 => "looks healthy",
            51..=75 => "is lightly wounded",
            26..=50 => "is badly wounded",
            1..=25 => "is near death",
            _ => "is dead",
        }
    });

    if let (Ok(npc_hp_pct), Ok(conn)) = (npc_hp_pct, world.get::<&ClientConnection>(player)) {
        send_to_client(
            registry,
            conn.client_id,
            format!(
                "You begin fighting {}. (Your HP: {}  |  {} {})",
                npc_name, player_hp, npc_name, npc_hp_pct
            ),
        );
    }
}
