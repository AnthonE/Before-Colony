//! Drawing the city's life (`bc_sim::colony::traffic`, `walkers`), the parts with no Bevy in them:
//! what each car and person looks like (its body, livery, wear, outfit and build, all from its seed
//! and where it is), what a car has lit, the `MeshTag` that carries all of it to the shader
//! (`shaders/life_lib.wgsl` keeps the same layout; a test reads it), who's drawn at which level of
//! detail, and the keyed pools that keep an entity on the same car or person while they're drawn.

use std::collections::HashMap;

use bc_sim::colony::city::{Stage, district_of, unit};
use bc_sim::colony::traffic::{Blink, Car, Kind};
use bc_sim::content::city::DistrictKind;
use glam::Vec3;

use crate::life_mesh::Body;

/// The palette (sRGB, perceptual roughness): the vehicles' 0..32, the people's 32..64.
pub const PALETTE: [(f32, f32, f32, f32); 64] = [
    // 0..8: the runabouts' pale paints.
    (0.92, 0.93, 0.94, 0.3),
    (0.78, 0.8, 0.82, 0.3),
    (0.93, 0.9, 0.8, 0.3),
    (0.7, 0.8, 0.9, 0.3),
    (0.72, 0.88, 0.8, 0.3),
    (0.93, 0.9, 0.66, 0.3),
    (0.66, 0.68, 0.7, 0.28),
    (0.6, 0.8, 0.82, 0.3),
    // 8..16: the hatches' faded ones.
    (0.42, 0.12, 0.12, 0.55),
    (0.18, 0.3, 0.2, 0.55),
    (0.66, 0.52, 0.18, 0.55),
    (0.14, 0.18, 0.32, 0.55),
    (0.36, 0.25, 0.17, 0.6),
    (0.42, 0.43, 0.44, 0.55),
    (0.6, 0.22, 0.18, 0.55),
    (0.2, 0.36, 0.38, 0.55),
    // 16..20: vans.
    (0.86, 0.86, 0.84, 0.5),
    (0.76, 0.7, 0.58, 0.5),
    (0.55, 0.56, 0.57, 0.5),
    (0.8, 0.8, 0.76, 0.5),
    // 20..24: taxis, and 24 their amber band.
    (0.94, 0.94, 0.92, 0.3),
    (0.94, 0.94, 0.92, 0.3),
    (0.92, 0.88, 0.76, 0.3),
    (0.82, 0.84, 0.86, 0.3),
    (0.95, 0.62, 0.1, 0.35),
    // 25 trim, 26 glass, 27 tyres, 28 primer, 29..32 accents.
    (0.05, 0.05, 0.055, 0.6),
    (0.03, 0.04, 0.05, 0.06),
    (0.035, 0.035, 0.04, 0.9),
    (0.45, 0.46, 0.44, 0.8),
    (0.3, 0.55, 0.85, 0.35),
    (0.9, 0.45, 0.38, 0.35),
    (0.55, 0.75, 0.3, 0.35),
    // 32..44: tops (42 and 43 hi-vis).
    (0.9, 0.9, 0.88, 0.8),
    (0.62, 0.72, 0.86, 0.8),
    (0.2, 0.21, 0.23, 0.75),
    (0.13, 0.16, 0.28, 0.75),
    (0.06, 0.06, 0.07, 0.75),
    (0.7, 0.14, 0.13, 0.8),
    (0.78, 0.6, 0.2, 0.8),
    (0.36, 0.38, 0.22, 0.85),
    (0.88, 0.6, 0.66, 0.8),
    (0.15, 0.5, 0.52, 0.8),
    (0.98, 0.45, 0.08, 0.7),
    (0.85, 0.95, 0.15, 0.7),
    // 44..52: bottoms.
    (0.18, 0.26, 0.42, 0.85),
    (0.6, 0.54, 0.4, 0.85),
    (0.06, 0.06, 0.07, 0.8),
    (0.24, 0.25, 0.27, 0.8),
    (0.12, 0.14, 0.24, 0.8),
    (0.48, 0.48, 0.5, 0.85),
    (0.16, 0.3, 0.55, 0.85),
    (0.86, 0.82, 0.7, 0.85),
    // 52..56: shoes.
    (0.05, 0.05, 0.05, 0.5),
    (0.3, 0.18, 0.1, 0.6),
    (0.88, 0.88, 0.86, 0.6),
    (0.4, 0.4, 0.42, 0.6),
    // 56..60: skin.
    (0.96, 0.8, 0.69, 0.6),
    (0.85, 0.64, 0.5, 0.6),
    (0.62, 0.43, 0.3, 0.6),
    (0.38, 0.25, 0.17, 0.6),
    // 60..64: hair.
    (0.05, 0.04, 0.035, 0.7),
    (0.2, 0.12, 0.07, 0.7),
    (0.75, 0.6, 0.35, 0.7),
    (0.6, 0.6, 0.6, 0.7),
];

/// Where the people's paints start in [`PALETTE`] (a figure's tag counts from here).
pub const PEOPLE: u32 = 32;

/// Each tag's fields: their lowest bit (`life_lib.wgsl`'s `TAG_*`, `CAR_*` and `FIG_*`).
pub mod tag {
    /// A car's paint and second paint (palette indices, 5 bits each).
    pub const CAR_PAINT: u32 = 0;
    pub const CAR_PAINT2: u32 = 5;
    /// A car's lamps on, braking, indicator (2 bits: 0 none, 1 left, 2 right), parked, sign lit.
    pub const CAR_LAMPS: u32 = 10;
    pub const CAR_BRAKE: u32 = 11;
    pub const CAR_BLINK: u32 = 12;
    pub const CAR_PARKED: u32 = 14;
    pub const CAR_SIGN: u32 = 15;
    /// How worn it is (3 bits, 0 new to 7).
    pub const CAR_WEAR: u32 = 16;
    /// A figure's top and bottom (5 bits each, from [`super::PEOPLE`]), skin, hair and shoes (2 bits
    /// each: the palette's fours).
    pub const FIG_TOP: u32 = 0;
    pub const FIG_BOTTOM: u32 = 5;
    pub const FIG_SKIN: u32 = 10;
    pub const FIG_HAIR: u32 = 12;
    pub const FIG_SHOES: u32 = 14;
    /// Both: how much of it is there (4 bits, 15 all of it: the shader's dither), and its seed
    /// (8 bits: an indicator's beat, a panel's shade).
    pub const TAG_FADE: u32 = 19;
    pub const TAG_SEED: u32 = 23;
}

