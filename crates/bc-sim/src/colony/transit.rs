//! The trams: one line down the middle of each strip's avenue, eleven stations from Hub Gate to
//! the building site, twelve trains a line on one timetable. Like everything in the colony it's a
//! closed form of the tick, so every client sees each train where the server has it and nothing
//! about them is ever sent.
//!
//! A train runs the line out and back: it waits at a station with its doors open, runs to the
//! next (speeding up and slowing down at [`ACCEL`]), and at the end of the line comes back on the
//! other track. Out-bound trains (+x, towards the building site) keep to the track on the avenue's
//! +s side, in-bound ones to the −s side; between the tracks at each station is an island platform
//! at the cars' floor height, with steps at both ends.
//!
//! Positions are city coordinates (`frame::CityPos`): `x` along, `s` across, `h` up.

use super::city::{CityBox, Rect};
use super::frame::STRIP_WIDTH;
use crate::TICK_HZ;
use crate::math::sqrt;

pub const STATIONS: usize = 11;
/// The first station (by Hub Gate), and the distance between stations, m.
pub const FIRST_STATION: f32 = -15_500.0;
pub const STATION_GAP: f32 = 2_450.0;
pub const TRAINS: u32 = 12;
/// A run from one station to the next, and a stop at one, ticks (81 s and 19.8 s).
pub const RUN_TICKS: u32 = 2_430;
pub const DWELL_TICKS: u32 = 594;
const STOP_TICKS: u32 = RUN_TICKS + DWELL_TICKS;
/// Runs in a round trip.
const LEGS: u32 = 2 * (STATIONS as u32 - 1);
/// A train's round trip, ticks (33.6 min), and the time between trains (2.8 min).
pub const PERIOD_TICKS: u32 = LEGS * STOP_TICKS;
pub const HEADWAY_TICKS: u32 = PERIOD_TICKS / TRAINS;
/// How hard a train speeds up and slows down, m/s².
pub const ACCEL: f32 = 1.5;
/// Doors open this long after a train stops and close this long before it leaves, ticks.
pub const DOOR_MARGIN: u32 = 60;
/// A train: its cars, their length and the gap between them, their width and height, m.
pub const CARS: usize = 3;
pub const CAR_LENGTH: f32 = 24.0;
pub const CAR_GAP: f32 = 1.0;
pub const CAR_WIDTH: f32 = 3.0;
pub const CAR_HEIGHT: f32 = 3.4;
pub const TRAIN_LENGTH: f32 = CARS as f32 * CAR_LENGTH + (CARS as f32 - 1.0) * CAR_GAP;
/// The cars' floor above the ground, and the platforms' top, m.
pub const FLOOR: f32 = 1.0;
/// Each track's middle from the avenue's, m.
pub const TRACK_OFFSET: f32 = 4.5;
/// The island platform: half its width (to a hand's breadth from the cars), its length, and its
/// steps (five of them at each end, 0.2 m up and 0.6 m deep).
pub const PLATFORM_HALF: f32 = TRACK_OFFSET - CAR_WIDTH * 0.5 - 0.1;
pub const PLATFORM_LENGTH: f32 = 80.0;
const STEPS: usize = 5;
const STEP_DEPTH: f32 = 0.6;
/// The doors: two a side on each car, this far either side of its middle, this wide, m.
pub const DOOR_AT: f32 = 6.0;
pub const DOOR_WIDTH: f32 = 1.6;
/// The strips' lines run out of step with each other, by this much, ticks.
const STRIP_OFFSET: u32 = HEADWAY_TICKS / 3;

/// Where station `i` is along the strip, m.
pub fn station_x(i: usize) -> f32 {
    FIRST_STATION + STATION_GAP * i as f32
}

/// When trains stand at station `i` with their doors on side `dir` (the out-bound track's, > 0, or
/// the in-bound one's): one does every [`HEADWAY_TICKS`], from this tick in each headway on. None
/// on a terminus's other side: trains turn back there on the track they leave by.
pub fn stands(strip: u8, i: usize, dir: f32) -> Option<u32> {
    let leg = if dir > 0.0 {
        (i < STATIONS - 1).then_some(i as u32)?
    } else {
        (i > 0).then_some(LEGS - i as u32)?
    };
    let offset = u32::from(strip % 3) * STRIP_OFFSET;
    Some((leg * STOP_TICKS + PERIOD_TICKS - offset) % HEADWAY_TICKS)
}

/// The top of a station's island platform at `x` along the strip, if it's over it: its steps
/// climb from either end.
pub fn platform_top(x: f32) -> Option<f32> {
    let near = ((x - FIRST_STATION) / STATION_GAP + 0.5) as i32;
    let i = near.clamp(0, STATIONS as i32 - 1) as usize;
    let from_end = 0.5 * PLATFORM_LENGTH - (x - station_x(i)).abs();
    if from_end < 0.0 {
        return None;
    }
    let step = (from_end / STEP_DEPTH) as usize;
    Some(FLOOR * (step + 1).min(STEPS) as f32 / STEPS as f32)
}

