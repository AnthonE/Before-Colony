//! On foot and at the wheel in the ring's town (`docs/FREE_ROAM.md`): down a spoke's lift to Hub
//! Gate's square, the walker on `bc_sim::foot::DeckGround` in the ring's deck's frame, `(x, h, −s)`
//! (`bc_sim::colony::deck`), under its 1 g, all the way round the ring; a car taken from the street,
//! on `bc_sim::car`'s bicycle model against the town's walls; their pose as the plaza has it; the
//! lifts' doors to ride back up by; and the routes a guide walks a pilot along (the dev hooks).

use glam::Vec3;

use crate::vehicle::{DriverPose, Kind, Vehicle};
use crate::walker::Walker;
use bc_proto::presence::{PersonPose, RING_DECK};
use bc_sim::car::RING_FLOOR;
use bc_sim::colony::deck::{self, DECKS, DeckPos, RING};
use bc_sim::colony::town;

pub use bc_sim::foot::DeckGround;

/// The ring's floor, to walk.
pub const GROUND: DeckGround = DeckGround { deck: RING };
/// How near its step a pilot uses a lift from, looking at its door, m (the server takes them up
/// within 8 m of it).
pub const LIFT_USE: f32 = 4.0;
/// How near a car's side a pilot takes it from, m.
pub const CAR_USE: f32 = 2.6;

/// Round the ring at its floor, m.
pub fn round() -> f32 {
    DECKS[RING as usize].round()
}

/// Just out of lift `k`'s door onto its square, facing out across it (fore, −x).
pub fn out_of_the_lift(k: u16) -> Walker {
    let (x, t) = town::out_of_the_lift(k);
    let mut w = Walker::at(DeckPos::new(RING, x, town::deck_s(t), 0.0).walker(), -Vec3::X);
    w.grounded = true;
    w
}

/// Where a walker's feet are on the ring's floor (`s` in its round).
pub fn feet(w: &Walker) -> DeckPos {
    DeckPos::from_walker(RING, w.feet).wrapped()
}

/// Keeps a walker's `s` within the ring's round as they walk on round it: true if it wrapped (a
/// guide's waypoints are a turn off then).
pub fn keep_round(w: &mut Walker) -> bool {
    let round = round();
    let s = -w.feet.z;
    if (0.0..round).contains(&s) {
        return false;
    }
    w.feet.z = -DeckPos::new(RING, 0.0, s, 0.0).wrapped().s;
    true
}

/// A walker in the town, as the plaza has them (`bc_proto::presence`, [`RING_DECK`]).
pub fn pose_of(w: &Walker) -> PersonPose {
    let at = feet(w);
    let flat = Vec3::new(w.vel.x, 0.0, w.vel.z).length();
    PersonPose {
        strip: RING_DECK,
        x: at.x,
        s: at.s,
        h: at.h,
        yaw: w.yaw,
        pitch: w.pitch,
        roll: 0.0,
        speed: flat,
        grounded: w.grounded,
        running: flat > 5.0,
        ride: 0,
        jets: 0,
    }
}

/// A car's driver in the town, as the plaza has them.
pub fn pose_driving(v: &Vehicle) -> PersonPose {
    let p = v.pose(0.0);
    PersonPose { s: DeckPos::new(RING, p.x, p.s, 0.0).wrapped().s, ..p }
}

/// The lift whose door a walker can use: by its step, looking at its door.
pub fn lift_in_view(w: &Walker) -> Option<u16> {
    let at = feet(w);
    (0..town::LIFTS).find(|k| {
        let (x, t) = town::out_of_the_lift(*k);
        let ds = deck::round_from(RING, at.s, town::deck_s(t));
        let near = (at.x - x).hypot(ds) <= LIFT_USE;
        // The door is a pace back (+x) from the step, at head height.
        let (dx, _) = town::lift_door(*k);
        let to_door = Vec3::new(dx - at.x, 1.5 - at.h - bc_sim::foot::EYE, -ds);
        near && w.look().dot(to_door.normalize_or_zero()) > 0.5
    })
}

