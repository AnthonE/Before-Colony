//! The town's people taken out of the crowd (`docs/FREE_ROAM.md`, 3: the promoted tier). Once
//! something acts on one of [`street`]'s people (a car tearing along where they walk, someone run
//! down beside them, a car taken from its driver), the closed form no longer has them: the server
//! does, from where the closed form had them on that tick, till they've settled and they and the
//! place the crowd would have them are out of everyone's sight, when the crowd has them back.
//!
//! This is one of them: the ground they keep to ([`Way`]: the closed form's own, so nobody taken out
//! of the crowd steps into a road either), how they feel ([`Mood`]) and a step. Nothing here
//! allocates or reads a clock; the server keeps them in fixed pools.

use core::f32::consts::PI;

use super::street::{self, FILE, Person, Pose, WALK_IN, round_rect};
use super::town::{self, BlockId, COLUMNS, ROUND, TownBox};
use crate::TICK_HZ;
use crate::math::{atan2, cos, hypot, sin};

/// How fast someone scared runs, m/s: this, and up to [`RUN_MORE`] more by who they are.
pub const RUN: f32 = 4.2;
const RUN_MORE: f32 = 1.6;
/// How long a scare lasts, ticks: this, and up to as long again by who they are.
pub const FLEE_TICKS: u32 = 6 * TICK_HZ;
/// How long someone knocked down stands about once they're up, ticks.
pub const DAZED_TICKS: u32 = 2 * TICK_HZ;
/// How fast they get going, and come to a stop, m/s².
const SPEED_UP: f32 = 8.0;
const SLOW_DOWN: f32 = 3.0;
/// A stride walking, m, and how much longer it gets for each m/s faster.
const STRIDE: f32 = 1.45;
const STRIDE_MORE: f32 = 0.35;
/// The square's middle, kept clear of where its edges' walkers go, m (as [`street`] stands them).
const SQUARE_IN: f32 = 6.0;

/// The ground someone taken out of the crowd keeps to: the closed form's own for them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Way {
    /// Round their block inside its kerb (`path`: their file of it), the crowd's way round (`back`:
    /// against `u`), at their own `pace`, m/s.
    Block { path: TownBox, back: bool, pace: f32 },
    /// Along a rim walk at `x` across the ring, the crowd's way along it (`dir`, round `t`).
    Rim { x: f32, dir: f32, pace: f32 },
    /// Anywhere in a square's open middle, where they stood about.
    Square { area: TownBox },
}

impl Way {
    /// The way of the person `p` as the crowd has them.
    pub fn of(p: &Person) -> Option<Way> {
        if p.id >> 28 == 0xF {
            let dir = if p.yaw > 0.5 * PI { 1.0 } else { -1.0 };
            return Some(Way::Rim { x: p.x, dir, pace: p.speed });
        }
        let block = p.id >> 4;
        let id = BlockId::new((block / u32::from(COLUMNS)) as u16, (block % u32::from(COLUMNS)) as u8);
        let r = town::block_rect(id).inset(WALK_IN);
        if p.pose == Pose::Stand {
            return Some(Way::Square { area: r.inset(SQUARE_IN) });
        }
        let back = p.seed & 1 == 1;
        let path = r.inset(if back { 0.0 } else { FILE });
        Some(Way::Block { path, back, pace: p.speed })
    }

    /// Round the way once, m (none for a square's middle).
    fn round(&self) -> f32 {
        match self {
            Way::Block { path, .. } => 2.0 * ((path.x1 - path.x0) + (path.t1 - path.t0)),
            Way::Rim { .. } => ROUND,
            Way::Square { .. } => 0.0,
        }
    }

    /// Where `u` along it is, `(x, t)`, and the heading going on along it (`u` growing).
    fn at(&self, u: f32) -> (f32, f32, f32) {
        match *self {
            Way::Block { path, .. } => {
                let m = self.round();
                let u = u - crate::math::floor(u / m) * m;
                round_rect(&path, u.min(m - 1e-3), false)
            }
            Way::Rim { x, .. } => (x, town::wrap(u), PI),
            Way::Square { .. } => (0.0, 0.0, 0.0),
        }
    }

