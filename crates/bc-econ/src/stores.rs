//! A hangar's stores: bulk goods by the kilogram, weapons by the piece, and parts one by one, each
//! with its condition (a part is only as good as the armour it has left).

use std::collections::BTreeMap;

use bc_proto::{FrameId, Part};
use serde::{Deserialize, Serialize};

use crate::item::{Item, line_serde, part_serde};

/// A mobile-suit part on the shelf.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartUnit {
    #[serde(with = "line_serde")]
    pub line: FrameId,
    #[serde(with = "part_serde")]
    pub part: Part,
    /// Armour left, 1..=100 %: new parts are 100, salvage much less.
    pub condition: u8,
}

impl PartUnit {
    pub fn new(line: FrameId, part: Part) -> Self {
        Self { line, part, condition: 100 }
    }

    pub fn item(&self) -> Item {
        Item::Part(self.line, self.part)
    }
}

/// Stock, kept tidy: no zero entries; parts sorted, best condition first within each kind.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stores {
    /// Ores and materials, kg; weapons, pieces.
    stock: BTreeMap<Item, u64>,
    /// Parts, one by one.
    parts: Vec<PartUnit>,
}

impl Stores {
    /// How much of `item` there is: kg, pieces, or (a part) new pieces, the only ones that trade
    /// or count as ingredients.
    pub fn get(&self, item: Item) -> u64 {
        match item {
            Item::Part(line, part) => {
                self.parts.iter().filter(|u| u.line == line && u.part == part && u.condition >= 100).count()
                    as u64
            }
            _ => self.stock.get(&item).copied().unwrap_or(0),
        }
    }

    pub fn has(&self, item: Item, qty: u64) -> bool {
        self.get(item) >= qty
    }

    /// Adds `qty` of `item` (parts: new ones).
    pub fn add(&mut self, item: Item, qty: u64) {
        if qty == 0 {
            return;
        }
        match item {
            Item::Part(line, part) => {
                for _ in 0..qty {
                    self.add_part(PartUnit::new(line, part));
                }
            }
            _ => {
                let e = self.stock.entry(item).or_default();
                *e = e.saturating_add(qty);
            }
        }
    }

    /// Takes `qty` of `item` if there's that much (parts: new ones); `false` and nothing taken
    /// otherwise.
    pub fn take(&mut self, item: Item, qty: u64) -> bool {
        if !self.has(item, qty) {
            return false;
        }
        match item {
            Item::Part(line, part) => {
                for _ in 0..qty {
                    if let Some(i) =
                        self.parts.iter().position(|u| u.line == line && u.part == part && u.condition >= 100)
                    {
                        self.parts.remove(i);
                    }
                }
            }
            _ => {
                if let Some(e) = self.stock.get_mut(&item) {
                    *e -= qty;
                    if *e == 0 {
                        self.stock.remove(&item);
                    }
                }
            }
        }
        true
    }

    /// Takes up to `qty`; returns how much it took.
    pub fn take_up_to(&mut self, item: Item, qty: u64) -> u64 {
        let n = self.get(item).min(qty);
        let _ = self.take(item, n);
        n
    }

    /// Whether every one of `needs` (times `times`) is there.
    pub fn has_all(&self, needs: &[(Item, u64)], times: u64) -> bool {
        needs.iter().all(|(item, qty)| self.has(*item, qty.saturating_mul(times)))
    }

    /// Takes every one of `needs` (times `times`), or nothing.
    pub fn take_all(&mut self, needs: &[(Item, u64)], times: u64) -> bool {
        if !self.has_all(needs, times) {
            return false;
        }
        for (item, qty) in needs {
            let _ = self.take(*item, qty * times);
        }
        true
    }

    pub fn add_all(&mut self, goods: &[(Item, u64)], times: u64) {
        for (item, qty) in goods {
            self.add(*item, qty.saturating_mul(times));
        }
    }