/// The district round the ring at `s`, and its name on the signs.
pub fn district_at(s: f32) -> (u16, &'static str) {
    let d = town::cell_at(town::town_t(s)) / town::CELLS_PER_DISTRICT;
    (d, town::district(d).name)
}

/// A car for the taking near `at` (the walker's feet): the nearest within [`CAR_USE`] of its side,
/// of the kerbs' parked cars ([`each_parked`], but those `taken` already) and the ones the pilot
/// `left` where they got out (for those, which of them).
pub fn car_at_hand(at: DeckPos, taken: &[(i32, i32)], left: &[Vehicle]) -> Option<(Vehicle, Option<usize>)> {
    let mut best: Option<(f32, Vehicle, Option<usize>)> = None;
    let mut offer = |v: Vehicle, k: Option<usize>| {
        let ds = deck::round_from(RING, at.s, v.s);
        let (fx, fs) = v.forward();
        // How far from its box: along it, and across.
        let along = ((at.x - v.x) * fx - ds * fs).abs() - 0.5 * v.spec().length;
        let across = ((at.x - v.x) * -fs - ds * fx).abs() - 0.5 * v.spec().width;
        let d = along.max(0.0).hypot(across.max(0.0));
        if d <= CAR_USE && best.is_none_or(|(b, ..)| d < b) {
            best = Some((d, v, k));
        }
    };
    each_parked(at.x, at.s, 12.0, |v| {
        if !taken.contains(&parked_id(&v)) {
            offer(v, None);
        }
    });
    for (k, v) in left.iter().enumerate() {
        offer(*v, Some(k));
    }
    best.map(|(_, v, k)| (v, k))
}

/// How many parking bays there are along a cell's length of kerb, and how far apart their middles
/// are, m (`town.wgsl` paints a tick between each): a whole number round the ring, so the same bays
/// whichever way round they're counted.
pub const BAYS_PER_CELL: u16 = 21;
pub const BAY: f32 = town::CELL / BAYS_PER_CELL as f32;
/// How far in from a street's kerb a parked car's middle is, m.
const BAY_IN: f32 = 1.5;

/// The cars parked at the kerbs of the streets between the town's columns, within `reach` m of
/// `(x, s)`: a closed form of the place, the same on every screen. One bay in three or so has a car
/// in it, nose the way its side of the street is driven (traffic keeps right). Each is a
/// [`Vehicle`] standing where it is, in the deck's `s` nearest the asker's.
pub fn each_parked(x: f32, s: f32, reach: f32, mut f: impl FnMut(Vehicle)) {
    let t = town::town_t(s);
    let half = town::STREET * 0.5;
    let bays = i32::from(town::CELLS) * i32::from(BAYS_PER_CELL);
    for c in 0..town::COLUMNS {
        let (_, x1, _, hi) = town::column(c);
        // The street along this column's far edge, if a street is what's there.
        if hi != half || (x1 - x).abs() > reach + half {
            continue;
        }
        let (b0, b1) = (((t - reach) / BAY).floor() as i32, ((t + reach) / BAY).ceil() as i32);
        for g in b0..=b1 {
            let bay = g.rem_euclid(bays);
            let cell = (bay / i32::from(BAYS_PER_CELL)) as u16;
            // Only along a block's side: not across the cross streets at a cell's ends.
            let row = town::block_rect(town::BlockId::new(cell, c));
            let along = (bay as f32 + 0.5) * BAY;
            if along < row.t0 + 10.0 || along > row.t1 - 10.0 {
                continue;
            }
            for side in [-1.0f32, 1.0] {
                let h = town::mix(u32::from(c) * 2 + u32::from(side > 0.0), bay as u32, 0xCA5);
                if !h.is_multiple_of(3) {
                    continue;
                }
                let px = x1 + side * (half - BAY_IN);
                // Keeping right: on the +x side of the street's middle, the way `t` grows.
                let yaw = if side > 0.0 { std::f32::consts::PI } else { 0.0 };
                let ps = s + ((g as f32 + 0.5) * BAY - t);
                if (px - x).hypot(ps - s) <= reach {
                    f(Vehicle::new(Kind::Car, RING_FLOOR, ps, px, yaw));
                }
            }
        }
    }
}

