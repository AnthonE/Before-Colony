//! What the colony asks for its homes (`colony::homes`, `docs/OPEN_CITY.md`): a flat's rent by the
//! week, a house's or a top-storey flat's price, and what a buyer gets back selling it. Closed forms
//! of the home, so the page shows the price the server charges.
//!
//! A flat's rent is about a third of a patrol's pay for 60 m² low down with nothing to look at
//! (`charter::PATROL_REWARD`, 1,500 cr), more for its size, its height and its view.

use crate::colony::city::{CityBox, Rect};
use crate::colony::homes::{Home, Kind, Plan, View};
use crate::content::city::DistrictKind;

/// Rent a week for every m² of a home, cr, before its height and its view.
pub const RENT_PER_M2: f32 = 5.0;
/// Each storey up adds this share to the rent.
pub const STOREY_SHARE: f32 = 0.03;
/// A home bought costs this many weeks of what it would rent for.
pub const PRICE_WEEKS: f32 = 60.0;
/// Of a home's price, the share the colony pays back when its owner sells it, %.
pub const SELL_BACK_PCT: u64 = 80;
/// The most homes one pilot holds at once.
pub const MAX_HELD: usize = 2;
/// A week, and an hour, s (rent is charged by the hour from what the home's panel holds).
pub const WEEK_S: u64 = 7 * 24 * 3_600;
pub const HOUR_S: u64 = 3_600;
/// Rent unpaid this long and the lease lapses, s (Gates' grief protection's day).
pub const GRACE_S: u64 = 24 * HOUR_S;
/// A home bought and left unvisited this long is sold back to the colony, s.
pub const ABANDONED_S: u64 = 30 * 24 * HOUR_S;

/// What a view adds to the rent.
fn view_share(v: View) -> f32 {
    match v {
        View::Yard => 0.9,
        View::Street => 1.0,
        View::Plaza => 1.1,
        View::Park => 1.2,
        View::Canal => 1.25,
        View::Avenue | View::Window => 1.3,
    }
}

/// Whether `home` is bought rather than rented: a house, or a flat on its building's top storey.
pub fn bought(home: &Home) -> bool {
    home.kind == Kind::Houses || home.top
}

/// What `home` rents for a week (or would, if it's one that's bought), cr: in tens.
pub fn rent(home: &Home) -> u64 {
    let storey = if home.kind == Kind::Flats { home.id.storey() } else { 0 };
    let district = if home.district == DistrictKind::OldTown { 1.15 } else { 1.0 };
    let r = home.area * RENT_PER_M2 * (1.0 + STOREY_SHARE * storey as f32) * view_share(home.view) * district;
    ((r / 10.0 + 0.5) as u64).max(1) * 10
}

/// What `home` costs to buy, cr: in hundreds.
pub fn price(home: &Home) -> u64 {
    ((rent(home) as f32 * PRICE_WEEKS / 100.0 + 0.5) as u64) * 100
}

/// What the colony pays back for `home`, sold, cr.
pub fn sell_back(home: &Home) -> u64 {
    price(home) * SELL_BACK_PCT / 100
}

/// What `home` costs an hour to rent, cr, with `crew` on its panel's list: past four, each adds 2%,
/// past ten 4% (Gates' `group_pct`), at most three times the rent.
pub fn hourly(home: &Home, crew: usize) -> f32 {
    let extra = (crew.saturating_sub(4).min(6) * 2 + crew.saturating_sub(10) * 4) as f32 / 100.0;
    rent(home) as f32 / (WEEK_S / HOUR_S) as f32 * (1.0 + extra).min(3.0)
}

/// What a home is furnished with (`docs/OPEN_CITY.md`, H2): bought from its panel's catalogue,
/// placed in build mode, seen by everyone who comes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Furniture {
    Bed = 0,
    Sofa = 1,
    Armchair = 2,
    Table = 3,
    Chair = 4,
    Kitchen = 5,
    Wardrobe = 6,
    Chest = 7,
    Safe = 8,
    Lamp = 9,
    Screen = 10,
    Plant = 11,
    Rug = 12,
    /// A model of the pilot's own suit, in its livery, on a stand (`PEERS.md`: people want their
    /// own build).
    SuitModel = 13,
}

