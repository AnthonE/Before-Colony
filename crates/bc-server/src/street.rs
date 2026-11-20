//! The ring's town's street as the server keeps it (`docs/FREE_ROAM.md`, P2's first half: the taken
//! set). The town's crowd is a closed form nobody stores (`bc_sim::colony::street`), but whatever a
//! pilot takes out of it is the server's from then on, so every screen agrees: someone a car knocked
//! down (or took away as a fare), a car driven off from its kerb, a car of the traffic taken from its
//! driver, and the cars pilots have left about. Off the sector's tick, like the plaza.
//!
//! **By district** (`content::town::DISTRICTS`): each of the twelve holds its own fixed sets, sized
//! here and never growing (filling one past its size lets the oldest go), and a version that moves
//! whenever any of them does. A session down in the town is sent the district it's in and the one
//! either side ([`Street::update_for`]), each whole when its version moves or the pilot comes near
//! it: past them, nothing of the street is drawn. The traffic's cars taken are filed where they were
//! taken but sent to everyone in the town, since a lane's places go all the way round the ring.
//!
//! **Taken on a pilot's word, checked**: a hit, a fare, a car from a kerb, from the traffic or where it
//! was left are each taken only if they could be, by where the plaza last had the pilot. Whoever
//! holds a car ([`Held`]) leaves it somewhere for anyone ([`Street::leave`]).
//!
//! **Released**: a person knocked down gets up after a while ([`DOWN_TICKS`]); a car left about goes
//! once it has stood out of everyone's sight for a while ([`RELEASE_TICKS`]), and then the kerb it
//! came from has its car again, or the traffic its car, as soon as that place is out of sight too
//! ([`Street::sweep`]).

use std::sync::Mutex;

use bc_econ::wire::{Down, LeftCar, TakenDistrict, Update};
use bc_proto::presence::{PersonPose, RING_DECK};
use bc_sim::TICK_HZ;
use bc_sim::colony::deck::{RING, round_from};
use bc_sim::colony::{street, town};

/// How long someone knocked down lies there before they get up, ticks.
pub const DOWN_TICKS: u32 = 25 * TICK_HZ;
/// The districts round the ring.
pub const DISTRICTS: usize = town::DISTRICT_COUNT as usize;
/// The most of each a district holds at once: people down or away, kerbs empty, the traffic's cars
/// taken, cars left about. One more lets the oldest go (the one down soonest up; the kerb emptied,
/// the car taken or the car left longest ago).
pub const MAX_DOWN: usize = 24;
pub const MAX_KERBS: usize = 32;
pub const MAX_LANES: usize = 16;
pub const MAX_LEFT: usize = 16;
/// How far from the car the person named may have been, m: a pose is up to a few metres old at a
/// car's speed.
const REACH: f32 = 8.0;
/// How far back the hit may have been, ticks, a few at a time.
const BACK: [u32; 4] = [0, 6, 12, 20];
/// How near a car a pilot on foot takes it from, m: at its side (`bc_client_core::town::CAR_USE`),
/// by a pose a few strides old.
pub const AT_HAND: f32 = 8.0;
/// How far across the ring from a pilot the lane of a car of the traffic they take may run, m.
const LANE_REACH: f32 = 4.5;
/// How far from where the plaza has them a pilot leaves their car, m.
pub const LEAVE_REACH: f32 = 15.0;
/// Further than any screen draws the town's people or cars (`bc_client_core::life`'s tiers reach
/// 600 m at most), m: past it, out of that pilot's sight.
pub const SIGHT: f32 = 1_000.0;
/// How long a car left about stands out of everyone's sight before it goes, ticks.
pub const RELEASE_TICKS: u32 = 120 * TICK_HZ;
/// How often what's to be released is looked for, ticks.
const SWEEP_EVERY: u32 = TICK_HZ;

/// Where a car a pilot holds came from: a kerb's bay (`street::kerb`), or the traffic
/// (`street::Car::id`). Its look is that one's, on every screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Kerb(u32),
    Traffic(u32),
}

impl Origin {
    fn of(car: &LeftCar) -> Option<Origin> {
        car.kerb.map(Origin::Kerb).or(car.car.map(Origin::Traffic))
    }
}

/// The car a pilot holds (at its wheel, or just out of it till they say where they left it): where it
/// came from, and, taken where it was left, who left it there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Held {
    pub from: Origin,
    pub left_by: Option<u16>,
}

/// A fixed number of places, sized at start-up and never growing: one more pushed in takes the
/// place of the oldest.
#[derive(Clone, Copy, Debug)]
struct Pool<T: Copy, const N: usize> {
    slots: [Option<T>; N],
}

impl<T: Copy, const N: usize> Default for Pool<T, N> {
    fn default() -> Self {
        Self { slots: [None; N] }
    }
}

impl<T: Copy, const N: usize> Pool<T, N> {
    /// Puts `t` in a free place, or (none free) in the place of the one whose `age` is least, which
    /// is given back.
    fn push(&mut self, t: T, age: impl Fn(&T) -> u32) -> Option<T> {
        if let Some(free) = self.slots.iter_mut().find(|s| s.is_none()) {
            *free = Some(t);
            return None;
        }
        let oldest = (0..N).min_by_key(|&k| self.slots[k].as_ref().map_or(0, &age))?;
        self.slots[oldest].replace(t)
    }

