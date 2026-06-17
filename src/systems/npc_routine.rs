use crate::components::{NpcRoutine, Position};

/// How many ticks must pass between NPC routine executions.
/// At 200 ms per tick this equals exactly 60 real-world seconds (1 game minute).
pub const NPC_ROUTINE_INTERVAL_TICKS: u64 = 300;

/// Phase 3 of each tick: iterates every entity that has both an `NpcRoutine`
/// and a `Position` component. When `NPC_ROUTINE_INTERVAL_TICKS` have elapsed
/// since the NPC last acted, its scheduled behaviour fires and its timestamp
/// is updated to the current tick.
///
/// No `.await` calls are permitted here — all work must complete synchronously.
pub fn npc_routine_system(world: &mut hecs::World, current_tick: u64) {
    for (_entity, (routine, _position)) in world.query_mut::<(&mut NpcRoutine, &Position)>() {
        let elapsed = current_tick.saturating_sub(routine.last_action_tick);

        if elapsed >= NPC_ROUTINE_INTERVAL_TICKS {
            // Execute scheduled NPC behaviour.
            // Future implementations will pattern-match on additional components
            // such as `PatrolRoute`, `VendorInventory`, or `DialogueCycle` to
            // drive more complex AI without branching this function.
            routine.last_action_tick = current_tick;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_npc(world: &mut hecs::World, last_action_tick: u64) -> hecs::Entity {
        world.spawn((NpcRoutine { last_action_tick }, Position { room_id: 1 }))
    }

    #[test]
    fn npc_does_not_trigger_before_interval() {
        let mut world = hecs::World::new();
        let entity = spawn_npc(&mut world, 0);

        npc_routine_system(&mut world, NPC_ROUTINE_INTERVAL_TICKS - 1);

        let routine = world.get::<&NpcRoutine>(entity).unwrap();
        assert_eq!(routine.last_action_tick, 0, "Should not have fired yet");
    }

    #[test]
    fn npc_triggers_exactly_at_interval() {
        let mut world = hecs::World::new();
        let entity = spawn_npc(&mut world, 0);

        npc_routine_system(&mut world, NPC_ROUTINE_INTERVAL_TICKS);

        let routine = world.get::<&NpcRoutine>(entity).unwrap();
        assert_eq!(
            routine.last_action_tick, NPC_ROUTINE_INTERVAL_TICKS,
            "Should have updated last_action_tick"
        );
    }

    #[test]
    fn npc_triggers_past_interval() {
        let mut world = hecs::World::new();
        let entity = spawn_npc(&mut world, 0);
        let late_tick = NPC_ROUTINE_INTERVAL_TICKS + 50;

        npc_routine_system(&mut world, late_tick);

        let routine = world.get::<&NpcRoutine>(entity).unwrap();
        assert_eq!(routine.last_action_tick, late_tick);
    }

    #[test]
    fn npcs_with_staggered_start_ticks_trigger_independently() {
        let mut world = hecs::World::new();

        // npc1 acted at tick 200 — only 200 ticks will have elapsed at tick 400.
        let npc1 = spawn_npc(&mut world, 200);
        // npc2 acted at tick 0 — 400 ticks will have elapsed.
        let npc2 = spawn_npc(&mut world, 0);

        npc_routine_system(&mut world, 400);

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
        // Spawn an NpcRoutine with no Position — should not be queried.
        world.spawn((NpcRoutine {
            last_action_tick: 0,
        },));

        // Must not panic.
        npc_routine_system(&mut world, 1000);
    }

    #[test]
    fn empty_world_does_not_panic() {
        let mut world = hecs::World::new();
        npc_routine_system(&mut world, 9999);
    }
}
