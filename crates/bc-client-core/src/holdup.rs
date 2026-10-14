//! Why a trigger won't fire: the HUD's one word where it used to say only `---`.
//!
//! Mecha Royale (`docs/PEERS.md`) learnt it from its players' reports: a dead trigger, or health
//! falling with nobody shooting, reads as a bug unless the screen names the cause and, where there
//! is one, the cure. Everything here is read off the own state the snapshot carries (and the
//! prediction's stagger), in the order the server's weapons step refuses a shot, so the word is
//! the reason the server has.

use bc_proto::OwnState;
use bc_proto::snapshot::own_flags;
use bc_sim::content::{ArmSlot, FrameSpec, Kit, Kits, WeaponClass, weapon};

/// What holds a loadout weapon back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holdup {
    /// Inside the colony, out of the Blast Hall: nothing fires there.
    Safe,
    /// Knocked off balance: every weapon is down until it steadies.
    Staggered,
    /// Changing form (Altair ↔ Aquila).
    Transforming,
    /// Too hot to fire until it cools (Full Salvo fires through it).
    Overheated,
    /// Its rounds are spent.
    Empty,
    /// The arm (or the part) it's mounted on was shot off.
    Lost,
    /// Its hand is holding something.
    HandFull,
    /// Not enough energy in the capacitor for a shot.
    LowEnergy,
    /// Cycling between shots: this many ticks to go.
    Cycling(u8),
}

impl Holdup {
    /// The HUD's words for it: the cause, and what to do about it where there's something to do.
    pub fn words(self) -> String {
        match self {
            Holdup::Safe => "SAFE".into(),
            Holdup::Staggered => "STAGGERED".into(),
            Holdup::Transforming => "CHANGING FORM".into(),
            Holdup::Overheated => "OVERHEATED · let it cool".into(),
            Holdup::Empty => "EMPTY · rearm in the bay".into(),
            Holdup::Lost => "ARM LOST".into(),
            Holdup::HandFull => "HAND FULL · B stows, T throws".into(),
            Holdup::LowEnergy => "LOW ENERGY · let it charge".into(),
            Holdup::Cycling(t) => format!("{:.1}s", f32::from(t) / bc_sim::TICK_HZ as f32),
        }
    }
}

/// Cycling shorter than this isn't worth a word: an automatic weapon between rounds.
const CYCLE_SHOWN: u8 = 15;

/// Why loadout `slot` (0 or 1, a gun or a launcher) of the own suit `o` of frame `spec` can't fire,
/// `None` if it can (or it's a blade, which strikes on its own terms). `safe`: inside the colony
/// out of the Blast Hall; `staggered`: as the prediction has it; `holding`: which hand holds
/// something (`Some(true)` the right), as `bc_sim::suits::Suits::arm_free` asks.
pub fn holdup(
    spec: &FrameSpec,
    o: &OwnState,
    slot: usize,
    safe: bool,
    staggered: bool,
    holding: Option<bool>,
) -> Option<Holdup> {
    let mount = spec.loadout.get(slot).copied().flatten()?;
    let w = weapon(mount.weapon);
    if w.class == WeaponClass::Melee {
        return None;
    }
    if safe {
        return Some(Holdup::Safe);
    }
    if staggered {
        return Some(Holdup::Staggered);
    }
    if o.flags & own_flags::TRANSFORMING != 0 {
        return Some(Holdup::Transforming);
    }
    if o.weapon_ready & (1 << slot) != 0 {
        return None;
    }
    if o.flags & own_flags::OVERHEAT != 0 {
        return Some(Holdup::Overheated);
    }
    if w.ammo > 0 && o.ammo[slot.min(1)] == 0 {
        return Some(Holdup::Empty);
    }
    let lost = |part: bc_proto::Part| o.parts[part as usize] <= 0.0;
    if lost(mount.arm.part()) || (mount.arm == ArmSlot::Both && lost(bc_proto::Part::ArmL)) {
        return Some(Holdup::Lost);
    }
    let busy = holding.is_some_and(|right| match mount.arm {
        ArmSlot::Left => !right,
        ArmSlot::Right | ArmSlot::Nose => right,
        ArmSlot::Both => true,
        _ => false,
    });
    if busy {
        return Some(Holdup::HandFull);
    }
    let energy_cap = spec.energy_cap * bc_sim::tuning::own_tuning(o).energy_cap;
    if w.energy > 0.0 && o.energy * energy_cap < w.energy {
        return Some(Holdup::LowEnergy);
    }
    let wait = o.arms.wait[slot];
    (CYCLE_SHOWN..bc_proto::snapshot::OwnArms::NEVER).contains(&wait).then_some(Holdup::Cycling(wait))
}

