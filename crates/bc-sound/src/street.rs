//! What the pilot hears on foot, in the ring's air: their own steps, paced by the ground they
//! cover; the town's traffic and its crowd, by how much of each is near; the car they drive, by
//! its speed, and its door as they get in and out; the cars going by; a lift's ride; and the bay's
//! machinery. A model of the pilot's state in, loop levels and one-shot cues out, as
//! [`crate::cockpit`] is for the suit, so every rule is a test.
//!
//! **Steps are distance, not time** (Gates' `crates/sound/src/steps.rs`): one each stride of ground
//! covered on the ground, the stride lengthening with speed so a run's cadence stays near a walk's.
//! Nothing in the air, and a cut (a lift, the ring's seam) is no walk.

use crate::Cue;
use crate::cockpit::LoopLevel;

/// The street's loops, in the order [`StreetOut::loops`] lists them.
pub const LOOPS: [Cue; 4] = [Cue::TrafficBed, Cue::CrowdBed, Cue::CarEngine, Cue::BayHum];

/// The quickest steps come, s (the mixer's cooldown for [`Cue::StreetStep`]).
pub const STEP_GAP: f32 = 0.15;
/// Below this speed a frame's move is no step (a nudge off a wall), m/s.
pub const STEP_MIN_SPEED: f32 = 0.4;
/// Steps a second: the stride is the speed over it, within [`STRIDE`] m.
pub const CADENCE: f32 = 2.7;
pub const STRIDE: (f32, f32) = (0.6, 2.4);
/// A run: steps land at full gain at this speed, m/s (`bc_sim::foot::RUN`); slower, quieter, down
/// to [`STEP_QUIET`].
pub const STEP_FULL: f32 = 7.0;
pub const STEP_QUIET: f32 = 0.45;
/// A frame's move farther than this, m, and quicker than [`CUT_SPEED`] m/s, is a cut (a lift's
/// door, the ring's seam), not a walk. (A slow frame's long stride is still a walk: the walker catches up
/// with up to 4 s of a guided walk in one frame, `bc-client`'s `GUIDE_CATCH_UP`.)
pub const TELEPORT: f32 = 2.5;
pub const CUT_SPEED: f32 = 15.0;
/// The longest frame steps are paced over, s (a hidden tab's return is one frame of this).
const LONGEST_FRAME: f64 = 4.0;

/// A car going by is heard within this, m.
pub const PASS_RADIUS: f32 = 60.0;
/// It sounds as it first comes this near, m, going at least [`PASS_SPEED`] m/s; again only once
/// it's been [`PASS_CLEAR`] m off.
pub const PASS_NEAR: f32 = 6.0;
pub const PASS_CLEAR: f32 = 12.0;
pub const PASS_SPEED: f32 = 4.0;
/// How many cars going by it keeps in mind at once (more than a boulevard's six lanes hold within
/// [`PASS_CLEAR`] going at [`PASS_SPEED`] or more).
const PASSING: usize = 32;
/// A car out of mind this long, s, is forgotten (it went out of the client's traffic).
const PASS_FORGET: f64 = 1.0;

/// A fight on foot ([`Cue::Blow`], [`Cue::Fall`]) is heard within this, m.
pub const BRAWL_RADIUS: f32 = 40.0;

/// The car's engine: its playback rate at idle and at [`TOP_SPEED`] m/s (`bc_sim::car`'s car).
pub const ENGINE_RATE: (f32, f32) = (0.6, 1.8);
pub const TOP_SPEED: f32 = 30.0;

/// The town is never quite still: the traffic's bed and the crowd's at their least, down there.
pub const TRAFFIC_FLOOR: f32 = 0.12;
pub const CROWD_FLOOR: f32 = 0.05;
/// In a car, the street outside is this much of what it is on foot.
pub const IN_A_CAR: (f32, f32) = (0.55, 0.35);
/// The bay's machinery heard on Bay Row, through its airlock, of what it is in the bay.
pub const ON_THE_ROW: f32 = 0.4;
/// How quickly each loop follows what's near, s (to come most of the way): the beds slowly, as
/// what's near changes; the engine quickly, as the throttle does.
const EASE: [f32; 4] = [0.8, 1.0, 0.12, 0.5];