/// The palette's runs: where each starts.
pub mod paint {
    pub const RUNABOUT: u32 = 0;
    pub const HATCH: u32 = 8;
    pub const VAN: u32 = 16;
    pub const TAXI: u32 = 20;
    pub const AMBER: u32 = 24;
    pub const PRIMER: u32 = 28;
    pub const ACCENT: u32 = 29;
    pub const TOPS: u32 = 32;
    pub const BOTTOMS: u32 = 44;
    pub const SHOES: u32 = 52;
    pub const SKIN: u32 = 56;
    pub const HAIR: u32 = 60;
}

/// A fade in sixteenths, 15 for all of it.
fn fade_bits(fade: f32) -> u32 {
    (fade.clamp(0.0, 1.0) * 15.0).round() as u32
}

/// What a car has lit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lights {
    pub lamps: bool,
    pub brake: bool,
    /// The indicator lit this moment: 0 none, 1 left, 2 right.
    pub blink: u8,
    pub parked: bool,
    pub sign: bool,
}

/// The indicators' beat, Hz.
pub const BLINK_HZ: f64 = 1.5;

/// How far through the indicators' beat the clock `seconds` is, 0..1.
pub fn beat(seconds: f64) -> f32 {
    (seconds * BLINK_HZ).rem_euclid(1.0) as f32
}

/// What's lit on `c`, with the city's lamps `lamps` lit (`time::Day::lamps`), `beat` through the
/// indicators' beat ([`beat`]): its lamps as the city's come on, its brakes while it slows or
/// stands (so a queue at the red glows red), its indicator on the first half of its own beat, a
/// free taxi's sign; nothing at all parked.
pub fn lights(c: &Car, lamps: f32, beat: f32) -> Lights {
    if c.parked {
        return Lights { parked: true, ..Lights::default() };
    }
    let on = (beat + (c.seed & 255) as f32 * 0.618).fract() < 0.5;
    Lights {
        lamps: lamps > 0.15,
        brake: c.accel < -0.4 || c.speed < 0.2,
        blink: match c.blink {
            _ if !on => 0,
            Blink::None => 0,
            Blink::Left => 1,
            Blink::Right => 2,
        },
        parked: false,
        sign: c.kind == Kind::Taxi && unit(c.seed, 7) < 0.6,
    }
}

/// A vehicle's look: its body, paints (palette indices) and wear (0..7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Livery {
    pub body: Body,
    pub paint: u32,
    pub paint2: u32,
    pub wear: u8,
}

/// Strips 0, 1, 2: Charter, Canal, Gardens.
const CANAL: u8 = 1;
const GARDENS: u8 = 2;

/// How `kind` with `seed` looks on strip `strip` in district `district`: a car is a clean
/// runabout more often on Charter and in the offices, an old hatch more often on Canal and in the
/// works; worn more on Canal, by the works and the port, and in older bodies.
pub fn livery(kind: Kind, seed: u32, strip: u8, district: Option<DistrictKind>) -> Livery {
    use DistrictKind as D;
    let (works, offices) = match district {
        Some(D::Works | D::Port) => (true, false),
        Some(D::Business | D::Civic) => (false, true),
        _ => (false, false),
    };
    let body = match kind {
        Kind::Taxi => Body::Taxi,
        Kind::Van => Body::Van,
        Kind::Scooter => Body::Scooter,
        Kind::Car => {
            let share = [0.7, 0.35, 0.55][usize::from(strip % 3)]
                + if works { -0.2 } else { 0.0 }
                + if offices { 0.1 } else { 0.0 };
            if unit(seed, 11) < share { Body::Runabout } else { Body::Hatch }
        }
    };
    let pick = |n: u32, k: u32| (seed >> k) % n;
    let (paint, paint2) = match body {
        Body::Runabout => {
            let p = paint::RUNABOUT + pick(8, 0);
            (p, if pick(2, 5) == 0 { paint::ACCENT + pick(3, 3) } else { paint::RUNABOUT })
        }
        Body::Hatch => {
            let p = paint::HATCH + pick(8, 0);
            // The odd door: primer, or another car's.
            let odd = match pick(10, 3) {
                0 => paint::PRIMER,
                1 => paint::HATCH + pick(8, 7),
                _ => p,
            };
            (p, odd)
        }
        Body::Taxi => (paint::TAXI + pick(4, 0), paint::AMBER),
        Body::Van => {
            let p = paint::VAN + pick(4, 0);
            (p, p)
        }
        Body::Scooter => {
            let p = if pick(2, 0) == 0 { paint::ACCENT + pick(3, 1) } else { paint::RUNABOUT + pick(8, 1) };
            (p, p)
        }
    };
    let by_strip = match strip % 3 {
        CANAL => 4,
        GARDENS => 2,
        _ => 1,
    };
    let by_body = match body {
        Body::Runabout => -2,
        Body::Taxi | Body::Scooter => -1,
        Body::Hatch | Body::Van => 1,
    };
    let jitter = pick(3, 9) as i32 - 1;
    let wear =
        (by_strip + if works { 2 } else { 0 } - i32::from(offices) + by_body + jitter).clamp(0, 7) as u8;
    Livery { body, paint, paint2, wear }
}

/// Car `c`'s livery on strip `strip`, the same wherever it drives: its district is its home's
/// (`Car::home`), not where it is.
pub fn car_livery(c: &Car, strip: u8) -> Livery {
    let district = c.home().and_then(|b| district_of(strip, b, Stage(0))).map(|d| d.1);
    livery(c.kind, c.seed, strip, district)
}

/// A car's `MeshTag`: its livery, lights, fade (0..1) and seed.
pub fn car_tag(l: &Livery, lit: Lights, fade: f32, seed: u32) -> u32 {
    (l.paint & 31) << tag::CAR_PAINT
        | (l.paint2 & 31) << tag::CAR_PAINT2
        | u32::from(lit.lamps) << tag::CAR_LAMPS
        | u32::from(lit.brake) << tag::CAR_BRAKE
        | u32::from(lit.blink & 3) << tag::CAR_BLINK
        | u32::from(lit.parked) << tag::CAR_PARKED
        | u32::from(lit.sign) << tag::CAR_SIGN
        | u32::from(l.wear.min(7)) << tag::CAR_WEAR
        | fade_bits(fade) << tag::TAG_FADE
        | (seed & 255) << tag::TAG_SEED
}