    /// How far along it `(x, t)` is (the nearest place on it, for a block).
    fn along(&self, x: f32, t: f32) -> f32 {
        match *self {
            Way::Block { path: r, .. } => {
                let (w, h) = (r.x1 - r.x0, r.t1 - r.t0);
                let (x, t) = (x.clamp(r.x0, r.x1), t.clamp(r.t0, r.t1));
                let edges = [(t - r.t0).abs(), (r.x1 - x).abs(), (r.t1 - t).abs(), (x - r.x0).abs()];
                let mut best = 0;
                for k in 1..4 {
                    if edges[k] < edges[best] {
                        best = k;
                    }
                }
                match best {
                    0 => x - r.x0,
                    1 => w + (t - r.t0),
                    2 => w + h + (r.x1 - x),
                    _ => 2.0 * w + h + (r.t1 - t),
                }
            }
            Way::Rim { .. } => t,
            Way::Square { .. } => 0.0,
        }
    }

    /// Which way along it the crowd has them go: 1 with `u`, −1 against it.
    fn crowds_way(&self) -> f32 {
        match *self {
            Way::Block { back, .. } => {
                if back {
                    -1.0
                } else {
                    1.0
                }
            }
            Way::Rim { dir, .. } => dir,
            Way::Square { .. } => 0.0,
        }
    }

    /// How fast the crowd has them walk, m/s.
    fn pace(&self) -> f32 {
        match *self {
            Way::Block { pace, .. } | Way::Rim { pace, .. } => pace,
            Way::Square { .. } => 0.0,
        }
    }
}

/// How someone taken out of the crowd feels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mood {
    /// Running from `(x, t)` till the tick `until`.
    Scared { from: (f32, f32), until: u32 },
    /// Just up off the ground, standing about till `until`.
    Dazed { until: u32 },
    /// Settled: going about as the crowd would, where they are now.
    Calm,
}

/// How far round the ring from `a` to `b` the short way, m.
pub fn round_dt(a: f32, b: f32) -> f32 {
    let d = town::wrap(b - a);
    if d > 0.5 * ROUND { d - ROUND } else { d }
}

/// One of the town's people out of the crowd.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loose {
    pub id: u32,
    pub seed: u32,
    pub way: Way,
    /// How far along their way they are (a block's or a rim walk's), m.
    pub u: f32,
    /// Where they are: across the ring, round it (`0..ROUND`), m; the way they face (the walker's
    /// yaw: 0 faces −t, τ/4 faces +x) and how fast they go, m/s.
    pub x: f32,
    pub t: f32,
    pub yaw: f32,
    pub speed: f32,
    /// Where they are in their stride, 0 to 1.
    pub stride: f32,
    /// Which way along their way they're going: 1 with `u`, −1 against it.
    pub dir: f32,
    pub mood: Mood,
}

impl Loose {
    /// The person `p` (the crowd's, where the tick has them) out of it, as they were.
    pub fn from_crowd(p: &Person) -> Option<Loose> {
        let way = Way::of(p)?;
        let u = way.along(p.x, p.t);
        Some(Loose {
            id: p.id,
            seed: p.seed,
            way,
            u,
            x: p.x,
            t: town::wrap(p.t),
            yaw: p.yaw,
            speed: p.speed,
            stride: p.stride,
            dir: way.crowds_way(),
            mood: Mood::Calm,
        })
    }

    /// The person `id`, knocked down at `(x, t)`, getting up there at tick `tick`: back on their way
    /// where it's nearest, standing about a moment first.
    pub fn getting_up(id: u32, (x, t): (f32, f32), yaw: f32, tick: u32) -> Option<Loose> {
        let p = street::person(id, tick)?;
        let way = Way::of(&p)?;
        let (x, t) = match way {
            Way::Block { .. } | Way::Rim { .. } => {
                let u = way.along(x, town::wrap(t));
                let (wx, wt, _) = way.at(u);
                (wx, wt)
            }
            Way::Square { area } => (x.clamp(area.x0, area.x1), town::wrap(t).clamp(area.t0, area.t1)),
        };
        Some(Loose {
            id,
            seed: p.seed,
            way,
            u: way.along(x, t),
            x,
            t,
            yaw,
            speed: 0.0,
            stride: 0.0,
            dir: way.crowds_way(),
            mood: Mood::Dazed { until: tick + DAZED_TICKS },
        })
    }

