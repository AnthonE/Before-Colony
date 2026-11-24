//! The colony's reach: the space round the ring and its hub where pilots' weapons are safe on one
//! another (`docs/DECISIONS.md`, "Suits fight each other away from the colony"), and the rule that
//! says who may harm whom, everywhere.
//!
//! The reach is [`REACH_M`] from the ring's hull and from the docking hub's drum, so the hub's
//! mouth, its bays and the dock off it are deep inside, and so is Fornax. Past it is open space.
//! Both are closed forms of where you ask: nothing is stored, nothing allocates, and the server,
//! the owner's prediction and every client's HUD answer alike (libm through `math`).
//!
//! [`harms`] and [`hostile`] decide every hit and every lock. Under [`Law::Sides`] (arcade, the
//! tests' default) they're the sides' old rule exactly: a suit harms another side's. Under
//! [`Law::OpenSpace`], two pilots (people or agents) harm each other only when both are out in
//! open space, whatever their sides; inside the reach, or firing from inside it, nothing a pilot
//! does lands on another pilot. Drones are the Almagest's machines and fight by side wherever they
//! are, and pilots fight them by side.

use bc_proto::{Faction, PilotKind};
use glam::Vec3;

use super::hub::{DRUM_RADIUS, DRUM_X};
use crate::math::sqrt;
use crate::world::{COLONY_CENTER, colony_clearance};

/// How far the colony's reach extends from the ring's hull and the hub's drum, m (the owner's
/// default, `docs/DECISIONS.md`). Fornax, 3.0 to 4.1 km from the hull, is inside it; a value
/// between those would cut it in two, which `tests::no_landmark_straddles_the_line` refuses.
pub const REACH_M: f32 = 5_000.0;

/// How far `p` is outside the docking hub's drum (a solid of revolution on the colony's axis, so
/// its turn doesn't matter), m; negative inside it.
pub fn hub_clearance(p: Vec3) -> f32 {
    let rel = p - COLONY_CENTER;
    let d = sqrt(rel.y * rel.y + rel.z * rel.z);
    let along = (DRUM_X.0 - rel.x).max(rel.x - DRUM_X.1);
    let across = d - DRUM_RADIUS;
    if along > 0.0 && across > 0.0 { sqrt(along * along + across * across) } else { along.max(across) }
}

/// How far `p` is from the colony (the nearer of its ring and its hub), m; negative inside either.
pub fn clearance(p: Vec3) -> f32 {
    colony_clearance(p).min(hub_clearance(p))
}

/// Whether `p` is in the colony's reach: within [`REACH_M`] of the ring's hull or the hub's drum.
pub fn in_reach(p: Vec3) -> bool {
    clearance(p) <= REACH_M
}

/// How far `p` is from the reach's edge, m: positive in open space (that far out), negative inside
/// it (that far to go to open space).
pub fn past_line(p: Vec3) -> f32 {
    clearance(p) - REACH_M
}

/// The reach's outer edge round the colony's axis at `x` along it from the ring's middle (the
/// colony's frame, −X toward the hub): its radius from the axis, m, or `None` past its ends. The
/// edge of whichever reaches further there, the ring's or the hub's (the chart draws it).
pub fn outline(x: f32) -> Option<f32> {
    use crate::world::{COLONY_HALF_WIDTH, COLONY_RADIUS};
    let round = |off: f32, r: f32| {
        let a = off.max(0.0);
        (a <= REACH_M).then(|| r + sqrt(REACH_M * REACH_M - a * a))
    };
    let ring = round(x.abs() - COLONY_HALF_WIDTH, COLONY_RADIUS);
    let hub = round((DRUM_X.0 - x).max(x - DRUM_X.1), DRUM_RADIUS);
    match (ring, hub) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

/// The law a sector keeps between pilots (`SimConfig::law`). Clients are told in their Welcome.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Law {
    /// Sides: a suit harms another side's (and its own too, with friendly fire on). Arcade's, and
    /// every test's unless it says otherwise.
    #[default]
    Sides,
    /// Open space: pilots harm pilots of any side when both are outside the colony's reach, and
    /// none inside it; drones and pilots fight by side anywhere.
    OpenSpace,
}

/// Who a shooter or a target is to the law: its side, whether it's a drone, and whether it's out
/// in open space (a shooter where it fired from, a target where it's struck).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Party {
    pub faction: Faction,
    pub drone: bool,
    pub open: bool,
}