/// The pilot on foot (or at a wheel) this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StreetIn {
    /// Down in the ring's town, on foot or at the wheel.
    pub in_town: bool,
    /// In the hangar bay; on Bay Row, outside its airlock.
    pub in_bay: bool,
    pub on_deck: bool,
    /// On their feet and walking (not at a wheel, in a seat, on a tram, or in a sequence): where
    /// the feet are, m (in any frame that's continuous while they walk), and whether they're on
    /// the ground.
    pub feet: Option<[f32; 3]>,
    pub grounded: bool,
    /// At the wheel of a car in the town, and how fast it's going, m/s.
    pub driving: bool,
    pub speed: f32,
    /// How much of the town's traffic is near, 0..1, and how many of its people, 0..1 (the client
    /// weighs them by distance).
    pub traffic: f32,
    pub crowd: f32,
    /// Lifts ridden so far: one for each ride the pilot took (down a spoke from the bay, across
    /// the hub, back up). A wake where they left off is no ride.
    pub rides: u32,
}

/// What the street sounds like this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreetOut {
    pub loops: [LoopLevel; LOOPS.len()],
}

/// The odometer: ground covered on the ground since the last step.
#[derive(Clone, Copy, Debug, Default)]
struct Steps {
    last: Option<[f32; 3]>,
    banked: f32,
    stride: f32,
}

/// The stride at `speed` m/s: [`CADENCE`] steps a second, within [`STRIDE`].
pub fn stride(speed: f32) -> f32 {
    (speed / CADENCE).clamp(STRIDE.0, STRIDE.1)
}

impl Steps {
    /// The feet at `at` after `dt` s: a step's gain (0..1) if one lands.
    fn sample(&mut self, at: [f32; 3], grounded: bool, dt: f32) -> Option<f32> {
        let last = self.last.replace(at)?;
        // Across the floor only: a stair's rise is no extra step.
        let (dx, dz) = (at[0] - last[0], at[2] - last[2]);
        let d = (dx * dx + dz * dz).sqrt();
        if !grounded || d > TELEPORT.max(CUT_SPEED * dt) {
            // In the air, nothing banks (and landing releases no burst); a cut walks nowhere.
            self.banked = 0.0;
            return None;
        }
        let speed = if dt > 1e-4 { d / dt } else { 0.0 };
        if speed < STEP_MIN_SPEED {
            return None;
        }
        let stride = stride(speed);
        if self.stride > 0.0 {
            // Ground banked at one pace is spent at the next's.
            self.banked *= stride / self.stride;
        }
        self.stride = stride;
        self.banked += d;
        if self.banked < stride {
            return None;
        }
        // Subtract, so a slow frame's second stride is kept; but never a burst after a hitch.
        self.banked = (self.banked - stride).min(stride);
        Some(STEP_QUIET + (1.0 - STEP_QUIET) * (speed / STEP_FULL).clamp(0.0, 1.0))
    }
}

/// The street's memory between frames.
#[derive(Clone, Copy, Debug, Default)]
pub struct Street {
    prev: Option<StreetIn>,
    then: Option<f64>,
    steps: Steps,
    /// Each loop's level as last heard.
    levels: [f32; LOOPS.len()],
    /// The cars going by, by id, and when each was last within [`PASS_CLEAR`].
    passing: [Option<(u32, f64)>; PASSING],
}

