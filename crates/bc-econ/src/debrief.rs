//! A sortie's payout sheet (`docs/DESIGN.md`, "The debrief"; after Armored Core VI's mission
//! payout and X-Wing's debrief): what the sortie earned and what it cost, each at the colony's
//! values (`catalogue::value`), and the net. The hangar works it out as a sortie ends
//! (`Hangar::came_home`, `Hangar::lost`), and it goes to the pilot with the sortie's news
//! (`wire::Update::Sortie`).

use bc_sim::content::kits::Kit;
use serde::{Deserialize, Serialize};

use crate::catalogue::{MUNITIONS_ITEM, PROPELLANT_ITEM, munitions_per_load, value, worth};
use crate::item::Item;
use crate::stores::Stores;
use crate::suit::{Suit, repair_cost};

/// What a sortie came to, line by line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Debrief {
    /// What it earned (credits, positive) and what it cost (negative), in order. Lines that come
    /// to nothing are left out.
    pub lines: Vec<Line>,
}

/// One line of a [`Debrief`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub what: String,
    pub cr: i64,
}

impl Debrief {
    /// The sortie's net, credits.
    pub fn net(&self) -> i64 {
        self.lines.iter().map(|l| l.cr).sum()
    }

    fn earned(&mut self, what: &str, cr: u64) {
        if cr > 0 {
            self.lines.push(Line { what: what.into(), cr: i64::try_from(cr).unwrap_or(i64::MAX) });
        }
    }

    fn spent(&mut self, what: &str, cr: u64) {
        if cr > 0 {
            self.lines.push(Line { what: what.into(), cr: -i64::try_from(cr).unwrap_or(i64::MAX) });
        }
    }

    /// A sortie the suit came home from: `out` as it launched, `back` as it docked, with
    /// `kits_back` of its rack unused. It earned `bounty`, and brought home `ore` and `salvage`
    /// (their worth).
    pub fn docked(
        out: &Suit,
        back: &Suit,
        kits_back: &[u8; Kit::COUNT],
        bounty: u32,
        ore: u64,
        salvage: u64,
    ) -> Self {
        let mut d = Self::default();
        d.earned("BOUNTIES", u64::from(bounty));
        d.earned("ORE", ore);
        d.earned("SALVAGE", salvage);
        // What it burnt (under anime rules the tank tops itself up out there, so never more
        // than it went out with).
        let burnt = out.propellant.saturating_sub(back.propellant);
        d.spent("PROPELLANT", worth(PROPELLANT_ITEM, value(PROPELLANT_ITEM), u64::from(burnt)));
        // Rounds fired, or lost with their mount.
        let mut fired = [0u16; 3];
        for (m, f) in fired.iter_mut().enumerate() {
            let left = if back.mounts[m] { back.ammo[m] } else { 0 };
            *f = out.ammo[m].saturating_sub(left);
        }
        d.spent("ROUNDS", rounds_worth(out, &fired));
        let used: u64 = Kit::ALL
            .into_iter()
            .map(|k| {
                value(Item::Kit(k)) * u64::from(out.kits[k as usize].saturating_sub(kits_back[k as usize]))
            })
            .sum();
        d.spent("THE RACK", used);
        // Armour: what bringing the parts that came home back to how they went out takes.
        let mut armour = 0;
        let mut lost = 0;
        for (p, (was, is)) in out.parts.iter().zip(back.parts).enumerate() {
            let part = bc_proto::Part::ALL[p];
            match (*was, is) {
                (Some(was), Some(is)) if is < was => {
                    armour += repair_cost(out.line, part, is, was)
                        .iter()
                        .map(|(item, q)| worth(*item, value(*item), *q))
                        .sum::<u64>();
                }
                // Shot off: the part, as worn as it was.
                (Some(was), None) => lost += value(Item::Part(out.line, part)) * u64::from(was) / 100,
                _ => {}
            }
        }
        d.spent("ARMOUR", armour);
        // ...and the weapons and equipment that went with them.
        for m in 0..3 {
            if let (true, false, Some(w)) = (out.mounts[m], back.mounts[m], out.weapon_on(m)) {
                lost += value(Item::Weapon(w));
            }
        }
        for (was, is) in out.modules.iter().zip(back.modules) {
            if let (Some(k), None) = (was, is) {
                lost += value(Item::Module(*k));
            }
        }
        d.spent("SHOT OFF", lost);
        d
    }

    /// A sortie the suit was lost on: `out` as it launched (if the hangar knew it). It earned
    /// `bounty`.
    pub fn lost(out: Option<&Suit>, bounty: u32) -> Self {
        let mut d = Self::default();
        d.earned("BOUNTIES", u64::from(bounty));
        if let Some(suit) = out {
            d.spent("THE SUIT", suit_worth(suit));
        }
        d
    }
}