/// A piece of furniture's footprint across and deep and its height (m), its price (cr), whether
/// people walk into it (a rug they walk over), and how many kinds of things it holds (a chest's,
/// a safe's; 0: nothing).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FurnitureSpec {
    pub kind: Furniture,
    pub slug: &'static str,
    pub name: &'static str,
    pub size: (f32, f32, f32),
    pub price: u64,
    pub solid: bool,
    pub holds: u8,
}

const fn f(
    kind: Furniture,
    slug: &'static str,
    name: &'static str,
    size: (f32, f32, f32),
    price: u64,
    solid: bool,
    holds: u8,
) -> FurnitureSpec {
    FurnitureSpec { kind, slug, name, size, price, solid, holds }
}

/// The catalogue, in its order on the page.
pub const FURNITURE: [FurnitureSpec; 14] = {
    use Furniture::*;
    [
        f(Bed, "bed", "a bed", (1.6, 2.1, 0.6), 900, true, 0),
        f(Sofa, "sofa", "a sofa", (2.2, 0.95, 0.85), 700, true, 0),
        f(Armchair, "armchair", "an armchair", (0.9, 0.9, 0.95), 300, true, 0),
        f(Table, "table", "a table", (1.6, 0.9, 0.75), 350, true, 0),
        f(Chair, "chair", "a chair", (0.5, 0.5, 0.9), 80, true, 0),
        f(Kitchen, "kitchen", "a kitchen line", (2.4, 0.65, 0.95), 1_200, true, 0),
        f(Wardrobe, "wardrobe", "a wardrobe", (1.2, 0.6, 2.0), 450, true, 0),
        f(Chest, "chest", "a chest", (1.0, 0.6, 0.6), 250, true, 12),
        f(Safe, "safe", "a safe", (0.7, 0.7, 1.0), 2_500, true, 6),
        f(Lamp, "lamp", "a lamp", (0.4, 0.4, 1.6), 120, true, 0),
        f(Screen, "screen", "a screen", (1.6, 0.3, 1.0), 600, true, 0),
        f(Plant, "plant", "a plant", (0.5, 0.5, 1.2), 60, true, 0),
        f(Rug, "rug", "a rug", (2.4, 1.7, 0.01), 200, false, 0),
        f(SuitModel, "suit_model", "a model of your suit", (0.8, 0.8, 1.4), 1_500, true, 0),
    ]
};

impl Furniture {
    pub fn spec(self) -> &'static FurnitureSpec {
        &FURNITURE[self as usize]
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        FURNITURE.iter().find(|f| f.slug == slug).map(|f| f.kind)
    }

    pub fn from_u8(k: u8) -> Option<Self> {
        FURNITURE.get(k as usize).map(|f| f.kind)
    }
}

/// The most pieces a home holds.
pub const MAX_PIECES: usize = 64;
/// What the colony pays back for a piece sold, %.
pub const FURNITURE_BACK_PCT: u64 = 50;
/// How far in front of a home's door its floor is kept clear, m (nobody's walled in or out).
pub const DOOR_CLEAR: f32 = 1.0;

/// Where a piece of furniture stands in `home`: its middle `at` (m from the home's floor's −s, −x
/// corner), turned `turn` quarter turns (0: its width across the strip), on storey `storey` (a
/// house's; a flat's is 0). The box people meet (a rug's too: they walk over it).
pub fn piece_box(home: &Home, kind: Furniture, at: (f32, f32), turn: u8, storey: u32) -> CityBox {
    let (w, d, h) = kind.spec().size;
    let (ws, wx) = if turn.is_multiple_of(2) { (w, d) } else { (d, w) };
    let (s, x) = (home.rect.s0 + at.0, home.rect.x0 + at.1);
    let floor = floor_of(home, storey);
    CityBox {
        rect: Rect::new(s - ws * 0.5, s + ws * 0.5, x - wx * 0.5, x + wx * 0.5),
        h0: floor,
        h1: floor + h,
    }
}

/// Storey `storey`'s floor in `home`, m up.
pub fn floor_of(home: &Home, storey: u32) -> f32 {
    if home.kind == Kind::Houses { crate::colony::homes::level(storey) } else { home.floor }
}

