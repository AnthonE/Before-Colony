use bc_proto::buttons::{BOOST, FIRE_PRIMARY, FLIGHT_ASSIST, RCS_SHARP};
use bc_proto::{InputCmd, NO_SLOT, Part};
use glam::Vec3;

use crate::content::{FrameSpec, weapon};
use crate::math::{Rng, angle_between, length, normalize_or};
use crate::perception::{Contact, Perception, SelfView};
use crate::zero::fire_control;

/// What a doll is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Action {
    #[default]
    Patrol,
    Engage,
    Strafe,
    Evade,
    Retreat,
    Regroup,
}

/// Per-doll brain state.
#[derive(Clone, Copy, Debug)]
pub struct AiState {
    pub action: Action,
    pub target: u16,
    pub think_at: u32,
    pub strafe_sign: f32,
    pub strafe_flip_at: u32,
    pub evade_until: u32,
    pub evade_dir: Vec3,
    /// Patrol anchor.
    pub anchor: Vec3,
    pub squad: u8,
    /// Squad focus target (set by squad logic or the tactical oracle).
    pub order_target: u16,
    pub rng: u32,
    /// Current aim error (re-rolled when thinking).
    pub aim_noise: Vec3,
    pub shot_seq: u8,
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            action: Action::Patrol,
            target: NO_SLOT,
            think_at: 0,
            strafe_sign: 1.0,
            strafe_flip_at: 0,
            evade_until: 0,
            evade_dir: Vec3::X,
            anchor: Vec3::ZERO,
            squad: 0,
            order_target: NO_SLOT,
            rng: 1,
            aim_noise: Vec3::ZERO,
            shot_seq: 0,
        }
    }
}

/// Tunables for a brain.
#[derive(Clone, Copy, Debug)]
pub struct DollProfile {
    /// Distance the doll tries to fight at, m.
    pub preferred_range: f32,
    /// Aim error, radians (1σ-ish).
    pub aim_error: f32,
    /// Only fire when the target is within this fraction of weapon range.
    pub fire_range_frac: f32,
    /// Seconds between strafe reversals: min + random span.
    pub strafe_min_s: f32,
    pub strafe_span_s: f32,
}

/// OZ Mobile Doll defaults.
pub const DOLL: DollProfile = DollProfile {
    preferred_range: 1_600.0,
    aim_error: 0.0025,
    fire_range_frac: 0.8,
    strafe_min_s: 2.0,
    strafe_span_s: 3.0,
};

/// A ZERO seizure: closer, sharper, relentless.
pub const SEIZED: DollProfile = DollProfile {
    preferred_range: 700.0,
    aim_error: 0.0008,
    fire_range_frac: 0.9,
    strafe_min_s: 1.0,
    strafe_span_s: 1.0,
};

fn rng_of(ai: &AiState, tick: u32) -> Rng {
    Rng::new(u64::from(ai.rng) << 32 | u64::from(tick))
}

/// Re-plans: picks a target and an action by utility.
pub fn think(p: &Perception, ai: &mut AiState, tick: u32, profile: &DollProfile, primary_range: f32) {
    let mut rng = rng_of(ai, tick);
    // --- Target selection (with hysteresis). ---
    let mut best: Option<(&Contact, f32)> = None;
    for c in p.hostiles() {
        if c.hull <= 0.0 {
            continue;
        }
        let mut score = 1.0 / (1.0 + c.dist / 1_500.0);
        if c.locked_on_me || c.aiming_at_me {
            score *= 1.6;
        }
        if c.slot == ai.order_target {
            score *= 2.0;
        }
        if c.slot == ai.target {
            score *= 1.3;
        }
        score *= 1.0 + 0.5 * (1.0 - c.hull);
        if best.is_none_or(|(_, s)| score > s) {
            best = Some((c, score));
        }
    }
    ai.target = best.map_or(NO_SLOT, |(c, _)| c.slot);

    // --- Action utilities. ---
    let hull = p.me.parts[Part::Torso as usize];
    let mut options: [(Action, f32); 6] = [
        (Action::Patrol, 0.2),
        (Action::Engage, 0.0),
        (Action::Strafe, 0.0),
        (Action::Evade, 0.0),
        (
            Action::Retreat,
            if hull < 0.25 {
                0.95
            } else if p.me.energy < 0.08 {
                0.45
            } else {
                0.0
            },
        ),
        (Action::Regroup, 0.0),
    ];
    if let Some((t, _)) = best {
        let close = t.dist <= profile.preferred_range * 1.3;
        options[1].1 = if close { 0.45 } else { 0.8 };
        options[2].1 = if close { 0.75 } else { 0.25 };
        if t.dist > primary_range * 1.5 {
            options[1].1 = 0.85;
        }
    } else if length(p.me.pos - ai.anchor) > 4_000.0 {
        options[5].1 = 0.8;
    }
    let incoming = p.hostiles().find(|c| c.aiming_at_me && c.firing && c.dist < 3_000.0);
    if let Some(att) = incoming
        && tick >= ai.evade_until
        && rng.next_f32() < 0.6
    {
        options[3].1 = 0.9;
        let los = normalize_or(p.me.pos - att.pos, Vec3::Z);
        let side = normalize_or(los.cross(p.me.rot * Vec3::Y), Vec3::X);
        let sign = if rng.next_f32() < 0.5 { -1.0 } else { 1.0 };
        ai.evade_dir = normalize_or(side * sign + (p.me.rot * Vec3::Y) * rng.signed() * 0.5, Vec3::X);
        ai.evade_until = tick + crate::config::secs(1.0);
    }
    if tick < ai.evade_until {
        options[3].1 = options[3].1.max(0.9);
    }
    let mut chosen = options[0];
    for o in options {
        if o.1 > chosen.1 {
            chosen = o;
        }
    }
    ai.action = chosen.0;

    // Predictable strafing rhythm: flip on a timer.
    if tick >= ai.strafe_flip_at {
        ai.strafe_sign = -ai.strafe_sign;
        ai.strafe_flip_at =
            tick + crate::config::secs(profile.strafe_min_s + rng.next_f32() * profile.strafe_span_s);
    }
    ai.aim_noise = Vec3::new(rng.signed(), rng.signed(), rng.signed()) * profile.aim_error;
}

