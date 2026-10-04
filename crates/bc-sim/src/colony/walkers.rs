//! The city's people: who walks its pavements, crosses Hub Gate's square, sits on the avenue's
//! benches and waits on the platforms. Like everything in the colony they're a closed form of the
//! tick: [`each_walker`] works out the few near an area from the lines they walk and the slots on
//! them, the same on every screen. Nothing is stored or sent, and the server isn't asked.
//!
//! They're ghosts to pilots (a pilot walks or drives through them), but never to each other or to
//! the city. Their lines are laid where nothing stands: round each block's pavement between its
//! lamp posts and its walls, up and down the avenue's two walks between its trees and its beds,
//! along the quays, round a park's loop and a plaza's monument, down the window banks'
//! promenades, and over Hub Gate's square on the dark bands of its paving. Every line is a loop,
//! its people on slots evenly round it at the line's speed, so they keep their distance; and no
//! two loops come within arm's reach, but for the square's, which cross, and whose slots are timed
//! so that people pass each other's paths half a slot apart.
//!
//! A line moves its people on a slot every `step` ticks, and a colony day is a whole number of its
//! laps, so everyone on a line comes round each day to the bit. Who's out is the slot's own draw
//! against how busy the street is (its district, the hour, a station near), asked as the slot
//! passes its line's start, where people come and go; the platforms' people come up the steps,
//! board the next train and get off it on its timetable (`transit`).
//!
//! The only roads anybody crosses are the narrow cross streets (three of every four), on their
//! zebras, at any time: the traffic (`traffic`) drives none of them and parks no nearer a zebra than
//! 10 m. Nobody crosses a street along the strip, a wide cross street or the avenue yet (the signals'
//! people's half is for that); `tests/life.rs` checks the people against the cars.

use core::f32::consts::{FRAC_PI_2, PI, TAU};

use super::city::{
    AVENUE, BLOCK, BlockInfo, BlockKind, CANAL_ROW, CITY, CityBox, DISTRICT_BLOCKS, GRID_X0, HUB_GATE, KERB,
    Rect, SIDEWALK, SITE, Stage, block, block_index, block_rect, channel, cross_width, district_of, grid_x,
    has_block, lots, mix, room, row_at, row_span, unit,
};
use super::frame::STRIP_WIDTH;
use super::furniture::{
    AVENUE_TREE, BENCH_END, BENCH_HEIGHT, BENCH_PITCH, LAMP_CORNER, LAMP_IN, PARK_LOOP, PLAZA_RING,
    lamp_count,
};
use super::time::{DAY_TICKS, Day, day};
use super::transit::{
    CAR_WIDTH, DOOR_AT, DOOR_MARGIN, DOOR_WIDTH, HEADWAY_TICKS, PLATFORM_HALF, PLATFORM_LENGTH, STATIONS,
    TRACK_OFFSET, car_offset, platform_top, stands, station_x,
};
use crate::TICK_HZ;
use crate::content::city::DistrictKind;
use crate::math::{atan2, cos, floor, sin, sqrt};

/// Half a person's width, m: nobody comes nearer another's middle than twice this.
pub const RADIUS: f32 = 0.3;
/// Slots down a line are about this far apart, m.
pub const SLOT: f32 = 4.0;
/// People come and go over this far down their line, m.
pub const FADE: f32 = 1.5;
/// Somebody standing about is asked whether they're still there this often, ticks (30 s), and
/// comes or goes over this long (2 s).
const HOLD: u32 = 900;
const HOLD_FADE: u32 = 60;

/// What a person's doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    Walk,
    Run,
    Stand,
    /// On a bench: `(s, x)` is the seat (as `city::Seat`'s), `h` the ground under it.
    Sit,
}

/// One of the city's people, this moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Walker {
    /// Who: the same while they're out (a client's pool keeps their figure by it).
    pub id: u32,
    /// Where, in city coordinates: `h` the ground under their feet.
    pub s: f32,
    pub x: f32,
    pub h: f32,
    /// Which way they face: the walker's yaw (0 faces −s, τ/4 faces +x).
    pub yaw: f32,
    /// Over the ground, m/s.
    pub speed: f32,
    pub pose: Pose,
    /// How far through a stride, 0..1 (two steps; `figure::stride_phase`'s cycle).
    pub stride: f32,
    /// 1 when they're there; less while they come out or go in (a door, a train, the steps).
    pub fade: f32,
    /// Their clothes and build.
    pub seed: u32,
}

// ---- How busy the streets are -------------------------------------------------------------------

/// How busy a place is, 0..1: by day, at night, and the extra of an evening out.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Busy {
    day: f32,
    night: f32,
    eve: f32,
}

/// The evening's middle, through the day (`Day::phase`): the dusk's, as the lamps come on.
const EVENING: f32 = 0.8;

impl Busy {
    const HUB: Busy = Busy { day: 1.0, night: 0.3, eve: 0.3 };
    const BANK: Busy = Busy { day: 0.5, night: 0.04, eve: 0.35 };

    /// A district's streets.
    fn of(kind: Option<DistrictKind>) -> Busy {
        use DistrictKind::*;
        let (day, night, eve) = match kind {
            None => (0.0, 0.0, 0.0),
            Some(Civic) => (0.75, 0.06, 0.1),
            Some(Business) => (1.0, 0.05, 0.15),
            Some(Midtown) => (0.85, 0.2, 0.45),
            Some(Residential) => (0.45, 0.08, 0.2),
            Some(OldTown) => (0.7, 0.25, 0.4),
            Some(University) => (0.65, 0.1, 0.25),
            Some(Works) => (0.35, 0.03, 0.05),
            Some(Park) => (0.5, 0.04, 0.15),
            Some(Port) => (0.3, 0.05, 0.05),
        };
        Busy { day, night, eve }
    }

    /// Busier by `k`.
    fn times(self, k: f32) -> Busy {
        Busy { day: self.day * k, night: self.night * k, eve: self.eve * k }
    }

    /// How busy at the hour `d`.
    fn at(&self, d: &Day) -> f32 {
        let lit = smooth(d.daylight / 0.2);
        let eve = smooth(1.0 - (d.phase - EVENING).abs() / 0.09);
        (self.night + (self.day - self.night) * lit + self.eve * eve).clamp(0.0, 1.0)
    }
}

/// Busier near a station: half as busy again at its platform, less out to 300 m.
fn by_station(x: f32) -> f32 {
    let i = (((x - station_x(0)) / (station_x(1) - station_x(0)) + 0.5) as i32).clamp(0, STATIONS as i32 - 1);
    1.0 + 0.5 * (1.0 - (x - station_x(i as usize)).abs() / 300.0).max(0.0)
}

/// A block's (or its stretch of street's) streets: its district's, Hub Gate's round the square,
/// none on the building site.
fn busy_at(strip: u8, bx: i32, row: i32, stage: Stage) -> Busy {
    if bx < CITY.0 {
        return Busy::HUB;
    }
    let b = Busy::of(district_of(strip, bx, stage).map(|(_, k)| k));
    if row.abs() <= 2 { b.times(by_station(grid_x(bx) + 64.0)) } else { b }
}

// ---- Lines and their slots ----------------------------------------------------------------------

/// The tick, and how far into the next.
#[derive(Clone, Copy, Debug)]
struct Clock {
    t: u32,
    frac: f32,
}

impl Clock {
    /// A line's slots at a slot every `step` ticks: how many it's moved on, and how far to the next.
    fn slots(&self, step: u32) -> (u32, f32) {
        (self.t / step, ((self.t % step) as f32 + self.frac) / step as f32)
    }

    /// Seconds since tick `t0` (which may be ahead: then negative).
    fn since(&self, t0: i64) -> f32 {
        (i64::from(self.t) - t0) as f32 / TICK_HZ as f32 + self.frac / TICK_HZ as f32
    }
}

/// How many slots a loop `len` long gets, about [`SLOT`] apart and no nearer than 2 m, so that a
/// day is a whole number of its laps at a slot every `step` ticks (which divides a day).
fn fit(len: f32, step: u32) -> u32 {
    let total = DAY_TICKS / step;
    let want = len / SLOT;
    let most = len / 2.0;
    let mut best = 0u32;
    let mut d = 1u32;
    while d * d <= total {
        if total.is_multiple_of(d) {
            for c in [d, total / d] {
                let better = best == 0 || (c as f32 - want).abs() < (best as f32 - want).abs();
                if c as f32 <= most && better {
                    best = c;
                }
            }
        }
        d += 1;
    }
    best.max(1)
}

/// A loop round a rectangle with rounded corners of radius `r` (at most half its shorter side; a
/// capsule, or a circle, when it's that). Its ring parameter runs counter-clockwise seen with +x
/// to the right and +s up: +x along its −s side, from that side's start.
#[derive(Clone, Copy, Debug)]
struct Ring {
    rect: Rect,
    r: f32,
}

impl Ring {
    /// Its straights' lengths: along x, and across.
    fn sides(&self) -> (f32, f32) {
        ((self.rect.length() - 2.0 * self.r).max(0.0), (self.rect.width() - 2.0 * self.r).max(0.0))
    }

    fn len(&self) -> f32 {
        let (a, b) = self.sides();
        2.0 * (a + b) + TAU * self.r
    }

    /// The point `v` round it, and the way on there: `(s, x, ds, dx)`.
    fn at(&self, v: f32) -> (f32, f32, f32, f32) {
        let (a, b) = self.sides();
        let (r, q, e) = (self.r, FRAC_PI_2 * self.r, self.rect);
        let corner = |cx: f32, cs: f32, from: f32, d: f32| {
            let th = from + if r > 0.0 { d / r } else { 0.0 };
            let (sn, cs_) = (sin(th), cos(th));
            (cs + r * sn, cx + r * cs_, cs_, -sn)
        };
        let mut v = v;
        if v < a {
            return (e.s0, e.x0 + r + v, 0.0, 1.0);
        }
        v -= a;
        if v < q {
            return corner(e.x1 - r, e.s0 + r, -FRAC_PI_2, v);
        }
        v -= q;
        if v < b {
            return (e.s0 + r + v, e.x1, 1.0, 0.0);
        }
        v -= b;
        if v < q {
            return corner(e.x1 - r, e.s1 - r, 0.0, v);
        }
        v -= q;
        if v < a {
            return (e.s1, e.x1 - r - v, 0.0, -1.0);
        }
        v -= a;
        if v < q {
            return corner(e.x0 + r, e.s1 - r, FRAC_PI_2, v);
        }
        v -= q;
        if v < b {
            return (e.s1 - r - v, e.x0, -1.0, 0.0);
        }
        v -= b;
        corner(e.x0 + r, e.s0 + r, PI, v.min(q))
    }

