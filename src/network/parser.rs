use crate::command::Command;
use crate::direction::Direction;

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
        assert_eq!(parse("remove shield"), Command::Unequip("shield".to_string()));
        assert_eq!(parse("examine sword"), Command::Examine("sword".to_string()));
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
}
