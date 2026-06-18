# Wayfarer — Command Reference

Commands are case-insensitive. Arguments in `<angle brackets>` are required; `[square brackets]` are optional. Aliases are separated by `/`.

---

## Movement

| Command | Description |
|---|---|
| `north` / `n` | Move north |
| `south` / `s` | Move south |
| `east` / `e` | Move east |
| `west` / `w` | Move west |
| `northeast` / `ne` | Move northeast |
| `northwest` / `nw` | Move northwest |
| `southeast` / `se` | Move southeast |
| `southwest` / `sw` | Move southwest |
| `up` / `u` | Move up |
| `down` / `d` | Move down |

Moving while in combat breaks combat automatically.

---

## Combat

| Command | Description |
|---|---|
| `kill <target>` / `attack <target>` / `k <target>` / `hit <target>` | Attack an NPC in the current room by name |
| `flee` / `fl` / `run` | Escape combat through the first available exit |

**Combat notes:**
- Combat is real-time — both sides attack on a per-entity tick timer (every 200 ms tick).
- Hostile NPCs auto-aggro players who enter their room. The NPC attacks immediately; the player must type `kill <target>` to fight back (AFK protection).
- If an NPC initiates combat, the player still has `InCombat` tracking and can `flee` without typing `kill` first.
- Player attack speed scales with Dexterity: `max(3, 10 - DEX/5)` ticks between attacks.
- Passive NPCs (flag set via `@npassive`) never retaliate when attacked.
- When you kill an NPC you receive XP. Enough XP causes a level-up.
- Killed NPCs respawn at their home room after 60 seconds (300 ticks).
- If a player dies they are teleported back to the starting room with half HP restored.

---

## General

| Command | Description |
|---|---|
| `look` / `l` | Describe the current room, including exits, items on the floor, and NPCs present |
| `examine <target>` / `x <target>` / `ex <target>` | Read the description of an item (floor, bag, or equipped) or an NPC in the room |
| `say <message>` | Broadcast a message to everyone in the same room |
| `talk <name>` | Talk to a named NPC in the current room |
| `score` / `sc` | Display your character stats (level, HP, MP, STR, DEX, KNW, location) |
| `balance` / `bal` / `money` / `gold` / `wallet` | Show your current wealth |
| `quests` / `quest` / `ql` | Show your active quest log |
| `help` / `h` / `?` | Show a full list of commands, grouped by category |
| `help <command>` | Show details and aliases for a specific command |
| `quit` / `exit` / `bye` | Save and disconnect |

---

## Inventory & Equipment

| Command | Description |
|---|---|
| `inventory` / `inv` / `i` | List your bag contents and equipped items |
| `get <item>` / `take <item>` | Pick up a named item from the floor |
| `drop <item>` | Drop an item from your bag onto the floor |
| `equip <item>` / `wear <item>` / `wield <item>` | Equip a named item from your bag |
| `unequip <slot/item>` / `remove` / `unwear` / `unwield` | Move an equipped item back into your bag, by slot name or item name |

### Equipment slots

| Keyword(s) | Slot |
|---|---|
| `head`, `helm`, `helmet` | Head |
| `chest`, `torso`, `body`, `armor` | Chest |
| `shoulders`, `shoulder`, `pauldrons` | Shoulders |
| `back`, `cape`, `cloak` | Back |
| `gloves`, `hands`, `gauntlets` | Gloves |
| `legs`, `pants`, `leggings`, `greaves` | Legs |
| `feet`, `boots`, `shoes`, `sandals` | Feet |
| `lefthand`, `lhand`, `mainhand`, `main`, `weapon` | Left Hand (main hand) |
| `righthand`, `rhand`, `offhand`, `off`, `shield` | Right Hand (off hand) |
| `necklace`, `neck`, `amulet` | Necklace |
| `ring`, `ring1` | Ring 1 |
| `ring2` | Ring 2 |
| `bag`, `bag1` | Bag 1 |
| `bag2` | Bag 2 |
| `bag3` | Bag 3 |
| `bag4` | Bag 4 |

---

## Admin — General

> All admin commands are prefixed with `@` and require the admin flag on your character.

| Command | Description |
|---|---|
| `@who` | List all currently connected players |

---

## Admin — Rooms

| Command | Description |
|---|---|
| `@roominfo` | Show the current room's id, name, description, exits (with destination ids), and floor items with their ids |
| `@rename <name>` | Rename the current room |
| `@redesc <description>` | Change the current room's description |
| `@goto <room_id>` | Teleport to a room by numeric id |
| `@dig <direction> <room name>` | Carve a new room in the given direction and link it bidirectionally. Example: `@dig north Tower of Stars` |
| `@link <direction> <room_id>` | Add a one-way exit from the current room to an existing room. Example: `@link east 5` |
| `@unlink <direction>` | Remove an exit from the current room. Example: `@unlink west` |

