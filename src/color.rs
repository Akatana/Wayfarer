/// Converts XML-like color markup to ANSI escape codes.
///
/// Supported opening tags: `<red>`, `<green>`, `<yellow>`, `<blue>`,
/// `<magenta>`, `<cyan>`, `<white>`, `<bold>`, `<dim>`.
/// Any matching closing tag (e.g. `</red>`) resets all attributes.
/// Unknown tags are passed through literally so arbitrary angle-bracket
/// content in room descriptions is not corrupted.
pub fn render(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 16);
    let mut rest = input;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        rest = &rest[open + 1..];
        if let Some(close) = rest.find('>') {
            let tag = &rest[..close];
            match ansi_for_tag(tag) {
                Some(code) => out.push_str(code),
                None => {
                    out.push('<');
                    out.push_str(tag);
                    out.push('>');
                }
            }
            rest = &rest[close + 1..];
        } else {
            // Unclosed `<` — emit literally and stop scanning.
            out.push('<');
            break;
        }
    }
    out.push_str(rest);
    out
}

/// Strips all recognized color tags, returning plain text.
/// Unknown tags are preserved. Useful for logging and plain-text clients.
pub fn strip(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        rest = &rest[open + 1..];
        if let Some(close) = rest.find('>') {
            let tag = &rest[..close];
            if ansi_for_tag(tag).is_none() {
                out.push('<');
                out.push_str(tag);
                out.push('>');
            }
            rest = &rest[close + 1..];
        } else {
            out.push('<');
            break;
        }
    }
    out.push_str(rest);
    out
}

fn ansi_for_tag(tag: &str) -> Option<&'static str> {
    if let Some(name) = tag.strip_prefix('/') {
        return match name {
            "red" | "green" | "yellow" | "blue" | "magenta" | "cyan" | "white" | "bold" | "dim" => {
                Some("\x1b[0m")
            }
            _ => None,
        };
    }
    match tag {
        "red" => Some("\x1b[31m"),
        "green" => Some("\x1b[32m"),
        "yellow" => Some("\x1b[33m"),
        "blue" => Some("\x1b[34m"),
        "magenta" => Some("\x1b[35m"),
        "cyan" => Some("\x1b[36m"),
        "white" => Some("\x1b[37m"),
        "bold" => Some("\x1b[1m"),
        "dim" => Some("\x1b[2m"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn red_tag_produces_ansi_red() {
        assert_eq!(render("<red>danger</red>"), "\x1b[31mdanger\x1b[0m");
    }

    #[test]
    fn multiple_tags_in_one_string() {
        assert_eq!(
            render("<green>go</green> or <red>stop</red>"),
            "\x1b[32mgo\x1b[0m or \x1b[31mstop\x1b[0m"
        );
    }

    #[test]
    fn unknown_tags_pass_through_literally() {
        assert_eq!(render("<b>html</b>"), "<b>html</b>");
    }

    #[test]
    fn unclosed_angle_bracket_passes_through() {
        assert_eq!(render("text <incomplete"), "text <incomplete");
    }

    #[test]
    fn plain_text_is_unchanged() {
        assert_eq!(render("no tags here"), "no tags here");
    }

    #[test]
    fn strip_removes_known_tags_leaves_unknown() {
        assert_eq!(strip("<red>danger</red> <b>keep</b>"), "danger <b>keep</b>");
    }

    #[test]
    fn strip_round_trips_to_plain_text() {
        assert_eq!(strip("<bold>Town Square</bold>"), "Town Square");
    }

    #[test]
    fn all_colors_render_without_eating_content() {
        for tag in &[
            "red", "green", "yellow", "blue", "magenta", "cyan", "white", "bold", "dim",
        ] {
            let input = format!("<{tag}>text</{tag}>");
            let out = render(&input);
            assert!(out.contains("text"), "{tag} ate the content");
            assert!(!out.contains(&format!("<{tag}>")), "{tag} was not replaced");
        }
    }

    #[test]
    fn nested_tags_render_correctly() {
        let out = render("<bold><red>warning</red></bold>");
        assert_eq!(out, "\x1b[1m\x1b[31mwarning\x1b[0m\x1b[0m");
    }
}
