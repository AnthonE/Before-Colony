//! Lock-on: fighting a target as if on the ground (`docs/LOCK.md`).
//!
//! The pilot locks a hostile ([`Lock::tap`]). While the lock holds and flight assist is on, the
//! keys stop meaning "thrust along the suit" and start meaning "move about the target"
//! ([`shape`]): W closes on it and stops short of it (never faster than the suit can brake from), S
//! backs off, A/D circle it at the range it's at, and with Space and C let go the suit settles onto
//! the target's level, the fight's floor. Space and C rise off it or drop below it; let go and the
//! suit comes back. The fight's up is away from the colony (`bc_sim::world::colony_up`), shared by
//! both pilots; a target standing on a body is fought with that body's ground for the floor.
//!
//! None of it is the server's to work out. What the keys come to goes out in the command as an
//! ordinary stick, with the [`LockOn`] that flight assist holds it relative to (the target's
//! velocity, as this client sees it, and the fight's up), so the server and this client's prediction
//! fly exactly the same thing. The aim stays the pilot's own: the lock moves the suit, it doesn't
//! point the guns.

use bc_proto::buttons::{BURST, FLIGHT_ASSIST};
use bc_proto::snapshot::{ent_flags, zero_mode};
use bc_proto::{FrameId, InputCmd, LockOn};
use bc_sim::config::DT;
use bc_sim::content::{frame, weapon};
use bc_sim::flight::{FA_RESPONSE, fa_cruise};
use bc_sim::ground::{Footing, GRIP_ACCEL, lockon_assist};
use bc_sim::world::{colony_altitude, colony_up};
use glam::Vec3;

use crate::InputContext;
use crate::interp::Pose;
use crate::surface::surface_hint;
use crate::world::World;

/// A tap locks the hostile nearest the crosshair within this angle (rad)...
pub const PICK_CONE: f32 = 0.175; // 10°
/// ...else the one nearest it within this (what's on screen, near enough)...
pub const VIEW_CONE: f32 = 1.05; // 60°
/// ...else the nearest at all, within this range, m.
pub const PICK_RANGE: f32 = 4_000.0;
/// The lock breaks past this range, m...
pub const BREAK_RANGE: f32 = 5_000.0;
/// ...or once the target hasn't been heard of for this many ticks (out of sight).
pub const STALE_TICKS: u32 = 30;
/// How long the lock key is held to let go, s (a tap locks, or moves the lock on).
pub const HOLD_TO_RELEASE: f64 = 0.35;

/// The most the target's position is carried forward to where the own suit is predicted, ticks.
const EXTRAPOLATE_MAX: f64 = 10.0;
/// The share of the thrusters the helper plans to brake and turn with.
const PLAN: f32 = 0.8;
/// The hardest the helper brakes for a stop, m/s² (3 g: within what a pilot bears for good).
const BRAKE_MAX: f32 = 30.0;
/// W stops this far outside a blade's reach (a share of it, plus metres), so the lunge carries in.
const STOP_REACH: f32 = 0.7;
const STOP_PAD: f32 = 5.0;
/// Where W stops for a suit without a blade, m.
const STOP_BARE: f32 = 15.0;
/// Circling, the suit turns to keep the target ahead no faster than this share of its AMBAC rate.
const ORBIT_TURN: f32 = 0.7;
/// Settling onto the floor no faster than this share of the cruise.
const SETTLE_SHARE: f32 = 0.5;

/// Why a lock went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Broke {
    /// The target is a wreck.
    Downed,
    /// Out of sight: off sensors, jamming, hidden.
    Lost,
    /// Too far.
    Range,
    /// The pilot let go, or their own suit is gone.
    Released,
}

/// The pilot's lock, and what the helper keeps from one tick to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lock {
    /// The suit locked, by slot and the life it's on.
    target: Option<(u16, u8)>,
    /// The range A/D circle at and an idle stick holds, m (set by W and S).
    hold: Option<f32>,
}