/// Produces this tick's control command.
pub fn drive(
    me: &SelfView,
    target: Option<&Contact>,
    ai: &mut AiState,
    tick: u32,
    profile: &DollProfile,
    spec: &FrameSpec,
) -> InputCmd {
    let mut buttons = FLIGHT_ASSIST | RCS_SHARP;
    let fwd = me.forward();
    let primary = spec.loadout[0];

    // `desired`: world-frame velocity direction, |·| ≤ 1 (flight assist turns it into thrust).
    let (aim, desired) = if let (Some(t), Some(mount)) = (target, primary) {
        let w = weapon(mount.weapon);
        let muzzle = me.pos + me.rot * mount.arm.muzzle();
        // Perfect *linear* lead: assumes the target keeps its current velocity.
        let lead = match fire_control::intercept(muzzle, me.vel, w.speed, t.pos, t.vel, Vec3::ZERO) {
            Some(sol) => sol.dir,
            None => normalize_or(t.pos - me.pos, fwd),
        };
        let aim = normalize_or(lead + ai.aim_noise, lead);
        let los = normalize_or(t.pos - me.pos, fwd);
        let range_err = t.dist - profile.preferred_range;
        let radial = los * (range_err / 800.0).clamp(-1.0, 1.0);
        let lateral = normalize_or(los.cross(me.rot * Vec3::Y), Vec3::X) * ai.strafe_sign;
        let desired = match ai.action {
            Action::Strafe => radial * 0.6 + lateral * 0.9,
            Action::Engage => radial + lateral * 0.25,
            Action::Evade => {
                buttons |= BOOST;
                ai.evade_dir
            }
            Action::Retreat => {
                buttons |= BOOST;
                -los
            }
            _ => radial,
        };
        let in_range = t.dist < w.range * profile.fire_range_frac;
        let can_point = angle_between(aim, fwd) < mount.arm.cone() * 0.9;
        if in_range && can_point && me.ready[0] && !me.overheated && t.hostile {
            buttons |= FIRE_PRIMARY;
        }
        (aim, desired)
    } else {
        // Patrol: orbit the anchor; regroup: head back to it.
        let to_anchor = ai.anchor - me.pos;
        let d = length(to_anchor);
        let radial = normalize_or(to_anchor, fwd);
        let tangent = normalize_or(radial.cross(Vec3::Y), Vec3::X);
        let desired = if ai.action == Action::Regroup || d > 2_500.0 {
            radial
        } else {
            tangent * 0.5 + radial * ((d - 1_200.0) / 1_500.0).clamp(-0.5, 0.5)
        };
        (normalize_or(desired, fwd), desired)
    };

    if buttons & FIRE_PRIMARY != 0 {
        ai.shot_seq = ai.shot_seq.wrapping_add(1);
    }
    let local = me.rot.conjugate() * desired;
    let q = |v: f32| (v.clamp(-1.0, 1.0) * 127.0) as i8;
    InputCmd {
        tick,
        view_tick_q4: tick << 4,
        aim,
        thrust: [q(local.x), q(local.y), q(local.z)],
        roll: 0,
        buttons,
        lock_target: target.map_or(NO_SLOT, |t| t.slot),
        shot_seq: ai.shot_seq,
    }
}
