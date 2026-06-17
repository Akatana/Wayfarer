# Wayfarer

A tick-based MUD server engine written in Rust. Supports classic Telnet and WebSocket connections simultaneously, with a fixed-rate game loop, ECS-based world state, and SQLite character/room persistence.

[![CI](https://github.com/Akatana/Wayfarer/actions/workflows/ci.yml/badge.svg)](https://github.com/Akatana/Wayfarer/actions/workflows/ci.yml)

---

## Features

- **Dual-protocol networking** — Telnet on port 4000, WebSocket on port 4001; same game loop serves both
- **Fixed-tick game loop** — 200 ms per tick (5 TPS); no `.await` inside tick body, guaranteed determinism
- **ECS world state** — [hecs](https://github.com/Ralith/hecs) archetypal ECS; players, NPCs, and items are entities with composable components
- **Account system** — register or login on connect; bcrypt password hashing; the first registered account automatically becomes admin
- **Character management** — each account can hold multiple characters; create, select, or delete from a menu before entering the world
- **Room world from SQLite** — rooms and exits are stored in the database; seed data is inserted on first boot so the world can be edited without recompiling
- **Character persistence** — stats and position saved on quit via a fire-and-forget async queue drained between ticks
- **Color markup** — room descriptions and game text support `<red>…</red>` style tags, rendered to ANSI on the way out to the socket
- **Admin accounts** — admin flag on accounts; `AdminFlag` ECS component gates privileged commands (`@who`)
- **NPC routine system** — fires every 300 ticks (60 real seconds); stub ready for patrol/dialogue logic

---

## Quick Start

```bash
cargo run
```

On first boot the engine will:

1. Create `wayfarer.db` in the current directory
2. Run schema migrations (`accounts`, `characters`, `rooms`, `exits` tables)
3. Seed the starter world (4 rooms) if the `rooms` table is empty
4. Start the Telnet server on **port 4000** and WebSocket server on **port 4001**

The first account registered via the in-game menu automatically becomes admin.

Connect with any Telnet client:

```bash
telnet localhost 4000
# or
nc localhost 4000
```

Connect from a browser console (WebSocket):

```js
const ws = new WebSocket("ws://localhost:4001");
ws.onmessage = e => console.log(e.data);
ws.send("R");          // Register
ws.send("alice");      // username
ws.send("password1");  // password
ws.send("password1");  // confirm
ws.send("N");          // New character
ws.send("Aldric");     // character name
ws.send("look");
ws.send("north");
```

---

## Architecture

Three layers communicate via `tokio::sync::mpsc` channels:

```
┌──────────────────────────────────────────┐
│           Network Layer (async)          │
│   Telnet (4000)    WebSocket (4001)      │
│   session.rs       ws_session.rs         │
│   parser.rs        color::render()       │
└────────────────────┬─────────────────────┘
                     │  PlayerInput / register / deregister
                     ▼
┌──────────────────────────────────────────┐
│         Game Loop Layer (sync)           │
│   200 ms fixed tick — no .await inside  │
│   hecs ECS world  ·  RoomRegistry        │
│   input → movement → NPC → output       │
└────────────────────┬─────────────────────┘
                     │  pending_saves (fire-and-forget)
                     ▼
┌──────────────────────────────────────────┐
│        Storage Layer (async)             │
│   SeaORM + SQLite                        │
│   accounts  ·  characters  ·  rooms      │
└──────────────────────────────────────────┘
```

**Key invariant:** no `.await` inside the tick body. All async work (DB saves, connection registration) happens in the gaps between ticks.

---

## Project Layout

```
src/
├── network/    # Telnet + WebSocket accept loops, session handlers, command parser
├── systems/    # Input dispatch, movement, NPC routines, output routing
├── world/      # Room registry, player registry, seed data
├── db/         # SeaORM entities and schema for accounts, characters, rooms, and exits
├── game_loop.rs
├── game_state.rs
└── color.rs    # <tag> → ANSI markup renderer
tests/
└── integration_tests.rs
```