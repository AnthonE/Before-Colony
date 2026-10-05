//! The city's traffic: its cars as a closed form of the tick, like the trams. Every client works
//! out the same cars from the strip, the place asked about and the tick; nothing about them is sent
//! or stored, and the server isn't involved. They're ghosts to pilots (a pilot's own car and body
//! pass through them), but they never pass through each other, the city's solids, the trams or the
//! people on the crossings.
//!
//! **Rings.** Every block row has its own traffic, going round it with the blocks on its right
//! (traffic keeps right, as `city_lib.wgsl`'s `KEEP_RIGHT` paints it): out along the street on its
//! outer side, back along the one on its inner side (the avenue's carriageway, for the rows beside
//! it), and round the end blocks of its run on the wide cross streets. A ring has a track for each
//! lane it runs in: track 0 in the lane by the kerb, track 1 in the next. Track 0's rings go round
//! one 512 m stretch of their row (four blocks, wide cross street to wide cross street); track 1's
//! go round the whole row, from Hub Gate's square to the building site. So the kerb lane turns right
//! at every wide street while the next lane runs on. Every street's half belongs to the row beside
//! it and every lane to one track, so no two rings share any road and no car's path ever crosses
//! another's: nothing turns left, nothing crosses the tram's median, nothing turns round.
//!
//! **Signals.** The wide cross streets (every fourth, 512 m apart) have signals at every junction,
//! on an 80 s cycle: 40 s for the cars, all of them at once (no two paths cross), then 40 s for the
//! people, every way at once. At 12.8 m/s the cars take 40 s from one signal to the next,
//! and the signals' phases alternate along the streets and from street to street, so both ways along
//! every street run in a green wave. The narrow cross streets have neither signals nor cars: people
//! cross them where they like (`walkers`). The people's half is for crossing the rest, which nobody
//! does yet.
//!
//! **Platoons.** A ring's cars run in platoons of up to [`SLOTS`], one a cycle. A platoon waits at
//! the red at the end of each cross leg, nose to tail behind the stop line, and moves off on the
//! green one car after another; then it runs its green wave down the street. Each car is its own
//! closed form: it leaves its place in the queue at its time, drives as fast as its turns and its
//! place in the next queue allow, and stops there. The same rules from a place further back and a
//! moment later: no car ever closes on the one ahead.
//!
//! **Day and night.** Track 0's platoons stay in at night, each from its own hour: drawing up at the
//! queue it parks at, it pulls in to the bays behind its places, and when its hour to go out comes,
//! its cars pull out into their places one after another while it would have stood there. Parked
//! cars also line the other streets' bays, where the paint has them, for good.
//!
//! [`each_car`] gives them all, [`each_ring_car`] the rings' (the tick moves them) and
//! [`each_bay_car`] those parked for good (it never does). Positions are city coordinates
//! (`frame::CityPos`): `s` across, `x` along.

use core::f32::consts::FRAC_PI_2;
use core::ops::{Add, Mul, Sub};

use super::city::{
    AVENUE, BLOCK, CITY, MEDIAN, ROWS, Rect, SITE, STREET, Stage, WIDE_STREET, block_index, cross_width,
    district_of, grid_x, lane_width, mix, unit,
};
use super::frame::STRIP_WIDTH;
use super::furniture::ROAD_OUT;
use super::time::{DAY_TICKS, day};
use crate::TICK_HZ;
use crate::content::city::DistrictKind;
use crate::math::{atan2, cos, floor, sin, sqrt};

const HZ: f32 = TICK_HZ as f32;
/// The signals' cycle, ticks (80 s): the cars' half (its last [`AMBER`] amber), then the people's
/// (its first [`ALL_RED`] red both ways).
pub const CYCLE: u32 = 80 * TICK_HZ;
pub const GO: u32 = CYCLE / 2;
pub const AMBER: u32 = 3 * TICK_HZ;
pub const ALL_RED: u32 = 2 * TICK_HZ;
/// Signals stand at every junction of every fourth cross street (the wide ones).
pub const SIGNAL_EVERY: i32 = 4;
/// The rows' traffic runs from the cross street at Hub Gate's square's edge to this one (on the
/// building site's first block), and sixteen blocks further for each district built out of the site.
pub const FIRST: i32 = CITY.0;
pub const LAST: i32 = 200;
/// The strips' signals run out of step, by this much, ticks.
const STRIP_PHASE: u32 = CYCLE / 3;
/// Speeds, m/s: down the streets along the strip (512 m from signal to signal in half a cycle) and
/// on the cross legs; how hard the cars speed up and brake, and the most they lean on a turn, m/s².
pub const CRUISE: f32 = 12.8;
pub const LEG_SPEED: f32 = 10.0;
pub const ACCEL: f32 = 2.0;
pub const BRAKE: f32 = 2.5;
pub const LATERAL: f32 = 2.5;
/// A lane's width (the paint's), m.
pub const LANE: f32 = 3.6;
/// A platoon's cars, at most; their places in a queue, middle to middle (m), and how long each waits
/// to move off after the one ahead (s).
pub const SLOTS: usize = 10;
pub const SPACING: f32 = 7.0;
pub const START_DELAY: f32 = 1.2;
/// The longest vehicle (a van), which the stop lines leave room for, m.
pub const LONGEST: f32 = 5.2;
/// A queue's first car stops with its nose this far short of the kerb of the street it waits to turn
/// into (behind the paint's stop line, 5.8 to 6.3 m out; at the avenue, from half a metre past its
/// pavement's edge), m.
pub const STOP_FRONT: f32 = 6.5;
/// The bays track 0's cars park in: this far towards the kerb from the lane by it (the paint's bays
/// by a wide street's kerb), and this far back from the car's place in the queue (m). Pulling out
/// takes [`PULL_TIME`], a platoon's cars [`PULL_GAP`] apart (s).
pub const BAY_OFFSET: f32 = 3.075;
pub const BAY_BACK: f32 = 9.0;
pub const PULL_TIME: f32 = 5.0;
pub const PULL_GAP: f32 = 2.0;
/// A car signals a turn this far before it, m.
const SIGNAL_AHEAD: f32 = 30.0;
/// The paint's bays: every 6 m along a kerb, none within 10 m of a crossing.
const BAY_PITCH: f32 = 6.0;
const BAY_CLEAR: f32 = 10.0;
/// The bank road: the half of row 12's outer street that's in row 12, a lane a way.
const BANK_HALF: f32 = 10.0;
/// How far a car reaches from its middle (a van's half-diagonal and more), m.
const REACH: f32 = 3.0;
/// A platoon's cars are never further than this from its first, m.
const PLATOON_REACH: f32 = 320.0;
/// Track 1's places with a car, all day.
const THROUGH: f32 = 0.7;

