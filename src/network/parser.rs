use crate::command::Command;
use crate::direction::Direction;

fn parse_direction_word(s: &str) -> Option<Direction> {
    s.parse::<Direction>().ok()
}

/// Parses a raw text line received from the network into a `Command`.
///
/// Supports full direction names and common single-letter MUD abbreviations.
/// Input is trimmed and matched case-insensitively. Unrecognised input
/// becomes `Command::Unknown` rather than an error.
pub fn parse(input: &str) -> Command {
    let input = input.trim();
    let (verb, rest) = match input.split_once(' ') {
        Some((v, r)) => (v, r.trim()),
        None => (input, ""),
    };

    match verb.to_lowercase().as_str() {
        "n" | "north" => Command::Move(Direction::North),
        "s" | "south" => Command::Move(Direction::South),
        "e" | "east" => Command::Move(Direction::East),
        "w" | "west" => Command::Move(Direction::West),
        "ne" | "northeast" => Command::Move(Direction::NorthEast),
        "nw" | "northwest" => Command::Move(Direction::NorthWest),
        "se" | "southeast" => Command::Move(Direction::SouthEast),
        "sw" | "southwest" => Command::Move(Direction::SouthWest),
        "u" | "up" => Command::Move(Direction::Up),
        "d" | "down" => Command::Move(Direction::Down),
        "l" | "look" => Command::Look,
        "get" | "take" => Command::Get(rest.to_string()),
        "drop" => Command::Drop(rest.to_string()),
        "inventory" | "inv" | "i" => Command::Inventory,
        "equip" | "wear" | "wield" => Command::Equip(rest.to_string()),
        "unequip" | "remove" | "unwear" | "unwield" => Command::Unequip(rest.to_string()),
        "examine" | "x" | "ex" => Command::Examine(rest.to_string()),
        "score" | "sc" => Command::Score,
        "say" => Command::Say(rest.to_string()),
        "quit" | "exit" | "bye" => Command::Quit,
        "@who" => Command::AdminWho,
        "@goto" => rest
            .parse::<u64>()
            .map(Command::AdminGoto)
            .unwrap_or(Command::Unknown(input.to_string())),
        "@dig" => {
            let (dir_str, name) = rest.split_once(' ').unwrap_or((rest, ""));
            match parse_direction_word(dir_str) {
                Some(dir) if !name.trim().is_empty() => {
                    Command::AdminDig(dir, name.trim().to_string())
                }
                _ => Command::Unknown(input.to_string()),
            }
        }
        "@link" => {
            let (dir_str, id_str) = rest.split_once(' ').unwrap_or((rest, ""));
            match (parse_direction_word(dir_str), id_str.trim().parse::<u64>()) {
                (Some(dir), Ok(id)) => Command::AdminLink(dir, id),
                _ => Command::Unknown(input.to_string()),
            }
        }
        "@unlink" => match parse_direction_word(rest) {
            Some(dir) => Command::AdminUnlink(dir),
            None => Command::Unknown(input.to_string()),
        },
        "@rename" if !rest.is_empty() => Command::AdminRename(rest.to_string()),
        "@redesc" if !rest.is_empty() => Command::AdminRedesc(rest.to_string()),
        "@roominfo" => Command::AdminRoomInfo,
        "@mitem" if !rest.is_empty() => Command::AdminMitem(rest.to_string()),
        "@destroy" if !rest.is_empty() => Command::AdminDestroy(rest.to_string()),
        "kill" | "attack" | "k" | "hit" if !rest.is_empty() => Command::Attack(rest.to_string()),
        "flee" | "fl" | "run" => Command::Flee,
        "talk" if !rest.is_empty() => Command::Talk(rest.to_string()),
        "@mnpc" if !rest.is_empty() => Command::AdminMnpc(rest.to_string()),
        "@ndestroy" if !rest.is_empty() => Command::AdminNdestroy(rest.to_string()),
        "@nlist" => Command::AdminNlist,
        "@ninfo" => rest
            .parse::<i64>()
            .map(Command::AdminNinfo)
            .unwrap_or(Command::Unknown(input.to_string())),
        "balance" | "money" | "gold" | "wallet" => Command::Balance,
        "quests" | "quest" | "ql" => Command::QuestLog,
        "help" | "h" | "?" => {
            if rest.is_empty() {
                Command::Help(None)
            } else {
                Command::Help(Some(rest.to_string()))
            }
        }
        "@qlist" => Command::AdminQlist,
        "@qinfo" => rest
            .parse::<i64>()
            .map(Command::AdminQinfo)
            .unwrap_or(Command::Unknown(input.to_string())),
        "@qgive" | "@qreset" => {
            let (name, id_str) = rest.split_once(' ').unwrap_or((rest, ""));
            match id_str.trim().parse::<i64>() {
                Ok(id) if !name.is_empty() => {
                    if verb == "@qgive" {
                        Command::AdminQgive(name.to_string(), id)
                    } else {
                        Command::AdminQreset(name.to_string(), id)
                    }
                }
                _ => Command::Unknown(input.to_string()),
            }
        }
        "@nname" | "@ndesc" | "@ngreet" | "@nhostile" | "@npassive" | "@npatrol" => {
            let (id_str, payload) = rest.split_once(' ').unwrap_or((rest, ""));
            match id_str.parse::<i64>() {
                Ok(id) if !payload.is_empty() => match &verb[1..] {
                    "nname" => Command::AdminNname(id, payload.to_string()),
                    "ndesc" => Command::AdminNdesc(id, payload.to_string()),
                    "ngreet" => Command::AdminNgreet(id, payload.to_string()),
                    "nhostile" => match payload.to_lowercase().as_str() {
                        "true" | "yes" | "1" => Command::AdminNhostile(id, true),
                        "false" | "no" | "0" => Command::AdminNhostile(id, false),
                        _ => Command::Unknown(input.to_string()),
                    },
                    "npassive" => match payload.to_lowercase().as_str() {
                        "true" | "yes" | "1" => Command::AdminNpassive(id, true),
                        "false" | "no" | "0" => Command::AdminNpassive(id, false),
                        _ => Command::Unknown(input.to_string()),
                    },
                    "npatrol" => Command::AdminNpatrol(id, payload.to_string()),
                    _ => Command::Unknown(input.to_string()),
                },
                _ => Command::Unknown(input.to_string()),
            }
        }
        "@ispawn" => rest
            .trim()
            .parse::<i64>()
            .map(Command::AdminIspawn)
            .unwrap_or(Command::Unknown(input.to_string())),
        "@idefs" => Command::AdminIdefs,
        "@iname" | "@idesc" | "@islot" | "@ireq" => {
            // All item-edit commands share the format: @cmd <id> <payload>
            let (id_str, payload) = rest.split_once(' ').unwrap_or((rest, ""));
            match id_str.parse::<i64>() {
                Ok(id) if !payload.is_empty() => match &verb[1..] {
                    "iname" => Command::AdminIname(id, payload.to_string()),
                    "idesc" => Command::AdminIdesc(id, payload.to_string()),
                    "islot" => Command::AdminIslot(id, payload.to_string()),
                    "ireq" => {
                        let (stat, val_str) = payload.split_once(' ').unwrap_or((payload, ""));
                        match val_str.trim().parse::<i32>() {
                            Ok(val) => Command::AdminIreq(id, stat.to_string(), val),
                            Err(_) => Command::Unknown(input.to_string()),
                        }
                    }
                    _ => Command::Unknown(input.to_string()),
                },
                _ => Command::Unknown(input.to_string()),
            }
        }
        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
            Command::DialogueChoice(verb.parse::<usize>().unwrap())
        }
        _ => Command::Unknown(input.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_letter_cardinal_directions() {
        assert_eq!(parse("n"), Command::Move(Direction::North));
        assert_eq!(parse("s"), Command::Move(Direction::South));
        assert_eq!(parse("e"), Command::Move(Direction::East));
        assert_eq!(parse("w"), Command::Move(Direction::West));
        assert_eq!(parse("u"), Command::Move(Direction::Up));
        assert_eq!(parse("d"), Command::Move(Direction::Down));
    }

    #[test]
    fn parses_full_direction_names() {
        assert_eq!(parse("north"), Command::Move(Direction::North));
        assert_eq!(parse("northeast"), Command::Move(Direction::NorthEast));
        assert_eq!(parse("southwest"), Command::Move(Direction::SouthWest));
    }

    #[test]
    fn parses_case_insensitively() {
        assert_eq!(parse("NORTH"), Command::Move(Direction::North));
        assert_eq!(parse("Look"), Command::Look);
        assert_eq!(parse("QUIT"), Command::Quit);
    }

    #[test]
    fn parses_say_with_message() {
        assert_eq!(
            parse("say hello there"),
            Command::Say("hello there".to_string())
        );
    }

    #[test]
    fn say_with_no_message_produces_empty_string() {
        assert_eq!(parse("say"), Command::Say(String::new()));
    }

    #[test]
    fn parses_look_aliases() {
        assert_eq!(parse("l"), Command::Look);
        assert_eq!(parse("look"), Command::Look);
    }

    #[test]
    fn parses_quit_aliases() {
        assert_eq!(parse("quit"), Command::Quit);
        assert_eq!(parse("exit"), Command::Quit);
        assert_eq!(parse("bye"), Command::Quit);
    }

    #[test]
    fn unknown_input_wraps_in_unknown_variant() {
        assert_eq!(
            parse("frobnicate"),
            Command::Unknown("frobnicate".to_string())
        );
    }

    #[test]
    fn trims_leading_and_trailing_whitespace() {
        assert_eq!(parse("  north  "), Command::Move(Direction::North));
        assert_eq!(parse("  look  "), Command::Look);
    }

    #[test]
    fn parses_item_commands() {
        assert_eq!(parse("get sword"), Command::Get("sword".to_string()));
        assert_eq!(parse("take coin"), Command::Get("coin".to_string()));
        assert_eq!(parse("drop helm"), Command::Drop("helm".to_string()));
        assert_eq!(parse("inventory"), Command::Inventory);
        assert_eq!(parse("inv"), Command::Inventory);
        assert_eq!(parse("i"), Command::Inventory);
        assert_eq!(parse("equip sword"), Command::Equip("sword".to_string()));
        assert_eq!(parse("wear helm"), Command::Equip("helm".to_string()));
        assert_eq!(parse("unequip head"), Command::Unequip("head".to_string()));
        assert_eq!(
            parse("remove shield"),
            Command::Unequip("shield".to_string())
        );
        assert_eq!(
            parse("examine sword"),
            Command::Examine("sword".to_string())
        );
        assert_eq!(parse("x ring"), Command::Examine("ring".to_string()));
    }

    #[test]
    fn parses_score_command() {
        assert_eq!(parse("score"), Command::Score);
        assert_eq!(parse("sc"), Command::Score);
    }

    #[test]
    fn parses_admin_who_command() {
        assert_eq!(parse("@who"), Command::AdminWho);
    }

    #[test]
    fn parses_admin_goto() {
        assert_eq!(parse("@goto 5"), Command::AdminGoto(5));
        assert!(matches!(parse("@goto notanumber"), Command::Unknown(_)));
    }

    #[test]
    fn parses_admin_dig() {
        assert_eq!(
            parse("@dig north Tower of Stars"),
            Command::AdminDig(Direction::North, "Tower of Stars".to_string())
        );
        assert!(matches!(parse("@dig north"), Command::Unknown(_)));
        assert!(matches!(parse("@dig banana Room"), Command::Unknown(_)));
    }

    #[test]
    fn parses_admin_link() {
        assert_eq!(
            parse("@link east 3"),
            Command::AdminLink(Direction::East, 3)
        );
        assert!(matches!(parse("@link east"), Command::Unknown(_)));
    }

    #[test]
    fn parses_admin_unlink() {
        assert_eq!(
            parse("@unlink south"),
            Command::AdminUnlink(Direction::South)
        );
        assert!(matches!(parse("@unlink"), Command::Unknown(_)));
    }

    #[test]
    fn parses_admin_rename_and_redesc() {
        assert_eq!(
            parse("@rename Town Square"),
            Command::AdminRename("Town Square".to_string())
        );
        assert_eq!(
            parse("@redesc A vast open square."),
            Command::AdminRedesc("A vast open square.".to_string())
        );
        assert!(matches!(parse("@rename"), Command::Unknown(_)));
    }

    #[test]
    fn parses_admin_roominfo() {
        assert_eq!(parse("@roominfo"), Command::AdminRoomInfo);
    }

    #[test]
    fn parses_admin_mitem_and_destroy() {
        assert_eq!(
            parse("@mitem a silver key"),
            Command::AdminMitem("a silver key".to_string())
        );
        assert_eq!(
            parse("@destroy sword"),
            Command::AdminDestroy("sword".to_string())
        );
        assert!(matches!(parse("@mitem"), Command::Unknown(_)));
    }

    #[test]
    fn parses_admin_ispawn_and_idefs() {
        assert_eq!(parse("@ispawn 7"), Command::AdminIspawn(7));
        assert_eq!(parse("@idefs"), Command::AdminIdefs);
        assert!(matches!(parse("@ispawn notanumber"), Command::Unknown(_)));
        assert!(matches!(parse("@ispawn"), Command::Unknown(_)));
    }

    #[test]
    fn parses_admin_item_edit_commands() {
        assert_eq!(
            parse("@iname 42 a rusty blade"),
            Command::AdminIname(42, "a rusty blade".to_string())
        );
        assert_eq!(
            parse("@idesc 42 A sword forged in shadow."),
            Command::AdminIdesc(42, "A sword forged in shadow.".to_string())
        );
        assert_eq!(
            parse("@islot 42 lefthand"),
            Command::AdminIslot(42, "lefthand".to_string())
        );
        assert_eq!(
            parse("@islot 42 none"),
            Command::AdminIslot(42, "none".to_string())
        );
        assert_eq!(
            parse("@ireq 42 str 5"),
            Command::AdminIreq(42, "str".to_string(), 5)
        );
        assert_eq!(
            parse("@ireq 42 level 3"),
            Command::AdminIreq(42, "level".to_string(), 3)
        );
        // Bad inputs
        assert!(matches!(parse("@iname notanid name"), Command::Unknown(_)));
        assert!(matches!(
            parse("@ireq 42 str notanumber"),
            Command::Unknown(_)
        ));
        assert!(matches!(parse("@iname 42"), Command::Unknown(_)));
    }
}