impl Party {
    /// A suit of `faction` flown by `pilot`, at `p`.
    pub fn at(faction: Faction, pilot: PilotKind, p: Vec3) -> Self {
        Self { faction, drone: pilot == PilotKind::Drone, open: !in_reach(p) }
    }

    /// A pilot of `faction` out in open space (tests, tools).
    pub const fn pilot(faction: Faction) -> Self {
        Self { faction, drone: false, open: true }
    }

    /// A drone of `faction`.
    pub const fn drone(faction: Faction) -> Self {
        Self { faction, drone: true, open: true }
    }
}

/// Whether `by` may set its sights on `on` under `law`: lock it, have its ward stop `on`'s beams,
/// count it a threat. Friendly fire changes none of this (a ward never stops an ally's beam, and
/// an ally is never designated).
pub fn hostile(law: Law, by: Party, on: Party) -> bool {
    let sides = by.faction != on.faction;
    if by.drone || on.drone {
        return sides;
    }
    match law {
        Law::Sides => sides,
        Law::OpenSpace => by.open && on.open,
    }
}

/// Whether `on` hides from `by` what it hides from its enemies (a jammer, being parked, hidden or
/// cold) under `law`: `by` is of another side, or `on` is a pilot out in open space and `by` a
/// pilot who could hunt it there (wherever `by` is now: one parked just inside the line would
/// otherwise watch a sleeper at an ally's sight, then cross and strike). Sensors, seekers and the
/// jammer's shimmer go by it, so a hide spot protects a sleeper from a same-side hunter as it
/// does from another side's. Drones go by side.
pub fn unfriendly(law: Law, by: Party, on: Party) -> bool {
    by.faction != on.faction || (law == Law::OpenSpace && on.open && !on.drone && !by.drone)
}

