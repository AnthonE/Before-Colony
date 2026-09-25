//! Parameters for the weapons that aren't straight-flying projectiles: blades and claws, homing
//! missiles, and the flamethrower's cone.

use glam::Vec3;

/// How a blade moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stroke {
    /// Sweeps an arc from `arc_from` to `arc_to` (in the suit's frame), `range` long.
    Swing,
    /// Drives straight out along the aim to `range` and back (Shenlong's Dragon Fang).
    Thrust,
}

/// A melee weapon's motion. Its reach is the weapon's `range`, the blade's radius its `radius`.
#[derive(Clone, Copy, Debug)]
pub struct MeleeSpec {
    pub stroke: Stroke,
    /// Phases, in ticks: winding up, the stroke itself (when it can hit), recovering.
    pub windup: u8,
    pub active: u8,
    pub recovery: u8,
    /// Recovery after a clash (both strikes parried).
    pub clash_recovery: u8,
    /// Blade direction at the start and the end of a swing, suit frame (x right, y up, z forward).
    pub arc_from: Vec3,
    pub arc_to: Vec3,
    /// Samples along the stroke per active tick (a long blade's tip travels far in a tick).
    pub sub_steps: u8,
    /// A second blade, from the other hand, mirrored left-right (heat shotels).
    pub twin: bool,
    /// Needs both arms (Cross Crusher); otherwise a twin strike loses the blade of a missing arm.
    pub both_arms: bool,
    /// Drives the suit forward through the windup and the stroke.
    pub lunge: bool,
    /// Meets other clashable blades: both strikes are parried.
    pub clashable: bool,
}

impl MeleeSpec {
    /// Ticks from the start of a strike until the weapon can start another (with its cooldown).
    pub fn duration(&self) -> u16 {
        u16::from(self.windup) + u16::from(self.active) + u16::from(self.recovery)
    }
}

/// A homing missile's motor, seeker and lock.
#[derive(Clone, Copy, Debug)]
pub struct MissileSpec {
    /// Speed off the rail, m/s, on top of the launcher's velocity.
    pub launch_speed: f32,
    /// The motor's thrust as acceleration, m/s².
    pub accel: f32,
    /// The motor's total Δv, m/s. Once spent the missile coasts: it can no longer steer.
    pub dv: f32,
    /// Proportional-navigation gain.
    pub nav: f32,
    /// Flight time, ticks.
    pub life: u16,
    /// Proximity fuse radius, m.
    pub fuse: f32,
    /// The seeker holds a target within this range (m) and half-angle off the nose (rad).
    pub seeker_range: f32,
    pub seeker_cone: f32,
    /// Acquiring a lock: the target within this range (m) and half-angle off the aim (rad), held
    /// for this many ticks.
    pub lock_range: f32,
    pub lock_cone: f32,
    pub lock_ticks: u8,
}

/// A flamethrower's cone. Its reach is the weapon's `range`; each application does the weapon's
/// `damage` and costs one round.
#[derive(Clone, Copy, Debug)]
pub struct ConeSpec {
    /// Half-angle, rad.
    pub half_angle: f32,
    /// Ticks between applications.
    pub interval: u8,
    /// Heat each application puts into a target (it can drive it to overheat).
    pub target_heat: f32,
}
