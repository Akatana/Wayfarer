/// Formats a raw copper value as a human-readable gold/silver/copper string.
///
/// 1g = 100s = 10,000c. Zero denominations are omitted except when the total
/// is 0, which renders as "0c".
///
/// Examples: 12345c → "1g 23s 45c", 100c → "1s", 10000c → "1g"
pub fn format_copper(copper: i64) -> String {
    if copper == 0 {
        return "0c".to_string();
    }
    let (copper, sign) = if copper < 0 {
        (-copper, "-")
    } else {
        (copper, "")
    };
    let g = copper / 10_000;
    let s = (copper % 10_000) / 100;
    let c = copper % 100;
    let mut parts = Vec::with_capacity(3);
    if g > 0 {
        parts.push(format!("{g}g"));
    }
    if s > 0 {
        parts.push(format!("{s}s"));
    }
    if c > 0 {
        parts.push(format!("{c}c"));
    }
    format!("{}{}", sign, parts.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_renders_as_zero_copper() {
        assert_eq!(format_copper(0), "0c");
    }

    #[test]
    fn pure_copper() {
        assert_eq!(format_copper(42), "42c");
    }

    #[test]
    fn pure_silver() {
        assert_eq!(format_copper(100), "1s");
    }

    #[test]
    fn pure_gold() {
        assert_eq!(format_copper(10_000), "1g");
    }

    #[test]
    fn mixed_denominations() {
        assert_eq!(format_copper(12345), "1g 23s 45c");
    }

    #[test]
    fn gold_and_silver_no_copper() {
        assert_eq!(format_copper(10_200), "1g 2s");
    }

    #[test]
    fn negative_value() {
        assert_eq!(format_copper(-150), "-1s 50c");
    }
}