/// Whether a piece standing at `b` (`piece_box`) on storey `storey` fits in `home` (of building
/// `plan`): on its floor, inside its walls, clear of its stairs and rails, a metre clear of its door,
/// and off the other pieces there (`others`: their boxes, and whether they're solid); a rug lies
/// under anything. Why not, if it doesn't.
pub fn fits(
    home: &Home,
    plan: &Plan,
    kind: Furniture,
    b: &CityBox,
    storey: u32,
    mut others: impl Iterator<Item = (CityBox, bool)>,
) -> Result<(), &'static str> {
    if storey >= home.storeys {
        return Err("there's no such floor");
    }
    if !home.rect.inset(-0.01).holds(&b.rect) {
        return Err("it doesn't fit inside the walls there");
    }
    if storey == 0 {
        let (ds, dx) = home.inside;
        let clear = Rect::new(ds - DOOR_CLEAR, ds + DOOR_CLEAR, dx - DOOR_CLEAR, dx + DOOR_CLEAR);
        if b.rect.overlaps(&clear) && kind.spec().solid {
            return Err("keep the door clear");
        }
    }
    if plan.each_box_near(&b.rect, b.h0 + 0.02, b.h1.max(b.h0 + 0.05), &mut |_| true) {
        return Err("it's in a wall, the stairs or a rail there");
    }
    if kind.spec().solid && others.any(|(o, solid)| solid && o.h0 == b.h0 && o.rect.overlaps(&b.rect)) {
        return Err("something's already there");
    }
    Ok(())
}

/// Where someone asleep in the bed standing at `bed` (`piece_box`) gets up in `home` (of building
/// `plan`): beside it, on the first of its sides (its long sides first) with room to stand there,
/// inside the walls and off the stairs, the rails and the other pieces `others`; else just inside
/// the door. Its `(s, x)`, m in the city, on the bed's floor.
pub fn wake_spot(home: &Home, plan: &Plan, bed: &CityBox, others: &[CityBox]) -> (f32, f32) {
    // Half a pilot's room to stand in, m, and how far off the bed's side.
    const STAND: f32 = 0.3;
    const OFF: f32 = STAND + 0.1;
    let r = bed.rect;
    let (ms, mx) = r.middle();
    let along_x = r.length() >= r.width();
    let (long, ends) = ([(r.s0 - OFF, mx), (r.s1 + OFF, mx)], [(ms, r.x0 - OFF), (ms, r.x1 + OFF)]);
    let sides =
        if along_x { [long[0], long[1], ends[0], ends[1]] } else { [ends[0], ends[1], long[0], long[1]] };
    sides
        .into_iter()
        .find(|&(s, x)| {
            let me = Rect::new(s - STAND, s + STAND, x - STAND, x + STAND);
            home.rect.holds(&me)
                && !plan.each_box_near(&me, bed.h0 + 0.05, bed.h0 + 1.8, &mut |_| true)
                && !others.iter().any(|o| o.h1 > bed.h0 + 0.05 && o.rect.overlaps(&me))
        })
        .unwrap_or(home.inside)
}

#[cfg(test)]
mod furniture_tests {
    use super::*;
    use crate::colony::city::{CITY, ROWS, Stage, block, lots};
    use crate::colony::homes::plan;

    #[test]
    fn a_sofa_fits_in_the_middle_of_a_flat_but_not_in_its_doorway_or_on_another() {
        let (p, home) = (CITY.0..CITY.1)
            .flat_map(|bx| (1..=ROWS).filter_map(move |row| block(0, bx, row, Stage(0))))
            .find_map(|b| {
                lots(&b).as_slice().iter().find_map(|bd| plan(&b, bd).filter(|p| p.kind == Kind::Flats))
            })
            .map(|p| (p, p.home(1, 0).unwrap()))
            .unwrap();
        let mid = (home.rect.width() * 0.5, home.rect.length() * 0.5);
        let sofa = piece_box(&home, Furniture::Sofa, mid, 0, 0);
        assert_eq!(fits(&home, &p, Furniture::Sofa, &sofa, 0, core::iter::empty()), Ok(()));
        assert!(
            fits(&home, &p, Furniture::Table, &sofa, 0, [(sofa, true)].into_iter()).is_err(),
            "on the sofa"
        );
        let rug = piece_box(&home, Furniture::Rug, mid, 1, 0);
        assert_eq!(fits(&home, &p, Furniture::Rug, &rug, 0, [(sofa, true)].into_iter()), Ok(()), "under it");
        let door = (home.inside.0 - home.rect.s0, home.inside.1 - home.rect.x0);
        let blocking = piece_box(&home, Furniture::Wardrobe, door, 0, 0);
        assert!(fits(&home, &p, Furniture::Wardrobe, &blocking, 0, core::iter::empty()).is_err());
        let out = piece_box(&home, Furniture::Bed, (-1.0, mid.1), 0, 0);
        assert!(fits(&home, &p, Furniture::Bed, &out, 0, core::iter::empty()).is_err(), "through the wall");
    }