/// How people dress in a district: 0 as they like, 1 for the office, 2 for work, 3 for leisure.
pub fn dress(strip: u8, district: Option<DistrictKind>) -> u8 {
    use DistrictKind as D;
    match district {
        Some(D::Business | D::Civic) => 1,
        Some(D::Works | D::Port) => 2,
        Some(D::Park | D::University) => 3,
        _ if strip % 3 == GARDENS => 3,
        _ => 0,
    }
}

/// What somebody wears: their cut (`life_mesh::CUTS`: 0 trousers, 1 a skirt), and their top, bottom,
/// skin, hair and shoes (palette indices).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outfit {
    pub cut: usize,
    pub top: u32,
    pub bottom: u32,
    pub skin: u32,
    pub hair: u32,
    pub shoes: u32,
}

/// Somebody's outfit, from their seed and how their district dresses ([`dress`]).
pub fn outfit(seed: u32, dress: u8) -> Outfit {
    let pick = |list: &[u32], k: u32| list[(unit(seed, k) * list.len() as f32) as usize % list.len()];
    let (skirts, tops, bottoms): (f32, &[u32], &[u32]) = match dress {
        1 => (0.3, &[0, 0, 1, 2, 3], &[46, 47, 48]),
        2 => (0.05, &[10, 11, 7, 9, 2], &[50, 45, 44]),
        3 => (0.4, &[0, 1, 5, 6, 8, 9], &[51, 44, 45]),
        _ => (0.3, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9], &[44, 45, 46, 49]),
    };
    Outfit {
        cut: usize::from(unit(seed, 21) < skirts),
        top: paint::TOPS + pick(tops, 22),
        bottom: pick(bottoms, 23),
        skin: paint::SKIN + pick(&[0, 1, 2, 3], 24),
        hair: paint::HAIR + pick(&[0, 0, 1, 1, 2, 3], 25),
        shoes: paint::SHOES + pick(&[0, 0, 1, 2, 3], 26),
    }
}

/// What somebody out in the city wears on strip `strip`, the same wherever they walk: their strip's
/// way of dressing, not their district's (the walkers' lines run on across districts, and nothing
/// says which is theirs).
pub fn person_outfit(p: &Person, strip: u8) -> Outfit {
    outfit(p.w.seed, dress(strip, None))
}

/// A figure's `MeshTag`: its outfit, fade (0..1) and seed.
pub fn figure_tag(o: &Outfit, fade: f32, seed: u32) -> u32 {
    ((o.top - PEOPLE) & 31) << tag::FIG_TOP
        | ((o.bottom - PEOPLE) & 31) << tag::FIG_BOTTOM
        | ((o.skin - paint::SKIN) & 3) << tag::FIG_SKIN
        | ((o.hair - paint::HAIR) & 3) << tag::FIG_HAIR
        | ((o.shoes - paint::SHOES) & 3) << tag::FIG_SHOES
        | fade_bits(fade) << tag::TAG_FADE
        | (seed & 255) << tag::TAG_SEED
}

/// Somebody's build: how wide (0.9 to 1, never wider than the figure, which fills the walkers'
/// radius) and how tall (0.92 to 1.08) beside the figure as made.
pub fn build(seed: u32) -> (f32, f32) {
    (0.9 + 0.1 * unit(seed, 31), 0.92 + 0.16 * unit(seed, 32))
}

// ---- Who's drawn ------------------------------------------------------------------------------

/// Something that might be drawn: who, how far (squared, m²) and where from the eye, and
/// how much it counts (its distance squared, sixteen times as much behind the view: four times as
/// far), and whether a level has taken it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub id: u64,
    /// What it is, for the caller (its index in its own list).
    pub index: usize,
    pub d2: f32,
    pub rank: f32,
    /// A ring's car (moving, or parked for the night) rather than one in a bay for good.
    pub ring: bool,
    pub taken: bool,
}

/// The view's cone, widened: anything outside it counts as four times as far.
const CONE_COS: f32 = 0.2;
/// Nothing within this of the eye is behind it (the camera turns, and a car beside you is near).
const BESIDE: f32 = 12.0;

impl Seen {
    /// `offset` from the eye, which looks along `forward` (a unit vector).
    pub fn new(id: u64, index: usize, offset: Vec3, forward: Vec3, ring: bool) -> Self {
        let d2 = offset.length_squared();
        let d = d2.sqrt();
        let behind = d > BESIDE && offset.dot(forward) < CONE_COS * d;
        Seen { id, index, d2, rank: if behind { 16.0 * d2 } else { d2 }, ring, taken: false }
    }
}

/// Puts them in the order they're chosen in: by rank, then by id (so a showcase draws the same
/// every run).
pub fn rank(seen: &mut [Seen]) {
    seen.sort_unstable_by(|a, b| a.rank.total_cmp(&b.rank).then(a.id.cmp(&b.id)));
}

/// One level's choice from `seen` (in [`rank`]'s order): those not yet taken, within `reach` and
/// passing `which`, up to `cap`, first come first served; marks them taken and gives their places
/// in `seen`.
pub fn select(
    seen: &mut [Seen],
    cap: usize,
    reach: f32,
    which: impl Fn(&Seen) -> bool,
    out: &mut Vec<usize>,
) {
    out.clear();
    let r2 = reach * reach;
    for (i, s) in seen.iter_mut().enumerate() {
        if out.len() >= cap {
            break;
        }
        if !s.taken && s.d2 <= r2 && which(s) {
            s.taken = true;
            out.push(i);
        }
    }
}

/// A pool's entities by who they show: an id chosen again keeps last frame's entity, a new one
/// takes the lowest free, so a parked car's entity isn't touched while it stays drawn.
#[derive(Debug, Default)]
pub struct Slots {
    held: Vec<Option<u64>>,
    by_id: HashMap<u64, usize>,
    wanted: Vec<bool>,
    fresh: Vec<usize>,
}

impl Slots {
    pub fn new(size: usize) -> Self {
        Slots {
            held: vec![None; size],
            by_id: HashMap::with_capacity(size * 2),
            wanted: vec![false; size],
            fresh: Vec::with_capacity(size),
        }
    }