/// What's driven.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Car,
    Van,
    Taxi,
    Scooter,
}

impl Kind {
    /// Its length, width and height, m.
    pub const fn size(self) -> (f32, f32, f32) {
        match self {
            Kind::Car => (4.4, 1.8, 1.5),
            Kind::Van => (LONGEST, 2.0, 2.1),
            Kind::Taxi => (4.6, 1.8, 1.55),
            Kind::Scooter => (1.9, 0.7, 1.2),
        }
    }

    fn draw(u: f32) -> Kind {
        if u < 0.08 {
            Kind::Scooter
        } else if u < 0.22 {
            Kind::Van
        } else if u < 0.32 {
            Kind::Taxi
        } else {
            Kind::Car
        }
    }
}

/// Which indicator is flashing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blink {
    None,
    Right,
    Left,
}

/// A car, this moment. It stands on the road at `h = 0`: the streets and the canal's bridges are
/// flush with the floor (`city::ground` is 0 off the blocks).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Car {
    /// Who it is, for ever: its ring, platoon and place, or its bay.
    pub id: u64,
    pub kind: Kind,
    /// Its livery's draw.
    pub seed: u32,
    /// Its middle, `(s, x)`, on the ground.
    pub s: f32,
    pub x: f32,
    /// The way it faces, a unit `(ds, dx)`, and as the walker's yaw (0 faces −s, τ/4 faces +x).
    pub dir: (f32, f32),
    pub yaw: f32,
    /// How fast it's going (m/s), and its acceleration along `dir` (m/s²: its brake lights below 0).
    pub speed: f32,
    pub accel: f32,
    pub blink: Blink,
    /// Parked: its lights are off.
    pub parked: bool,
}

impl Car {
    /// Its footprint's corners, `(s, x)`, round from its front right.
    pub fn corners(&self) -> [(f32, f32); 4] {
        let (l, w, _) = self.kind.size();
        let (f, r) = (V::new(self.dir.0, self.dir.1), V::new(-self.dir.1, self.dir.0));
        let (c, hl, hw) = (V::new(self.s, self.x), 0.5 * l, 0.5 * w);
        [c + f * hl + r * hw, c - f * hl + r * hw, c - f * hl - r * hw, c + f * hl - r * hw]
            .map(|p| (p.s, p.x))
    }

    /// The rectangle round its footprint.
    pub fn bounds(&self) -> Rect {
        let c = self.corners();
        c[1..].iter().fold(Rect::new(c[0].0, c[0].0, c[0].1, c[0].1), |r, &(s, x)| {
            Rect::new(r.s0.min(s), r.s1.max(s), r.x0.min(x), r.x1.max(x))
        })
    }

    /// The block it belongs to, the same wherever it drives (for its look): its bay's, or the first
    /// of the stretch its ring goes round; none for a ring the length of its row (through traffic).
    pub fn home(&self) -> Option<i32> {
        if self.id >> 63 == 1 {
            Some(block_index(self.x))
        } else if (self.id >> (RING_AT + 5)) & 1 == 0 {
            Some(FIRST + SIGNAL_EVERY * ((self.id >> CELL_AT) & 0xFF) as i32)
        } else {
            None
        }
    }
}

/// Where a ring's car's id has its ring (its strip, side, track and row, from the top) and its
/// stretch's cell, above its platoon's slot.
const RING_AT: u32 = 20;
const CELL_AT: u32 = 12;

/// The cars' light at a junction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Light {
    Green,
    Amber,
    Red,
}

/// A junction's signals, this moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Signal {
    /// The cars' light, every way at once.
    pub light: Light,
    /// The people's, every way at once: while the cars' is red, but for its first moments.
    pub walk: bool,
    /// Seconds till the cars' light next changes.
    pub left: f32,
}

/// The last cross street the rows' traffic reaches, with the site built out to `stage`.
pub fn last_street(stage: Stage) -> i32 {
    (LAST + 16 * i32::from(stage.0)).min(SITE.1 - 1)
}

/// Whether the junctions on cross street `bx` have signals.
pub fn signalised(bx: i32, stage: Stage) -> bool {
    bx % SIGNAL_EVERY == 0 && (FIRST..=last_street(stage)).contains(&bx)
}

/// When the cars' half of the cycle starts at the junction of signalised cross street `bx` with
/// along-street `k` (0 the avenue, ±1 to ±12 the streets along the rows' outer edges either side),
/// ticks into the cycle: alternating from junction to junction both ways.
pub fn phase(strip: u8, bx: i32, k: i32) -> u32 {
    (STRIP_PHASE * u32::from(strip % 3) + GO * (((bx / SIGNAL_EVERY) + k.abs()) & 1) as u32) % CYCLE
}

/// The signals at the junction of cross street `bx` with along-street `k`, at tick `t` plus `frac`
/// of the next, if it has any.
pub fn signal(strip: u8, bx: i32, k: i32, stage: Stage, t: u32, frac: f32) -> Option<Signal> {
    if !signalised(bx, stage) {
        return None;
    }
    let into = ((t % CYCLE + CYCLE - phase(strip, bx, k)) % CYCLE) as f32 + frac.clamp(0.0, 1.0);
    let (green, go, cycle) = ((GO - AMBER) as f32, GO as f32, CYCLE as f32);
    Some(if into < green {
        Signal { light: Light::Green, walk: false, left: (green - into) / HZ }
    } else if into < go {
        Signal { light: Light::Amber, walk: false, left: (go - into) / HZ }
    } else {
        Signal { light: Light::Red, walk: into >= go + ALL_RED as f32, left: (cycle - into) / HZ }
    })
}

/// A point or a way on a strip, `(s, x)`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct V {
    s: f32,
    x: f32,
}

impl V {
    const fn new(s: f32, x: f32) -> Self {
        Self { s, x }
    }

    /// A way turned to the right (traffic's right: facing +x, it's −s).
    fn right(self) -> V {
        V::new(-self.x, self.s)
    }

    fn dot(self, o: V) -> f32 {
        self.s * o.s + self.x * o.x
    }
}

impl Add for V {
    type Output = V;
    fn add(self, o: V) -> V {
        V::new(self.s + o.s, self.x + o.x)
    }
}

impl Sub for V {
    type Output = V;
    fn sub(self, o: V) -> V {
        V::new(self.s - o.s, self.x - o.x)
    }
}