/// One candidate for a lock: a live hostile, its angle off the aim, its range.
struct Candidate {
    slot: u16,
    generation: u8,
    angle: f32,
    range: f32,
}

fn candidates(world: &World, from: Vec3, aim: Vec3, t: f64) -> impl Iterator<Item = Candidate> + '_ {
    (0..world.entities.len() as u16).filter_map(move |slot| {
        let track = world.entity(slot)?;
        let e = &track.latest;
        if e.faction == world.faction || e.flags & ent_flags::WRECK != 0 || world.own_slot() == Some(slot) {
            return None;
        }
        let to = track.sample(t, &world.bodies).pos - from;
        let range = to.length();
        (range <= PICK_RANGE).then(|| Candidate {
            slot,
            generation: e.generation,
            angle: aim.angle_between(to),
            range,
        })
    })
}

impl Lock {
    /// The suit locked, if any.
    pub fn slot(&self) -> Option<u16> {
        self.target.map(|(s, _)| s)
    }

    pub fn locked(&self) -> bool {
        self.target.is_some()
    }

    /// The lock key tapped, from `from` along `aim` at time `t` (ticks): not locked, it locks the
    /// hostile nearest the crosshair (within [`PICK_CONE`], else within [`VIEW_CONE`], else the
    /// nearest within [`PICK_RANGE`]); locked, it moves on to the hostile nearest the crosshair
    /// that isn't the one locked, if there is one. Whether a lock was taken.
    pub fn tap(&mut self, world: &World, from: Vec3, aim: Vec3, t: f64) -> bool {
        let current = self.slot();
        let pick = |cone: f32| {
            candidates(world, from, aim, t)
                .filter(|c| Some(c.slot) != current && c.angle <= cone)
                .min_by(|a, b| a.angle.total_cmp(&b.angle))
        };
        let next = pick(PICK_CONE).or_else(|| pick(VIEW_CONE)).or_else(|| {
            candidates(world, from, aim, t)
                .filter(|c| Some(c.slot) != current)
                .min_by(|a, b| a.range.total_cmp(&b.range))
        });
        match next {
            Some(c) => {
                *self = Lock { target: Some((c.slot, c.generation)), hold: None };
                true
            }
            None => false,
        }
    }

    pub fn release(&mut self) {
        *self = Lock::default();
    }

    /// Whether the lock still holds, from `from` at time `t`: it's dropped (and why returned) once
    /// the target is a wreck, a suit on another life, out of sight or too far.
    pub fn validate(&mut self, world: &World, from: Vec3, t: f64) -> Option<Broke> {
        let (slot, generation) = self.target?;
        let broke = match world.entity(slot) {
            None => Some(Broke::Lost),
            Some(track) if track.latest.generation != generation => Some(Broke::Lost),
            Some(track) if track.latest.flags & ent_flags::WRECK != 0 => Some(Broke::Downed),
            Some(track) if world.tick.saturating_sub(track.latest_tick) > STALE_TICKS => Some(Broke::Lost),
            Some(track) if track.sample(t, &world.bodies).pos.distance(from) > BREAK_RANGE => {
                Some(Broke::Range)
            }
            Some(_) => None,
        };
        if broke.is_some() {
            self.release();
        }
        broke
    }

    /// The target as drawn at time `t`.
    pub fn target(&self, world: &World, t: f64) -> Option<Pose> {
        let (slot, _) = self.target?;
        Some(world.entity(slot)?.sample(t, &world.bodies))
    }
}

/// How hard to close a gap `e` (m) so as to stop on it, m/s, braking at `a` (m/s²) through flight
/// assist's lag: the stopping curve `√(2·a·|e|) − a·τ`, but no more than `|e| / 2τ` near the end,
/// so it settles rather than chattering.
pub fn glide(e: f32, a: f32) -> f32 {
    let mag = e.abs();
    let curve = ((2.0 * a * mag).sqrt() - a * FA_RESPONSE).max(0.0);
    (mag / (2.0 * FA_RESPONSE)).min(curve).copysign(e)
}

