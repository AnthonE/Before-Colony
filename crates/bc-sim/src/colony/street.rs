//! The town's street life (`docs/FREE_ROAM.md`, P1, the ambient tier): its traffic and its people on
//! foot as closed forms of the tick and of where you ask, as the cylinder's `traffic` and `walkers`
//! were. Every client works out the same cars and the same people from the tick; nothing about them
//! is stored or sent, and nothing here allocates.
//!
//! **Traffic** runs round the ring and never across it: every lane ([`lane`]) is a loop the whole way
//! round, its cars in places a fixed way apart, all at the lane's own speed, so no car's path ever
//! crosses another's and none closes on the one ahead. The boulevard's carriageways run one way each
//! (three lanes of them, the fastest by the median); the rim roads and the streets between the
//! columns of blocks carry a lane each way, keeping right. A lap takes a whole number of ticks, so
//! where a car is depends only on the tick within its lane's lap.
//!
//! **People** walk round their own block ([`each_person`]): inside its kerb, clear of its buildings,
//! a lap of it a whole number of ticks; some stand about in the lifts' squares; and strollers go
//! round the ring along the rim walks. Nobody steps into a road.
//!
//! Positions are the town's own: `x` across the ring, `t` round it from Hub Gate (given in the turn
//! of the ring they were asked in, so a span astride Hub Gate comes back whole). Headings are the
//! walker's yaw (0 faces −t; π the way `t` grows).

use core::f32::consts::{FRAC_PI_2, PI};

use super::town::{
    self, BOULEVARD_HALF, BlockId, BlockUse, CELL, CELLS, COLUMNS, MEDIAN_HALF, RIM_ROAD, RIM_WALK, ROUND,
    STREET,
};
use crate::TICK_HZ;
use crate::content::town::DistrictKind;
use crate::math::floor;
use crate::world::COLONY_HALF_WIDTH;

const HZ: f32 = TICK_HZ as f32;

/// What's driven.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Car,
    Van,
    Taxi,
    /// One of the Watch's cars, on its beat.
    Watch,
}

/// A lane of the town's traffic: a loop round the ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lane {
    /// Across the ring, m.
    pub x: f32,
    /// The way it runs: 1 the way `t` grows, −1 back.
    pub dir: f32,
    /// A lap of the ring, ticks.
    pub lap: u32,
    /// Its places for cars, round the ring.
    pub slots: u32,
    /// The share of its places with a car in them.
    pub fill: f32,
}

impl Lane {
    /// How fast its cars go, m/s.
    pub fn speed(&self) -> f32 {
        ROUND * HZ / self.lap as f32
    }

    /// How far apart its places are, m.
    pub fn pitch(&self) -> f32 {
        ROUND / self.slots as f32
    }
}

const fn lane_of(x: f32, dir: f32, speed: f32, pitch: f32, fill: f32) -> Lane {
    Lane { x, dir, lap: (ROUND * HZ / speed + 0.5) as u32, slots: (ROUND / pitch) as u32, fill }
}

/// The boulevard's lanes out from its median (its carriageways' middles are ±20; lanes 6 m wide), and
/// how fast each runs, m/s: the kerb's lane is left clear.
const BOULEVARD_LANES: [(f32, f32); 3] =
    [(MEDIAN_HALF + 3.0, 19.0), (MEDIAN_HALF + 9.0, 16.0), (MEDIAN_HALF + 15.0, 13.0)];
/// A two-way road's lane off its middle, m: the streets', and the rim roads'.
const STREET_LANE: f32 = 1.75;
const RIM_LANE: f32 = 1.875;
/// The streets between columns of blocks (a column's far edge where a street is what's there).
const STREETS: usize = 10;
/// How many lanes there are.
pub const LANES: usize = 2 * BOULEVARD_LANES.len() + 4 + 2 * STREETS;

