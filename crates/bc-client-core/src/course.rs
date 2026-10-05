//! A run of the Proving Ground's course (`bc_sim::colony::course`, `docs/TRAINING.md`), as the
//! pilot's own client keeps it: the clock starts when the suit flies through the start ring and
//! stops when it stands on the pad at Hub Gate after every ring in order. Flying the start ring again
//! starts it over; two minutes without a ring lets it lapse.
//!
//! Like the objectives it's the client's own and rewards nothing: the best time is kept with the
//! settings (`course_best_ms`), and the Charter Board's certificate is its class against the par.
//! Times are the suit's own clock (the prediction's ticks), with the crossing worked out to the
//! fraction of a tick, so a frame rate can't buy a tenth.

use bc_sim::TICK_HZ;
use bc_sim::colony::course::{GATES, crossed};
use glam::Vec3;

/// The course's par, s. A Leo flown by rote through every ring's middle on flight assist at a
/// steady 80 m/s, then brought to rest over the pad and set down on it, takes about 2:25; it can
/// fly level at 190 m/s (240 boosting), so a pilot who flies a line beats two minutes.
pub const PAR_S: f64 = 120.0;
/// So long without a ring and a run lapses, ticks.
pub const LAPSE_TICKS: f64 = 120.0 * TICK_HZ as f64;

/// The Charter Board's flight certificate, by the time against [`PAR_S`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// Within par.
    First,
    /// Within half as long again.
    Second,
    /// Flown.
    Third,
}

impl Class {
    pub fn of(secs: f64) -> Self {
        if secs <= PAR_S {
            Class::First
        } else if secs <= PAR_S * 1.5 {
            Class::Second
        } else {
            Class::Third
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Class::First => "FIRST CLASS",
            Class::Second => "SECOND CLASS",
            Class::Third => "THIRD CLASS",
        }
    }
}

/// What a frame of the run did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// Through the start ring: the clock runs (again, if it was).
    Started,
    /// Through ring `i` (1 and on).
    Gate(usize),
    /// On the pad after every ring: the time, s.
    Finished(f64),
    /// Too long without a ring.
    Lapsed,
}

/// The run: which ring is next, and since when the clock has run.
#[derive(Clone, Copy, Debug, Default)]
pub struct Run {
    /// Where the suit was last frame, on the suit's clock (ticks).
    last: Option<(Vec3, f64)>,
    /// When the start ring was flown, and the last ring since.
    started: Option<f64>,
    at_gate: f64,
    /// The ring flown next (1 and on while running).
    next: usize,
}

impl Run {
    /// Steps once a frame with where the suit is (the colony's own frame) at tick `t` of its clock,
    /// and whether it stands on the pad.
    pub fn step(&mut self, pos: Vec3, t: f64, on_pad: bool) -> Option<Event> {
        let (a, ta) = self.last.replace((pos, t))?;
        if t <= ta {
            return None;
        }
        let at = |f: f32| ta + (t - ta) * f64::from(f);
        // The start ring starts the clock, and starts it over.
        if let Some(f) = crossed(0, a, pos) {
            self.started = Some(at(f));
            self.at_gate = t;
            self.next = 1;
            return Some(Event::Started);
        }
        let start = self.started?;
        if self.next < GATES.len() {
            if let Some(f) = crossed(self.next, a, pos) {
                self.next += 1;
                self.at_gate = at(f);
                return Some(Event::Gate(self.next - 1));
            }
        } else if on_pad {
            self.reset();
            return Some(Event::Finished((t - start) / f64::from(TICK_HZ)));
        }
        if t - self.at_gate > LAPSE_TICKS {
            self.reset();
            return Some(Event::Lapsed);
        }
        None
    }

    /// Off the course: out of the colony, docked, or anything that moves the suit by more than it
    /// flew.
    pub fn reset(&mut self) {
        self.started = None;
        self.next = 0;
    }

    pub fn running(&self) -> bool {
        self.started.is_some()
    }

    /// The ring flown next while running ([`GATES`]' length: the pad).
    pub fn next(&self) -> Option<usize> {
        self.started.map(|_| self.next)
    }

