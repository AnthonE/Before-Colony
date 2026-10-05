//! A run of the Proving Ground's course (`bc_sim::colony::course`, `docs/TRAINING.md`), as the
//! pilot's own client keeps it for its HUD: the clock starts when the suit flies through the start
//! ring and stops when it stands on the pad at Hub Gate after every ring in order. Flying the start
//! ring again starts it over; two minutes without a ring lets it lapse.
//!
//! The run itself is `bc_sim`'s ([`Run`]): the server's interior sector keeps one alike for every
//! pilot flying there, and its time is the one the Proving Ground's board keeps. Stepped with the
//! predicted suit's place at the end of each tick (as the server has it), the client's run reads
//! the same time. The best time is also kept with the settings (`course_best_ms`), and the Charter
//! Board's certificate is its class against the par.

pub use bc_sim::colony::course::{Class, Event, LAPSE_TICKS, PAR_S, Run};

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
    use bc_sim::TICK_HZ;
    use bc_sim::colony::course::{GATES, PAD, STRIP, centre, on_pad, way};
    use bc_sim::colony::frame::CityPos;
    use bc_sim::colony::interior::WorldKind;
    use bc_sim::ground::Footing;
    use bc_sim::sim::Loadout;
    use bc_sim::tuning::FlightRules;
    use bc_sim::{Sim, SimConfig};
    use glam::Vec3;

    #[test]
    fn the_clock_reads_minutes_seconds_and_tenths() {
        assert_eq!(clock(0.0), "0:00.0");
        assert_eq!(clock(102.37), "1:42.3");
        assert_eq!(clock(59.99), "0:59.9");
        assert_eq!(clock(-3.0), "0:00.0");
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