/// Which parked car a [`Vehicle`] from [`each_parked`] is: its street and side, and its bay round
/// the ring (so a client can leave out the one its pilot drove off in).
pub fn parked_id(v: &Vehicle) -> (i32, i32) {
    let bay = (town::wrap(town::town_t(v.s)) / BAY).floor() as i32;
    ((v.x * 2.0).round() as i32, bay)
}

/// The cars near a pilot that their own car and their legs meet: the kerbs' parked ones (but those
/// taken), the ones left about, and the traffic's where the tick has it. Gathered once a frame; each
/// car is three circles down its length.
#[derive(Clone, Debug, Default)]
pub struct Obstacles {
    /// Each car's middle (`x`, and `s` in the asker's turn of the ring), heading, and whether it's
    /// one of the traffic's (moving: nothing on foot is stopped by those).
    cars: Vec<(f32, f32, f32, bool)>,
}

/// A car's circles: their radius, and how far along its length from its middle the outer two are, m.
const CAR_CIRCLE: (f32, f32) = (0.95, 1.3);

impl Obstacles {
    /// The cars within `reach` m of `at` at tick `tick` plus `frac`.
    pub fn gather(
        &mut self,
        at: DeckPos,
        reach: f32,
        taken: &[(i32, i32)],
        left: &[Vehicle],
        (tick, frac): (u32, f32),
    ) {
        self.cars.clear();
        let cars = &mut self.cars;
        each_parked(at.x, at.s, reach, |v| {
            if !taken.contains(&parked_id(&v)) {
                cars.push((v.x, v.s, v.yaw, false));
            }
        });
        for v in left {
            let s = at.s + deck::round_from(RING, at.s, v.s);
            if (v.x - at.x).hypot(s - at.s) <= reach {
                cars.push((v.x, s, v.yaw, false));
            }
        }
        let t = town::town_t(at.s);
        bc_sim::colony::street::each_car(
            at.x - reach,
            at.x + reach,
            t - reach,
            t + reach,
            tick,
            frac,
            &mut |c| {
                cars.push((c.x, at.s + (c.t - t), c.yaw, true));
            },
        );
    }

    fn circles(x: f32, s: f32, yaw: f32) -> [(f32, f32); 3] {
        let (fx, fs) = bc_sim::car::forward(yaw);
        [-1.0f32, 0.0, 1.0].map(|k| (x + fx * k * CAR_CIRCLE.1, s + fs * k * CAR_CIRCLE.1))
    }

    /// Whether a car at `v` runs into any of them.
    pub fn blocks_car(&self, v: &Vehicle) -> bool {
        let mine = Self::circles(v.x, v.s, v.yaw);
        let reach = 2.0 * CAR_CIRCLE.0;
        self.cars.iter().any(|(x, s, yaw, _)| {
            // (The asker's turn of the ring, whichever the car's `s` is in.)
            let s = v.s + deck::round_from(RING, v.s, *s);
            (x - v.x).abs() < 6.0
                && (s - v.s).abs() < 6.0
                && Self::circles(*x, s, *yaw)
                    .iter()
                    .any(|(ox, os)| mine.iter().any(|(mx, ms)| (ox - mx).hypot(os - ms) < reach))
        })
    }

    /// Whether a walker's box `min..max` (the deck's walker frame, `(x, h, −s)`) is in one of the
    /// cars standing about (the traffic's are no wall to anyone on foot).
    pub fn hits(&self, min: Vec3, max: Vec3) -> bool {
        if min.y > 1.45 {
            return false;
        }
        let (s0, s1) = (-max.z, -min.z);
        self.cars.iter().filter(|c| !c.3).any(|(x, s, yaw, _)| {
            let s = s0 + deck::round_from(RING, s0, *s);
            Self::circles(*x, s, *yaw).iter().any(|(cx, cs)| {
                let (dx, ds) = (cx - cx.clamp(min.x, max.x), cs - cs.clamp(s0, s1));
                dx * dx + ds * ds < CAR_CIRCLE.0 * CAR_CIRCLE.0
            })
        })
    }