/// Where a track runs across the strip: the out-bound (`dir` > 0) or the in-bound one, m.
pub fn track_s(dir: f32) -> f32 {
    STRIP_WIDTH * 0.5 + if dir > 0.0 { TRACK_OFFSET } else { -TRACK_OFFSET }
}

/// A train, this moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrainState {
    pub strip: u8,
    pub k: u8,
    /// Its middle along the strip, and its track's middle across it, m.
    pub x: f32,
    pub s: f32,
    /// Out-bound (+1) or in-bound (−1).
    pub dir: f32,
    /// Its speed, m/s, and its acceleration along +x, m/s².
    pub speed: f32,
    pub accel: f32,
    /// The station it stands at, if it does.
    pub at: Option<usize>,
    /// Its doors are open.
    pub doors: bool,
}

impl TrainState {
    /// Its velocity along +x, m/s.
    pub fn vel_x(&self) -> f32 {
        self.dir * self.speed
    }

    /// Car `c`'s middle along the strip, m (car 0 at the −x end, whichever way it runs, so a
    /// rider's car stays put when a train turns back).
    pub fn car_x(&self, c: usize) -> f32 {
        self.x + car_offset(c)
    }
}

/// Car `c`'s middle from its train's, along +x, m.
pub fn car_offset(c: usize) -> f32 {
    -0.5 * TRAIN_LENGTH + 0.5 * CAR_LENGTH + c as f32 * (CAR_LENGTH + CAR_GAP)
}

/// The run's top speed: a run of `STATION_GAP` m in `RUN_TICKS`, speeding up and slowing down at
/// `ACCEL`, m/s.
fn cruise() -> f32 {
    let t = RUN_TICKS as f32 / TICK_HZ as f32;
    0.5 * (ACCEL * t - sqrt(ACCEL * ACCEL * t * t - 4.0 * ACCEL * STATION_GAP))
}

/// `tau` s into a run: how far along it (m), how fast (m/s) and its acceleration (m/s²).
fn run(tau: f32) -> (f32, f32, f32) {
    let t = RUN_TICKS as f32 / TICK_HZ as f32;
    let v = cruise();
    let ta = v / ACCEL;
    let tau = tau.clamp(0.0, t);
    if tau < ta {
        (0.5 * ACCEL * tau * tau, ACCEL * tau, ACCEL)
    } else if tau < t - ta {
        (0.5 * ACCEL * ta * ta + v * (tau - ta), v, 0.0)
    } else {
        let left = t - tau;
        (STATION_GAP - 0.5 * ACCEL * left * left, ACCEL * left, -ACCEL)
    }
}

/// Train `k` of strip `strip`'s line at tick `t` plus `frac` of the next.
pub fn train(strip: u8, k: u8, t: u32, frac: f32) -> TrainState {
    let offset = (u32::from(k) % TRAINS) * HEADWAY_TICKS + u32::from(strip % 3) * STRIP_OFFSET;
    let phase = (t % PERIOD_TICKS + offset) % PERIOD_TICKS;
    let leg = phase / STOP_TICKS;
    let within = (phase - leg * STOP_TICKS) as f32 + frac.clamp(0.0, 1.0);
    let half = LEGS / 2;
    let (from, dir) = if leg < half { (leg as usize, 1.0) } else { ((LEGS - leg) as usize, -1.0) };
    let s = track_s(dir);
    if within < DWELL_TICKS as f32 {
        let doors = (DOOR_MARGIN as f32..(DWELL_TICKS - DOOR_MARGIN) as f32).contains(&within);
        return TrainState {
            strip,
            k,
            x: station_x(from),
            s,
            dir,
            speed: 0.0,
            accel: 0.0,
            at: Some(from),
            doors,
        };
    }
    let (d, v, a) = run((within - DWELL_TICKS as f32) / TICK_HZ as f32);
    TrainState {
        strip,
        k,
        x: station_x(from) + dir * d,
        s,
        dir,
        speed: v,
        accel: dir * a,
        at: None,
        doors: false,
    }
}

