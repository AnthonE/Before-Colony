//! Heat and the Watch in the ring's town, as the client draws it (`docs/OPEN_CITY.md`, "Heat and the
//! Watch"; `docs/FREE_ROAM.md`, 4): what a pilot does in the street draws the Watch's eye, and its
//! units come.
//!
//! - **Heat** is how much the Watch wants a pilot, 0 to 5, and it's the server's: it keeps every
//!   pilot's by `bc_sim::colony::watch`'s tables (what each [`Offence`] adds, the levels, the
//!   cool-down) and says so (`Update::Heat`; everyone else's rides in the plaza's poses). What only
//!   this client sees it do (a car taken from a kerb or its driver, a knock) it reports
//!   (`Request::Offence`), which can only ever add to it.
//! - **The units** are the server's too (`bc-server`'s `dispatch`): its cars and Sirius hounds, sent
//!   out of the Watch's stations by road to its incidents and after whoever it wants, the same for
//!   everyone. It stops a wanted pilot itself and takes the fine. The client is told those near it
//!   a few times a second (`Update::Units`), and [`Pursuit`] draws them, each eased from where it's
//!   drawn to where the server last said it is.

use bc_econ::wire::{Unit, UnitState};
use bc_sim::car::RING_FLOOR;
use bc_sim::colony::deck::{DeckPos, RING, round_from};

use crate::vehicle::{Kind, Vehicle};

pub use bc_sim::colony::watch::{COOL, MAX_HEAT, Offence, UnitKind, units};

/// The Watch's fine at `level` of heat, credits (the server's: `bc_sim::colony::watch::fine`).
pub fn fine(level: u8) -> u64 {
    bc_sim::colony::watch::fine(level)
}

/// The heat as the HUD shows it: a diamond a level ("◆◆"); nothing at 0. (Only the lit ones: the
/// page's fonts have no ◇, and one borrowed from another font draws twice the size.)
pub fn stars(level: u8) -> String {
    "◆".repeat(usize::from(level.min(MAX_HEAT)))
}

/// Whether one of the Watch's own cars on its beat is within `reach` m of `at` at tick `tick`.
pub fn watch_car_near(at: DeckPos, reach: f32, (tick, frac): (u32, f32)) -> bool {
    bc_sim::colony::watch::car_near(at.x, at.s, reach, tick, frac)
}

/// Whether one of the Watch's own cars on its beat within `reach` m of `at` has a clear line to
/// it: nothing of the town's (a building, a lift's pier) between them at eye height.
pub fn watch_car_sees(at: DeckPos, reach: f32, (tick, frac): (u32, f32)) -> bool {
    bc_sim::colony::watch::car_sees(at.x, at.s, reach, tick, frac)
}

/// Whether a car driven at `at` is where people walk: on a block, a pavement or a rim walk.
pub fn where_people_walk(at: DeckPos) -> bool {
    bc_sim::colony::watch::where_people_walk(at.x, at.s)
}

/// How long a word of the server's on its units is drawn towards, s: about till the next comes.
const EASE: f32 = 0.35;
/// Further than this from where it's drawn, m, a unit is put where it's said to be: it can't have
/// driven it.
const SNAP: f32 = 60.0;

/// One of the Watch's units as it's drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    pub id: u16,
    pub kind: UnitKind,
    pub state: UnitState,
    /// Where it's drawn: a car's body; a hound's place over the floor and its heading.
    pub car: Vehicle,
    /// Where it was drawn at the server's last word (`(x, s, heading)`), where that said it is, and
    /// how far from one to the other it's come (0 to 1).
    from: (f32, f32, f32),
    to: (f32, f32, f32),
    u: f32,
}

impl Drawn {
    /// `dt` s further from where it was drawn to where it was said to be.
    fn step_by(&mut self, dt: f32) {
        use std::f32::consts::{PI, TAU};
        self.u = (self.u + dt / EASE).min(1.0);
        let ds = round_from(RING, self.from.1, self.to.1);
        let turn = (self.to.2 - self.from.2 + PI).rem_euclid(TAU) - PI;
        self.car.x = self.from.0 + (self.to.0 - self.from.0) * self.u;
        self.car.s = self.from.1 + ds * self.u;
        self.car.yaw = self.from.2 + turn * self.u;
        self.car.speed = if self.u < 1.0 { (self.to.0 - self.from.0).hypot(ds) / EASE } else { 0.0 };
    }
}

/// The Watch's units near the pilot, as the server says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pursuit {
    pub units: Vec<Drawn>,
    /// The server's words heard so far (`HangarState::units_heard`).
    heard: u64,
}

impl Pursuit {
    /// Nobody near.
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// The server's word on its units near the pilot, the `heard`th (`Update::Units`): each one
    /// eased from where it's drawn to where it's said to be, one new to the pilot put there, and
    /// one no longer said gone. (Heard already, nothing.)
    pub fn hear(&mut self, heard: u64, units: &[Unit]) {
        if heard == self.heard {
            return;
        }
        self.heard = heard;
        self.units.retain(|d| units.iter().any(|u| u.id == d.id));
        for u in units {
            let to = (u.x, u.s, u.heading);
            match self.units.iter_mut().find(|d| d.id == u.id) {
                Some(d) => {
                    let far = (u.x - d.car.x).hypot(round_from(RING, d.car.s, u.s)) > SNAP;
                    let from = if far { to } else { (d.car.x, d.car.s, d.car.yaw) };
                    *d = Drawn { state: u.state, from, to, u: if far { 1.0 } else { 0.0 }, ..*d };
                    if far {
                        d.step_by(0.0);
                    }
                }
                None => self.units.push(Drawn {
                    id: u.id,
                    kind: u.kind,
                    state: u.state,
                    car: Vehicle::new(Kind::Car, RING_FLOOR, u.s, u.x, u.heading),
                    from: to,
                    to,
                    u: 1.0,
                }),
            }
        }
    }