    pub fn len(&self) -> usize {
        self.held.len()
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// Who slot `i` shows, if anyone.
    pub fn held(&self, i: usize) -> Option<u64> {
        self.held[i]
    }

    /// Gives each of `ids` (distinct; at most the pool's size are seated) a slot, in `out` by
    /// `ids`' order (`usize::MAX` for any past the pool's size); frees the rest.
    pub fn assign(&mut self, ids: &[u64], out: &mut Vec<usize>) {
        out.clear();
        self.wanted.iter_mut().for_each(|w| *w = false);
        self.fresh.clear();
        let n = ids.len().min(self.held.len());
        for (k, id) in ids[..n].iter().enumerate() {
            match self.by_id.get(id) {
                Some(&i) => {
                    self.wanted[i] = true;
                    out.push(i);
                }
                None => {
                    self.fresh.push(k);
                    out.push(usize::MAX);
                }
            }
        }
        for i in 0..self.held.len() {
            if !self.wanted[i]
                && let Some(id) = self.held[i].take()
            {
                self.by_id.remove(&id);
            }
        }
        let mut free = 0;
        for &k in &self.fresh {
            while self.held[free].is_some() {
                free += 1;
            }
            self.held[free] = Some(ids[k]);
            self.by_id.insert(ids[k], free);
            out[k] = free;
        }
        out.resize(ids.len(), usize::MAX);
    }
}

// ---- A frame's choice --------------------------------------------------------------------------

/// A level of detail's pool: how many it holds at most, and how far it reaches (m, from the eye).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    pub cap: usize,
    pub reach: f32,
}

/// What a graphics tier draws of the city's life: the people asked for (half the side of the
/// square round the eye, m), and each pool's level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LifeTier {
    pub people: f32,
    pub near_people: Level,
    pub mid_people: Level,
    pub near_cars: Level,
    /// The rings' cars past the near pool (two boxes and their lamps).
    pub mid_cars: Level,
    /// The cars parked for good past the near pool.
    pub bay_cars: Level,
}

const fn level(cap: usize, reach: f32) -> Level {
    Level { cap, reach }
}

/// The tiers' life, Low to Ultra (the pools are made at Ultra's caps).
pub const TIERS: [LifeTier; 4] = [
    LifeTier {
        people: 60.0,
        near_people: level(24, 30.0),
        mid_people: level(40, 60.0),
        near_cars: level(12, 40.0),
        mid_cars: level(24, 150.0),
        bay_cars: level(24, 50.0),
    },
    LifeTier {
        people: 90.0,
        near_people: level(48, 45.0),
        mid_people: level(96, 90.0),
        near_cars: level(24, 70.0),
        mid_cars: level(64, 250.0),
        bay_cars: level(64, 110.0),
    },
    LifeTier {
        people: 130.0,
        near_people: level(96, 60.0),
        mid_people: level(192, 130.0),
        near_cars: level(40, 110.0),
        mid_cars: level(128, 400.0),
        bay_cars: level(128, 200.0),
    },
    LifeTier {
        people: 180.0,
        near_people: level(144, 80.0),
        mid_people: level(288, 180.0),
        near_cars: level(64, 150.0),
        mid_cars: level(192, 600.0),
        bay_cars: level(192, 300.0),
    },
];

/// The pools, by index into [`Choice::picks`].
pub const PEOPLE_NEAR: usize = 0;
pub const PEOPLE_MID: usize = 1;
pub const CARS_NEAR: usize = 2;
pub const CARS_MID: usize = 3;
pub const BAYS: usize = 4;

impl LifeTier {
    /// Each pool's level, by index.
    pub fn levels(&self) -> [Level; 5] {
        [self.near_people, self.mid_people, self.near_cars, self.mid_cars, self.bay_cars]
    }
}

/// Where the eye is: its strip, `s`, `x` and height over the floor, and the way it looks in the
/// strip's frame there (`x` along, `y` up, `z` = −s; a unit vector).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eye {
    pub strip: u8,
    pub s: f32,
    pub x: f32,
    pub h: f32,
    pub forward: Vec3,
}

/// Where nothing of life is drawn, or not wholly: round the suits standing on the city (`(s, x)`),
/// under the pilots on foot (`(s, x)`: a civilian there dissolves), and the city's people the
/// showcase has named (their ids).
#[derive(Clone, Copy, Debug, Default)]
pub struct Clears<'a> {
    pub suits: &'a [(f32, f32)],
    pub pilots: &'a [(f32, f32)],
    pub borrowed: &'a [u32],
}

/// Nothing within this of a standing suit's ground point, m: its feet.
pub const SUIT_CLEAR: f32 = 9.0;
/// Nothing within this of the eye, m.
const EYE_CLEAR: f32 = 1.0;
/// A civilian dissolves under a pilot from this far to this near, m.
const PILOT_FADE: (f32, f32) = (0.8, 0.4);
/// A scooter's rider stands on its deck, this high.
pub const DECK: f32 = 0.4;

/// One of the people to draw: a walker, or a scooter's rider (who has the scooter's seed and
/// stands on its deck).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Person {
    pub id: u64,
    pub w: bc_sim::colony::walkers::Walker,
}

/// A frame's choice: the cars and people near enough to the eye, and who each pool draws (indices
/// into `cars` for the cars' pools, `people` for the people's, nearest first). Its buffers are kept
/// from frame to frame.
#[derive(Debug, Default)]
pub struct Choice {
    pub cars: Vec<Car>,
    pub people: Vec<Person>,
    pub picks: [Vec<usize>; 5],
    car_seen: Vec<Seen>,
    people_seen: Vec<Seen>,
    taken: Vec<usize>,
}