/// A station's island platform and its steps, as solid boxes (`city::each_solid`'s): calls `f`
/// with each near `area`; stops early when it returns true, and says whether it did.
pub fn platform_solids(area: &Rect, mut f: impl FnMut(&CityBox) -> bool) -> bool {
    let mid = STRIP_WIDTH * 0.5;
    let (s0, s1) = (mid - PLATFORM_HALF, mid + PLATFORM_HALF);
    if area.s1 < s0 || area.s0 > s1 {
        return false;
    }
    let near = ((0.5 * (area.x0 + area.x1) - FIRST_STATION) / STATION_GAP + 0.5) as i32;
    for i in (near - 1).max(0)..=(near + 1).min(STATIONS as i32 - 1) {
        let x = station_x(i as usize);
        let (x0, x1) = (x - 0.5 * PLATFORM_LENGTH, x + 0.5 * PLATFORM_LENGTH);
        let flight = STEPS as f32 * STEP_DEPTH;
        let top = CityBox { rect: Rect::new(s0, s1, x0 + flight, x1 - flight), h0: -50.0, h1: FLOOR };
        if top.rect.overlaps(area) && f(&top) {
            return true;
        }
        for n in 0..STEPS {
            let h1 = FLOOR * (n + 1) as f32 / STEPS as f32;
            let d = n as f32 * STEP_DEPTH;
            for r in [Rect::new(s0, s1, x0 + d, x0 + flight), Rect::new(s0, s1, x1 - flight, x1 - d)] {
                let b = CityBox { rect: r, h0: -50.0, h1 };
                if r.overlaps(area) && f(&b) {
                    return true;
                }
            }
        }
    }
    false
}

