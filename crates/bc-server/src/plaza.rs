//! The colony's people on foot ("the plaza"), kept off the sector's tick like the hangar: the pose
//! each pilot in the city last sent, checked against the city before anyone else sees it, and who
//! each pilot sees (the people on their strip near them, nearest first).
//!
//! A pose is taken only if it could be: on the pilot's own strip, within the colony, not inside a
//! wall (`bc_sim::colony::city::solid`, the walls every client walks into), no further from the
//! last one taken than a running pilot (or a car) could go, and, the first, near the strip's Hub
//! Gate, where the lift comes down; on a tram only from beside its open doors, and in a car only
//! from a motor pool. Anything else isn't passed on.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bc_proto::presence::{MAX_PEOPLE, PersonPose, PlazaWriter, RIDER_S};
use bc_sim::colony::city::{MAX_HEIGHT, Stage, place_door, solid};
use bc_sim::colony::frame::{STRIP_WIDTH, STRIPS, within_caps};
use bc_sim::colony::pools::pool_near;
use bc_sim::colony::transit::{self, CAR_WIDTH, TRAIN_LENGTH, TRAINS};
use bc_sim::content::city::{PLACES, PlaceKind};
use glam::Vec3;

/// Faster than anyone runs (the walker's run is 7 m/s), with room for a late packet on top, m/s.
const MAX_SPEED: f32 = 9.0 * 1.5;
/// Faster than anything drives (a car's top speed is 30 m/s), likewise.
const MAX_DRIVE: f32 = 35.0 * 1.3;
const SLACK: f32 = 2.0;
/// Standing still earns no more reach than this long's worth, s. (Reach is measured from when the
/// pilot last moved, not from their last pose: a slow client repeats one pose many times between
/// frames, then jumps a frame's walk at once.)
const STILL: f32 = 2.0;
/// A pilot's first pose must be this near their strip's Hub Gate, m: everyone comes down its lift.
const ARRIVAL: f32 = 150.0;
/// Someone not heard from this long isn't shown, s.
const HIDE: f32 = 5.0;
/// How far a pilot sees others, m.
const NEAR: f32 = 1_500.0;
/// Getting on or off a tram: how near its body the pilot must be (m), and how far either side of
/// the sector's tick its doors may have been open (ticks: the pilot's clock is their own).
const DOORSTEP: f32 = 8.0;
const DOOR_SLACK: u32 = 90;

/// What became of a pose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Taken,
    /// Older than one already taken (reordered).
    Stale,
    /// It couldn't be: not passed on.
    Implausible,
    /// Not in the city.
    Away,
}

struct Person {
    name: String,
    strip: u8,
    pose: Option<PersonPose>,
    seq: u16,
    heard: Instant,
    /// When their pose last moved (no earlier than `STILL` before `heard`).
    moved_at: Instant,
    refused: u32,
    /// When a refusal of theirs was last logged.
    said: Option<Instant>,
}

/// Everyone in the city, by client slot, and how many poses were refused, by why.
#[derive(Default)]
pub struct Plaza {
    people: Mutex<HashMap<u16, Person>>,
    refused_by: Mutex<[u32; WHYS.len()]>,
}

/// Why a pose wasn't passed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Why {
    /// Not the pilot's strip.
    Strip,
    /// Inside a wall, outside the colony, or (riding) outside the train's cars.
    Wall,
    /// Further from the last one than they could have gone.
    Reach,
    /// A first pose away from Hub Gate.
    Arrival,
    /// On a train without having stood by its open doors.
    Board,
    /// Off a train away from its open doors.
    Alight,
    /// In a vehicle not from a motor pool.
    Pool,
    /// From a train into a vehicle, or one train to another.
    Mode,
}

const WHYS: [&str; 8] = ["strip", "wall", "reach", "arrival", "board", "alight", "pool", "mode"];

/// Where strip `strip`'s Hub Gate lets people out: `(s, x)`.
fn hub_gate(strip: u8) -> (f32, f32) {
    PLACES
        .iter()
        .find(|p| p.kind == PlaceKind::HubGate && p.strip == strip)
        .map_or((STRIP_WIDTH * 0.5, -15_900.0), |p| place_door(p).0)
}