impl Choice {
    /// Works out what's drawn round `eye` at tick `t` plus `frac`: the cars by their distance from
    /// the eye, the people only while it's within the people's reach of the ground, and the near
    /// scooters' riders.
    pub fn choose(&mut self, eye: &Eye, tier: &LifeTier, t: u32, frac: f32, clears: &Clears) {
        use bc_sim::colony::city::{Rect, Stage};
        use bc_sim::colony::traffic::{each_bay_car, each_ring_car};
        use bc_sim::colony::walkers::{Pose, Walker, each_walker};
        self.cars.clear();
        self.people.clear();
        let offset = |s: f32, x: f32, h: f32| Vec3::new(x - eye.x, h - eye.h, eye.s - s);
        let clear = |s: f32, x: f32, h: f32| {
            offset(s, x, h).length_squared() >= EYE_CLEAR * EYE_CLEAR
                && clears
                    .suits
                    .iter()
                    .all(|(ss, sx)| (s - ss).powi(2) + (x - sx).powi(2) >= SUIT_CLEAR * SUIT_CLEAR)
        };
        // How far round the eye's foot a reach goes, from up where it is.
        let ground = |reach: f32| (reach * reach - eye.h * eye.h).max(0.0).sqrt();
        let around = |r: f32| Rect::new(eye.s - r, eye.s + r, eye.x - r, eye.x + r);
        let ring = ground(tier.mid_cars.reach.max(tier.near_cars.reach));
        let rings = if ring > 0.0 {
            let cars = &mut self.cars;
            each_ring_car(eye.strip, &around(ring), Stage(0), t, frac, |c| {
                if clear(c.s, c.x, 0.0) {
                    cars.push(*c);
                }
                false
            });
            cars.len()
        } else {
            0
        };
        let bay = ground(tier.bay_cars.reach.max(tier.near_cars.reach));
        if bay > 0.0 {
            let cars = &mut self.cars;
            each_bay_car(eye.strip, &around(bay), Stage(0), |c| {
                if clear(c.s, c.x, 0.0) {
                    cars.push(*c);
                }
                false
            });
        }
        if eye.h < tier.mid_people.reach {
            let people = &mut self.people;
            each_walker(eye.strip, &around(tier.people), Stage(0), t, frac, |w| {
                if !clears.borrowed.contains(&w.id) && clear(w.s, w.x, w.h) {
                    // Dissolving under a pilot passing through.
                    let near =
                        clears.pilots.iter().map(|(s, x)| (w.s - s).hypot(w.x - x)).fold(f32::MAX, f32::min);
                    let fade =
                        w.fade * ((near - PILOT_FADE.1) / (PILOT_FADE.0 - PILOT_FADE.1)).clamp(0.0, 1.0);
                    if fade > 0.0 {
                        people.push(Person { id: u64::from(w.id), w: Walker { fade, ..*w } });
                    }
                }
                false
            });
        }
        self.car_seen.clear();
        for (i, c) in self.cars.iter().enumerate() {
            self.car_seen.push(Seen::new(c.id, i, offset(c.s, c.x, 0.0), eye.forward, i < rings));
        }
        rank(&mut self.car_seen);
        let [pn, pm, cn, cm, bays] = &mut self.picks;
        let taken = &mut self.taken;
        let mut pick = |seen: &mut [Seen], l: Level, which: &dyn Fn(&Seen) -> bool, out: &mut Vec<usize>| {
            select(seen, l.cap, l.reach, which, taken);
            out.clear();
            out.extend(taken.iter().map(|i| seen[*i].index));
        };
        let seen = &mut self.car_seen;
        pick(seen, tier.near_cars, &|_| true, cn);
        pick(seen, tier.mid_cars, &|s| s.ring, cm);
        pick(seen, tier.bay_cars, &|s| !s.ring, bays);
        // The near scooters' riders, figures of their own on the decks, first among the people near
        // whatever their reach (a far scooter's mesh has its rider).
        let walkers = self.people.len();
        for c in cn.iter().map(|i| &self.cars[*i]).filter(|c| c.kind == Kind::Scooter && !c.parked) {
            let w = Walker {
                id: 0,
                s: c.s,
                x: c.x,
                h: DECK,
                yaw: c.yaw,
                speed: 0.0,
                pose: Pose::Stand,
                stride: 0.0,
                fade: 1.0,
                seed: c.seed,
            };
            self.people.push(Person { id: 1 << 62 | (c.id & ((1 << 62) - 1)), w });
        }
        let riders = self.people.len() - walkers;
        self.people_seen.clear();
        for (i, p) in self.people[..walkers].iter().enumerate() {
            self.people_seen.push(Seen::new(p.id, i, offset(p.w.s, p.w.x, p.w.h), eye.forward, true));
        }
        rank(&mut self.people_seen);
        let seen = &mut self.people_seen;
        let near = Level { cap: tier.near_people.cap.saturating_sub(riders), ..tier.near_people };
        pick(seen, near, &|_| true, pn);
        pn.splice(0..0, walkers..walkers + riders);
        pick(seen, tier.mid_people, &|_| true, pm);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::city::{Rect, Stage};
    use bc_sim::colony::traffic::each_car;

    const LIB: &str = include_str!("../../bc-client/src/shaders/life_lib.wgsl");

    fn constant(name: &str) -> u32 {
        let line = LIB
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("const {name}:")))
            .unwrap_or_else(|| panic!("no {name} in life_lib.wgsl"));
        let v = line.split('=').nth(1).unwrap().trim().trim_end_matches(';').trim().trim_end_matches('u');
        v.parse().unwrap_or_else(|_| panic!("{name} = {v}"))
    }

    /// The street's bounce on cars and people is the one on the city's walls.
    #[test]
    fn the_bounce_is_the_walls_bounce() {
        const SKY: &str = include_str!("../../bc-client/src/shaders/colony_sky.wgsl");
        let albedo =
            |src: &str| src.lines().find(|l| l.starts_with("const GROUND_ALBEDO:")).map(str::to_owned);
        assert!(albedo(LIB).is_some(), "no GROUND_ALBEDO in life_lib.wgsl");
        assert_eq!(albedo(LIB), albedo(SKY));
    }

    #[test]
    fn the_shader_keeps_these_numbers() {
        use crate::life_mesh::slot;
        for (name, v) in [
            ("CAR_PAINT", tag::CAR_PAINT),
            ("CAR_PAINT2", tag::CAR_PAINT2),
            ("CAR_LAMPS", tag::CAR_LAMPS),
            ("CAR_BRAKE", tag::CAR_BRAKE),
            ("CAR_BLINK", tag::CAR_BLINK),
            ("CAR_PARKED", tag::CAR_PARKED),
            ("CAR_SIGN", tag::CAR_SIGN),
            ("CAR_WEAR", tag::CAR_WEAR),
            ("FIG_TOP", tag::FIG_TOP),
            ("FIG_BOTTOM", tag::FIG_BOTTOM),
            ("FIG_SKIN", tag::FIG_SKIN),
            ("FIG_HAIR", tag::FIG_HAIR),
            ("FIG_SHOES", tag::FIG_SHOES),
            ("TAG_FADE", tag::TAG_FADE),
            ("TAG_SEED", tag::TAG_SEED),
            ("PEOPLE", PEOPLE),
            ("SKIN", paint::SKIN),
            ("HAIR", paint::HAIR),
            ("SHOES", paint::SHOES),
            ("TRIM", 25),
            ("GLASS", 26),
            ("TYRE", 27),
            ("SLOT_PAINT", slot::PAINT.into()),
            ("SLOT_PAINT2", slot::PAINT2.into()),
            ("SLOT_TRIM", slot::TRIM.into()),
            ("SLOT_GLASS", slot::GLASS.into()),
            ("SLOT_TYRE", slot::TYRE.into()),
            ("SLOT_HEAD", slot::HEAD.into()),
            ("SLOT_TAIL", slot::TAIL.into()),
            ("SLOT_BLINK_L", slot::BLINK_L.into()),
            ("SLOT_BLINK_R", slot::BLINK_R.into()),
            ("SLOT_DRIVER", slot::DRIVER.into()),
            ("SLOT_SIGN", slot::SIGN.into()),
            ("SLOT_TOP", slot::TOP.into()),
            ("SLOT_BOTTOM", slot::BOTTOM.into()),
            ("SLOT_SKIN", slot::SKIN.into()),
            ("SLOT_HAIR", slot::HAIR.into()),
            ("SLOT_SHOES", slot::SHOES.into()),
        ] {
            assert_eq!(constant(name), v, "{name}");
        }
        // The trim, glass and tyres' paints are where the shader looks for them.
        assert_eq!(PALETTE[25].0, 0.05);
        assert_eq!(PALETTE[26].3, 0.06);
    }

    /// Reads a field back out of a tag.
    fn field(t: u32, at: u32, bits: u32) -> u32 {
        (t >> at) & ((1 << bits) - 1)
    }

    #[test]
    fn tags_round_trip() {
        for seed in [0u32, 1, 77, 255, 1 << 20, u32::MAX] {
            for (kind, strip) in
                [(Kind::Car, 0u8), (Kind::Car, 1), (Kind::Taxi, 2), (Kind::Van, 1), (Kind::Scooter, 0)]
            {
                let l = livery(kind, seed, strip, Some(DistrictKind::Works));
                assert_eq!(l.body.kind(), kind);
                let lit = Lights {
                    lamps: true,
                    brake: seed % 2 == 0,
                    blink: (seed % 3) as u8,
                    parked: false,
                    sign: true,
                };
                for fade in [0.0f32, 0.5, 1.0] {
                    let t = car_tag(&l, lit, fade, seed);
                    assert_eq!(field(t, tag::CAR_PAINT, 5), l.paint);
                    assert_eq!(field(t, tag::CAR_PAINT2, 5), l.paint2);
                    assert_eq!(field(t, tag::CAR_LAMPS, 1), 1);
                    assert_eq!(field(t, tag::CAR_BRAKE, 1), u32::from(lit.brake));
                    assert_eq!(field(t, tag::CAR_BLINK, 2), u32::from(lit.blink));
                    assert_eq!(field(t, tag::CAR_PARKED, 1), 0);
                    assert_eq!(field(t, tag::CAR_SIGN, 1), 1);
                    assert_eq!(field(t, tag::CAR_WEAR, 3), u32::from(l.wear));
                    assert_eq!(field(t, tag::TAG_FADE, 4), (fade * 15.0).round() as u32);
                    assert_eq!(field(t, tag::TAG_SEED, 8), seed & 255);
                }
            }
            for d in 0..4 {
                let o = outfit(seed, d);
                let t = figure_tag(&o, 1.0, seed);
                assert!(
                    o.top >= paint::TOPS
                        && o.top < paint::BOTTOMS
                        && o.bottom >= paint::BOTTOMS
                        && o.bottom < paint::SHOES
                );
                assert_eq!(field(t, tag::FIG_TOP, 5) + PEOPLE, o.top);
                assert_eq!(field(t, tag::FIG_BOTTOM, 5) + PEOPLE, o.bottom);
                assert_eq!(field(t, tag::FIG_SKIN, 2) + paint::SKIN, o.skin);
                assert_eq!(field(t, tag::FIG_HAIR, 2) + paint::HAIR, o.hair);
                assert_eq!(field(t, tag::FIG_SHOES, 2) + paint::SHOES, o.shoes);
                assert_eq!(field(t, tag::TAG_FADE, 4), 15);
                assert!(o.cut < crate::life_mesh::CUTS);
            }
            let (w, h) = build(seed);
            assert!((0.9..=1.0).contains(&w) && (0.92..=1.08).contains(&h));
        }
    }

    #[test]
    fn lights_follow_the_traffic() {
        // Downtown at the evening's rush: queues at the red brake, the moving don't, parked cars
        // show nothing.
        let mid = bc_sim::colony::frame::STRIP_WIDTH * 0.5;
        let area = Rect::new(mid - 700.0, mid + 700.0, -9_000.0, -7_000.0);
        let (mut queued, mut moving, mut parked, mut blinking) = (0, 0, 0, 0);
        for t in (43_200..43_200 + 2_400).step_by(120) {
            each_car(0, &area, Stage(0), t, 0.0, |c| {
                let l = lights(c, 1.0, 0.3);
                // The indicator on half its beat, off the other half.
                if c.blink != Blink::None {
                    let lit = (0..20).filter(|k| lights(c, 1.0, *k as f32 / 20.0).blink != 0).count();
                    assert_eq!(lit, 10, "{c:?}");
                    blinking += 1;
                }
                if c.parked {
                    assert_eq!(l, Lights { parked: true, ..Lights::default() });
                    parked += 1;
                } else {
                    assert!(l.lamps);
                    if c.speed < 0.2 {
                        assert!(l.brake);
                        queued += 1;
                    } else if c.accel >= 0.0 {
                        assert!(!l.brake);
                        moving += 1;
                    }
                }
                false
            });
        }
        assert!(
            queued > 0 && moving > 0 && parked > 0 && blinking > 0,
            "{queued} queued, {moving} moving, {parked} parked, {blinking} blinking"
        );
        assert_eq!(beat(0.5 / BLINK_HZ), 0.5);
        assert!(beat(3_600.0 * 24.0 + 0.1 / BLINK_HZ) < 0.11, "a day on");
    }

    #[test]
    fn liveries_read_as_each_strip() {
        let share = |strip: u8| {
            let n = 4_000;
            let r =
                (0..n).filter(|s| livery(Kind::Car, *s * 7_919, strip, None).body == Body::Runabout).count();
            r as f32 / n as f32
        };
        let (charter, canal) = (share(0), share(1));
        assert!(charter > 0.6 && canal < 0.45, "runabouts: {charter} on Charter, {canal} on Canal");
        let worn = |strip: u8| {
            (0..1_000u32).map(|s| u32::from(livery(Kind::Car, s * 31, strip, None).wear)).sum::<u32>()
        };
        assert!(worn(1) > worn(0));
        for s in 0..2_000u32 {
            let l = livery(Kind::Car, s, (s % 3) as u8, None);
            assert!(l.paint < 32 && l.paint2 < 32 && l.wear <= 7);
        }
    }

    #[test]
    fn nobody_changes_their_look_crossing_a_district_line() {
        // Strip 1's line from the port to the homes (bx 24), the evening's rush: every car and
        // everybody drawn there keeps the look they had when first seen.
        use bc_sim::colony::city::{block_index, grid_x};
        use bc_sim::colony::frame::STRIP_WIDTH;
        let mid = STRIP_WIDTH * 0.5;
        let (mut cars, mut people, mut sides) = (HashMap::new(), HashMap::new(), [false; 2]);
        for t in (43_200..43_200 + 3_600).step_by(6) {
            for s in [30.0, mid - 600.0, mid - 200.0, mid + 200.0, mid + 600.0, STRIP_WIDTH - 30.0] {
                let eye = Eye { strip: 1, s, x: grid_x(24), h: 1.65, forward: Vec3::X };
                let mut c = Choice::default();
                c.choose(&eye, &TIERS[3], t, 0.0, &Clears::default());
                for car in &c.cars {
                    sides[usize::from(block_index(car.x) < 24)] = true;
                    let l = car_livery(car, 1);
                    assert_eq!(*cars.entry(car.id).or_insert(l), l, "{car:?}");
                }
                for p in &c.people {
                    let o = person_outfit(p, 1);
                    assert_eq!(*people.entry(p.id).or_insert(o), o, "{p:?}");
                }
            }
        }
        let (n, m) = (cars.len(), people.len());
        assert!(sides == [true; 2] && n > 500 && m > 500, "{n} cars, {m} people");
    }

    fn seen(id: u64, d: f32, behind: bool, ring: bool) -> Seen {
        let forward = Vec3::Z;
        let offset = if behind { -Vec3::Z * d } else { Vec3::Z * d };
        Seen::new(id, id as usize, offset, forward, ring)
    }

    #[test]
    fn select_takes_the_nearest_up_to_each_cap() {
        let mut list: Vec<Seen> =
            (0..50u64).map(|i| seen(i, 5.0 + (i * 37 % 50) as f32 * 4.0, i % 7 == 0, i % 2 == 0)).collect();
        // Two at the same distance: the lower id first.
        list.push(seen(1_000, 9.0, false, true));
        list.push(seen(999, 9.0, false, true));
        let run = |list: &[Seen]| {
            let mut l = list.to_vec();
            rank(&mut l);
            let (mut near, mut mid, mut bay) = (Vec::new(), Vec::new(), Vec::new());
            select(&mut l, 8, 60.0, |_| true, &mut near);
            select(&mut l, 10, 150.0, |s| s.ring, &mut mid);
            select(&mut l, 5, 120.0, |s| !s.ring, &mut bay);
            let ids = |v: &[usize]| v.iter().map(|i| l[*i].id).collect::<Vec<_>>();
            (ids(&near), ids(&mid), ids(&bay), l)
        };
        let (near, mid, bay, l) = run(&list);
        assert_eq!(near.len(), 8);
        assert!(mid.len() <= 10 && bay.len() <= 5);
        let pos = |id: u64| near.iter().position(|x| *x == id);
        assert!(pos(999).unwrap() < pos(1_000).unwrap(), "ties by id");
        // The near level's are the nearest that rank first; nobody is in two levels.
        let worst = near.iter().map(|id| l.iter().find(|s| s.id == *id).unwrap().rank).fold(0.0, f32::max);
        for s in &l {
            if !near.contains(&s.id) && s.d2 <= 3_600.0 {
                assert!(s.rank >= worst, "{s:?} left out of the near level");
            }
        }
        for id in &mid {
            assert!(!near.contains(id) && !bay.contains(id));
        }
        for id in &bay {
            assert!(!l.iter().find(|s| s.id == *id).unwrap().ring);
        }
        // Behind the view counts four times as far.
        assert_eq!(seen(1, 20.0, true, true).rank, 6_400.0);
        assert_eq!(seen(1, 20.0, false, true).rank, 400.0);
        // But not right beside the eye.
        assert_eq!(seen(1, 10.0, true, true).rank, 100.0);
        assert_eq!(run(&list).0, near, "the same every time");
        let mut shuffled = list.clone();
        shuffled.reverse();
        assert_eq!(run(&shuffled).0, near, "whatever order they're found in");
    }

    #[test]
    fn slots_are_stable_and_bounded() {
        let mut slots = Slots::new(4);
        let mut out = Vec::new();
        slots.assign(&[10, 20, 30], &mut out);
        assert_eq!(out, vec![0, 1, 2]);
        // 20 stays where it was; 10 leaves; 40 and 50 take the free ones; 60 doesn't fit.
        slots.assign(&[20, 40, 30, 50, 60], &mut out);
        assert_eq!(out, vec![1, 0, 2, 3, usize::MAX]);
        assert_eq!(slots.held(1), Some(20));
        slots.assign(&[20, 40, 30, 50], &mut out);
        assert_eq!(out, vec![1, 0, 2, 3], "the same set keeps its slots");
        slots.assign(&[], &mut out);
        assert!(out.is_empty() && (0..4).all(|i| slots.held(i).is_none()));
    }

    fn downtown() -> Eye {
        let mid = bc_sim::colony::frame::STRIP_WIDTH * 0.5;
        Eye { strip: 0, s: mid + 32.5, x: -8_000.0, h: 1.65, forward: Vec3::X }
    }

    #[test]
    fn a_frame_keeps_to_its_tier_and_its_clears() {
        let eye = downtown();
        let t = 43_200;
        let mut free = Choice::default();
        free.choose(&eye, &TIERS[3], t, 0.0, &Clears::default());
        // Borrow one walker, stand a pilot on another and a suit on the carriageway.
        let walkers: Vec<&Person> = free.people.iter().filter(|p| p.id < 1 << 32).collect();
        assert!(walkers.len() > 20, "{} people downtown", walkers.len());
        let (lent, under) = (walkers[0].w, walkers[1].w);
        let suit = (eye.s - 20.0, eye.x + 30.0);
        let (suits, pilots, borrowed) = ([suit], [(under.s, under.x)], [lent.id]);
        let clears = Clears { suits: &suits, pilots: &pilots, borrowed: &borrowed };
        for tier in &TIERS {
            let mut c = Choice::default();
            c.choose(&eye, tier, t, 0.0, &clears);
            let mut again = Choice::default();
            again.choose(&eye, tier, t, 0.0, &clears);
            assert_eq!(c.picks, again.picks, "the same every time");
            for (k, l) in tier.levels().iter().enumerate() {
                assert!(c.picks[k].len() <= l.cap, "pool {k}: {} over {}", c.picks[k].len(), l.cap);
            }
            // Every near scooter moving has its rider among the people near, and only those do (a
            // far scooter's mesh has its own).
            let near_scooters: Vec<u64> = c.picks[CARS_NEAR]
                .iter()
                .map(|i| c.cars[*i])
                .filter(|car| car.kind == Kind::Scooter && !car.parked)
                .map(|car| 1 << 62 | car.id)
                .collect();
            let riders: Vec<u64> =
                c.picks[PEOPLE_NEAR].iter().map(|i| c.people[*i].id).filter(|id| *id >= 1 << 62).collect();
            assert_eq!(riders, near_scooters);
            assert!(tier.near_cars.cap <= tier.near_people.cap, "room for every near scooter's rider");
            let mut ids = Vec::new();
            for k in [PEOPLE_NEAR, PEOPLE_MID] {
                for i in &c.picks[k] {
                    let p = c.people[*i];
                    let d = Vec3::new(p.w.x - eye.x, p.w.h - eye.h, p.w.s - eye.s).length();
                    assert!(p.id >= 1 << 62 || d <= tier.levels()[k].reach + 1e-3);
                    assert!(p.w.id != lent.id || p.id >= 1 << 32, "a borrowed walker drawn");
                    assert!((p.w.s - under.s).hypot(p.w.x - under.x) > 0.4, "a civilian in a pilot");
                    ids.push(p.id);
                }
            }
            for k in [CARS_NEAR, CARS_MID, BAYS] {
                for i in &c.picks[k] {
                    let car = c.cars[*i];
                    let d = Vec3::new(car.x - eye.x, -eye.h, car.s - eye.s).length();
                    assert!(d <= tier.levels()[k].reach + 1e-3);
                    assert!((car.s - suit.0).hypot(car.x - suit.1) >= SUIT_CLEAR, "a car in a suit's feet");
                    ids.push(car.id);
                }
            }
            let n = ids.len();
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), n, "something drawn twice");
            assert!(!c.picks[CARS_NEAR].is_empty() && !c.picks[PEOPLE_NEAR].is_empty());
        }
    }

    /// What each tier draws at the busiest places and from up the cap lift: entities, the meshes
    /// they use (each a batch, at least) and triangles. `cargo test -p bc-client-core --release
    /// how_many -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn how_many() {
        use crate::life_mesh::{Gait, bank_frame, civilian_mesh, mid_figure, vehicle_mesh};
        let mid = bc_sim::colony::frame::STRIP_WIDTH * 0.5;
        let places = [
            ("Hub Gate, noon", Eye { strip: 0, s: mid, x: -15_700.0, h: 1.65, forward: Vec3::X }, 14_400),
            (
                "Hub Gate, the evening's rush",
                Eye { strip: 0, s: mid, x: -15_700.0, h: 1.65, forward: Vec3::X },
                43_200,
            ),
            ("downtown, the evening's rush", downtown(), 43_200),
            (
                "the cap lift, night",
                Eye { strip: 0, s: mid, x: -15_990.0, h: 700.0, forward: Vec3::new(0.94, -0.34, 0.0) },
                60_000,
            ),
            (
                "Gardens' avenue, night",
                Eye { strip: 2, s: mid + 30.0, x: 2_000.0, h: 1.65, forward: Vec3::X },
                60_000,
            ),
        ];
        let tris_v: Vec<[usize; 2]> = crate::life_mesh::Body::ALL
            .iter()
            .map(|b| [vehicle_mesh(*b, true).triangles(), vehicle_mesh(*b, false).triangles()])
            .collect();
        let tris_near =
            civilian_mesh(0, Gait::Walk, 0.0).triangles().max(civilian_mesh(1, Gait::Walk, 0.0).triangles());
        let tris_mid = mid_figure(Gait::Walk, 0.0).triangles();
        for (name, eye, t) in places {
            for (k, tier) in TIERS.iter().enumerate() {
                let mut c = Choice::default();
                c.choose(&eye, tier, t, 0.0, &Clears::default());
                // A warm frame's cost, natively (wasm is two or three times this).
                let start = std::time::Instant::now();
                for k in 0..10 {
                    c.choose(&eye, tier, t + k, 0.0, &Clears::default());
                }
                let us = start.elapsed().as_micros() / 10;
                c.choose(&eye, tier, t, 0.0, &Clears::default());
                let (mut meshes, mut tris) = (std::collections::HashSet::new(), 0);
                for (pool, near) in [(PEOPLE_NEAR, true), (PEOPLE_MID, false)] {
                    for i in &c.picks[pool] {
                        let p = &c.people[*i];
                        let g = Gait::of(p.w.pose);
                        let cut = if near { person_outfit(p, eye.strip).cut } else { 9 };
                        meshes.insert((near, cut, bank_frame(g, near, p.w.stride)));
                        tris += if near { tris_near } else { tris_mid };
                    }
                }
                for (pool, near) in [(CARS_NEAR, true), (CARS_MID, false), (BAYS, false)] {
                    for i in &c.picks[pool] {
                        let car = &c.cars[*i];
                        let b = car_livery(car, eye.strip).body;
                        meshes.insert((near, 100 + b.index(), 0));
                        tris += tris_v[b.index()][usize::from(!near)];
                    }
                }
                let n: Vec<usize> = c.picks.iter().map(Vec::len).collect();
                println!(
                    "{name}, tier {k}: {} people and {} cars found; drawn {:?} (people near, mid; cars near, mid, bays): {} entities, {} meshes, {} triangles; {us} µs",
                    c.people.len(),
                    c.cars.len(),
                    n,
                    n.iter().sum::<usize>(),
                    meshes.len(),
                    tris
                );
            }
        }
    }
}