/// The street along column `c`'s far edge, if a street is what's there: its middle, m across the ring.
fn street_x(c: u8) -> Option<f32> {
    let (_, x1, _, hi) = town::column(c);
    (hi == STREET * 0.5).then_some(x1)
}

/// Lane `k` of the town's traffic (0 to [`LANES`]): the boulevard's, the rim roads', the streets'.
/// Traffic keeps right: looking the way `t` grows, right is aft (+x).
pub fn lane(k: usize) -> Option<Lane> {
    let b = BOULEVARD_LANES.len();
    if k < 2 * b {
        // Aft of the median the way `t` grows; fore of it, back.
        let (off, speed) = BOULEVARD_LANES[k % b];
        let dir = if k < b { 1.0 } else { -1.0 };
        return Some(lane_of(dir * off, dir, speed, 34.0, 0.5));
    }
    let k = k - 2 * b;
    if k < 4 {
        // The rim roads: fore and aft, a lane each way.
        let mid = (COLONY_HALF_WIDTH - RIM_WALK - 0.5 * RIM_ROAD) * if k < 2 { -1.0 } else { 1.0 };
        let dir = if k.is_multiple_of(2) { 1.0 } else { -1.0 };
        return Some(lane_of(mid + dir * RIM_LANE, dir, 12.0, 30.0, 0.3));
    }
    let k = k - 4;
    let street = k / 2;
    let mut n = 0;
    let mut c = 0;
    while c < COLUMNS {
        if let Some(x) = street_x(c) {
            if n == street {
                let dir = if k.is_multiple_of(2) { 1.0 } else { -1.0 };
                // Each street at its own pace, so they don't run in step.
                let speed = 8.5 + 0.45 * (street % 5) as f32;
                return Some(lane_of(x + dir * STREET_LANE, dir, speed, 22.0, 0.42));
            }
            n += 1;
        }
        c += 1;
    }
    None
}

/// A car of the town's traffic, where the tick has it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Car {
    /// Its lane and its place in it: the same car all the way round.
    pub id: u32,
    pub x: f32,
    pub t: f32,
    pub yaw: f32,
    pub speed: f32,
    pub kind: Kind,
    pub seed: u32,
}

/// How far a lane's traffic has gone round at tick `tick` plus `frac`, m (the way it runs).
fn gone(l: &Lane, tick: u32, frac: f32) -> f32 {
    ((tick % l.lap) as f32 + frac.clamp(0.0, 1.0)) / l.lap as f32 * ROUND
}

/// Every car of the town's traffic within `x0..x1` across the ring and `t0..t1` round it (any `t`,
/// either side of Hub Gate) at tick `tick` plus `frac` of the next.
pub fn each_car(x0: f32, x1: f32, t0: f32, t1: f32, tick: u32, frac: f32, f: &mut impl FnMut(&Car)) {
    for k in 0..LANES {
        let Some(l) = lane(k) else { continue };
        if l.x < x0 || l.x > x1 {
            continue;
        }
        let pitch = l.pitch();
        let off = l.dir * gone(&l, tick, frac);
        // Place `i` is at `(i + ½) pitch + off`, whichever turn of the ring `i` counts in.
        let (i0, i1) = (floor((t0 - off) / pitch - 0.5) as i32 + 1, floor((t1 - off) / pitch - 0.5) as i32);
        for i in i0..=i1 {
            let slot = i.rem_euclid(l.slots as i32) as u32;
            let seed = town::mix(k as u32, slot, 0xCA7);
            if town::unit(seed, 1) >= l.fill {
                continue;
            }
            let kind = match seed % 23 {
                0 => Kind::Watch,
                1..=3 => Kind::Taxi,
                4..=7 => Kind::Van,
                _ => Kind::Car,
            };
            f(&Car {
                id: (k as u32) << 16 | slot,
                x: l.x,
                t: (i as f32 + 0.5) * pitch + off,
                yaw: if l.dir > 0.0 { PI } else { 0.0 },
                speed: l.speed(),
                kind,
                seed,
            });
        }
    }
}