    /// `dt` s on: each further from where it was drawn to where it was said to be.
    pub fn step(&mut self, dt: f32) {
        for d in &mut self.units {
            d.step_by(dt);
        }
    }

    /// Whether one of them, not on its way home, is within `reach` m of `at`.
    pub fn near(&self, at: DeckPos, reach: f32) -> bool {
        self.units.iter().any(|d| {
            d.state != UnitState::Returning
                && (d.car.x - at.x).hypot(round_from(RING, at.s, d.car.s)) <= reach
        })
    }

    /// Forgotten (out of the town).
    pub fn clear(&mut self) {
        self.units.clear();
    }

    /// The Watch's cars, for whoever draws them (and the traffic, which brakes for them).
    pub fn cars(&self) -> impl Iterator<Item = &Vehicle> {
        self.units.iter().filter(|d| d.kind == UnitKind::Car).map(|d| &d.car)
    }

    /// Its hounds: where over the floor each is, and its heading.
    pub fn hounds(&self) -> impl Iterator<Item = &Vehicle> {
        self.units.iter().filter(|d| d.kind == UnitKind::Hound).map(|d| &d.car)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::{street, town};

    fn unit(id: u16, kind: UnitKind, x: f32, s: f32, heading: f32) -> Unit {
        Unit { id, kind, x, s, heading, state: UnitState::Driving }
    }

    #[test]
    fn the_servers_units_are_eased_from_one_word_to_the_next() {
        let s0 = town::deck_s(town::CELL * 6.5);
        let mut p = Pursuit::default();
        assert!(p.is_empty());
        p.hear(1, &[unit(3, UnitKind::Car, 29.0, s0, 0.0), unit(5, UnitKind::Hound, 29.0, s0 + 10.0, 0.0)]);
        assert_eq!((p.cars().count(), p.hounds().count()), (1, 1));
        let at = DeckPos::new(RING, 29.0, s0, 0.0);
        assert!(p.near(at, 1.0));
        // The same word again is nothing new.
        p.hear(1, &[]);
        assert_eq!(p.units.len(), 2);
        // On 5 m and turned: drawn there over the next word's while, not at once.
        p.hear(
            2,
            &[unit(3, UnitKind::Car, 29.0, s0 + 5.0, 0.4), unit(5, UnitKind::Hound, 29.0, s0 + 15.0, 0.0)],
        );
        p.step(EASE * 0.5);
        let car = *p.cars().next().unwrap();
        assert!((car.s - (s0 + 2.5)).abs() < 1e-3 && (car.yaw - 0.2).abs() < 1e-3, "{car:?}");
        assert!(car.speed > 10.0, "{car:?}");
        p.step(EASE);
        let car = *p.cars().next().unwrap();
        assert!((car.s - (s0 + 5.0)).abs() < 1e-3 && car.speed == 0.0, "{car:?}");
        // Said to be somewhere it can't have driven to: put there.
        let round = bc_sim::colony::deck::DECKS[RING as usize].round();
        p.hear(3, &[Unit { state: UnitState::Returning, ..unit(3, UnitKind::Car, 29.0, round - 2.0, 0.0) }]);
        let car = *p.cars().next().unwrap();
        assert!(car.s == round - 2.0 && car.speed == 0.0, "{car:?}");
        // Across Hub Gate the short way round, and the way home isn't near.
        p.hear(4, &[Unit { state: UnitState::Returning, ..unit(3, UnitKind::Car, 29.0, 2.0, 0.0) }]);
        assert_eq!(p.units.len(), 1, "the hound's not said: gone");
        p.step(EASE * 0.5);
        let car = *p.cars().next().unwrap();
        assert!(round_from(RING, 0.0, car.s).abs() < 2.5, "{car:?}");
        assert!(!p.near(DeckPos::new(RING, 29.0, 0.0, 0.0), 10.0));
        p.clear();
        assert!(p.is_empty());
    }

    #[test]
    fn the_watchs_cars_on_their_beats_see_down_their_street() {
        assert!(watch_car_near(
            DeckPos::new(RING, 20.0, town::deck_s(town::CELL * 20.5), 0.0),
            3_000.0,
            (0, 0.0)
        ));
        let mut beat = None;
        street::each_car(-1_000.0, 1_000.0, 0.0, 6_000.0, 0, 0.0, &mut |c| {
            if c.kind == street::Kind::Watch && c.x.abs() > 100.0 {
                beat = beat.or(Some(*c));
            }
        });
        let c = beat.expect("one of the Watch's on a street between the blocks");
        let ahead = DeckPos::new(RING, c.x, town::deck_s(c.t + 30.0), 0.0);
        assert!(watch_car_sees(ahead, 80.0, (0, 0.0)));
        assert!(where_people_walk(DeckPos::new(RING, -300.0, town::deck_s(town::CELL * 30.5), 0.0)));
        assert_eq!((stars(0), stars(2), fine(2)), (String::new(), "◆◆".into(), 600));
    }
}