    /// The ring parameter of a point on one of its straights.
    fn v_of(&self, s: f32, x: f32) -> f32 {
        let (a, b) = self.sides();
        let (r, q, e) = (self.r, FRAC_PI_2 * self.r, self.rect);
        let d = [(s - e.s0).abs(), (x - e.x1).abs(), (s - e.s1).abs(), (x - e.x0).abs()];
        let k = (0..4).fold(0, |m, i| if d[i] < d[m] { i } else { m });
        match k {
            0 => (x - e.x0 - r).clamp(0.0, a),
            1 => a + q + (s - e.s0 - r).clamp(0.0, b),
            2 => a + 2.0 * q + b + (e.x1 - r - x).clamp(0.0, a),
            _ => 2.0 * a + 3.0 * q + b + (e.s1 - r - s).clamp(0.0, b),
        }
    }

    /// Calls `g` with the stretches of its sides that may lie in `area`: `(lo, hi)` round it, in
    /// its side `(from, to)`. Stops when `g` returns true, and says whether it did.
    fn spans(&self, area: &Rect, mut g: impl FnMut(f32, f32, f32, f32) -> bool) -> bool {
        let (a, b) = self.sides();
        let (r, q, e) = (self.r, FRAC_PI_2 * self.r, self.rect);
        let (xa, xb, sa, sb) = (e.x0 + r, e.x1 - r, e.s0 + r, e.s1 - r);
        let within = |lo: f32, hi: f32, v: f32, len: f32| (lo <= hi).then_some((v + lo, v + hi, v, v + len));
        let hits = |x0: f32, x1: f32, s0: f32, s1: f32| {
            area.x0 <= x1 && x0 <= area.x1 && area.s0 <= s1 && s0 <= area.s1
        };
        let mut v = 0.0;
        let mut out = [None; 8];
        if a > 0.0 && (area.s0..=area.s1).contains(&e.s0) {
            out[0] = within((area.x0 - xa).max(0.0), (area.x1 - xa).min(a), v, a);
        }
        v += a;
        if hits(xb, e.x1, e.s0, sa) {
            out[1] = Some((v, v + q, v, v + q));
        }
        v += q;
        if b > 0.0 && (area.x0..=area.x1).contains(&e.x1) {
            out[2] = within((area.s0 - sa).max(0.0), (area.s1 - sa).min(b), v, b);
        }
        v += b;
        if hits(xb, e.x1, sb, e.s1) {
            out[3] = Some((v, v + q, v, v + q));
        }
        v += q;
        if a > 0.0 && (area.s0..=area.s1).contains(&e.s1) {
            out[4] = within((xb - area.x1).max(0.0), (xb - area.x0).min(a), v, a);
        }
        v += a;
        if hits(e.x0, xa, sb, e.s1) {
            out[5] = Some((v, v + q, v, v + q));
        }
        v += q;
        if b > 0.0 && (area.x0..=area.x1).contains(&e.x0) {
            out[6] = within((sb - area.s1).max(0.0), (sb - area.s0).min(b), v, b);
        }
        v += b;
        // (The last side runs to the end, whatever its sum's rounding.)
        if hits(e.x0, xa, e.s0, sa) {
            out[7] = Some((v, v + q, v, f32::INFINITY));
        }
        out.into_iter().flatten().any(|(lo, hi, from, to)| g(lo, hi, from, to))
    }
}

/// A loop people walk, its slots round it: `n` of them, moving on one every `step` ticks, slot 0
/// at `start` round the ring at step 0; the way it's walked (`ccw`: as its ring runs); where its
/// people come and go; the ground under it; the share of its slots taken when it's busiest, and
/// how busy it is.
#[derive(Clone, Copy, Debug)]
struct Line {
    ring: Ring,
    ccw: bool,
    n: u32,
    step: u32,
    start: f32,
    /// Where people come and go, down the walk from `start` (lowest first; `np` of them): its
    /// corners, or a capsule's ends ([`Line::at_corners`]). Who's out is asked as a slot passes one.
    portals: [f32; 4],
    np: usize,
    h: f32,
    seed: u32,
    share: f32,
    busy: Busy,
    run: bool,
    /// It crosses streets: on them, the ground is the street's, not `h`.
    streets: bool,
}

impl Line {
    #[allow(clippy::too_many_arguments)]
    fn new(ring: Ring, ccw: bool, step: u32, h: f32, seed: u32, share: f32, busy: Busy, run: bool) -> Line {
        let n = fit(ring.len(), step);
        Line {
            ring,
            ccw,
            n,
            step,
            start: 0.0,
            portals: [0.0; 4],
            np: 1,
            h,
            seed,
            share,
            busy,
            run,
            streets: false,
        }
    }

    /// The same, its people coming and going at its corners (a capsule's at its ends, a circle's at
    /// its start).
    fn at_corners(mut self) -> Line {
        let (a, b) = self.ring.sides();
        let (q, len) = (FRAC_PI_2 * self.ring.r, self.ring.len());
        let corners = [a + 0.5 * q, a + 1.5 * q + b, 2.0 * a + 2.5 * q + b, 2.0 * a + 3.5 * q + 2.0 * b];
        let (v, np) = match (a > 0.0, b > 0.0) {
            (true, true) => (corners, 4),
            (true, false) => ([a + q, 2.0 * a + 3.0 * q, 0.0, 0.0], 2),
            (false, true) => ([0.0, b + 2.0 * q, 0.0, 0.0], 2),
            (false, false) => ([self.start, 0.0, 0.0, 0.0], 1),
        };
        let mut u = v.map(|v| wrap(if self.ccw { v - self.start } else { self.start - v }, len));
        u[..np].sort_by(f32::total_cmp);
        self.portals = u;
        self.np = np;
        self
    }

    fn slot(&self) -> f32 {
        self.ring.len() / self.n as f32
    }

    /// The ground under somebody on it at `x`.
    fn h_at(&self, x: f32) -> f32 {
        if self.streets { kerb_or_street(x) } else { self.h }
    }

    fn speed(&self) -> f32 {
        self.slot() * TICK_HZ as f32 / self.step as f32
    }

    /// Whether slot `j`'s person is out on leg `leg` (`np` legs a lap, each from a portal to the
    /// next): asked at the hour it passed the leg's portal.
    fn out(&self, j: u32, leg: i64) -> bool {
        let (lap, k) = (leg.div_euclid(self.np as i64), leg.rem_euclid(self.np as i64) as usize);
        let at = lap * i64::from(self.n) - i64::from(j) + floor(self.portals[k] / self.slot()) as i64;
        let t = (at.max(0) as u64 * u64::from(self.step) % u64::from(DAY_TICKS)) as u32;
        unit(mix(self.seed, j, 0x0D7), 0) < self.share * self.busy.at(&day(t, 0.0))
    }

    /// Slot `j`'s leg at step `steps` plus `between`, and how far it is from the leg's portal and
    /// to the next, m.
    fn leg(&self, j: u32, steps: u32, between: f32) -> (i64, f32, f32) {
        let (n, np, len) = (self.n, self.np, self.ring.len());
        let all = u64::from(j) + u64::from(steps);
        let u = ((all % u64::from(n)) as f32 + between) * self.slot();
        let p = &self.portals[..np];
        let k = p.iter().filter(|p| **p <= u).count();
        let since = if k == 0 { u + len - p[np - 1] } else { u - p[k - 1] };
        let till = if k == np { p[0] + len - u } else { p[k] - u };
        ((all / u64::from(n)) as i64 * np as i64 + k as i64 - 1, since, till)
    }

    /// Ring parameter of the point `u` down the walk from its start.
    fn v(&self, u: f32) -> f32 {
        let len = self.ring.len();
        let v = if self.ccw { self.start + u } else { self.start - u };
        let w = v - floor(v / len) * len;
        if w >= len { w - len } else { w }
    }

    /// The person in slot `j`, where the line has them at step `steps` plus `between`.
    fn walker(&self, j: u32, steps: u32, between: f32, fade: f32) -> Walker {
        let n = self.n;
        let i = (j + steps % n) % n;
        let lam = self.slot();
        let (s, x, ds, dx) = self.ring.at(self.v((i as f32 + between) * lam));
        let (ds, dx) = if self.ccw { (ds, dx) } else { (-ds, -dx) };
        let seed = mix(self.seed, j, 0x5EED);
        let per = if self.run { 2.6 } else { 1.5 };
        let strides = floor(lam / per + 0.5).max(1.0);
        Walker {
            id: mix(self.seed, j, 0x1D),
            s,
            x,
            h: self.h_at(x),
            yaw: yaw_of(ds, dx),
            speed: self.speed(),
            pose: if self.run { Pose::Run } else { Pose::Walk },
            stride: fract(between * strides + unit(seed, 2)),
            fade,
            seed,
        }
    }

    /// Calls `f` with each of its people in `area` but those whose slots `away` says they're off
    /// (sitting down); stops when `f` returns true, and says whether it did.
    fn each(
        &self,
        area: &Rect,
        c: Clock,
        away: impl Fn(u32) -> bool,
        f: &mut impl FnMut(&Walker) -> bool,
    ) -> bool {
        let (len, n) = (self.ring.len(), self.n);
        let lam = len / n as f32;
        let (steps, between) = c.slots(self.step);
        let shift = steps % n;
        self.ring.spans(area, |lo, hi, from, to| {
            // The slots that may be in this stretch, by where their place down the walk is.
            let (u0, u1) = if self.ccw {
                (lo - self.start, hi - self.start)
            } else {
                (self.start - hi, self.start - lo)
            };
            let i0 = floor(u0 / lam - between) as i64 - 1;
            let i1 = (floor(u1 / lam - between) as i64 + 1).min(i0 + i64::from(n) - 1);
            for k in i0..=i1 {
                let i = k.rem_euclid(i64::from(n)) as u32;
                let u = (i as f32 + between) * lam;
                let v = self.v(u);
                if !(from <= v && v < to) {
                    continue;
                }
                let j = (i + n - shift) % n;
                let (leg, since, till) = self.leg(j, steps, between);
                if !self.out(j, leg) || away(j) {
                    continue;
                }
                let mut fade = 1.0f32;
                if since < FADE && !self.out(j, leg - 1) {
                    fade = since / FADE;
                }
                if till < FADE && !self.out(j, leg + 1) {
                    fade = fade.min(till / FADE);
                }
                let w = self.walker(j, steps, between, fade);
                if fade > 0.0 && inside(area, w.s, w.x) && f(&w) {
                    return true;
                }
            }
            false
        })
    }
}

// ---- Small pieces -------------------------------------------------------------------------------