/// A car's body as solid boxes in its own frame (`x` along from its middle, `s` across from its
/// middle, `h` up from its floor): the floor, the roof, the ends, and the sides with their doors
/// left open if `open`. Calls `f` with each.
pub fn car_walls(open: bool, mut f: impl FnMut(&CityBox) -> bool) -> bool {
    let (hl, hw, t) = (0.5 * CAR_LENGTH, 0.5 * CAR_WIDTH, 0.12);
    let height = CAR_HEIGHT - FLOOR + 0.6;
    let floor = CityBox { rect: Rect::new(-hw, hw, -hl, hl), h0: -0.5, h1: 0.0 };
    let roof = CityBox { rect: Rect::new(-hw, hw, -hl, hl), h0: height, h1: height + 0.2 };
    let ends = [
        CityBox { rect: Rect::new(-hw, hw, -hl - t, -hl + t), h0: -0.5, h1: height },
        CityBox { rect: Rect::new(-hw, hw, hl - t, hl + t), h0: -0.5, h1: height },
    ];
    for b in [floor, roof].iter().chain(ends.iter()) {
        if f(b) {
            return true;
        }
    }
    // Each side: wall between and beyond the doors (or all of it, shut).
    let half_door = 0.5 * DOOR_WIDTH;
    let spans: [(f32, f32); 3] =
        [(-hl, -DOOR_AT - half_door), (-DOOR_AT + half_door, DOOR_AT - half_door), (DOOR_AT + half_door, hl)];
    for side in [-1.0f32, 1.0] {
        let (s0, s1) = if side < 0.0 { (-hw - t, -hw + t) } else { (hw - t, hw + t) };
        if !open {
            if f(&CityBox { rect: Rect::new(s0, s1, -hl, hl), h0: -0.5, h1: height }) {
                return true;
            }
            continue;
        }
        for (x0, x1) in spans {
            if f(&CityBox { rect: Rect::new(s0, s1, x0, x1), h0: -0.5, h1: height }) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_timetable_adds_up() {
        assert_eq!(PERIOD_TICKS, 60_480);
        assert_eq!(TRAINS, u32::from(bc_proto::presence::MAX_TRAINS), "the wire names every train");
        assert_eq!(PERIOD_TICKS % TRAINS, 0);
        assert!((station_x(STATIONS - 1) - 9_000.0).abs() < 1e-3);
        let v = cruise();
        assert!((50.0..60.0).contains(&v), "{v} m/s");
        let (d, speed, _) = run(RUN_TICKS as f32 / TICK_HZ as f32);
        assert!((d - STATION_GAP).abs() < 0.05 && speed.abs() < 1e-3, "{d} {speed}");
    }

    #[test]
    fn trains_come_round_exactly_and_move_smoothly() {
        for k in 0..TRAINS as u8 {
            for t in [0u32, 1_234, 30_000, 59_999] {
                assert_eq!(train(1, k, t, 0.3), train(1, k, t + PERIOD_TICKS, 0.3));
            }
        }
        // Tick by tick: no jumps along the track, |a| ≤ ACCEL, speed ≤ cruise.
        let mut last = train(0, 3, 0, 0.0);
        for t in 1..PERIOD_TICKS {
            let now = train(0, 3, t, 0.0);
            assert!(now.accel.abs() <= ACCEL + 1e-4 && now.speed <= cruise() + 1e-3);
            if now.dir == last.dir {
                let dx = (now.x - last.x) * now.dir;
                assert!((-1e-3..=cruise() / TICK_HZ as f32 + 0.01).contains(&dx), "a jump at {t}: {dx}");
            } else {
                // Turning back at the end of the line: standing at its terminus.
                assert!(now.at.is_some() && (now.x - last.x).abs() < 1e-3, "turned back at {t}");
            }
            last = now;
        }
    }

    #[test]
    fn trains_keep_their_distance_and_stand_at_platforms_with_doors_open() {
        for strip in 0..3u8 {
            let mut opened = 0;
            for t in (0..PERIOD_TICKS).step_by(15) {
                let all: [TrainState; TRAINS as usize] =
                    core::array::from_fn(|k| train(strip, k as u8, t, 0.0));
                for (i, a) in all.iter().enumerate() {
                    for b in &all[i + 1..] {
                        if a.dir == b.dir {
                            assert!((a.x - b.x).abs() >= 300.0, "trains {} and {} at {t}", a.k, b.k);
                        }
                    }
                    if a.doors {
                        opened += 1;
                        let st = a.at.expect("doors open only at a station");
                        // Every door along the platform's edge.
                        for c in 0..CARS {
                            for door in [-DOOR_AT, DOOR_AT] {
                                let x = a.car_x(c) + door;
                                assert!(
                                    (x - station_x(st)).abs() < 0.5 * PLATFORM_LENGTH - 3.0,
                                    "a door off the platform"
                                );
                            }
                        }
                        let edge = (a.s - STRIP_WIDTH * 0.5).abs() - 0.5 * CAR_WIDTH;
                        assert!((edge - PLATFORM_HALF).abs() < 0.2, "the gap to the platform: {edge}");
                    }
                }
            }
            assert!(opened > 0);
        }
    }

    #[test]
    fn trains_stand_when_the_timetable_says() {
        for strip in 0..3u8 {
            for i in 0..STATIONS {
                for dir in [1.0f32, -1.0] {
                    let Some(at) = stands(strip, i, dir) else {
                        assert!((i == 0 && dir < 0.0) || (i == STATIONS - 1 && dir > 0.0));
                        continue;
                    };
                    for t in [at, at + HEADWAY_TICKS * 5, at + DWELL_TICKS - 1 + HEADWAY_TICKS * 11] {
                        let n = (0..TRAINS as u8)
                            .filter(|&k| {
                                let tr = train(strip, k, t, 0.0);
                                tr.at == Some(i) && tr.dir == dir
                            })
                            .count();
                        assert_eq!(n, 1, "strip {strip} station {i} dir {dir} at {t}");
                    }
                    let gone = (0..TRAINS as u8).all(|k| {
                        train(strip, k, at + DWELL_TICKS, 0.0).at != Some(i)
                            || train(strip, k, at + DWELL_TICKS, 0.0).dir != dir
                    });
                    assert!(gone, "the train's still there after its dwell");
                }
            }
        }
    }

    #[test]
    fn the_platform_is_climbed_by_its_steps() {
        let x = station_x(4);
        let s = STRIP_WIDTH * 0.5;
        let top_at = |x: f32| {
            let mut h = f32::MIN;
            platform_solids(&Rect::new(s - 0.1, s + 0.1, x - 0.01, x + 0.01), |b| {
                h = h.max(b.h1);
                false
            });
            h
        };
        assert_eq!(top_at(x), FLOOR);
        let mut last = 0.0;
        for k in 0..=10 {
            let h = top_at(x - 0.5 * PLATFORM_LENGTH + 0.3 + k as f32 * 0.3);
            assert!(h - last <= 0.2 + 1e-4, "a step too high at {k}: {last} to {h}");
            last = h.max(last);
        }
        assert_eq!(top_at(x - 0.5 * PLATFORM_LENGTH - 1.0), f32::MIN, "nothing past its end");
        // Off the platform (on the track): nothing.
        let mut any = false;
        platform_solids(&Rect::new(s + 4.0, s + 5.0, x - 1.0, x + 1.0), |_| {
            any = true;
            false
        });
        assert!(!any);
    }

    #[test]
    fn a_car_with_its_doors_open_lets_you_through_them_only() {
        let hit = |open: bool, s: f32, x: f32| {
            car_walls(open, |b| {
                b.rect.overlaps(&Rect::new(s - 0.3, s + 0.3, x - 0.3, x + 0.3)) && b.h1 > 0.5 && b.h0 < 1.5
            })
        };
        let side = 0.5 * CAR_WIDTH;
        assert!(!hit(true, side, DOOR_AT), "through an open door");
        assert!(hit(false, side, DOOR_AT), "a shut one");
        assert!(hit(true, side, 0.0), "the wall between the doors");
        assert!(!hit(true, 0.0, 0.0), "the aisle");
    }
}