    fn iter(&self) -> impl Iterator<Item = &T> {
        self.slots.iter().flatten()
    }

    fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.slots.iter_mut().flatten()
    }

    /// The first that `f` picks, taken out.
    fn take(&mut self, f: impl Fn(&T) -> bool) -> Option<T> {
        self.slots.iter_mut().find(|s| s.as_ref().is_some_and(&f))?.take()
    }

    /// Lets go of those `keep` doesn't keep (each given to `gone`): how many.
    fn retain(&mut self, mut keep: impl FnMut(&mut T) -> bool, mut gone: impl FnMut(T)) -> usize {
        let mut n = 0;
        for s in &mut self.slots {
            if let Some(t) = s.as_mut()
                && !keep(t)
            {
                gone(*t);
                *s = None;
                n += 1;
            }
        }
        n
    }

    fn len(&self) -> usize {
        self.iter().count()
    }
}

/// A kerb whose car was driven off, or a car of the traffic taken out of its lane: which, the tick
/// it was, and whether the car taken from it is still about (driven, or left somewhere). Till it
/// isn't, its place stays empty.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Mark {
    id: u32,
    at: u32,
    held: bool,
    /// A car of the Watch's on patrol promoted to answer, not taken by a pilot.
    patrol: bool,
}

/// A car left about, and the last tick a pilot in the town had it in sight.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Left {
    car: LeftCar,
    seen: u32,
}

/// One district's sets.
#[derive(Default)]
struct District {
    version: u64,
    down: Pool<Down, MAX_DOWN>,
    kerbs: Pool<Mark, MAX_KERBS>,
    lanes: Pool<Mark, MAX_LANES>,
    left: Pool<Left, MAX_LEFT>,
}

impl District {
    fn view(&self, d: usize) -> TakenDistrict {
        TakenDistrict {
            district: d as u16,
            version: self.version,
            down: self.down.iter().copied().collect(),
            kerbs: self.kerbs.iter().map(|m| m.id).collect(),
            left: self.left.iter().map(|l| l.car).collect(),
        }
    }
}

#[derive(Default)]
struct Inner {
    districts: [District; DISTRICTS],
    /// Moves whenever the traffic's cars taken (in any district) do.
    lanes: u64,
    /// The last car left's id (each a new one).
    left_ids: u32,
    swept: Option<u32>,
}

impl Inner {
    fn is_down(&self, person: u32) -> bool {
        self.districts.iter().any(|d| d.down.iter().any(|p| p.id == person))
    }

    /// Those whose time is up get up.
    fn get_up(&mut self, tick: u32) {
        for d in &mut self.districts {
            if d.down.retain(|p| p.until > tick, |_| ()) > 0 {
                d.version += 1;
            }
        }
    }

    /// `p` is down (or away) in district `d` from now: the one soonest up gets up to make room.
    fn lie(&mut self, d: usize, p: Down) {
        let district = &mut self.districts[d];
        district.down.push(p, |p| p.until);
        district.version += 1;
    }

    /// The car from `from` is no longer about: its place may have its car again (once out of
    /// sight, [`Street::sweep`]).
    fn unhold(&mut self, from: Origin) {
        for d in &mut self.districts {
            let marks = match from {
                Origin::Kerb(k) => d.kerbs.iter_mut().find(|m| m.id == k),
                Origin::Traffic(c) => d.lanes.iter_mut().find(|m| m.id == c),
            };
            if let Some(m) = marks {
                m.held = false;
            }
        }
    }
}

/// What's been taken out of the town's crowd, district by district.
#[derive(Default)]
pub struct Street {
    inner: Mutex<Inner>,
}

/// The district of the ring's town at `s` on the ring's deck.
pub fn district_at(s: f32) -> usize {
    usize::from(town::cell_at(town::town_t(s)) / town::CELLS_PER_DISTRICT)
}

/// The districts round `s` on the ring's deck: the one before, the one it's in, the one after.
pub fn near(s: f32) -> [u16; 3] {
    let (d, n) = (district_at(s) as u16, DISTRICTS as u16);
    [(d + n - 1) % n, d, (d + 1) % n]
}

/// What a session down in the town was last sent of its street ([`Street::update_for`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sent {
    near: Option<[u16; 3]>,
    versions: [Option<u64>; DISTRICTS],
    lanes: Option<u64>,
}

/// On foot, for taking a car.
fn afoot(pose: &PersonPose) -> Result<(), &'static str> {
    if pose.strip != RING_DECK || pose.driving() {
        return Err("on foot, at a car's side: that's where it's taken from");
    }
    Ok(())
}