    pub fn add_part(&mut self, unit: PartUnit) {
        let unit = PartUnit { condition: unit.condition.clamp(1, 100), ..unit };
        let at = self
            .parts
            .iter()
            .position(|u| {
                (u.item(), std::cmp::Reverse(u.condition)) > (unit.item(), std::cmp::Reverse(unit.condition))
            })
            .unwrap_or(self.parts.len());
        self.parts.insert(at, unit);
    }

    /// Takes the best `line` `part` on the shelf, whatever its condition.
    pub fn take_best_part(&mut self, line: FrameId, part: Part) -> Option<PartUnit> {
        let i = self.parts.iter().position(|u| u.line == line && u.part == part)?;
        Some(self.parts.remove(i))
    }

    /// Takes the worst `line` `part` on the shelf (what gets scrapped first).
    pub fn take_worst_part(&mut self, line: FrameId, part: Part) -> Option<PartUnit> {
        let i = self.parts.iter().rposition(|u| u.line == line && u.part == part)?;
        Some(self.parts.remove(i))
    }

    /// Every bulk good and weapon with how much there is, in item order.
    pub fn stock(&self) -> impl Iterator<Item = (Item, u64)> + '_ {
        self.stock.iter().map(|(i, q)| (*i, *q))
    }

    /// Every part, grouped by kind, best first.
    pub fn parts(&self) -> &[PartUnit] {
        &self.parts
    }

    pub fn is_empty(&self) -> bool {
        self.stock.is_empty() && self.parts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Material, Ore};
    use bc_proto::WeaponKind;

    #[test]
    fn stock_adds_takes_and_never_goes_negative() {
        let mut s = Stores::default();
        let ti = Item::Ore(Ore::Titanium);
        s.add(ti, 500);
        assert!(!s.take(ti, 600));
        assert_eq!(s.get(ti), 500);
        assert!(s.take(ti, 500));
        assert!(s.is_empty(), "no zero entries left behind");
        s.add(Item::Weapon(WeaponKind::BeamRifle), 2);
        assert_eq!(s.take_up_to(Item::Weapon(WeaponKind::BeamRifle), 5), 2);
        let needs = [(Item::Material(Material::Steel), 100), (ti, 10)];
        s.add(Item::Material(Material::Steel), 250);
        assert!(!s.take_all(&needs, 2), "no titanium: nothing taken");
        assert_eq!(s.get(Item::Material(Material::Steel)), 250);
        s.add(ti, 20);
        assert!(s.take_all(&needs, 2));
        assert_eq!(s.get(Item::Material(Material::Steel)), 50);
    }

    #[test]
    fn parts_keep_their_condition_and_only_new_ones_count_as_stock() {
        let mut s = Stores::default();
        let arm = Item::Part(FrameId::Leo, Part::ArmL);
        s.add_part(PartUnit { line: FrameId::Leo, part: Part::ArmL, condition: 40 });
        s.add(arm, 1);
        s.add_part(PartUnit { line: FrameId::Leo, part: Part::ArmL, condition: 70 });
        s.add_part(PartUnit { line: FrameId::Leo, part: Part::Head, condition: 0 });
        assert_eq!(s.get(arm), 1);
        let conditions: Vec<u8> =
            s.parts().iter().filter(|u| u.part == Part::ArmL).map(|u| u.condition).collect();
        assert_eq!(conditions, [100, 70, 40]);
        assert_eq!(s.parts().iter().find(|u| u.part == Part::Head).unwrap().condition, 1, "clamped");
        assert_eq!(s.take_worst_part(FrameId::Leo, Part::ArmL).unwrap().condition, 40);
        assert!(s.take(arm, 1));
        assert!(!s.take(arm, 1), "the 70% arm isn't new");
        assert_eq!(s.take_best_part(FrameId::Leo, Part::ArmL).unwrap().condition, 70);
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Stores>(&json).unwrap(), s);
    }
}
