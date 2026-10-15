//! What's under the pilot's crosshair: how far along the aim from their eye the first thing they
//! can see there is. The command carries it (`InputCmd::converge`), and the guns aim at that point
//! (`bc_sim::aim`), so a shot goes where the crosshair is; the HUD draws the crosshair there when
//! the camera isn't the eye (the chase camera).
//!
//! What it looks for is what the shots meet: inside the colony, the Blast Hall's targets and walls
//! (where its pilot sees them); out in space, the rocks, the landmarks, the colony's hull and the
//! suits as they're drawn. Nothing there: the locked target's range, else [`OPEN_RANGE`].

use bc_proto::snapshot::ent_flags;
use bc_sim::bodies::sweep_landmarks;
use bc_sim::collide::sweep_capsules;
use bc_sim::colony::hall;
use bc_sim::content::frame;
use glam::{Quat, Vec3};

use crate::ClientCore;

/// With nothing under the crosshair, how far off its point is taken to be, m: as far as a fight is.
pub const OPEN_RANGE: f32 = 1_500.0;
/// How far out the crosshair looks for something, m: past the longest gun's reach.
pub const REACH: f32 = 12_000.0;
/// Inside the colony, how far: across the Blast Hall and more.
const HALL_REACH: f32 = 400.0;

/// The own suit's eye as drawn now: where the cockpit's camera is and the crosshair's line starts.
pub fn eye(core: &ClientCore) -> Option<(Vec3, Quat)> {
    let v = core.own_view().filter(|v| v.alive)?;
    Some((v.pos + v.rot * frame(v.frame).eye(), v.rot))
}

/// How far along `aim` from `eye` the first thing the pilot sees is, at `t` (render-clock ticks:
/// the targets and the suits as they're drawn).
pub fn range(core: &ClientCore, eye: Vec3, aim: Vec3, t: f64) -> f32 {
    let (k, frac) = (t.max(0.0).floor() as u32, (t.max(0.0) - t.max(0.0).floor()) as f32);
    if core.inside() {
        // Only the Blast Hall's rounds fly in here: its targets, its walls.
        return hall::shot_end(eye, eye + aim * HALL_REACH, 0.0, k, frac)
            .map_or(OPEN_RANGE, |(f, _)| f * HALL_REACH);
    }
    let far = eye + aim * REACH;
    let mut nearest = f32::INFINITY;
    let mut meet = |s: f32| nearest = nearest.min(s * REACH);
    if let Some((s, _)) = core.predict.field.sweep(eye, far, 0.0) {
        meet(s);
    }
    if let Some((s, _)) = sweep_landmarks(core.world.bodies.landmarks(), eye, far, 0.0, k, frac) {
        meet(s);
    }
    if let Some(s) = bc_sim::world::colony_sweep(eye, far, 0.0) {
        meet(s);
    }
    let world = &core.world;
    let own = world.own_slot();
    for (slot, track) in world.entities.iter().enumerate() {
        let Some(track) = track else { continue };
        if Some(slot as u16) == own || track.latest.flags & ent_flags::WRECK != 0 {
            continue;
        }
        let pose = track.sample(t, &world.bodies);
        // (Only those near enough the line to matter.)
        let along = (pose.pos - eye).dot(aim);
        if along <= 0.0 || (pose.pos - eye - aim * along).length() > 40.0 {
            continue;
        }
        let caps = &frame(track.latest.frame).capsules;
        if let Some((s, _)) = sweep_capsules(eye, far, 0.0, caps, pose.pos, pose.rot, 0) {
            meet(s);
        }
    }
    if nearest.is_finite() {
        return nearest;
    }
    // The locked target, ahead.
    world
        .own
        .filter(|o| o.lock_target != bc_proto::NO_SLOT)
        .and_then(|o| world.pose(o.lock_target, t))
        .map(|p| (p.pos - eye).dot(aim))
        .filter(|d| *d > 0.0)
        .unwrap_or(OPEN_RANGE)
}
