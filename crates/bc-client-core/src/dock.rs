//! Where the pilot's suit docks, and what Enter does in it.
//!
//! Under survival rules a suit goes home by docking (`Sim::docked`, the own state's DOCKED flag):
//! at rest in the dock's ring of lights off the docking hub's mouth, or standing on the deck hatch
//! in that mouth; inside the colony, at rest in the inner gate's ring; a trainer, at rest on the
//! Blast Hall's gantry. The HUD marks where ([`Home`]) and says ENTER there.
//!
//! In a suit, Enter asks to dock where the suit is docked or coming in to dock (within
//! [`Home::approach`] of it, where the HUD's marker says to come to rest), so the dock's refusals
//! ("come to rest inside ...") come where they teach something. Anywhere else, and under arcade
//! rules (where the dock buys a hold by itself), Enter opens the radio, as it does on foot
//! (the browser's `chat.rs`); on the chart it sets a course. [`EnterKey`] routes each press.

use bc_proto::snapshot::own_flags;
use bc_sim::colony::{hall, interior};
use bc_sim::content::salvage::{DOCK_CENTER, DOCK_RADIUS};
use glam::Vec3;

use crate::ClientCore;

/// Where a suit docks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Home {
    /// The dock's ring of lights off the docking hub's mouth (its deck hatch is in the mouth).
    Dock,
    /// The inner gate's ring, inside the colony.
    InnerGate,
    /// A trainer's: its pad on the Blast Hall's gantry.
    Gantry,
}

impl Home {
    /// Where a suit docks: a trainer on its gantry; else inside the colony the inner gate, and out
    /// in the sector the dock.
    pub fn of(interior: bool, trainer: bool) -> Self {
        match (interior, trainer) {
            (_, true) => Home::Gantry,
            (true, false) => Home::InnerGate,
            (false, false) => Home::Dock,
        }
    }

    /// Its middle, in the sector's frame (inside the colony, the colony's own).
    pub fn at(self) -> Vec3 {
        match self {
            Home::Dock => DOCK_CENTER,
            Home::InnerGate => interior::INNER_GATE,
            Home::Gantry => hall::gantry(),
        }
    }

    /// Its name on the HUD's marker.
    pub fn name(self) -> &'static str {
        match self {
            Home::Dock => "DOCK",
            Home::InnerGate => "INNER GATE",
            Home::Gantry => "GANTRY",
        }
    }

    /// What docking there does, on the marker.
    pub fn verb(self) -> &'static str {
        match self {
            Home::Gantry => "climb out",
            _ => "home",
        }
    }

    /// Where a suit comes to rest to dock there, on the marker while it comes in.
    pub fn rest(self) -> &'static str {
        match self {
            Home::Gantry => "AT REST ON ITS PAD",
            _ => "AT REST IN ITS RING",
        }
    }

    /// How near its middle a suit is coming in to dock, m: twice its ring's radius (the gantry's
    /// pad's). The dock's is its ring and 100 m more: the hub's mouth and its deck hatch, but not
    /// the bays' doors round it, which a launch comes out of.
    pub fn approach(self) -> f32 {
        match self {
            Home::Dock => DOCK_RADIUS + 100.0,
            Home::InnerGate => 2.0 * interior::INNER_GATE_RADIUS,
            Home::Gantry => 2.0 * hall::GANTRY_RADIUS,
        }
    }

    /// Whether a suit at `pos` is coming in to dock here.
    pub fn near(self, pos: Vec3) -> bool {
        pos.distance_squared(self.at()) <= self.approach() * self.approach()
    }

    /// Whether Enter asks to dock here: the suit docked (the own state's flag), or at `pos`
    /// coming in.
    pub fn enter_docks(self, docked: bool, pos: Option<Vec3>) -> bool {
        docked || pos.is_some_and(|p| self.near(p))
    }
}

/// Where a press of Enter goes. It's decided once a frame, before anything acts on it (the
/// browser's radio decides, and its docking and the docking hints read what was decided), so one
/// press never does both, nor neither, though the suit moves on between them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EnterKey {
    /// Not pressed, or nothing here takes it: a panel up, a sequence playing, the chart (which
    /// keeps it for a course).
    #[default]
    Unused,
    /// The suit asks to dock.
    Dock,
    /// The radio's line opens.
    Radio,
}

impl EnterKey {
    /// A press of Enter, with the keys the game's (`free`: in the world, no panel up, the radio's
    /// line shut): walking (`on_foot`), the radio's; in a suit (`in_suit`: off the chart and out of
    /// the bay's sequences), the dock's where the suit docks ([`enter_docks`]: `docks`), the
    /// radio's anywhere else.
    pub fn route(pressed: bool, free: bool, on_foot: bool, in_suit: bool, docks: bool) -> Self {
        if !pressed || !free {
            EnterKey::Unused
        } else if on_foot {
            EnterKey::Radio
        } else if in_suit {
            if docks { EnterKey::Dock } else { EnterKey::Radio }
        } else {
            EnterKey::Unused
        }
    }
}

