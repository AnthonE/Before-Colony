//! The kit-aware pilot: the Mobile Doll's judgement (targets, footwork) flying a Gundam's whole
//! kit. Agents and the browser autopilot use it; the server's own Mobile Dolls and a ZERO seizure
//! keep the plain doll's reflexes.
//!
//! - **Weapons by range band.** Guns fire inside their reach (leading with the gun that fits the
//!   range); launchers once the lock is acquired; the flamethrower close in; the Dragon Fang
//!   between a third of its reach and all of it; blades when the target will be within reach by
//!   the end of the windup.
//! - **Footwork.** A melee-first frame pursues: it closes to a blade's length, boosting from afar.
//!   The others keep the doll's range and strafing, at the frame's preferred distance.
//! - **Specials.** Neo-Bird for long hauls, the jammer while closing, Full Open with a lock inside
//!   1.2 km, the Cross Crusher at arm's length.
//! - **Care.** It breaks sideways from a missile tracking it, and eases off before G-strain blacks
//!   its pilot out.

use bc_proto::buttons::{
    BOOST, FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, MELEE, MODE, RCS_SHARP, SPECIAL,
};
use bc_proto::{InputCmd, NO_SLOT};
use glam::Vec3;

use super::mobile_doll::{Action, AiState, DollProfile};
use crate::config::{DT, G0};
use crate::content::{FrameSpec, SpecialKind, WeaponClass, weapon};
use crate::math::{angle_between, floor, length, normalize_or, sin, sqrt};
use crate::perception::{Contact, SelfView};
use crate::zero::fire_control;

/// Neo-Bird: taken for targets (or a way home) further than this, m...
const BIRD_OUT: f32 = 4_500.0;
/// ...and left once they're closer than this.
const BIRD_IN: f32 = 3_000.0;
/// The jammer goes on beyond this range (closing in unseen), m, with more than this energy, and
/// stays on down to the second.
const JAM_RANGE: f32 = 0.0;
const JAM_ENERGY_ON: f32 = 0.5;
const JAM_ENERGY_OFF: f32 = 0.15;
/// Full Open: with a lock, inside this range, with this much hull.
const FULL_OPEN_RANGE: f32 = 1_200.0;
const FULL_OPEN_HULL: f32 = 0.35;
/// The Cross Crusher's pincer at this range, m.
const CRUSH_RANGE: f32 = 11.0;
/// Blades swing at targets within this angle of the nose.
const SWING_CONE: f32 = 0.52; // 30°
/// Pursuit boosts beyond this range, m, closes no faster than it can brake from at this
/// deceleration (m/s²: 3 g, within what a pilot bears), and weaves on the way in beyond the second
/// range (the dolls lead linearly).
const BOOST_RANGE: f32 = 600.0;
const PURSUIT_BRAKING: f32 = 30.0;
const WEAVE_RANGE: f32 = 250.0;
/// A pursuer points its main engine where its velocity has to go (side thrusters are weak), and
/// turns to face its target within this range, m.
const FACE_RANGE: f32 = 120.0;
/// Above this G-strain it stops boosting and flies at an acceleration a pilot can bear for good
/// (below the 6 g that builds strain); closing for the kill, it spends more.
const STRAIN_EASE: f32 = 0.5;
const STRAIN_EASE_CLOSE: f32 = 0.7;
const CLOSE_RANGE: f32 = 300.0;
const SUSTAINED_G: f32 = 5.0;

