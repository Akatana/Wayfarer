/// A single entry in the help registry.
///
/// Add an entry here when adding a new command; `help` and `help <topic>`
/// will automatically pick it up without any other changes required.
pub struct HelpEntry {
    /// Primary name used when the player types `help <topic>`.
    pub command: &'static str,
    /// Comma-separated alternative names (abbreviations, aliases).
    pub aliases: &'static str,
    /// Full usage string shown in both list and detail views.
    pub syntax: &'static str,
    /// One-line description.
    pub description: &'static str,
    /// Grouping header in the full help output.
    pub category: &'static str,
    /// If true, only shown to admins.
    pub admin_only: bool,
}

/// Ordered category headers for the full `help` listing.
pub const CATEGORIES: &[&str] = &[
    "Movement",
    "Communication",
    "Inventory",
    "Combat",
    "Information",
    "General",
    "Admin: World",
    "Admin: Items",
    "Admin: NPCs",
    "Admin: Quests",
];

/// Every command registered for the help system.
///
/// To add a new command: append a `HelpEntry` here. Nothing else needs
/// to change — the `help` command iterates this slice at runtime.
pub static HELP_ENTRIES: &[HelpEntry] = &[
    // ── Movement ─────────────────────────────────────────────────────────────
    HelpEntry {
        command: "north",
        aliases: "south, east, west, northeast, northwest, southeast, southwest, up, down, n, s, e, w, ne, nw, se, sw, u, d",
        syntax: "<direction>",
        description: "Move in a direction: north/n, south/s, east/e, west/w, ne, nw, se, sw, up, down.",
        category: "Movement",
        admin_only: false,
    },
    // ── Communication ─────────────────────────────────────────────────────────
    HelpEntry {
        command: "say",
        aliases: "",
        syntax: "say <message>",
        description: "Say something to everyone in the room.",
        category: "Communication",
        admin_only: false,
    },
    HelpEntry {
        command: "talk",
        aliases: "",
        syntax: "talk <npc>",
        description: "Start a conversation with an NPC. Type a number to choose a response; type 'bye' to leave.",
        category: "Communication",
        admin_only: false,
    },
    // ── Inventory ─────────────────────────────────────────────────────────────
    HelpEntry {
        command: "inventory",
        aliases: "i, inv",
        syntax: "inventory",
        description: "Show what you are carrying and wearing.",
        category: "Inventory",
        admin_only: false,
    },
    HelpEntry {
        command: "get",
        aliases: "take",
        syntax: "get <item>",
        description: "Pick up an item from the room.",
        category: "Inventory",
        admin_only: false,
    },
    HelpEntry {
        command: "drop",
        aliases: "",
        syntax: "drop <item>",
        description: "Drop an item from your inventory onto the floor.",
        category: "Inventory",
        admin_only: false,
    },
    HelpEntry {
        command: "equip",
        aliases: "wear, wield",
        syntax: "equip <item>",
        description: "Equip a wearable or wieldable item.",
        category: "Inventory",
        admin_only: false,
    },
    HelpEntry {
        command: "unequip",
        aliases: "remove, unwear, unwield",
        syntax: "unequip <item>",
        description: "Remove an equipped item back to your inventory.",
        category: "Inventory",
        admin_only: false,
    },
    // ── Combat ────────────────────────────────────────────────────────────────
    HelpEntry {
        command: "kill",
        aliases: "attack, k, hit",
        syntax: "kill <target>",
        description: "Attack an NPC in the current room. The NPC will retaliate unless passive.",
        category: "Combat",
        admin_only: false,
    },
    HelpEntry {
        command: "flee",
        aliases: "fl, run",
        syntax: "flee",
        description: "Run from combat, escaping through the first available exit.",
        category: "Combat",
        admin_only: false,
    },
    // ── Information ───────────────────────────────────────────────────────────
    HelpEntry {
        command: "look",
        aliases: "l",
        syntax: "look",
        description: "Describe the current room, its contents, and exits.",
        category: "Information",
        admin_only: false,
    },
    HelpEntry {
        command: "examine",
        aliases: "x, ex",
        syntax: "examine <target>",
        description: "Examine an item, NPC, or object in detail.",
        category: "Information",
        admin_only: false,
    },
    HelpEntry {
        command: "score",
        aliases: "sc",
        syntax: "score",
        description: "Show your character stats, level, HP, and attributes.",
        category: "Information",
        admin_only: false,
    },
    HelpEntry {
        command: "balance",
        aliases: "bal, money, gold, wallet",
        syntax: "balance",
        description: "Show your current wealth.",
        category: "Information",
        admin_only: false,
    },
    HelpEntry {
        command: "quests",
        aliases: "quest, ql",
        syntax: "quests",
        description: "Show your active quest log.",
        category: "Information",
        admin_only: false,
    },
    // ── General ───────────────────────────────────────────────────────────────
    HelpEntry {
        command: "help",
        aliases: "h, ?",
        syntax: "help [command]",
        description: "Show this help, or type 'help <command>' for details on a specific command.",
        category: "General",
        admin_only: false,
    },
    HelpEntry {
        command: "quit",
        aliases: "exit, bye",
        syntax: "quit",
        description: "Save your progress and disconnect.",
        category: "General",
        admin_only: false,
    },
    // ── Admin: World ──────────────────────────────────────────────────────────
    HelpEntry {
        command: "@who",
        aliases: "",
        syntax: "@who",
        description: "List all connected players.",
        category: "Admin: World",
        admin_only: true,
    },
    HelpEntry {
        command: "@goto",
        aliases: "",
        syntax: "@goto <room_id>",
        description: "Teleport to a room by id.",
        category: "Admin: World",
        admin_only: true,
    },
    HelpEntry {
        command: "@roominfo",
        aliases: "",
        syntax: "@roominfo",
        description: "Show technical details about the current room.",
        category: "Admin: World",
        admin_only: true,
    },
    HelpEntry {
        command: "@dig",
        aliases: "",
        syntax: "@dig <direction> <room name>",
        description: "Create a new room in a direction and link it.",
        category: "Admin: World",
        admin_only: true,
    },
    HelpEntry {
        command: "@link",
        aliases: "",
        syntax: "@link <direction> <room_id>",
        description: "Add an exit from the current room in a direction.",
        category: "Admin: World",
        admin_only: true,
    },
    HelpEntry {
        command: "@unlink",
        aliases: "",
        syntax: "@unlink <direction>",
        description: "Remove an exit from the current room.",
        category: "Admin: World",
        admin_only: true,
    },
    HelpEntry {
        command: "@rename",
        aliases: "",
        syntax: "@rename <new name>",
        description: "Rename the current room.",
        category: "Admin: World",
        admin_only: true,
    },
    HelpEntry {
        command: "@redesc",
        aliases: "",
        syntax: "@redesc <new description>",
        description: "Rewrite the current room's description.",
        category: "Admin: World",
        admin_only: true,
    },
    // ── Admin: Items ──────────────────────────────────────────────────────────
    HelpEntry {
        command: "@mitem",
        aliases: "",
        syntax: "@mitem <name> [/ <description>]",
        description: "Create an item in the current room.",
        category: "Admin: Items",
        admin_only: true,
    },
    HelpEntry {
        command: "@destroy",
        aliases: "",
        syntax: "@destroy <item name or id>",
        description: "Permanently delete an item.",
        category: "Admin: Items",
        admin_only: true,
    },
    HelpEntry {
        command: "@iname",
        aliases: "",
        syntax: "@iname <id> <new name>",
        description: "Rename an item by id.",
        category: "Admin: Items",
        admin_only: true,
    },
    HelpEntry {
        command: "@idesc",
        aliases: "",
        syntax: "@idesc <id> <new description>",
        description: "Rewrite an item's description by id.",
        category: "Admin: Items",
        admin_only: true,
    },
    HelpEntry {
        command: "@islot",
        aliases: "",
        syntax: "@islot <id> <slot | none>",
        description: "Set or clear an item's equip slot.",
        category: "Admin: Items",
        admin_only: true,
    },
    HelpEntry {
        command: "@ireq",
        aliases: "",
        syntax: "@ireq <id> <stat> <value>",
        description: "Set an equip requirement (level, strength, dexterity, knowledge).",
        category: "Admin: Items",
        admin_only: true,
    },
    // ── Admin: NPCs ───────────────────────────────────────────────────────────
    HelpEntry {
        command: "@mnpc",
        aliases: "",
        syntax: "@mnpc <name> [/ <description>]",
        description: "Create an NPC in the current room.",
        category: "Admin: NPCs",
        admin_only: true,
    },
    HelpEntry {
        command: "@ndestroy",
        aliases: "",
        syntax: "@ndestroy <name or id>",
        description: "Permanently remove an NPC.",
        category: "Admin: NPCs",
        admin_only: true,
    },
    HelpEntry {
        command: "@nlist",
        aliases: "",
        syntax: "@nlist",
        description: "List all NPCs in the world with their ids and locations.",
        category: "Admin: NPCs",
        admin_only: true,
    },
    HelpEntry {
        command: "@ninfo",
        aliases: "",
        syntax: "@ninfo <id>",
        description: "Show full details for an NPC by id.",
        category: "Admin: NPCs",
        admin_only: true,
    },
    HelpEntry {
        command: "@nname",
        aliases: "",
        syntax: "@nname <id> <new name>",
        description: "Rename an NPC.",
        category: "Admin: NPCs",
        admin_only: true,
    },
    HelpEntry {
        command: "@ndesc",
        aliases: "",
        syntax: "@ndesc <id> <new description>",
        description: "Set an NPC's description.",
        category: "Admin: NPCs",
        admin_only: true,
    },
    HelpEntry {
        command: "@ngreet",
        aliases: "",
        syntax: "@ngreet <id> <text | none>",
        description: "Set or clear an NPC's greeting (shown on 'talk').",
        category: "Admin: NPCs",
        admin_only: true,
    },
    HelpEntry {
        command: "@nhostile",
        aliases: "",
        syntax: "@nhostile <id> <true | false>",
        description: "Toggle whether an NPC auto-attacks players on sight.",
        category: "Admin: NPCs",
        admin_only: true,
    },
    HelpEntry {
        command: "@npassive",
        aliases: "",
        syntax: "@npassive <id> <true | false>",
        description: "Toggle whether an NPC retaliates when attacked.",
        category: "Admin: NPCs",
        admin_only: true,
    },
    HelpEntry {
        command: "@npatrol",
        aliases: "",
        syntax: "@npatrol <id> <room,room,... | none>",
        description: "Set or clear an NPC's patrol route (comma-separated room ids).",
        category: "Admin: NPCs",
        admin_only: true,
    },
    // ── Admin: Quests ─────────────────────────────────────────────────────────
    HelpEntry {
        command: "@qlist",
        aliases: "",
        syntax: "@qlist",
        description: "List all quest definitions.",
        category: "Admin: Quests",
        admin_only: true,
    },
    HelpEntry {
        command: "@qinfo",
        aliases: "",
        syntax: "@qinfo <id>",
        description: "Show full details for a quest by id.",
        category: "Admin: Quests",
        admin_only: true,
    },
    HelpEntry {
        command: "@qgive",
        aliases: "",
        syntax: "@qgive <player name> <quest_id>",
        description: "Give a quest to a player by name.",
        category: "Admin: Quests",
        admin_only: true,
    },
    HelpEntry {
        command: "@qreset",
        aliases: "",
        syntax: "@qreset <player name> <quest_id>",
        description: "Reset a player's quest back to the start.",
        category: "Admin: Quests",
        admin_only: true,
    },
];

/// Returns the entry whose `command` or any comma-separated alias matches
/// `topic` (case-insensitive), filtered by admin visibility.
pub fn find_entry(topic: &str, is_admin: bool) -> Option<&'static HelpEntry> {
    let needle = topic.trim().to_lowercase();
    HELP_ENTRIES.iter().find(|e| {
        if e.admin_only && !is_admin {
            return false;
        }
        if e.command == needle {
            return true;
        }
        e.aliases
            .split(',')
            .any(|a| a.trim().to_lowercase() == needle)
    })
}