/// How someone gets about.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Foot,
    Train(u8),
    Drive(u8),
}

fn mode(p: &PersonPose) -> Mode {
    match p.riding() {
        Some(k) => Mode::Train(k),
        None if p.driving() => Mode::Drive(p.ride),
        None => Mode::Foot,
    }
}

/// Where someone is on their strip, `(s, x)`, at tick `tick` (a rider: where their train is).
fn place(p: &PersonPose, tick: u32) -> (f32, f32) {
    match p.riding() {
        Some(k) => {
            let t = transit::train(p.strip, k, tick, 0.0);
            (t.s + p.s - RIDER_S, t.x + p.x)
        }
        None => (p.s, p.x),
    }
}

/// Whether train `k` of strip `strip`'s line stood with its doors open near `(s, x)` around
/// `tick`.
fn at_open_doors(strip: u8, k: u8, s: f32, x: f32, tick: u32) -> bool {
    (tick.saturating_sub(DOOR_SLACK)..=tick + DOOR_SLACK).step_by(10).any(|t| {
        let tr = transit::train(strip, k, t, 0.0);
        let dx = ((x - tr.x).abs() - 0.5 * TRAIN_LENGTH).max(0.0);
        let ds = ((s - tr.s).abs() - 0.5 * CAR_WIDTH).max(0.0);
        tr.doors && dx.hypot(ds) <= DOORSTEP
    })
}

/// Whether a rider's place in their train could be: inside its cars.
fn aboard(p: &PersonPose) -> bool {
    p.x.abs() <= 0.5 * TRAIN_LENGTH + 0.5
        && (p.s - RIDER_S).abs() <= 0.5 * CAR_WIDTH + 0.3
        && (-0.5..=3.0).contains(&p.h)
        && p.riding().is_some_and(|k| u32::from(k) < TRAINS)
}

/// Whether a pose could be anyone's: inside the colony, its body out of the walls (a thin core of
/// it: a step's worth of the walker's box may brush one).
fn possible(p: &PersonPose) -> bool {
    if p.riding().is_some() {
        return aboard(p);
    }
    let inside =
        within_caps(p.x) && (0.0..=STRIP_WIDTH).contains(&p.s) && (-4.0..=MAX_HEIGHT + 4.0).contains(&p.h);
    inside
        && !solid(
            p.strip,
            Vec3::new(p.x - 0.05, p.h + 0.4, -p.s - 0.05),
            Vec3::new(p.x + 0.05, p.h + 1.4, -p.s + 0.05),
            Stage(0),
        )
}

impl Plaza {
    /// `id` came down to strip `strip`'s Hub Gate.
    pub fn enter(&self, id: u16, name: &str, strip: u8) {
        if let Ok(mut all) = self.people.lock() {
            let strip = strip % STRIPS as u8;
            all.insert(
                id,
                Person {
                    name: name.into(),
                    strip,
                    pose: None,
                    seq: 0,
                    heard: Instant::now(),
                    moved_at: Instant::now(),
                    refused: 0,
                    said: None,
                },
            );
        }
    }

    /// `id` left the city (up the lift, or for good).
    pub fn leave(&self, id: u16) {
        if let Ok(mut all) = self.people.lock() {
            all.remove(&id);
        }
    }