/// This tick's command for a kit-aware pilot.
pub fn drive_kit(
    me: &SelfView,
    target: Option<&Contact>,
    ai: &mut AiState,
    tick: u32,
    profile: &DollProfile,
    spec: &FrameSpec,
) -> InputCmd {
    let mut buttons = FLIGHT_ASSIST | RCS_SHARP;
    let fwd = me.forward();
    // Presses (MELEE, SPECIAL) act on their first tick: offered every other tick they're edges.
    let edge = tick.is_multiple_of(2);
    let to_anchor = ai.anchor - me.pos;
    let far = target.map_or(length(to_anchor), |t| t.dist);

    let (aim, mut desired) = match target {
        Some(t) if t.hostile => {
            let los = normalize_or(t.pos - me.pos, fwd);
            // Lead with a gun that reaches, else look straight at it (blades, fang, flame, launchers).
            let gun = (0..2).find(|&s| {
                spec.loadout[s].is_some_and(|m| {
                    let w = weapon(m.weapon);
                    matches!(w.class, WeaponClass::Beam | WeaponClass::Ballistic)
                        && t.dist < w.range * profile.fire_range_frac
                })
            });
            let desired = footwork(me, t, los, ai, tick, profile, spec, &mut buttons);
            let cruise = spec.fa_speed * if buttons & BOOST != 0 { 1.8 } else { 1.0 };
            let push = desired * cruise - me.vel;
            let aim = match gun.and_then(|s| spec.loadout[s]) {
                _ if spec.ai.melee_first && t.dist > FACE_RANGE && length(push) > 40.0 => {
                    normalize_or(push, los)
                }
                Some(m) => {
                    let w = weapon(m.weapon);
                    let muzzle = me.pos + me.rot * m.arm.muzzle();
                    fire_control::intercept(muzzle, me.vel, w.speed, t.pos, t.vel, Vec3::ZERO)
                        .map_or(los, |sol| sol.dir)
                }
                None => los,
            };
            let aim = normalize_or(aim + ai.aim_noise, aim);
            buttons |= weapons(me, t, los, aim, spec, profile, edge);
            buttons |= special_press(me, t, los, spec, edge);
            (aim, desired)
        }
        _ => {
            // Patrol: orbit the anchor; regroup: head back to it.
            let d = length(to_anchor);
            let radial = normalize_or(to_anchor, fwd);
            let tangent = normalize_or(radial.cross(Vec3::Y), Vec3::X);
            let desired = if ai.action == Action::Regroup || d > 2_500.0 {
                radial
            } else {
                tangent * 0.5 + radial * ((d - 1_200.0) / 1_500.0).clamp(-0.5, 0.5)
            };
            (normalize_or(desired, fwd), desired)
        }
    };

    // Toggled specials, held on MODE.
    let hostile = target.filter(|t| t.hostile);
    let mode = match spec.special {
        // Neo-Bird for the long haul (the bird is the form MODE holds).
        SpecialKind::Transform { .. } => {
            let bird = !spec.playable;
            far > if bird { BIRD_IN } else { BIRD_OUT }
        }
        SpecialKind::HyperJammer { .. } => {
            let keep = if me.kit.special_active { JAM_ENERGY_OFF } else { JAM_ENERGY_ON };
            hostile.is_some_and(|t| t.dist > JAM_RANGE) && me.energy > keep
        }
        _ => false,
    };
    if mode {
        buttons |= MODE;
    }

    // A missile on its way: break sideways, hard.
    if me.kit.missile_incoming && hostile.is_none_or(|t| t.dist > 60.0) {
        let across = hostile.map_or(fwd, |t| normalize_or(t.pos - me.pos, fwd));
        desired = normalize_or(across.cross(me.rot * Vec3::Y), Vec3::X) * ai.strafe_sign;
        buttons |= BOOST;
    }
    // Flight assist closes any gap to the velocity it's asked for at full thrust, which a Gundam's
    // pilot can't bear for long. Strained, it flies unassisted instead, accelerating toward the
    // same velocity at what a pilot bears for good.
    let ease = if hostile.is_some_and(|t| t.dist < CLOSE_RANGE) { STRAIN_EASE_CLOSE } else { STRAIN_EASE };
    let local = if me.g_strain > ease {
        buttons &= !(BOOST | FLIGHT_ASSIST);
        let want = (desired * spec.fa_speed - me.vel) / 0.5;
        let accel = me.rot.conjugate() * want.clamp_length_max(SUSTAINED_G * G0);
        let mass = spec.mass(me.propellant * spec.propellant_cap);
        let forward = if accel.z >= 0.0 { spec.main_thrust } else { spec.retro_thrust };
        Vec3::new(accel.x / spec.side_thrust, accel.y / spec.side_thrust, accel.z / forward) * mass
    } else {
        me.rot.conjugate() * desired
    };

    if buttons & (FIRE_PRIMARY | FIRE_SECONDARY) != 0 {
        ai.shot_seq = ai.shot_seq.wrapping_add(1);
    }
    let q = |v: f32| floor(v.clamp(-1.0, 1.0) * 127.0 + 0.5) as i8;
    InputCmd {
        tick,
        view_tick_q4: tick << 4,
        aim,
        thrust: [q(local.x), q(local.y), q(local.z)],
        roll: 0,
        buttons,
        lock_target: hostile.map_or(NO_SLOT, |t| t.slot),
        shot_seq: ai.shot_seq,
    }
}