**Valid directions for `@dig`, `@link`, `@unlink`:** `north`, `south`, `east`, `west`, `northeast`, `northwest`, `southeast`, `southwest`, `up`, `down` (and single-letter abbreviations).

---

## Admin — Items

| Command | Description |
|---|---|
| `@mitem <name> [/ <description>]` | Create a new item in the current room. Returns the item's `#id`. Example: `@mitem a rusty key / A key caked in rust.` |
| `@destroy <name>` | Permanently destroy a named item on the floor of the current room |
| `@iname <id> <name>` | Rename an item by id |
| `@idesc <id> <description>` | Set an item's description by id |
| `@islot <id> <slot>` | Set an item's equip slot by id. Use any slot keyword from the table above, or `none` to make the item unequippable |
| `@ireq <id> <stat> <value>` | Set one stat requirement on an item by id. `stat` must be one of `str`, `dex`, `knw`, `level`. Set to `0` to clear |

**Item editing example workflow:**
```
@mitem a war axe / A heavy double-headed axe.
→ [Admin] 'a war axe' (#1042) created in this room.

@islot 1042 lefthand
@ireq 1042 str 10
@ireq 1042 level 3
```

---

## Admin — NPCs

| Command | Description |
|---|---|
| `@mnpc <name> [/ <description>]` | Create a new NPC in the current room. Returns the NPC's `#id`. Example: `@mnpc a town guard / A guard in worn leather armor.` |
| `@ndestroy <id\|name>` | Permanently destroy an NPC. Accepts a numeric id (world-wide) or a name match in the current room |
| `@nlist` | List all NPCs in the world with their ids, current rooms, and flags |
| `@ninfo <id>` | Show full details for an NPC: name, description, greeting, hostile/passive flags, current room, and patrol route |
| `@nname <id> <name>` | Rename an NPC by id |
| `@ndesc <id> <description>` | Set an NPC's description by id |
| `@ngreet <id> <text>` | Set an NPC's greeting (shown when a player uses `talk`). Use `none` to clear it |
| `@nhostile <id> <true\|false>` | Toggle auto-aggro: hostile NPCs attack any player who enters their room. Accepts `true`/`yes`/`1` or `false`/`no`/`0` |
| `@npassive <id> <true\|false>` | Toggle retaliation: passive NPCs never fight back when attacked. Accepts `true`/`yes`/`1` or `false`/`no`/`0` |
| `@npatrol <id> <room_ids>` | Set a patrol route as a comma-separated list of room ids. The NPC advances one step every 60 seconds. Use `none` to clear and make the NPC stationary |

**NPC combat stats** (set in `assets/npcs.json` or directly in the database):

| Field | Default | Description |
|---|---|---|
| `max_hp` | 20 | Maximum hit points |
| `min_damage` | 1 | Minimum damage per attack |
| `max_damage` | 4 | Maximum damage per attack |
| `attack_ticks` | 10 | Ticks between NPC attacks (200 ms/tick → 10 = 2 s) |
| `xp_reward` | 10 | XP awarded to the player who kills this NPC |

**NPC editing example workflow:**
```
@mnpc a wandering merchant / A cheerful merchant with a heavy pack.
→ [Admin] NPC 'a wandering merchant' (#3) created in this room.

@ngreet 3 Fine wares, fine prices! What can I do for you?
@npatrol 3 1,2,3,2
@ninfo 3
```

**Patrol notes:**
- The patrol list is a cycle — after the last room the NPC wraps back to the first.
- Players in a room are notified when a patrolling NPC enters or leaves, including the direction.
- Patrolling NPCs respawn at the first room in their patrol route after being killed.
- The NPC's current room is persisted to the database so patrol position survives server restarts.
- If consecutive steps share the same room id, the NPC stays put for that interval (useful for timed pauses).

---

## Admin — Quests

| Command | Description |
|---|---|
| `@qlist` | List all quest definitions with their ids and names |
| `@qinfo <id>` | Show full details for a quest: phases, objectives, completion NPCs, and XP rewards |
| `@qgive <player name> <quest_id>` | Give a quest to a named player (they must be online) |
| `@qreset <player name> <quest_id>` | Reset a player's quest back to phase 1, objective 1 |

**Quest notes:**
- Quests are defined in `assets/quests.json` and seeded into the database on first boot.
- Quests are awarded automatically when a player `talk`s to an NPC that offers one, or `examine`s a qualifying item.
- Quest objectives are tracked per-player. Eligible objectives advance automatically on the relevant action (kill, examine, talk).
- When all objectives in a phase are met, the player must return to the completion NPC to advance or complete the quest.
