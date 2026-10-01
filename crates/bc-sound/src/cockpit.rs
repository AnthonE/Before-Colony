//! What the pilot hears of their own suit: the engines as they're worked, the lock tone building,
//! warnings, the hold and the dock, the ZERO System, the grip and the feet on a body, the reactor
//! going dark in hiding. A model of the cockpit's state in, loop levels and one-shot cues out, so
//! every rule is a test.

use crate::Cue;

/// The cockpit's loops, in the order [`CockpitOut::loops`] lists them.
pub const LOOPS: [Cue; 9] = [
    Cue::ThrusterLoop,
    Cue::BoostLoop,
    Cue::ChargeLoop,
    Cue::SaberHum,
    Cue::LockSolid,
    Cue::MissileAlarm,
    Cue::GStrain,
    Cue::ZeroDrone,
    Cue::CockpitHum,
];

/// The pilot's suit this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CockpitIn {
    /// In the world (the link is up and the suit exists).
    pub in_world: bool,
    pub alive: bool,
    /// How hard the engines are worked, 0..1.
    pub thrust: f32,
    pub boost: bool,
    pub rcs: bool,
    /// Propellant left, 0..1.
    pub propellant: f32,
    /// G-strain, 0..1.
    pub g_strain: f32,
    pub blackout: bool,
    /// The Twin Buster Rifle's charge, 0..1.
    pub charge: f32,
    pub saber: bool,
    /// A missile lock building on a target, 0..1 (0: none).
    pub lock_progress: f32,
    pub locked: bool,
    /// Someone is locking onto the pilot.
    pub warned: bool,
    /// A missile is inbound.
    pub incoming: bool,
    pub docked: bool,
    pub zero: bool,
    pub seized: bool,
    pub jamming: bool,
    pub transforming: bool,
    pub held: bool,
    pub cargo_kg: u32,
    pub credits: u32,
    /// How the suit stands on a body: 0 free, 1 grounded (standing on it), 2 aloft (in its grip),
    /// as the own state has it.
    pub footing: u8,
    /// How fast it came down on landing, m/s (read the frame it's grounded).
    pub touchdown: f32,
    /// Feet put down so far (one per plant).
    pub footfalls: u32,
    /// The grip is armed.
    pub grip: bool,
    /// How well hidden it is: 0 exposed, 1 settling, 2 cold, 3 hidden (sensors have lost it).
    pub cover: u8,
}

/// [`CockpitIn::footing`]: standing on a body.
pub const GROUNDED: u8 = 1;
/// [`CockpitIn::cover`]: hidden.
pub const HIDDEN: u8 = 3;
/// The quickest footsteps come, s.
pub const FOOTSTEP_GAP: f32 = 0.2;
/// A landing this fast (m/s), the most the grip lets a suit come down at, thuds at full.
pub const TOUCHDOWN_FULL: f32 = 8.0;

/// One loop's level and pitch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoopLevel {
    pub cue: Cue,
    /// 0..1, before the cue's own gain and the buses.
    pub gain: f32,
    /// Playback rate.
    pub rate: f32,
}

/// What the cockpit sounds like this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CockpitOut {
    pub loops: [LoopLevel; LOOPS.len()],
    /// The master low-pass, Hz: G-LOC and heavy strain muffle everything.
    pub lowpass_hz: f32,
}

/// Low propellant: warns below this, and stops above [`FUEL_OK`].
pub const FUEL_LOW: f32 = 0.15;
pub const FUEL_OK: f32 = 0.22;

/// The cockpit's memory between frames.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cockpit {
    prev: Option<CockpitIn>,
    next_lock_beep: f64,
    next_fuel_beep: f64,
    next_puff: f64,
    next_step: f64,
    fuel_low: bool,
}

