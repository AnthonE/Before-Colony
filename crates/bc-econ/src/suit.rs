//! The suit standing in the bay: the parts its pilot has fitted, the weapons on its mounts, what's
//! loaded and what's in the tank.
//!
//! The torso is the suit (the cockpit and reactor), so a suit exists once a torso is fitted.
//! Anything else may be missing: a suit can launch with no arms, and flies as a suit that lost
//! them would (the simulation already knows how). A weapon needs the part it hangs on: shot off,
//! the part takes its weapon with it.

use bc_proto::{FrameId, Part, WeaponKind};
use bc_sim::content::{ArmSlot, frame};
use bc_sim::sim::{Homecoming, Loadout};
use serde::{Deserialize, Serialize};

use crate::catalogue::{munitions_per_load, recipe, rounds_per_load, tank_kg};
use crate::item::{Item, line_serde, part_serde};
use crate::stores::PartUnit;

/// Where something is fitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Slot {
    Part {
        #[serde(with = "part_serde")]
        part: Part,
    },
    /// A loadout mount: 0 primary, 1 secondary, 2 melee.
    Mount { mount: u8 },
}

/// A suit in the bay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suit {
    #[serde(with = "line_serde")]
    pub line: FrameId,
    /// Each fitted part's condition (1..=100 %), by [`Part`]; `None` where nothing is fitted.
    pub parts: [Option<u8>; Part::COUNT],
    /// Weapons fitted on the loadout's mounts.
    pub mounts: [bool; 3],
    /// Rounds loaded, per mount.
    pub ammo: [u16; 3],
    /// Propellant in the tank, kg.
    pub propellant: u32,
}

/// The frame line a form belongs to (Neo-Bird is Wing Zero).
pub fn line_of(frame_id: FrameId) -> FrameId {
    match frame_id {
        FrameId::WingZeroBird => FrameId::WingZero,
        f => f,
    }
}

impl Suit {
    /// A suit built on `torso`: nothing else fitted yet, dry.
    pub fn on(torso: PartUnit) -> Self {
        let mut parts = [None; Part::COUNT];
        parts[Part::Torso as usize] = Some(torso.condition.clamp(1, 100));
        Self { line: torso.line, parts, mounts: [false; 3], ammo: [0; 3], propellant: 0 }
    }

    /// A new suit of `line`, everything fitted, loaded and fuelled.
    pub fn complete(line: FrameId) -> Self {
        let full = Loadout::full(line);
        Self {
            line,
            parts: [Some(100); Part::COUNT],
            mounts: [true; 3],
            ammo: full.ammo,
            propellant: tank_kg(line),
        }
        .with_mounts_of_its_loadout()
    }

    /// Clears mounts the frame doesn't have (a frame with two weapons has no third mount).
    fn with_mounts_of_its_loadout(mut self) -> Self {
        for m in 0..3 {
            self.mounts[m] &= self.weapon_on(m).is_some();
        }
        self
    }

    /// The weapon mount `m` carries on this frame.
    pub fn weapon_on(&self, m: usize) -> Option<WeaponKind> {
        frame(self.line).loadout.get(m).copied().flatten().map(|mount| mount.weapon)
    }

    fn mount_arm(&self, m: usize) -> Option<ArmSlot> {
        frame(self.line).loadout.get(m).copied().flatten().map(|mount| mount.arm)
    }

    /// Whether the part a mount hangs on is fitted (both arms, for a two-handed weapon).
    pub fn mount_has_its_part(&self, m: usize) -> bool {
        match self.mount_arm(m) {
            Some(ArmSlot::Both) => {
                self.parts[Part::ArmL as usize].is_some() && self.parts[Part::ArmR as usize].is_some()
            }
            Some(arm) => self.parts[arm.part() as usize].is_some(),
            None => false,
        }
    }

