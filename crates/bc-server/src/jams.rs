//! The ring's town's traffic held up, the server's (`docs/FREE_ROAM.md`, 3, P2): the closed form's
//! cars round every pilot down in the town, each braked for whatever stands in its lane (the pilots
//! on foot and at the wheel, the cars left about, the Watch's units) and queued behind the car in
//! front, as `bc_client_core::traffic` steps them on a client, here once for everyone. Each session
//! in the town is sent the held cars near it (`Update::Traffic`) and its client takes the server's
//! word for them, predicting between.
//!
//! Off the sector's tick, like the Watch's units: a task steps it ten times a second.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use bc_client_core::traffic::{InTheWay, Traffic};
use bc_econ::wire::HeldCar;
use bc_sim::colony::deck::{DeckPos, RING, round_from};

/// How far round each pilot the traffic is stepped, m (as a client's: `onfoot`'s `TOWN_TRAFFIC`).
pub const REACH: f32 = 230.0;
/// A session is sent the held cars within this far of it, m.
pub const SEND_NEAR: f32 = 400.0;

/// The town's traffic as the server holds it.
#[derive(Default)]
pub struct Jams {
    inner: Mutex<Traffic>,
    version: AtomicU64,
}

impl Jams {
    /// `dt` s on at tick `tick`, round `pilots` (each where they are), with `way` in the lanes.
    pub fn step(&self, pilots: &[DeckPos], tick: u32, dt: f32, way: &[InTheWay]) {
        let Ok(mut t) = self.inner.lock() else { return };
        let before = t.held_cars();
        t.step_round(pilots, REACH, (tick, 0.0), dt, way);
        if t.held_cars() != before {
            self.version.fetch_add(1, Ordering::AcqRel);
        }
    }

    /// The held cars within [`SEND_NEAR`] of `at`.
    pub fn near(&self, at: DeckPos) -> Vec<HeldCar> {
        let Ok(t) = self.inner.lock() else { return Vec::new() };
        t.held_cars()
            .into_iter()
            .filter(|h| round_from(RING, at.s, bc_sim::colony::town::deck_s(h.t)).abs() <= SEND_NEAR)
            .collect()
    }

    /// How many cars are held up now.
    pub fn held(&self) -> usize {
        self.inner.lock().map_or(0, |t| t.held())
    }

    /// Moves whenever which cars are held, or where, does.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }
}

/// One step of the town's traffic: round every pilot down in the town, braked for them (on foot or
/// at the wheel), for the cars left about and for the Watch's units out; the cars taken out of
/// their lanes left out.
pub fn step_town(
    jams: &Jams,
    plaza: &crate::plaza::Plaza,
    street: &crate::street::Street,
    dispatch: &crate::dispatch::Dispatch,
    tick: u32,
    dt: f32,
) {
    use bc_client_core::vehicle::{Kind, Vehicle};
    use bc_sim::car::RING_FLOOR;
    let poses = plaza.town_poses();
    if poses.is_empty() {
        if jams.held() > 0 {
            jams.step(&[], tick, dt, &[]);
        }
        return;
    }
    let pilots: Vec<DeckPos> = poses.iter().map(|p| DeckPos::new(RING, p.x, p.s, 0.0)).collect();
    let (lanes, left) = street.for_the_traffic();
    let mut way: Vec<InTheWay> = poses
        .iter()
        .map(|p| {
            if p.driving() {
                InTheWay::car(&Vehicle::new(Kind::Car, RING_FLOOR, p.s, p.x, p.yaw))
            } else {
                InTheWay::on_foot(p.x, p.s)
            }
        })
        .collect();
    way.extend(
        left.iter().map(|&(x, s, yaw)| InTheWay::car(&Vehicle::new(Kind::Car, RING_FLOOR, s, x, yaw))),
    );
    way.extend(dispatch.seen().iter().filter(|u| u.state != crate::dispatch::State::Home).map(|u| {
        InTheWay::car(&Vehicle::new(Kind::Car, RING_FLOOR, bc_sim::colony::town::deck_s(u.t), u.x, u.yaw))
    }));
    if let Ok(mut t) = jams.inner.lock() {
        t.set_taken(&lanes);
    }
    jams.step(&pilots, tick, dt, &way);
}
