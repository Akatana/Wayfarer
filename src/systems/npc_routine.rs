use crate::components::{ClientConnection, Name, NpcId, NpcRoutine, PatrolRoute, Position};
use crate::direction::Direction;
use crate::npc::NpcRoomSave;
use crate::systems::output::{send_to_client, OutputRegistry};
use crate::world::room::RoomRegistry;

/// How many ticks must pass between NPC routine executions.
/// At 200 ms per tick this equals exactly 60 real-world seconds (1 game minute).
pub const NPC_ROUTINE_INTERVAL_TICKS: u64 = 300;

/// Phase 3 of each tick: drives NPC patrol movement and any future behaviours.
///
/// Patrolling NPCs advance to the next room in their route when the interval
/// elapses. Moves are queued into `pending_moves` for async DB persistence
/// between ticks. No `.await` calls are permitted here.
pub fn npc_routine_system(
    world: &mut hecs::World,
    current_tick: u64,
    pending_moves: &mut Vec<NpcRoomSave>,
    registry: &OutputRegistry,
    room_registry: &RoomRegistry,
) {
    // Pass 1: patrolling NPCs — advance route and update Position.
    // Collect (entity, from_room, to_room) for notification after the query drops.
    let mut patrol_moves: Vec<(hecs::Entity, u64, u64)> = Vec::new();
    {
        let mut q = world.query::<(&mut NpcRoutine, &mut Position, &mut PatrolRoute, &NpcId)>();
        for (entity, (routine, pos, patrol, npc_id)) in q.iter() {
            let elapsed = current_tick.saturating_sub(routine.last_action_tick);
            if elapsed < NPC_ROUTINE_INTERVAL_TICKS || patrol.rooms.is_empty() {
                continue;
            }
            routine.last_action_tick = current_tick;
            let next_index = (patrol.index + 1) % patrol.rooms.len();
            patrol.index = next_index;
            let next_room = patrol.rooms[next_index];
            if next_room != pos.room_id {
                let old_room = pos.room_id;
                pos.room_id = next_room;
                pending_moves.push(NpcRoomSave {
                    npc_id: npc_id.0,
                    room_id: next_room,
                });
                patrol_moves.push((entity, old_room, next_room));
            }
        }
    }

    // Notify players in rooms the NPC left and entered.
    for (npc_entity, from_room, to_room) in patrol_moves {
        let npc_name = world
            .get::<&Name>(npc_entity)
            .ok()
            .map(|n| capitalize_first(&n.0))
            .unwrap_or_default();

        let dir = exit_direction(room_registry, from_room, to_room);

        let leave_msg = match dir {
            Some(d) => format!("{} leaves to the {}.", npc_name, d),
            None => format!("{} leaves.", npc_name),
        };
        let arrive_msg = match dir {
            Some(d) => format!("{} arrives from the {}.", npc_name, d.opposite()),
            None => format!("{} arrives.", npc_name),
        };

        let leavers: Vec<u64> = {
            let mut q = world.query::<(&Position, &ClientConnection)>();
            q.iter()
                .filter(|(_, (pos, _))| pos.room_id == from_room)
                .map(|(_, (_, conn))| conn.client_id)
                .collect()
        };
        for cid in leavers {
            send_to_client(registry, cid, leave_msg.clone());
        }

        let arrivals: Vec<u64> = {
            let mut q = world.query::<(&Position, &ClientConnection)>();
            q.iter()
                .filter(|(_, (pos, _))| pos.room_id == to_room)
                .map(|(_, (_, conn))| conn.client_id)
                .collect()
        };
        for cid in arrivals {
            send_to_client(registry, cid, arrive_msg.clone());
        }
    }

    // Pass 2: stationary NPCs (or patrol NPCs whose tick was already reset above).
    // Placeholder for future on-interval behaviours (dialogue cycles, respawns, etc.).
    {
        let mut q = world.query::<(&mut NpcRoutine, &Position)>();
        for (_, (routine, _)) in q.iter() {
            let elapsed = current_tick.saturating_sub(routine.last_action_tick);
            if elapsed >= NPC_ROUTINE_INTERVAL_TICKS {
                routine.last_action_tick = current_tick;
            }
        }
    }
}