    /// Where `item` would go on this suit, if it fits and that place is free.
    pub fn slot_for(&self, item: Item) -> Option<Slot> {
        match item {
            Item::Part(line, part) if line == self.line && self.parts[part as usize].is_none() => {
                Some(Slot::Part { part })
            }
            Item::Weapon(w) => (0..3)
                .find(|m| self.weapon_on(*m) == Some(w) && !self.mounts[*m])
                .map(|m| Slot::Mount { mount: m as u8 }),
            _ => None,
        }
    }

    /// The tank's size, kg.
    pub fn tank(&self) -> u32 {
        tank_kg(self.line)
    }

    /// Rounds a full load holds on mount `m` (0: it fires energy, or it's a blade).
    pub fn full_load(&self, m: usize) -> u16 {
        self.weapon_on(m).filter(|w| munitions_per_load(*w) > 0).map_or(0, rounds_per_load)
    }

    /// What the simulation launches.
    pub fn loadout(&self) -> Loadout {
        let mut parts = [0.0; Part::COUNT];
        for (f, c) in parts.iter_mut().zip(self.parts) {
            *f = c.map_or(0.0, |c| f32::from(c) / 100.0);
        }
        let mut mounts = 0u8;
        for (m, fitted) in self.mounts.iter().enumerate() {
            if *fitted {
                mounts |= 1 << m;
            }
        }
        Loadout {
            parts,
            mounts,
            ammo: self.ammo,
            propellant: self.propellant as f32,
            systems: Default::default(),
        }
    }

    /// The suit as it came home: parts worn or gone (a weapon goes with the part it hung on),
    /// rounds fired, propellant burnt.
    pub fn came_home(&mut self, home: &Homecoming) {
        for (c, f) in self.parts.iter_mut().zip(home.parts) {
            *c = c.and_then(|_| (f > 0.0).then(|| ((f * 100.0).round() as u8).clamp(1, 100)));
        }
        // The torso is the suit: it came home, so it's there.
        let torso = &mut self.parts[Part::Torso as usize];
        *torso = Some(torso.unwrap_or(1));
        for m in 0..3 {
            self.mounts[m] = self.mounts[m] && home.mounts & (1 << m) != 0 && self.mount_has_its_part(m);
            self.ammo[m] = if self.mounts[m] { home.ammo[m].min(self.full_load(m)) } else { 0 };
        }
        self.propellant = (home.propellant.max(0.0) as u32).min(self.tank());
    }

    /// Parts fitted, and how many of them are worn.
    pub fn fitted(&self) -> impl Iterator<Item = (Part, u8)> + '_ {
        Part::ALL.into_iter().zip(self.parts).filter_map(|(p, c)| c.map(|c| (p, c)))
    }

    /// The suit's value as parts and weapons, at new prices, scaled by condition.
    pub fn items(&self) -> Vec<(Item, u8)> {
        let mut v: Vec<(Item, u8)> = self.fitted().map(|(p, c)| (Item::Part(self.line, p), c)).collect();
        for m in 0..3 {
            if let (true, Some(w)) = (self.mounts[m], self.weapon_on(m)) {
                v.push((Item::Weapon(w), 100));
            }
        }
        v
    }
}

/// What it takes to bring a `line` `part` from `from` % to `to` %: its recipe's materials in
/// proportion, at 60% (the frame is there; only the armour and wiring are new). Rounded up.
pub fn repair_cost(line: FrameId, part: Part, from: u8, to: u8) -> Vec<(Item, u64)> {
    let Some(r) = recipe(Item::Part(line, part)) else { return Vec::new() };
    let missing = u64::from(to.saturating_sub(from).min(100));
    r.inputs
        .iter()
        .map(|(item, qty)| (*item, (qty * missing * 60).div_ceil(100 * 100)))
        .filter(|(_, q)| *q > 0)
        .collect()
}