impl Street {
    fn lock(&self) -> Option<std::sync::MutexGuard<'_, Inner>> {
        self.inner.lock().ok()
    }

    /// A pilot at `pose` (as the plaza last had them) says their car hit `person` at about tick
    /// `tick`: true if it could be, and that person is down from now (false too if they already
    /// are).
    pub fn hit(&self, person: u32, pose: &PersonPose, tick: u32) -> bool {
        if pose.strip != RING_DECK || !pose.driving() {
            return false;
        }
        let t = town::town_t(pose.s);
        let mut found = None;
        for back in BACK {
            street::each_person(
                pose.x - REACH,
                pose.x + REACH,
                t - REACH,
                t + REACH,
                tick.saturating_sub(back),
                0.0,
                &mut |p| {
                    if p.id == person && (p.x - pose.x).hypot(p.t - t) <= REACH {
                        found = found.or(Some(*p));
                    }
                },
            );
            if found.is_some() {
                break;
            }
        }
        let Some(p) = found else { return false };
        let Some(mut inner) = self.lock() else { return false };
        inner.get_up(tick);
        if inner.is_down(person) {
            return false;
        }
        // Where they were, in the deck's own `s`; knocked the way the car was going.
        let s = town::deck_s(p.t);
        inner.lie(
            district_at(s),
            Down { id: p.id, x: p.x, s, yaw: pose.yaw, until: tick + DOWN_TICKS, away: false },
        );
        true
    }

    /// A pilot stopped at `pose` takes `person` as a fare: someone standing in a lift's square
    /// within hail of their car, not down and not already gone. They're off the street from now
    /// (till [`Street::set_down`], or [`FARE_TICKS`]); the lift whose square they stood in.
    pub fn pick_up(&self, person: u32, pose: &PersonPose, tick: u32) -> Option<u16> {
        if pose.strip != RING_DECK || !pose.driving() || pose.speed > 3.0 {
            return None;
        }
        let t = town::town_t(pose.s);
        let mut found = None;
        street::each_person(pose.x - HAIL, pose.x + HAIL, t - HAIL, t + HAIL, tick, 0.0, &mut |p| {
            if p.id == person && p.pose == street::Pose::Stand && (p.x - pose.x).hypot(p.t - t) <= HAIL {
                found = Some(*p);
            }
        });
        let p = found?;
        let from = lift_near(p.x, town::deck_s(p.t), SQUARE)?;
        let mut inner = self.lock()?;
        inner.get_up(tick);
        if inner.is_down(person) {
            return None;
        }
        let s = town::deck_s(p.t);
        inner.lie(
            district_at(s),
            Down { id: p.id, x: p.x, s, yaw: p.yaw, until: tick + FARE_TICKS, away: true },
        );
        Some(from)
    }

    /// A fare set down (or lost): back on the street, where the town has them.
    pub fn set_down(&self, person: u32) {
        let Some(mut inner) = self.lock() else { return };
        for d in &mut inner.districts {
            if d.down.retain(|p| !(p.away && p.id == person), |_| ()) > 0 {
                d.version += 1;
            }
        }
    }

    /// A pilot on foot at `pose` takes the car parked in bay `kerb` at tick `tick`: if there is
    /// one, at their side, and nobody's driven it off already. Its kerb is empty from now.
    pub fn take_parked(&self, kerb: u32, pose: &PersonPose, tick: u32) -> Result<Held, &'static str> {
        afoot(pose)?;
        let p = street::parked(kerb).ok_or("there's no car parked there")?;
        let s = town::deck_s(p.t);
        if apart((p.x, s), (pose.x, pose.s)) > AT_HAND {
            return Err("that car's too far off: at its side");
        }
        let mut inner = self.lock().ok_or("the street's shut")?;
        let d = &mut inner.districts[district_at(s)];
        if d.kerbs.iter().any(|m| m.id == kerb) {
            return Err("that car's gone from its kerb");
        }
        d.kerbs.push(Mark { id: kerb, at: tick, held: true, patrol: false }, |m| m.at);
        d.version += 1;
        Ok(Held { from: Origin::Kerb(kerb), left_by: None })
    }

    /// A pilot on foot at `pose` takes car `car` of the traffic from its driver at tick `tick`: if
    /// its lane runs at their side and its place in it is no further on than one held back for
    /// them could be (`street::PATIENCE`), and nobody has it already. Gone from the traffic from
    /// now; what it was (the Watch minds one of its own).
    pub fn jack(&self, car: u32, pose: &PersonPose, tick: u32) -> Result<(Held, street::Kind), &'static str> {
        afoot(pose)?;
        let c = street::car(car, tick).ok_or("there's no car there")?;
        // How far its place is on past them the way its lane runs (held back, it's behind it).
        let dir = if c.yaw > 1.0 { 1.0 } else { -1.0 };
        let on = dir * round_from(RING, pose.s, town::deck_s(c.t));
        if (c.x - pose.x).abs() > LANE_REACH || !(-AT_HAND..=street::PATIENCE + 60.0).contains(&on) {
            return Err("that car's not at your side");
        }
        let mut inner = self.lock().ok_or("the street's shut")?;
        if inner.districts.iter().any(|d| d.lanes.iter().any(|m| m.id == car)) {
            return Err("somebody's taken that car already");
        }
        let d = &mut inner.districts[district_at(pose.s)];
        d.lanes.push(Mark { id: car, at: tick, held: true, patrol: false }, |m| m.at);
        inner.lanes += 1;
        Ok((Held { from: Origin::Traffic(car), left_by: None }, c.kind))
    }

    /// The traffic's cars out of their lanes, anywhere (`crate::jams` leaves them out), and the
    /// cars left about, each `(x, s, yaw)` (`crate::jams` brakes for them).
    pub fn for_the_traffic(&self) -> (Vec<u32>, Vec<(f32, f32, f32)>) {
        let Some(inner) = self.lock() else { return (Vec::new(), Vec::new()) };
        let lanes = inner.districts.iter().flat_map(|d| d.lanes.iter().map(|m| m.id)).collect();
        let left = inner
            .districts
            .iter()
            .flat_map(|d| d.left.iter().map(|l| (l.car.x, l.car.s, l.car.yaw)))
            .collect();
        (lanes, left)
    }

    /// Car `car` of the traffic, one of the Watch's on patrol, promoted to answer something
    /// (`crate::dispatch`), where it was (the deck's `s`): gone from the traffic for everyone, as a
    /// car taken is, till its place is out of sight.
    pub fn promote(&self, car: u32, s: f32, tick: u32) {
        let Some(mut inner) = self.lock() else { return };
        if inner.districts.iter().any(|d| d.lanes.iter().any(|m| m.id == car)) {
            return;
        }
        let d = &mut inner.districts[district_at(s)];
        d.lanes.push(Mark { id: car, at: tick, held: false, patrol: true }, |m| m.at);
        // (The lanes' version moves, so clients hear it's gone.)
        inner.lanes += 1;
    }

    /// A pilot on foot at `pose` takes the car left about that's `id`: if it's at their side.
    pub fn take_left(&self, id: u32, pose: &PersonPose) -> Result<Held, &'static str> {
        afoot(pose)?;
        let mut inner = self.lock().ok_or("the street's shut")?;
        for d in &mut inner.districts {
            let Some(l) = d.left.iter().find(|l| l.car.id == id) else { continue };
            if apart((l.car.x, l.car.s), (pose.x, pose.s)) > AT_HAND {
                return Err("that car's too far off: at its side");
            }
            let Some(l) = d.left.take(|l| l.car.id == id) else { break };
            d.version += 1;
            let from = Origin::of(&l.car).ok_or("that car's gone")?;
            return Ok(Held { from, left_by: Some(l.car.by) });
        }
        Err("that car's gone")
    }

    /// The pilot `by`, at `pose`, leaves the car they hold at `(x, s)` facing `yaw` at tick `tick`:
    /// there for anyone from now (the district's oldest left car goes, if it's full). Its id.
    pub fn leave(
        &self,
        held: Held,
        by: u16,
        (x, s, yaw): (f32, f32, f32),
        pose: &PersonPose,
        tick: u32,
    ) -> Result<u32, &'static str> {
        if pose.strip != RING_DECK || !x.is_finite() || !s.is_finite() || !yaw.is_finite() {
            return Err("there's nowhere there to leave a car");
        }
        if apart((x, s), (pose.x, pose.s)) > LEAVE_REACH {
            return Err("you left your car further off than that");
        }
        let s = town::deck_s(town::town_t(s));
        let mut inner = self.lock().ok_or("the street's shut")?;
        inner.left_ids = inner.left_ids.wrapping_add(1).max(1);
        let id = inner.left_ids;
        let (kerb, car) = match held.from {
            Origin::Kerb(k) => (Some(k), None),
            Origin::Traffic(c) => (None, Some(c)),
        };
        let car = LeftCar { id, x, s, yaw: yaw.rem_euclid(std::f32::consts::TAU), kerb, car, by, at: tick };
        let d = &mut inner.districts[district_at(s)];
        let gone = d.left.push(Left { car, seen: tick }, |l| l.car.at);
        d.version += 1;
        if let Some(from) = gone.and_then(|l| Origin::of(&l.car)) {
            inner.unhold(from);
        }
        Ok(id)
    }

    /// The car a pilot held is gone (the Watch took it with them, or they left the town without
    /// saying where they left it): its place may have its car again.
    pub fn drop_held(&self, held: Held) {
        if let Some(mut inner) = self.lock() {
            inner.unhold(held.from);
        }
    }

    /// At tick `tick`, with the pilots down in the town at `sights` (`(x, s)` on its floor), at most
    /// once a second: those down whose time is up get up; a car left about that has stood out of
    /// everyone's sight for [`RELEASE_TICKS`] goes; and a kerb or a car of the traffic whose car is
    /// no longer about has its car again, once its place is out of everyone's sight.
    pub fn sweep(&self, tick: u32, sights: &[(f32, f32)]) {
        let Some(mut inner) = self.lock() else { return };
        if inner.swept.is_some_and(|t| tick.wrapping_sub(t) < SWEEP_EVERY) {
            return;
        }
        inner.swept = Some(tick);
        inner.get_up(tick);
        let seen = |x: f32, s: f32| sights.iter().any(|&p| apart((x, s), p) <= SIGHT);
        let mut gone = Vec::new();
        for d in &mut inner.districts {
            let n = d.left.retain(
                |l| {
                    if seen(l.car.x, l.car.s) {
                        l.seen = tick;
                    }
                    tick.wrapping_sub(l.seen) < RELEASE_TICKS
                },
                |l| gone.extend(Origin::of(&l.car)),
            );
            if n > 0 {
                d.version += 1;
            }
        }
        for from in gone {
            inner.unhold(from);
        }
        let mut lanes = 0;
        for d in &mut inner.districts {
            let back = |m: &mut Mark, at: Option<(f32, f32)>| m.held || at.is_none_or(|(x, s)| seen(x, s));
            let kerbs =
                d.kerbs.retain(|m| back(m, street::parked(m.id).map(|p| (p.x, town::deck_s(p.t)))), |_| ());
            if kerbs > 0 {
                d.version += 1;
            }
            lanes += d
                .lanes
                .retain(|m| back(m, street::car(m.id, tick).map(|c| (c.x, town::deck_s(c.t)))), |_| ());
        }
        if lanes > 0 {
            inner.lanes += 1;
        }
    }

    /// What a pilot at `s` on the ring's deck hasn't been sent yet, by `sent` (which then has it), if
    /// anything: the districts round them whose versions moved or that are new to them, and the
    /// traffic's cars taken if that moved.
    pub fn update_for(&self, s: f32, tick: u32, sent: &mut Sent) -> Option<Update> {
        let near = near(s);
        let mut inner = self.lock()?;
        inner.get_up(tick);
        let mut districts = Vec::new();
        for (d, v) in sent.versions.iter_mut().enumerate() {
            if !near.contains(&(d as u16)) {
                *v = None;
            }
        }
        for d in near.map(usize::from) {
            let district = &inner.districts[d];
            if sent.versions[d] != Some(district.version) {
                sent.versions[d] = Some(district.version);
                districts.push(district.view(d));
            }
        }
        let lanes = (sent.lanes != Some(inner.lanes)).then(|| {
            sent.lanes = Some(inner.lanes);
            inner.districts.iter().flat_map(|d| d.lanes.iter().map(|m| m.id)).collect()
        });
        let moved = sent.near != Some(near);
        sent.near = Some(near);
        (moved || !districts.is_empty() || lanes.is_some()).then(|| Update::Street {
            near: near.to_vec(),
            districts,
            lanes,
        })
    }

    /// For `/status` at tick `tick`: how many are lying in the street (not the fares away), and how
    /// many kerbs are empty, cars of the traffic taken and cars left about.
    pub fn counts(&self, tick: u32) -> (usize, usize, usize, usize) {
        let Some(inner) = self.lock() else { return (0, 0, 0, 0) };
        inner.districts.iter().fold((0, 0, 0, 0), |(a, b, c, e), d| {
            (
                a + d.down.iter().filter(|p| p.until > tick && !p.away).count(),
                b + d.kerbs.len(),
                c + d.lanes.iter().filter(|m| !m.patrol).count(),
                e + d.left.len(),
            )
        })
    }

    /// Who's down (or away) at tick `tick`, everywhere.
    pub fn down(&self, tick: u32) -> Vec<Down> {
        let Some(mut inner) = self.lock() else { return Vec::new() };
        inner.get_up(tick);
        inner.districts.iter().flat_map(|d| d.down.iter().copied()).collect()
    }
}