    /// A pose `id` sent, heard at `now`, when the sector was at tick `tick`.
    pub fn accept(&self, id: u16, seq: u16, pose: PersonPose, now: Instant, tick: u32) -> Verdict {
        let Ok(mut all) = self.people.lock() else { return Verdict::Away };
        let Some(me) = all.get_mut(&id) else { return Verdict::Away };
        let why = match me.pose {
            _ if pose.strip != me.strip => Some(Why::Strip),
            _ if !possible(&pose) => Some(Why::Wall),
            Some(_) if (seq.wrapping_sub(me.seq) as i16) <= 0 => return Verdict::Stale,
            Some(last) => {
                let dt = now.saturating_duration_since(me.moved_at).as_secs_f32();
                let moved = |speed: f32| {
                    let reach = speed * dt + SLACK;
                    (pose.x - last.x).hypot(pose.s - last.s) <= reach && (pose.h - last.h).abs() <= reach
                };
                let check = |ok: bool, why: Why| (!ok).then_some(why);
                match (mode(&last), mode(&pose)) {
                    // Walking, on the ground or in a car of a train.
                    (Mode::Foot, Mode::Foot) => check(moved(MAX_SPEED), Why::Reach),
                    (Mode::Train(a), Mode::Train(b)) => check(a == b && moved(MAX_SPEED), Why::Reach),
                    // Getting on: from beside its open doors.
                    (Mode::Foot, Mode::Train(k)) => {
                        check(at_open_doors(pose.strip, k, last.s, last.x, tick), Why::Board)
                    }
                    // Getting off: out of its open doors.
                    (Mode::Train(k), Mode::Foot) => {
                        check(at_open_doors(pose.strip, k, pose.s, pose.x, tick), Why::Alight)
                    }
                    // Driving: no faster than a car goes; taken from a motor pool; left anywhere.
                    (Mode::Drive(a), Mode::Drive(b)) => check(a == b && moved(MAX_DRIVE), Why::Reach),
                    (Mode::Foot, Mode::Drive(_)) => {
                        check(pool_near(pose.strip, last.s, last.x).is_some() && moved(MAX_SPEED), Why::Pool)
                    }
                    (Mode::Drive(_), Mode::Foot) => check(moved(MAX_DRIVE), Why::Reach),
                    // From a train to a car, or one train to another.
                    _ => Some(Why::Mode),
                }
            }
            None => {
                let (s, x) = hub_gate(me.strip);
                let ok = pose.riding().is_none() && (pose.x - x).hypot(pose.s - s) <= ARRIVAL;
                (!ok).then_some(Why::Arrival)
            }
        };
        if let Some(why) = why {
            // Said now and then (never where: only why, how far and how long since the last).
            if me.said.is_none_or(|t| now.saturating_duration_since(t).as_secs_f32() > 5.0) {
                me.said = Some(now);
                let dt = now.saturating_duration_since(me.moved_at).as_secs_f32();
                let moved = me.pose.map_or(0.0, |l| (pose.x - l.x).hypot(pose.s - l.s));
                tracing::info!(
                    slot = id,
                    ?why,
                    moved = moved as u32,
                    dt,
                    refused = me.refused + 1,
                    "a pose not passed on"
                );
            }
            me.refused += 1;
            if let Ok(mut by) = self.refused_by.lock() {
                by[why as usize] += 1;
            }
            return Verdict::Implausible;
        }
        let still = me.pose.is_some_and(|l| (l.x, l.s, l.h, l.ride) == (pose.x, pose.s, pose.h, pose.ride));
        me.moved_at = match now.checked_sub(Duration::from_secs_f32(STILL)) {
            Some(earliest) if still => me.moved_at.max(earliest),
            _ => now,
        };
        me.pose = Some(pose);
        me.seq = seq;
        me.heard = now;
        Verdict::Taken
    }