/// How somebody on foot holds themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    Walk,
    Stand,
}

/// One of the town's people on foot, where the tick has them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Person {
    /// Who they are: the same person every tick.
    pub id: u32,
    pub x: f32,
    pub t: f32,
    pub yaw: f32,
    pub speed: f32,
    /// Where they are in their stride, 0 to 1.
    pub stride: f32,
    pub pose: Pose,
    pub seed: u32,
}

/// How far in from its kerb a block's people walk, m (and the other way's file, [`FILE`] further in):
/// clear of its buildings, which stand 2 m in.
pub const WALK_IN: f32 = 0.8;
const FILE: f32 = 0.6;
/// A stride's length, m.
const STRIDE: f32 = 1.45;
/// The most people a block has walking round it, and standing about in it.
pub const MAX_WALKERS: u32 = 7;
pub const MAX_STANDING: u32 = 8;
/// The rim walks' strollers: where their two files walk off a rim walk's middle (m), and how far
/// apart their places are.
const STROLL_OFF: f32 = 9.0;
const STROLL_PITCH: f32 = 26.0;
const STROLL_SPEED: f32 = 1.25;

/// How many walk round block `id`, and how many stand about in it.
pub fn people_on(id: BlockId) -> (u32, u32) {
    let h = town::mix(u32::from(id.cell), u32::from(id.column), 0x9E0);
    let extra = h % 3;
    match town::block_use(id) {
        BlockUse::Square => (5 + extra, 4 + (h >> 4) % 5),
        BlockUse::Park => (2 + extra, (h >> 4) % 3),
        BlockUse::Place(_) => (2 + extra, 0),
        BlockUse::Frames => (h % 2, 0),
        BlockUse::Built => match town::district(id.district()).kind {
            DistrictKind::Business | DistrictKind::Midtown => (4 + extra, 0),
            DistrictKind::Civic | DistrictKind::OldTown => (3 + extra, 0),
            DistrictKind::Residential => (2 + extra, 0),
            DistrictKind::Works => (h % 3, 0),
            DistrictKind::Park | DistrictKind::Site => (extra, 0),
        },
    }
}

/// The seed of the person `id` names (their look comes from it), wherever the tick has them.
pub fn seed_of(id: u32) -> u32 {
    if id >> 28 == 0xF {
        return town::mix(0x57 + ((id >> 16) & 0xFFF), id & 0xFFFF, 0x5720);
    }
    let (block, j) = (id >> 4, id & 15);
    if j < 8 { town::mix(block, j, 0xF007) } else { town::mix(block, j - 8, 0x57A9D) }
}

/// Where `u` m round a rectangle's edge is (from its low corner, along `x` first), and the heading
/// there going that way round; `back` goes the other way.
fn round_rect(r: &town::TownBox, u: f32, back: bool) -> (f32, f32, f32) {
    let (w, h) = (r.x1 - r.x0, r.t1 - r.t0);
    // Headings: +x is τ/4, the way `t` grows π, −x 3τ/4, back round `t` 0.
    let (x, t, yaw) = if u < w {
        (r.x0 + u, r.t0, FRAC_PI_2)
    } else if u < w + h {
        (r.x1, r.t0 + (u - w), PI)
    } else if u < 2.0 * w + h {
        (r.x1 - (u - w - h), r.t1, 3.0 * FRAC_PI_2)
    } else {
        (r.x0, r.t1 - (u - 2.0 * w - h), 0.0)
    };
    (x, t, if back { yaw + PI } else { yaw })
}