    /// Something frightening at `(x, t)` at tick `tick`: they run from it, the way along their way
    /// that takes them off from it (in a square, straight off), for a while yet.
    pub fn scare(&mut self, from: (f32, f32), tick: u32) {
        let until = tick + FLEE_TICKS + (town::unit(self.seed, 8) * FLEE_TICKS as f32) as u32;
        let (ax, at) = (self.x - from.0, round_dt(from.1, self.t));
        if !matches!(self.way, Way::Square { .. }) {
            let (_, _, yaw) = self.way.at(self.u);
            let along = ax * sin(yaw) - at * cos(yaw);
            self.dir = if along > 0.05 {
                1.0
            } else if along < -0.05 {
                -1.0
            } else if self.seed & 2 == 0 {
                1.0
            } else {
                -1.0
            };
        }
        self.mood = Mood::Scared { from, until };
    }

    /// Whether they've settled: going about as the crowd would.
    pub fn settled(&self) -> bool {
        self.mood == Mood::Calm
    }

    /// Whether they're running scared.
    pub fn scared(&self) -> bool {
        matches!(self.mood, Mood::Scared { .. })
    }

    /// How fast they'd go now, m/s.
    fn wants(&self) -> f32 {
        match self.mood {
            Mood::Scared { .. } => RUN + RUN_MORE * town::unit(self.seed, 7),
            Mood::Dazed { .. } => 0.0,
            Mood::Calm => self.way.pace(),
        }
    }

    /// `dt` s on, at tick `tick`: their mood worn off if its time is up, and on along their way.
    pub fn step(&mut self, tick: u32, dt: f32) {
        match self.mood {
            Mood::Scared { until, .. } | Mood::Dazed { until } if tick >= until => {
                self.mood = Mood::Calm;
                // Back the way the crowd goes, at their own pace.
                if self.way.crowds_way() != 0.0 {
                    self.dir = self.way.crowds_way();
                }
            }
            _ => {}
        }
        let want = self.wants();
        let rate = if want > self.speed { SPEED_UP } else { SLOW_DOWN };
        self.speed += (want - self.speed).clamp(-rate * dt, rate * dt);
        let go = self.speed * dt;
        match self.way {
            Way::Square { area } => {
                if let Mood::Scared { from, .. } = self.mood {
                    let (ax, at) = (self.x - from.0, round_dt(from.1, self.t));
                    let n = hypot(ax, at).max(1e-3);
                    let (vx, vt) = (ax / n * go, at / n * go);
                    let (x, t) =
                        ((self.x + vx).clamp(area.x0, area.x1), (self.t + vt).clamp(area.t0, area.t1));
                    if go > 1e-4 {
                        self.yaw = atan2(x - self.x, -(t - self.t));
                    }
                    (self.x, self.t) = (x, t);
                }
            }
            _ => {
                self.u += self.dir * go;
                let m = self.way.round();
                self.u -= crate::math::floor(self.u / m) * m;
                let (x, t, yaw) = self.way.at(self.u);
                (self.x, self.t) = (x, t);
                self.yaw = if self.dir < 0.0 { yaw + PI } else { yaw };
            }
        }
        let stride = STRIDE + STRIDE_MORE * (self.speed - 1.3).max(0.0);
        let stride = self.stride + go / stride;
        self.stride = stride - crate::math::floor(stride);
    }