/// What melting down `item` at `condition` % gives back: half of what went into it, as worn as
/// it is. Rounded down.
pub fn scrap_yield(item: Item, condition: u8) -> Vec<(Item, u64)> {
    let Some(r) = recipe(item) else { return Vec::new() };
    if item.bulk() {
        return Vec::new();
    }
    let c = u64::from(condition.min(100));
    r.inputs.iter().map(|(i, qty)| (*i, qty * c * 50 / (100 * 100))).filter(|(_, q)| *q > 0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Material;

    fn home(suit: &Suit) -> Homecoming {
        let l = suit.loadout();
        Homecoming {
            frame: suit.line,
            parts: l.parts,
            mounts: l.mounts,
            ammo: l.ammo,
            propellant: l.propellant,
            systems: l.systems,
            cargo_kg: [0; 4],
            held: None,
            bounty: 0,
        }
    }

    #[test]
    fn a_complete_suit_launches_whole_and_comes_home_as_it_left() {
        for line in crate::item::LINES {
            let suit = Suit::complete(line);
            let l = suit.loadout();
            assert_eq!(l, Loadout::full(line), "{line:?}");
            let mut back = suit.clone();
            back.came_home(&home(&suit));
            assert_eq!(back, suit, "{line:?}");
        }
    }

    #[test]
    fn worn_parts_round_trip_through_the_simulation_exactly() {
        let mut suit = Suit::complete(FrameId::Leo);
        suit.parts = [Some(55), Some(61), None, Some(33), Some(100), Some(7)];
        let mut back = suit.clone();
        back.came_home(&home(&suit));
        assert_eq!(back.parts, suit.parts);
    }

    #[test]
    fn a_part_shot_off_takes_its_weapon() {
        let mut suit = Suit::complete(FrameId::Leo);
        let mut h = home(&suit);
        // The Leo's left arm holds the machine cannon and the saber.
        h.parts[Part::ArmL as usize] = 0.0;
        h.ammo[1] = 120;
        suit.came_home(&h);
        assert_eq!(suit.parts[Part::ArmL as usize], None);
        assert_eq!(suit.mounts, [true, false, false]);
        assert_eq!(suit.ammo[1], 0);
        // A new arm has a free mount for the cannon.
        let mut s = suit.clone();
        s.parts[Part::ArmL as usize] = Some(100);
        assert_eq!(s.slot_for(Item::Weapon(WeaponKind::MachineCannon)), Some(Slot::Mount { mount: 1 }));
        assert_eq!(s.slot_for(Item::Weapon(WeaponKind::BeamRifle)), None, "already fitted");
        assert_eq!(s.slot_for(Item::Weapon(WeaponKind::TwinBusterRifle)), None, "not a Leo's");
        assert_eq!(s.slot_for(Item::Part(FrameId::WingZero, Part::ArmL)), None, "another line's");
    }

    #[test]
    fn repairs_cost_in_proportion_and_scrap_gives_half_back() {
        let full = repair_cost(FrameId::Leo, Part::Torso, 0, 100);
        let half = repair_cost(FrameId::Leo, Part::Torso, 50, 100);
        let r = recipe(Item::Part(FrameId::Leo, Part::Torso)).unwrap();
        for ((item, f), (_, h)) in full.iter().zip(&half) {
            let need = r.inputs.iter().find(|(i, _)| i == item).unwrap().1;
            assert_eq!(*f, (need * 60).div_ceil(100));
            assert!(*h * 2 >= *f && *h * 2 <= *f + 1, "{item}: {h} vs {f}");
        }
        assert!(repair_cost(FrameId::Leo, Part::Head, 100, 100).is_empty());
        let scrap = scrap_yield(Item::Part(FrameId::Leo, Part::Torso), 100);
        let steel = scrap.iter().find(|(i, _)| *i == Item::Material(Material::Steel)).unwrap().1;
        let need = r.inputs.iter().find(|(i, _)| *i == Item::Material(Material::Steel)).unwrap().1;
        assert_eq!(steel, need / 2);
        assert!(scrap_yield(Item::Material(Material::Steel), 100).is_empty());
    }
}