/// Everyone on foot in the town within `x0..x1` across the ring and `t0..t1` round it (any `t`) at
/// tick `tick` plus `frac` of the next: round their blocks, standing in the squares, and strolling
/// the rim walks.
pub fn each_person(x0: f32, x1: f32, t0: f32, t1: f32, tick: u32, frac: f32, f: &mut impl FnMut(&Person)) {
    let frac = frac.clamp(0.0, 1.0);
    let cells = i32::from(CELLS);
    let (c0, c1) = (floor(t0 / CELL) as i32, floor(t1 / CELL) as i32);
    for cu in c0..=c1 {
        let cell = cu.rem_euclid(cells) as u16;
        let turn = (cu - i32::from(cell)) as f32 * CELL;
        for c in 0..COLUMNS {
            let (cx0, cx1, ..) = town::column(c);
            if cx1 < x0 || cx0 > x1 {
                continue;
            }
            let id = BlockId::new(cell, c);
            let r = town::block_rect(id).inset(WALK_IN);
            let (walking, standing) = people_on(id);
            let lap_m = 2.0 * ((r.x1 - r.x0) + (r.t1 - r.t0));
            let block = u32::from(cell) * u32::from(COLUMNS) + u32::from(c);
            for j in 0..walking.min(MAX_WALKERS) {
                let seed = town::mix(block, j, 0xF007);
                let speed = 1.15 + 0.45 * town::unit(seed, 1);
                let lap = (lap_m / speed * HZ + 0.5) as u32;
                let back = seed & 1 == 1;
                // Each a file's width off the other way's, so two meeting pass.
                let lane = if back { 0.0 } else { FILE };
                let path = r.inset(lane);
                let path_m = 2.0 * ((path.x1 - path.x0) + (path.t1 - path.t0));
                let phase = (((tick % lap) as f32 + frac) / lap as f32 + town::unit(seed, 2)) % 1.0;
                let u = if back { 1.0 - phase } else { phase } * path_m;
                let (x, t, yaw) = round_rect(&path, u.min(path_m - 1e-3), back);
                let t = t + turn;
                if x < x0 || x > x1 || t < t0 || t > t1 {
                    continue;
                }
                let stride = phase * path_m / STRIDE;
                f(&Person {
                    id: block << 4 | j,
                    x,
                    t,
                    yaw,
                    speed: path_m * HZ / lap as f32,
                    stride: stride - floor(stride),
                    pose: Pose::Walk,
                    seed,
                });
            }
            for j in 0..standing.min(MAX_STANDING) {
                let seed = town::mix(block, j, 0x57A9D);
                let inner = r.inset(6.0);
                let x = inner.x0 + (inner.x1 - inner.x0) * town::unit(seed, 1);
                let t = inner.t0 + (inner.t1 - inner.t0) * town::unit(seed, 2) + turn;
                if x < x0 || x > x1 || t < t0 || t > t1 {
                    continue;
                }
                f(&Person {
                    id: block << 4 | (8 + j),
                    x,
                    t,
                    yaw: town::unit(seed, 3) * 2.0 * PI,
                    speed: 0.0,
                    stride: 0.0,
                    pose: Pose::Stand,
                    seed,
                });
            }
        }
    }
    // The rim walks' strollers: a file each way along each, round the whole ring.
    let slots = (ROUND / STROLL_PITCH) as u32;
    let pitch = ROUND / slots as f32;
    let lap = (ROUND / STROLL_SPEED * HZ + 0.5) as u32;
    for k in 0..4u32 {
        let side = if k < 2 { -1.0 } else { 1.0 };
        let dir = if k.is_multiple_of(2) { 1.0 } else { -1.0 };
        let x = side * (COLONY_HALF_WIDTH - 0.5 * RIM_WALK) + dir * STROLL_OFF;
        if x < x0 || x > x1 {
            continue;
        }
        let gone = ((tick % lap) as f32 + frac) / lap as f32 * ROUND;
        let off = dir * gone;
        let (i0, i1) = (floor((t0 - off) / pitch - 0.5) as i32 + 1, floor((t1 - off) / pitch - 0.5) as i32);
        for i in i0..=i1 {
            let slot = i.rem_euclid(slots as i32) as u32;
            let seed = town::mix(0x57 + k, slot, 0x5720);
            if town::unit(seed, 1) >= 0.4 {
                continue;
            }
            let stride = (gone + town::unit(seed, 2) * STRIDE) / STRIDE;
            f(&Person {
                id: 0xF000_0000 | k << 16 | slot,
                x: x + (town::unit(seed, 3) - 0.5) * 6.0,
                t: (i as f32 + 0.5) * pitch + off,
                yaw: if dir > 0.0 { PI } else { 0.0 },
                speed: ROUND * HZ / lap as f32,
                stride: stride - floor(stride),
                pose: Pose::Walk,
                seed,
            });
        }
    }
}

