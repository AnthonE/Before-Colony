//! What's damaged or failed inside a suit's parts, as the hangar keeps it: on the suit in the bay,
//! and on each part on the shelf (a part carries its systems' faults when it's stripped, and back
//! when it's fitted). In the records it's a map from system to level,
//! `{"reactor": "damaged", "tank": "failed"}`, empty when everything works.

use std::collections::BTreeMap;
use std::fmt;

use bc_proto::{FrameId, Part};
use bc_sim::content::salvage::is_gundam;
use bc_sim::content::systems::{DAMAGED, FAILED, OK};
use bc_sim::content::{System, Systems};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::catalogue::{COMPONENTS, ELECTRONICS, EXOTICS};
use crate::item::Item;

/// Each system's level (`bc_sim::content::Systems`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Faults(pub Systems);

impl Faults {
    pub const NONE: Faults = Faults(Systems::OK);

    pub fn is_empty(&self) -> bool {
        self.0.is_ok()
    }

    pub fn level(&self, sys: System) -> u8 {
        self.0.get(sys)
    }

    pub fn set(&mut self, sys: System, level: u8) {
        self.0.set(sys, level);
    }

    /// The faulted systems, in order, with their levels.
    pub fn iter(&self) -> impl Iterator<Item = (System, u8)> + '_ {
        System::ALL.into_iter().map(|s| (s, self.level(s))).filter(|(_, l)| *l != OK)
    }

    /// How many systems are faulted.
    pub fn count(&self) -> usize {
        self.iter().count()
    }

    /// Only `part`'s systems' faults.
    pub fn of_part(&self, part: Part) -> Faults {
        let mut out = Faults::NONE;
        for s in System::of_part(part) {
            out.set(s, self.level(s));
        }
        out
    }

    /// With `part`'s systems' faults replaced by `from`'s.
    pub fn with_part(mut self, part: Part, from: Faults) -> Faults {
        for s in System::of_part(part) {
            self.set(s, from.level(s));
        }
        self
    }

    /// Every system in `part` at `level`.
    pub fn all(part: Part, level: u8) -> Faults {
        Faults::NONE.with_part(part, {
            let mut f = Faults::NONE;
            for s in System::of_part(part) {
                f.set(s, level);
            }
            f
        })
    }
}

/// What restoring `sys` of a `line` suit from `level` to working takes: machined components and
/// electronics (twice and a half as much for a failed one), and a Gundam's exotic metals.
pub fn overhaul_cost(line: FrameId, level: u8) -> Vec<(Item, u64)> {
    let (components, electronics, exotics) = match level {
        DAMAGED => (20, 5, 5),
        FAILED => (50, 13, 10),
        _ => return Vec::new(),
    };
    let mut v = vec![(COMPONENTS, components), (ELECTRONICS, electronics)];
    if is_gundam(line) {
        v.push((EXOTICS, exotics));
    }
    v
}

fn level_name(level: u8) -> &'static str {
    match level {
        DAMAGED => "damaged",
        FAILED => "failed",
        _ => "ok",
    }
}

impl fmt::Display for Faults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for (s, l) in self.iter() {
            if !first {
                f.write_str(", ")?;
            }
            first = false;
            write!(f, "{} {}", s.name(), level_name(l))?;
        }
        Ok(())
    }
}

impl Serialize for Faults {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let map: BTreeMap<&str, &str> = self.iter().map(|(sys, l)| (sys.slug(), level_name(l))).collect();
        map.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Faults {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let map = BTreeMap::<String, String>::deserialize(d)?;
        let mut out = Faults::NONE;
        for (sys, level) in map {
            let sys = System::from_slug(&sys)
                .ok_or_else(|| serde::de::Error::custom(format!("no such system: {sys}")))?;
            let level = match level.as_str() {
                "damaged" => DAMAGED,
                "failed" => FAILED,
                "ok" => OK,
                _ => return Err(serde::de::Error::custom(format!("no such level: {level}"))),
            };
            out.set(sys, level);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faults_read_as_words_and_round_trip() {
        let mut f = Faults::NONE;
        f.set(System::Reactor, DAMAGED);
        f.set(System::Tank, FAILED);
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(json, r#"{"reactor":"damaged","tank":"failed"}"#);
        assert_eq!(serde_json::from_str::<Faults>(&json).unwrap(), f);
        assert_eq!(serde_json::to_string(&Faults::NONE).unwrap(), "{}");
        assert_eq!(f.of_part(Part::Torso), f);
        assert!(f.of_part(Part::Head).is_empty());
        assert_eq!(Faults::all(Part::Backpack, FAILED).count(), 2);
        assert_eq!(f.to_string(), "reactor damaged, propellant tank failed");
    }

    #[test]
    fn a_failed_system_costs_more_to_restore_and_a_gundams_more_still() {
        let leo = overhaul_cost(FrameId::Leo, DAMAGED);
        let worse = overhaul_cost(FrameId::Leo, FAILED);
        assert!(worse[0].1 > leo[0].1);
        assert!(overhaul_cost(FrameId::WingZero, DAMAGED).len() > leo.len());
        assert!(overhaul_cost(FrameId::Leo, OK).is_empty());
    }
}
