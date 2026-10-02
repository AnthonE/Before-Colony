//! Riding the colony's trams (`bc_sim::colony::transit`): a car's inside is the walker's world
//! while they ride, in the car's own frame (`x` along from its middle, `h` up from its floor, `−s`
//! across from its middle: the city's walker frame, moved), so the car carries them wherever the
//! timetable takes it, and its speeding up and slowing down pushes them about. They get on through
//! an open door of a standing train, from its platform, and off the same way.

use bc_proto::presence::{PersonPose, RIDER_S};
use bc_sim::colony::city::{CityBox, Rect};
use bc_sim::colony::frame::{CityPos, STRIP_WIDTH, gravity};
use bc_sim::colony::transit::{
    CAR_LENGTH, CAR_WIDTH, CARS, DOOR_AT, DOOR_WIDTH, FLOOR, PLATFORM_HALF, PLATFORM_LENGTH, STATION_GAP,
    STATIONS, TRACK_OFFSET, TrainState, car_offset, car_walls, station_x,
};
use glam::Vec3;

use crate::city::CityGround;
use crate::walker::{Solid, Walker};

/// Riding train `k` of the strip's line, in car `car`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rider {
    pub k: u8,
    pub car: usize,
}

fn hit(b: &CityBox, area: &Rect, h0: f32, h1: f32) -> bool {
    b.rect.overlaps(area) && b.h0 < h1 && h0 < b.h1
}

/// A car's inside, as the walker's world: its walls (and its doors, if they're open), the colony's
/// gravity at its floor's height, and the push of the train's acceleration (`accel`, along +x).
#[derive(Clone, Copy, Debug)]
pub struct CarInside {
    pub open: bool,
    pub accel: f32,
}

impl Solid for CarInside {
    fn hits(&self, min: Vec3, max: Vec3) -> bool {
        let area = Rect::new(-max.z, -min.z, min.x, max.x);
        car_walls(self.open, |b| hit(b, &area, min.y, max.y))
    }

    fn gravity(&self, feet: Vec3) -> f32 {
        gravity(FLOOR + feet.y)
    }

    fn push(&self) -> Vec3 {
        Vec3::new(-self.accel, 0.0, 0.0)
    }
}

/// The city with the trains that stand at its stations in it: their cars' walls, doors and all,
/// so a pilot on a platform walks in through a door and not through a wall.
pub struct CityAndTrains<'a> {
    pub ground: CityGround,
    pub trains: &'a [TrainState],
}

impl Solid for CityAndTrains<'_> {
    fn hits(&self, min: Vec3, max: Vec3) -> bool {
        if self.ground.hits(min, max) {
            return true;
        }
        let (s0, s1, x0, x1) = (-max.z, -min.z, min.x, max.x);
        if self.screens(&Rect::new(s0, s1, x0, x1), min.y, max.y) {
            return true;
        }
        self.trains.iter().filter(|t| t.at.is_some()).any(|t| {
            (0..CARS).any(|c| {
                let (cx, cs) = (t.car_x(c), t.s);
                if x1 < cx - CAR_LENGTH || x0 > cx + CAR_LENGTH || s1 < cs - CAR_WIDTH || s0 > cs + CAR_WIDTH
                {
                    return false;
                }
                let area = Rect::new(s0 - cs, s1 - cs, x0 - cx, x1 - cx);
                car_walls(t.doors, |b| hit(b, &area, min.y - FLOOR, max.y - FLOOR))
            })
        })
    }

    fn gravity(&self, feet: Vec3) -> f32 {
        self.ground.gravity(feet)
    }
}

/// The platforms' screen doors: glass along each long edge of the island, this tall, with a door
/// wherever a standing train's car has one (they stop in the same place every time), open while
/// it is.
const SCREEN: f32 = 2.2;
const SCREEN_THICKNESS: f32 = 0.08;

