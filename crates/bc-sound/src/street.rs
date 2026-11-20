//! What the pilot hears in the ring's town: their own car's engine and tyres, the Watch's sirens
//! coming, the traffic round them and the street itself. A model of the street's state in, loop
//! levels and one-shot cues out, as [`crate::cockpit`] is for the suit, so every rule is a test.
//!
//! Air carries sound down here, so it falls off with distance as it does nowhere else in the
//! game: the siren's level is [`crate::falloff`] of the nearest unit's distance, the traffic's of
//! the nearest moving car's.

use crate::Cue;
use crate::cockpit::LoopLevel;

/// The street's loops, in the order [`StreetOut::loops`] lists them.
pub const LOOPS: [Cue; 5] =
    [Cue::EngineLoop, Cue::TyreLoop, Cue::SirenLoop, Cue::TrafficLoop, Cue::StreetBed];

/// A siren carries this far, m.
pub const SIREN_RADIUS: f32 = 650.0;
/// The traffic is heard within this far of the nearest moving car, m.
pub const TRAFFIC_RADIUS: f32 = 220.0;
/// Gears: the road speed each tops out at, m/s (the last runs to the car's top).
const GEARS: [f32; 4] = [7.0, 13.0, 20.0, 27.0];
/// The engine's playback rate at idle and at the top of a gear.
const IDLE_RATE: f32 = 0.55;
const REDLINE_RATE: f32 = 1.55;
/// Skidding: sideways speed (m/s) where the tyres start to squeal, and where they're at full.
const SCRUB: (f32, f32) = (1.5, 7.0);

/// The street round the pilot this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StreetIn {
    /// Down in the town and in the world (not in a menu's sequence).
    pub in_town: bool,
    /// At the wheel.
    pub driving: bool,
    /// The car's speed along itself, m/s (backwards negative).
    pub speed: f32,
    /// The throttle, -1..1.
    pub throttle: f32,
    /// How fast it's sliding sideways, m/s (the handbrake's turn, a hard corner).
    pub slip: f32,
    /// Times the pilot's car has hit something so far (a new one is a crash).
    pub knocks: u32,
    /// Times the pilot has got in or out of a car so far.
    pub doors: u32,
    /// The horn is held.
    pub horn: bool,
    /// The nearest of the Watch's cars with its siren going, m (none: `f32::MAX`), and how fast
    /// it closes on the pilot, m/s (positive: coming).
    pub siren: f32,
    pub siren_closing: f32,
    /// The nearest moving car of the traffic, m (none: `f32::MAX`), and how many are in earshot.
    pub traffic: f32,
    pub cars: u16,
    /// People in sight.
    pub people: u16,
    /// The colony's daylight, 0 (night) to 1.
    pub daylight: f32,
}

impl StreetIn {
    /// Nothing near: no siren, no traffic.
    pub fn empty() -> Self {
        Self { siren: f32::MAX, traffic: f32::MAX, daylight: 1.0, ..Self::default() }
    }
}

/// What the street sounds like this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreetOut {
    pub loops: [LoopLevel; LOOPS.len()],
}

/// The street's memory between frames.
#[derive(Clone, Copy, Debug, Default)]
pub struct Street {
    prev: Option<StreetIn>,
    next_horn: f64,
    /// The engine's revs as the gearbox has them, 0..1 within the gear, eased.
    revs: f32,
}

/// The gear a car is in at `speed` m/s, and how far through it, 0..1.
pub fn gear(speed: f32) -> (usize, f32) {
    let v = speed.abs();
    let mut lo = 0.0;
    for (k, &top) in GEARS.iter().enumerate() {
        if v < top || k + 1 == GEARS.len() {
            return (k, ((v - lo) / (top - lo)).clamp(0.0, 1.0));
        }
        lo = top;
    }
    (GEARS.len() - 1, 1.0)
}