    pub fn len(&self) -> usize {
        self.cars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cars.is_empty()
    }
}

/// The ring's floor with the cars standing on it, to walk: the town's walls, and the cars'.
pub struct Street<'a>(pub &'a Obstacles);

impl bc_sim::foot::Solid for Street<'_> {
    fn hits(&self, min: Vec3, max: Vec3) -> bool {
        bc_sim::foot::Solid::hits(&GROUND, min, max) || self.0.hits(min, max)
    }

    fn gravity(&self, feet: Vec3) -> f32 {
        bc_sim::foot::Solid::gravity(&GROUND, feet)
    }
}

/// Drives `v` on `dt` s among `cars`: as `Vehicle::step`, but a step into another car is a knock,
/// as a wall is (it stops dead, bounced back a little). One already in another (the traffic drove
/// into it) drives on out. True if it knocked a car.
pub fn drive(v: &mut Vehicle, d: &bc_sim::car::Drive, dt: f32, cars: &Obstacles) -> bool {
    let before = *v;
    let was_in = cars.blocks_car(&before);
    v.step(d, dt);
    let knocked = !was_in && cars.blocks_car(v);
    if knocked {
        *v = Vehicle { speed: before.speed * -0.2, wheel: v.wheel, ..before };
    }
    knocked
}

/// A car the town's people step aside from: a pilot's, where its middle is on the ring's floor,
/// its heading and how fast it goes (back: negative).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mover {
    pub x: f32,
    pub s: f32,
    pub yaw: f32,
    pub speed: f32,
}

impl Mover {
    pub fn of(v: &Vehicle) -> Self {
        Self { x: v.x, s: v.s, yaw: v.yaw, speed: v.speed }
    }
}

/// People mind a car coming at them from this far off, m, within this far either side of its way;
/// they start to step aside after [`REACT`] s, at [`SIDESTEP`] m/s, [`ASIDE`] m at most; and step
/// back over this far behind it.
const MIND: f32 = 6.0;
const MIND_SIDE: f32 = 2.6;
const REACT: f32 = 0.25;
const SIDESTEP: f32 = 3.0;
const ASIDE: f32 = 1.4;
const BEHIND: f32 = 4.0;
const PASSED: f32 = 2.6;
/// A car going this fast knocks down whoever it reaches, m/s; slower, it only shoves past.
pub const HIT_SPEED: f32 = 6.0;

/// How far someone at `(x, s)` has stepped aside from `car`: out of its way, by how long it's been
/// coming at them (so a slow car parts the crowd and a fast one doesn't give them time), and back
/// once it's past. A closed form of where each is: the same on every screen that sees the car.
/// `seed` says which way someone dead ahead goes.
pub fn dodge(x: f32, s: f32, seed: u32, car: &Mover) -> (f32, f32) {
    let speed = car.speed.abs();
    if speed < 1.0 {
        return (0.0, 0.0);
    }
    let (fx, fs) = bc_sim::car::forward(car.yaw);
    let (fx, fs) = if car.speed < 0.0 { (-fx, -fs) } else { (fx, fs) };
    let (rx, rs) = (x - car.x, deck::round_from(RING, car.s, s));
    let ahead = rx * fx + rs * fs;
    let side = rs * fx - rx * fs;
    // (Past them once its tail is: half a car's length and a bit.)
    let past = (-ahead - PASSED).max(0.0);
    if past >= BEHIND || ahead > MIND || side.abs() >= MIND_SIDE {
        return (0.0, 0.0);
    }
    // How long it's been coming at them, and how far that's let them get; easing back behind it.
    let since = (MIND - ahead.max(0.0)) / speed;
    let back = 1.0 - past / BEHIND;
    let reach = ((since - REACT) * SIDESTEP).clamp(0.0, ASIDE) * back * (1.0 - side.abs() / MIND_SIDE);
    let way = if side.abs() > 0.05 {
        side.signum()
    } else if seed & 1 == 0 {
        1.0
    } else {
        -1.0
    };
    (-fs * way * reach, fx * way * reach)
}