    /// The clock at tick `t`, s.
    pub fn elapsed(&self, t: f64) -> Option<f64> {
        self.started.map(|s| ((t - s) / f64::from(TICK_HZ)).max(0.0))
    }
}

/// A time as the HUD shows it: `1:42.3`.
pub fn clock(secs: f64) -> String {
    let tenths = (secs.max(0.0) * 10.0).floor() as u64;
    format!("{}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::buttons::{FLIGHT_ASSIST, GRIP};
    use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
    use bc_sim::colony::course::{PAD, STRIP, centre, on_pad, pad_centre, way};
    use bc_sim::colony::frame::CityPos;
    use bc_sim::colony::interior::WorldKind;
    use bc_sim::ground::Footing;
    use bc_sim::sim::Loadout;
    use bc_sim::tuning::FlightRules;
    use bc_sim::{Sim, SimConfig};

    /// Moves straight through every ring's middle, a tick at a time.
    fn through_the_rings(run: &mut Run, t: &mut f64, events: &mut Vec<Event>) {
        for i in 0..GATES.len() {
            let (c, n) = (centre(i), way(i));
            for k in [-3.0, -1.0, 1.0, 3.0] {
                *t += 1.0;
                events.extend(run.step(c + n * k, *t, false));
            }
        }
    }

    #[test]
    fn a_run_starts_at_the_start_ring_counts_the_rings_in_order_and_stops_on_the_pad() {
        let mut run = Run::default();
        let mut t = 0.0;
        let mut events = Vec::new();
        // The pad before the rings counts for nothing.
        assert_eq!(run.step(pad_centre(), 0.0, true), None);
        through_the_rings(&mut run, &mut t, &mut events);
        assert_eq!(events[0], Event::Started);
        assert_eq!(events[1..], (1..GATES.len()).map(Event::Gate).collect::<Vec<_>>()[..]);
        assert_eq!(run.next(), Some(GATES.len()));
        // On the pad: the clock stops. It ran from half way between the ticks either side of the
        // start ring (2 and 3) to the tick the suit stood there.
        t += 30.0;
        let Some(Event::Finished(secs)) = run.step(pad_centre(), t, true) else { panic!("finished") };
        let ticks = t - 2.5;
        assert!((secs - ticks / f64::from(TICK_HZ)).abs() < 1e-9, "{secs}");
        assert!(!run.running());
    }

    #[test]
    fn a_ring_out_of_order_counts_for_nothing_and_the_start_ring_starts_over() {
        let mut run = Run::default();
        let (c0, n0) = (centre(0), way(0));
        run.step(c0 - n0, 1.0, false);
        assert_eq!(run.step(c0 + n0, 2.0, false), Some(Event::Started));
        // Ring 3 before ring 1.
        let (c3, n3) = (centre(3), way(3));
        run.step(c3 - n3, 3.0, false);
        assert_eq!(run.step(c3 + n3, 4.0, false), None);
        assert_eq!(run.next(), Some(1));
        // Back through the start ring: the clock starts over from there.
        run.step(c0 - n0, 100.0, false);
        assert_eq!(run.step(c0 + n0, 101.0, false), Some(Event::Started));
        assert!((run.elapsed(101.0).unwrap() - 0.5 / f64::from(TICK_HZ)).abs() < 1e-9);
    }

    #[test]
    fn a_run_lapses_without_a_ring_and_a_jump_crosses_nothing() {
        let mut run = Run::default();
        let (c0, n0) = (centre(0), way(0));
        run.step(c0 - n0, 1.0, false);
        run.step(c0 + n0, 2.0, false);
        assert!(run.running());
        assert_eq!(run.step(c0 + n0 * 5.0, 2.0 + LAPSE_TICKS + 1.0, false), Some(Event::Lapsed));
        assert!(!run.running());
        // Docked and launched again: a move across the colony, through the start ring, is no
        // flight through it.
        let mut run = Run::default();
        run.step(c0 - n0 * 200.0, 1.0, false);
        assert_eq!(run.step(c0 + n0 * 200.0, 2.0, false), None);
    }

    #[test]
    fn the_clock_reads_minutes_seconds_and_tenths() {
        assert_eq!(clock(0.0), "0:00.0");
        assert_eq!(clock(102.37), "1:42.3");
        assert_eq!(clock(59.99), "0:59.9");
        assert_eq!(clock(-3.0), "0:00.0");
        assert_eq!(Class::of(PAR_S), Class::First);
        assert_eq!(Class::of(PAR_S + 1.0), Class::Second);
        assert_eq!(Class::of(PAR_S * 2.0), Class::Third);
    }

    /// The command flying a suit on flight assist toward `to` at up to `top` m/s, easing in over
    /// the last stretch when `stop` (as `bc-server`'s inside test flies).
    fn toward(pos: Vec3, vel: Vec3, rot: glam::Quat, to: Vec3, top: f32, stop: bool) -> InputCmd {
        let d = to - pos;
        let speed = if stop { (d.length() * 0.3).min(top) } else { top };
        let want = d.normalize_or_zero() * speed;
        let local = rot.conjugate() * (want - vel);
        let q = |v: f32| (v * 6.0).clamp(-127.0, 127.0) as i8;
        InputCmd {
            buttons: FLIGHT_ASSIST,
            thrust: [q(local.x), q(local.y), q(local.z)],
            aim: d.normalize_or(Vec3::X),
            ..InputCmd::default()
        }
    }

    #[test]
    fn a_leo_flies_the_course_from_the_inner_gate_and_lands_on_the_pad() {
        // In the server's interior, its pull, its air and its city, by the anime rules (the
        // server's own): a Leo launched in at the inner gate flies through each ring's middle at a
        // steady 80 m/s, comes to rest over the pad and sets down there with the grip armed. The
        // run sees every ring and stops on the pad. Flown so, by rote and with a careful landing,
        // it's a second-class certificate: the first asks for more. (By the real rules a Leo's tank
        // runs dry holding it up in the air before the turn over the top.)
        let mut sim = Sim::new(SimConfig {
            target_dolls: 0,
            field_rocks: 0,
            landmarks: 0,
            survival: true,
            flight: FlightRules::Anime,
            world: WorldKind::Interior,
            ..SimConfig::default()
        });
        let frame = FrameId::Leo;
        let id = sim.launch(frame, Faction::Colonies, PilotKind::Human, &Loadout::full(frame)).unwrap();
        let i = id.idx();
        let mut run = Run::default();
        let mut events = Vec::new();
        let over_pad = CityPos::new(STRIP, PAD.0, PAD.1, 30.0).to_colony();
        let mut landing = false;
        for _ in 0..TICK_HZ * 400 {
            let t = sim.next_tick();
            let f = sim.suits.flight[i];
            let grounded = sim.suits.footing[i] == Footing::Grounded;
            if let Some(e) = run.step(f.pos, f64::from(t), grounded && on_pad(f.pos)) {
                events.push(e);
                if let Event::Finished(_) = e {
                    break;
                }
            }
            let next = run.next().unwrap_or(0);
            let cmd = if next < GATES.len() {
                // Through the ring's middle, aiming on past it along its way.
                toward(f.pos, f.vel, f.rot, centre(next) + way(next) * 20.0, 80.0, false)
            } else if !landing && (f.pos.distance(over_pad) > 3.0 || f.vel.length() > 2.0) {
                toward(f.pos, f.vel, f.rot, over_pad, 80.0, true)
            } else {
                // At rest over the pad: the grip armed, and the city brings it down.
                landing = true;
                InputCmd { buttons: FLIGHT_ASSIST | GRIP, aim: Vec3::X, ..InputCmd::default() }
            };
            sim.set_input(id, InputCmd { tick: t, view_tick_q4: t << 4, ..cmd });
            sim.step();
        }
        let Some(&Event::Finished(secs)) = events.last() else {
            panic!("finished: {events:?}, at {}", sim.suits.flight[i].pos)
        };
        assert_eq!(events.iter().filter(|e| matches!(e, Event::Gate(_))).count(), GATES.len() - 1);
        println!("the course, flown by rote: {}", clock(secs));
        assert_eq!(Class::of(secs), Class::Second, "{}", clock(secs));
    }
}