/// The boulevard's kerbs, for the tests: nothing of the traffic is outside its carriageways.
pub const BOULEVARD: (f32, f32) = (MEDIAN_HALF, BOULEVARD_HALF);

#[cfg(test)]
#[allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]
mod tests {
    use super::*;
    use crate::colony::town::{Ground, TownBox, ground_at};
    use alloc::vec::Vec;

    fn cars(x0: f32, x1: f32, t0: f32, t1: f32, tick: u32, frac: f32) -> Vec<Car> {
        let mut out = Vec::new();
        each_car(x0, x1, t0, t1, tick, frac, &mut |c| out.push(*c));
        out
    }

    fn people(x0: f32, x1: f32, t0: f32, t1: f32, tick: u32, frac: f32) -> Vec<Person> {
        let mut out = Vec::new();
        each_person(x0, x1, t0, t1, tick, frac, &mut |p| out.push(*p));
        out
    }

    const HALF: f32 = COLONY_HALF_WIDTH;

    #[test]
    fn every_lane_is_on_a_road_keeping_right_and_laps_the_ring_in_whole_ticks() {
        let mut n = 0;
        for k in 0..LANES {
            let l = lane(k).unwrap_or_else(|| panic!("lane {k}"));
            n += 1;
            for t in [3.0f32, 700.0, 5_000.0, ROUND - 40.0] {
                // (Not where a lift's square takes the avenue: the boulevard and the streets run on.)
                assert_eq!(ground_at(l.x, t + CELL * 0.5), Ground::Road, "lane {k} at {t}");
            }
            assert!((6.0..22.0).contains(&l.speed()), "lane {k}: {} m/s", l.speed());
            assert!(l.pitch() > 12.0 && l.slots > 100, "lane {k}");
            // A lap's worth of ticks on, the same cars in the same places.
            let (a, b) = (
                cars(l.x - 0.1, l.x + 0.1, 0.0, 900.0, 77, 0.25),
                cars(l.x - 0.1, l.x + 0.1, 0.0, 900.0, 77 + l.lap, 0.25),
            );
            assert!(!a.is_empty(), "lane {k} is empty for 900 m");
            assert_eq!(a.len(), b.len());
            for (p, q) in a.iter().zip(&b) {
                assert!(p.id == q.id && (p.t - q.t).abs() < 1e-2, "lane {k}");
            }
        }
        assert_eq!(n, LANES);
        assert!(lane(LANES).is_none());
        // Keeping right: aft of a road's middle the way `t` grows.
        let all = cars(-HALF, HALF, 0.0, 600.0, 1_000, 0.0);
        for c in &all {
            let before = cars(c.x - 0.1, c.x + 0.1, c.t - 30.0, c.t + 30.0, 999, 0.0);
            let was = before.iter().find(|b| b.id == c.id).expect("the same car a tick before");
            let went = c.t - was.t;
            assert!((went.abs() - c.speed / HZ).abs() < 2e-2, "{c:?} went {went}");
            assert_eq!(went > 0.0, c.yaw == PI, "{c:?}");
        }
        // On the boulevard, aft of the median runs the way `t` grows.
        assert!(all.iter().filter(|c| c.x.abs() < BOULEVARD_HALF).all(|c| (c.x > 0.0) == (c.yaw == PI)));
        assert!(
            all.iter().any(|c| c.kind == Kind::Watch)
                || cars(-HALF, HALF, 0.0, 4_000.0, 0, 0.0).iter().any(|c| c.kind == Kind::Watch)
        );
    }