    /// The people `viewer` sees, into their plaza datagram: on their strip, heard from lately and
    /// near them, nearest first. Their slots go into `shown`.
    pub fn fill(&self, viewer: u16, now: Instant, tick: u32, w: &mut PlazaWriter<'_>, shown: &mut Vec<u16>) {
        shown.clear();
        let Ok(all) = self.people.lock() else { return };
        let Some(me) = all.get(&viewer) else { return };
        let (s, x) = me.pose.map_or_else(|| hub_gate(me.strip), |p| place(&p, tick));
        let mut near: Vec<(f32, u16, f32, PersonPose)> = all
            .iter()
            .filter(|(id, p)| **id != viewer && p.strip == me.strip)
            .filter_map(|(id, p)| {
                let pose = p.pose?;
                let age = now.saturating_duration_since(p.heard).as_secs_f32();
                let (ps, px) = place(&pose, tick);
                let d = (px - x).hypot(ps - s);
                (age < HIDE && d < NEAR).then_some((d, *id, age, pose))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, id, age, pose) in near.into_iter().take(MAX_PEOPLE) {
            if !w.push(id, age, &pose) {
                break;
            }
            shown.push(id);
        }
    }

    /// `id`'s name, as they came down.
    pub fn name(&self, id: u16) -> Option<String> {
        self.people.lock().ok()?.get(&id).map(|p| p.name.clone())
    }

    /// For `/status`: how many poses have been refused, by why.
    pub fn refused_by(&self) -> Vec<(&'static str, u32)> {
        let by = self.refused_by.lock().map(|b| *b).unwrap_or_default();
        WHYS.iter().zip(by).map(|(w, n)| (*w, n)).collect()
    }

    /// For `/status`: how many are in the city, by strip, and how many poses have been refused
    /// (no positions).
    pub fn counts(&self) -> (usize, [usize; 3], u32) {
        let Ok(all) = self.people.lock() else { return (0, [0; 3], 0) };
        let mut by_strip = [0; 3];
        for p in all.values() {
            by_strip[p.strip as usize % 3] += 1;
        }
        (all.len(), by_strip, all.values().map(|p| p.refused).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::MAX_DATAGRAM;
    use bc_proto::presence::PlazaReader;

    fn at(strip: u8, s: f32, x: f32) -> PersonPose {
        PersonPose { strip, x, s, grounded: true, ..PersonPose::default() }
    }

    fn seen(plaza: &Plaza, viewer: u16, now: Instant) -> Vec<(u16, PersonPose)> {
        seen_at(plaza, viewer, now, 0)
    }

    fn seen_at(plaza: &Plaza, viewer: u16, now: Instant, tick: u32) -> Vec<(u16, PersonPose)> {
        let mut buf = [0u8; MAX_DATAGRAM];
        let mut w = PlazaWriter::new(&mut buf, 1, Some(0));
        let mut shown = Vec::new();
        plaza.fill(viewer, now, tick, &mut w, &mut shown);
        let n = w.finish();
        let mut r = PlazaReader::new(&buf[..n]).unwrap();
        let mut out = Vec::new();
        while let Some((id, _, p)) = r.next_person() {
            out.push((id, p));
        }
        assert_eq!(out.iter().map(|(id, _)| *id).collect::<Vec<_>>(), shown);
        out
    }

    #[test]
    fn people_come_down_at_hub_gate_walk_and_see_each_other() {
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        plaza.enter(1, "Heero", 0);
        plaza.enter(2, "Duo", 0);
        assert_eq!(plaza.accept(1, 1, at(0, s, x + 2.0), t0, 0), Verdict::Taken);
        assert_eq!(plaza.accept(2, 1, at(0, s + 3.0, x + 2.0), t0, 0), Verdict::Taken);
        let me = seen(&plaza, 1, t0);
        assert_eq!(me.len(), 1);
        assert_eq!(me[0].0, 2);
        // A second's walk is fine; a reordered packet is dropped.
        let t1 = t0 + Duration::from_secs(1);
        assert_eq!(plaza.accept(2, 3, at(0, s + 3.0, x + 9.0), t1, 0), Verdict::Taken);
        assert_eq!(plaza.accept(2, 2, at(0, s + 3.0, x + 5.0), t1, 0), Verdict::Stale);
        assert!((seen(&plaza, 1, t1)[0].1.x - (x + 9.0)).abs() < 0.01);
        assert_eq!(plaza.counts().0, 2);
        plaza.leave(2);
        assert!(seen(&plaza, 1, t1).is_empty());
    }

    #[test]
    fn a_slow_client_repeating_its_pose_between_frames_isnt_refused() {
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        plaza.enter(1, "Hilde", 0);
        assert_eq!(plaza.accept(1, 1, at(0, s, x), t0, 0), Verdict::Taken);
        // At under 1 fps: the same pose at 15 Hz for 1.5 s, then the next frame's 7 m run at once.
        let mut seq = 1;
        for i in 1..=22 {
            seq += 1;
            let t = t0 + Duration::from_millis(i * 67);
            assert_eq!(plaza.accept(1, seq, at(0, s, x), t, 0), Verdict::Taken);
        }
        let t1 = t0 + Duration::from_millis(1_550);
        assert_eq!(plaza.accept(1, seq + 1, at(0, s, x + 7.0), t1, 0), Verdict::Taken);
        // Standing still a minute doesn't earn a long jump.
        for i in 1..=900 {
            seq += 1;
            let t = t1 + Duration::from_millis(i * 67);
            assert_eq!(plaza.accept(1, seq + 1, at(0, s, x + 7.0), t, 0), Verdict::Taken);
        }
        let t2 = t1 + Duration::from_millis(900 * 67 + 67);
        assert_eq!(plaza.accept(1, seq + 2, at(0, s, x + 207.0), t2, 0), Verdict::Implausible);
    }

    #[test]
    fn a_teleport_a_wall_or_another_strip_isnt_passed_on() {
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        plaza.enter(1, "Trowa", 0);
        // Not down the lift: far from Hub Gate.
        assert_eq!(plaza.accept(1, 1, at(0, s, x + 2_000.0), t0, 0), Verdict::Implausible);
        assert_eq!(plaza.accept(1, 2, at(0, s, x + 2.0), t0, 0), Verdict::Taken);
        // A kilometre in a tenth of a second.
        let t1 = t0 + Duration::from_millis(100);
        assert_eq!(plaza.accept(1, 3, at(0, s, x + 1_000.0), t1, 0), Verdict::Implausible);
        // Another strip.
        assert_eq!(plaza.accept(1, 4, at(1, s, x + 2.0), t1, 0), Verdict::Implausible);
        // Inside Hub Gate's terminal (the end cap's foot).
        let wall = at(0, STRIP_WIDTH * 0.5, -15_975.0);
        assert!(!possible(&wall));
        assert!(!possible(&at(0, s, -16_100.0)), "beyond the end cap");
        assert_eq!(plaza.counts().2, 3);
        // Someone never in the city.
        assert_eq!(plaza.accept(9, 1, at(0, s, x), t1, 0), Verdict::Away);
    }

    #[test]
    fn pilots_ride_the_trams_from_platform_to_platform() {
        use bc_sim::colony::transit::{DOOR_AT, PERIOD_TICKS, PLATFORM_HALF, train};
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        let k = 2;
        // When train `k` stands at station `i` with its doors open.
        let open_at = |i: usize| {
            (0..PERIOD_TICKS)
                .find(|t| train(0, k, *t, 0.0).doors && train(0, k, *t, 0.0).at == Some(i))
                .unwrap()
        };
        let tick = open_at(0);
        let tr = train(0, k, tick, 0.0);
        let door = tr.car_x(0) + DOOR_AT;
        let island = STRIP_WIDTH * 0.5 + tr.dir * (PLATFORM_HALF - 0.3);
        plaza.enter(1, "Trowa", 0);
        plaza.enter(2, "Catherine", 0);
        for id in [1, 2] {
            assert_eq!(plaza.accept(id, 1, at(0, s, x + 2.0), t0, tick), Verdict::Taken);
            let platform = PersonPose { h: 1.0, ..at(0, island, door) };
            assert_eq!(plaza.accept(id, 2, platform, t0 + Duration::from_secs(60), tick), Verdict::Taken);
        }
        // In through the door.
        let rider = |lx: f32| PersonPose { x: lx, s: RIDER_S + 0.4, h: 0.0, ride: k + 1, ..at(0, 0.0, 0.0) };
        assert_eq!(
            plaza.accept(1, 3, rider(door - tr.x), t0 + Duration::from_secs(61), tick),
            Verdict::Taken
        );
        // Seen inside the train, wherever it is.
        let seen_by_2 = seen_at(&plaza, 2, t0 + Duration::from_secs(62), tick);
        assert_eq!(seen_by_2[0].1.riding(), Some(k));
        // Walking down the car as it runs.
        assert_eq!(
            plaza.accept(1, 4, rider(door - tr.x + 3.0), t0 + Duration::from_secs(63), tick + 600),
            Verdict::Taken
        );
        // Out at the next station, onto its platform.
        let next = open_at(1);
        let tr1 = train(0, k, next, 0.0);
        let out = PersonPose { h: 1.0, ..at(0, island, tr1.car_x(0) + DOOR_AT) };
        assert_eq!(plaza.accept(1, 5, out, t0 + Duration::from_secs(150), next), Verdict::Taken);
        // Nobody gets on a train that's running, or one far away.
        let running = (0..PERIOD_TICKS).find(|t| train(0, k, *t, 0.0).speed > 30.0).unwrap();
        assert_eq!(
            plaza.accept(2, 3, rider(0.0), t0 + Duration::from_secs(64), running),
            Verdict::Implausible
        );
        // Nor stands outside its cars while aboard.
        assert_eq!(plaza.accept(2, 4, rider(80.0), t0 + Duration::from_secs(65), tick), Verdict::Implausible);
    }

    #[test]
    fn cars_come_from_the_motor_pools_and_keep_to_a_cars_speed() {
        use bc_proto::presence::RIDE_CAR;
        use bc_sim::colony::pools::pool;
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        let (ps, px) = pool(0, 0);
        plaza.enter(1, "Duo", 0);
        assert_eq!(plaza.accept(1, 1, at(0, s, x + 2.0), t0, 0), Verdict::Taken);
        // Not from the middle of the street.
        let car = |s: f32, x: f32| PersonPose { ride: RIDE_CAR, ..at(0, s, x) };
        assert_eq!(
            plaza.accept(1, 2, car(s, x + 3.0), t0 + Duration::from_millis(100), 0),
            Verdict::Implausible
        );
        // From the pool.
        let t1 = t0 + Duration::from_secs(10);
        assert_eq!(plaza.accept(1, 3, at(0, ps, px), t1, 0), Verdict::Taken);
        assert_eq!(plaza.accept(1, 4, car(ps, px + 0.5), t1 + Duration::from_millis(100), 0), Verdict::Taken);
        // 30 m/s for a second, fine; 300 m in one, not.
        let t2 = t1 + Duration::from_millis(1_100);
        assert_eq!(plaza.accept(1, 5, car(ps, px + 30.5), t2, 0), Verdict::Taken);
        assert_eq!(
            plaza.accept(1, 6, car(ps, px + 330.0), t2 + Duration::from_secs(1), 0),
            Verdict::Implausible
        );
        // Out beside it.
        assert_eq!(
            plaza.accept(1, 7, at(0, ps + 1.6, px + 30.5), t2 + Duration::from_secs(2), 0),
            Verdict::Taken
        );
    }

    #[test]
    fn only_the_strip_and_the_near_are_shown() {
        let plaza = Plaza::default();
        let t0 = Instant::now();
        let (s, x) = hub_gate(0);
        plaza.enter(1, "Quatre", 0);
        plaza.accept(1, 1, at(0, s, x), t0, 0);
        plaza.enter(2, "Wufei", 1);
        let (s1, x1) = hub_gate(1);
        plaza.accept(2, 1, at(1, s1, x1), t0, 0);
        assert!(seen(&plaza, 1, t0).is_empty(), "another strip");
        // Quiet for longer than HIDE: not shown.
        plaza.enter(3, "Zechs", 0);
        plaza.accept(3, 1, at(0, s + 1.0, x), t0, 0);
        assert_eq!(seen(&plaza, 1, t0).len(), 1);
        assert!(seen(&plaza, 1, t0 + Duration::from_secs(6)).is_empty());
        assert_eq!(plaza.name(3).as_deref(), Some("Zechs"));
        assert_eq!(plaza.counts().1, [2, 1, 0]);
    }
}