    /// As one of the crowd is drawn: where, the way they face, how they hold themselves.
    pub fn person(&self) -> Person {
        Person {
            id: self.id,
            x: self.x,
            t: self.t,
            yaw: self.yaw,
            speed: self.speed,
            stride: self.stride,
            pose: if self.speed > 0.05 { Pose::Walk } else { Pose::Stand },
            seed: self.seed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::town::{Ground, ground_at};

    /// A walker, a stander and a stroller of the crowd's, near Hub Gate.
    fn some(tick: u32) -> (Person, Option<Person>, Person) {
        let (mut walker, mut stander, mut stroller) = (None, None, None);
        street::each_person(-1_000.0, 1_000.0, 0.0, 2.0 * town::CELL, tick, 0.0, &mut |p| {
            if p.id >> 28 == 0xF {
                stroller = stroller.or(Some(*p));
            } else if p.pose == Pose::Stand {
                stander = stander.or(Some(*p));
            } else {
                walker = walker.or(Some(*p));
            }
        });
        (walker.unwrap(), stander, stroller.unwrap())
    }

    #[test]
    fn the_crowds_own_person_is_found_alone() {
        let mut n = 0;
        for tick in [0, 977, 123_456] {
            street::each_person(-1_000.0, 1_000.0, 3_000.0, 3_000.0 + town::CELL, tick, 0.0, &mut |p| {
                let q = street::person(p.id, tick).expect("found");
                assert!(
                    q.id == p.id && (q.x - p.x).abs() < 1e-3 && round_dt(q.t, p.t).abs() < 1e-2,
                    "{p:?} {q:?}"
                );
                n += 1;
            });
        }
        assert!(n > 50, "{n}");
        assert!(street::person(0xFFFF_FFFF, 0).is_none());
    }

    #[test]
    fn out_of_the_crowd_they_start_where_it_had_them_and_keep_to_its_ground() {
        let (walker, stander, stroller) = some(4_000);
        for p in [Some(walker), stander, Some(stroller)].into_iter().flatten() {
            let mut l = Loose::from_crowd(&p).unwrap();
            assert!((l.x - p.x).abs() < 1e-2 && round_dt(l.t, p.t).abs() < 1e-2, "{p:?} {l:?}");
            // Calm, they go on as the crowd does: the same way, about as fast.
            let (u0, round) = (l.u, l.way.round());
            for k in 0..30 {
                l.step(4_000 + k * 3, 0.1);
            }
            if round > 0.0 {
                let mut moved = l.dir * (l.u - u0);
                moved -= crate::math::floor(moved / round + 0.5) * round;
                assert!((moved - 3.0 * p.speed).abs() < 0.05, "{p:?} moved {moved}");
            }
            // Scared by something right by them (on the side with less room, in a square), they run
            // off, and never step off where people walk.
            let (sx, st) = match l.way {
                Way::Square { area } => (
                    if l.x > 0.5 * (area.x0 + area.x1) { 1.0 } else { -1.0 },
                    if l.t > 0.5 * (area.t0 + area.t1) { 1.0 } else { -1.0 },
                ),
                _ => (1.0, 1.0),
            };
            let from = (l.x + sx, l.t + st);
            l.scare(from, 4_090);
            let start = (l.x, l.t);
            let mut far = 0.0f32;
            for k in 0..120 {
                l.step(4_090 + k * 3, 0.1);
                assert!(
                    matches!(ground_at(l.x, l.t), Ground::Block(_) | Ground::RimWalk),
                    "{p:?} stepped off at ({}, {})",
                    l.x,
                    l.t
                );
                far = far.max(hypot(l.x - start.0, round_dt(start.1, l.t)));
            }
            assert!(far > 8.0, "{p:?} ran {far}");
            // In the end they settle.
            for k in 0..200 {
                l.step(4_500 + k * 3, 0.1);
            }
            assert!(l.settled(), "{l:?}");
        }
    }

    #[test]
    fn up_off_the_ground_they_stand_a_moment_then_walk_on() {
        let (walker, ..) = some(9_000);
        let mut l = Loose::getting_up(walker.id, (walker.x, walker.t), 1.0, 9_000).unwrap();
        assert!(!l.settled() && l.speed == 0.0);
        l.step(9_000 + DAZED_TICKS / 2, 0.1);
        assert!(l.speed == 0.0 && l.person().pose == Pose::Stand);
        for k in 0..20 {
            l.step(9_000 + DAZED_TICKS + k * 3, 0.1);
        }
        assert!(l.settled() && l.speed > 0.5, "{l:?}");
    }
}