impl Street {
    /// Steps once a frame, `dt` s on: `cue` is called for every one-shot this frame starts, with
    /// its gain (0..1).
    pub fn frame(&mut self, now: f64, dt: f32, i: &StreetIn, cue: &mut dyn FnMut(Cue, f32)) -> StreetOut {
        let live = i.in_town;
        if let Some(p) = self.prev.filter(|p| p.in_town && live) {
            if i.knocks > p.knocks && i.driving {
                // How hard: what speed it had going into it.
                cue(Cue::Crash, (0.35 + p.speed.abs() / 20.0).min(1.0));
            }
            if i.doors > p.doors {
                cue(Cue::CarDoor, 1.0);
            }
        }
        if live && i.driving && i.horn && now >= self.next_horn {
            cue(Cue::Horn, 1.0);
            self.next_horn = now + 0.4;
        }
        if !i.horn {
            self.next_horn = now;
        }
        self.prev = Some(*i);

        // The engine: revs climb through each gear and drop at the change; the throttle opens
        // it up. Easing keeps a gear change from being a jump.
        let (g, through) = gear(i.speed);
        let load = i.throttle.abs().clamp(0.0, 1.0);
        let want = if i.driving { (through * 0.85 + load * 0.15 + 0.08 * g as f32).min(1.0) } else { 0.0 };
        let k = (dt * 9.0).clamp(0.0, 1.0);
        self.revs += (want - self.revs) * k;
        let on = |b: bool| if b && live { 1.0 } else { 0.0 };
        let level = |cue: Cue| -> LoopLevel {
            let (gain, rate) = match cue {
                Cue::EngineLoop => (
                    on(i.driving) * (0.35 + 0.45 * load + 0.2 * self.revs),
                    IDLE_RATE + (REDLINE_RATE - IDLE_RATE) * self.revs,
                ),
                Cue::TyreLoop => {
                    let s = ((i.slip.abs() - SCRUB.0) / (SCRUB.1 - SCRUB.0)).clamp(0.0, 1.0);
                    (on(i.driving) * s, 0.9 + 0.2 * s)
                }
                Cue::SirenLoop => {
                    // Coming, a little sharp; going, flat (the air's Doppler, gently).
                    let shift = (1.0 + i.siren_closing / 340.0).clamp(0.9, 1.1);
                    (on(true) * crate::falloff(i.siren, SIREN_RADIUS), shift)
                }
                Cue::TrafficLoop => {
                    let busy = (f32::from(i.cars) / 12.0).clamp(0.2, 1.0);
                    (on(true) * crate::falloff(i.traffic, TRAFFIC_RADIUS).sqrt() * busy, 1.0)
                }
                Cue::StreetBed => {
                    // Quieter at night; a crowd round them fills it out.
                    let crowd = (f32::from(i.people) / 20.0).clamp(0.0, 1.0);
                    let hour = 0.45 + 0.55 * i.daylight.clamp(0.0, 1.0);
                    // At the wheel the car's shell keeps some of it out.
                    let shell = if i.driving { 0.55 } else { 1.0 };
                    (on(true) * hour * (0.6 + 0.4 * crowd) * shell, 1.0)
                }
                _ => (0.0, 1.0),
            };
            LoopLevel { cue, gain: gain.clamp(0.0, 1.0), rate: rate.clamp(0.25, 4.0) }
        };
        StreetOut { loops: LOOPS.map(level) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn town() -> StreetIn {
        StreetIn { in_town: true, ..StreetIn::empty() }
    }

    fn run(s: &mut Street, i: &StreetIn, secs: f32) -> (StreetOut, Vec<Cue>) {
        let mut cues = Vec::new();
        let mut out = None;
        let n = (secs * 60.0) as u32;
        for k in 0..n.max(1) {
            out = Some(s.frame(f64::from(k) / 60.0, 1.0 / 60.0, i, &mut |c, _| cues.push(c)));
        }
        (out.unwrap(), cues)
    }

    fn of(o: &StreetOut, cue: Cue) -> LoopLevel {
        *o.loops.iter().find(|l| l.cue == cue).unwrap()
    }

    #[test]
    fn out_of_the_town_its_silent_and_on_foot_theres_no_engine() {
        let (o, cues) = run(&mut Street::default(), &StreetIn::empty(), 1.0);
        assert!(o.loops.iter().all(|l| l.gain == 0.0) && cues.is_empty());
        let (o, _) = run(&mut Street::default(), &town(), 1.0);
        assert_eq!(of(&o, Cue::EngineLoop).gain, 0.0);
        assert!(of(&o, Cue::StreetBed).gain > 0.3, "the street's there");
        // Night is quieter.
        let (n, _) = run(&mut Street::default(), &StreetIn { daylight: 0.0, ..town() }, 1.0);
        assert!(of(&n, Cue::StreetBed).gain < of(&o, Cue::StreetBed).gain);
    }

    #[test]
    fn the_engine_climbs_each_gear_and_drops_at_the_change() {
        let mut s = Street::default();
        let drive = |speed: f32| StreetIn { driving: true, speed, throttle: 1.0, ..town() };
        let (idle, _) = run(&mut s, &StreetIn { throttle: 0.0, ..drive(0.0) }, 1.0);
        let idle = of(&idle, Cue::EngineLoop);
        assert!(idle.gain > 0.3 && idle.rate < 0.75, "{idle:?}");
        let top_of_first = of(&run(&mut s, &drive(6.8), 1.0).0, Cue::EngineLoop).rate;
        let into_second = of(&run(&mut s, &drive(7.3), 1.0).0, Cue::EngineLoop).rate;
        assert!(top_of_first > into_second + 0.3, "{top_of_first} then {into_second}");
        let flat_out = of(&run(&mut s, &drive(29.0), 1.0).0, Cue::EngineLoop);
        assert!(flat_out.rate > 1.4 && flat_out.gain > 0.9, "{flat_out:?}");
        assert_eq!(gear(-3.0).0, 0, "reversing is first");
    }

    #[test]
    fn a_siren_swells_as_the_watch_comes_and_the_traffic_with_cars_near() {
        let mut s = Street::default();
        let far = of(
            &run(&mut s, &StreetIn { siren: 500.0, siren_closing: 10.0, ..town() }, 0.1).0,
            Cue::SirenLoop,
        );
        let near =
            of(&run(&mut s, &StreetIn { siren: 30.0, siren_closing: 10.0, ..town() }, 0.1).0, Cue::SirenLoop);
        let gone = of(&run(&mut s, &StreetIn { siren: 900.0, ..town() }, 0.1).0, Cue::SirenLoop);
        assert!(near.gain > far.gain && far.gain > 0.0 && gone.gain == 0.0);
        assert!(near.rate > 1.0, "coming: sharp");
        let quiet = of(&run(&mut s, &town(), 0.1).0, Cue::TrafficLoop).gain;
        let busy =
            of(&run(&mut s, &StreetIn { traffic: 12.0, cars: 14, ..town() }, 0.1).0, Cue::TrafficLoop).gain;
        assert!(quiet == 0.0 && busy > 0.5, "{quiet} {busy}");
    }

    #[test]
    fn a_crash_a_door_a_horn_and_a_skid() {
        let mut s = Street::default();
        let at = StreetIn { driving: true, speed: 15.0, ..town() };
        run(&mut s, &at, 0.1);
        let (_, cues) = run(&mut s, &StreetIn { knocks: 1, doors: 1, speed: 0.0, ..at }, 0.02);
        assert_eq!(cues, vec![Cue::Crash, Cue::CarDoor]);
        // The horn held: a blast, and again while it's held.
        let (_, cues) = run(&mut s, &StreetIn { horn: true, knocks: 1, doors: 1, ..at }, 1.0);
        assert!((2..=3).contains(&cues.iter().filter(|c| **c == Cue::Horn).count()), "{cues:?}");
        // On foot, the horn's nothing.
        let (_, cues) = run(&mut s, &StreetIn { horn: true, driving: false, knocks: 1, doors: 1, ..at }, 0.5);
        assert!(cues.is_empty(), "{cues:?}");
        let skid = of(&run(&mut s, &StreetIn { slip: 6.0, knocks: 1, doors: 1, ..at }, 0.1).0, Cue::TyreLoop);
        let grip = of(&run(&mut s, &StreetIn { slip: 0.5, knocks: 1, doors: 1, ..at }, 0.1).0, Cue::TyreLoop);
        assert!(skid.gain > 0.7 && grip.gain == 0.0);
    }
}