/// How near a stopped car someone hails it from, m; how near a lift's step counts as its square;
/// and how long a fare stays off the street at most, ticks.
pub const HAIL: f32 = 12.0;
pub const SQUARE: f32 = 190.0;
pub const FARE_TICKS: u32 = 20 * 60 * TICK_HZ;
/// How near the lift's step a fare is set down, m.
pub const SET_DOWN: f32 = 60.0;

/// The lift whose step is within `reach` m of `(x, s)` on the ring's floor, if one is.
pub fn lift_near(x: f32, s: f32, reach: f32) -> Option<u16> {
    (0..town::LIFTS).find(|k| {
        let (lx, lt) = town::out_of_the_lift(*k);
        apart((x, s), (lx, town::deck_s(lt))) <= reach
    })
}

/// Where the fare `person` from lift `from`'s square asks to go, and what they pay: another lift's
/// square, one to three round from theirs, [`bc_econ::wire::FARE_PER_KM`] a kilometre of the way.
pub fn fare_for(person: u32, from: u16) -> (u16, u64) {
    let hops = 1 + (town::mix(person, u32::from(from), 0xFA2E) % 3) as u16;
    let back = town::mix(person, 7, 0xFA2E) & 1 == 1;
    let to = if back { (from + town::LIFTS - hops) % town::LIFTS } else { (from + hops) % town::LIFTS };
    let km = f32::from(hops) * town::ROUND / f32::from(town::LIFTS) / 1_000.0;
    (to, (km * bc_econ::wire::FARE_PER_KM as f32).round() as u64)
}