    #[test]
    fn a_sleeper_gets_up_beside_their_bed_where_there_is_room() {
        let (p, home) = (CITY.0..CITY.1)
            .flat_map(|bx| (1..=ROWS).filter_map(move |row| block(0, bx, row, Stage(0))))
            .find_map(|b| {
                lots(&b).as_slice().iter().find_map(|bd| plan(&b, bd).filter(|p| p.kind == Kind::Flats))
            })
            .map(|p| (p, p.home(1, 0).unwrap()))
            .unwrap();
        let mid = (home.rect.width() * 0.5, home.rect.length() * 0.5);
        let bed = piece_box(&home, Furniture::Bed, mid, 0, 0);
        let up = wake_spot(&home, &p, &bed, &[bed]);
        let (bs, bx) = bed.rect.middle();
        assert!((up.0 - bs).abs() < 2.0 && (up.1 - bx).abs() < 2.0, "beside it: {up:?}");
        assert!(!bed.rect.contains(up.0, up.1) && home.rect.contains(up.0, up.1));
        // Hemmed in on every side, they get up inside the door.
        let mut all = [bed; 5];
        for (b, (ds, dx)) in all.iter_mut().zip([(-1.2, 0.0), (1.2, 0.0), (0.0, -1.5), (0.0, 1.5)]) {
            *b = piece_box(&home, Furniture::Wardrobe, (mid.0 + ds, mid.1 + dx), 0, 0);
        }
        let hemmed = wake_spot(&home, &p, &bed, &all);
        assert!(
            hemmed == home.inside || !all.iter().any(|b| b.rect.contains(hemmed.0, hemmed.1)),
            "{hemmed:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::city::{CITY, ROWS, Stage, block, lots};
    use crate::colony::homes::plan;

    #[test]
    fn rents_and_prices_are_in_the_pilots_reach() {
        let (mut lo, mut hi, mut dear) = (u64::MAX, 0, 0);
        for bx in CITY.0..CITY.0 + 160 {
            for row in -ROWS..=ROWS {
                let Some(b) = block(0, bx, row, Stage(0)) else { continue };
                for bd in lots(&b).as_slice() {
                    let Some(p) = plan(&b, bd) else { continue };
                    for unit in 0..p.homes_per_storey() {
                        let home = p.home(0, unit).unwrap();
                        let r = rent(&home);
                        assert_eq!(r % 10, 0);
                        assert!(price(&home) > sell_back(&home));
                        if bought(&home) {
                            dear = dear.max(price(&home));
                        } else {
                            (lo, hi) = (lo.min(r), hi.max(r));
                        }
                    }
                }
            }
        }
        // A flat's week is a patrol's pay or less; a house is a lot of patrols.
        assert!(lo >= 150 && hi <= 1_500, "rents {lo}..{hi}");
        assert!(dear > 30_000 && dear < 400_000, "the dearest {dear}");
    }

    #[test]
    fn a_big_crew_pays_more_but_never_more_than_three_times() {
        let b = (CITY.0..CITY.1)
            .flat_map(|bx| (1..=ROWS).filter_map(move |row| block(0, bx, row, Stage(0))))
            .find_map(|b| lots(&b).as_slice().iter().find_map(|bd| plan(&b, bd)))
            .unwrap();
        let home = b.home(0, 0).unwrap();
        assert_eq!(hourly(&home, 1), hourly(&home, 4));
        assert!(hourly(&home, 5) > hourly(&home, 4));
        assert!(hourly(&home, 200) <= hourly(&home, 1) * 3.0 + 1e-3);
    }
}