/// Which weapons to fire at `t`.
fn weapons(
    me: &SelfView,
    t: &Contact,
    los: Vec3,
    aim: Vec3,
    spec: &FrameSpec,
    p: &DollProfile,
    edge: bool,
) -> u16 {
    let fwd = me.forward();
    let mut buttons = 0;
    // A jamming suit holds its fire: a shot would show it for the jammer's break.
    let unseen = me.kit.special_active && matches!(spec.special, SpecialKind::HyperJammer { .. });
    for (slot, button) in [(0, FIRE_PRIMARY), (1, FIRE_SECONDARY)] {
        if unseen {
            break;
        }
        let Some(m) = spec.loadout[slot] else { continue };
        if !me.ready[slot] {
            continue;
        }
        let w = weapon(m.weapon);
        let fire = match w.class {
            WeaponClass::Beam | WeaponClass::Ballistic => {
                t.dist < w.range * p.fire_range_frac && angle_between(aim, fwd) < m.arm.cone() * 0.9
            }
            WeaponClass::Missile => {
                me.kit.lock_acquired && w.missile.is_some_and(|ms| t.dist < ms.lock_range)
            }
            WeaponClass::Cone => w.cone.is_some_and(|c| {
                t.dist < w.range * 0.9 && angle_between(los, fwd) < m.arm.cone().min(c.half_angle * 3.0)
            }),
            // A blade in a gun slot (the Dragon Fang) thrusts along the aim.
            WeaponClass::Melee => {
                t.dist > w.range * 0.35 && t.dist < w.range + 3.0 && angle_between(los, fwd) < m.arm.cone()
            }
        };
        if fire && !me.overheated {
            buttons |= button;
        }
    }
    // The melee slot swings when the target will be well inside its reach mid-stroke, closing as it
    // is and lunging (a blade's arc reaches furthest ahead partway through the stroke).
    if let Some(m) = spec.loadout[2]
        && let Some(ms) = weapon(m.weapon).melee
        && me.ready[2]
        && edge
    {
        let closing = (me.vel - t.vel).dot(los).max(0.0);
        let secs = f32::from(ms.windup + ms.active / 2) * DT;
        let lunge = if ms.lunge {
            let accel = 1.5 * spec.main_thrust / spec.mass(me.propellant * spec.propellant_cap);
            0.5 * accel * secs * secs
        } else {
            0.0
        };
        let mid_stroke = t.dist - closing * secs - lunge;
        if mid_stroke < weapon(m.weapon).range * 0.7 + 2.0 && angle_between(los, fwd) < SWING_CONE {
            buttons |= MELEE;
        }
    }
    buttons
}

/// A pressed special (Full Open, the Cross Crusher), if now's the time.
fn special_press(me: &SelfView, t: &Contact, los: Vec3, spec: &FrameSpec, edge: bool) -> u16 {
    if !edge || !me.kit.special_ready {
        return 0;
    }
    let now = match spec.special {
        SpecialKind::FullOpen { .. } => {
            me.kit.lock_acquired && t.dist < FULL_OPEN_RANGE && me.hull() > FULL_OPEN_HULL
        }
        SpecialKind::MeleeMove { .. } => {
            t.dist < CRUSH_RANGE && angle_between(los, me.forward()) < SWING_CONE
        }
        _ => false,
    };
    if now { SPECIAL } else { 0 }
}

/// Where to fly relative to `t` (a velocity for flight assist, as a share of cruise).
#[allow(clippy::too_many_arguments)]
fn footwork(
    me: &SelfView,
    t: &Contact,
    los: Vec3,
    ai: &AiState,
    tick: u32,
    profile: &DollProfile,
    spec: &FrameSpec,
    buttons: &mut u16,
) -> Vec3 {
    if spec.ai.melee_first {
        // Pursue to just outside a blade's length (the lunge carries it in): match its velocity,
        // and close on top of that no faster than it can brake from, slowing to a creep near the
        // end, weaving on the way in.
        let reach = spec.loadout[2].map_or(10.0, |m| weapon(m.weapon).range);
        let gap = (t.dist - (reach * 0.7 + 5.0)).max(0.0);
        let boost = t.dist > BOOST_RANGE;
        let cruise = spec.fa_speed.max(1.0) * if boost { 1.8 } else { 1.0 };
        if boost {
            *buttons |= BOOST;
        }
        let closing = sqrt(2.0 * PURSUIT_BRAKING * gap).min((1.5 * gap).max(40.0)).min(cruise);
        let mut v = t.vel + los * closing;
        if t.dist > WEAVE_RANGE {
            let lateral = normalize_or(los.cross(me.rot * Vec3::Y), Vec3::X);
            v += lateral * (sin(tick as f32 * 0.2) * 0.35 * spec.fa_speed);
        }
        return v / cruise;
    }
    let range_err = t.dist - profile.preferred_range;
    let radial = los * (range_err / 800.0).clamp(-1.0, 1.0);
    let lateral = normalize_or(los.cross(me.rot * Vec3::Y), Vec3::X) * ai.strafe_sign;
    match ai.action {
        Action::Strafe => radial * 0.6 + lateral * 0.9,
        Action::Engage => radial + lateral * 0.25,
        Action::Evade => {
            *buttons |= BOOST;
            ai.evade_dir
        }
        Action::Retreat => {
            *buttons |= BOOST;
            -los
        }
        _ => radial,
    }
}