/// Whether a blow `by` strikes lands on `on` under `law`, friendly fire `ff` on or off: every hit
/// site asks this.
pub fn harms(law: Law, ff: bool, by: Party, on: Party) -> bool {
    ff || hostile(law, by, on)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::hub::{BAYS, MOUTH_X, bay_pose};
    use crate::content::landmarks::LANDMARKS;
    use crate::content::salvage::DOCK_CENTER;
    use crate::field::{FIELD_CENTER, SPAWN_BASES};
    use crate::world::{COLONY_HALF_WIDTH, COLONY_RADIUS};

    const FACTIONS: [Faction; 3] = [Faction::Almagest, Faction::Colonies, Faction::Alliance];

    fn parties() -> impl Iterator<Item = Party> {
        FACTIONS.into_iter().flat_map(|faction| {
            [(false, false), (false, true), (true, false), (true, true)]
                .into_iter()
                .map(move |(drone, open)| Party { faction, drone, open })
        })
    }

    #[test]
    fn sides_is_the_old_rule_exactly() {
        for by in parties() {
            for on in parties() {
                let differ = by.faction != on.faction;
                assert_eq!(hostile(Law::Sides, by, on), differ, "{by:?} → {on:?}");
                for ff in [false, true] {
                    assert_eq!(harms(Law::Sides, ff, by, on), ff || differ, "{by:?} → {on:?}, ff {ff}");
                }
            }
        }
    }

    #[test]
    fn open_space_pilots_fight_pilots_only_outside() {
        let (a, b) = (Party::pilot(Faction::Colonies), Party::pilot(Faction::Colonies));
        assert!(harms(Law::OpenSpace, false, a, b), "same side, both out");
        let inside = Party { open: false, ..b };
        assert!(!harms(Law::OpenSpace, false, a, inside), "into the reach");
        assert!(!harms(Law::OpenSpace, false, inside, a), "out of the reach");
        assert!(!harms(Law::OpenSpace, false, inside, inside), "both in");
        let alliance = Party { faction: Faction::Alliance, open: false, drone: false };
        assert!(!harms(Law::OpenSpace, false, alliance, inside), "the reach shelters from every side");
        // Drones fight by side, in the reach or out.
        let drone = Party { open: false, ..Party::drone(Faction::Almagest) };
        assert!(harms(Law::OpenSpace, false, drone, inside));
        assert!(harms(Law::OpenSpace, false, inside, drone));
        assert!(!harms(Law::OpenSpace, false, drone, Party::drone(Faction::Almagest)));
        // Friendly fire still lets everything land; it never makes a lock or a ward hostile.
        assert!(harms(Law::OpenSpace, true, inside, inside));
        assert!(!hostile(Law::OpenSpace, inside, a));
    }

    #[test]
    fn a_sleeper_out_in_open_space_hides_from_its_own_side_on_either_side_of_the_line() {
        for by in parties() {
            for on in parties() {
                assert_eq!(unfriendly(Law::Sides, by, on), by.faction != on.faction, "{by:?} → {on:?}");
            }
        }
        let out = Party::pilot(Faction::Colonies);
        let inside = Party { open: false, ..out };
        assert!(unfriendly(Law::OpenSpace, out, out), "a hunter out with it");
        assert!(unfriendly(Law::OpenSpace, inside, out), "one just inside the line, who could cross");
        assert!(!unfriendly(Law::OpenSpace, out, inside), "a pilot the reach shelters hides nothing");
        assert!(!unfriendly(Law::OpenSpace, Party::drone(Faction::Colonies), out), "drones by side");
        assert!(!unfriendly(Law::OpenSpace, out, Party::drone(Faction::Colonies)), "drones by side");
        assert!(unfriendly(Law::OpenSpace, Party { faction: Faction::Alliance, ..inside }, inside));
    }

    #[test]
    fn the_hub_the_bays_and_the_dock_are_in_reach() {
        let mouth = COLONY_CENTER + Vec3::X * MOUTH_X;
        assert!(in_reach(mouth), "the hub's mouth (the deck hatch)");
        assert!(in_reach(DOCK_CENTER), "the dock");
        assert!(in_reach(COLONY_CENTER), "the ring's middle");
        for n in 1..=BAYS as u8 {
            for t in (0..2_000).step_by(97) {
                assert!(in_reach(bay_pose(n, t, 0.0).pos), "bay {n} at {t}");
            }
        }
        // 5 km straight out of the mouth, the launch lane, is still in; past it is open space.
        assert!(in_reach(mouth - Vec3::X * REACH_M), "5 km out of the mouth");
        assert!(!in_reach(mouth - Vec3::X * (REACH_M + 50.0)), "past 5 km out of the mouth");
        // 5 km off the hull, round the ring.
        let off_hull = COLONY_CENTER + Vec3::Z * (COLONY_RADIUS + REACH_M - 10.0);
        assert!(in_reach(off_hull));
        assert!(!in_reach(off_hull + Vec3::Z * 20.0));
        assert!(!in_reach(COLONY_CENTER + Vec3::X * (COLONY_HALF_WIDTH + REACH_M + 10.0)), "aft");
        assert!((past_line(off_hull) + 10.0).abs() < 0.01);
    }

    #[test]
    fn the_field_the_bases_and_the_spawns_are_open_space() {
        assert!(!in_reach(FIELD_CENTER));
        for b in SPAWN_BASES {
            assert!(!in_reach(b), "{b}");
        }
        // The sides' spawns in arcade.
        for p in [Vec3::new(-4_200.0, 700.0, -3_200.0), Vec3::new(4_200.0, 700.0, -3_200.0)] {
            assert!(!in_reach(p), "{p}");
        }
    }

    /// The outline the chart draws is the reach's edge: every point round it is [`REACH_M`] from
    /// the colony, and it ends where the reach does.
    #[test]
    fn the_outline_is_the_edge() {
        use crate::math::{cos, sin};
        let mut x = -7_000.0;
        while x <= 7_000.0 {
            match outline(x) {
                Some(r) => {
                    for k in 0..8 {
                        let a = k as f32 * 0.785;
                        let p = COLONY_CENTER + Vec3::new(x, r * cos(a), r * sin(a));
                        assert!((clearance(p) - REACH_M).abs() < 1.0, "x {x}: {}", clearance(p));
                    }
                }
                None => assert!(!in_reach(COLONY_CENTER + Vec3::X * x), "x {x}"),
            }
            x += 125.0;
        }
        assert_eq!(outline(0.0), Some(COLONY_RADIUS + REACH_M));
        assert!(outline(MOUTH_X - REACH_M + 1.0).is_some() && outline(MOUTH_X - REACH_M - 1.0).is_none());
    }

    /// No landmark straddles the line: each is wholly in the reach or wholly out of it wherever its
    /// orbit takes it (clearance moves no faster than distance), so Fornax is in at 5 km.
    #[test]
    fn no_landmark_straddles_the_line() {
        for d in LANDMARKS.iter().filter(|d| !d.colony) {
            let c = clearance(d.center);
            let swept = d.orbit_radius + d.bound;
            let wholly_out = c - swept > REACH_M;
            let wholly_in = c + swept <= REACH_M;
            assert!(wholly_in || wholly_out, "{} straddles the reach: {c} ± {swept}", d.name);
            if d.name == "FORNAX" {
                assert!(wholly_in, "Fornax is in the colony's reach");
            }
        }
    }
}
