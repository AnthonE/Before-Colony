//! A beam as drawn at a moment: where its head is, and where it stopped, if it has.
//!
//! A shot is drawn from the moment it leaves the muzzle, never before (its head is never behind
//! the gun), and up to whatever stopped it: a suit it struck (the server's `Hit` cuts it there,
//! [`Beam::end`]), or, inside the colony, the Blast Hall's targets and walls, judged where they
//! were when the server flew it ([`Beam::judge`]). What else stops a shot out in space (rocks, the
//! colony's hull, the landmarks, a ward) is the view's to find, from what it draws.

use bc_sim::TICK_HZ;
use bc_sim::colony::hall::{Stop, shot_end, target};
use bc_sim::content::weapon;
use glam::Vec3;

use crate::world::Beam;

/// What stopped a beam, as the client can tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    /// It struck a suit (the server's `Hit`): the sparks are the hit's own.
    Suit,
    /// One of the Blast Hall's targets.
    Target(u8),
    /// The hall's walls, floor or roof.
    Wall,
}

/// A beam at a moment.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    /// Where it left the gun, as drawn.
    pub origin: Vec3,
    /// Its head, or where it stopped.
    pub head: Vec3,
    pub dir: Vec3,
    /// How far it has flown from the gun, m (up to where it stopped).
    pub travelled: f32,
    /// Where it stopped by now, if it has: the point, what, and the surface's normal there.
    pub stop: Option<(Vec3, Ended, Vec3)>,
}

/// Beam `b` at `t` on the clock it's drawn on, leaving `origin` at `spawn` (its own, or, for an own
/// shot heard of late, where and when it was first drawn leaving the gun), judged against the hall
/// `judge` ticks back on the server's clock. None before it leaves the gun or after its life.
/// `inside`: in the colony, where only the Blast Hall's training rounds fly.
pub fn sample(b: &Beam, origin: Vec3, spawn: f64, judge: f64, t: f64, inside: bool) -> Option<Sample> {
    if t < spawn || t > spawn + b.ttl_ticks {
        return None;
    }
    let dir = b.velocity.normalize_or(Vec3::Z);
    let head = origin + b.velocity * ((t - spawn) / f64::from(TICK_HZ)) as f32;
    let mut stop =
        b.end.filter(|e| head.distance(origin) >= *e).map(|e| (origin + dir * e, Ended::Suit, -dir));
    if inside {
        let tj = (t - judge).max(0.0);
        let (k, frac) = (tj.floor() as u32, (tj - tj.floor()) as f32);
        if let Some((f, what)) = shot_end(origin, head, weapon(b.weapon).radius, k, frac) {
            let at = origin + (head - origin) * f;
            if stop.is_none_or(|(s, ..)| at.distance(origin) < s.distance(origin)) {
                stop = Some(match what {
                    Stop::Target(i) => {
                        (at, Ended::Target(i), (at - target(usize::from(i), k, frac)).normalize_or(-dir))
                    }
                    Stop::Wall => (at, Ended::Wall, -dir),
                });
            }
        }
    }
    let head = stop.map_or(head, |(at, ..)| at);
    Some(Sample { origin, head, dir, travelled: head.distance(origin), stop })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::WeaponKind;
    use bc_sim::colony::hall::{TARGETS, gantry};

    fn beam(origin: Vec3, dir: Vec3, speed: f32) -> Beam {
        Beam {
            id: 1,
            shooter: 0,
            weapon: WeaponKind::BeamRifle,
            origin,
            velocity: dir * speed,
            spawn_tick: 100.0,
            ttl_ticks: 45.0,
            predicted: false,
            shot_seq: 1,
            key: 1,
            end: None,
            judge: 0.0,
            rebase: false,
        }
    }

    #[test]
    fn nothing_is_drawn_before_it_leaves_the_gun_or_behind_it() {
        let b = beam(Vec3::ZERO, Vec3::Z, 3_000.0);
        assert!(sample(&b, b.origin, b.spawn_tick, 0.0, 99.5, false).is_none());
        let s = sample(&b, b.origin, b.spawn_tick, 0.0, 100.0, false).unwrap();
        assert_eq!(s.head, Vec3::ZERO);
        let s = sample(&b, b.origin, b.spawn_tick, 0.0, 101.0, false).unwrap();
        assert!((s.head.z - 100.0).abs() < 1e-3);
        assert!(sample(&b, b.origin, b.spawn_tick, 0.0, 146.0, false).is_none());
    }

    #[test]
    fn a_beam_that_struck_a_suit_is_drawn_up_to_it() {
        let mut b = beam(Vec3::ZERO, Vec3::Z, 3_000.0);
        b.end = Some(150.0);
        let before = sample(&b, b.origin, b.spawn_tick, 0.0, 101.0, false).unwrap();
        assert!(before.stop.is_none());
        let after = sample(&b, b.origin, b.spawn_tick, 0.0, 103.0, false).unwrap();
        let (at, what, _) = after.stop.unwrap();
        assert_eq!(what, Ended::Suit);
        assert!((at.z - 150.0).abs() < 1e-3 && (after.travelled - 150.0).abs() < 1e-3);
        assert_eq!(after.head, at);
    }

    #[test]
    fn a_round_in_the_hall_stops_on_the_target_where_the_server_judged_it() {
        // From the gantry at a bobbing target, where it was 6 ticks before the drawing clock's
        // time (the pilot's view lag): judged then it scores; judged without the lag it may not.
        let from = gantry() + bc_sim::colony::frame::up_at(gantry()) * 8.0;
        for i in 0..TARGETS {
            let at = target(i, 200, 0.0);
            let mut b = beam(from, (at - from).normalize(), 4_000.0);
            b.spawn_tick = 206.0;
            b.weapon = WeaponKind::BeamRifle;
            let s = sample(&b, from, 206.0, 6.0, 207.0, true).unwrap();
            // (Another target may stand in the way: the one aimed at, or one nearer.)
            assert!(matches!(s.stop, Some((_, Ended::Target(_), _))), "target {i}");
            assert!(s.travelled < from.distance(at));
        }
    }
}