/// What lift `k`'s square is called: Hub Gate, or the district it opens on.
pub fn gate_name(k: u16) -> &'static str {
    town::lift_name(k)
}

/// How far apart two places on the ring's floor are, m (the short way round).
pub fn apart(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(round_from(RING, a.1, b.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::presence::RIDE_CAR;

    /// Standing (or at the wheel: `ride`) at `(x, s)` on the ring's floor.
    fn at(x: f32, s: f32, ride: u8) -> PersonPose {
        PersonPose { strip: RING_DECK, x, s, ride, grounded: true, ..PersonPose::default() }
    }

    /// What `street` sends a pilot at `s` who was last sent `sent`: the districts, and the lanes.
    fn sent_to(
        street: &Street,
        s: f32,
        tick: u32,
        sent: &mut Sent,
    ) -> (Vec<TakenDistrict>, Option<Vec<u32>>) {
        match street.update_for(s, tick, sent) {
            Some(Update::Street { near: n, districts, lanes }) => {
                assert_eq!(n, near(s).to_vec());
                (districts, lanes)
            }
            Some(other) => panic!("{other:?}"),
            None => (Vec::new(), None),
        }
    }

    /// The cars parked in district `d`, by where they stand: `(kerb, x, s)`.
    fn parked_in(d: u16) -> Vec<(u32, f32, f32)> {
        let t0 = town::DISTRICT * f32::from(d);
        let mut all = Vec::new();
        street::each_parked(-1_000.0, 1_000.0, t0 + 1.0, t0 + town::DISTRICT - 1.0, &mut |p| {
            all.push((p.kerb, p.x, town::deck_s(p.t)));
        });
        all
    }

    #[test]
    fn a_hit_is_taken_only_from_a_car_beside_the_person_and_they_get_up_again() {
        // Somebody walking near Hub Gate at tick 900.
        let (x, t) = town::out_of_the_lift(0);
        let mut who = None;
        street::each_person(x - 200.0, x + 200.0, t - 200.0, t + 200.0, 900, 0.0, &mut |p| {
            if p.pose == street::Pose::Walk {
                who = who.or(Some(*p));
            }
        });
        let p = who.expect("somebody about");
        let by = |dx: f32, ride: u8| at(p.x + dx, town::deck_s(p.t), ride);
        let street = Street::default();
        // On foot beside them: no. In a car 40 m off: no. Somebody who isn't there: no.
        assert!(!street.hit(p.id, &by(1.0, 0), 900));
        assert!(!street.hit(p.id, &by(40.0, RIDE_CAR), 900));
        assert!(!street.hit(p.id ^ 0x5555, &by(1.0, RIDE_CAR), 900));
        assert_eq!(street.counts(900).0, 0);
        let mut sent = Sent::default();
        let here = town::deck_s(p.t);
        let (first, _) = sent_to(&street, here, 900, &mut sent);
        assert!(first.iter().all(|d| d.down.is_empty()) && first.len() == 3);
        // In a car at their side, a few ticks on: down, where they were, once.
        assert!(street.hit(p.id, &by(2.0, RIDE_CAR), 905));
        assert!(!street.hit(p.id, &by(2.0, RIDE_CAR), 906), "already down");
        let down = street.down(906);
        assert_eq!(down.len(), 1);
        assert!(apart((down[0].x, down[0].s), (p.x, here)) < 1.5, "{:?} vs {p:?}", down[0]);
        assert_eq!(down[0].until, 905 + DOWN_TICKS);
        assert_eq!((street.counts(906).0, street.counts(905 + DOWN_TICKS).0), (1, 0));
        // Whoever watches that district is sent it again, them in it.
        let (again, _) = sent_to(&street, here, 906, &mut sent);
        assert_eq!(again.len(), 1);
        assert_eq!(
            (again[0].district as usize, again[0].down.as_slice()),
            (district_at(here), down.as_slice())
        );
        // Their time up, they're gone from it, and its version has moved for whoever watches.
        assert!(street.down(905 + DOWN_TICKS + 1).is_empty());
        let (up, _) = sent_to(&street, here, 905 + DOWN_TICKS + 1, &mut sent);
        assert!(up.len() == 1 && up[0].down.is_empty());
        assert!(street.update_for(here, 905 + DOWN_TICKS + 2, &mut sent).is_none(), "nothing new");
    }

    #[test]
    fn a_fare_is_someone_standing_in_a_lifts_square_by_a_stopped_car() {
        let (x, t) = town::out_of_the_lift(0);
        let mut who = None;
        street::each_person(x - 110.0, x - 15.0, t + 15.0, t + 110.0, 0, 0.0, &mut |p| {
            if p.pose == street::Pose::Stand {
                who = who.or(Some(*p));
            }
        });
        let p = who.expect("somebody standing in Hub Gate's square");
        let by =
            |dx: f32, speed: f32, ride: u8| PersonPose { speed, ..at(p.x + dx, town::deck_s(p.t), ride) };
        let street = Street::default();
        // Not on foot, not driving past, not from across the square.
        assert_eq!(street.pick_up(p.id, &by(3.0, 0.0, 0), 0), None);
        assert_eq!(street.pick_up(p.id, &by(3.0, 9.0, RIDE_CAR), 0), None);
        assert_eq!(street.pick_up(p.id, &by(30.0, 0.0, RIDE_CAR), 0), None);
        // Stopped beside them: taken, from Hub Gate's square; gone from the street, and not twice.
        assert_eq!(street.pick_up(p.id, &by(3.0, 0.0, RIDE_CAR), 0), Some(0));
        assert_eq!(street.pick_up(p.id, &by(3.0, 0.0, RIDE_CAR), 1), None);
        assert!(street.down(1).iter().any(|d| d.id == p.id && d.away));
        assert!(!street.hit(p.id, &by(1.0, 9.0, RIDE_CAR), 2), "nobody's there to hit");
        // Where to: another gate, one to three round, paid by the way.
        let (to, pay) = fare_for(p.id, 0);
        assert!(to != 0 && to < town::LIFTS);
        let hops = u64::from(to.min(town::LIFTS - to));
        // (A gate to the next is a sixth of the ring: π km.)
        assert_eq!(pay, (hops as f32 * std::f32::consts::PI * 40.0).round() as u64, "{hops} gates round");
        assert_eq!((gate_name(0), gate_name(1)), ("HUB GATE", "TOWER HILL"));
        // Set down, they're back on the street.
        street.set_down(p.id);
        assert!(street.down(3).is_empty());
        assert_eq!(lift_near(x, town::deck_s(t), 5.0), Some(0));
        assert_eq!(lift_near(x, town::deck_s(t) + 800.0, SQUARE), None);
    }

    #[test]
    fn a_kerbs_car_is_taken_once_from_its_side_and_only_its_districts_neighbours_hear() {
        let cars = parked_in(3);
        let (kerb, x, s) = cars[0];
        let street = Street::default();
        // From a car, from 20 m off, a bay that has no car: no.
        assert!(street.take_parked(kerb, &at(x + 1.0, s, RIDE_CAR), 10).is_err());
        assert!(street.take_parked(kerb, &at(x, s + 20.0, 0), 10).is_err());
        assert!(street.take_parked(kerb ^ 0x8000_0000, &at(x, s, 0), 10).is_err());
        let held = street.take_parked(kerb, &at(x + 1.0, s + 1.0, 0), 10).expect("at its side");
        assert_eq!(held, Held { from: Origin::Kerb(kerb), left_by: None });
        assert!(street.take_parked(kerb, &at(x + 1.0, s, 0), 11).is_err(), "gone already");
        assert_eq!(street.counts(11).1, 1);
        // A pilot in the district, and in the one after: it's empty for both. Two districts on, a
        // pilot's not told of district 3 at all; nor once one has gone there and come away again.
        let next = s + town::DISTRICT;
        let far = s + 2.0 * town::DISTRICT;
        for (spot, told) in [(s, true), (next, true), (far, false)] {
            let mut sent = Sent::default();
            let (districts, _) = sent_to(&street, spot, 12, &mut sent);
            let three = districts.iter().find(|d| d.district == 3);
            assert_eq!(three.is_some(), told, "at {spot}");
            if let Some(three) = three {
                assert_eq!((three.kerbs.as_slice(), three.version), (&[kerb][..], 1));
            }
        }
        let mut sent = Sent::default();
        sent_to(&street, s, 12, &mut sent);
        assert!(street.update_for(s, 13, &mut sent).is_none());
        let (away, _) = sent_to(&street, far, 13, &mut sent);
        assert!(away.iter().all(|d| d.district != 3) && away.len() == 2, "only the new ones: {away:?}");
        let (back, _) = sent_to(&street, s, 14, &mut sent);
        assert!(back.iter().any(|d| d.district == 3 && d.kerbs == [kerb]), "sent again on the way back");
    }

    #[test]
    fn a_districts_sets_never_grow_the_oldest_goes() {
        let cars = parked_in(5);
        assert!(cars.len() > MAX_KERBS + 1, "{} cars parked in district 5", cars.len());
        let street = Street::default();
        for (k, &(kerb, x, s)) in cars.iter().take(MAX_KERBS + 1).enumerate() {
            street.take_parked(kerb, &at(x, s, 0), 100 + k as u32).expect("at its side");
        }
        let mut sent = Sent::default();
        let (districts, _) = sent_to(&street, cars[0].2, 200, &mut sent);
        let five = districts.iter().find(|d| d.district == 5).expect("district 5");
        assert_eq!(five.kerbs.len(), MAX_KERBS, "full, not grown");
        assert!(!five.kerbs.contains(&cars[0].0), "the first taken has its car again");
        assert!(cars[1..=MAX_KERBS].iter().all(|c| five.kerbs.contains(&c.0)));
        // So with people down: the one soonest up gets up to make room.
        let mut inner = street.lock().unwrap();
        for k in 0..=MAX_DOWN as u32 {
            inner.lie(5, Down { id: k, x: 0.0, s: 0.0, yaw: 0.0, until: 1_000 + k, away: false });
        }
        let ids: Vec<u32> = inner.districts[5].down.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), MAX_DOWN);
        assert!(!ids.contains(&0) && ids.contains(&(MAX_DOWN as u32)));
        // And with cars left about: the one left longest ago goes, and its kerb's car can come back.
        drop(inner);
        let held = Held { from: Origin::Kerb(cars[1].0), left_by: None };
        let (x, s) = (cars[1].1, cars[1].2);
        for k in 0..=MAX_LEFT as u32 {
            street.leave(held, 1, (x, s + 0.1 * k as f32, 0.0), &at(x, s, 0), 300 + k).expect("left");
        }
        let inner = street.lock().unwrap();
        let left = &inner.districts[5].left;
        assert_eq!(left.len(), MAX_LEFT);
        assert!(left.iter().all(|l| l.car.at > 300));
        assert!(inner.districts[5].kerbs.iter().any(|m| m.id == cars[1].0 && !m.held));
    }

    #[test]
    fn a_car_left_about_is_anyones_and_goes_out_of_everyones_sight() {
        let cars = parked_in(0);
        let (kerb, x, s) = cars[cars.len() / 2];
        let street = Street::default();
        let held = street.take_parked(kerb, &at(x, s, 0), 0).unwrap();
        // Driven 300 m round and left there, not from 40 m off it.
        let there = (x, s + 300.0, 1.0);
        assert!(street.leave(held, 7, there, &at(x, s + 340.0, 0), 900).is_err());
        let id = street.leave(held, 7, there, &at(x + 2.0, s + 301.0, 0), 900).unwrap();
        let mut sent = Sent::default();
        let (districts, _) = sent_to(&street, s, 901, &mut sent);
        let left: Vec<LeftCar> = districts.iter().flat_map(|d| d.left.clone()).collect();
        assert_eq!(left.len(), 1);
        assert_eq!(
            (left[0].id, left[0].kerb, left[0].car, left[0].by, left[0].at),
            (id, Some(kerb), None, 7, 900)
        );
        assert!((left[0].x - x).abs() < 1e-3 && apart((left[0].x, left[0].s), (x, s + 300.0)) < 1e-2);
        // Somebody else takes it from its side (not from across the street); it's gone for everyone.
        assert!(street.take_left(id, &at(x + 25.0, s + 300.0, 0)).is_err());
        let theirs = street.take_left(id, &at(x - 1.5, s + 300.0, 0)).unwrap();
        assert_eq!(theirs, Held { from: Origin::Kerb(kerb), left_by: Some(7) });
        assert!(street.take_left(id, &at(x - 1.5, s + 300.0, 0)).is_err(), "gone");
        let (districts, _) = sent_to(&street, s, 950, &mut sent);
        assert!(districts.iter().all(|d| d.left.is_empty()) && !districts.is_empty());
        // Left again; a pilot stays near it, so it stays, however long.
        let id = street.leave(theirs, 8, there, &at(x, s + 300.0, 0), 1_000).unwrap();
        let watcher = [(x, s + 500.0)];
        for k in 0..10 {
            street.sweep(1_000 + k * RELEASE_TICKS, &watcher);
        }
        assert_eq!(street.counts(0).3, 1);
        // Out of everyone's sight for long enough, it goes; its kerb's car is back once that's out of
        // sight too (here, as it's going).
        let end = 1_000 + 9 * RELEASE_TICKS;
        let away = [(x, s + 300.0 + 2.0 * SIGHT)];
        street.sweep(end + RELEASE_TICKS - SWEEP_EVERY, &away);
        assert_eq!(street.counts(0).3, 1, "not yet");
        street.sweep(end + RELEASE_TICKS + SWEEP_EVERY, &away);
        assert_eq!(street.counts(0), (0, 0, 0, 0), "car {id} gone, its kerb's car back");
        // But while the kerb's in someone's sight, it stays empty after its car's gone.
        let held = street.take_parked(kerb, &at(x, s, 0), 0).unwrap();
        street.leave(held, 7, there, &at(x, s + 300.0, 0), 10).unwrap();
        street.sweep(10 + 2 * RELEASE_TICKS, &[(x, s - 800.0)]);
        assert_eq!(street.counts(0), (0, 1, 0, 0));
        street.sweep(10 + 3 * RELEASE_TICKS, &[]);
        assert_eq!(street.counts(0), (0, 0, 0, 0));
    }

    #[test]
    fn a_car_of_the_traffic_is_jacked_beside_its_lane_and_everyone_in_the_town_hears() {
        // A street's lane near Hub Gate, and a car on it at tick 5,000.
        let tick = 5_000;
        let (lx, lt) = town::out_of_the_lift(0);
        let mut found = None;
        street::each_car(lx - 300.0, lx - 40.0, lt, lt + 400.0, tick, 0.0, &mut |c| {
            if c.kind != street::Kind::Watch {
                found = found.or(Some(*c));
            }
        });
        let c = found.expect("a car near Hub Gate");
        let dir = if c.yaw > 1.0 { 1.0 } else { -1.0 };
        let cs = town::deck_s(c.t);
        let street = Street::default();
        // Standing in its lane behind its place (it's held back for them): theirs, once.
        assert!(street.jack(c.id, &at(c.x, cs - dir * 30.0, 0), tick).is_ok_and(|(_, k)| k == c.kind));
        assert!(street.jack(c.id, &at(c.x, cs - dir * 30.0, 0), tick).is_err(), "taken already");
        // Not from the pavement 10 m across, nor further behind its place than it's ever held, nor
        // well ahead of it, nor from a car.
        let street = Street::default();
        assert!(street.jack(c.id, &at(c.x + 10.0, cs, 0), tick).is_err());
        assert!(street.jack(c.id, &at(c.x, cs - dir * (street::PATIENCE + 100.0), 0), tick).is_err());
        assert!(street.jack(c.id, &at(c.x, cs + dir * 40.0, 0), tick).is_err());
        assert!(street.jack(c.id, &at(c.x, cs, RIDE_CAR), tick).is_err());
        let (held, _) = street.jack(c.id, &at(c.x, cs - dir * 600.0, 0), tick).unwrap();
        assert_eq!(held.from, Origin::Traffic(c.id));
        // Everyone down in the town hears, wherever they are.
        for spot in [cs, cs + 5.0 * town::DISTRICT] {
            let mut sent = Sent::default();
            assert_eq!(sent_to(&street, spot, tick, &mut sent).1, Some(vec![c.id]));
            assert_eq!(sent_to(&street, spot, tick + 1, &mut sent).1, None, "once");
        }
        // Left and gone out of sight, its place is back in the traffic once that's out of sight.
        street.leave(held, 1, (c.x, cs, 0.0), &at(c.x, cs, 0), tick).unwrap();
        street.sweep(tick + 2 * RELEASE_TICKS, &[]);
        assert_eq!(street.counts(tick).2, 0);
        let mut sent = Sent::default();
        assert_eq!(sent_to(&street, cs, tick + 2 * RELEASE_TICKS, &mut sent).1, Some(vec![]));
    }
}
