/// All cardinal, ordinal, and vertical directions for room-based navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
    Up,
    Down,
}

impl Direction {
    /// Parses a lowercase direction string as stored in the database.
    /// Mirrors the `Display` impl — the two must stay in sync.
    pub fn from_str(s: &str) -> Option<Direction> {
        match s {
            "north"     => Some(Direction::North),
            "south"     => Some(Direction::South),
            "east"      => Some(Direction::East),
            "west"      => Some(Direction::West),
            "northeast" => Some(Direction::NorthEast),
            "northwest" => Some(Direction::NorthWest),
            "southeast" => Some(Direction::SouthEast),
            "southwest" => Some(Direction::SouthWest),
            "up"        => Some(Direction::Up),
            "down"      => Some(Direction::Down),
            _           => None,
        }
    }

    /// Returns the logical inverse of this direction (the exit you'd use to return).
    pub fn opposite(self) -> Direction {
        match self {
            Direction::North     => Direction::South,
            Direction::South     => Direction::North,
            Direction::East      => Direction::West,
            Direction::West      => Direction::East,
            Direction::NorthEast => Direction::SouthWest,
            Direction::NorthWest => Direction::SouthEast,
            Direction::SouthEast => Direction::NorthWest,
            Direction::SouthWest => Direction::NorthEast,
            Direction::Up        => Direction::Down,
            Direction::Down      => Direction::Up,
        }
    }
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Direction::North     => "north",
            Direction::South     => "south",
            Direction::East      => "east",
            Direction::West      => "west",
            Direction::NorthEast => "northeast",
            Direction::NorthWest => "northwest",
            Direction::SouthEast => "southeast",
            Direction::SouthWest => "southwest",
            Direction::Up        => "up",
            Direction::Down      => "down",
        };
        write!(f, "{}", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_of_north_is_south() {
        assert_eq!(Direction::North.opposite(), Direction::South);
    }

    #[test]
    fn opposite_of_northeast_is_southwest() {
        assert_eq!(Direction::NorthEast.opposite(), Direction::SouthWest);
    }

    #[test]
    fn opposite_is_its_own_inverse() {
        let dirs = [
            Direction::North, Direction::South, Direction::East, Direction::West,
            Direction::NorthEast, Direction::NorthWest, Direction::SouthEast,
            Direction::SouthWest, Direction::Up, Direction::Down,
        ];
        for dir in dirs {
            assert_eq!(dir.opposite().opposite(), dir, "{dir} failed round-trip");
        }
    }

    #[test]
    fn display_formats_correctly() {
        assert_eq!(Direction::North.to_string(), "north");
        assert_eq!(Direction::NorthEast.to_string(), "northeast");
        assert_eq!(Direction::Up.to_string(), "up");
    }
}