impl Street {
    /// A car of the town's traffic `id`, `dist` m off and going `speed` m/s, `now`: the gain
    /// (0..1) of [`Cue::CarPass`] if it's just come by (once as it comes within [`PASS_NEAR`];
    /// again only after it's been [`PASS_CLEAR`] off). Ask for every car near each frame.
    pub fn car(&mut self, now: f64, id: u32, dist: f32, speed: f32) -> Option<f32> {
        let known = self.passing.iter().position(|p| p.is_some_and(|(k, _)| k == id));
        if let Some(k) = known {
            self.passing[k] = (dist <= PASS_CLEAR).then_some((id, now));
            return None;
        }
        if dist > PASS_NEAR || speed < PASS_SPEED {
            return None;
        }
        // Into a free place, else the one longest out of mind.
        let since = |k: usize| self.passing[k].map_or(f64::MIN, |p| p.1);
        let k = self
            .passing
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| (0..PASSING).min_by(|a, b| since(*a).total_cmp(&since(*b))).unwrap_or(0));
        self.passing[k] = Some((id, now));
        Some((speed / 20.0).clamp(0.35, 1.0))
    }

    /// Steps once a frame: `cue` is called for every one-shot this frame starts, with its gain
    /// (0..1).
    pub fn frame(&mut self, now: f64, i: &StreetIn, cue: &mut dyn FnMut(Cue, f32)) -> StreetOut {
        // The frame's time: all of it for the steps (paced by the ground a slow frame covers), a
        // quarter of a second at most for the loops' easing.
        let frame = self.then.map_or(0.0, |t| (now - t).clamp(0.0, LONGEST_FRAME)) as f32;
        let dt = frame.min(0.25);
        self.then = Some(now);
        if let Some(p) = self.prev {
            if i.rides > p.rides {
                cue(Cue::LiftRide, 1.0);
            }
            // In or out of a car: its door.
            if p.in_town && i.in_town && p.driving != i.driving {
                cue(Cue::CarDoor, 1.0);
            }
        }
        match i.feet {
            Some(at) => {
                if let Some(gain) = self.steps.sample(at, i.grounded, frame) {
                    cue(Cue::StreetStep, gain);
                }
            }
            None => self.steps = Steps::default(),
        }
        if !i.in_town {
            self.passing = [None; PASSING];
        }
        for p in &mut self.passing {
            if p.is_some_and(|(_, at)| now - at > PASS_FORGET) {
                *p = None;
            }
        }
        self.prev = Some(*i);

        let town = |b: bool| if b && i.in_town { 1.0 } else { 0.0 };
        let car = i.driving && i.in_town;
        let pace = (i.speed.abs() / TOP_SPEED).clamp(0.0, 1.0);
        let targets = [
            town(true)
                * (TRAFFIC_FLOOR + (1.0 - TRAFFIC_FLOOR) * i.traffic.clamp(0.0, 1.0))
                * if car { IN_A_CAR.0 } else { 1.0 },
            town(true)
                * (CROWD_FLOOR + (1.0 - CROWD_FLOOR) * i.crowd.clamp(0.0, 1.0))
                * if car { IN_A_CAR.1 } else { 1.0 },
            town(car) * (0.55 + 0.45 * pace),
            if i.in_bay {
                1.0
            } else if i.on_deck {
                ON_THE_ROW
            } else {
                0.0
            },
        ];
        for (k, level) in self.levels.iter_mut().enumerate() {
            *level += (targets[k] - *level) * (1.0 - (-dt / EASE[k]).exp());
            // (Under a hundredth, about −40 dB, a bed going out is out.)
            if *level < 0.01 && targets[k] == 0.0 {
                *level = 0.0;
            }
        }
        let rate = |cue: Cue| match cue {
            Cue::CarEngine => ENGINE_RATE.0 + (ENGINE_RATE.1 - ENGINE_RATE.0) * pace,
            _ => 1.0,
        };
        StreetOut {
            loops: std::array::from_fn(|k| LoopLevel {
                cue: LOOPS[k],
                gain: self.levels[k],
                rate: rate(LOOPS[k]),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cockpit::{Cockpit, CockpitIn};

    const FRAME: f64 = 1.0 / 60.0;

    fn walking(at: [f32; 3]) -> StreetIn {
        StreetIn { in_town: true, feet: Some(at), grounded: true, ..Default::default() }
    }

    fn step(s: &mut Street, now: f64, i: StreetIn) -> (StreetOut, Vec<(Cue, f32)>) {
        let mut cues = Vec::new();
        let out = s.frame(now, &i, &mut |q, g| cues.push((q, g)));
        (out, cues)
    }

    fn gain(out: &StreetOut, cue: Cue) -> f32 {
        out.loops.iter().find(|l| l.cue == cue).map_or(0.0, |l| l.gain)
    }

    /// Walks `secs` s at `speed` m/s along x from `from`: the steps heard.
    fn walk(s: &mut Street, from: f64, secs: f64, speed: f32) -> usize {
        let mut n = 0;
        let mut k = 0;
        while k as f64 * FRAME < secs {
            let t = k as f64 * FRAME;
            let x = speed * t as f32;
            n +=
                step(s, from + t, walking([x, 0.0, 0.0])).1.iter().filter(|q| q.0 == Cue::StreetStep).count();
            k += 1;
        }
        n
    }

    #[test]
    fn steps_follow_the_ground_covered() {
        // A walk (4.2 m/s, `bc_sim::foot::WALK`) for 10 s: its cadence, near enough.
        let mut s = Street::default();
        let n = walk(&mut s, 0.0, 10.0, 4.2);
        assert!((25..=29).contains(&n), "{n} steps at a walk");
        // Standing still: none. A run: about the same cadence, the stride longer.
        let mut s = Street::default();
        assert_eq!(walk(&mut s, 0.0, 3.0, 0.0), 0);
        let mut s = Street::default();
        let run = walk(&mut s, 0.0, 10.0, 7.0);
        assert!((25..=31).contains(&run), "{run} steps at a run");
        // A creep: fewer.
        let mut s = Street::default();
        let creep = walk(&mut s, 0.0, 10.0, 1.0);
        assert!((14..=18).contains(&creep), "{creep} steps at a creep");
    }

    #[test]
    fn steps_ignore_frame_rate() {
        let count = |hz: f64| {
            let mut s = Street::default();
            let mut n = 0;
            for k in 0..(10.0 * hz) as usize {
                let t = k as f64 / hz;
                let x = 4.2 * t as f32;
                n += step(&mut s, t, walking([x, 0.0, 0.0])).1.len();
            }
            n
        };
        let (slow, fast) = (count(30.0), count(240.0));
        assert!(slow.abs_diff(fast) <= 1, "{slow} at 30 Hz, {fast} at 240 Hz");
    }

    #[test]
    fn a_slow_machine_still_walks() {
        // Two frames a second (the walker covers each frame's ground in one stride, 2.1 m at a
        // walk): steps still come, fewer than at a smooth rate but never none.
        let mut s = Street::default();
        let mut n = 0;
        for k in 0..20 {
            let t = k as f64 * 0.5;
            n += step(&mut s, t, walking([4.2 * t as f32, 0.0, 0.0])).1.len();
        }
        assert!((15..=28).contains(&n), "{n} steps in 10 s at 2 Hz");
        // A guided walk catching up a whole second in a frame (4.2 m): a step, not a cut.
        let (_, cues) = step(&mut s, 11.0, walking([4.2 * 11.0, 0.0, 0.0]));
        assert!(cues.iter().any(|q| q.0 == Cue::StreetStep), "{cues:?}");
        // But a lift's door is a cut however slow the frame.
        let (_, cues) = step(&mut s, 11.7, walking([4.2 * 11.0, 0.0, 300.0]));
        assert!(cues.is_empty(), "{cues:?}");
    }

    #[test]
    fn no_steps_in_the_air_or_across_a_cut() {
        let mut s = Street::default();
        step(&mut s, 0.0, walking([0.0; 3]));
        // A jump that covers 3 m: nothing, and nothing banked to land with.
        for k in 1..40 {
            let at = [k as f32 * 0.075, 1.0, 0.0];
            let (_, cues) = step(&mut s, k as f64 * FRAME, StreetIn { grounded: false, ..walking(at) });
            assert!(cues.is_empty());
        }
        let (_, cues) = step(&mut s, 40.0 * FRAME, walking([3.0, 0.0, 0.0]));
        assert!(cues.is_empty(), "landing is no step");
        // Put 300 m round the ring in a frame (a lift's door, the seam): no step.
        let (_, cues) = step(&mut s, 41.0 * FRAME, walking([3.0, 0.0, 300.0]));
        assert!(cues.is_empty());
        // At a wheel (no feet): none, however far the car goes (its door, once).
        for k in 0..120 {
            let i = StreetIn { feet: None, driving: true, speed: 20.0, ..walking([0.0; 3]) };
            let (_, cues) = step(&mut s, 2.0 + k as f64 * FRAME, i);
            assert!(cues.iter().all(|q| q.0 == Cue::CarDoor) && (k == 0 || cues.is_empty()), "{cues:?}");
        }
    }

    #[test]
    fn a_run_is_louder_than_a_creep() {
        let loudest = |speed: f32| {
            let mut s = Street::default();
            let mut most = 0.0f32;
            for k in 0..300 {
                let t = k as f64 * FRAME;
                let at = [speed * t as f32, 0.0, 0.0];
                for (_, g) in step(&mut s, t, walking(at)).1 {
                    most = most.max(g);
                }
            }
            most
        };
        assert!(loudest(7.0) > 0.99);
        assert!(loudest(1.0) < 0.6 && loudest(1.0) >= STEP_QUIET);
    }

    #[test]
    fn silent_outside_the_town_and_the_bay() {
        let mut s = Street::default();
        for k in 0..120 {
            let i = StreetIn { traffic: 1.0, crowd: 1.0, driving: true, speed: 20.0, ..Default::default() };
            let (out, cues) = step(&mut s, k as f64 * FRAME, i);
            assert!(out.loops.iter().all(|l| l.gain == 0.0), "{out:?}");
            assert!(cues.is_empty());
        }
        // A car kept in mind is forgotten out of the town.
        assert!(s.car(5.0, 7, 2.0, 15.0).is_some());
        step(&mut s, 5.1, StreetIn::default());
        assert!(s.passing.iter().all(Option::is_none));
    }

    #[test]
    fn the_beds_follow_whats_near_and_fade() {
        let mut s = Street::default();
        let mut out = step(&mut s, 0.0, walking([0.0; 3])).0;
        for k in 1..300 {
            out =
                step(&mut s, k as f64 * FRAME, StreetIn { traffic: 0.0, crowd: 0.0, ..walking([0.0; 3]) }).0;
        }
        let (quiet_traffic, quiet_crowd) = (gain(&out, Cue::TrafficBed), gain(&out, Cue::CrowdBed));
        assert!((quiet_traffic - TRAFFIC_FLOOR).abs() < 0.01, "the town hums: {quiet_traffic}");
        assert!(quiet_crowd > 0.0 && quiet_crowd < 0.1);
        // Into a busy square: up, not at once.
        let busy = StreetIn { traffic: 0.8, crowd: 0.9, ..walking([0.0; 3]) };
        let first = step(&mut s, 300.0 * FRAME, busy).0;
        assert!(gain(&first, Cue::CrowdBed) < 0.2, "eased in");
        for k in 301..600 {
            out = step(&mut s, k as f64 * FRAME, busy).0;
        }
        assert!(gain(&out, Cue::TrafficBed) > 0.75 && gain(&out, Cue::CrowdBed) > 0.85);
        // Up the lift: the town fades away.
        for k in 600..900 {
            out = step(&mut s, k as f64 * FRAME, StreetIn { in_bay: true, ..Default::default() }).0;
        }
        assert_eq!(gain(&out, Cue::TrafficBed), 0.0);
        assert_eq!(gain(&out, Cue::CrowdBed), 0.0);
        assert!(gain(&out, Cue::BayHum) > 0.99, "the bay hums");
        for k in 900..1_200 {
            out = step(&mut s, k as f64 * FRAME, StreetIn { on_deck: true, ..Default::default() }).0;
        }
        assert!((gain(&out, Cue::BayHum) - ON_THE_ROW).abs() < 0.01, "through the airlock");
    }

    #[test]
    fn the_door_once_each_way_and_the_engine_by_speed() {
        let mut s = Street::default();
        step(&mut s, 0.0, walking([0.0; 3]));
        let seat = StreetIn { feet: None, driving: true, ..walking([0.0; 3]) };
        assert_eq!(step(&mut s, 0.1, seat).1, vec![(Cue::CarDoor, 1.0)]);
        assert!(step(&mut s, 0.2, seat).1.is_empty(), "once");
        let mut last = 0.0;
        for k in 0..=10 {
            let (out, _) = step(&mut s, 1.0 + f64::from(k), StreetIn { speed: 3.0 * k as f32, ..seat });
            let engine = out.loops.iter().find(|l| l.cue == Cue::CarEngine).copied();
            let engine = engine.expect("the engine");
            assert!(engine.rate > last, "the engine rises with speed");
            assert!(engine.gain > 0.5);
            last = engine.rate;
        }
        assert!((last - ENGINE_RATE.1).abs() < 1e-3);
        // Reversing sounds as going forward.
        let (out, _) = step(&mut s, 20.0, StreetIn { speed: -30.0, ..seat });
        assert!((out.loops[2].rate - ENGINE_RATE.1).abs() < 1e-3);
        assert_eq!(step(&mut s, 21.0, walking([0.0; 3])).1, vec![(Cue::CarDoor, 1.0)]);
        // Out of the car, the engine stops.
        let mut out = step(&mut s, 21.1, walking([0.0; 3])).0;
        for k in 0..60 {
            out = step(&mut s, 21.2 + k as f64 * FRAME, walking([0.0; 3])).0;
        }
        assert!(gain(&out, Cue::CarEngine) < 0.01);
        // Taken into the town in a car (a wake at the wheel): no door.
        let mut s = Street::default();
        assert!(step(&mut s, 0.0, seat).1.is_empty());
    }

    #[test]
    fn a_lift_rides_once_and_a_wake_is_no_ride() {
        let mut s = Street::default();
        // Woken in the town (the first frame): no ride, whatever the count.
        assert!(step(&mut s, 0.0, StreetIn { rides: 3, ..walking([0.0; 3]) }).1.is_empty());
        assert!(step(&mut s, 0.1, StreetIn { rides: 3, in_bay: true, ..Default::default() }).1.is_empty());
        let (_, cues) = step(&mut s, 0.2, StreetIn { rides: 4, ..walking([0.0; 3]) });
        assert_eq!(cues, vec![(Cue::LiftRide, 1.0)]);
        assert!(step(&mut s, 0.3, StreetIn { rides: 4, ..walking([0.0; 3]) }).1.is_empty());
    }

    #[test]
    fn a_car_goes_by_once() {
        let mut s = Street::default();
        step(&mut s, 0.0, walking([0.0; 3]));
        // Coming up the lane at 15 m/s, 3 m off the kerb: heard once as it comes by.
        let mut heard = 0;
        for k in 0..240 {
            let t = k as f64 * FRAME;
            let along = -30.0 + 15.0 * t as f32;
            let dist = along.hypot(3.0);
            heard += usize::from(s.car(t, 42, dist, 15.0).is_some());
            step(&mut s, t, walking([0.0; 3]));
        }
        assert_eq!(heard, 1);
        // Parked, or held back in a jam beside the pilot: nothing.
        assert_eq!(s.car(5.0, 43, 2.0, 0.0), None);
        assert_eq!(s.car(5.0, 44, 2.0, 1.5), None);
        // Gone 12 m off and back (round the block): heard again.
        assert!(s.car(6.0, 45, 4.0, 10.0).is_some());
        assert!(s.car(6.1, 45, 5.0, 10.0).is_none());
        assert!(s.car(6.5, 45, 20.0, 10.0).is_none());
        assert!(s.car(9.0, 45, 4.0, 10.0).is_some());
        // Out of the client's traffic for a while: forgotten, and heard when it next comes.
        assert!(s.car(10.0, 46, 4.0, 10.0).is_some());
        step(&mut s, 12.0, walking([0.0; 3]));
        assert!(s.car(12.1, 46, 4.0, 10.0).is_some());
        // The boulevard's six lanes busy beside the pilot: each car heard once, however long they
        // stay.
        let mut s = Street::default();
        let mut heard = 0;
        for k in 0..120 {
            let t = k as f64 * FRAME;
            heard += (0..24).filter(|id| s.car(t, *id, 2.0 + *id as f32 * 0.15, 10.0).is_some()).count();
            step(&mut s, t, walking([0.0; 3]));
        }
        assert_eq!(heard, 24);
    }

    #[test]
    fn the_street_sits_under_the_cockpit() {
        // The cockpit flying flat out: its engines and its hum, as the client mixes them.
        let mut c = Cockpit::default();
        let flying =
            CockpitIn { in_world: true, alive: true, thrust: 1.0, propellant: 1.0, ..Default::default() };
        let cockpit: f32 =
            c.frame(0.0, &flying, &mut |_, _| {}).loops.iter().map(|l| l.gain * l.cue.def().gain).sum();
        // The busiest the street gets: on foot in a full square; at the wheel flat out in traffic.
        let mixed = |i: StreetIn| {
            let mut s = Street::default();
            let mut out = step(&mut s, 0.0, i).0;
            for k in 1..600 {
                out = step(&mut s, k as f64 * FRAME, i).0;
            }
            out.loops.iter().map(|l| l.gain * l.cue.def().gain).sum::<f32>()
        };
        let afoot = mixed(StreetIn { traffic: 1.0, crowd: 1.0, ..walking([0.0; 3]) });
        let driving = mixed(StreetIn {
            traffic: 1.0,
            crowd: 1.0,
            driving: true,
            speed: 30.0,
            feet: None,
            ..walking([0.0; 3])
        });
        assert!(afoot < cockpit && driving < cockpit, "{afoot} and {driving} under {cockpit}");
        for cue in LOOPS {
            assert!(cue.def().gain <= Cue::ThrusterLoop.def().gain, "{cue:?}");
        }
        // The pilot's own steps are softer than the suit's.
        assert!(Cue::StreetStep.def().gain < Cue::Footstep.def().gain);
    }
}