/// Where the pilot's own suit docks.
pub fn home(core: &ClientCore) -> Home {
    Home::of(core.predict.interior(), core.hangar.trainer)
}

/// Whether Enter, in the pilot's own suit, asks to dock (else it opens the radio): under survival
/// rules, the suit alive, and [`Home::enter_docks`] where it docks, as drawn this frame. What the
/// docking and the radio both read, and the HUD's marker says.
pub fn enter_docks(core: &ClientCore) -> bool {
    let survival = core.welcome.is_some_and(|w| w.survival);
    let Some(own) = core.world.own.filter(|o| o.alive && survival) else { return false };
    let pos = core.own_view().filter(|v| v.alive).map(|v| v.pos);
    home(core).enter_docks(own.flags & own_flags::DOCKED != 0, pos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::frame::{CityPos, STRIP_WIDTH};
    use bc_sim::colony::hub::{BAY_DOOR_X, BAY_RADIUS, DECK_HATCH_RADIUS, MOUTH_X};
    use bc_sim::field::FIELD_CENTER;
    use bc_sim::world::COLONY_CENTER;

    /// One press goes one place: on foot to the radio; in a suit to the dock where it docks, else
    /// to the radio; nowhere with a panel up, on the chart or in a sequence.
    #[test]
    fn one_press_of_enter_goes_one_place() {
        use EnterKey::{Dock, Radio, Unused};
        let route = EnterKey::route;
        assert_eq!(route(true, true, true, false, false), Radio, "on foot");
        assert_eq!(route(true, true, true, false, true), Radio, "on foot, though the suit is docked");
        assert_eq!(route(true, true, false, true, true), Dock, "coming in to dock");
        assert_eq!(route(true, true, false, true, false), Radio, "out in the field");
        assert_eq!(route(true, true, false, false, true), Unused, "on the chart, or in a sequence");
        assert_eq!(route(true, false, false, true, true), Unused, "a panel up, or the line open");
        assert_eq!(route(true, false, true, false, false), Unused, "a terminal's panel");
        assert_eq!(route(false, true, false, true, true), Unused, "not pressed");
        assert_eq!(EnterKey::default(), Unused);
    }

    #[test]
    fn a_trainer_docks_on_its_gantry_and_a_suit_inside_at_the_inner_gate() {
        assert_eq!(Home::of(false, false), Home::Dock);
        assert_eq!(Home::of(true, false), Home::InnerGate);
        assert_eq!(Home::of(true, true), Home::Gantry);
        assert_eq!(Home::Gantry.verb(), "climb out");
        assert_eq!(Home::InnerGate.verb(), "home");
    }

    /// Out in the sector: in the ring (at rest or not) and on the docking hub's mouth, Enter docks
    /// (or is told to come to rest); out by the bays' doors, where a launch throws a suit, and
    /// over the field, it doesn't.
    #[test]
    fn out_in_the_sector_enter_docks_at_the_dock_and_its_deck_hatch_only() {
        let dock = Home::Dock;
        assert!(dock.enter_docks(true, None), "docked, wherever it's drawn");
        assert!(dock.enter_docks(false, Some(DOCK_CENTER + Vec3::Y * (DOCK_RADIUS * 0.9))));
        assert!(dock.enter_docks(false, Some(DOCK_CENTER - Vec3::X * DOCK_RADIUS * 1.5)), "coming in");
        // The deck hatch, in the middle of the mouth.
        let mouth = COLONY_CENTER + Vec3::X * MOUTH_X;
        assert!(dock.near(mouth) && dock.near(mouth + Vec3::Y * DECK_HATCH_RADIUS));
        let door = COLONY_CENTER + Vec3::new(BAY_DOOR_X, BAY_RADIUS, 0.0);
        assert!(!dock.enter_docks(false, Some(door)), "out of the bay's door");
        assert!(!dock.enter_docks(false, Some(FIELD_CENTER)));
        assert!(!dock.enter_docks(false, None));
    }

    #[test]
    fn inside_the_colony_enter_docks_at_the_inner_gate_or_a_trainers_gantry() {
        let gate = Home::InnerGate;
        assert!(gate.enter_docks(false, Some(interior::INNER_GATE + Vec3::X * 150.0)));
        let down_the_colony = CityPos::new(0, 0.0, STRIP_WIDTH * 0.5, 300.0).to_colony();
        assert!(!gate.enter_docks(false, Some(down_the_colony)));
        let pad = Home::Gantry;
        assert!(pad.enter_docks(false, Some(hall::gantry() + Vec3::new(0.0, 0.0, 20.0))));
        // Docked hovering high over the pad (the flag says so), and away across the hall at its
        // targets, where Enter is the radio's.
        assert!(pad.enter_docks(true, Some(hall::gantry())));
        let target = hall::target(0, 0, 0.0);
        assert!(!pad.near(target), "{}", target.distance(hall::gantry()));
    }
}