/// The alarm for what's draining the suit with nobody shooting at it: the one line that names
/// the cause and the cure, up for as long as the drain lasts (Mecha Royale's alarm strip:
/// "i was just standing still but my HP was going down"). A blow announces itself; a leaking tank
/// or a suit run hot doesn't. `None` when nothing is.
pub fn drain(o: &OwnState) -> Option<String> {
    if !o.alive {
        return None;
    }
    let rack = Kits(o.kits);
    // What the rack holds for it, on its key (survival: keys 1-4, in `Kit::ALL`'s order).
    let cure = |kit: Kit, otherwise: &str| {
        if rack.get(kit) > 0 {
            format!("{} {}", kit as u8 + 1, kit.name().to_uppercase())
        } else {
            otherwise.to_string()
        }
    };
    let leak = bc_sim::tuning::own_tuning(o).leak_kg_s;
    if leak > 0.0 && o.propellant > 0.0 {
        return Some(format!("PROPELLANT LEAK -{leak:.0} kg/s · {}", cure(Kit::Patch, "the bay mends it")));
    }
    if o.flags & own_flags::OVERHEAT != 0 {
        return Some(format!("OVERHEATED · weapons hold · {}", cure(Kit::Coolant, "let it cool")));
    }
    None
}

#[cfg(test)]
mod tests {
    use bc_proto::{FrameId, NO_CHUNK, Part, WeaponKind};
    use bc_sim::content::frame;

    use super::*;

    fn own(f: FrameId) -> OwnState {
        OwnState {
            frame: f,
            alive: true,
            parts: [1.0; Part::COUNT],
            energy: 1.0,
            ammo: [100, 100],
            held: NO_CHUNK,
            ..OwnState::default()
        }
    }

    #[test]
    fn says_what_holds_each_weapon_back() {
        let spec = frame(FrameId::Argo);
        let gun = spec.loadout[1].expect("the Argo's machine cannon");
        assert_eq!(gun.weapon, WeaponKind::MachineCannon);
        let mut o = own(FrameId::Argo);
        let why = |o: &OwnState, holding| holdup(spec, o, 1, false, false, holding);
        o.weapon_ready = 0b11;
        assert_eq!(why(&o, None), None);
        assert_eq!(holdup(spec, &o, 1, true, false, None), Some(Holdup::Safe));
        assert_eq!(holdup(spec, &o, 1, false, true, None), Some(Holdup::Staggered));
        o.weapon_ready = 0;
        o.flags = own_flags::OVERHEAT;
        assert_eq!(why(&o, None), Some(Holdup::Overheated));
        o.flags = 0;
        o.ammo = [100, 0];
        assert_eq!(why(&o, None), Some(Holdup::Empty));
        o.ammo = [100, 100];
        o.parts[gun.arm.part() as usize] = 0.0;
        assert_eq!(why(&o, None), Some(Holdup::Lost));
        o.parts = [1.0; Part::COUNT];
        // Its hand holding something; the other hand's doesn't matter.
        let right = gun.arm == ArmSlot::Right;
        assert_eq!(why(&o, Some(right)), Some(Holdup::HandFull));
        o.arms.wait[1] = 45;
        assert_eq!(why(&o, Some(!right)), Some(Holdup::Cycling(45)));
        assert_eq!(Holdup::Cycling(45).words(), "1.5s");
        // A round or two between shots isn't worth a word.
        o.arms.wait[1] = 2;
        assert_eq!(why(&o, None), None);
    }

    #[test]
    fn an_energy_weapon_without_the_energy_says_so() {
        let spec = frame(FrameId::Argo);
        assert_eq!(spec.loadout[0].map(|m| m.weapon), Some(WeaponKind::BeamRifle));
        let mut o = own(FrameId::Argo);
        o.energy = 0.0;
        assert_eq!(holdup(spec, &o, 0, false, false, None), Some(Holdup::LowEnergy));
        // A blade has its own terms.
        assert_eq!(holdup(spec, &o, 2, false, false, None), None);
    }

    #[test]
    fn the_alarm_names_a_silent_drain_and_its_cure() {
        let mut o = own(FrameId::Argo);
        assert_eq!(drain(&o), None);
        o.flags = own_flags::OVERHEAT;
        assert_eq!(drain(&o).as_deref(), Some("OVERHEATED · weapons hold · let it cool"));
        let mut rack = Kits::NONE;
        rack.set(Kit::Coolant, 1);
        o.kits = rack.0;
        assert!(drain(&o).is_some_and(|s| s.ends_with("2 COOLANT FLUSH")));
        o.alive = false;
        assert_eq!(drain(&o), None);
    }
}