/// Where W stops short of the target: just outside the blade's reach (the lunge carries it in),
/// or a little way off for a suit without one, m.
pub fn stop_range(frame_id: FrameId) -> f32 {
    frame(frame_id).loadout[2].map_or(STOP_BARE, |m| weapon(m.weapon).range * STOP_REACH + STOP_PAD)
}

/// The command for this tick, locked on: `cmd` is what the keys and the mouse ask for as if no
/// lock were held (its thrust the keys: x right, y up, z forward), and `lock` the pilot's lock.
/// Flying free with flight assist on, the stick is reshaped to move about the target and the
/// [`LockOn`] it holds relative to goes with it. Otherwise (on a body or coming down onto one,
/// flight assist off, ZERO flying the suit, folded into Neo-Bird) the keys go as they are; the
/// target is still designated either way.
pub fn shape(cmd: InputCmd, lock: &mut Lock, ctx: &InputContext) -> InputCmd {
    let Some(slot) = lock.slot() else { return cmd };
    let mut out = InputCmd { lock_target: slot, ..cmd };
    let world = ctx.world;
    let p = &ctx.predict;
    let s = p.state;
    let Some(own) = world.own.filter(|o| o.alive) else { return out };
    let Some(track) = world.entity(slot) else { return out };
    let free = p.mover().footing == Footing::Free;
    let seized = own.zero_mode == zero_mode::SEIZED;
    let bird = p.form.frame == FrameId::WingZeroBird;
    let landing = cmd.pressed(bc_proto::buttons::GRIP) && surface_hint(&p.bodies(ctx.tick), &s).is_some();
    if !free || !cmd.pressed(FLIGHT_ASSIST) || seized || bird || landing {
        return out;
    }
    let spec = frame(p.form.frame);
    let mods = p.mods();

    // The target when the own suit is as predicted (after the tick before this command's, as a
    // snapshot of that tick has it): drawn at the view time, carried on at its velocity (which is
    // what flight assist holds to).
    let pose = track.sample(ctx.view_tick, &world.bodies);
    let ahead = (f64::from(ctx.tick) - 1.0 - ctx.view_tick).clamp(0.0, EXTRAPOLATE_MAX) as f32 * DT;
    let q = pose.pos + pose.vel * ahead;
    let vt = pose.vel;
    let r = q - s.pos;
    // The fight's up: the colony's, or the ground of the body the target stands on (then there's
    // no floor to settle on: its ground is in the way).
    let (up, floor) = match pose.ground {
        Some(g) => (g.up, false),
        None => (colony_up(s.pos), true),
    };
    // How far above the suit's level the target is: over the colony's curve, its altitude less the
    // suit's (a target a few kilometres round the hull at the same height is on the same floor);
    // over a body's ground, along its up. And how far off it is along the ground.
    let h = if floor { colony_altitude(q) - colony_altitude(s.pos) } else { r.dot(up) };
    let flat = r - up * r.dot(up);
    let d = (r.length_squared() - h * h).max(0.0).sqrt();
    // (Straight above or below it, any way across will do: the nose's.)
    let nose = s.rot * Vec3::Z;
    let across_nose = (nose - up * nose.dot(up)).normalize_or(up.any_orthonormal_vector());
    let toward = if flat.length_squared() > 1.0 { flat.normalize() } else { across_nose };
    let around = up.cross(toward);

    // What the keys ask for, and what the suit can do.
    let keys = cmd.thrust_vec();
    let mass = spec.mass(s.propellant) + mods.extra_mass_kg as f32;
    let brake = (PLAN * spec.retro_thrust * mods.retro / mass).min(BRAKE_MAX);
    let side = PLAN * spec.side_thrust * mods.side / mass;
    let stop = stop_range(p.form.frame);
    // The wire's own numbers, so the reshaped stick is read back exactly as it's meant.
    let probe = InputCmd { lockon: Some(LockOn { ref_vel: vt, up }), ..out }.quantized();
    let cruise = fa_cruise(spec, mods, &probe, s.propellant);
    let Some(axes) = lockon_assist(&probe, &s, spec) else { return out };
    // A burst step's press goes along the keys themselves, in the fight's axes (`flight::Burst`).
    if cmd.pressed(BURST) && !s.burst.held {
        out.lockon = probe.lockon;
        return out;
    }

    // Toward the target (+) or away: W closes and stops at `stop`; S opens; neither holds.
    let mut closing = if keys.z > 0.0 {
        lock.hold = None;
        glide(d - stop, brake).min(cruise * keys.z)
    } else if keys.z < 0.0 {
        lock.hold = None;
        cruise * keys.z
    } else {
        let hold = *lock.hold.get_or_insert(d.max(stop));
        glide(d - hold, GRIP_ACCEL)
    };
    // Around it: as fast as the side thrusters can turn the circle and the suit can turn to keep
    // facing it, leaning in for flight assist's lag so the circle doesn't widen.
    let orbit = cruise.min((side * d).sqrt()).min(ORBIT_TURN * spec.ambac_rate * d);
    let across = orbit * keys.x;
    closing += across * across * FA_RESPONSE / d.max(stop);
    // Off the floor, or back down onto it.
    let rising = if keys.y != 0.0 {
        cruise * keys.y
    } else if floor {
        glide(h, GRIP_ACCEL).clamp(-SETTLE_SHARE * cruise, SETTLE_SHARE * cruise)
    } else {
        0.0
    };
    // Relative to the target; and whatever the reference's cap took off the target's velocity.
    let want = toward * closing + around * across + up * rising + (vt - axes.ref_vel);
    let stick = Vec3::new(want.dot(axes.right), want.dot(axes.up), want.dot(axes.fwd)) / cruise.max(1.0);
    let stick = stick / stick.abs().max_element().max(1.0);
    let q8 = |v: f32| (v * 127.0).round().clamp(-127.0, 127.0) as i8;
    out.thrust = [q8(stick.x), q8(stick.y), q8(stick.z)];
    out.lockon = probe.lockon;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A point mass under flight assist's first-order lag, flown by `glide` toward a stop: it never
    /// passes the stop by more than a hand's breadth, from far and fast or near and slow.
    #[test]
    fn glide_stops_short_of_the_stop() {
        for &(gap, a) in &[(3_000.0f32, 16.5f32), (400.0, 30.0), (40.0, 6.0), (5.0, 16.5), (0.5, 6.0)] {
            let (mut x, mut v) = (gap, 0.0f32);
            let mut min_x = x;
            for _ in 0..30 * 120 {
                let want = glide(x, a);
                // Flight assist eases onto it, no harder than the thrusters (here, a / PLAN).
                let dv = ((want - v) / FA_RESPONSE * DT).clamp(-a / PLAN * DT, a / PLAN * DT);
                v += dv;
                x -= v * DT;
                min_x = min_x.min(x);
            }
            // (Coming in from far, the thrusters' limit can carry it a hand's breadth past.)
            assert!(min_x > -0.5, "{gap} m at {a} m/s²: passed the stop by {}", -min_x);
            assert!(x.abs() < 0.5, "{gap} m at {a} m/s²: settled {x} off it");
        }
        assert_eq!(glide(0.0, 10.0), 0.0);
        assert!(glide(-100.0, 10.0) < 0.0);
    }

    #[test]
    fn w_stops_outside_the_blade() {
        assert!((stop_range(FrameId::Leo) - (9.0 * 0.7 + 5.0)).abs() < 1e-4);
        assert!(stop_range(FrameId::Deathscythe) > stop_range(FrameId::Heavyarms));
    }
}