impl Mul<f32> for V {
    type Output = V;
    fn mul(self, k: f32) -> V {
        V::new(self.s * k, self.x * k)
    }
}

fn smooth(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

fn smooth_slope(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    6.0 * u * (1.0 - u)
}

/// The outer edge of row `k` from the strip's middle.
fn row_edge(k: i32) -> f32 {
    AVENUE * 0.5 + BLOCK * k as f32
}

/// How far from the kerb the lane by it runs on a two-way street `w` wide (`city_lib.wgsl`'s
/// `paint_road`: 3.6 m lanes out from the middle's markings, two a way, three on the wide streets).
fn kerb_lane(w: f32) -> f32 {
    if w > STREET { w * 0.5 - 1.6 - 1.8 - 2.0 * LANE } else { w * 0.5 - 0.3 - 1.8 - LANE }
}

/// How far out from a two-way street `w` wide's middle line the paint's bays' middles are (0.15 m
/// past the lanes' edge line, 2.25 m deep).
fn bay_line(w: f32) -> f32 {
    let edge = if w > STREET {
        1.6 + 3.0 * LANE
    } else if w < STREET {
        LANE
    } else {
        0.3 + 2.0 * LANE
    };
    edge + 0.15 + 1.125
}

/// Along-street `k` (0 the avenue's carriageway, 1 to 12 the streets along the rows' outer edges, 12
/// the bank road) on row `row`'s side of it (`row` is `k` or `k + 1`): its kerb there (m out from the
/// strip's middle), and how far out from that kerb track `track`'s lane runs.
fn along(k: i32, row: i32, track: usize) -> (f32, f32) {
    let off = LANE * track as f32;
    if k == 0 {
        // The carriageway's lanes count in from its pavement: 17.9 and 14.3 m from the middle line.
        return (ROAD_OUT, ROAD_OUT - (MEDIAN * 0.5 + 0.9 + 1.8 + 2.0 * LANE) + off);
    }
    let w = lane_width(k);
    if k == ROWS {
        // The bank road: its lane 1.8 m in from its middle line.
        return (row_edge(k) - w * 0.5, BANK_HALF - 1.8 + off);
    }
    let kerb = if row == k { row_edge(k) - w * 0.5 } else { row_edge(k) + w * 0.5 };
    (kerb, kerb_lane(w) + off)
}

/// The tracks row `row`'s rings have: one by the bank road, which has a lane a way.
fn tracks(row: i32) -> usize {
    if row == ROWS { 1 } else { 2 }
}

/// How busy a district's streets are: the share of track 0's places with a car, and of its streets'
/// bays with one parked.
fn busy(d: Option<(u8, DistrictKind)>) -> (f32, f32) {
    use DistrictKind::*;
    match d.map(|(_, k)| k) {
        Some(Business) => (0.95, 0.25),
        Some(Midtown) => (0.9, 0.45),
        Some(Civic) => (0.8, 0.3),
        Some(Port) => (0.75, 0.4),
        Some(Works) => (0.7, 0.4),
        Some(Residential) => (0.6, 0.55),
        Some(University) => (0.6, 0.45),
        Some(OldTown) => (0.5, 0.6),
        Some(Park) => (0.35, 0.2),
        None => (0.2, 0.1),
    }
}

/// The share of the day's traffic out on the streets through the day (0 at the start of dawn): little
/// at night, all of it at the morning's and the evening's rush, and still most as the lamps come on.
fn out_share(phase: f32) -> f32 {
    const AT: [(f32, f32); 10] = [
        (0.0, 0.3),
        (0.06, 0.7),
        (0.14, 1.0),
        (0.3, 0.85),
        (0.55, 0.8),
        (0.72, 1.0),
        (0.83, 0.85),
        (0.9, 0.35),
        (0.97, 0.25),
        (1.0, 0.3),
    ];
    let p = phase.clamp(0.0, 1.0);
    let mut i = 1;
    while i < AT.len() - 1 && AT[i].0 < p {
        i += 1;
    }
    let ((p0, v0), (p1, v1)) = (AT[i - 1], AT[i]);
    v0 + (v1 - v0) * ((p - p0) / (p1 - p0)).clamp(0.0, 1.0)
}

/// A turn to the right round a block's corner.
#[derive(Clone, Copy, Debug, Default)]
struct Turn {
    /// Its middle and its radius.
    c: V,
    r: f32,
    /// The way in (the way out is to its right).
    e: V,
    len: f32,
}

impl Turn {
    /// The turn round corner `k`, coming in along `e` in a lane `a` out from the kerb beside it and
    /// leaving in one `b` out from the kerb of the street it turns into, on the radius `max(a, b)`
    /// (so that neither the car nor its sweep ever cuts the corner).
    fn new(k: V, e: V, a: f32, b: f32) -> Self {
        let r = a.max(b);
        Self { c: k + e * (b - r) + e.right() * (r - a), r, e, len: r * FRAC_PI_2 }
    }

    fn start(&self) -> V {
        self.c - self.e.right() * self.r
    }

    fn end(&self) -> V {
        self.c + self.e * self.r
    }

    /// `u` m round it: where, and which way.
    fn at(&self, u: f32) -> (V, V) {
        let th = u / self.r;
        let (sn, cs) = (sin(th), cos(th));
        (self.c + (self.e * sn - self.e.right() * cs) * self.r, self.e * cs + self.e.right() * sn)
    }

    /// The speed to take it at.
    fn speed(&self) -> f32 {
        sqrt(LATERAL * self.r).min(LEG_SPEED)
    }
}

/// A drive from a stop to a stop through five stretches (the rest of a cross leg, a turn, a street
/// along the strip, a turn, a cross leg), each with its own top speed: as fast as the limits, the
/// car's acceleration and its braking allow. `v` is its speed at the stretches' ends, `go` how each
/// goes.
#[derive(Clone, Copy, Debug)]
struct Drive {
    z: [f32; 5],
    v: [f32; 6],
    go: [Stretch; 5],
}

/// How a stretch of a drive goes: the top speed it reaches, and how long it speeds up to it, holds it
/// and brakes from it, s.
#[derive(Clone, Copy, Debug, Default)]
struct Stretch {
    top: f32,
    up: f32,
    hold: f32,
    down: f32,
}

/// Where a car is on its drive: how far along, how fast, its acceleration, and which stretch.
#[derive(Clone, Copy, Debug)]
struct Along {
    r: f32,
    v: f32,
    a: f32,
    zone: usize,
}

impl Drive {
    fn new(z: [f32; 5], lim: [f32; 5]) -> Self {
        let mut v = [0.0f32; 6];
        for i in 1..5 {
            v[i] = lim[i - 1].min(lim[i]).min(sqrt(v[i - 1] * v[i - 1] + 2.0 * ACCEL * z[i - 1].max(0.0)));
        }
        for i in (0..5).rev() {
            v[i] = v[i].min(sqrt(v[i + 1] * v[i + 1] + 2.0 * BRAKE * z[i].max(0.0)));
        }
        let mut go = [Stretch::default(); 5];
        for (i, g) in go.iter_mut().enumerate() {
            let (v0, v1, z) = (v[i], v[i + 1], z[i].max(0.0));
            let peak = sqrt((2.0 * ACCEL * BRAKE * z + BRAKE * v0 * v0 + ACCEL * v1 * v1) / (ACCEL + BRAKE));
            let top = lim[i].min(peak).max(v0.max(v1));
            let flat = z - (top * top - v0 * v0) / (2.0 * ACCEL) - (top * top - v1 * v1) / (2.0 * BRAKE);
            let hold = if top > 1e-4 { flat.max(0.0) / top } else { 0.0 };
            *g = Stretch { top, up: (top - v0) / ACCEL, hold, down: (top - v1) / BRAKE };
        }
        Self { z, v, go }
    }

    /// Its length, m, and how long it takes, s.
    fn length(&self) -> f32 {
        self.z.iter().map(|z| z.max(0.0)).sum()
    }

    fn time(&self) -> f32 {
        self.go.iter().map(|s| s.up + s.hold + s.down).sum()
    }

    /// Where it is `t` s after setting off.
    fn at(&self, t: f32) -> Along {
        let (mut t, mut r) = (t.max(0.0), 0.0);
        for (i, s) in self.go.iter().enumerate() {
            let v0 = self.v[i];
            let ramp = v0 * s.up + 0.5 * ACCEL * s.up * s.up;
            if t < s.up {
                return Along { r: r + v0 * t + 0.5 * ACCEL * t * t, v: v0 + ACCEL * t, a: ACCEL, zone: i };
            }
            if t < s.up + s.hold {
                return Along { r: r + ramp + s.top * (t - s.up), v: s.top, a: 0.0, zone: i };
            }
            if t < s.up + s.hold + s.down {
                let u = t - s.up - s.hold;
                let r = r + ramp + s.top * s.hold + s.top * u - 0.5 * BRAKE * u * u;
                return Along { r, v: s.top - BRAKE * u, a: -BRAKE, zone: i };
            }
            t -= s.up + s.hold + s.down;
            r += self.z[i].max(0.0);
        }
        Along { r, v: 0.0, a: 0.0, zone: 4 }
    }
}

/// A ring: a track's cars round a run of a row's blocks, between wide cross streets. Round it from
/// the corner where its up leg (from the inner street out to the outer) turns onto the outer street:
/// that turn, the outer street, the turn onto the down leg, the down leg, the turn onto the inner
/// street, the inner street, the turn onto the up leg, the up leg.
#[derive(Clone, Copy, Debug)]
struct Ring {
    strip: u8,
    side: f32,
    row: i32,
    track: usize,
    cell: u32,
    turns: [Turn; 4],
    /// Where each piece starts round the ring (`at[8]` is its length).
    at: [f32; 9],
    /// The first car's place in the queue on the down leg and on the up leg.
    stop: [f32; 2],
    /// Its lap (ticks), its platoons, and when platoon 0 leaves the up leg's queue (ticks into the
    /// cycle).
    lap: u32,
    platoons: u32,
    t0: u32,
    density: f32,
    seed: u32,
    /// Its platoons stay in when the hour has no use for them.
    hourly: bool,
}

/// What a car is doing.
#[derive(Clone, Copy, Debug)]
enum Doing {
    /// Driving half `h` of its lap, `t` s since it moved off; `park` if it pulls in to its bay at the
    /// end.
    Drive { h: usize, t: f32, park: bool },
    /// Parked in its bay at the end of half `h`.
    Parked { h: usize },
    /// Pulling out of its bay at the end of half `h` into its place in the queue: `u` of the way.
    Pull { h: usize, u: f32 },
}

impl Ring {
    /// Track `track`'s ring of row `row` on side `side` of the avenue (+1 the +s side), round the
    /// blocks between cross streets `b0` and `b1`.
    fn new(strip: u8, side: f32, row: i32, track: usize, b0: i32, b1: i32, stage: Stage) -> Self {
        let mid = STRIP_WIDTH * 0.5;
        // The up leg runs from the inner street out to the outer at its −x end on the +s side, at
        // its +x end on the −s side (traffic keeping right, the blocks on its right).
        let (up, dn) = if side > 0.0 { (b0, b1) } else { (b1, b0) };
        let kx = |b: i32| {
            if b == b0 { grid_x(b) + cross_width(b) * 0.5 } else { grid_x(b) - cross_width(b) * 0.5 }
        };
        let cross = |b: i32| kerb_lane(cross_width(b)) + LANE * track as f32;
        let (ko, lo) = along(row, row, track);
        let (ki, li) = along(row - 1, row, track);
        let (out, fwd) = (V::new(side, 0.0), V::new(0.0, side));
        let (so, si) = (mid + side * ko, mid + side * ki);
        let turns = [
            Turn::new(V::new(so, kx(up)), out, cross(up), lo),
            Turn::new(V::new(so, kx(dn)), fwd, lo, cross(dn)),
            Turn::new(V::new(si, kx(dn)), out * -1.0, cross(dn), li),
            Turn::new(V::new(si, kx(up)), fwd * -1.0, li, cross(up)),
        ];
        let mut at = [0.0f32; 9];
        for i in 0..4 {
            let (t, next) = (turns[i], turns[(i + 1) % 4]);
            at[2 * i + 1] = at[2 * i] + t.len;
            at[2 * i + 2] = at[2 * i + 1] + (next.start() - t.end()).dot(t.e.right());
        }
        // A queue's first car: its nose behind the stop line, so its middle this far before the turn
        // it waits for.
        let wait = |front: f32, b: f32, t: &Turn| front + 0.5 * LONGEST + (b - t.r);
        let inner = STOP_FRONT + if row == 1 { 0.5 } else { 0.0 };
        let stop = [at[4] - wait(inner, li, &turns[2]), at[8] - wait(STOP_FRONT, lo, &turns[0])];
        // A half lap from queue to queue: its cells' green waves and one more cycle's half, so that
        // it moves off from each as the light there turns green.
        let cells = ((b1 - b0) / SIGNAL_EVERY) as u32;
        let cell = ((b0 - FIRST) / SIGNAL_EVERY) as u32;
        let key =
            (u32::from(strip) << 12) | (u32::from(side > 0.0) << 11) | ((row as u32) << 4) | track as u32;
        Self {
            strip,
            side,
            row,
            track,
            cell,
            turns,
            at,
            stop,
            lap: CYCLE * (cells + 1),
            platoons: cells + 1,
            t0: phase(strip, up, row),
            density: if track == 0 { busy(district_of(strip, b0, stage)).0 } else { THROUGH },
            seed: mix(0x7AFF, key, cell),
            hourly: track == 0 && cells == 1,
        }
    }

    /// Where on the ring and which way, `q` m round it.
    fn pose(&self, q: f32) -> (V, V) {
        let len = self.at[8];
        let q = q - len * floor(q / len);
        let mut i = 0;
        while i < 7 && q >= self.at[i + 1] {
            i += 1;
        }
        let (u, t) = (q - self.at[i], &self.turns[i / 2]);
        if i % 2 == 0 {
            t.at(u.min(t.len))
        } else {
            let e = t.e.right();
            (t.end() + e * u, e)
        }
    }

    /// Half `h`'s drive for the car in place `n`: from its place in the queue it starts in to its
    /// place in the next (or, `park`ing, its bay there); and where it starts round the ring.
    fn drive(&self, h: usize, n: usize, park: bool) -> (Drive, f32) {
        let back = SPACING * n as f32;
        let bay = if park { BAY_BACK } else { 0.0 };
        let (from, to, p) =
            if h == 0 { (self.stop[1], self.stop[0], 0) } else { (self.stop[0], self.stop[1], 4) };
        let (a, b) = (&self.turns[p / 2], &self.turns[p / 2 + 1]);
        let start = if h == 0 { self.at[8] } else { self.at[4] };
        let z = [
            start - from + back,
            a.len,
            self.at[p + 2] - self.at[p + 1],
            b.len,
            to - self.at[p + 3] - back - bay,
        ];
        (Drive::new(z, [LEG_SPEED, a.speed(), CRUISE, b.speed(), LEG_SPEED]), from - back)
    }

    /// Whether platoon `p`'s cars are out on the streets after its `k`th draw-up at the queue it parks
    /// at, from the hour then: each platoon of a ring that parks has its own share of the day's traffic
    /// below which it stays in.
    fn out(&self, p: u32, k: i64) -> bool {
        if !self.hourly {
            return true;
        }
        let at = i64::from(self.anchor(p)) + k * i64::from(self.lap) + i64::from(self.lap / 2);
        let phase = day(at.rem_euclid(i64::from(DAY_TICKS)) as u32, 0.0).phase;
        out_share(phase) >= 0.25 + 0.7 * unit(self.seed, 900 + p)
    }

    /// When platoon `p` sets off on the half of its lap that ends at the queue it parks at, ticks into
    /// its lap: the first platoon parks at the down leg's queue, the second at the up leg's.
    fn anchor(&self, p: u32) -> u32 {
        let start = (self.t0 + CYCLE * p) % self.lap;
        start + if self.hourly && p % 2 == 1 { self.lap / 2 } else { 0 }
    }

    /// What platoon `p`'s car in place `n` is doing at tick `t` plus `frac` (if that place has one).
    fn doing(&self, p: u32, n: usize, t: u32, frac: f32) -> Doing {
        let lap = i64::from(self.lap);
        let rel = i64::from(t) - i64::from(self.anchor(p));
        let (mut k, mut into) = (rel.div_euclid(lap), rel.rem_euclid(lap) as f32 + frac.clamp(0.0, 1.0));
        into -= START_DELAY * HZ * n as f32;
        if into < 0.0 {
            into += lap as f32;
            k -= 1;
        }
        // Its `k`th lap, from setting off on the half that ends at the queue it parks at (half 0, for a
        // ring that doesn't).
        let (half, first) = ((self.lap / 2) as f32, if self.hourly { (p % 2) as usize } else { 0 });
        if into >= half {
            return if self.out(p, k) {
                Doing::Drive { h: 1 - first, t: (into - half) / HZ, park: false }
            } else {
                Doing::Parked { h: first }
            };
        }
        let t = into / HZ;
        match (self.out(p, k - 1), self.out(p, k)) {
            (true, now) => Doing::Drive { h: first, t, park: !now },
            (false, false) => Doing::Parked { h: first },
            (false, true) => {
                // Out of its bay one after another from the first, once the platoon would have drawn
                // up, in time for its green.
                let pull = self.drive(first, 0, false).0.time() + 1.0 + (PULL_GAP - START_DELAY) * n as f32;
                if t < pull {
                    Doing::Parked { h: first }
                } else {
                    Doing::Pull { h: first, u: ((t - pull) / PULL_TIME).min(1.0) }
                }
            }
        }
    }

    /// Whether platoon `p`'s place `n` has a car.
    fn manned(&self, p: u32, n: usize) -> bool {
        unit(self.seed, p * 16 + n as u32) < self.density
    }

    /// Where platoon `p`'s place `n` is, which way it faces, and how it's going: its speed, its
    /// acceleration, its indicator, and whether it's parked.
    fn place(&self, p: u32, n: usize, t: u32, frac: f32) -> (V, V, f32, f32, Blink, bool) {
        // Round the ring, out to its right (a cross leg's kerb), its slope off the ring's way, its speed
        // and acceleration, its indicator, and parked.
        let (q, y, slope, speed, accel, blink, parked) = match self.doing(p, n, t, frac) {
            Doing::Drive { h, t, park } => {
                let (d, from) = self.drive(h, n, park);
                let (at, end) = (d.at(t), d.length());
                let (z1, z3) = (d.z[0], d.z[0] + d.z[1] + d.z[2]);
                // Its indicator for the turn ahead: each turn, and the queue (every queue waits to turn
                // right).
                let turning = (at.zone <= 1 && at.r > z1 - SIGNAL_AHEAD)
                    || ((2..=3).contains(&at.zone) && at.r > z3 - SIGNAL_AHEAD)
                    || at.r > end - SIGNAL_AHEAD;
                if park && at.r > end - BAY_BACK - SIGNAL_AHEAD {
                    // Pulling in: over the last of the leg to its bay.
                    let u = (at.r - (end - BAY_BACK)) / BAY_BACK;
                    let slope = BAY_OFFSET * smooth_slope(u) / BAY_BACK;
                    (from + at.r, BAY_OFFSET * smooth(u), slope, at.v, at.a, Blink::Right, false)
                } else {
                    let blink = if turning { Blink::Right } else { Blink::None };
                    (from + at.r, 0.0, 0.0, at.v, at.a, blink, false)
                }
            }
            Doing::Parked { h } => {
                let (d, from) = self.drive(h, n, true);
                (from + d.length(), BAY_OFFSET, 0.0, 0.0, 0.0, Blink::None, true)
            }
            Doing::Pull { h, u } => {
                let (d, from) = self.drive(h, n, true);
                let w = smooth(u);
                let slope = -BAY_OFFSET * smooth_slope(w) / BAY_BACK;
                let blink = if u < 1.0 { Blink::Left } else { Blink::None };
                let v = BAY_BACK * smooth_slope(u) / PULL_TIME;
                (
                    from + d.length() + BAY_BACK * w,
                    BAY_OFFSET * (1.0 - smooth(w)),
                    slope,
                    v,
                    0.0,
                    blink,
                    false,
                )
            }
        };
        let (at, e) = self.pose(q);
        let dir = (e + e.right() * slope) * (1.0 / sqrt(1.0 + slope * slope));
        (at + e.right() * y, dir, speed, accel, blink, parked)
    }

    /// Platoon `p`'s car in place `n` at tick `t` plus `frac`, if that place has one.
    fn car(&self, p: u32, n: usize, t: u32, frac: f32) -> Option<Car> {
        if !self.manned(p, n) {
            return None;
        }
        let (at, dir, speed, accel, blink, parked) = self.place(p, n, t, frac);
        let slot = p * 16 + n as u32;
        let ring = (u64::from(self.strip) << 7)
            | (u64::from(self.side > 0.0) << 6)
            | ((self.track as u64) << 5)
            | self.row as u64;
        Some(Car {
            id: (ring << RING_AT) | (u64::from(self.cell) << CELL_AT) | u64::from(slot),
            kind: Kind::draw(unit(self.seed, 1_000 + slot)),
            seed: mix(self.seed, slot, 0xCA7),
            s: at.s,
            x: at.x,
            dir: (dir.s, dir.x),
            yaw: atan2(dir.x, -dir.s),
            speed,
            accel,
            blink,
            parked,
        })
    }

    /// Its cars that may touch `area`, to `give` (stopping when it says so).
    fn each(&self, area: &Rect, t: u32, frac: f32, give: &mut impl FnMut(Car) -> bool) -> bool {
        // A ring that doesn't park has nothing parked, and a platoon is all within a few hundred
        // metres of its first car: only those near the area are asked about.
        let first = [self.drive(0, 0, false).0, self.drive(1, 0, false).0];
        for p in 0..self.platoons {
            if !self.hourly
                && let Doing::Drive { h, t, .. } = self.doing(p, 0, t, frac)
            {
                let (at, _) = self.pose(self.stop[1 - h] + first[h].at(t).r);
                let ds = (area.s0 - at.s).max(at.s - area.s1).max(0.0);
                let dx = (area.x0 - at.x).max(at.x - area.x1).max(0.0);
                if ds * ds + dx * dx > PLATOON_REACH * PLATOON_REACH {
                    continue;
                }
            }
            for n in 0..SLOTS {
                if let Some(c) = self.car(p, n, t, frac)
                    && give(c)
                {
                    return true;
                }
            }
        }
        false
    }
}

/// The cars near a footprint at tick `t` plus `frac`, for anything that wants them one at a time:
/// the rings' (moving, and track 0's parked for the night) and those parked for good in the streets'
/// bays. Calls `f` with each whose footprint overlaps `area`; stops early when it returns true, and
/// says whether it did. Nothing is stored: each car is worked out from its ring, or its bay, and the
/// tick. They're [`each_ring_car`]'s and [`each_bay_car`]'s: on each side of the avenue in turn, the
/// rings' and then the bays'.
pub fn each_car(
    strip: u8,
    area: &Rect,
    stage: Stage,
    t: u32,
    frac: f32,
    mut f: impl FnMut(&Car) -> bool,
) -> bool {
    let mut give = |c: Car| c.bounds().overlaps(area) && f(&c);
    in_reach(area, stage)
        && SIDES.into_iter().any(|side| {
            rings(strip, side, area, stage, t, frac, &mut give) || bays(strip, side, area, stage, &mut give)
        })
}

/// [`each_car`]'s cars that the tick moves: the rings', driving, queued at the signals and (track
/// 0's) parked for the night.
pub fn each_ring_car(
    strip: u8,
    area: &Rect,
    stage: Stage,
    t: u32,
    frac: f32,
    mut f: impl FnMut(&Car) -> bool,
) -> bool {
    let mut give = |c: Car| c.bounds().overlaps(area) && f(&c);
    in_reach(area, stage) && SIDES.into_iter().any(|side| rings(strip, side, area, stage, t, frac, &mut give))
}

/// [`each_car`]'s cars parked for good in the paint's bays: the same at every tick, they never move.
pub fn each_bay_car(strip: u8, area: &Rect, stage: Stage, mut f: impl FnMut(&Car) -> bool) -> bool {
    let mut give = |c: Car| c.bounds().overlaps(area) && f(&c);
    in_reach(area, stage) && SIDES.into_iter().any(|side| bays(strip, side, area, stage, &mut give))
}

/// The avenue's sides, −s and +s, in the order they're asked about.
const SIDES: [f32; 2] = [-1.0, 1.0];

/// Whether `area` is anywhere near the rows' traffic: from the cross street at Hub Gate's square's
/// edge to the last one built out to `stage`.
fn in_reach(area: &Rect, stage: Stage) -> bool {
    area.x1 >= grid_x(FIRST) - WIDE_STREET && area.x0 <= grid_x(last_street(stage)) + WIDE_STREET
}

/// How far out from the avenue's middle line `area` reaches on side `side`: from `a0` to `a1` (`a1`
/// below 0 when it's all on the other side).
fn out_from_middle(area: &Rect, side: f32) -> (f32, f32) {
    let mid = STRIP_WIDTH * 0.5;
    if side > 0.0 { (area.s0 - mid, area.s1 - mid) } else { (mid - area.s1, mid - area.s0) }
}

/// The rings' cars on side `side` of the avenue that may touch `area`, to `give`: each row's track 0
/// round the stretches near it, and its track 1 round the whole row.
fn rings(
    strip: u8,
    side: f32,
    area: &Rect,
    stage: Stage,
    t: u32,
    frac: f32,
    give: &mut impl FnMut(Car) -> bool,
) -> bool {
    let (a0, a1) = out_from_middle(area, side);
    if a1 < 0.0 {
        return false;
    }
    let (last, cell) = (last_street(stage), BLOCK * SIGNAL_EVERY as f32);
    let cells = (last - FIRST) / SIGNAL_EVERY;
    let near = |x: f32| floor((x - grid_x(FIRST)) / cell) as i32;
    let (c0, c1) = (near(area.x0 - WIDE_STREET).max(0), near(area.x1 + WIDE_STREET).min(cells - 1));
    for row in 1..=ROWS {
        // A ring keeps to its row and the halves of its streets beside it.
        let lo = if row == 1 { MEDIAN * 0.5 } else { row_edge(row - 1) };
        let hi = if row == ROWS { row_edge(ROWS) - BANK_HALF } else { row_edge(row) };
        if a1 < lo - REACH || a0 > hi + REACH {
            continue;
        }
        for c in c0..=c1 {
            let b0 = FIRST + SIGNAL_EVERY * c;
            if Ring::new(strip, side, row, 0, b0, b0 + SIGNAL_EVERY, stage).each(area, t, frac, give) {
                return true;
            }
        }
        if tracks(row) > 1 && Ring::new(strip, side, row, 1, FIRST, last, stage).each(area, t, frac, give) {
            return true;
        }
    }
    false
}

/// The car parked for good in a bay, if it has one: a closed form of the bay's `key`.
fn parked(key: u64, s: f32, x: f32, dir: V, d: Option<(u8, DistrictKind)>) -> Option<Car> {
    let seed = mix(0xBA75, key as u32, (key >> 32) as u32);
    (unit(seed, 0) < busy(d).1).then(|| Car {
        id: 1 << 63 | key,
        kind: if unit(seed, 1) < 0.2 { Kind::Van } else { Kind::Car },
        seed,
        s,
        x,
        dir: (dir.s, dir.x),
        yaw: atan2(dir.x, -dir.s),
        speed: 0.0,
        accel: 0.0,
        blink: Blink::None,
        parked: true,
    })
}

/// The cars parked for good in the bays on side `side` of the avenue: both kerbs of the streets along
/// the strip, and of the narrow cross streets (the wide ones' bays are track 0's), to `give`. Each
/// faces the way its side of the street's traffic goes.
fn bays(strip: u8, side: f32, area: &Rect, stage: Stage, give: &mut impl FnMut(Car) -> bool) -> bool {
    if out_from_middle(area, side).1 < 0.0 {
        return false;
    }
    let (mid, last) = (STRIP_WIDTH * 0.5, last_street(stage));
    let sign = u64::from(side > 0.0);
    let near = |s: f32, x: f32| {
        s + REACH >= area.s0 && s - REACH <= area.s1 && x + REACH >= area.x0 && x - REACH <= area.x1
    };
    let b0 = (floor((area.x0 - REACH - grid_x(0)) / BLOCK) as i32).max(FIRST);
    let b1 = (floor((area.x1 + REACH - grid_x(0)) / BLOCK) as i32).min(last - 1);
    // Along the strip, every stretch between cross streets: the half nearer the avenue goes the way
    // the row inside it goes round, out along its outer street.
    for k in 1..=ROWS {
        let (centre, w) =
            if k == ROWS { (row_edge(k) - BANK_HALF, 2.0 * BANK_HALF) } else { (row_edge(k), lane_width(k)) };
        for (half, out) in [(0u64, -1.0f32), (1, 1.0)] {
            let s = mid + side * (centre + out * bay_line(w));
            if s + REACH < area.s0 || s - REACH > area.s1 {
                continue;
            }
            let dir = V::new(0.0, -side * out);
            for b in b0..=b1 {
                let x0 = grid_x(b) + cross_width(b) * 0.5;
                let len = grid_x(b + 1) - cross_width(b + 1) * 0.5 - x0;
                let bays = floor((len - BAY_CLEAR) / BAY_PITCH) as i32;
                for i in 2..bays {
                    let x = x0 + BAY_PITCH * (i as f32 + 0.5);
                    let key = (u64::from(strip) << 56)
                        | (sign << 55)
                        | (half << 54)
                        | ((k as u64) << 48)
                        | ((b as u64) << 8)
                        | i as u64;
                    if near(s, x)
                        && parked(key, s, x, dir, district_of(strip, b, stage)).is_some_and(&mut *give)
                    {
                        return true;
                    }
                }
            }
        }
    }
    // Across it: the narrow cross streets between the streets along it (bays every 6 m of `s`, as
    // the paint has them), +s on the +x half.
    for b in b0..=b1 + 1 {
        let gx = grid_x(b);
        if b % SIGNAL_EVERY == 0
            || b <= FIRST
            || b >= last
            || gx + STREET < area.x0 - REACH
            || gx - STREET > area.x1 + REACH
        {
            continue;
        }
        for k in 1..=ROWS {
            let lo = if k == 1 { ROAD_OUT + 0.5 } else { row_edge(k - 1) + lane_width(k - 1) * 0.5 };
            let hi = row_edge(k) - lane_width(k) * 0.5;
            let (s0, s1) = if side > 0.0 { (mid + lo, mid + hi) } else { (mid - hi, mid - lo) };
            if s1 + REACH < area.s0 || s0 - REACH > area.s1 {
                continue;
            }
            let (i0, i1) = (
                floor((s0 + BAY_CLEAR) / BAY_PITCH) as i32 + 1,
                floor((s1 - BAY_CLEAR) / BAY_PITCH) as i32 - 1,
            );
            for (half, out) in [(0u64, -1.0f32), (1, 1.0)] {
                let (x, dir) = (gx + out * bay_line(STREET), V::new(out, 0.0));
                for i in i0..=i1 {
                    let s = BAY_PITCH * (i as f32 + 0.5);
                    let key = (1 << 62)
                        | (u64::from(strip) << 56)
                        | (sign << 55)
                        | (half << 54)
                        | ((k as u64) << 48)
                        | ((b as u64) << 16)
                        | (i as u64 & 0xFFFF);
                    if near(s, x)
                        && parked(key, s, x, dir, district_of(strip, b, stage)).is_some_and(&mut *give)
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAGE: Stage = Stage(0);

    /// A sample of every kind of ring: every row either side, both tracks, at the rows' ends and in
    /// the middle, on every strip.
    fn rings(mut f: impl FnMut(&Ring, i32, i32)) {
        for strip in 0..3u8 {
            for side in [-1.0f32, 1.0] {
                for row in 1..=ROWS {
                    for b0 in [FIRST, 40, 100, last_street(STAGE) - SIGNAL_EVERY] {
                        f(
                            &Ring::new(strip, side, row, 0, b0, b0 + SIGNAL_EVERY, STAGE),
                            b0,
                            b0 + SIGNAL_EVERY,
                        );
                    }
                    if tracks(row) > 1 {
                        f(
                            &Ring::new(strip, side, row, 1, FIRST, last_street(STAGE), STAGE),
                            FIRST,
                            last_street(STAGE),
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_rings_close_up_round_their_blocks() {
        rings(|r, _, _| {
            for i in 0..4 {
                let (t, next) = (r.turns[i], r.turns[(i + 1) % 4]);
                // Each straight runs from one turn's end to the next's start, in line with both.
                let gap = next.start() - t.end();
                assert!(gap.dot(t.e).abs() < 1e-2, "a turn's end out of line with the next's start: {r:?}");
                assert!(gap.dot(t.e.right()) > 50.0, "a straight too short: {r:?}");
                assert!((next.e.dot(t.e.right()) - 1.0).abs() < 1e-6, "turns not a right turn apart");
            }
            for i in 1..9 {
                let (a, ea) = r.pose(r.at[i] - 1e-3);
                let (b, eb) = r.pose(r.at[i] + 1e-3);
                assert!(
                    (b - a).dot(b - a) < 1e-4 && (eb - ea).dot(eb - ea) < 1e-4,
                    "a kink at piece {i}: {r:?}"
                );
            }
        });
    }

    #[test]
    fn laps_keep_time_with_the_signals() {
        rings(|r, b0, b1| {
            let (up, dn) = if r.side > 0.0 { (b0, b1) } else { (b1, b0) };
            // It leaves the up leg's queue as the light turns green for its outer street there, and
            // the down leg's as it does for its inner street.
            assert_eq!(r.t0, phase(r.strip, up, r.row));
            assert_eq!((r.t0 + r.lap / 2) % CYCLE, phase(r.strip, dn, r.row - 1));
            assert_eq!(r.lap % CYCLE, 0);
        });
    }

    #[test]
    fn every_car_draws_up_in_time_and_its_queue_and_bays_fit_their_leg() {
        rings(|r, _, _| {
            let half = (r.lap / 2) as f32 / HZ;
            for h in 0..2 {
                // The straight of the leg this half ends on, and where the kerb of the street it
                // turned off is, round the ring.
                let (leg, turn) = if h == 0 { (r.at[3], r.turns[1]) } else { (r.at[7], r.turns[3]) };
                let (kerb, _) = along(if h == 0 { r.row } else { r.row - 1 }, r.row, r.track);
                let kerb = leg - ((turn.end().s - STRIP_WIDTH * 0.5).abs() - kerb).abs();
                for n in 0..SLOTS {
                    let (d, from) = r.drive(h, n, false);
                    let time = d.time();
                    assert!(
                        time + 8.0 < half,
                        "place {n} draws up {:.1} s before its green: {r:?}",
                        half - time
                    );
                    assert!(d.z.iter().all(|z| *z > 0.0), "{:?}", d.z);
                    // Its place in the queue on the leg's straight.
                    let spill = from + d.length() - 0.5 * LONGEST - leg;
                    assert!(spill > 0.0, "place {n}'s queue spills off its leg by {spill}: {r:?}");
                    if r.hourly {
                        // Its bay, and the run in to it, on the straight and clear of the crossing it
                        // came in by.
                        let (p, from) = r.drive(h, n, true);
                        let bay = from + p.length();
                        assert!(bay - BAY_BACK > leg, "place {n}'s run in to its bay starts on the turn");
                        let clear = bay - 0.5 * LONGEST - kerb;
                        assert!(clear >= BAY_CLEAR, "place {n}'s bay {clear:.1} m from the crossing: {r:?}");
                    }
                }
                if r.hourly {
                    let pull =
                        r.drive(h, 0, false).0.time() + 1.0 + (PULL_GAP - START_DELAY) * (SLOTS - 1) as f32;
                    assert!(pull + PULL_TIME + 1.0 < half, "the last pulls out too late: {pull}: {r:?}");
                }
            }
        });
    }

    #[test]
    fn a_drive_speeds_up_and_slows_down_smoothly_and_ends_where_it_should() {
        let r = Ring::new(1, 1.0, 3, 0, 60, 64, STAGE);
        for (h, n, park) in [(0, 0, false), (1, 8, false), (0, 4, true)] {
            let (d, _) = r.drive(h, n, park);
            let (a, b) = (r.turns[2 * h].speed(), r.turns[2 * h + 1].speed());
            let lim = [LEG_SPEED, a, CRUISE, b, LEG_SPEED];
            let mut last = d.at(0.0);
            assert_eq!(last.r, 0.0);
            let steps = 4_000;
            let dt = d.time() / steps as f32;
            for i in 1..=steps {
                let now = d.at(dt * i as f32);
                assert!(
                    now.r >= last.r - 1e-4 && now.r - last.r <= CRUISE * dt + 1e-3,
                    "{h} {n}: {last:?} → {now:?}"
                );
                assert!((now.v - last.v).abs() <= BRAKE * dt + 1e-3, "{h} {n}: {last:?} → {now:?}");
                assert!(now.v <= lim[now.zone] + 1e-3, "{h} {n}: over the limit, {now:?}");
                last = now;
            }
            assert!((last.r - d.length()).abs() < 1e-2 && last.v.abs() < 1e-3, "{last:?} of {}", d.length());
        }
    }

    #[test]
    fn the_day_brings_the_cars_out_and_the_night_takes_them_in() {
        assert!(out_share(0.92) < 0.4 && out_share(0.14) == 1.0 && out_share(0.72) == 1.0);
        let r = Ring::new(0, 1.0, 2, 0, 40, 44, STAGE);
        assert_eq!(DAY_TICKS % r.lap, 0, "a day holds whole laps of a stretch's ring");
        let laps = i64::from(DAY_TICKS / r.lap);
        for p in 0..r.platoons {
            let out = (0..laps).filter(|k| r.out(p, *k)).count();
            assert!(out > 0 && out < laps as usize, "platoon {p} out {out} of {laps} laps");
            assert_eq!(r.out(p, 3), r.out(p, 3 + laps), "the same every day");
        }
    }
}