/// Whoever of the town's people a car at `v` runs down at tick `tick` plus `frac`: someone it
/// reaches, going [`HIT_SPEED`] or more, wherever they'd stepped aside to. Not those `down` already.
pub fn struck(v: &Vehicle, (tick, frac): (u32, f32), down: &[u32]) -> Option<bc_sim::colony::street::Person> {
    if v.speed.abs() < HIT_SPEED {
        return None;
    }
    let me = Mover::of(v);
    let t = town::town_t(v.s);
    let reach = CAR_CIRCLE.0 + bc_sim::foot::HALF;
    let circles = Obstacles::circles(v.x, v.s, v.yaw);
    let mut hit = None;
    bc_sim::colony::street::each_person(v.x - 7.0, v.x + 7.0, t - 7.0, t + 7.0, tick, frac, &mut |p| {
        if hit.is_some() || down.contains(&p.id) {
            return;
        }
        let s = v.s + (p.t - t);
        let (dx, ds) = dodge(p.x, s, p.seed, &me);
        let (px, ps) = (p.x + dx, s + ds);
        if circles.iter().any(|(cx, cs)| (cx - px).hypot(cs - ps) < reach) {
            hit = Some(*p);
        }
    });
    hit
}

/// Where a guide walks a pilot in the town to from `from` (their feet), by name (the dev hooks'
/// `walk_to`): `lift`, the step at Hub Gate's lift's door; `boulevard`, the boulevard's pavement
/// between the lift's piers; `car`, beside the nearest parked car. The square and its avenue are open
/// ground, so these go straight. The route, and the way to face at its end.
pub fn route(spot: &str, from: Vec3) -> Option<(Vec<Vec3>, Vec3)> {
    let at = DeckPos::from_walker(RING, from);
    let near = |x: f32, t: f32| {
        let s = at.s + deck::round_from(RING, at.s, town::deck_s(t));
        DeckPos::new(RING, x, s, 0.0).walker()
    };
    match spot {
        "lift" => {
            let (x, t) = town::out_of_the_lift(0);
            Some((vec![near(x, t)], Vec3::X))
        }
        "boulevard" => Some((vec![near(-town::BOULEVARD_HALF - 4.0, town::lift_t(0))], Vec3::X)),
        "car" => {
            let mut best: Option<(f32, Vehicle)> = None;
            each_parked(at.x, at.s, 400.0, |v| {
                let d = (v.x - at.x).hypot(v.s - at.s);
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, v));
                }
            });
            let (_, v) = best?;
            // Beside its kerb side: the pavement is towards the block, away from the street's middle.
            let (fx, fs) = v.forward();
            let side = 0.5 * v.spec().width + 1.2;
            let spot = DeckPos::new(RING, v.x + fs * side, v.s - fx * side, 0.0).walker();
            let (cx, cz) = (v.x - spot.x, -v.s - spot.z);
            Some((vec![spot], Vec3::new(cx, 0.0, cz).normalize_or_zero()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::walker::{Guide, Stride};
    use bc_sim::car::Drive;

    fn walk(w: &mut Walker, guide: &mut Guide, secs: f32) -> bool {
        for _ in 0..(secs * 60.0) as u32 {
            let stride = guide.steer(w, 1.0 / 60.0);
            w.step(&GROUND, &stride, 1.0 / 60.0);
            if guide.arrived() {
                return true;
            }
        }
        false
    }

    #[test]
    fn out_of_every_lift_a_pilot_stands_on_its_square_and_can_turn_and_ride_back_up() {
        for k in 0..town::LIFTS {
            let mut w = out_of_the_lift(k);
            let start = w.feet;
            for _ in 0..60 {
                w.step(&GROUND, &Stride::default(), 1.0 / 60.0);
            }
            assert!(w.grounded && (w.feet - start).length() < 0.02, "lift {k}: {:?}", w.feet);
            assert_eq!(lift_in_view(&w), None, "lift {k}: facing away from its door");
            w.turn(std::f32::consts::PI, 0.0);
            assert_eq!(lift_in_view(&w), Some(k), "lift {k}: turned to its door");
            let p = pose_of(&w);
            let (x, t) = town::out_of_the_lift(k);
            assert_eq!(p.strip, RING_DECK);
            assert!(
                (p.x - x).abs() < 1e-3 && (p.s - town::deck_s(t)).abs() < 0.01 && p.h.abs() < 0.02,
                "{p:?}"
            );
        }
        assert_eq!(district_at(town::deck_s(10.0)).1, "CHARTER SQUARE");
        assert_eq!(district_at(town::deck_s(-10.0)).1, "THE BUILDING SITE");
    }

    #[test]
    fn a_pilot_walks_from_the_lift_to_the_boulevard_and_back_and_on_round_the_seam() {
        let mut w = out_of_the_lift(0);
        let (there, facing) = route("boulevard", w.feet).unwrap();
        let mut guide = Guide::new(there, Some(facing));
        assert!(walk(&mut w, &mut guide, 40.0), "to the boulevard: stuck at {:?}", w.feet);
        let (back, facing) = route("lift", w.feet).unwrap();
        let mut guide = Guide::new(back, Some(facing));
        assert!(walk(&mut w, &mut guide, 40.0), "back to the lift: stuck at {:?}", w.feet);
        for _ in 0..60 {
            let stride = guide.steer(&mut w, 1.0 / 60.0);
            w.step(&GROUND, &stride, 1.0 / 60.0);
        }
        assert_eq!(lift_in_view(&w), Some(0), "at {:?} looking {:?}", w.feet, w.look());
        // Along the boulevard's pavement across where the deck's `s` wraps: on they go, kept in its
        // round, the plaza's pose with them.
        let mut w = Walker::at(DeckPos::new(RING, -35.0, round() - 4.0, 0.0).walker(), -Vec3::Z);
        w.grounded = true;
        let mut wrapped = false;
        for _ in 0..(3.0 * 60.0) as u32 {
            w.step(&GROUND, &Stride { forward: 1.0, ..Stride::default() }, 1.0 / 60.0);
            wrapped |= keep_round(&mut w);
        }
        let p = pose_of(&w);
        assert!(wrapped && p.s > 4.0 && p.s < 20.0, "{p:?}");
    }

    #[test]
    fn cars_stand_parked_along_the_streets_the_same_for_everyone_and_one_can_be_driven_off() {
        let (x, t) = town::out_of_the_lift(0);
        let s = town::deck_s(t);
        let mut cars = Vec::new();
        each_parked(x, s, 300.0, |v| cars.push(v));
        assert!(cars.len() > 20, "{} cars within 300 m of Hub Gate's lift", cars.len());
        // Asked again from a little way off (the other side of where the town's `t` wraps), the same
        // cars.
        let mut again = Vec::new();
        each_parked(x + 40.0, s + 25.0, 400.0, |v| again.push(v));
        for v in &cars {
            let same = |a: &Vehicle| {
                (a.x - v.x).abs() < 1e-3 && deck::round_from(RING, a.s, v.s).abs() < 0.02 && a.yaw == v.yaw
            };
            assert!(again.iter().any(same), "{v:?}");
        }
        // Each stands clear on a street's carriageway, by its kerb; no two share a bay.
        for (i, v) in cars.iter().enumerate() {
            assert!(v.fits(v.s, v.x, v.yaw), "{v:?} is in a wall");
            assert_eq!(town::ground_at(v.x, town::town_t(v.s)), town::Ground::Road, "{v:?}");
            for o in &cars[i + 1..] {
                assert!((o.x - v.x).hypot(o.s - v.s) > 4.5, "{v:?} and {o:?}");
                assert_ne!(parked_id(v), parked_id(o));
            }
        }
        // A pilot walked to the nearest stands at its side and can take it; driven off, it goes.
        let mut w = out_of_the_lift(0);
        let (there, facing) = route("car", w.feet).unwrap();
        let mut guide = Guide::new(there, Some(facing));
        assert!(walk(&mut w, &mut guide, 120.0), "to a car: stuck at {:?}", w.feet);
        let (mut car, left) = car_at_hand(feet(&w), &[], &[]).expect("a car at hand");
        assert_eq!(left, None);
        // Taken, its bay is empty; left where the pilot got out, it's at hand there.
        assert!(
            car_at_hand(feet(&w), &[parked_id(&car)], &[])
                .is_none_or(|(v, _)| parked_id(&v) != parked_id(&car))
        );
        assert_eq!(car_at_hand(feet(&w), &[parked_id(&car)], &[car]).map(|(_, k)| k), Some(Some(0)));
        let from = (car.x, car.s);
        for _ in 0..(4.0 * 60.0) as u32 {
            car.step(&Drive { throttle: 1.0, ..Drive::default() }, 1.0 / 60.0);
        }
        assert!((car.s - from.1).abs() > 20.0 && (car.x - from.0).abs() < 0.5, "{car:?}");
        let p = pose_driving(&car);
        assert!(p.driving() && p.strip == RING_DECK && (0.0..round()).contains(&p.s));
    }

    #[test]
    fn the_cars_standing_about_stop_a_walker_and_a_car_and_the_traffic_stops_a_car() {
        let (x, t) = town::out_of_the_lift(0);
        let at = DeckPos::new(RING, x, town::deck_s(t), 0.0);
        let mut near = Vec::new();
        each_parked(at.x, at.s, 300.0, |v| near.push(v));
        let target = near[0];
        let here = DeckPos::new(RING, target.x, target.s, 0.0);
        let mut cars = Obstacles::default();
        cars.gather(here, 40.0, &[], &[], (0, 0.0));
        assert!(!cars.is_empty());
        // A walker walked at its side from the pavement stops short of it; with it taken, walks on.
        let (fx, fs) = target.forward();
        let start = DeckPos::new(RING, target.x + fs * 4.0, target.s - fx * 4.0, 0.0);
        let walk = |cars: &Obstacles| {
            let to_car = Vec3::new(target.x - start.x, 0.0, -(target.s - start.s)).normalize();
            let mut w = Walker::at(start.walker(), to_car);
            w.grounded = true;
            for _ in 0..(4.0 * 60.0) as u32 {
                w.step(&Street(cars), &Stride { forward: 1.0, ..Stride::default() }, 1.0 / 60.0);
            }
            (feet(&w).x - target.x).hypot(feet(&w).s - target.s)
        };
        let stopped = walk(&cars);
        assert!((1.0..1.6).contains(&stopped), "stopped {stopped} m from its middle");
        let mut gone = Obstacles::default();
        gone.gather(here, 40.0, &[parked_id(&target)], &[], (0, 0.0));
        assert!(walk(&gone) > 3.0, "through where it stood");
        // A car left out on Hub Gate's square (nothing else about): one driven at it from behind
        // knocks against it and stops; never inside it.
        let left =
            Vehicle::new(Kind::Car, RING_FLOOR, town::deck_s(60.0), -100.0, std::f32::consts::FRAC_PI_2);
        let by = DeckPos::new(RING, left.x, left.s, 0.0);
        let mut square = Obstacles::default();
        square.gather(by, 25.0, &[], &[left], (0, 0.0));
        assert_eq!(square.len(), 1);
        let mut v = Vehicle::new(Kind::Car, RING_FLOOR, left.s, left.x - 22.0, left.yaw);
        let mut knocked = false;
        for _ in 0..(8.0 * 60.0) as u32 {
            let before = v.speed;
            drive(&mut v, &Drive { throttle: 1.0, ..Drive::default() }, 1.0 / 60.0, &square);
            knocked |= before > 2.0 && v.speed < 0.0;
            assert!(!square.blocks_car(&v), "inside it: {v:?}");
        }
        assert!(knocked, "{v:?}");
        assert!(v.x < left.x - 3.0 && v.x > left.x - 8.0, "{v:?}");
        // The traffic's cars are there to knock against too: one on the boulevard, where the tick
        // has it.
        let mut on_the_boulevard = None;
        bc_sim::colony::street::each_car(0.0, 32.0, t, t + 300.0, 900, 0.0, &mut |c| {
            on_the_boulevard = on_the_boulevard.or(Some(*c));
        });
        let c = on_the_boulevard.expect("a car on the boulevard");
        let by = DeckPos::new(RING, c.x, at.s + (c.t - t), 0.0);
        let mut cars = Obstacles::default();
        cars.gather(by, 30.0, &[], &[], (900, 0.0));
        assert!(cars.blocks_car(&Vehicle::new(Kind::Car, RING_FLOOR, by.s + 1.0, by.x, c.yaw)));
        assert!(!cars.blocks_car(&Vehicle::new(Kind::Car, RING_FLOOR, by.s, by.x + 6.0, c.yaw)));
        let w = by.walker();
        assert!(!cars.hits(w - Vec3::new(0.3, 0.0, 0.3), w + Vec3::new(0.3, 1.8, 0.3)), "no wall on foot");
    }

    #[test]
    fn people_step_aside_from_a_slow_car_and_a_fast_one_runs_them_down() {
        use bc_sim::colony::street;
        // Somebody walking a block's pavement near Hub Gate, and a car driven straight at them from
        // 20 m back along the way they walk.
        let (x, t) = town::out_of_the_lift(0);
        let mut who = None;
        street::each_person(x - 200.0, x + 200.0, t + 30.0, t + 200.0, 0, 0.0, &mut |p| {
            if p.pose == street::Pose::Walk && p.speed > 0.5 {
                who = who.or(Some(*p));
            }
        });
        let p = who.expect("somebody walking");
        let s = town::deck_s(p.t);
        let (fx, fs) = bc_sim::car::forward(p.yaw);
        let run = |speed: f32| {
            // (A frozen moment: the person where tick 0 has them, the car closing at `speed`.)
            let mut least = f32::MAX;
            let mut hit = false;
            let mut d = 20.0;
            while d > -8.0 {
                let car =
                    Vehicle { speed, ..Vehicle::new(Kind::Car, RING_FLOOR, s - fs * d, p.x - fx * d, p.yaw) };
                let (dx, ds) = dodge(p.x, s, p.seed, &Mover::of(&car));
                // How far off the car's line they are as its nose reaches them.
                if d.abs() < 2.0 {
                    least = least.min(dx.hypot(ds));
                }
                hit |= struck(&car, (0, 0.0), &[]).is_some_and(|h| h.id == p.id);
                d -= speed / 60.0;
            }
            (least, hit)
        };
        let (aside, hit) = run(3.0);
        assert!(aside > 1.2 && !hit, "a slow car: {aside} m aside, hit {hit}");
        let (aside, hit) = run(18.0);
        assert!(aside < 0.4 && hit, "a fast one: {aside} m aside, hit {hit}");
        // Already down, nobody's hit twice; and nobody minds a car standing still or one going away.
        let on_them = Vehicle { speed: 18.0, ..Vehicle::new(Kind::Car, RING_FLOOR, s, p.x, p.yaw) };
        assert!(struck(&on_them, (0, 0.0), &[]).is_some());
        assert!(struck(&on_them, (0, 0.0), &[p.id]).is_none_or(|h| h.id != p.id));
        let parked = Mover { speed: 0.0, ..Mover::of(&on_them) };
        assert_eq!(dodge(p.x + 0.5, s, 0, &parked), (0.0, 0.0));
        let gone = Mover { x: p.x + fx * 12.0, s: s + fs * 12.0, ..Mover::of(&on_them) };
        assert_eq!(dodge(p.x, s, 0, &gone), (0.0, 0.0));
    }
}