impl Cockpit {
    /// Steps once a frame: `cue` is called for every one-shot this frame starts, with its gain
    /// (0..1).
    pub fn frame(&mut self, now: f64, i: &CockpitIn, cue: &mut dyn FnMut(Cue, f32)) -> CockpitOut {
        let live = i.in_world && i.alive;
        if let Some(p) = self.prev.filter(|p| p.in_world && i.in_world) {
            if p.alive && !i.alive {
                cue(Cue::Destroyed, 1.0);
            }
            if !p.alive && i.alive {
                cue(Cue::Launch, 1.0);
            }
            if live {
                if i.docked && !p.docked {
                    cue(Cue::Dock, 1.0);
                }
                if i.credits > p.credits {
                    cue(Cue::Sale, 1.0);
                }
                if i.held && !p.held {
                    cue(Cue::Grab, 1.0);
                }
                if p.held && !i.held {
                    cue(if i.cargo_kg > p.cargo_kg { Cue::Stow } else { Cue::Throw }, 1.0);
                }
                if i.cargo_kg < p.cargo_kg && !i.docked && i.credits == p.credits {
                    cue(Cue::Jettison, 1.0);
                }
                if i.zero && !p.zero {
                    cue(Cue::ZeroOn, 1.0);
                }
                if i.seized && !p.seized {
                    cue(Cue::Seizure, 1.0);
                }
                if i.jamming != p.jamming {
                    cue(if i.jamming { Cue::JammerOn } else { Cue::JammerOff }, 1.0);
                }
                if i.transforming && !p.transforming {
                    cue(Cue::Transform, 1.0);
                }
                // The grip: armed, or a surface taking hold; disarmed, or letting go. One a frame.
                let (held, was) = (i.footing != 0, p.footing != 0);
                if (i.grip && !p.grip) || (held && !was) {
                    cue(Cue::MagLock, 1.0);
                } else if (p.grip && !i.grip) || (was && !held) {
                    cue(Cue::MagRelease, 1.0);
                }
                if was && !held {
                    cue(Cue::PushOff, 1.0);
                }
                if i.footing == GROUNDED && p.footing != GROUNDED {
                    cue(Cue::Touchdown, (i.touchdown / TOUCHDOWN_FULL).clamp(0.1, 1.0));
                }
                if i.footing == GROUNDED && i.footfalls > p.footfalls && now >= self.next_step {
                    cue(Cue::Footstep, 1.0);
                    self.next_step = now + f64::from(FOOTSTEP_GAP);
                }
                if i.cover == HIDDEN && p.cover != HIDDEN {
                    cue(Cue::GoDark, 1.0);
                } else if p.cover == HIDDEN && i.cover != HIDDEN {
                    cue(Cue::PowerUp, 1.0);
                }
            }
        } else if live {
            // Just arrived in the world.
            cue(Cue::Launch, 1.0);
        }

        // Repeating cues.
        if live && i.lock_progress > 0.0 && !i.locked {
            if now >= self.next_lock_beep {
                cue(Cue::LockBeep, 1.0);
                // Faster as the lock builds: from about twice a second to ten times.
                self.next_lock_beep = now + f64::from(0.45 - 0.35 * i.lock_progress.clamp(0.0, 1.0));
            }
        } else {
            self.next_lock_beep = now;
        }
        if i.propellant < FUEL_LOW {
            self.fuel_low = true;
        } else if i.propellant > FUEL_OK {
            self.fuel_low = false;
        }
        if live && self.fuel_low {
            if now >= self.next_fuel_beep {
                cue(Cue::LowFuel, 1.0);
                self.next_fuel_beep = now + 2.5;
            }
        } else {
            self.next_fuel_beep = now;
        }
        if live && i.rcs {
            if now >= self.next_puff {
                cue(Cue::RcsPuff, 1.0);
                self.next_puff = now + 0.14;
            }
        } else {
            self.next_puff = now;
        }
        self.prev = Some(*i);

        let on = |b: bool| if b && live { 1.0 } else { 0.0 };
        let level = |cue: Cue| -> LoopLevel {
            let (gain, rate) = match cue {
                Cue::ThrusterLoop => {
                    (on(true) * (0.12 + 0.88 * i.thrust.clamp(0.0, 1.0)), 0.85 + 0.3 * i.thrust)
                }
                Cue::BoostLoop => (on(i.boost), 1.0),
                Cue::ChargeLoop => (on(i.charge > 0.01) * (0.3 + 0.7 * i.charge), 0.6 + 1.6 * i.charge),
                Cue::SaberHum => (on(i.saber), 1.0),
                Cue::LockSolid => (on(i.locked), 1.0),
                Cue::MissileAlarm => (on(i.incoming || i.warned), if i.incoming { 1.4 } else { 1.0 }),
                Cue::GStrain => (
                    on(i.g_strain > 0.45) * ((i.g_strain - 0.45) / 0.55).clamp(0.0, 1.0),
                    0.9 + 0.8 * i.g_strain,
                ),
                Cue::ZeroDrone => (on(i.zero || i.seized) * if i.seized { 1.0 } else { 0.7 }, 1.0),
                Cue::CockpitHum => (on(true), 1.0),
                _ => (0.0, 1.0),
            };
            LoopLevel { cue, gain, rate: rate.clamp(0.25, 4.0) }
        };
        let lowpass_hz = if !live {
            20_000.0
        } else if i.blackout {
            350.0
        } else if i.g_strain > 0.6 {
            20_000.0 - (i.g_strain.min(1.0) - 0.6) / 0.4 * 17_000.0
        } else {
            20_000.0
        };
        CockpitOut { loops: LOOPS.map(level), lowpass_hz }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flying() -> CockpitIn {
        CockpitIn { in_world: true, alive: true, propellant: 1.0, ..Default::default() }
    }

    fn step(c: &mut Cockpit, now: f64, i: CockpitIn) -> (CockpitOut, Vec<Cue>) {
        let mut cues = Vec::new();
        let out = c.frame(now, &i, &mut |q, _| cues.push(q));
        (out, cues)
    }

    fn gain(out: &CockpitOut, cue: Cue) -> f32 {
        out.loops.iter().find(|l| l.cue == cue).map_or(0.0, |l| l.gain)
    }

    #[test]
    fn the_engines_follow_the_throttle() {
        let mut c = Cockpit::default();
        let mut last = -1.0;
        for k in 0..=10 {
            let (out, _) = step(&mut c, k as f64, CockpitIn { thrust: k as f32 / 10.0, ..flying() });
            let g = gain(&out, Cue::ThrusterLoop);
            assert!(g > last);
            last = g;
        }
        assert!(last > 0.99);
    }

    #[test]
    fn nothing_on_the_title_screen() {
        let mut c = Cockpit::default();
        let (out, cues) = step(&mut c, 0.0, CockpitIn { thrust: 1.0, boost: true, ..Default::default() });
        assert!(out.loops.iter().all(|l| l.gain == 0.0));
        assert!(cues.is_empty());
    }

    #[test]
    fn arriving_and_dying_and_coming_back() {
        let mut c = Cockpit::default();
        assert_eq!(step(&mut c, 0.0, flying()).1, vec![Cue::Launch]);
        assert!(step(&mut c, 0.1, flying()).1.is_empty(), "once");
        assert_eq!(step(&mut c, 0.2, CockpitIn { alive: false, ..flying() }).1, vec![Cue::Destroyed]);
        assert_eq!(step(&mut c, 5.0, flying()).1, vec![Cue::Launch]);
    }

    #[test]
    fn the_lock_tone_quickens_then_holds() {
        let mut c = Cockpit::default();
        step(&mut c, 0.0, flying());
        let count = |c: &mut Cockpit, from: f64, progress: f32| {
            let mut n = 0;
            let mut t = from;
            while t < from + 2.0 {
                n += step(c, t, CockpitIn { lock_progress: progress, ..flying() })
                    .1
                    .iter()
                    .filter(|q| **q == Cue::LockBeep)
                    .count();
                t += 1.0 / 60.0;
            }
            n
        };
        let slow = count(&mut c, 1.0, 0.1);
        let fast = count(&mut c, 3.0, 0.9);
        assert!(fast > slow * 2, "{slow} then {fast}");
        let (out, cues) = step(&mut c, 6.0, CockpitIn { lock_progress: 1.0, locked: true, ..flying() });
        assert!(!cues.contains(&Cue::LockBeep));
        assert_eq!(gain(&out, Cue::LockSolid), 1.0);
    }

    #[test]
    fn low_fuel_warns_with_hysteresis() {
        let mut c = Cockpit::default();
        step(&mut c, 0.0, flying());
        let beeps = |c: &mut Cockpit, from: f64, prop: f32| {
            (0..300)
                .map(|k| step(c, from + k as f64 / 60.0, CockpitIn { propellant: prop, ..flying() }).1)
                .filter(|q| q.contains(&Cue::LowFuel))
                .count()
        };
        assert_eq!(beeps(&mut c, 1.0, 0.5), 0);
        assert!(beeps(&mut c, 10.0, 0.1) >= 2);
        // Between the two thresholds, still low.
        assert!(beeps(&mut c, 20.0, 0.18) >= 1);
        assert_eq!(beeps(&mut c, 30.0, 0.3), 0);
    }

    #[test]
    fn the_hold_and_the_dock() {
        let mut c = Cockpit::default();
        step(&mut c, 0.0, flying());
        assert_eq!(step(&mut c, 1.0, CockpitIn { held: true, ..flying() }).1, vec![Cue::Grab]);
        assert_eq!(step(&mut c, 2.0, CockpitIn { cargo_kg: 500, ..flying() }).1, vec![Cue::Stow]);
        assert_eq!(step(&mut c, 3.0, CockpitIn { held: true, cargo_kg: 500, ..flying() }).1, vec![Cue::Grab]);
        assert_eq!(step(&mut c, 4.0, CockpitIn { cargo_kg: 500, ..flying() }).1, vec![Cue::Throw]);
        assert_eq!(
            step(&mut c, 5.0, CockpitIn { cargo_kg: 500, docked: true, ..flying() }).1,
            vec![Cue::Dock]
        );
        // Selling empties the hold for credits: a sale, not a jettison.
        assert_eq!(step(&mut c, 6.0, CockpitIn { docked: true, credits: 90, ..flying() }).1, vec![Cue::Sale]);
    }

    #[test]
    fn blackout_muffles_everything() {
        let mut c = Cockpit::default();
        let (out, _) = step(&mut c, 0.0, CockpitIn { blackout: true, g_strain: 1.0, ..flying() });
        assert!(out.lowpass_hz < 1_000.0);
        let (out, _) = step(&mut c, 0.1, flying());
        assert_eq!(out.lowpass_hz, 20_000.0);
    }

    #[test]
    fn touchdown_thuds_once_scaled_by_speed() {
        let mut c = Cockpit::default();
        step(&mut c, 0.0, flying());
        let aloft = CockpitIn { footing: 2, grip: true, ..flying() };
        step(&mut c, 0.1, aloft);
        let mut heard = Vec::new();
        c.frame(0.2, &CockpitIn { footing: GROUNDED, touchdown: 4.0, ..aloft }, &mut |q, g| {
            heard.push((q, g))
        });
        assert_eq!(heard, vec![(Cue::Touchdown, 0.5)]);
        // Standing on, nothing more.
        assert!(step(&mut c, 0.3, CockpitIn { footing: GROUNDED, touchdown: 4.0, ..aloft }).1.is_empty());
        // A hard landing thuds at full, a soft one quietly.
        step(&mut c, 0.4, aloft);
        let mut gain = 0.0;
        c.frame(0.5, &CockpitIn { footing: GROUNDED, touchdown: 30.0, ..aloft }, &mut |_, g| gain = g);
        assert_eq!(gain, 1.0);
        step(&mut c, 0.6, aloft);
        c.frame(0.7, &CockpitIn { footing: GROUNDED, touchdown: 0.2, ..aloft }, &mut |_, g| gain = g);
        assert!(gain > 0.0 && gain < 0.2);
    }

    #[test]
    fn footsteps_follow_footfalls() {
        let mut c = Cockpit::default();
        let ground = CockpitIn { footing: GROUNDED, grip: true, ..flying() };
        step(&mut c, 0.0, ground);
        let count = |c: &mut Cockpit, from: f64, secs: f64, per_s: f64| {
            let mut n = 0;
            let mut k = 0;
            while f64::from(k) / 60.0 < secs {
                let t = from + f64::from(k) / 60.0;
                let falls = ((t - from) * per_s) as u32;
                n += step(c, t, CockpitIn { footfalls: 1_000 + falls, ..ground })
                    .1
                    .iter()
                    .filter(|q| **q == Cue::Footstep)
                    .count();
                k += 1;
            }
            n
        };
        // A walk's 1.5 steps a second: a step heard for each.
        assert_eq!(count(&mut c, 1.0, 4.0, 1.5), 6);
        // Standing still: none.
        assert_eq!(count(&mut c, 10.0, 2.0, 0.0), 0);
        // Faster than the gap allows: no more than one in 0.2 s.
        assert!(count(&mut c, 20.0, 2.0, 20.0) <= 11);
        // Feet in the air make no sound.
        let aloft = CockpitIn { footing: 2, footfalls: 5_000, ..ground };
        assert!(!step(&mut c, 30.0, aloft).1.contains(&Cue::Footstep));
    }

    #[test]
    fn maglock_and_godark_cue_once() {
        let mut c = Cockpit::default();
        step(&mut c, 0.0, flying());
        // Armed: a lock. Caught (still armed): a lock. Down: a thud, no lock.
        assert_eq!(step(&mut c, 1.0, CockpitIn { grip: true, ..flying() }).1, vec![Cue::MagLock]);
        assert_eq!(step(&mut c, 2.0, CockpitIn { grip: true, footing: 2, ..flying() }).1, vec![Cue::MagLock]);
        assert_eq!(
            step(&mut c, 3.0, CockpitIn { grip: true, footing: GROUNDED, touchdown: 3.0, ..flying() }).1,
            vec![Cue::Touchdown]
        );
        // Hidden: dark, once; seen again: up, once.
        let down = CockpitIn { grip: true, footing: GROUNDED, ..flying() };
        assert_eq!(step(&mut c, 4.0, CockpitIn { cover: 1, ..down }).1, vec![]);
        assert_eq!(step(&mut c, 5.0, CockpitIn { cover: HIDDEN, ..down }).1, vec![Cue::GoDark]);
        assert!(step(&mut c, 5.1, CockpitIn { cover: HIDDEN, ..down }).1.is_empty());
        assert_eq!(step(&mut c, 6.0, CockpitIn { cover: 0, ..down }).1, vec![Cue::PowerUp]);
        // Disarmed on the ground: one release, and the push off it.
        assert_eq!(step(&mut c, 7.0, flying()).1, vec![Cue::MagRelease, Cue::PushOff]);
        assert!(step(&mut c, 7.1, flying()).1.is_empty());
    }
}