    #[test]
    fn cars_never_meet_and_come_back_whole_astride_hub_gate() {
        for tick in [0u32, 12_345, 4_000_000_000] {
            let all = cars(-HALF, HALF, -400.0, 400.0, tick, 0.5);
            assert!(all.len() > 60, "{} cars within 400 m of Hub Gate", all.len());
            for (i, a) in all.iter().enumerate() {
                assert!((-400.0..=400.0).contains(&a.t));
                for b in &all[i + 1..] {
                    let (dx, dt) = ((a.x - b.x).abs(), (a.t - b.t).abs());
                    assert!(dx > 3.0 || dt > 12.0, "{a:?} and {b:?}");
                }
            }
            // The same cars asked for a turn of the ring on.
            let turn = cars(-HALF, HALF, ROUND - 400.0, ROUND + 400.0, tick, 0.5);
            assert_eq!(all.len(), turn.len());
            for (a, b) in all.iter().zip(&turn) {
                assert!(a.id == b.id && (a.t + ROUND - b.t).abs() < 0.05, "{a:?} vs {b:?}");
            }
        }
    }

    #[test]
    fn people_keep_to_their_blocks_and_the_rim_walks_clear_of_every_wall_and_road() {
        let mut n = 0;
        for (tick, t0) in [(0u32, -300.0f32), (9_999, 1_500.0), (700_000, 9_000.0), (31, ROUND - 200.0)] {
            let all = people(-HALF, HALF, t0, t0 + 500.0, tick, 0.3);
            assert!(all.len() > 150, "{} people in 500 m of the ring", all.len());
            for p in &all {
                let g = ground_at(p.x, p.t);
                assert!(matches!(g, Ground::Block(_) | Ground::RimWalk), "{p:?} is on {g:?}");
                let q = TownBox::new(p.x - 0.3, p.x + 0.3, p.t - 0.3, p.t + 0.3, 0.1, 1.8);
                let mut clear = true;
                town::each_box(q.x0, q.x1, q.t0, q.t1, &mut |b| clear &= !b.overlaps(&q));
                assert!(clear, "{p:?} is in a wall");
                assert!((0.0..1.0).contains(&p.stride) && (t0..=t0 + 500.0).contains(&p.t));
                n += 1;
            }
            // Each one's seed is their id's.
            assert!(all.iter().all(|p| seed_of(p.id) == p.seed));
            // Nobody twice.
            let mut ids: Vec<u32> = all.iter().map(|p| p.id).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), all.len());
        }
        assert!(n > 800);
        // A walker goes on at their own pace, the way they face; someone standing stays put.
        let (a, b) = (people(-HALF, HALF, 0.0, 300.0, 500, 0.0), people(-HALF, HALF, -5.0, 305.0, 530, 0.0));
        let mut walked = 0;
        for p in &a {
            let Some(q) = b.iter().find(|q| q.id == p.id) else { continue };
            let d = libm::hypotf(q.x - p.x, q.t - p.t);
            match p.pose {
                Pose::Stand => assert!(d < 1e-3),
                Pose::Walk => {
                    // (Less across a corner of their block.)
                    assert!(d <= p.speed + 0.05 && d > p.speed * 0.6, "{p:?} went {d} m in a second");
                    walked += 1;
                }
            }
        }
        assert!(walked > 80, "{walked}");
        // The squares have people standing about.
        let (x, t) = town::out_of_the_lift(0);
        let near = people(x - 120.0, x + 120.0, t - 120.0, t + 120.0, 0, 0.0);
        assert!(near.iter().filter(|p| p.pose == Pose::Stand).count() >= 4, "round Hub Gate's lift");
    }
}
