//! The key bindings, once: the title screen's controls sheet, the F1 overlay and the pause menu's
//! Controls page are all drawn from [`BINDINGS`], so they can't disagree with each other or with
//! the input code (the browser client's `input.rs`).

/// A group of bindings, in the order they're shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// In the hangar bay, on foot.
    OnFoot,
    Flight,
    Weapons,
    Salvage,
    System,
}

impl Group {
    pub const ALL: [Group; 5] = [Group::OnFoot, Group::Flight, Group::Weapons, Group::Salvage, Group::System];

    pub fn title(self) -> &'static str {
        match self {
            Group::OnFoot => "IN THE HANGAR",
            Group::Flight => "FLIGHT",
            Group::Weapons => "COMBAT",
            Group::Salvage => "SALVAGE",
            Group::System => "SYSTEM",
        }
    }
}

/// One line of the controls sheet.
#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub group: Group,
    /// The keys, as printed.
    pub keys: &'static str,
    pub action: &'static str,
}

const fn b(group: Group, keys: &'static str, action: &'static str) -> Binding {
    Binding { group, keys, action }
}

/// Every binding the pilot has.
pub const BINDINGS: &[Binding] = &[
    b(Group::OnFoot, "Mouse", "Look (click the game to take control)"),
    b(Group::OnFoot, "W / A / S / D", "Walk"),
    b(Group::OnFoot, "Shift", "Run"),
    b(Group::OnFoot, "Space", "Jump"),
    b(Group::OnFoot, "E", "Use: the fabricator, the stores, the exchange, the suit's console, the cockpit"),
    b(Group::Flight, "Mouse", "Aim (click the game to take control)"),
    b(Group::Flight, "W / S", "Thrust forward / back"),
    b(Group::Flight, "A / D", "Thrust left / right"),
    b(Group::Flight, "Space / C", "Thrust up / down"),
    b(Group::Flight, "Q / E", "Roll"),
    b(Group::Flight, "Shift", "Boost"),
    b(Group::Flight, "X", "Brake"),
    b(Group::Flight, "R", "RCS: fast turns (burns propellant)"),
    b(Group::Flight, "V", "Flight assist on/off (off: fully Newtonian)"),
    b(Group::Weapons, "Left mouse", "Primary weapon"),
    b(Group::Weapons, "Right mouse", "Secondary weapon"),
    b(Group::Weapons, "F", "Melee: saber, scythe, shotels, glaive, knife"),
    b(Group::Weapons, "H", "Special: Neo-Bird, Hyper Jammer, Full Open Attack, Cross Crusher"),
    b(Group::Weapons, "Z", "ZERO System on/off"),
    b(Group::Weapons, "Hold on target", "Missile lock (fire once it reads LOCKED)"),
    b(Group::Salvage, "G", "Grab on/off: the free hand takes what it touches"),
    b(Group::Salvage, "B", "Stow what's in hand"),
    b(Group::Salvage, "T", "Throw"),
    b(Group::Salvage, "J", "Jettison the hold"),
    b(
        Group::Salvage,
        "Enter",
        "Dock: at rest inside the dock's ring of lights (the colony's -X end), into your bay",
    ),
    b(Group::System, "Esc", "Menu"),
    b(Group::System, "F1", "This list"),
    b(Group::System, "F10", "Graphics quality"),
    b(
        Group::System,
        "1 - 6",
        "Arcade rules, when destroyed: relaunch as Leo, Wing Zero, Heavyarms, Deathscythe, Sandrock, Shenlong",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_group_has_bindings_and_no_key_repeats_within_one() {
        for g in Group::ALL {
            let keys: Vec<&str> = BINDINGS.iter().filter(|b| b.group == g).map(|b| b.keys).collect();
            assert!(!keys.is_empty(), "{g:?}");
            let mut sorted = keys.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), keys.len(), "{g:?} lists a key twice");
        }
    }

    #[test]
    fn the_sheet_is_in_group_order() {
        let order: Vec<usize> =
            BINDINGS.iter().map(|b| Group::ALL.iter().position(|g| *g == b.group).unwrap()).collect();
        assert!(order.windows(2).all(|w| w[0] <= w[1]));
    }
}
