//! The town's census (`docs/LIFE.md`, 7.2; `docs/FREE_ROAM.md`, 3): everyone in the ambient crowd
//! (`street::each_person`) is somebody. Their id, the same every tick, hashes to a name, a home on a
//! block people live on and a job, against the town's plan and `content::town`'s names: a closed
//! form, never stored or sent, the same person on every screen and on the server. It's what the
//! tiers over the crowd stand on: a person promoted, followed or spoken to keeps who they were.

use super::street;
use super::town::{self, BlockId, BlockUse, CELLS, CELLS_PER_DISTRICT, COLUMNS, DISTRICT_COUNT, mix, unit};
use crate::content::town::{DistrictKind, FAMILY_NAMES, GIVEN_NAMES, PLACES, PlaceKind};

/// Who somebody in the town is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    /// Their given and family names (indices into `content::town`'s [`GIVEN_NAMES`] and
    /// [`FAMILY_NAMES`]).
    pub given: u16,
    pub family: u16,
    /// The block they live on: one of the town's homes ([`is_home`]).
    pub home: BlockId,
    pub job: Job,
}

impl Identity {
    pub fn given_name(&self) -> &'static str {
        GIVEN_NAMES[usize::from(self.given) % GIVEN_NAMES.len()]
    }

    pub fn family_name(&self) -> &'static str {
        FAMILY_NAMES[usize::from(self.family) % FAMILY_NAMES.len()]
    }
}

/// Where somebody works.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Job {
    /// On a block of the town's ([`is_work`]): in its buildings (offices and towers, halls, shops,
    /// workshops, depots, as its district builds them), out on the building site's frames, or keeping
    /// a park.
    Block(BlockId),
    /// At one of the colony's key places, on its block: the Exchange floor, The Arrival, the Charter
    /// Board's hall (`content::town::PLACES`), or a station of the Watch (`town::watch_station`).
    Place(PlaceKind, BlockId),
}

impl Job {
    /// The block they work on.
    pub fn block(self) -> BlockId {
        match self {
            Job::Block(b) | Job::Place(_, b) => b,
        }
    }
}

/// Whether people live on block `id`: the houses and flats of the districts where people live (Lock
/// Town, the Terraces), Old Town's low houses, and midtown's blocks (the Basin, Green Meridian).
pub fn is_home(id: BlockId) -> bool {
    town::block_use(id) == BlockUse::Built
        && matches!(
            town::district(id.district()).kind,
            DistrictKind::Residential | DistrictKind::OldTown | DistrictKind::Midtown
        )
}

/// Whether people work on block `id`: in its buildings, on its frames or in its park (not a lift's
/// square, nor a key place's site: those are [`Job::Place`]).
pub fn is_work(id: BlockId) -> bool {
    matches!(town::block_use(id), BlockUse::Built | BlockUse::Frames | BlockUse::Park)
}

/// The block someone with id `id` walks round (`street::each_person`), if they do: not the rim walks'
/// strollers.
pub fn own_block(id: u32) -> Option<BlockId> {
    if id >> 28 == 0xF {
        return None;
    }
    let block = id >> 4;
    let (cell, column) = (block / u32::from(COLUMNS), block % u32::from(COLUMNS));
    (cell < u32::from(CELLS)).then(|| BlockId::new(cell as u16, column as u8))
}

/// How many of the town live in a district of kind `kind`, and work in one, by share.
fn weight(kind: DistrictKind, living: bool) -> u32 {
    use DistrictKind::*;
    match (kind, living) {
        (Residential, true) => 4,
        (OldTown, true) => 3,
        (Midtown, true) => 2,
        (_, true) => 0,
        (Business, false) => 4,
        (Civic | Midtown | Works, false) => 3,
        (OldTown | Site, false) => 2,
        (Residential | Park, false) => 1,
    }
}

/// A district picked by `h`, as many times more likely as its [`weight`].
fn district_by(h: u32, living: bool) -> u16 {
    let total: u32 = (0..DISTRICT_COUNT).map(|d| weight(town::district(d).kind, living)).sum();
    let mut pick = h % total.max(1);
    for d in 0..DISTRICT_COUNT {
        let w = weight(town::district(d).kind, living);
        if pick < w {
            return d;
        }
        pick -= w;
    }
    0
}

/// A block of district `d` that `ok` takes, picked by `h`: the one it lands on, or the next that
/// does, round the district's blocks.
fn block_in(d: u16, h: u32, ok: fn(BlockId) -> bool) -> Option<BlockId> {
    let n = u32::from(CELLS_PER_DISTRICT) * u32::from(COLUMNS);
    let first = h % n;
    (0..n).map(|k| (first + k) % n).find_map(|k| {
        let cell = d * CELLS_PER_DISTRICT + (k / u32::from(COLUMNS)) as u16;
        let id = BlockId::new(cell, (k % u32::from(COLUMNS)) as u8);
        ok(id).then_some(id)
    })
}