fn smooth(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

fn fract(v: f32) -> f32 {
    v - floor(v)
}

fn wrap(v: f32, m: f32) -> f32 {
    let w = v - floor(v / m) * m;
    if w >= m { w - m } else { w }
}

/// The walker's yaw facing `(ds, dx)`.
fn yaw_of(ds: f32, dx: f32) -> f32 {
    atan2(dx, -ds)
}

/// Whether a point is in `area`: from its low edges, short of its high ones (so tiles of a bigger
/// area find everyone once).
fn inside(area: &Rect, s: f32, x: f32) -> bool {
    area.s0 <= s && s < area.s1 && area.x0 <= x && x < area.x1
}

/// Somebody standing at `(s, x)` on ground `h`, facing about `yaw`, while the place is busy
/// enough for them (`share` of its spots when busiest): there or not is asked every [`HOLD`]
/// ticks, and they come or go over [`HOLD_FADE`]. They look about a little.
#[allow(clippy::too_many_arguments)]
fn stander(s: f32, x: f32, h: f32, yaw: f32, seed: u32, share: f32, busy: Busy, c: Clock) -> Option<Walker> {
    let w = c.t / HOLD;
    let there = |w: u32| unit(seed, 0) < share * busy.at(&day(w * HOLD, 0.0));
    let now = there(w);
    let into = ((c.t % HOLD) as f32 + c.frac) / HOLD_FADE as f32;
    let fade = match (now, w > 0 && there(w - 1)) {
        (true, true) => 1.0,
        (true, false) if w > 0 => into.min(1.0),
        (true, false) => 1.0,
        (false, true) if into < 1.0 => 1.0 - into,
        _ => return None,
    };
    if fade <= 0.0 {
        return None;
    }
    // A look round every 16 to 30 s (a whole number of times a day).
    let period = [480u32, 600, 720, 900][(seed % 4) as usize];
    let ph = ((c.t % period) as f32 + c.frac) / period as f32 + unit(seed, 1);
    Some(Walker {
        id: mix(seed, 0x57A, 0),
        s,
        x,
        h,
        yaw: yaw + 0.35 * sin(TAU * ph),
        speed: 0.0,
        pose: Pose::Stand,
        stride: 0.0,
        fade,
        seed: mix(seed, 0x5EED, 1),
    })
}

/// Down a path of points at `speed` for `d` metres: where, and the way on.
fn along(path: &[(f32, f32)], d: f32) -> ((f32, f32), (f32, f32)) {
    let mut left = d.max(0.0);
    for w in path.windows(2) {
        let ((s0, x0), (s1, x1)) = (w[0], w[1]);
        let l = sqrt((s1 - s0) * (s1 - s0) + (x1 - x0) * (x1 - x0));
        if l <= 0.0 {
            continue;
        }
        let dir = ((s1 - s0) / l, (x1 - x0) / l);
        if left <= l {
            return ((s0 + dir.0 * left, x0 + dir.1 * left), dir);
        }
        left -= l;
    }
    let n = path.len();
    let (a, b) = (path[n - 2], path[n - 1]);
    let l = sqrt((b.0 - a.0) * (b.0 - a.0) + (b.1 - a.1) * (b.1 - a.1)).max(1e-6);
    (b, ((b.0 - a.0) / l, (b.1 - a.1) / l))
}

fn path_len(path: &[(f32, f32)]) -> f32 {
    path.windows(2)
        .map(|w| sqrt((w[1].0 - w[0].0) * (w[1].0 - w[0].0) + (w[1].1 - w[0].1) * (w[1].1 - w[0].1)))
        .sum()
}

// ---- Round a block's pavement -------------------------------------------------------------------

/// A block's pavement's lines, in from its kerb: one by the kerb, walked with the kerb on the
/// right; two by the walls, the other way (so on any side people keep to the right); how many ticks
/// a slot, and the share taken when busiest. The street lamps stand 0.8 m in (0.92 m to their far
/// side), the walls 5 m.
const PAVEMENT: [(f32, bool, u32, f32); 3] =
    [(1.6, true, 90, 0.2), (2.6, false, 80, 0.22), (3.6, false, 108, 0.16)];
/// The pavement's lines turn their corners round a point this far in from both kerbs.
const PAVEMENT_TURN: f32 = 4.6;
/// Somebody standing by the walls, or at the kerb (between its lamps), this far in; a spot for
/// them every so far, none within so far of a corner.
const BY_WALL: f32 = 4.5;
const AT_KERB: f32 = 0.95;
const SPOT_GAP: f32 = 7.0;
const SPOT_CORNER: f32 = 7.0;

/// Most rows' pavements run round a whole run of their blocks, from wide cross street to wide cross
/// street (every fourth), crossing the narrow ones on their zebras (0.5 to 4.5 m out from the lane's
/// kerb, as the lines are): the traffic (pass 4.1) keeps off those streets, its rings turning at the
/// wide ones, and parks no nearer a zebra than 10 m. Not the avenue's rows, whose cross streets'
/// zebras are out on the avenue's pavement and whose kerbs there have bays beside them, nor the
/// canal's, whose quays its water cuts.
const RUN: i32 = 4;
/// The pavement down a run's side on a narrow cross street: a capsule on the run's lines' insets,
/// turning back this far short of the corners, where the run's lines cross.
const SIDE_CLEAR: f32 = 5.0;

fn runs_round(row: i32) -> bool {
    row.abs() >= 2 && row != CANAL_ROW
}

/// The blocks of row `row`'s run `g` (blocks `RUN·g` on, to the next wide cross street) that there
/// are: Hub Gate's and the city's, built out to `stage`.
fn run(row: i32, g: i32, stage: Stage) -> Option<(i32, i32)> {
    let last = (CITY.1 + DISTRICT_BLOCKS * i32::from(stage.0)).min(SITE.1);
    let mut lo = (RUN * g).max(HUB_GATE.0);
    let hi = (RUN * g + RUN - 1).min(last);
    while lo <= hi && !has_block(lo, row) {
        lo += 1;
    }
    (lo <= hi).then_some((lo, hi))
}

/// The ground under somebody on a run's line at `x`: the street, if they're wholly on a cross
/// street, else the kerb's top.
fn kerb_or_street(x: f32) -> f32 {
    let g = floor((x - GRID_X0) / BLOCK + 0.5) as i32;
    if (x - grid_x(g)).abs() + RADIUS <= 0.5 * cross_width(g) { 0.0 } else { KERB }
}

/// A run's lines: a block's (`PAVEMENT`), round the whole run.
fn run_lines(strip: u8, row: i32, (lo, hi): (i32, i32), busy: Busy) -> [Line; 3] {
    let (a, b) = (block_rect(lo, row), block_rect(hi, row));
    let rect = Rect::new(a.s0, a.s1, a.x0, b.x1);
    let seed = mix(u32::from(strip) + 1, lo as u32, (row + 64) as u32);
    core::array::from_fn(|k| {
        let (inset, ccw, step, share) = PAVEMENT[k];
        let ring = Ring { rect: rect.inset(inset), r: PAVEMENT_TURN - inset };
        // Its people come and go at its corners, on the run's end blocks.
        let mut l = Line::new(ring, ccw, step, KERB, mix(seed, 0x5A1 + k as u32, 0), share, busy, false);
        l.streets = true;
        l.at_corners()
    })
}

/// A run's block's sides on its narrow cross streets: a capsule down each.
fn side_lines(b: &BlockInfo, (lo, hi): (i32, i32), busy: Busy) -> [Option<Line>; 2] {
    let r = b.rect;
    let (near, far) = (PAVEMENT[0].0, PAVEMENT[2].0);
    let side = |x0: f32, x1: f32, n: u32| {
        let ring = Ring { rect: Rect::new(r.s0 + SIDE_CLEAR, r.s1 - SIDE_CLEAR, x0, x1), r: 0.5 * (x1 - x0) };
        Line::new(ring, true, 96, KERB, mix(b.seed, 0x51D + n, 0), 0.2, busy, false).at_corners()
    };
    [
        (b.bx > lo).then(|| side(r.x0 + near, r.x0 + far, 0)),
        (b.bx < hi).then(|| side(r.x1 - far, r.x1 - near, 1)),
    ]
}

/// A block's lines round it, on a row that doesn't run round runs (the avenue's).
fn pavement(b: &BlockInfo, busy: Busy) -> [Line; 3] {
    core::array::from_fn(|k| {
        let (inset, ccw, step, share) = PAVEMENT[k];
        let ring = Ring { rect: b.rect.inset(inset), r: PAVEMENT_TURN - inset };
        Line::new(ring, ccw, step, KERB, mix(b.seed, 0x5A1 + k as u32, 0), share, busy, false).at_corners()
    })
}

/// People standing on a block's pavement: by its walls (looking in at a window, or two talking),
/// and at its kerb between the lamp posts, looking out at the street.
fn pavement_standers(
    b: &BlockInfo,
    busy: Busy,
    area: &Rect,
    c: Clock,
    f: &mut impl FnMut(&Walker) -> bool,
) -> bool {
    let r = b.rect;
    let near = Rect::new(area.s0 - 1.0, area.s1 + 1.0, area.x0 - 1.0, area.x1 + 1.0);
    let door = match b.kind {
        BlockKind::Place(i) => room(usize::from(i)).map(|rm| rm.front.point(0.0, 0.0)),
        _ => None,
    };
    // Its sides: along x at s0 and s1 (out −s, +s), across at x0 and x1 (out −x, +x).
    for side in 0..4u32 {
        let (along_x, edge, out) = match side {
            0 => (true, r.s0, -1.0f32),
            1 => (true, r.s1, 1.0),
            2 => (false, r.x0, -1.0),
            _ => (false, r.x1, 1.0),
        };
        let (from, len) = if along_x { (r.x0, r.length()) } else { (r.s0, r.width()) };
        let band = edge - out * 5.0;
        let (lo_e, hi_e) = (edge.min(band), edge.max(band));
        let hit =
            if along_x { lo_e <= near.s1 && near.s0 <= hi_e } else { lo_e <= near.x1 && near.x0 <= hi_e };
        if !hit {
            continue;
        }
        // The kerb's lamp posts (`furniture::by_block`): corner to corner down the lanes, between
        // the corners down the cross streets; none on row ±1's side on the avenue.
        let lamps = lamp_count(len);
        let avenue_side = along_x && (b.row > 0) == (out < 0.0) && b.row.abs() == 1;
        let by_lamp = |u: f32| {
            if avenue_side {
                return false;
            }
            let k = floor(u / len * lamps as f32 + 0.5) as i32;
            (k - 1..=k + 1).any(|i| {
                if i < 0 || i > lamps as i32 || (!along_x && (i == 0 || i == lamps as i32)) {
                    return false;
                }
                let at = (len * i as f32 / lamps as f32).clamp(LAMP_CORNER, len - LAMP_CORNER);
                (u - at).abs() < 0.65
            })
        };
        let spots = floor((len - 2.0 * SPOT_CORNER) / SPOT_GAP) as i32;
        for k in 0..=spots {
            let u = SPOT_CORNER + SPOT_GAP * k as f32;
            let seed = mix(b.seed, 0x57 + side, k as u32);
            let kerb = k % 2 == 1;
            let inset = if kerb { AT_KERB } else { BY_WALL };
            if kerb && by_lamp(u) {
                continue;
            }
            let across = edge - out * inset;
            // Nobody in a key place's doorway.
            if let Some((ds, dx)) = door {
                let (s, x) = if along_x { (across, from + u) } else { (from + u, across) };
                if (s - ds).abs() < 4.0 && (x - dx).abs() < 4.0 {
                    continue;
                }
            }
            let at = |du: f32| if along_x { (across, from + u + du) } else { (from + u + du, across) };
            // Facing the street at the kerb, the wall by it; or two by the wall facing each other.
            let face = if kerb { out } else { -out };
            let facing = if along_x { yaw_of(face, 0.0) } else { yaw_of(0.0, face) };
            let pair = !kerb && unit(seed, 5) < 0.4;
            let ahead = if along_x { yaw_of(0.0, 1.0) } else { yaw_of(1.0, 0.0) };
            let two = [(-0.4, ahead, 0u32), (0.4, ahead + PI, 1)];
            let one = [(0.0, facing, 0u32)];
            for &(du, yaw, m) in if pair { &two[..] } else { &one[..] } {
                let (s, x) = at(du);
                if !inside(area, s, x) {
                    continue;
                }
                let share = if kerb { 0.12 } else { 0.18 };
                if let Some(w) = stander(s, x, KERB, yaw, mix(seed, m, 0), share, busy, c)
                    && f(&w)
                {
                    return true;
                }
            }
        }
    }
    false
}

// ---- Parks, plazas, quays -----------------------------------------------------------------------

/// A park's loop of path (`PARK_LOOP` in from its pavement, 3 m wide): a line either side of its
/// middle, the kerb's side walked with it on the right. Its lamps stand 2.4 m inside the loop's
/// middle.
const PARK_PATH: [(f32, bool); 2] = [(-0.7, true), (0.7, false)];
/// Round a park's middle (its paved ring, 11 m across): two lines, if no pavilion's near.
const PARK_ROUND: [(f32, bool); 2] = [(7.5, false), (9.0, true)];
/// Round a plaza's monument, inside its lamps' ring and out past it.
const PLAZA_ROUND: [(f32, bool); 4] = [(11.0, true), (12.4, false), (28.0, true), (29.4, false)];

fn circle(s: f32, x: f32, r: f32) -> Ring {
    Ring { rect: Rect::new(s - r, s + r, x - r, x + r), r }
}

/// A block's inner lines: a park's paths, a plaza's rings.
fn inner_lines(b: &BlockInfo, busy: Busy, out: &mut [Option<Line>; 4]) {
    let (ms, mx) = b.rect.middle();
    match b.kind {
        BlockKind::Park => {
            let mid = SIDEWALK + PARK_LOOP;
            for (k, (d, ccw)) in PARK_PATH.into_iter().enumerate() {
                let ring = Ring { rect: b.rect.inset(mid + d), r: 1.2 - d };
                let seed = mix(b.seed, 0x9A + k as u32, 0);
                out[k] = Some(Line::new(ring, ccw, 96, KERB, seed, 0.25, busy, false).at_corners());
            }
            // Round its middle only if no pavilion stands within reach of the ring.
            let clear = lots(b).as_slice().iter().all(|p| {
                let ds =
                    (ms - ms.clamp(p.foot.s0, p.foot.s1)).abs().max((p.foot.s0 - ms).max(ms - p.foot.s1));
                let dx =
                    (mx - mx.clamp(p.foot.x0, p.foot.x1)).abs().max((p.foot.x0 - mx).max(mx - p.foot.x1));
                sqrt(ds.max(0.0) * ds.max(0.0) + dx.max(0.0) * dx.max(0.0)) > PARK_ROUND[1].0 + 1.0
            });
            if clear {
                for (k, (r, ccw)) in PARK_ROUND.into_iter().enumerate() {
                    let seed = mix(b.seed, 0x9C + k as u32, 0);
                    out[2 + k] = Some(Line::new(circle(ms, mx, r), ccw, 108, KERB, seed, 0.3, busy, false));
                }
            }
        }
        BlockKind::Plaza => {
            for (k, (r, ccw)) in PLAZA_ROUND.into_iter().enumerate() {
                let seed = mix(b.seed, 0x9E + k as u32, 0);
                out[k] = Some(Line::new(circle(ms, mx, r), ccw, 96, KERB, seed, 0.3, busy, false));
            }
        }
        _ => {}
    }
}

/// People standing about a plaza's monument, looking up at it.
fn plaza_standers(
    b: &BlockInfo,
    busy: Busy,
    area: &Rect,
    c: Clock,
    f: &mut impl FnMut(&Walker) -> bool,
) -> bool {
    let (ms, mx) = b.rect.middle();
    let reach = lots(b).as_slice().first().map_or(5.0, |m| 0.5 * m.foot.width().max(m.foot.length()));
    let r = reach * core::f32::consts::SQRT_2 + 1.2;
    for k in 0..8u32 {
        let a = TAU * (k as f32 + 0.5) / 8.0;
        let (s, x) = (ms + r * sin(a), mx + r * cos(a));
        if !inside(area, s, x) {
            continue;
        }
        if let Some(w) = stander(s, x, KERB, yaw_of(-sin(a), -cos(a)), mix(b.seed, 0x7B, k), 0.3, busy, c)
            && f(&w)
        {
            return true;
        }
    }
    false
}

/// A quay's capsules, out from the water's edge: between its lamps (2.2 m out) and its trees (11 m),
/// and between its trees and its street's lamps (0.8 m in from the kerb, 28 m out).
const QUAY: [(f32, f32); 4] = [(3.6, 9.6), (4.8, 8.4), (13.6, 25.6), (14.8, 24.4)];
/// A capsule's ends this far in from the block's ends.
const QUAY_END: f32 = 1.0;
/// Somebody leaning on the railing, this far out from the water, every so far.
const AT_RAIL: f32 = 0.7;
const RAIL_GAP: f32 = 9.0;

/// A capsule along x between `s0 < s1`, its ends turned round points `outer` in from `x0` and `x1`
/// (so the ends of capsules about the same middle line, with the same `outer`, are concentric).
fn capsule(s0: f32, s1: f32, x0: f32, x1: f32, outer: f32) -> Ring {
    let r = 0.5 * (s1 - s0);
    Ring { rect: Rect::new(s0, s1, x0 + outer - r, x1 - outer + r), r }
}

fn quay_lines(b: &BlockInfo, busy: Busy, side: usize) -> [Line; 4] {
    let ch = channel(&b.rect);
    core::array::from_fn(|k| {
        let (y0, y1) = QUAY[k];
        let outer = 0.5 * (QUAY[k & 2].1 - QUAY[k & 2].0);
        let (s0, s1) = if side == 0 { (ch.s0 - y1, ch.s0 - y0) } else { (ch.s1 + y0, ch.s1 + y1) };
        let ring = capsule(s0, s1, b.rect.x0 + QUAY_END, b.rect.x1 - QUAY_END, outer);
        let seed = mix(b.seed, 0xC0 + side as u32, k as u32);
        Line::new(ring, true, 96, KERB, seed, 0.25, busy, false).at_corners()
    })
}

fn quay_standers(
    b: &BlockInfo,
    busy: Busy,
    area: &Rect,
    c: Clock,
    f: &mut impl FnMut(&Walker) -> bool,
) -> bool {
    let ch = channel(&b.rect);
    let r = b.rect;
    for (side, (s, face)) in [(ch.s0 - AT_RAIL, 1.0f32), (ch.s1 + AT_RAIL, -1.0)].into_iter().enumerate() {
        if s < area.s0 - 1.0 || s > area.s1 + 1.0 {
            continue;
        }
        let n = floor((r.length() - 12.0) / RAIL_GAP) as i32;
        for k in 0..=n {
            let x = r.x0 + 6.0 + RAIL_GAP * k as f32;
            if !inside(area, s, x) {
                continue;
            }
            let seed = mix(b.seed, 0xCA + side as u32, k as u32);
            if let Some(w) = stander(s, x, KERB, yaw_of(face, 0.0), seed, 0.22, busy, c)
                && f(&w)
            {
                return true;
            }
        }
    }
    false
}

fn by_block(b: &BlockInfo, area: &Rect, stage: Stage, c: Clock, f: &mut impl FnMut(&Walker) -> bool) -> bool {
    if !b.rect.overlaps(&Rect::new(area.s0 - 1.0, area.s1 + 1.0, area.x0 - 1.0, area.x1 + 1.0)) {
        return false;
    }
    let busy = busy_at(b.strip, b.bx, b.row, stage);
    match b.kind {
        BlockKind::Site => false,
        BlockKind::Canal => {
            for side in 0..2 {
                for l in quay_lines(b, busy, side) {
                    if l.each(area, c, |_| false, f) {
                        return true;
                    }
                }
            }
            quay_standers(b, busy, area, c, f)
        }
        _ => {
            // Round the block, or (on a run's row) down its sides on the run's narrow cross streets:
            // the run's own lines are `each_walker`'s.
            let mut lines = [None; 3];
            match run(b.row, b.bx.div_euclid(RUN), stage).filter(|_| runs_round(b.row)) {
                Some(span) => lines[..2].copy_from_slice(&side_lines(b, span, busy)),
                None => lines = pavement(b, busy).map(Some),
            }
            for l in lines.into_iter().flatten() {
                if l.each(area, c, |_| false, f) {
                    return true;
                }
            }
            let mut inner = [None; 4];
            inner_lines(b, busy, &mut inner);
            for l in inner.into_iter().flatten() {
                if l.each(area, c, |_| false, f) {
                    return true;
                }
            }
            if b.kind == BlockKind::Plaza && plaza_standers(b, busy, area, c, f) {
                return true;
            }
            pavement_standers(b, busy, area, c, f)
        }
    }
}

// ---- The avenue's walks and benches -------------------------------------------------------------

/// The avenue's pavements' capsules, out from its middle line: two in its first walk (between its
/// trees, 24.3 m out, and its beds, 31.5 m), two in its second (from the beds, 33.5 m, to the row's
/// kerb, 40 m, clear of The Arrival's benches 39 m out).
const AVENUE_WALKS: [(f32, f32); 4] = [(26.7, 31.1), (27.8, 30.0), (33.9, 37.6), (35.0, 36.5)];
/// Their ends this far in from the cross streets.
const AVENUE_END: f32 = 1.0;
/// Two seats a bench, this far either side of its middle; sitting down from a step in front.
const SEAT_AT: f32 = 0.45;
const SEAT_STAND: f32 = 0.55;
/// Sitting down (or getting up) takes this long, s.
const SIT_DOWN: f32 = 0.8;
/// Somebody sits two laps of their line when they do (one, if a day isn't an even number of them).
fn sit_laps(line: &Line) -> u32 {
    if (DAY_TICKS / (line.n * line.step)).is_multiple_of(2) { 2 } else { 1 }
}

/// The avenue's stretch between cross streets `bx` and `bx + 1`: where it starts, how long it is.
fn stretch(bx: i32) -> (f32, f32) {
    let x0 = grid_x(bx) + cross_width(bx) * 0.5;
    (x0, grid_x(bx + 1) - cross_width(bx + 1) * 0.5 - x0)
}

fn avenue_lines(strip: u8, bx: i32, side: f32, busy: Busy) -> [Line; 4] {
    let (x0, len) = stretch(bx);
    let mid = STRIP_WIDTH * 0.5;
    core::array::from_fn(|k| {
        let (a0, a1) = AVENUE_WALKS[k];
        let outer = 0.5 * (AVENUE_WALKS[k & 2].1 - AVENUE_WALKS[k & 2].0);
        let (s0, s1) = if side < 0.0 { (mid - a1, mid - a0) } else { (mid + a0, mid + a1) };
        let ring = capsule(s0, s1, x0 + AVENUE_END, x0 + len - AVENUE_END, outer);
        let seed = mix(u32::from(strip) + 1, bx as u32, 0xA70 + k as u32 + if side < 0.0 { 0 } else { 8 });
        Line::new(ring, true, 90, 0.0, seed, 0.3, busy, false).at_corners()
    })
}

/// A seat on one of the avenue's benches, and the slot of the line by it whose person sits there
/// now and then: its place, the way it faces, where they step off their line for it, the window
/// (of `sit_laps` laps) they keep, and which.
#[derive(Clone, Copy, Debug)]
struct Seat {
    s: f32,
    x: f32,
    face: f32,
    /// Where they leave their line for it, and where they stand to sit.
    off: (f32, f32),
    stand: (f32, f32),
    /// Its window starts at this step (in each window of `sit_laps` laps), and the slot it's for.
    phase: u32,
    j: u32,
    seed: u32,
}

/// The benches' seats down a stretch's pavement on `side` (`furniture::avenue`'s benches), each
/// with its sitter on the walk's inner line (`lines[0]`, `AVENUE_WALKS[0].0` out).
fn seats(strip: u8, bx: i32, side: f32, line: &Line, out: &mut impl FnMut(&Seat) -> bool) -> bool {
    let (x0, len) = stretch(bx);
    let mid = STRIP_WIDTH * 0.5;
    let last = floor((len - BENCH_END) / BENCH_PITCH) as i32;
    let lam = line.slot();
    // The inner line's way along x: +x on the +s pavement, −x on the −s one.
    let dx = side;
    let line_s = mid + side * AVENUE_WALKS[0].0;
    for jb in 1..=last {
        let xb = x0 + BENCH_PITCH * jb as f32;
        for q in 0..2u32 {
            let x = xb + if q == 0 { -SEAT_AT } else { SEAT_AT };
            let s = mid + side * AVENUE_TREE;
            let seed = mix(
                mix(u32::from(strip) + 1, bx as u32, 0x5EA + if side < 0.0 { 0 } else { 1 }),
                jb as u32,
                q,
            );
            // Off their line about 2 m before the seat, at a slot's place there.
            let u_want = wrap(line.ring.v_of(line_s, x - 2.0 * dx) - line.start, line.ring.len());
            let i = (floor(u_want / lam + 0.5) as u32) % line.n;
            let (os, ox, _, _) = line.ring.at(line.v(i as f32 * lam));
            // A slot of its own for each seat down the stretch, a bench's two half a lap apart (so
            // they never come and go at once); its window's start puts that slot here.
            let j = (jb as u32 - 1 + q * (line.n / 2)) % line.n;
            let phase = (i + line.n - j) % line.n;
            let seat =
                Seat { s, x, face: side, off: (os, ox), stand: (s + side * SEAT_STAND, x), phase, j, seed };
            if out(&seat) {
                return true;
            }
        }
    }
    false
}

impl Seat {
    /// Window `k` of its seat's line: whether its sitter sits it (they're out on every lap it
    /// touches, and their draw says so).
    fn sat(&self, line: &Line, k: i64) -> bool {
        if k < 0 {
            return false;
        }
        let window = u64::from(sit_laps(line) * line.n);
        let start = k as u64 * window + u64::from(self.phase);
        let windows = u64::from(DAY_TICKS) / (window * u64::from(line.step));
        let sits = unit(mix(self.seed, (k as u64 % windows.max(1)) as u32, 0x51), 0) < 0.45;
        // Every leg the window touches (its slot's back where it left at the end).
        let (leg0, _, _) = line.leg(self.j, start as u32, 0.0);
        let legs = i64::from(sit_laps(line)) * line.np as i64;
        sits && (leg0..=leg0 + legs).all(|l| line.out(self.j, l))
    }

    /// The window step `steps` is in.
    fn window(&self, line: &Line, steps: u32) -> i64 {
        let window = i64::from(sit_laps(line) * line.n);
        (i64::from(steps) - i64::from(self.phase)).div_euclid(window)
    }

    /// Its sitter this moment, if they're off their line for it.
    fn sitter(&self, line: &Line, c: Clock) -> Option<Walker> {
        let (steps, between) = c.slots(line.step);
        let k = self.window(line, steps);
        if !self.sat(line, k) {
            return None;
        }
        let window = i64::from(sit_laps(line) * line.n);
        let t0 = (k * window + i64::from(self.phase)) * i64::from(line.step);
        let tw = (window * i64::from(line.step)) as f32 / TICK_HZ as f32;
        let tau = c.since(t0);
        let v = line.speed();
        let walk = path_len(&[self.off, self.stand]) / v;
        let (before, after) = (self.sat(line, k - 1), self.sat(line, k + 1));
        let seat = (self.s, self.x);
        let base = line.walker(self.j, steps, between, 1.0);
        let sit = |at: (f32, f32)| Walker {
            s: at.0,
            x: at.1,
            yaw: yaw_of(self.face, 0.0),
            speed: 0.0,
            pose: Pose::Sit,
            stride: 0.0,
            ..base
        };
        let lerp = |a: (f32, f32), b: (f32, f32), u: f32| (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u);
        let moving = |from: (f32, f32), to: (f32, f32), d: f32| {
            let (at, dir) = along(&[from, to], d);
            Walker { s: at.0, x: at.1, yaw: yaw_of(dir.0, dir.1), speed: v, stride: fract(d / 1.5), ..base }
        };
        Some(if tau < walk + SIT_DOWN && !before {
            if tau < walk {
                moving(self.off, self.stand, tau * v)
            } else {
                sit(lerp(self.stand, seat, (tau - walk) / SIT_DOWN))
            }
        } else if tau > tw - walk - SIT_DOWN && !after {
            let back = tw - tau;
            if back < walk {
                moving(self.off, self.stand, back * v).turned()
            } else {
                sit(lerp(self.stand, seat, (back - walk) / SIT_DOWN))
            }
        } else {
            sit(seat)
        })
    }
}

impl Walker {
    /// The same, walking the other way.
    fn turned(self) -> Walker {
        Walker { yaw: self.yaw + PI, ..self }
    }
}

fn avenue(
    strip: u8,
    bx: i32,
    area: &Rect,
    stage: Stage,
    c: Clock,
    f: &mut impl FnMut(&Walker) -> bool,
) -> bool {
    if district_of(strip, bx, stage).is_none() {
        return false;
    }
    let busy = busy_at(strip, bx, 0, stage);
    let mid = STRIP_WIDTH * 0.5;
    for side in [-1.0f32, 1.0] {
        let (lo, hi) = (mid + side * AVENUE_WALKS[0].0 - 4.0, mid + side * (AVENUE * 0.5));
        if lo.max(hi) < area.s0 || lo.min(hi) > area.s1 {
            continue;
        }
        let lines = avenue_lines(strip, bx, side, busy);
        let line = lines[0];
        let (steps, _) = c.slots(line.step);
        // Who's sitting: their slots are off the line.
        let (mut gone, mut n) = ([u32::MAX; 16], 0);
        seats(strip, bx, side, &line, &mut |seat| {
            if seat.sat(&line, seat.window(&line, steps)) {
                gone[n] = seat.j;
                n += 1;
            }
            n == gone.len()
        });
        if line.each(area, c, |j| gone[..n].contains(&j), f) {
            return true;
        }
        for l in &lines[1..] {
            if l.each(area, c, |_| false, f) {
                return true;
            }
        }
        let mut stop = false;
        seats(strip, bx, side, &line, &mut |seat| {
            if let Some(w) = seat.sitter(&line, c)
                && inside(area, w.s, w.x)
            {
                stop = f(&w);
            }
            stop
        });
        if stop {
            return true;
        }
    }
    false
}

// ---- The window banks' promenades ---------------------------------------------------------------

/// A bank's promenade's capsules, in from the glass (its railing 0.3 m deep): the joggers' round
/// the outside, 8 blocks (1 km) long; the strollers' inside it, 2 blocks long (so they turn back
/// often, and come and go at their ends at the hour). Their ends turn round the same points as
/// the joggers' (concentric where they meet), this far in from their stretch's.
const BANK_JOG: (f32, f32) = (1.5, 11.1);
const BANK_WALKS: [(f32, f32); 3] = [(2.7, 9.9), (3.9, 8.7), (5.1, 7.5)];
const BANK_JOG_BLOCKS: i32 = 8;
const BANK_BLOCKS: i32 = 2;
const BANK_END: f32 = 1.0;

fn bank(strip: u8, area: &Rect, stage: Stage, c: Clock, f: &mut impl FnMut(&Walker) -> bool) -> bool {
    let reach = BANK_JOG.1 + 1.0;
    let outer = 0.5 * (BANK_JOG.1 - BANK_JOG.0);
    let sides = [(0.0f32, 1.0f32), (STRIP_WIDTH, -1.0)];
    for (k, (glass, inward)) in sides.into_iter().enumerate() {
        let (lo, hi) = (glass.min(glass + inward * reach), glass.max(glass + inward * reach));
        if hi < area.s0 || lo > area.s1 {
            continue;
        }
        for (blocks, lines) in [(BANK_JOG_BLOCKS, &[BANK_JOG][..]), (BANK_BLOCKS, &BANK_WALKS[..])] {
            let run = blocks == BANK_JOG_BLOCKS;
            let first = (block_index(area.x0) - HUB_GATE.0).div_euclid(blocks).max(0);
            let last = (block_index(area.x1) - HUB_GATE.0).div_euclid(blocks);
            for seg in first..=last {
                let (b0, b1) = (HUB_GATE.0 + seg * blocks, HUB_GATE.0 + (seg + 1) * blocks);
                // Only where the city's built, all the way along.
                if b1 - 1 > SITE.1 || district_of(strip, b1 - 1, stage).is_none() {
                    continue;
                }
                let (x0, x1) = (grid_x(b0) + BANK_END, grid_x(b1) - BANK_END);
                for (n, &(e0, e1)) in lines.iter().enumerate() {
                    let (s0, s1) = if inward > 0.0 { (e0, e1) } else { (STRIP_WIDTH - e1, STRIP_WIDTH - e0) };
                    let ring = capsule(s0, s1, x0, x1, outer);
                    let seed = mix(
                        u32::from(strip) + 1,
                        seg as u32,
                        0xBA0 + 8 * k as u32 + 4 * run as u32 + n as u32,
                    );
                    let (step, share) = if run { (45, 0.1) } else { (90, 0.16) };
                    let line = Line::new(ring, true, step, 0.0, seed, share, Busy::BANK, run).at_corners();
                    if line.each(area, c, |_| false, f) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

// ---- Hub Gate's square --------------------------------------------------------------------------

/// The square's paving's dark bands (`paint_square`): every 32 m, 16 m off the cap and off the
/// avenue's middle line. People walk them both ways, a line 2 m either side of each band, and turn
/// back round its middle at their ends.
const BAND: f32 = 32.0;
const BAND_LINE: f32 = 2.0;
/// Bands along x out to the 8th from the avenue's middle line (240 m); bands across from the 3rd
/// from the cap (the terminal's door stands before them) to the 19th; from the 15th on they cross
/// by the station, split either side of the tram.
const BANDS_ALONG: i32 = 8;
const BANDS_ACROSS: (i32, i32) = (2, 18);
const BANDS_SPLIT: i32 = 14;
/// A slot every 4 m, on every 80 ticks: 1.5 m/s, a day 4 laps of a long capsule.
const SQUARE_SLOT: f32 = 4.0;
const SQUARE_STEP: u32 = 80;
/// Slots round a capsule the square's length or width, and round half its width.
const SQUARE_LONG: u32 = 270;
const SQUARE_HALF: u32 = 135;
/// Where the square's capsules may reach: along x from just before the terminal's door (60 m out
/// from the cap) to short of the cross street (bx 8); across, short of the streets round it (283 m
/// out) and of the tram past the station.
const SQUARE_X0: f32 = -15_937.5;
const TRAM_CLEAR: f32 = 9.0;
/// Where the square's people cross each other, mod a slot: along x on the across bands' lines,
/// and across on the along bands' lines (both 2 m off a band, 16 m off the grid).
const LATTICE: f32 = 2.0;

fn square_x0() -> f32 {
    grid_x(HUB_GATE.0)
}

/// The start (low end) of a lattice capsule's straights at or past `lo`: so that its people stand
/// on the lattice's phase (`at` mod a slot) both ways round, its straights `a` long and its ends'
/// radius `r` (2·start + 2a + πr ≡ 2·at, mod a slot).
fn lattice_start(lo: f32, a: f32, r: f32, at: f32) -> f32 {
    let half = 0.5 * SQUARE_SLOT;
    lo + wrap(at - a - 0.5 * PI * r - lo, half)
}

/// A capsule of the square's lattice: along x (`along`) on the band at `c` (from the avenue's
/// middle line), or across on the band at `xb`, its straights from `lo` (or past it) round
/// `slots` slots, its people `at` mod a slot when the clock's on a slot.
fn lattice(along: bool, band: f32, lo: f32, slots: u32, seed: u32) -> Line {
    let r = BAND_LINE;
    let len = slots as f32 * SQUARE_SLOT;
    let a = 0.5 * len - PI * r;
    let mid = STRIP_WIDTH * 0.5;
    // Along x people cross the across bands' lines, which stand at LATTICE mod a slot; across, half
    // a slot off the along bands' lines.
    let at = if along { LATTICE } else { LATTICE + 0.5 * SQUARE_SLOT };
    let p = lattice_start(lo, a, r, at);
    let (ring, u_plus) = if along {
        (Ring { rect: Rect::new(mid + band - r, mid + band + r, p - r, p + a + r), r }, 0.0)
    } else {
        (Ring { rect: Rect::new(mid + p - r, mid + p + a + r, band - r, band + r), r }, 0.5 * PI * r)
    };
    let mut l = Line {
        ring,
        ccw: true,
        n: slots,
        step: SQUARE_STEP,
        start: 0.0,
        portals: [0.0; 4],
        np: 1,
        h: 0.0,
        seed,
        share: 0.28,
        busy: Busy::HUB,
        run: false,
        streets: false,
    };
    // On its + straight a slot's place is p + (start + u − u+): put that on `at`.
    l.start = wrap(at - p + u_plus, SQUARE_SLOT);
    l.at_corners()
}

/// Calls `g` with the square's capsules.
fn square_lines(strip: u8, mut g: impl FnMut(Line) -> bool) -> bool {
    let x0 = square_x0();
    let seed = |k: u32| mix(u32::from(strip) + 1, k, 0x5C0);
    for k in 0..BANDS_ALONG {
        for side in [-1.0f32, 1.0] {
            let c = side * (BAND * 0.5 + BAND * k as f32);
            if g(lattice(
                true,
                c,
                SQUARE_X0 + BAND_LINE,
                SQUARE_LONG,
                seed(k as u32 * 2 + (side > 0.0) as u32),
            )) {
                return true;
            }
        }
    }
    for j in BANDS_ACROSS.0..=BANDS_ACROSS.1 {
        let xb = x0 + BAND * 0.5 + BAND * j as f32;
        let id = 100 + j as u32 * 4;
        if j < BANDS_SPLIT {
            // Across the whole square, about its middle line.
            let a = 0.5 * SQUARE_LONG as f32 * SQUARE_SLOT - PI * BAND_LINE;
            if g(lattice(false, xb, -0.5 * a - 1.0, SQUARE_LONG, seed(id))) {
                return true;
            }
        } else {
            let near = TRAM_CLEAR + BAND_LINE;
            let a = 0.5 * SQUARE_HALF as f32 * SQUARE_SLOT - PI * BAND_LINE;
            if g(lattice(false, xb, near, SQUARE_HALF, seed(id + 1))) {
                return true;
            }
            if g(lattice(false, xb, -(near + a + SQUARE_SLOT * 0.5), SQUARE_HALF, seed(id + 2))) {
                return true;
            }
        }
    }
    false
}

/// People standing in twos, threes and fours in the middle of the square's cells, between bands.
fn square_groups(strip: u8, area: &Rect, c: Clock, f: &mut impl FnMut(&Walker) -> bool) -> bool {
    let (x0, mid) = (square_x0(), STRIP_WIDTH * 0.5);
    for j in BANDS_ACROSS.0 + 1..=BANDS_ACROSS.1 {
        let x = x0 + BAND * j as f32;
        if x < area.x0 - 2.0 || x > area.x1 + 2.0 {
            continue;
        }
        for k in -BANDS_ALONG..=BANDS_ALONG {
            if k == 0 {
                continue;
            }
            let s = mid + BAND * k as f32;
            if s < area.s0 - 2.0 || s > area.s1 + 2.0 {
                continue;
            }
            let seed = mix(u32::from(strip) + 1, j as u32, (k + 64) as u32 ^ 0x6A0);
            let n = 2 + (unit(seed, 3) * 3.0) as u32;
            let turn = TAU * unit(seed, 4);
            for m in 0..n {
                let a = turn + TAU * m as f32 / n as f32;
                let (ps, px) = (s + 0.75 * sin(a), x + 0.75 * cos(a));
                if !inside(area, ps, px) {
                    continue;
                }
                if let Some(w) =
                    stander(ps, px, 0.0, yaw_of(-sin(a), -cos(a)), mix(seed, m, 0), 0.3, Busy::HUB, c)
                    && f(&w)
                {
                    return true;
                }
            }
        }
    }
    false
}

fn square(strip: u8, area: &Rect, c: Clock, f: &mut impl FnMut(&Walker) -> bool) -> bool {
    let mid = STRIP_WIDTH * 0.5;
    let all = Rect::new(mid - 296.0, mid + 296.0, square_x0(), grid_x(CITY.0));
    if !all.overlaps(area) {
        return false;
    }
    square_lines(strip, |l| l.each(area, c, |_| false, f)) || square_groups(strip, area, c, f)
}

// ---- The platforms ------------------------------------------------------------------------------

/// A platform's people, out from its middle line on their train's side: the queues at its doors (a
/// file either side of each, two deep), the way off it, and the way on.
const QUEUE: [f32; 2] = [2.35, 1.7];
const QUEUE_BESIDE: f32 = 1.1;
const AISLE_OFF: f32 = 1.0;
const AISLE_ON: f32 = 0.35;
/// Through a door this far either side of its middle, into the car this far out (hidden there).
const BOARD_BESIDE: f32 = 0.45;
const IN_CAR: f32 = 3.7;
/// They come and go this far past the foot of the steps, and walk the platform at this speed.
const STAIRS_FOOT: f32 = 1.2;
const PLATFORM_WALK: f32 = 1.3;
/// The queue's front and back get on this long after the doors open, s; those getting off come out
/// from this long after, this far apart; people come up the steps for the next train from this long
/// after the doors open, and are all in their places this long before the next's open.
const BOARD_FRONT: f32 = 5.5;
const BOARD_BACK: f32 = 7.0;
const ALIGHT_FIRST: f32 = 0.3;
const ALIGHT_GAP: f32 = 1.0;
const COME_FROM: f32 = 40.0;
const COME_TILL: f32 = 8.0;
/// The longest walk to a place in a queue (from the foot of the steps to the innermost door's
/// inner file, then across to the front row), s: the last come up the steps this long sooner.
const WALK_MOST: f32 =
    (0.5 * PLATFORM_LENGTH + STAIRS_FOOT - (DOOR_AT - QUEUE_BESIDE) + QUEUE[0] - AISLE_ON) / PLATFORM_WALK;
/// Headways in a week of colony days.
const WEEK_CYCLES: i64 = (7 * DAY_TICKS / HEADWAY_TICKS) as i64;
const _: () = assert!((7 * DAY_TICKS).is_multiple_of(HEADWAY_TICKS));
/// The most in a queue, and off at a door.
const QUEUED: u32 = 12;
const ALIGHTING: u32 = 3;

/// The ground under somebody on a platform at `x` (its top, a step, or the floor past its end):
/// the highest under their feet.
fn platform_h(x: f32) -> f32 {
    platform_top(x - RADIUS).unwrap_or(0.0).max(platform_top(x + RADIUS).unwrap_or(0.0))
}

fn platform(
    strip: u8,
    i: usize,
    area: &Rect,
    c: Clock,
    stage: Stage,
    f: &mut impl FnMut(&Walker) -> bool,
) -> bool {
    let (xs, mid) = (station_x(i), STRIP_WIDTH * 0.5);
    let reach = 0.5 * PLATFORM_LENGTH + STAIRS_FOOT + 1.0;
    if !Rect::new(mid - IN_CAR - 0.5, mid + IN_CAR + 0.5, xs - reach, xs + reach).overlaps(area) {
        return false;
    }
    let busy = busy_at(strip, block_index(xs), 0, stage).times(1.5);
    let h_s = HEADWAY_TICKS as f32 / TICK_HZ as f32;
    for dir in [1.0f32, -1.0] {
        let Some(at) = stands(strip, i, dir) else { continue };
        let open = (at + DOOR_MARGIN) % HEADWAY_TICKS;
        // The doors opened last at the start of cycle `m`.
        let m = (i64::from(c.t) - i64::from(open)).div_euclid(i64::from(HEADWAY_TICKS));
        let t_open = |m: i64| i64::from(open) + m * i64::from(HEADWAY_TICKS);
        let psi = c.since(t_open(m));
        // Who's on it each cycle, again each week (a whole number of days and headways).
        let seed = |m: i64, k: u32| {
            mix(
                mix(u32::from(strip) + 1, i as u32, 0x9A7 + (dir > 0.0) as u32),
                m.rem_euclid(WEEK_CYCLES) as u32,
                k,
            )
        };
        let hour = |m: i64| busy.at(&day((t_open(m).max(0) as u64 % u64::from(DAY_TICKS)) as u32, 0.0));
        let queued = |m: i64, half: u32| {
            let k = QUEUED as f32 * (hour(m) * (0.4 + 0.9 * unit(seed(m, half), 7))).min(1.0);
            floor(k + 0.5) as u32
        };
        let walker =
            |id: u32, s: f32, x: f32, dir2: (f32, f32), speed: f32, pose: Pose, fade: f32, d: f32| Walker {
                id,
                s: mid + s,
                x,
                h: platform_h(x),
                yaw: yaw_of(dir2.0, dir2.1),
                speed,
                pose,
                stride: fract(d / 1.5),
                fade: fade.clamp(0.0, 1.0),
                seed: mix(id, 0x5EED, 2),
            };
        for (half, end) in [(0u32, -1.0f32), (1, 1.0)] {
            let doors: [f32; 3] = core::array::from_fn(|n| {
                let k = if end < 0.0 { n } else { 5 - n };
                car_offset(k / 2) + if k % 2 == 0 { -DOOR_AT } else { DOOR_AT }
            });
            let foot = xs + end * (0.5 * PLATFORM_LENGTH + STAIRS_FOOT);
            // The queue: those waiting for this cycle's train (m + 1), and this one's getting on.
            for (cycle, rel) in [(m + 1, psi), (m, psi + h_s)] {
                let n = queued(cycle, half);
                for k in 0..n {
                    let (row, door, file) =
                        ((k / 6) as usize, doors[((k % 6) / 2) as usize], (k % 2) as f32 * 2.0 - 1.0);
                    let x_q = xs + door + file * QUEUE_BESIDE;
                    let id = seed(cycle, 0x100 + half * 16 + k);
                    // Up the steps from the foot, along the way on, across to their place.
                    let come = COME_FROM
                        + (h_s - COME_FROM - COME_TILL - WALK_MOST) * (k as f32 + 0.15 + 0.7 * unit(id, 1))
                            / QUEUED as f32;
                    let to = [(dir * AISLE_ON, foot), (dir * AISLE_ON, x_q), (dir * QUEUE[row], x_q)];
                    let walk = path_len(&to) / PLATFORM_WALK;
                    // Getting on: the back steps up to the front, along to the door's line (clear of
                    // the screens and its frame), then straight in.
                    let board = h_s + if row == 0 { BOARD_FRONT } else { BOARD_BACK };
                    let x_d = xs + door + file * BOARD_BESIDE;
                    let on = [
                        (dir * QUEUE[row], x_q),
                        (dir * QUEUE[0], x_q),
                        (dir * QUEUE[0], x_d),
                        (dir * IN_CAR, x_d),
                    ];
                    let on_len = path_len(&on);
                    let w = if rel < come {
                        continue;
                    } else if rel < come + walk {
                        let d = (rel - come) * PLATFORM_WALK;
                        let ((s, x), dv) = along(&to, d);
                        walker(id, s, x, dv, PLATFORM_WALK, Pose::Walk, d / FADE, d)
                    } else if rel < board {
                        let (s, x) = to[2];
                        walker(id, s, x, (dir, 0.0), 0.0, Pose::Stand, 1.0, 0.0)
                    } else if rel < board + on_len / PLATFORM_WALK {
                        let d = (rel - board) * PLATFORM_WALK;
                        let ((s, x), dv) = along(&on, d);
                        let fade = (IN_CAR - s.abs()) / (IN_CAR - PLATFORM_HALF - 0.2);
                        walker(id, s, x, dv, PLATFORM_WALK, Pose::Walk, fade, d)
                    } else {
                        continue;
                    };
                    if w.fade > 0.0 && inside(area, w.s, w.x) && f(&w) {
                        return true;
                    }
                }
            }
            // Getting off this cycle's train: out of its doors, along the way off, down the steps.
            for (n, door) in doors.into_iter().enumerate() {
                let off =
                    (ALIGHTING as f32 * hour(m) * unit(seed(m, 0x200 + half * 4 + n as u32), 3) * 1.4) as u32;
                for q in 0..off.min(ALIGHTING) {
                    let id = seed(m, 0x300 + half * 16 + n as u32 * 4 + q);
                    let x_d = xs + door;
                    let path = [(dir * IN_CAR, x_d), (dir * AISLE_OFF, x_d), (dir * AISLE_OFF, foot)];
                    let d = (psi - ALIGHT_FIRST - ALIGHT_GAP * q as f32) * PLATFORM_WALK;
                    let len = path_len(&path);
                    if d < 0.0 || d > len {
                        continue;
                    }
                    let ((s, x), dv) = along(&path, d);
                    let fade = ((IN_CAR - s.abs()) / (IN_CAR - PLATFORM_HALF - 0.2)).min((len - d) / FADE);
                    let w = walker(id, s, x, dv, PLATFORM_WALK, Pose::Walk, fade, d);
                    if w.fade > 0.0 && inside(area, w.s, w.x) && f(&w) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

// ---- Everyone -----------------------------------------------------------------------------------

/// The city's people in `area` of strip `strip` at tick `t` plus `frac` of the next (the building
/// site built out to `stage`), for anything that wants them one at a time: calls `f` with each whose
/// place is in `area` (from its low edges, short of its high ones); stops early when it returns
/// true, and says whether it did. The same on every machine, to the bit.
pub fn each_walker(
    strip: u8,
    area: &Rect,
    stage: Stage,
    t: u32,
    frac: f32,
    mut f: impl FnMut(&Walker) -> bool,
) -> bool {
    // A tick's end is the next one's start, to the bit.
    let c = if frac >= 1.0 {
        Clock { t: t.wrapping_add(1), frac: 0.0 }
    } else {
        Clock { t, frac: frac.max(0.0) }
    };
    if area.s1 < 0.0 || area.s0 > STRIP_WIDTH || area.x1 < square_x0() || area.x0 > grid_x(SITE.1 + 1) {
        return false;
    }
    let f = &mut f;
    let b0 = block_index(area.x0).max(HUB_GATE.0);
    let b1 = block_index(area.x1).min(SITE.1);
    let (r0, r1) = (row_at(area.s0), row_at(area.s1));
    for bx in b0..=b1 {
        for row in r0..=r1 {
            let (s0, s1) = row_span(row);
            if s1 < area.s0 || s0 > area.s1 {
                continue;
            }
            let done = match block(strip, bx, row, stage) {
                Some(b) => by_block(&b, area, stage, c, f),
                None if row == 0 && bx >= CITY.0 => avenue(strip, bx, area, stage, c, f),
                None => false,
            };
            if done {
                return true;
            }
        }
    }
    // Round the runs of blocks, over their narrow cross streets.
    for row in r0..=r1 {
        let (s0, s1) = row_span(row);
        if !runs_round(row) || s1 < area.s0 || s0 > area.s1 {
            continue;
        }
        for g in b0.div_euclid(RUN)..=b1.div_euclid(RUN) {
            let Some(span) = run(row, g, stage) else { continue };
            let busy = busy_at(strip, (span.0 + span.1) / 2, row, stage);
            for l in run_lines(strip, row, span, busy) {
                if l.each(area, c, |_| false, f) {
                    return true;
                }
            }
        }
    }
    if bank(strip, area, stage, c, f) || square(strip, area, c, f) {
        return true;
    }
    let near = (((area.x0 + area.x1) * 0.5 - station_x(0)) / (station_x(1) - station_x(0)) + 0.5) as i32;
    let span = ((area.x1 - area.x0) / (station_x(1) - station_x(0))) as i32 + 1;
    for i in (near - span).max(0)..=(near + span).min(STATIONS as i32 - 1) {
        if platform(strip, i as usize, area, c, stage, f) {
            return true;
        }
    }
    false
}

/// What a sitter's body is, for what it mustn't touch: over the bench's slab, and their shins in
/// front of it (`(s, x)` the seat, facing `face` across). Boxes in city coordinates.
pub fn sitting(w: &Walker) -> [CityBox; 2] {
    let face = if sin(w.yaw).abs() > 0.7 { 0.0 } else { -cos(w.yaw).signum() };
    let seat = CityBox {
        rect: Rect::new(w.s - 0.2, w.s + 0.2, w.x - 0.22, w.x + 0.22),
        h0: w.h + BENCH_HEIGHT + 0.01,
        h1: w.h + 1.3,
    };
    let (a, b) = (w.s + face * 0.3, w.s + face * 0.75);
    let legs = CityBox {
        rect: Rect::new(a.min(b), a.max(b), w.x - 0.22, w.x + 0.22),
        h0: w.h + 0.02,
        h1: w.h + 0.45,
    };
    [seat, legs]
}

// What keeps the lines clear, by the numbers (the tests walk every line to make sure).
const _: () = {
    // A pavement's lines between its lamp posts (0.8 m in, 0.12 m thick) and its walls (5 m in).
    assert!(PAVEMENT[0].0 - RADIUS > LAMP_IN + 0.12 + 0.1 && PAVEMENT[2].0 + RADIUS < SIDEWALK);
    assert!(PAVEMENT[1].0 - PAVEMENT[0].0 >= 0.9 && PAVEMENT[2].0 - PAVEMENT[1].0 >= 0.9);
    // Standers a body's width off the lines.
    assert!(BY_WALL - PAVEMENT[2].0 >= 2.0 * RADIUS && PAVEMENT[0].0 - AT_KERB >= 2.0 * RADIUS);
    // The avenue's first walk clear of the benches' sitters' shins (0.75 m out) and the trees.
    assert!(AVENUE_WALKS[0].0 - RADIUS > AVENUE_TREE + 0.75 + 0.3);
    // A plaza's rings either side of its lamps' ring.
    assert!(PLAZA_ROUND[1].0 + RADIUS < PLAZA_RING - 0.5 && PLAZA_ROUND[2].0 - RADIUS > PLAZA_RING + 0.5);
    // A platform's queues on it, clear of the cars (half their width out from each track).
    assert!(QUEUE[0] + RADIUS < PLATFORM_HALF && PLATFORM_HALF < TRACK_OFFSET - 0.5 * CAR_WIDTH);
    assert!(QUEUE[1] - AISLE_OFF >= 2.0 * RADIUS && AISLE_OFF - AISLE_ON >= 2.0 * RADIUS);
    // Getting on two abreast through a door's gap, lined up with it before the screens.
    assert!(BOARD_BESIDE >= RADIUS && BOARD_BESIDE + RADIUS <= 0.5 * DOOR_WIDTH);
    // The last to come up the steps in their places before the doors open.
    let h_s = HEADWAY_TICKS as f32 / TICK_HZ as f32;
    assert!(h_s - COME_TILL - WALK_MOST > COME_FROM);
    // The square's capsules a whole number of laps a day.
    assert!(
        DAY_TICKS.is_multiple_of(SQUARE_LONG * SQUARE_STEP)
            && DAY_TICKS.is_multiple_of(SQUARE_HALF * SQUARE_STEP)
    );
};

// The tests gather every slot of whole lines to check them against each other: off the tick's path.
#[cfg(test)]
#[allow(clippy::disallowed_types, clippy::disallowed_methods)]
mod tests {
    use super::*;

    use crate::colony::city::{ROWS, solid};
    use glam::Vec3;

    extern crate std;
    use std::vec::Vec;

    /// Every slot of a line, whether anybody's in it or not: where at step `steps` plus `between`.
    fn all(l: &Line, steps: u32, between: f32) -> Vec<Walker> {
        (0..l.n).map(|j| l.walker(j, steps, between, 1.0)).collect()
    }

    fn standing_in_something(strip: u8, s: f32, x: f32, h: f32) -> bool {
        solid(
            strip,
            Vec3::new(x - RADIUS, h, -s - RADIUS),
            Vec3::new(x + RADIUS, h + 1.8, -s + RADIUS),
            Stage(0),
        )
    }

    /// Walks a line's ring every `d` metres: nothing solid on it, for a person.
    fn clear(strip: u8, l: &Line, d: f32) -> Option<(f32, f32)> {
        let len = l.ring.len();
        let n = (len / d) as u32;
        (0..n)
            .map(|k| l.ring.at(len * k as f32 / n as f32))
            .find(|p| standing_in_something(strip, p.0, p.1, l.h_at(p.1)))
            .map(|p| (p.0, p.1))
    }

    #[test]
    fn the_squares_people_cross_each_others_paths_half_a_slot_apart() {
        let mut lines = Vec::new();
        square_lines(1, |l| {
            lines.push(l);
            false
        });
        assert_eq!(
            lines.len(),
            2 * BANDS_ALONG as usize
                + (BANDS_SPLIT - BANDS_ACROSS.0) as usize
                + 2 * (BANDS_ACROSS.1 - BANDS_SPLIT + 1) as usize
        );
        let mut nearest = f32::MAX;
        // A slot's worth of the clock, every slot full.
        for k in 0..40u32 {
            let t = 1_000_000 + k * 2;
            let c = Clock { t, frac: 0.37 };
            let people: Vec<(usize, Walker)> = lines
                .iter()
                .enumerate()
                .flat_map(|(i, l)| {
                    let (steps, between) = c.slots(l.step);
                    all(l, steps, between).into_iter().map(move |w| (i, w))
                })
                .collect();
            // Bucket by 8 m cells so the pairs are few.
            let mut cells: std::collections::HashMap<(i32, i32), Vec<usize>> =
                std::collections::HashMap::new();
            for (n, (_, w)) in people.iter().enumerate() {
                cells.entry(((w.s / 8.0) as i32, (w.x / 8.0) as i32)).or_default().push(n);
            }
            for (n, (i, w)) in people.iter().enumerate() {
                let (cs, cx) = ((w.s / 8.0) as i32, (w.x / 8.0) as i32);
                for ds in -1..=1 {
                    for dx in -1..=1 {
                        for &m in cells.get(&(cs + ds, cx + dx)).map_or(&[][..], |v| &v[..]) {
                            if m <= n {
                                continue;
                            }
                            let (_, o) = people[m];
                            let d = sqrt((w.s - o.s) * (w.s - o.s) + (w.x - o.x) * (w.x - o.x));
                            if people[m].0 != *i {
                                nearest = nearest.min(d);
                            }
                            assert!(d >= 2.0 * RADIUS, "two people {d} m apart at {t}: {w:?} {o:?}");
                        }
                    }
                }
            }
        }
        assert!(nearest > 1.3, "people on crossing lines come {nearest} m apart");
    }

    #[test]
    fn every_line_is_clear_of_everything_solid() {
        let stage = Stage(0);
        for strip in 0..3u8 {
            // Every block's lines down a long stretch, Hub Gate's to the building site's.
            for bx in (HUB_GATE.0..=CITY.1).step_by(if strip == 0 { 1 } else { 3 }) {
                for row in -ROWS..=ROWS {
                    let Some(b) = block(strip, bx, row, stage) else { continue };
                    let busy = busy_at(strip, bx, row, stage);
                    let mut lines: Vec<Line> = Vec::new();
                    match b.kind {
                        BlockKind::Site => {}
                        BlockKind::Canal => {
                            lines.extend(quay_lines(&b, busy, 0));
                            lines.extend(quay_lines(&b, busy, 1));
                        }
                        _ => {
                            match run(row, bx.div_euclid(RUN), stage).filter(|_| runs_round(row)) {
                                Some(span) => lines.extend(side_lines(&b, span, busy).into_iter().flatten()),
                                None => lines.extend(pavement(&b, busy)),
                            }
                            let mut inner = [None; 4];
                            inner_lines(&b, busy, &mut inner);
                            lines.extend(inner.into_iter().flatten());
                        }
                    }
                    for l in &lines {
                        if let Some(p) = clear(strip, l, 0.25) {
                            panic!(
                                "strip {strip} block ({bx}, {row}) {:?}: a line through something at {p:?}",
                                b.kind
                            );
                        }
                    }
                }
                // The runs of blocks, over their narrow cross streets.
                for row in -ROWS..=ROWS {
                    if !runs_round(row) || bx.rem_euclid(RUN) != 0 && bx != HUB_GATE.0 {
                        continue;
                    }
                    let Some(span) = run(row, bx.div_euclid(RUN), stage) else { continue };
                    for l in run_lines(strip, row, span, Busy::HUB) {
                        if let Some(p) = clear(strip, &l, 0.25) {
                            panic!(
                                "strip {strip} the run ({span:?}, {row}): a line through something at {p:?}"
                            );
                        }
                    }
                }
                if bx >= CITY.0 {
                    for side in [-1.0f32, 1.0] {
                        for l in avenue_lines(strip, bx, side, Busy::HUB) {
                            if let Some(p) = clear(strip, &l, 0.25) {
                                panic!("strip {strip} the avenue at {bx}: a line through something at {p:?}");
                            }
                        }
                    }
                }
            }
            square_lines(strip, |l| {
                assert!(clear(strip, &l, 0.25).is_none(), "the square: {:?}", clear(strip, &l, 0.25));
                false
            });
        }
    }

    #[test]
    fn lines_keep_out_of_each_others_reach() {
        // Nested lines are a metre or more apart everywhere; parallel ones the same.
        let b = block(0, 20, 3, Stage(0)).unwrap();
        let p = pavement(&b, Busy::HUB);
        for k in 0..2 {
            for i in 0..400 {
                let (s, x, _, _) = p[k].ring.at(p[k].ring.len() * i as f32 / 400.0);
                let near = (0..2000)
                    .map(|m| p[k + 1].ring.at(p[k + 1].ring.len() * m as f32 / 2000.0))
                    .map(|q| sqrt((q.0 - s) * (q.0 - s) + (q.1 - x) * (q.1 - x)))
                    .fold(f32::MAX, f32::min);
                assert!(near > 0.95, "lines {k} and {} come {near} m apart", k + 1);
            }
        }
    }

    #[test]
    fn people_come_and_go_at_corners_never_on_a_crossing() {
        let stage = Stage(0);
        for row in [-7, -2, 3, 6, 12] {
            for g in [2, 10, 31, 49] {
                let Some(span) = run(row, g, stage) else { continue };
                for l in run_lines(1, row, span, Busy::HUB) {
                    assert_eq!(l.np, 4);
                    for &u in &l.portals[..l.np] {
                        let (s, x, ds, dx) = l.ring.at(l.v(u));
                        // On a corner's arc (heading neither straight along nor across), on a block.
                        assert!(ds.abs() > 0.5 && dx.abs() > 0.5, "a portal off a corner: {s} {x}");
                        assert_eq!(l.h_at(x), KERB, "a portal on a street at {x}");
                    }
                }
            }
        }
        // A capsule's at its two ends.
        let l = avenue_lines(0, 40, 1.0, Busy::HUB)[0];
        assert_eq!(l.np, 2);
        for &u in &l.portals[..2] {
            let (_, _, ds, dx) = l.ring.at(l.v(u));
            assert!(ds.abs() > 0.99 && dx.abs() < 0.01, "an end's middle heads across");
        }
    }

    #[test]
    fn a_day_is_whole_laps_of_every_line() {
        for len in [10.0f32, 47.0, 75.4, 200.0, 412.0, 1_000.0, 2_040.0] {
            for step in [45u32, 80, 90, 96, 108] {
                let n = fit(len, step);
                assert_eq!(DAY_TICKS % (n * step), 0, "{len} {step}");
                let lam = len / n as f32;
                assert!(lam >= 2.0 && (lam - SLOT).abs() < 1.6, "{len} m at {step}: {n} slots of {lam}");
            }
        }
    }

    #[test]
    fn a_ring_is_continuous_and_its_spans_find_every_point() {
        let rings = [
            Ring { rect: Rect::new(10.0, 110.0, -50.0, 54.0), r: 3.0 },
            Ring { rect: Rect::new(0.0, 4.0, 0.0, 300.0), r: 2.0 },
            circle(5.0, 5.0, 11.0),
            Ring { rect: Rect::new(-200.0, 200.0, 7.0, 11.0), r: 2.0 },
        ];
        for ring in rings {
            let len = ring.len();
            let mut last = ring.at(0.0);
            for k in 1..=4000 {
                let v = len * k as f32 / 4000.0;
                let p = ring.at(v.min(len - 1e-4));
                let step = sqrt((p.0 - last.0) * (p.0 - last.0) + (p.1 - last.1) * (p.1 - last.1));
                assert!(step <= len / 4000.0 * 1.01 + 1e-3, "a jump at {v}: {step}");
                // The way on points from the last point to this one.
                if step > 1e-4 {
                    let along = ((p.0 - last.0) * p.2 + (p.1 - last.1) * p.3) / step;
                    assert!(along > 0.98, "{v}: {along}");
                }
                last = p;
            }
            let p0 = ring.at(0.0);
            assert!((last.0 - p0.0).abs() < 1e-2 && (last.1 - p0.1).abs() < 1e-2, "closed");
            // Every point is in the span its area's spans give it.
            for k in 0..500 {
                let v = len * (k as f32 + 0.37) / 500.0;
                let (s, x, _, _) = ring.at(v);
                let area = Rect::new(s - 0.5, s + 0.5, x - 0.5, x + 0.5);
                let mut found = false;
                ring.spans(&area, |lo, hi, from, to| {
                    found |= from <= v && v < to && lo - 1e-3 <= v && v <= hi + 1e-3;
                    false
                });
                assert!(found, "{v} not found");
            }
        }
    }

    #[test]
    fn a_rings_straights_give_their_points_back() {
        let ring = Ring { rect: Rect::new(10.0, 110.0, -50.0, 54.0), r: 3.0 };
        for v in [0.5f32, 50.0, 120.0, 200.0, 250.0, 330.0] {
            let (s, x, _, _) = ring.at(v);
            let back = ring.v_of(s, x);
            let p = ring.at(back);
            if (p.0 - s).abs() < 1e-3 || (p.1 - x).abs() < 1e-3 {
                assert!((p.0 - s).abs() < 1e-2 && (p.1 - x).abs() < 1e-2, "{v}: {back}");
            }
        }
    }
}