impl CityAndTrains<'_> {
    /// Whether `area` (between heights `h0` and `h1`) meets a platform's screen: its glass, or one
    /// of its doors that's shut.
    fn screens(&self, area: &Rect, h0: f32, h1: f32) -> bool {
        let mid = STRIP_WIDTH * 0.5;
        if h1 < FLOOR
            || h0 > FLOOR + SCREEN
            || (area.s0 > mid + PLATFORM_HALF)
            || (area.s1 < mid - PLATFORM_HALF)
        {
            return false;
        }
        let near = ((0.5 * (area.x0 + area.x1) - station_x(0)) / STATION_GAP).round();
        if near < 0.0 || near >= STATIONS as f32 {
            return false;
        }
        let i = near as usize;
        let x = station_x(i);
        let half = 0.5 * PLATFORM_LENGTH;
        for side in [-1.0f32, 1.0] {
            let edge = mid + side * PLATFORM_HALF;
            let glass = Rect::new(
                edge.min(edge - side * SCREEN_THICKNESS),
                edge.max(edge - side * SCREEN_THICKNESS),
                x - half,
                x + half,
            );
            if !glass.overlaps(area) {
                continue;
            }
            // The train standing at this edge with its doors open, if there is one.
            let open = self.trains.iter().any(|t| t.doors && t.at == Some(i) && (t.s - mid) * side > 0.0);
            let in_door = |x0: f32, x1: f32| {
                (0..CARS).any(|c| {
                    [-DOOR_AT, DOOR_AT].iter().any(|d| {
                        let at = x + car_offset(c) + d;
                        x0 >= at - 0.5 * DOOR_WIDTH && x1 <= at + 0.5 * DOOR_WIDTH
                    })
                })
            };
            if !(open && in_door(area.x0.max(glass.x0), area.x1.min(glass.x1))) {
                return true;
            }
        }
        false
    }
}

/// The car's inside: how far in from its sides and ends a walker must be to be aboard, m.
const INSIDE: f32 = 0.35;

/// A walker on foot at `feet` (city coordinates) who has walked into a car of a standing train
/// with its doors open: which, and their feet in the car's frame.
pub fn boarding(feet: CityPos, trains: &[TrainState]) -> Option<(Rider, Vec3)> {
    trains.iter().filter(|t| t.doors).find_map(|t| {
        (0..CARS).find_map(|c| {
            let (lx, ls, lh) = (feet.x - t.car_x(c), feet.s - t.s, feet.h - FLOOR);
            let inside =
                lx.abs() < 0.5 * CAR_LENGTH - INSIDE && ls.abs() < 0.5 * CAR_WIDTH - INSIDE && lh > -0.3;
            inside.then_some((Rider { k: t.k, car: c }, Vec3::new(lx, lh.max(0.0), -ls)))
        })
    })
}

/// A pilot fallen onto the track beside a platform (the doors shut on them as their train pulled
/// out): where to put them back, on the platform's edge.
pub fn rescue(feet: CityPos) -> Option<CityPos> {
    let mid = STRIP_WIDTH * 0.5;
    let i = ((feet.x - station_x(0)) / STATION_GAP).round();
    if !(0.0..STATIONS as f32).contains(&i) || feet.h > FLOOR - 0.5 {
        return None;
    }
    let off = feet.s - mid;
    let beside = (PLATFORM_HALF..TRACK_OFFSET + 0.5 * CAR_WIDTH + 0.5).contains(&off.abs());
    let along = (feet.x - station_x(i as usize)).abs() < 0.5 * PLATFORM_LENGTH - 4.0;
    (beside && along)
        .then(|| CityPos::new(feet.strip, feet.x, mid + off.signum() * (PLATFORM_HALF - 0.4), FLOOR))
}

/// Where a rider's feet (`local`, in their car's frame) are in city coordinates.
pub fn in_city(r: &Rider, local: Vec3, t: &TrainState) -> CityPos {
    CityPos::new(t.strip, t.car_x(r.car) + local.x, t.s - local.z, FLOOR + local.y)
}

/// A rider who has walked out of their car's side (through an open door): where they stand now.
pub fn alighting(r: &Rider, local: Vec3, t: &TrainState) -> Option<CityPos> {
    (local.z.abs() > 0.5 * CAR_WIDTH + 0.25).then(|| in_city(r, local, t))
}

