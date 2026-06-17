// ── Runtime item types ────────────────────────────────────────────────────────

/// Stat requirements that must be met before an item can be equipped.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Deserialize)]
pub struct EquipRequirements {
    #[serde(default)]
    pub level: i32,
    #[serde(default)]
    pub strength: i32,
    #[serde(default)]
    pub dexterity: i32,
    #[serde(default)]
    pub knowledge: i32,
}

impl EquipRequirements {
    pub fn is_met_by(&self, stats: &crate::components::Stats) -> bool {
        stats.level >= self.level
            && stats.strength >= self.strength
            && stats.dexterity >= self.dexterity
            && stats.knowledge >= self.knowledge
    }

    pub fn has_any(&self) -> bool {
        self.level > 0 || self.strength > 0 || self.dexterity > 0 || self.knowledge > 0
    }

    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.level > 0 {
            parts.push(format!("Lv {}", self.level));
        }
        if self.strength > 0 {
            parts.push(format!("STR {}", self.strength));
        }
        if self.dexterity > 0 {
            parts.push(format!("DEX {}", self.dexterity));
        }
        if self.knowledge > 0 {
            parts.push(format!("KNW {}", self.knowledge));
        }
        parts.join(", ")
    }
}

/// Where an item currently lives.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemLocation {
    Room(u64),
    Inventory { char_id: i64 },
    Equipped { char_id: i64, slot: EquipSlot },
}

impl ItemLocation {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            ItemLocation::Room(_) => "room",
            ItemLocation::Inventory { .. } => "inventory",
            ItemLocation::Equipped { .. } => "equipped",
        }
    }
}

/// Full runtime description of an item, loaded from DB.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemData {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub equip_slot: Option<EquipSlot>,
    pub two_handed: bool,
    pub bag_capacity: Option<usize>,
    pub requirements: EquipRequirements,
    pub location: ItemLocation,
}

/// Queued item location update, drained between ticks like pending_saves.
pub struct ItemLocationSave {
    pub item_id: i64,
    pub location: ItemLocation,
}

// ── Equipment slot identifiers ────────────────────────────────────────────────

/// Equipment slot identifiers for the 16-slot paperdoll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EquipSlot {
    Head,
    Chest,
    Shoulders,
    Back,
    Gloves,
    Legs,
    Feet,
    /// Primary weapon hand.
    LeftHand,
    /// Shield / off-hand — blocked while a two-handed weapon is equipped.
    RightHand,
    Necklace,
    Ring1,
    Ring2,
    /// Bag slots — equipped bags expand the inventory limit.
    Bag1,
    Bag2,
    Bag3,
    Bag4,
}

impl EquipSlot {
    pub fn label(self) -> &'static str {
        match self {
            EquipSlot::Head => "Head",
            EquipSlot::Chest => "Chest",
            EquipSlot::Shoulders => "Shoulders",
            EquipSlot::Back => "Back",
            EquipSlot::Gloves => "Gloves",
            EquipSlot::Legs => "Legs",
            EquipSlot::Feet => "Feet",
            EquipSlot::LeftHand => "Left Hand",
            EquipSlot::RightHand => "Right Hand",
            EquipSlot::Necklace => "Necklace",
            EquipSlot::Ring1 => "Ring 1",
            EquipSlot::Ring2 => "Ring 2",
            EquipSlot::Bag1 => "Bag 1",
            EquipSlot::Bag2 => "Bag 2",
            EquipSlot::Bag3 => "Bag 3",
            EquipSlot::Bag4 => "Bag 4",
        }
    }

    /// Display order for the equipment panel.
    pub const fn all() -> [EquipSlot; 16] {
        [
            EquipSlot::Head,
            EquipSlot::Necklace,
            EquipSlot::Shoulders,
            EquipSlot::Chest,
            EquipSlot::Back,
            EquipSlot::Gloves,
            EquipSlot::Legs,
            EquipSlot::Feet,
            EquipSlot::LeftHand,
            EquipSlot::RightHand,
            EquipSlot::Ring1,
            EquipSlot::Ring2,
            EquipSlot::Bag1,
            EquipSlot::Bag2,
            EquipSlot::Bag3,
            EquipSlot::Bag4,
        ]
    }

    /// Parses a player-typed slot name (case-insensitive, spaces ignored).
    pub fn from_str(s: &str) -> Option<EquipSlot> {
        match s.to_lowercase().replace(' ', "").as_str() {
            "head" | "helm" | "helmet" => Some(EquipSlot::Head),
            "chest" | "torso" | "body" | "armor" => Some(EquipSlot::Chest),
            "shoulders" | "shoulder" | "pauldrons" => Some(EquipSlot::Shoulders),
            "back" | "cape" | "cloak" => Some(EquipSlot::Back),
            "gloves" | "hands" | "gauntlets" => Some(EquipSlot::Gloves),
            "legs" | "pants" | "leggings" | "greaves" => Some(EquipSlot::Legs),
            "feet" | "boots" | "shoes" | "sandals" => Some(EquipSlot::Feet),
            "lefthand" | "lhand" | "mainhand" | "main" | "weapon" => Some(EquipSlot::LeftHand),
            "righthand" | "rhand" | "offhand" | "off" | "shield" => Some(EquipSlot::RightHand),
            "necklace" | "neck" | "amulet" => Some(EquipSlot::Necklace),
            "ring" | "ring1" => Some(EquipSlot::Ring1),
            "ring2" => Some(EquipSlot::Ring2),
            // "bag" auto-picks the first free slot (like "ring").
            "bag" | "bag1" => Some(EquipSlot::Bag1),
            "bag2" => Some(EquipSlot::Bag2),
            "bag3" => Some(EquipSlot::Bag3),
            "bag4" => Some(EquipSlot::Bag4),
            _ => None,
        }
    }
}

impl std::fmt::Display for EquipSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_contains_sixteen_unique_slots() {
        let slots = EquipSlot::all();
        assert_eq!(slots.len(), 16);
        let mut seen = std::collections::HashSet::new();
        for s in slots {
            assert!(seen.insert(s), "duplicate slot: {s}");
        }
    }

    #[test]
    fn from_str_parses_known_aliases() {
        assert_eq!(EquipSlot::from_str("head"), Some(EquipSlot::Head));
        assert_eq!(EquipSlot::from_str("LEFT HAND"), Some(EquipSlot::LeftHand));
        assert_eq!(EquipSlot::from_str("shield"), Some(EquipSlot::RightHand));
        assert_eq!(EquipSlot::from_str("ring"), Some(EquipSlot::Ring1));
        assert_eq!(EquipSlot::from_str("ring2"), Some(EquipSlot::Ring2));
        assert_eq!(EquipSlot::from_str("cloak"), Some(EquipSlot::Back));
    }

    #[test]
    fn from_str_returns_none_for_unknown() {
        assert_eq!(EquipSlot::from_str("banana"), None);
        assert_eq!(EquipSlot::from_str(""), None);
    }

    #[test]
    fn display_matches_label() {
        assert_eq!(EquipSlot::LeftHand.to_string(), "Left Hand");
        assert_eq!(EquipSlot::Ring1.to_string(), "Ring 1");
    }
}
