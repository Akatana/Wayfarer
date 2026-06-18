use std::collections::HashMap;

use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use crate::command::{ClientId, PlayerInput};
use crate::game_state::GameState;
use crate::systems::{combat, input, npc_routine};

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
    // Load rooms from DB (seeds from JSON on first boot).
    let room_registry = crate::db::room::load_or_seed(&db)
        .await
        .expect("[GameLoop] Failed to load world from database");

    // Load items: seed DB on first boot, then spawn room items into ECS.
    let seed = crate::world::loader::load_seed(
        std::path::Path::new("assets/rooms"),
        std::path::Path::new("assets/items.json"),
    );
    crate::db::item::seed_if_empty(&db, &seed.item_defs, &seed.room_items)
        .await
        .expect("[GameLoop] Failed to seed items");
    let room_items = crate::db::item::load_in_rooms(&db)
        .await
        .expect("[GameLoop] Failed to load room items");

    let max_room_id = crate::db::room::max_id(&db)
        .await
        .expect("[GameLoop] Failed to fetch max room id");
    let max_item_id = crate::db::item::max_id(&db)
        .await
        .expect("[GameLoop] Failed to fetch max item id");

    // Load NPCs: seed DB on first boot, then spawn into ECS.
    let npc_seed = crate::world::loader::load_npcs(std::path::Path::new("assets/npcs.json"));
    crate::db::npc::seed_if_empty(&db, &npc_seed)
        .await
        .expect("[GameLoop] Failed to seed NPCs");
    let npcs = crate::db::npc::load_all(&db)
        .await
        .expect("[GameLoop] Failed to load NPCs");
    let max_npc_id = crate::db::npc::max_id(&db)
        .await
        .expect("[GameLoop] Failed to fetch max NPC id");

    // Load quests: seed DB on first boot, then build the in-memory HashMap.
    let quest_defs_vec =
        crate::world::loader::load_quests(std::path::Path::new("assets/quests.json"));
    crate::db::quest::seed_if_empty(&db, &quest_defs_vec)
        .await
        .expect("[GameLoop] Failed to seed quests");
    let quest_defs: std::collections::HashMap<i64, crate::quest::QuestDef> = quest_defs_vec
        .into_iter()
        .map(|def| (def.id, def))
        .collect();

    // Load NPC dialogue trees from the asset file (no DB storage — data is in-memory only).
    let dialogue_defs: std::collections::HashMap<i64, crate::dialogue::NpcDialogue> =
        crate::world::loader::load_dialogues(std::path::Path::new("assets/dialogues.json"))
            .into_iter()
            .map(|d| (d.npc_id, d))
            .collect();

    let mut state = GameState::with_rooms(room_registry);
    state.next_room_id = max_room_id + 1;
    state.next_item_id = max_item_id + 1;
    state.next_npc_id = max_npc_id + 1;
    state.quest_defs = quest_defs;
    state.dialogue_defs = dialogue_defs;
    crate::world::seed::spawn_items(&mut state.world, &room_items);
    crate::world::seed::spawn_npcs(&mut state.world, &npcs);

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
        for char_data in state.pending_saves.drain(..) {
            let db = db.clone();
            tokio::spawn(async move {
                if let Err(e) = crate::db::character::save(&db, char_data).await {
                    eprintln!("[DB] Character save failed: {e}");
                }
            });
        }

        // Drain item location saves queued by the previous tick's item handlers.
        for item_save in state.pending_item_saves.drain(..) {
            let db = db.clone();
            tokio::spawn(async move {
                if let Err(e) = crate::db::item::save_location(&db, &item_save).await {
                    eprintln!("[DB] Item save failed: {e}");
                }
            });
        }

        // Drain NPC patrol room saves queued by the previous tick's routine system.
        for npc_save in state.pending_npc_saves.drain(..) {
            let db = db.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    crate::db::npc::update_room(&db, npc_save.npc_id, npc_save.room_id).await
                {
                    eprintln!("[DB] NPC room save failed: {e}");
                }
            });
        }

        // Drain player quest state saves queued by the previous tick.
        for quest_save in state.pending_quest_saves.drain(..) {
            let db = db.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    crate::db::quest::save_player_quest(&db, quest_save.char_id, &quest_save.state)
                        .await
                {
                    eprintln!("[DB] Quest save failed: {e}");
                }
            });
        }

        // Drain admin world/item/npc operations queued by the previous tick.
        for op in state.pending_admin_ops.drain(..) {
            let db = db.clone();
            tokio::spawn(async move {
                use crate::game_state::AdminDbOp;
                let result = match op {
                    AdminDbOp::CreateRoom(room) => crate::db::room::create(&db, &room).await,
                    AdminDbOp::UpdateRoom {
                        id,
                        name,
                        description,
                    } => crate::db::room::update(&db, id, &name, &description).await,
                    AdminDbOp::UpsertExit {
                        room_id,
                        dir,
                        dest_id,
                    } => crate::db::room::upsert_exit(&db, room_id, dir, dest_id).await,
                    AdminDbOp::DeleteExit { room_id, dir } => {
                        crate::db::room::delete_exit(&db, room_id, dir).await
                    }
                    AdminDbOp::CreateItem(item) => crate::db::item::create(&db, &item).await,
                    AdminDbOp::DeleteItem(id) => crate::db::item::delete(&db, id).await,
                    AdminDbOp::UpdateItemName { id, name } => {
                        crate::db::item::update_name(&db, id, &name).await
                    }
                    AdminDbOp::UpdateItemDesc { id, description } => {
                        crate::db::item::update_description(&db, id, &description).await
                    }
                    AdminDbOp::UpdateItemSlot { id, equip_slot } => {
                        crate::db::item::update_slot(&db, id, equip_slot.as_deref()).await
                    }
                    AdminDbOp::UpdateItemReq {
                        id,
                        level,
                        strength,
                        dexterity,
                        knowledge,
                    } => {
                        crate::db::item::update_requirements(
                            &db, id, level, strength, dexterity, knowledge,
                        )
                        .await
                    }
                    AdminDbOp::CreateNpc(npc) => crate::db::npc::create(&db, &npc).await,
                    AdminDbOp::DeleteNpc(id) => crate::db::npc::delete(&db, id).await,
                    AdminDbOp::UpdateNpcName { id, name } => {
                        crate::db::npc::update_name(&db, id, &name).await
                    }
                    AdminDbOp::UpdateNpcDesc { id, description } => {
                        crate::db::npc::update_description(&db, id, &description).await
                    }
                    AdminDbOp::UpdateNpcGreet { id, greeting } => {
                        crate::db::npc::update_greeting(&db, id, greeting.as_deref()).await
                    }
                    AdminDbOp::UpdateNpcHostile { id, hostile } => {
                        crate::db::npc::update_hostile(&db, id, hostile).await
                    }
                    AdminDbOp::UpdateNpcPassive { id, passive } => {
                        crate::db::npc::update_passive(&db, id, passive).await
                    }
                    AdminDbOp::SetNpcPatrol { id, rooms } => {
                        crate::db::npc::set_patrol(&db, id, &rooms).await
                    }
                };
                if let Err(e) = result {
                    eprintln!("[DB] Admin op failed: {e}");
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
        npc_routine::npc_routine_system(
            &mut state.world,
            tick,
            &mut state.pending_npc_saves,
            &output_registry,
            &state.room_registry,
        );

        // Phase 4: Combat System
        combat::combat_system(&mut state, &output_registry);

        // Phase 5: Output Broadcast
        // Production: iterate active players and flush their pending output.
        // For now: no extra broadcast needed — output is sent inline by handlers.
    }
}