fn exit_direction(room_registry: &RoomRegistry, from: u64, to: u64) -> Option<Direction> {
    let room = room_registry.get(from)?;
    room.exits
        .iter()
        .find(|(_, e)| e.destination_room_id == to)
        .map(|(d, _)| *d)
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::NpcRoutine;

    fn no_moves() -> Vec<NpcRoomSave> {
        Vec::new()
    }

    fn no_registry() -> OutputRegistry {
        OutputRegistry::new()
    }

    fn spawn_npc(world: &mut hecs::World, last_action_tick: u64) -> hecs::Entity {
        world.spawn((NpcRoutine { last_action_tick }, Position { room_id: 1 }))
    }

    fn spawn_patrol_npc(
        world: &mut hecs::World,
        last_action_tick: u64,
        rooms: Vec<u64>,
        start_room: u64,
    ) -> hecs::Entity {
        let index = rooms.iter().position(|&r| r == start_room).unwrap_or(0);
        world.spawn((
            NpcId(1),
            NpcRoutine { last_action_tick },
            Position {
                room_id: start_room,
            },
            PatrolRoute { rooms, index },
        ))
    }

    #[test]
    fn npc_does_not_trigger_before_interval() {
        let mut world = hecs::World::new();
        let entity = spawn_npc(&mut world, 0);
        npc_routine_system(
            &mut world,
            NPC_ROUTINE_INTERVAL_TICKS - 1,
            &mut no_moves(),
            &no_registry(),
            &RoomRegistry::new(),
        );
        let routine = world.get::<&NpcRoutine>(entity).unwrap();
        assert_eq!(routine.last_action_tick, 0, "Should not have fired yet");
    }

    #[test]
    fn npc_triggers_exactly_at_interval() {
        let mut world = hecs::World::new();
        let entity = spawn_npc(&mut world, 0);
        npc_routine_system(
            &mut world,
            NPC_ROUTINE_INTERVAL_TICKS,
            &mut no_moves(),
            &no_registry(),
            &RoomRegistry::new(),
        );
        let routine = world.get::<&NpcRoutine>(entity).unwrap();
        assert_eq!(routine.last_action_tick, NPC_ROUTINE_INTERVAL_TICKS);
    }

    #[test]
    fn npc_triggers_past_interval() {
        let mut world = hecs::World::new();
        let entity = spawn_npc(&mut world, 0);
        let late_tick = NPC_ROUTINE_INTERVAL_TICKS + 50;
        npc_routine_system(
            &mut world,
            late_tick,
            &mut no_moves(),
            &no_registry(),
            &RoomRegistry::new(),
        );
        let routine = world.get::<&NpcRoutine>(entity).unwrap();
        assert_eq!(routine.last_action_tick, late_tick);
    }

    #[test]
    fn npcs_with_staggered_start_ticks_trigger_independently() {
        let mut world = hecs::World::new();
        let npc1 = spawn_npc(&mut world, 200);
        let npc2 = spawn_npc(&mut world, 0);
        npc_routine_system(
            &mut world,
            400,
            &mut no_moves(),
            &no_registry(),
            &RoomRegistry::new(),
        );
        let r1 = world.get::<&NpcRoutine>(npc1).unwrap();
        let r2 = world.get::<&NpcRoutine>(npc2).unwrap();
        assert_eq!(
            r1.last_action_tick, 200,
            "npc1: only 200 ticks elapsed, should not fire"
        );
        assert_eq!(
            r2.last_action_tick, 400,
            "npc2: 400 ticks elapsed, should fire"
        );
    }

    #[test]
    fn entity_without_position_is_ignored() {
        let mut world = hecs::World::new();
        world.spawn((NpcRoutine {
            last_action_tick: 0,
        },));
        npc_routine_system(
            &mut world,
            1000,
            &mut no_moves(),
            &no_registry(),
            &RoomRegistry::new(),
        );
    }

    #[test]
    fn empty_world_does_not_panic() {
        let mut world = hecs::World::new();
        npc_routine_system(
            &mut world,
            9999,
            &mut no_moves(),
            &no_registry(),
            &RoomRegistry::new(),
        );
    }

    #[test]
    fn patrol_npc_moves_at_interval() {
        let mut world = hecs::World::new();
        let entity = spawn_patrol_npc(&mut world, 0, vec![1, 2, 3], 1);
        let mut moves = Vec::new();
        npc_routine_system(
            &mut world,
            NPC_ROUTINE_INTERVAL_TICKS,
            &mut moves,
            &no_registry(),
            &RoomRegistry::new(),
        );
        let pos = world.get::<&Position>(entity).unwrap();
        assert_eq!(pos.room_id, 2, "Should have moved to room 2");
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].room_id, 2);
    }

    #[test]
    fn patrol_npc_cycles_back_to_start() {
        let mut world = hecs::World::new();
        let entity = spawn_patrol_npc(&mut world, 0, vec![1, 2], 1);
        let tick = NPC_ROUTINE_INTERVAL_TICKS;
        // First fire: 1 → 2
        let mut moves = Vec::new();
        npc_routine_system(
            &mut world,
            tick,
            &mut moves,
            &no_registry(),
            &RoomRegistry::new(),
        );
        assert_eq!(world.get::<&Position>(entity).unwrap().room_id, 2);
        // Second fire: 2 → 1 (wraps)
        let mut moves2 = Vec::new();
        npc_routine_system(
            &mut world,
            tick * 2,
            &mut moves2,
            &no_registry(),
            &RoomRegistry::new(),
        );
        assert_eq!(world.get::<&Position>(entity).unwrap().room_id, 1);
    }

    #[test]
    fn patrol_npc_does_not_queue_move_when_staying_in_same_room() {
        let mut world = hecs::World::new();
        // Patrol that repeats the same room: [1, 1]
        spawn_patrol_npc(&mut world, 0, vec![1, 1], 1);
        let mut moves = Vec::new();
        npc_routine_system(
            &mut world,
            NPC_ROUTINE_INTERVAL_TICKS,
            &mut moves,
            &no_registry(),
            &RoomRegistry::new(),
        );
        assert!(
            moves.is_empty(),
            "No DB save needed when room doesn't change"
        );
    }
}