/// Who the person with id `id` is (`street::Person::id`: anyone in the crowd, any tick).
pub fn identity(id: u32) -> Identity {
    let seed = street::seed_of(id);
    let h = mix(id, seed, 0xCE_2505);
    let given = (mix(h, 1, 0x61) % GIVEN_NAMES.len() as u32) as u16;
    let family = (mix(h, 2, 0xFA) % FAMILY_NAMES.len() as u32) as u16;
    let own = own_block(id);
    // Most walk round the block they live on, if people live there; the rest live elsewhere.
    let home = match own {
        Some(b) if is_home(b) && unit(h, 3) < 0.6 => b,
        _ => {
            let d = district_by(mix(h, 4, 0x40E), true);
            block_in(d, mix(h, 5, 0x40E), is_home).unwrap_or(BlockId::new(d * CELLS_PER_DISTRICT + 2, 0))
        }
    };
    // One in fifty with the Watch, at their own district's station or the next; a few at a key
    // place; some at work on the block they walk round; the rest somewhere over the town.
    let r = unit(h, 6);
    let job = if r < 0.02 {
        let near = own.or(Some(home)).map_or(0, |b| b.district());
        let d = (near + (mix(h, 7, 0x3A7C) % 3) as u16 + DISTRICT_COUNT - 1) % DISTRICT_COUNT;
        Job::Place(PlaceKind::WatchStation, town::watch_station(d))
    } else if r < 0.035 {
        let p = &PLACES[(mix(h, 8, 0x91A) as usize) % PLACES.len()];
        Job::Place(p.kind, BlockId::new(p.cell, p.column))
    } else {
        match own {
            Some(b) if is_work(b) && !is_home(b) && unit(h, 9) < 0.5 => Job::Block(b),
            _ => {
                let d = district_by(mix(h, 10, 0x30B), false);
                Job::Block(
                    block_in(d, mix(h, 11, 0x30B), is_work).unwrap_or(BlockId::new(d * CELLS_PER_DISTRICT + 2, 0)),
                )
            }
        }
    };
    Identity { given, family, home, job }
}

#[cfg(test)]
#[allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]
mod tests {
    use super::*;
    use crate::colony::town::{ROUND, block_use};
    use crate::world::COLONY_HALF_WIDTH as HALF;
    use alloc::vec;
    use alloc::vec::Vec;

    /// Everyone in the crowd round the whole ring at a tick, by id.
    fn crowd(tick: u32) -> Vec<u32> {
        let mut ids = Vec::new();
        street::each_person(-HALF, HALF, 0.0, ROUND, tick, 0.0, &mut |p| ids.push(p.id));
        ids
    }

    #[test]
    fn everyone_has_a_name_a_home_and_a_job_and_keeps_them() {
        let ids = crowd(4_321);
        assert!(ids.len() > 10_000, "{} people", ids.len());
        let (mut given, mut family) = (vec![0u32; GIVEN_NAMES.len()], vec![0u32; FAMILY_NAMES.len()]);
        let (mut at_home, mut at_own_work, mut watch, mut places) = (0, 0, 0, 0);
        for &id in &ids {
            let who = identity(id);
            assert_eq!(who, identity(id), "the same person twice");
            assert!(!who.given_name().is_empty() && !who.family_name().is_empty());
            given[usize::from(who.given)] += 1;
            family[usize::from(who.family)] += 1;
            assert!(is_home(who.home), "{id:#x} lives on {:?}: {:?}", who.home, block_use(who.home));
            at_home += u32::from(own_block(id) == Some(who.home));
            match who.job {
                Job::Block(b) => {
                    assert!(is_work(b), "{id:#x} works on {b:?}: {:?}", block_use(b));
                    at_own_work += u32::from(own_block(id) == Some(b));
                }
                Job::Place(kind, b) => {
                    assert_eq!(block_use(b), BlockUse::Place(kind), "{id:#x}");
                    if kind == PlaceKind::WatchStation {
                        watch += 1;
                    } else {
                        places += 1;
                    }
                }
            }
        }
        let n = ids.len() as f32;
        // No one name over a few in a hundred.
        for (names, counts) in [(&GIVEN_NAMES[..], &given), (&FAMILY_NAMES[..], &family)] {
            for (name, &k) in names.iter().zip(counts.iter()) {
                assert!(k > 0 && (k as f32) < n * 0.03, "{name}: {k} of {n}");
            }
        }
        // A street's crowd (everyone within a few hundred metres) rarely has two of the same name.
        let (mut seen, mut twice) = (0, 0);
        for t0 in [0.0f32, 3_000.0, 7_777.0, 12_000.0, ROUND - 250.0] {
            let mut names: Vec<(u16, u16)> = Vec::new();
            street::each_person(-HALF, HALF, t0, t0 + 500.0, 4_321, 0.0, &mut |p| {
                let who = identity(p.id);
                names.push((who.given, who.family));
            });
            names.sort_unstable();
            seen += names.len();
            twice += names.windows(2).filter(|w| w[0] == w[1]).count();
        }
        assert!(seen > 1_000 && (twice as f32) < seen as f32 * 0.03, "{twice} of {seen} share a name");
        assert!(at_home as f32 > n * 0.1 && (at_home as f32) < n * 0.7, "{at_home} at home");
        assert!(at_own_work as f32 > n * 0.05, "{at_own_work} at work where they walk");
        assert!((watch as f32) > n * 0.01 && (watch as f32) < n * 0.03, "{watch} with the Watch");
        assert!(places > 0, "nobody at a key place");
        // The rim walks' strollers live and work somewhere too.
        let stroller = ids.iter().find(|id| *id >> 28 == 0xF).expect("a stroller");
        assert_eq!(own_block(*stroller), None);
        assert!(is_home(identity(*stroller).home));
    }

    #[test]
    fn the_districts_people_live_in_have_their_homes() {
        let mut homes = vec![0u32; usize::from(DISTRICT_COUNT)];
        for cell in 0..CELLS {
            for c in 0..COLUMNS {
                let id = BlockId::new(cell, c);
                homes[usize::from(id.district())] += u32::from(is_home(id));
            }
        }
        for d in 0..DISTRICT_COUNT {
            let lives = weight(town::district(d).kind, true) > 0;
            assert_eq!(homes[usize::from(d)] > 100, lives, "district {d}: {} homes", homes[usize::from(d)]);
            assert!(block_in(d, 12_345, is_work).is_some(), "district {d}: nowhere to work");
        }
    }
}
