//! The exchange's books balance: whatever pilots do, credits and goods only enter or leave the
//! economy through the colony (what it pays and takes) and its fees; nothing is made or lost in
//! escrow, fills, cancels or collection.

use std::collections::BTreeMap;

use bc_econ::exchange::{Exchange, Side};
use bc_econ::hangar::Hangar;
use bc_econ::item::{Item, Material, Ore};
use bc_econ::{catalogue, stores::Stores};
use bc_proto::{FrameId, Part, WeaponKind};
use proptest::prelude::*;

const TRADERS: [&str; 4] = ["a", "b", "c", "d"];

fn items() -> Vec<Item> {
    vec![
        Item::Ore(Ore::Titanium),
        Item::Ore(Ore::Exotics),
        Item::Material(Material::Steel),
        Item::Material(Material::Gundanium),
        Item::Material(Material::Propellant),
        Item::Part(FrameId::Leo, Part::ArmL),
        Item::Part(FrameId::WingZero, Part::Head),
        Item::Weapon(WeaponKind::BeamRifle),
    ]
}

#[derive(Clone, Debug)]
enum Op {
    Order { who: usize, item: usize, buy: bool, price_pct: u64, qty: u64, rest: bool },
    Cancel { who: usize, nth: usize },
    Collect { who: usize },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        6 => (0..4usize, 0..8usize, any::<bool>(), 50..160u64, 1..4_000u64, any::<bool>())
            .prop_map(|(who, item, buy, price_pct, qty, rest)| Op::Order { who, item, buy, price_pct, qty, rest }),
        2 => (0..4usize, 0..4usize).prop_map(|(who, nth)| Op::Cancel { who, nth }),
        1 => (0..4usize).prop_map(|who| Op::Collect { who }),
    ]
}

/// Credits, and goods per item, held by pilots, in their accounts and in escrow.
fn ledger(hangars: &[Hangar], ex: &Exchange) -> (u128, BTreeMap<Item, u128>) {
    let mut credits: u128 = 0;
    let mut goods: BTreeMap<Item, u128> = BTreeMap::new();
    let mut count = |stores: &Stores| {
        for item in items() {
            *goods.entry(item).or_default() += u128::from(stores.get(item));
        }
    };
    for h in hangars {
        credits += u128::from(h.credits);
        count(&h.stores);
    }
    for (_, a) in ex.accounts() {
        credits += u128::from(a.credits);
        for (item, q) in &a.goods {
            *goods.entry(*item).or_default() += u128::from(*q);
        }
    }
    let (escrow, held) = ex.escrowed();
    credits += u128::from(escrow);
    for (item, q) in held {
        *goods.entry(item).or_default() += u128::from(q);
    }
    (credits, goods)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn credits_and_goods_are_conserved(ops in proptest::collection::vec(op(), 1..80)) {
        let mut ex = Exchange::new();
        let mut hangars: Vec<Hangar> = TRADERS
            .iter()
            .map(|_| {
                let mut h = Hangar { credits: 5_000_000, ..Hangar::default() };
                for item in items() {
                    h.stores.add(item, if item.bulk() { 20_000 } else { 5 });
                }
                h
            })
            .collect();
        let colony = |ex: &Exchange| -> BTreeMap<Item, f64> {
            items().into_iter().map(|i| (i, ex.colony_stock(i).unwrap_or(0.0))).collect()
        };
        let (c0, g0) = ledger(&hangars, &ex);
        let s0 = colony(&ex);
        for op in ops {
            match op {
                Op::Order { who, item, buy, price_pct, qty, rest } => {
                    let item = items()[item];
                    let qty = if item.bulk() { qty } else { 1 + qty % 3 };
                    let price = (catalogue::value(item).max(1_000) * price_pct / 100).max(1);
                    let side = if buy { Side::Buy } else { Side::Sell };
                    let _ = hangars[who].trade(&mut ex, TRADERS[who], item, side, price, qty, rest);
                }
                Op::Cancel { who, nth } => {
                    let id = ex.orders(TRADERS[who]).nth(nth).map(|o| o.id);
                    if let Some(id) = id {
                        prop_assert!(ex.cancel(TRADERS[who], id));
                    }
                }
                Op::Collect { who } => {
                    hangars[who].collect(&mut ex, TRADERS[who]);
                }
            }
            let (c, g) = ledger(&hangars, &ex);
            // Credits: in only from the colony's purchases, out to its sales and fees.
            prop_assert_eq!(
                c + u128::from(ex.colony_took) + u128::from(ex.fees),
                c0 + u128::from(ex.colony_paid)
            );
            // Goods: what pilots hold plus what the colony holds never changes.
            let s = colony(&ex);
            for item in items() {
                let pilots = g[&item] as f64;
                prop_assert_eq!(pilots + s[&item], g0[&item] as f64 + s0[&item], "{}", item);
            }
        }
        // Everyone collects and cancels: escrow empties into the hangars.
        for (k, t) in TRADERS.iter().enumerate() {
            ex.cancel_all(t);
            hangars[k].collect(&mut ex, t);
        }
        prop_assert_eq!(ex.escrowed(), (0, BTreeMap::new()));
        prop_assert!(ex.accounts().next().is_none());
    }
}
