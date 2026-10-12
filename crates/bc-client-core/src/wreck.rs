//! How a suit dies on screen: its wreck's choreography. The secondary blasts that go off through
//! its frame (which parts, when, how big), whether one of them is the big one, and how long what's
//! left burns, are drawn from a seed of the suit's own (its slot and its life's generation), so
//! every client plays the same death and nothing more travels than the wreck's flag.
//!
//! Mecha Royale (`docs/PEERS.md`) plans its wrecks the same way, from the mech's id: severed
//! limbs, cook-offs, a topple, smouldering points. Ours keep the limbs to the server (a part shot
//! off is its salvage chunk) and the plan to the blasts and the burn.

use bc_proto::Part;
use bc_sim::math::Rng;

/// The most secondary blasts a wreck goes up in.
pub const MAX_COOK_OFFS: usize = 6;

/// One secondary blast: `delay` seconds after the wreck is seen, at `part`, `scale` the size of a
/// suit's own explosion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CookOff {
    pub delay: f32,
    pub part: Part,
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WreckPlan {
    cook_offs: [Option<CookOff>; MAX_COOK_OFFS],
    /// The last blast is the reactor's: big, with a shockwave.
    pub catastrophic: bool,
    /// How long what's left burns, s.
    pub burn_secs: f32,
}

impl WreckPlan {
    pub fn cook_offs(&self) -> impl Iterator<Item = CookOff> + '_ {
        self.cook_offs.iter().flatten().copied()
    }
}

/// The wreck plan of the suit in `slot` on its life `generation`, with `parts` its armour per part
/// as it died (in eighths: a part at 0 is gone, and nothing goes off where it was).
pub fn plan(slot: u16, generation: u8, parts: [u8; Part::COUNT]) -> WreckPlan {
    let mut rng = Rng::new(0x3EC4_0000_0000 ^ (u64::from(slot) << 8) ^ u64::from(generation));
    let on: Vec<Part> =
        Part::ALL.into_iter().filter(|p| *p == Part::Torso || parts[*p as usize] > 0).collect();
    let catastrophic = rng.next_u32().is_multiple_of(6);
    let n = 3 + (rng.next_u32() % 3) as usize;
    let mut cook_offs = [None; MAX_COOK_OFFS];
    let mut at = 0.08 + 0.12 * rng.next_f32();
    for slot in cook_offs.iter_mut().take(n) {
        let part = on[(rng.next_u32() as usize) % on.len()];
        *slot = Some(CookOff { delay: at, part, scale: 0.25 + 0.3 * rng.next_f32() });
        at += 0.15 + 0.45 * rng.next_f32();
    }
    if catastrophic {
        // The reactor goes last, from the chest.
        cook_offs[n] = Some(CookOff { delay: at + 0.2 * rng.next_f32(), part: Part::Torso, scale: 1.1 });
    }
    let burn_secs = if catastrophic { 28.0 } else { 14.0 + 12.0 * rng.next_f32() };
    WreckPlan { cook_offs, catastrophic, burn_secs }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_suit_dies_the_same_way_and_others_differently() {
        let whole = [7; Part::COUNT];
        assert_eq!(plan(12, 1, whole), plan(12, 1, whole));
        let plans: Vec<WreckPlan> = (0..64).map(|s| plan(s, 0, whole)).collect();
        let counts: std::collections::HashSet<usize> = plans.iter().map(|p| p.cook_offs().count()).collect();
        assert!(counts.len() > 1, "not every wreck goes up alike");
        let big = plans.iter().filter(|p| p.catastrophic).count();
        assert!((3..=25).contains(&big), "the big one now and then: {big} in 64");
        // A next life on the slot dies its own way.
        assert!((0..4).any(|g| plan(12, g, whole) != plan(12, 1, whole)));
    }

    #[test]
    fn blasts_follow_one_another_on_what_is_left() {
        let mut parts = [7; Part::COUNT];
        parts[Part::ArmL as usize] = 0;
        parts[Part::Head as usize] = 0;
        for s in 0..200 {
            let p = plan(s, 3, parts);
            let mut last = 0.0;
            for c in p.cook_offs() {
                assert!(c.delay > last && c.delay < 4.0);
                assert!(c.part != Part::ArmL && c.part != Part::Head, "nothing goes off where nothing is");
                last = c.delay;
            }
            assert!((3..=MAX_COOK_OFFS).contains(&p.cook_offs().count()));
            assert!(p.burn_secs >= 14.0 && p.burn_secs <= 28.0);
            if p.catastrophic {
                let end = p.cook_offs().last().unwrap();
                assert_eq!((end.part, end.scale), (Part::Torso, 1.1));
            }
        }
    }
}
