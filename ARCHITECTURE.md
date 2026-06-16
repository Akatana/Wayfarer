# Wayfarer Architecture Documentation

Wayfarer is a modern, high-performance, tick-based MUD (Multi-User Dungeon) server engine written in Rust. It utilizes a split asynchronous/synchronous architecture to handle high-concurrency networking and database persistence without compromising the determinism of the core game world loop.

## 🛠️ Core Technology Stack
* **Language:** Rust (Stable)
* **Asynchronous Runtime:** `tokio` (Handles networking and I/O tasks)
* **ECS (Entity Component System):** `hecs` (Lightweight, in-memory archetypal ECS for game state)
* **Database / ORM:** `SeaORM` + `SQLx` (Asynchronous object-relational mapping)
* **Protocols:** Dual-protocol engine supporting standard **TCP/Telnet** and **WebSockets**

---

## 🏗️ System Architecture Overview

To ensure maximum performance and eliminate lag, Wayfarer splits its responsibilities across three distinct execution layers communicating via asynchronous channels (`tokio::sync::mpsc`).

>               ┌────────────────────────┐
>               │  Network Layer (Async) │
>               │   Telnet & WebSockets  │
>               └───────────┬────────────┘
>                           │ (Command Packets)
>                           ▼
>  ┌──────────────────────────────────────────────────┐
>  │               Game Loop Layer                    │
>  │ ⏰ Fixed Tick Loop (e.g., 200ms)                 │
>  │ 🧩 In-Memory Live ECS World (hecs)               │
>  └────────────────────────┬─────────────────────────┘
>                           │ (Save / Load Events)
>                           ▼
>               ┌────────────────────────┐
>               │  Database Layer (Async)│
>               │     SeaORM Worker      │
>               └────────────────────────┘

### 1. The Network Layer (Asynchronous)
* Manages listening ports for both standard Telnet (TCP) and WebSockets.
* Spawns a separate `tokio` task per connected player to handle raw I/O stream reading/framing.
* Parses incoming raw bytes into discrete text commands and passes them to an `Input` channel.
* Listens to an individual `Output` channel to stream text buffers back to the player.

### 2. The Game Loop Layer (Synchronous Engine Context)
* Driven by a fixed-rate `tokio::time::interval` timer (Target: **4 ticks per second / 250ms frames**).
* **Strict Rule:** No `.await` boundaries or blocking I/O calls are permitted inside the core system execution blocks to prevent frame drops.
* Each tick executes five distinct phases deterministically:
    1.  **Input Processing:** Drains the network channel, mapping player inputs to respective entities.
    2.  **Environment & Movement:** Handles room-to-room navigation (N, S, E, W, NE, etc.).
    3.  **NPC Routine System:** Translates the universal `current_tick` count into in-game server time (e.g., 240 ticks = 1 game hour) and adjusts NPC entities' states and positions based on their schedules.
    4.  **Game State Updates:** Resolves leveling curves, attributes, combat mechanics, and item states.
    5.  **Output Broadcast:** Generates formatted strings from the updated world state and routes them back to the active network channels.

### 3. The Storage Layer (Asynchronous Worker)
* Manages connections to the relational database via `SeaORM`.
* Acts as a cold-storage layer. The ECS world retains runtime state; the database is touched only on player authentication, manual character saving, or periodic system backups (e.g., every 2400 ticks).

---

## 🗂️ Data & Component Mapping

Data models are explicitly decoupled between storage layouts and runtime ECS components:

* **Database Entities (`SeaORM` Models):** `User`, `Character`, `ItemTemplate`, `SavedInventory`. Optimized for relational integrity and fast querying on startup.
* **ECS Components (`hecs` Types):** `Position { room_id }`, `Name(String)`, `Stats`, `NpcRoutine { last_action_tick }`, `ClassType`, `Experience`. Optimized for dense iterative access during execution loops.

## 🧭 Direction & Movement System
The world layout is defined through an array of interconnected Room entities. Connections support all basic cardinal and ordinal directions: `North`, `South`, `East`, `West`, `NorthEast`, `NorthWest`, `SouthEast`, `SouthWest`, `Up`, `Down`.