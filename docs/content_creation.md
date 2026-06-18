# Wayfarer — Content Creation Guide

Wayfarer allows content creators to build an immersive world by defining **Items**, **NPCs**, **Dialogue Trees**, and **Quests**. These entities are defined statically via JSON configuration files inside the `/assets` directory. On system boot, Wayfarer parses these assets and seeds them into the SQLite database (`wayfarer.db`) for persistence.

This guide details how to create and link these components together, both through static JSON configuration and in-game Admin commands.

---

## Table of Contents
1. [General Concepts](#general-concepts)
2. [Items (`assets/items.json`)](#1-items-assetsitemsjson)
3. [NPCs (`assets/npcs.json`)](#2-npcs-assetsnpcsjson)
4. [NPC Dialogues (`assets/dialogues.json`)](#3-npc-dialoguesassetsdialoguesjson)
5. [Quests (`assets/quests.json`)](#4-questsassetsquestsjson)
6. [Linking Everything Together: A Tutorial](#5-linking-everything-together-a-tutorial)

---

## General Concepts

Wayfarer operates on a dual-persistence model:
* **Static Assets:** The `/assets` files serve as the "source of truth" and seed the database on the first run of a new game database.
* **Database Persistence:** Once seeded, dynamic properties (like NPC positions, item locations, and character quest logs) are loaded from and saved to the SQLite database.
* **Admin Commands:** Authorized administrators can modify the live world in real time. Many in-game commands have a persistent effect, saving changes directly back to the database.

> [!WARNING]
> Static JSON files (`assets/items.json`, `assets/npcs.json`, etc.) are only read on first boot when the relevant database table is empty. Changes made after the server has seeded the database will not be reflected automatically. To apply sweeping changes to static definitions, delete `wayfarer.db` to force a full re-seed. Alternatively, use the in-game `@i*` admin commands to update definitions without wiping the database.

---

## 1. Items (`assets/items.json`)

Items use a **definition / instance** model, similar to WoW-style itemisation:

- **Definitions** (templates) live in `assets/items.json` and are seeded into the `item_definitions` database table on first boot. Each definition has a stable `id` used across rooms, quests, and loot tables.
- **Instances** are physical copies of a definition — one is created for each room placement listed in room JSON files, and more are created whenever an NPC drops loot or an admin uses `@ispawn`. Multiple players can each carry their own copy of the same item type.

`assets/items.json` defines the templates. Room JSON files reference them by `id` to place starting instances in the world.

### Item JSON Schema

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | `integer` | **Yes** | Unique stable ID. Crucial for linking to rooms, quests, and dialogues. |
| `name` | `string` | **Yes** | Display name of the item. Supports color tags (e.g., `<cyan>a key</cyan>`). |
| `description` | `string` | **Yes** | The text displayed when the item is inspected using `examine <item>`. |
| `equip_slot` | `string` | No | If equippable, the slot key (see list below). Set to `null` or omit if not equippable. |
| `two_handed` | `boolean` | No | If `true`, requires the weapon to be held with both hands (blocks the off-hand). Defaults to `false`. |
| `bag_capacity` | `integer` | No | If slot is a bag, specifies how many extra slots this item adds to character inventory. |
| `requirements` | `object` | No | Stat requirements to equip. Fields include `level`, `strength`, `dexterity`, and `knowledge`. |

### Valid Equipment Slots
When specifying `equip_slot`, use one of the following exact string keys:
* **Armor:** `Head`, `Chest`, `Shoulders`, `Back`, `Gloves`, `Legs`, `Feet`
* **Weapons & Shields:** `LeftHand` (Main hand), `RightHand` (Off hand / Shield)
* **Accessories:** `Necklace`, `Ring1`, `Ring2`
* **Bags:** `Bag1`, `Bag2`, `Bag3`, `Bag4`

### Item JSON Example
```json
[
  {
    "id": 1,
    "name": "a rusty sword",
    "description": "An old iron sword, pitted with rust. Still sharp enough to cut.",
    "equip_slot": "LeftHand",
    "requirements": { "strength": 5 }
  },
  {
    "id": 3,
    "name": "a small pouch",
    "description": "A leather pouch just big enough to hold a handful of odds and ends. (+5 slots)",
    "equip_slot": "Bag1",
    "bag_capacity": 5
  }
]
```

### In-Game Admin Commands for Items
Administrators can define new item types and spawn instances at runtime:
* `@idefs` — Lists all loaded item definitions (both from `items.json` and admin-created), showing their `def_id`, name, and equip slot.
* `@mitem <name> [/ <description>]` — Creates a new item definition and immediately spawns one instance in the current room. Returns both the `def_id` and the instance `id`.
* `@ispawn <def_id>` — Spawns a new instance of an existing definition in the current room. Use after `@iname`/`@islot`/`@ireq` to place updated copies.
* `@destroy <name>` — Permanently destroys a named item instance on the floor of the current room.
* `@iname <def_id> <name>` — Renames a definition by its `def_id`. Future `@ispawn` calls will use the new name.
* `@idesc <def_id> <description>` — Updates a definition's description.
* `@islot <def_id> <slot>` — Sets the definition's equip slot (or `none` to clear).
* `@ireq <def_id> <stat> <value>` — Sets a stat requirement on the definition (`str`, `dex`, `knw`, `level`). Set to `0` to clear.

> All `@i*` edit commands target a **definition id** (`def_id`), not an instance. Use `@idefs` to look up ids.

---

## 2. NPCs (`assets/npcs.json`)

Non-Player Characters (NPCs) populate rooms, execute schedules, engage in dialogue, and act as combat targets.

### NPC JSON Schema

| Field | Type | Default | Description |
|---|---|---|---|
| `id` | `integer` | — | Unique stable NPC identifier. |
| `name` | `string` | — | Display name of the NPC. |
| `description` | `string` | `""` | Description visible when using `examine <npc>`. |
| `greeting` | `string` | `null` | A single-line greeting when a player uses `talk <npc>`. Used if no dialogue tree exists. |
| `hostile` | `boolean` | `false` | If `true`, automatically attacks any player entering their room. |
| `passive` | `boolean` | `false` | If `true`, this NPC never fights back or retaliates when attacked. |
| `room_id` | `integer` | — | The initial starting room ID for the NPC. |
| `patrol` | `array [integer]` | `[]` | Comma-separated list of Room IDs. NPC moves along this cycle every 60 seconds. |
| `max_hp` | `integer` | `20` | Maximum health points. |
| `min_damage` | `integer` | `1` | Minimum attack damage per tick. |
| `max_damage` | `integer` | `4` | Maximum attack damage per tick. |
| `attack_ticks` | `integer` | `10` | Attack speed in server ticks (200ms per tick. `10` ticks = 2 seconds). |
| `xp_reward` | `integer` | `10` | Experience points awarded to a player upon defeating this NPC. |

### NPC JSON Example
```json
[
  {
    "id": 1,
    "name": "a weathered town guard",
    "description": "A stern guard in worn leather armor, eyes scanning the square.",
    "greeting": "Move along, citizen. Nothing to see here.",
    "hostile": false,
    "passive": false,
    "room_id": 1,
    "patrol": [1, 2, 1, 2],
    "max_hp": 60,
    "min_damage": 2,
    "max_damage": 6,
    "attack_ticks": 8,
    "xp_reward": 25
  }
]
```

### In-Game Admin Commands for NPCs
* `@mnpc <name> [/ <description>]` — Creates a new NPC in the current room.
* `@ndestroy <id|name>` — Destroys an NPC by numeric world ID or by name in the current room.
* `@nlist` — Lists all NPCs in the world, their IDs, locations, and flags.
* `@ninfo <id>` — Shows complete stats, patrol route, and status of an NPC.
* `@nname <id> <name>` / `@ndesc <id> <description>` — Edits basic text attributes.
* `@ngreet <id> <text>` — Updates the single-line greeting.
* `@nhostile <id> <true|false>` — Toggles auto-aggro behavior.
* `@npassive <id> <true|false>` — Toggles passive behavior (non-retaliating).
* `@npatrol <id> <room_ids>` — Sets patrol route (comma-separated IDs) or `none`.

---

## 3. NPC Dialogues (`assets/dialogues.json`)

Dialogue trees allow for rich, branching conversations with NPCs. They are stored entirely in-memory and link directly to the quest engine to award, update, or complete quests.

### Schema Structure
A dialogue definition is bound to an NPC via `npc_id`. It contains an array of `nodes`, where each node is a conversational state.

```json
{
  "npc_id": 1,
  "nodes": [
    {
      "id": "root",
      "text": "State your business, traveler.",
      "options": [ ... ]
    }
  ]
}
```

> [!IMPORTANT]
> Every dialogue tree **must** have a node with `"id": "root"`. This is the entry point when a player types `talk <npc>`.

### Dialogue Node Fields
* `id` (`string`): The node identifier (unique within this NPC's tree).
* `text` (`string`): What the NPC says when this node is displayed.
* `options` (`array`): Choices presented to the player. If empty, the conversation immediately terminates.

### Dialogue Option Fields
Each option contains:
* `text` (`string`): The response label shown to the player.
* `goto` (`string` or `null`): The ID of the node to navigate to next. Use `null` to exit dialogue.
* `conditions` (`array`): Pre-requisites that must be met for this choice to appear.
* `effects` (`array`): Side effects triggered when the choice is clicked.

---

### Dialogue Conditions
Conditions let you gate options behind character levels or quest progress. They are structured objects with a `"type"` tag:

| JSON Syntax | Description |
|---|---|
| `{ "type": "min_level", "level": <int> }` | Player must be at least the specified level. |
| `{ "type": "quest_not_started", "quest_id": <id> }` | Quest is not active or completed in the player's log. |
| `{ "type": "quest_active", "quest_id": <id> }` | Quest is currently active. |
| `{ "type": "quest_phase", "quest_id": <id>, "phase": <index> }` | Player is active on the 0-based phase index of the quest. |
| `{ "type": "quest_ready", "quest_id": <id> }` | All objectives for the current phase are completed, and it is ready to turn in. |
| `{ "type": "quest_ready_at_phase", "quest_id": <id>, "phase": <index> }` | All objectives for the specified 0-based phase index are met. |
| `{ "type": "quest_complete", "quest_id": <id> }` | The player has fully completed this quest. |

---

### Dialogue Effects
Effects fire side-effects in order when the option is selected. They are structured objects with a `"type"` tag:

| JSON Syntax | Description |
|---|---|
| `{ "type": "accept_quest", "quest_id": <id> }` | Starts the quest for the player (no-op if already started). |
| `{ "type": "mark_objective", "quest_id": <id> }` | Marks any active `Talk` objectives matching this NPC's ID as completed. |
| `{ "type": "turn_in_quest" }` | Turns in any eligible active quest phase that lists this NPC as the completion target. |
| `{ "type": "give_item", "item_id": <id> }` | Transfers the world item template with matching ID into the player's inventory. |

---

### Dialogue JSON Example
```json
[
  {
    "npc_id": 1,
    "nodes": [
      {
        "id": "root",
        "text": "Hello. Do you need something?",
        "options": [
          {
            "text": "I'm looking for a quest.",
            "goto": "quest_node",
            "conditions": [{ "type": "quest_not_started", "quest_id": 1 }]
          },
          {
            "text": "Goodbye.",
            "goto": null
          }
        ]
      },
      {
        "id": "quest_node",
        "text": "The roads are dangerous. Go explore the Northern Gate.",
        "options": [
          {
            "text": "I will go now.",
            "goto": null,
            "effects": [{ "type": "accept_quest", "quest_id": 1 }]
          }
        ]
      }
    ]
  }
]
```

---

## 4. Quests (`assets/quests.json`)

Quests provide long-term objectives for players. They support multi-stage completion tracking, objective types, and custom rewards.

### Quest JSON Schema

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | `integer` | **Yes** | Unique quest identifier. |
| `name` | `string` | **Yes** | Title of the quest (visible in logs). |
| `description` | `string` | **Yes** | High-level summary of the quest. |
| `giver_npc_id` | `integer` | No | ID of the NPC that offers the quest (adds an automatic `[!]` indicator to their name). Set to `null` if given via an item. |
| `giver_item_id` | `integer` | No | ID of the item that triggers the quest when `examined`. Set to `null` if given via NPC. |
| `phases` | `array [Phase]` | **Yes** | List of quest phases. Player progresses through them sequentially. |

### Quest Phase Structure
Each Phase contains:
* `description` (`string`): Description of what the player must do in this phase.
* `objectives` (`array [Objective]`): The list of tasks in the phase (see table below).
* `completion_npc_id` (`integer` or `null`): The NPC the player must talk to in order to finish this phase. If `null`, the phase auto-completes the moment all objectives are completed.
* `completion_text` (`string`): The text displayed to the player upon completing this phase.
* `xp_reward` (`integer`): Experience points awarded on phase completion.
* `lp_reward` (`integer`): Learning points awarded on phase completion.
* `copper_reward` (`integer`): Copper coins awarded on phase completion (e.g. `100` = 1 silver, `10000` = 1 gold).
* `item_rewards` (`array [integer]`): Array of item template IDs loaded from the world and placed directly in the player's inventory upon phase turn-in.

### Quest Objective Types
Wayfarer supports three types of objectives in a phase:

| Type | Objective JSON structure | Trigger Condition |
|---|---|---|
| **Reach** | `{ "type": "reach", "room_id": <id>, "description": "Go to..." }` | Automatically triggers when the player walks into the specified Room ID. |
| **Talk** | `{ "type": "talk", "npc_id": <id>, "description": "Talk to..." }` | Triggers via dialogue choice that calls `"effects": [{"type": "mark_objective", ...}]`. |
| **Examine**| `{ "type": "examine", "item_id": <id>, "description": "Inspect..." }` | Triggers when the player executes `examine <item>` on the floor or in inventory. |

### Quest JSON Example
```json
[
  {
    "id": 1,
    "name": "Scouting the Gate",
    "description": "The town guard wants you to check the northern border.",
    "giver_npc_id": 1,
    "giver_item_id": null,
    "phases": [
      {
        "description": "Walk to the Northern Gate and speak to the guard there.",
        "objectives": [
          { "type": "reach", "room_id": 2, "description": "Reach the Northern Gate" },
          { "type": "talk", "npc_id": 2, "description": "Speak to the Gatekeeper" }
        ],
        "completion_npc_id": 1,
        "completion_text": "Excellent work. Here is some gold and a shield.",
        "xp_reward": 100,
        "lp_reward": 2,
        "copper_reward": 5000,
        "item_rewards": [5]
      }
    ]
  }
]
```

### In-Game Admin Commands for Quests
* `@qlist` — Lists all quest templates loaded on the server.
* `@qinfo <id>` — Shows detailed phase configurations, objectives, and reward structures.
* `@qgive <player name> <quest_id>` — Manually awards a quest to an online player.
* `@qreset <player name> <quest_id>` — Resets a player's quest back to Phase 1, Objective 1.

---

## 5. Linking Everything Together: A Tutorial

To create a complete quest flow where a player starts a quest from an NPC, completes a task, and gets rewarded, follow these steps:

### Step 1: Create the Item Reward
In `assets/items.json`, define the item reward. We'll give it ID `10`:
```json
{
  "id": 10,
  "name": "a gleaming silver ring",
  "description": "A polished ring inscribed with protective ruins.",
  "equip_slot": "Ring1"
}
```

### Step 2: Create the Quest Giver and target NPCs
In `assets/npcs.json`, place two NPCs. The Giver (`id: 1`) in Room `1`, and the Scout Target (`id: 2`) in Room `2`.
```json
[
  {
    "id": 1,
    "name": "Commander Vael",
    "description": "The commander of the town watch.",
    "room_id": 1
  },
  {
    "id": 2,
    "name": "Apprentice Kaelen",
    "description": "A young mage studying ley lines.",
    "room_id": 2
  }
]
```

### Step 3: Define the Quest
In `assets/quests.json`, create a quest (`id: 5`) with one phase. The objective is to talk to Apprentice Kaelen (`npc_id: 2`). Upon completion, the player returns to Commander Vael (`completion_npc_id: 1`) to receive the silver ring (`item_rewards: [10]`).
```json
[
  {
    "id": 5,
    "name": "The Ley Line Crisis",
    "description": "Speak with the Apprentice at the Northern Gate to understand the source of the anomalies.",
    "giver_npc_id": 1,
    "giver_item_id": null,
    "phases": [
      {
        "description": "Deliver a message to Apprentice Kaelen.",
        "objectives": [
          { "type": "talk", "npc_id": 2, "description": "Ask Kaelen about the ley lines" }
        ],
        "completion_npc_id": 1,
        "completion_text": "Thank you for confirming Kaelen's safety. Here is your reward.",
        "xp_reward": 150,
        "item_rewards": [10]
      }
    ]
  }
]
```

### Step 4: Write the Dialogue Trees
To handle quest acceptance and objective progress, define dialogue trees in `assets/dialogues.json` for both NPCs:

1. **Commander Vael (NPC 1):** Offers the quest, and completes it upon return.
2. **Apprentice Kaelen (NPC 2):** Has an option to discuss the ley lines, which marks the talk objective complete.

```json
[
  {
    "npc_id": 1,
    "nodes": [
      {
        "id": "root",
        "text": "Hello citizen. The watch is busy today.",
        "options": [
          {
            "text": "Commander, do you need assistance?",
            "goto": "quest_offer",
            "conditions": [{ "type": "quest_not_started", "quest_id": 5 }]
          },
          {
            "text": "I spoke with Kaelen. Here is his report.",
            "goto": "quest_complete_node",
            "conditions": [{ "type": "quest_ready", "quest_id": 5 }],
            "effects": [{ "type": "turn_in_quest" }]
          },
          {
            "text": "Goodbye.",
            "goto": null
          }
        ]
      },
      {
        "id": "quest_offer",
        "text": "Actually, yes. Apprentice Kaelen went to inspect the gate hours ago and hasn't returned. Can you look for him?",
        "options": [
          {
            "text": "I'll find him.",
            "goto": "quest_accepted",
            "effects": [{ "type": "accept_quest", "quest_id": 5 }]
          }
        ]
      },
      {
        "id": "quest_accepted",
        "text": "Thank you. He is likely near the Northern Gate.",
        "options": [{ "text": "Understood.", "goto": null }]
      },
      {
        "id": "quest_complete_node",
        "text": "Good work. I'll make sure the watch keeps a close eye on Kaelen's findings. Take this ring for your effort.",
        "options": [{ "text": "Thank you, Commander.", "goto": null }]
      }
    ]
  },
  {
    "npc_id": 2,
    "nodes": [
      {
        "id": "root",
        "text": "Oh! You startled me. These energy readings are off the charts...",
        "options": [
          {
            "text": "Commander Vael sent me to check on you.",
            "goto": "report_status",
            "conditions": [{ "type": "quest_active", "quest_id": 5 }],
            "effects": [{ "type": "mark_objective", "quest_id": 5 }]
          },
          {
            "text": "Goodbye.",
            "goto": null
          }
        ]
      },
      {
        "id": "report_status",
        "text": "I'm safe, but the ley lines are unstable. Tell the Commander that I will remain here to gather more readings.",
        "options": [{ "text": "I will return to Vael at once.", "goto": null }]
      }
    ]
  }
]
```