/// A rider as the plaza has them: from their train's middle (`bc_proto::presence`).
pub fn pose_riding(r: &Rider, w: &Walker, strip: u8) -> PersonPose {
    let flat = Vec3::new(w.vel.x, 0.0, w.vel.z).length();
    PersonPose {
        strip,
        x: car_offset(r.car) + w.feet.x,
        s: RIDER_S - w.feet.z,
        h: w.feet.y,
        yaw: w.yaw,
        pitch: w.pitch,
        speed: flat,
        grounded: w.grounded,
        running: flat > 5.0,
        ride: r.k + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::walker::Stride;
    use bc_sim::colony::city::Stage;
    use bc_sim::colony::frame::STRIP_WIDTH;
    use bc_sim::colony::transit::{DOOR_AT, PERIOD_TICKS, PLATFORM_HALF, train};

    const DT: f32 = 1.0 / 60.0;

    /// Train 2 standing at station 1 with its doors open: the tick.
    fn standing() -> u32 {
        (0..PERIOD_TICKS)
            .find(|t| {
                let tr = train(0, 2, *t, 0.0);
                tr.doors && tr.at == Some(1) && train(0, 2, t + 200, 0.0).doors
            })
            .unwrap()
    }

    #[test]
    fn a_pilot_walks_in_through_an_open_door_and_rides_out_of_the_station() {
        let tick = standing();
        let tr = train(0, 2, tick, 0.0);
        // On the island platform, facing the car's door across the gap.
        let island = STRIP_WIDTH * 0.5 + tr.dir * (PLATFORM_HALF - 0.6);
        let door = tr.car_x(1) + DOOR_AT;
        let start = CityPos::new(0, door, island, FLOOR);
        let across = CityPos::new(0, door, island + tr.dir, FLOOR).walker() - start.walker();
        let mut w = Walker::at(start.walker(), across.normalize());
        let world = CityAndTrains { ground: CityGround { strip: 0, stage: Stage(0) }, trains: &[tr] };
        let mut on = None;
        for _ in 0..120 {
            w.step(&world, &Stride { forward: 1.0, ..Stride::default() }, DT);
            if let Some(b) = boarding(CityPos::from_walker(0, w.feet), &[tr]) {
                on = Some(b);
                break;
            }
        }
        let (rider, local) = on.expect("aboard through the door");
        assert_eq!(rider, Rider { k: 2, car: 1 });
        // Riding: the doors shut, the train pulls away; its push takes the rider down the car to
        // its end wall, and no further.
        let mut w = Walker::at(local, Vec3::Z);
        w.grounded = true;
        let mut t = tick;
        while train(0, 2, t, 0.0).at.is_some() {
            t += 1;
        }
        for _ in 0..(30 * 30) {
            let now = train(0, 2, t, 0.0);
            w.step(&CarInside { open: now.doors, accel: now.accel }, &Stride::default(), 1.0 / 30.0);
            assert!(w.feet.x.abs() < 0.5 * CAR_LENGTH && w.feet.z.abs() < 0.5 * CAR_WIDTH, "{:?}", w.feet);
            assert!(alighting(&rider, w.feet, &now).is_none());
            t += 1;
        }
        let tr = train(0, 2, t, 0.0);
        let p = in_city(&rider, w.feet, &tr);
        assert!((p.x - tr.car_x(1)).abs() < 0.5 * CAR_LENGTH && (p.h - FLOOR).abs() < 0.1, "{p:?}");
    }

    #[test]
    fn the_guide_takes_a_pilot_from_the_platforms_middle_in_through_a_door() {
        use crate::walker::Guide;
        // A train standing at Hub Gate's station (the end of the line) with its doors open.
        let tick = (0..PERIOD_TICKS)
            .find(|t| {
                let tr = train(0, 2, *t, 0.0);
                tr.doors && tr.at == Some(0) && train(0, 2, t + 300, 0.0).doors
            })
            .unwrap();
        let tr = train(0, 2, tick, 0.0);
        let mid = STRIP_WIDTH * 0.5;
        let door = tr.car_x(0) - DOOR_AT;
        let at = |x: f32, s: f32| CityPos::new(0, x, s, FLOOR).walker();
        // Coming along the island from its middle, then across to the door.
        let mut w = Walker::at(at(tr.x, mid), -Vec3::X);
        let world = CityAndTrains { ground: CityGround { strip: 0, stage: Stage(0) }, trains: &[tr] };
        for _ in 0..30 {
            w.step(&world, &Stride::default(), DT);
        }
        let mut guide = Guide::new(vec![at(door, mid), at(door, tr.s)], None);
        let mut on = None;
        for _ in 0..(20.0 / DT) as u32 {
            let stride = guide.steer(&mut w, DT);
            w.step(&world, &stride, DT);
            if let Some(b) = boarding(CityPos::from_walker(0, w.feet), &[tr]) {
                on = Some(b);
                break;
            }
        }
        assert!(on.is_some(), "never got on: {:?} {:?}", CityPos::from_walker(0, w.feet), guide.route);
    }

    #[test]
    fn the_platforms_screens_keep_you_off_the_track_but_for_a_trains_open_doors() {
        let x = bc_sim::colony::transit::station_x(3);
        let mid = STRIP_WIDTH * 0.5;
        let empty = CityAndTrains { ground: CityGround { strip: 0, stage: Stage(0) }, trains: &[] };
        // Walking straight off the island's +s edge, no train there: stopped at the glass.
        for along in [x - 20.0, x + car_offset(1) + DOOR_AT] {
            let start = CityPos::new(0, along, mid, FLOOR);
            let across = CityPos::new(0, along, mid + 1.0, FLOOR).walker() - start.walker();
            let mut w = Walker::at(start.walker(), across.normalize());
            for _ in 0..(4.0 / DT) as u32 {
                w.step(&empty, &Stride { forward: 1.0, ..Stride::default() }, DT);
            }
            let p = CityPos::from_walker(0, w.feet);
            assert!(p.s < mid + PLATFORM_HALF && (p.h - FLOOR).abs() < 0.05, "off the platform: {p:?}");
        }
    }

    #[test]
    fn a_pilot_on_the_track_beside_a_platform_is_put_back_on_it() {
        let mid = STRIP_WIDTH * 0.5;
        let x = bc_sim::colony::transit::station_x(2);
        let p = rescue(CityPos::new(0, x - 6.0, mid + 3.2, 0.0)).expect("put back");
        assert!((p.h - FLOOR).abs() < 1e-3 && p.s < mid + PLATFORM_HALF && p.s > mid);
        assert!(rescue(CityPos::new(0, x, mid - 4.0, 0.0)).is_some_and(|p| p.s < mid));
        // On the platform, on the avenue's road, or on the track between stations: left alone.
        assert!(rescue(CityPos::new(0, x, mid + 2.0, FLOOR)).is_none());
        assert!(rescue(CityPos::new(0, x, mid + 15.0, 0.0)).is_none());
        assert!(rescue(CityPos::new(0, x + 1_000.0, mid + 4.0, 0.0)).is_none());
    }

    #[test]
    fn the_walls_of_a_standing_train_keep_you_out_but_for_its_doors() {
        let tick = standing();
        let tr = train(0, 2, tick, 0.0);
        let world = CityAndTrains { ground: CityGround { strip: 0, stage: Stage(0) }, trains: &[tr] };
        let island = STRIP_WIDTH * 0.5 + tr.dir * (PLATFORM_HALF - 0.6);
        // Facing a wall between doors: stopped at it.
        let at = CityPos::new(0, tr.car_x(1), island, FLOOR);
        let across = CityPos::new(0, tr.car_x(1), island + tr.dir, FLOOR).walker() - at.walker();
        let mut w = Walker::at(at.walker(), across.normalize());
        for _ in 0..120 {
            w.step(&world, &Stride { forward: 1.0, ..Stride::default() }, DT);
        }
        assert!(boarding(CityPos::from_walker(0, w.feet), &[tr]).is_none());
        // A rider walks out of an open door onto the platform.
        let rider = Rider { k: 2, car: 1 };
        // The platform is on the car's side towards the avenue's middle (−s for an out-bound
        // train: +z in the walker's frame).
        let mut w = Walker::at(Vec3::new(DOOR_AT, 0.0, 0.0), Vec3::new(0.0, 0.0, tr.dir));
        w.grounded = true;
        let mut out = None;
        for _ in 0..240 {
            w.step(&CarInside { open: true, accel: 0.0 }, &Stride { forward: 1.0, ..Stride::default() }, DT);
            if let Some(p) = alighting(&rider, w.feet, &tr) {
                out = Some(p);
                break;
            }
        }
        let p = out.expect("out of the door");
        assert!((p.s - island).abs() < 1.0 && (p.h - FLOOR).abs() < 0.1, "{p:?}");
        // A rider's pose is from the train's middle.
        let pose = pose_riding(&rider, &w, 0);
        assert_eq!(pose.riding(), Some(2));
        assert!((pose.x - (tr.car_x(1) - tr.x + w.feet.x)).abs() < 1e-3);
    }
}
