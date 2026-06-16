use std::collections::HashMap;

use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use crate::command::{ClientId, PlayerInput};
use crate::game_state::GameState;
use crate::systems::{input, npc_routine};

/// Fixed tick duration: 200 ms → 5 ticks per real-world second.
pub const TICK_DURATION_MS: u64 = 200;

/// Entry point for the game loop task.
///
/// # Channel contracts
/// - `command_rx`: `PlayerInput` from the network layer (many writers → one reader).
/// - `register_rx`: per-client `(ClientId, Sender<String>)` arriving when a new
///   session starts. Registered before the session sends its first command.
/// - `deregister_rx`: `ClientId` arriving when a session disconnects.
/// - `db`: SeaORM connection used only for async save tasks spawned between ticks.
pub async fn run(
    mut command_rx: mpsc::Receiver<PlayerInput>,
    mut register_rx: mpsc::Receiver<(ClientId, mpsc::Sender<String>)>,
    mut deregister_rx: mpsc::Receiver<ClientId>,
    db: sea_orm::DatabaseConnection,
) {
    let room_registry = crate::db::room::load_or_seed(&db)
        .await
        .expect("[GameLoop] Failed to load world from database");

    let mut state = GameState::with_rooms(room_registry);
    let mut output_registry: HashMap<ClientId, mpsc::Sender<String>> = HashMap::new();
    let mut ticker = interval(Duration::from_millis(TICK_DURATION_MS));

    println!("[GameLoop] Started — {TICK_DURATION_MS}ms per tick (5 TPS).");

    loop {
        ticker.tick().await;

        // ── Between-tick async work ───────────────────────────────────────────

        // Register / deregister client output channels.
        while let Ok((id, tx)) = register_rx.try_recv() {
            output_registry.insert(id, tx);
        }
        while let Ok(id) = deregister_rx.try_recv() {
            output_registry.remove(&id);
        }

        // Drain character saves queued by the previous tick's quit/save handlers.
        // Each save is a fire-and-forget async task — the tick never blocks on I/O.
        for char_data in state.pending_saves.drain(..) {
            let db = db.clone();
            tokio::spawn(async move {
                if let Err(e) = crate::db::character::save(&db, char_data).await {
                    eprintln!("[DB] Character save failed: {e}");
                }
            });
        }

        // ── Tick execution (synchronous — no .await) ──────────────────────────

        state.advance_tick();

        // Phase 1: Input Processing
        input::process_input(&mut state, &mut command_rx, &output_registry);

        // Phase 2: Environment & Movement  (applied inside process_input for now)

        // Phase 3: NPC Routine System
        let tick = state.current_tick;
        npc_routine::npc_routine_system(&mut state.world, tick);

        // Phase 4: Game State Updates  (combat, levelling — future systems)

        // Phase 5: Output Broadcast
        // Production: iterate active players and flush their pending output.
        // For now: no extra broadcast needed — output is sent inline by handlers.
    }
}