/// Rounds `rounds` per mount of `suit`'s guns are worth (as munitions).
fn rounds_worth(suit: &Suit, rounds: &[u16; 3]) -> u64 {
    (0..3)
        .filter_map(|m| {
            let w = suit.weapon_on(m)?;
            let full = suit.full_load(m);
            if !suit.mounts[m] || full == 0 {
                return None;
            }
            let kg = u64::from(rounds[m].min(full)) * munitions_per_load(w) / u64::from(full);
            Some(worth(MUNITIONS_ITEM, value(MUNITIONS_ITEM), kg))
        })
        .sum()
}

/// What a suit is worth as it stands: its parts as worn as they are, its weapons and equipment,
/// and what's in its tank, its guns and its rack.
pub fn suit_worth(suit: &Suit) -> u64 {
    let fitted: u64 = suit.items().iter().map(|(item, c)| value(*item) * u64::from(*c) / 100).sum();
    let tank = worth(PROPELLANT_ITEM, value(PROPELLANT_ITEM), u64::from(suit.propellant));
    let rack: u64 =
        Kit::ALL.into_iter().map(|k| value(Item::Kit(k)) * u64::from(suit.kits[k as usize])).sum();
    fitted + tank + rounds_worth(suit, &suit.ammo) + rack
}

/// What stores are worth: bulk goods by the tonne, the rest by the piece, parts as worn as they
/// are.
pub fn stores_worth(stores: &Stores) -> u64 {
    let stock: u64 = stores.stock().map(|(item, q)| worth(item, value(item), q)).sum();
    let parts: u64 = stores.parts().iter().map(|u| value(u.item()) * u64::from(u.condition) / 100).sum();
    stock + parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::rounds_per_load;
    use bc_proto::{FrameId, Part};

    fn line(d: &Debrief, what: &str) -> i64 {
        d.lines.iter().find(|l| l.what == what).map_or(0, |l| l.cr)
    }

    #[test]
    fn a_sortie_home_is_paid_and_charged_for_what_it_did() {
        let out = Suit::complete(FrameId::Leo);
        let mut back = out.clone();
        // It burnt 500 kg, fired a third of its machine cannon's rounds, used its stim, took its
        // torso down to 70% and lost its left arm, with the machine cannon on it.
        back.propellant = out.propellant - 500;
        back.ammo[1] = out.ammo[1] - out.ammo[1] / 3;
        back.parts[Part::Torso as usize] = Some(70);
        back.parts[Part::ArmL as usize] = None;
        back.mounts[1] = false;
        back.ammo[1] = 0;
        let mut out_kits = out.clone();
        out_kits.kits[Kit::Stim as usize] = 1;
        let d = Debrief::docked(&out_kits, &back, &[0; Kit::COUNT], 400, 1_000, 0);
        assert_eq!(line(&d, "BOUNTIES"), 400);
        assert_eq!(line(&d, "ORE"), 1_000);
        assert_eq!(line(&d, "SALVAGE"), 0, "a line that comes to nothing is left out");
        assert!(d.lines.iter().all(|l| l.cr != 0));
        let burnt = worth(PROPELLANT_ITEM, value(PROPELLANT_ITEM), 500) as i64;
        assert_eq!(line(&d, "PROPELLANT"), -burnt);
        // The rounds lost with the arm count as spent too: all of the load it went out with.
        let w = out.weapon_on(1).unwrap();
        let kg = u64::from(out.ammo[1]) * munitions_per_load(w) / u64::from(rounds_per_load(w));
        assert_eq!(line(&d, "ROUNDS"), -(worth(MUNITIONS_ITEM, value(MUNITIONS_ITEM), kg) as i64));
        assert_eq!(line(&d, "THE RACK"), -(value(Item::Kit(Kit::Stim)) as i64));
        assert!(line(&d, "ARMOUR") < 0);
        let arm = value(Item::Part(FrameId::Leo, Part::ArmL)) + value(Item::Weapon(w));
        assert_eq!(line(&d, "SHOT OFF"), -(arm as i64));
        assert_eq!(d.net(), d.lines.iter().map(|l| l.cr).sum::<i64>());
    }

    #[test]
    fn a_suit_lost_is_written_off() {
        let out = Suit::complete(FrameId::Leo);
        let d = Debrief::lost(Some(&out), 250);
        assert_eq!(line(&d, "BOUNTIES"), 250);
        assert_eq!(line(&d, "THE SUIT"), -(suit_worth(&out) as i64));
        assert!(suit_worth(&out) > 10_000, "a whole Leo is worth something");
        // A loss the hangar can't account for: just what it earned.
        assert_eq!(Debrief::lost(None, 0).lines, vec![]);
    }

    #[test]
    fn stores_are_worth_their_goods_and_their_parts_as_worn() {
        let mut s = Stores::default();
        assert_eq!(stores_worth(&s), 0);
        s.add(PROPELLANT_ITEM, 2_000);
        s.add_part(crate::stores::PartUnit {
            condition: 50,
            ..crate::stores::PartUnit::new(FrameId::Leo, Part::Head)
        });
        let head = value(Item::Part(FrameId::Leo, Part::Head)) / 2;
        assert_eq!(stores_worth(&s), worth(PROPELLANT_ITEM, value(PROPELLANT_ITEM), 2_000) + head);
    }
}
