//! Weapons and battle effects, drawn from the view model (the [`BeamFeed`], [`FxEvent`]s and the
//! suits' [`SuitDrive`]s):
//! - beams and machine-cannon tracers as glowing ribbons (pooled, see `beams`);
//! - muzzle flashes, impacts, saber clashes, the Twin Buster Rifle's charge, ring and ionised
//!   trail, attitude jets and suits exploding, as particles (`particles`), shells and armour
//!   chips (`blast`);
//! - and the point lights those effects cast on the suits around them.

use std::collections::HashMap;

use bc_proto::WeaponKind;
use bc_proto::snapshot::ent_flags;
use bc_sim::content::frame;
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use crate::beams::{BeamMaterial, Ribbons, place_ribbon};
use crate::blast::Blasts;
use crate::camera::MainCamera;
use crate::gfx::Gfx;
use crate::particles::{At, Particles};
use crate::suits_vis::{SABER_DIR, SABER_HILT};
use crate::view::{BeamFeed, FxEvent, FxEvents, SuitDrive, VisTime};

const BEAMS: usize = 96;
const TRACERS: usize = 96;
/// Point lights for effects (the most any tier uses).
const LIGHTS: usize = 24;

#[derive(Component)]
pub struct BeamVis(usize);
#[derive(Component)]
pub struct TracerVis(usize);
#[derive(Component)]
pub struct FxLight(usize);

struct Tracer {
    origin: Vec3,
    vel: Vec3,
    born: f64,
}

/// A burst of light from an effect (muzzle, hit, blast, clash), fading over `life`.
struct Flash {
    pos: Vec3,
    born: f64,
    life: f64,
    lumens: f32,
    color: Color,
}

#[derive(Resource, Default)]
pub struct FxState {
    tracers: Vec<Tracer>,
    flashes: Vec<Flash>,
    last_tracer: HashMap<u16, f64>,
}

pub fn setup_fx(mut commands: Commands, ribbons: Res<Ribbons>) {
    for i in 0..BEAMS {
        commands.spawn((
            BeamVis(i),
            Mesh3d(ribbons.mesh.clone()),
            MeshMaterial3d(ribbons.rifle.material.clone()),
            MeshTag(i as u32 * 37 % 256),
            Transform::default(),
            Visibility::Hidden,
        ));
    }
    for i in 0..TRACERS {
        commands.spawn((
            TracerVis(i),
            Mesh3d(ribbons.mesh.clone()),
            MeshMaterial3d(ribbons.tracer.material.clone()),
            MeshTag(i as u32 * 53 % 256),
            Transform::default(),
            Visibility::Hidden,
        ));
    }
    for i in 0..LIGHTS {
        commands.spawn((
            FxLight(i),
            PointLight { intensity: 0.0, range: 1.0, shadow_maps_enabled: false, ..default() },
            Transform::default(),
            Visibility::Hidden,
        ));
    }
}

/// Shows or hides a pooled entity, writing only on change (no change-detection churn).
fn show(v: &mut Visibility, on: bool) {
    let want = if on { Visibility::Visible } else { Visibility::Hidden };
    if *v != want {
        *v = want;
    }
}

fn color(v: Vec3) -> Color {
    let m = v.max_element().max(1e-3);
    Color::linear_rgb(v.x / m, v.y / m, v.z / m)
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_fx(
    time: Res<VisTime>,
    gfx: Res<Gfx>,
    ribbons: Res<Ribbons>,
    feed: Res<BeamFeed>,
    mut events: ResMut<FxEvents>,
    suits: Query<&SuitDrive>,
    cams: Query<&GlobalTransform, With<MainCamera>>,
    mut state: ResMut<FxState>,
    mut particles: ResMut<Particles>,
    mut blasts: ResMut<Blasts>,
    mut beams: Query<
        (&BeamVis, &mut Transform, &mut Visibility, &mut MeshMaterial3d<BeamMaterial>),
        Without<TracerVis>,
    >,
    mut tracers: Query<(&TracerVis, &mut Transform, &mut Visibility), Without<BeamVis>>,
) {
    let now = time.now;
    let cap = gfx.settings.particles;
    let eye = cams.single().map_or(Vec3::ZERO, |c| c.translation());

    // --- Beams. ---
    for (vis, mut tf, mut v, mut mat) in &mut beams {
        match feed.0.get(vis.0) {
            Some(b) => {
                let look = ribbons.look(b.weapon);
                let length = look.length.min(b.travelled.max(1.0));
                place_ribbon(&mut tf, b.head, b.dir, length, look.half_width);
                if mat.0 != look.material {
                    mat.0 = look.material.clone();
                }
                show(&mut v, true);
                if b.weapon == WeaponKind::TwinBusterRifle {
                    particles.trail(cap, b.head, b.dir, length, look.color, time.dt);
                }
            }
            None => show(&mut v, false),
        }
    }

    // --- Machine-cannon tracers from anyone firing their secondary. ---
    for d in &suits {
        if d.flags & ent_flags::FIRING_SECONDARY == 0 {
            continue;
        }
        let Some(m) = frame(d.frame).loadout[1] else { continue };
        let last = state.last_tracer.get(&d.slot).copied().unwrap_or(f64::NEG_INFINITY);
        if now - last > 0.1 {
            state.last_tracer.insert(d.slot, now);
            let origin = d.pos + d.rot * m.arm.muzzle();
            let dir = d.aim.normalize_or(d.rot * Vec3::Z);
            state.tracers.push(Tracer { origin, vel: d.vel + dir * 1_200.0, born: now });
            particles.muzzle(cap, At { pos: origin, vel: d.vel }, dir, ribbons.tracer.color, 0.7);
            state.flashes.push(Flash {
                pos: origin,
                born: now,
                life: 0.06,
                lumens: 2.0e7,
                color: color(ribbons.tracer.color),
            });
        }
    }
    state.tracers.retain(|tr| now - tr.born < 1.2);
    state.last_tracer.retain(|_, t| now - *t < 1.0);
    let n = state.tracers.len();
    if n > TRACERS {
        state.tracers.drain(..n - TRACERS);
    }
    let tracer = &ribbons.tracer;
    for (vis, mut tf, mut v) in &mut tracers {
        match state.tracers.get(vis.0) {
            Some(tr) => {
                let head = tr.origin + tr.vel * (now - tr.born) as f32;
                let travelled = head.distance(tr.origin);
                let dir = tr.vel.normalize_or(Vec3::Z);
                place_ribbon(&mut tf, head, dir, tracer.length.min(travelled.max(1.0)), tracer.half_width);
                show(&mut v, true);
            }
            None => show(&mut v, false),
        }
    }

    // --- Attitude jets: sideways and vertical thrust vents vapour the other way. ---
    for d in &suits {
        if d.flags & ent_flags::WRECK != 0 {
            continue;
        }
        let at = |local: Vec3| At { pos: d.pos + d.rot * local, vel: d.vel };
        let (x, y) = (d.thrust.x, d.thrust.y);
        if x.abs() > 0.25 {
            let s = x.signum();
            particles.jet(
                cap,
                at(Vec3::new(-s * 4.4, 4.4, 0.0)),
                d.rot * Vec3::X * -s,
                30.0 * x.abs(),
                time.dt,
            );
        }
        if y.abs() > 0.25 {
            let s = y.signum();
            for side in [-1.0, 1.0] {
                let nozzle = at(Vec3::new(side * 3.2, 4.4 - s * 1.2, 0.0));
                particles.jet(cap, nozzle, d.rot * Vec3::Y * -s, 15.0 * y.abs(), time.dt);
            }
        }
    }

    // --- The Twin Buster Rifle drawing in energy while it charges. ---
    for d in &suits {
        if d.flags & ent_flags::CHARGING != 0
            && d.flags & ent_flags::WRECK == 0
            && let Some(m) = frame(d.frame).loadout[0]
        {
            let muzzle = d.pos + d.rot * (m.arm.muzzle() + Vec3::Z * 5.0);
            particles.charge(cap, At { pos: muzzle, vel: d.vel }, time.dt);
            state.flashes.push(Flash {
                pos: muzzle,
                born: now,
                life: 0.05,
                lumens: 6.0e7,
                color: Color::linear_rgb(0.9, 0.6, 1.0),
            });
        }
    }

    // --- One-shot effects. ---
    for ev in events.0.drain(..) {
        match ev {
            FxEvent::Hit { pos, weapon } => {
                let look = ribbons.look(weapon);
                let big = weapon == WeaponKind::TwinBusterRifle;
                // Sparks fly back toward the camera's side of the target.
                let normal = (eye - pos).normalize_or(Vec3::Y);
                let scale = if big { 3.0 } else { 1.0 };
                particles.impact(cap, At { pos, vel: Vec3::ZERO }, normal, look.color, scale);
                state.flashes.push(Flash {
                    pos,
                    born: now,
                    life: if big { 0.4 } else { 0.25 },
                    lumens: if big { 6.0e8 } else { 1.2e8 },
                    color: Color::srgb(1.0, 0.85, 0.6),
                });
            }
            FxEvent::Kill { pos } => {
                let at = At { pos, vel: Vec3::ZERO };
                particles.explosion(cap, at, 1.0);
                blasts.shockwave(pos, Vec3::ZERO, 70.0);
                blasts.chips(pos, Vec3::ZERO, 14);
                state.flashes.push(Flash {
                    pos,
                    born: now,
                    life: 1.4,
                    lumens: 1.6e9,
                    color: Color::srgb(1.0, 0.62, 0.3),
                });
            }
            FxEvent::Muzzle { pos, dir, vel, weapon } => {
                let look = ribbons.look(weapon);
                let buster = weapon == WeaponKind::TwinBusterRifle;
                let scale = if buster { 4.0 } else { 1.0 };
                particles.muzzle(cap, At { pos, vel }, dir, look.color, scale);
                if buster {
                    blasts.ring(pos + dir * 6.0, vel, dir, 28.0);
                }
                state.flashes.push(Flash {
                    pos,
                    born: now,
                    life: 0.1 * f64::from(scale),
                    lumens: 8.0e7 * scale,
                    color: color(look.color),
                });
            }
            // The camera shakes and flashes for this one (see `camera`).
            FxEvent::Struck { .. } => {}
            FxEvent::Clash { pos } => {
                particles.clash(cap, At { pos, vel: Vec3::ZERO });
                state.flashes.push(Flash {
                    pos,
                    born: now,
                    life: 0.3,
                    lumens: 2.5e8,
                    color: Color::srgb(1.0, 0.4, 0.75),
                });
            }
        }
    }
    state.flashes.retain(|f| now - f.born < f.life);
}

/// A light an effect wants this frame.
pub struct LightWish {
    pos: Vec3,
    color: Color,
    /// Luminous power, lm. Illuminance at distance d is `lumens / (4π d²)` lux; full sunlight is
    /// 100,000 lux.
    lumens: f32,
}

/// Lights the suits around flashes, blasts, sabers and Twin Buster beams: the pooled point lights
/// go to the effects nearest the camera, up to the tier's budget.
#[allow(clippy::too_many_arguments)]
pub fn update_fx_lights(
    time: Res<VisTime>,
    gfx: Res<Gfx>,
    ribbons: Res<Ribbons>,
    state: Res<FxState>,
    feed: Res<BeamFeed>,
    suits: Query<&SuitDrive>,
    cams: Query<&GlobalTransform, With<MainCamera>>,
    mut lights: Query<(&FxLight, &mut PointLight, &mut Transform, &mut Visibility)>,
    mut wishes: Local<Vec<LightWish>>,
) {
    let budget = gfx.settings.fx_lights.min(LIGHTS);
    wishes.clear();
    if budget > 0 {
        let now = time.now;
        for f in &state.flashes {
            let age = ((now - f.born) / f.life).clamp(0.0, 1.0) as f32;
            wishes.push(LightWish {
                pos: f.pos,
                color: f.color,
                lumens: f.lumens * (1.0 - age) * (1.0 - age),
            });
        }
        for d in &suits {
            if d.flags & ent_flags::SABER != 0 && d.flags & ent_flags::WRECK == 0 {
                // The middle of the blade in the left hand.
                let pos = d.pos + d.rot * (SABER_HILT + SABER_DIR * ribbons.saber.length * 0.5);
                wishes.push(LightWish { pos, color: Color::srgb(1.0, 0.3, 0.65), lumens: 3.0e7 });
            }
        }
        for b in &feed.0 {
            if b.weapon == WeaponKind::TwinBusterRifle {
                wishes.push(LightWish { pos: b.head, color: Color::srgb(0.95, 0.75, 1.0), lumens: 8.0e8 });
            }
        }
        if let Ok(cam) = cams.single() {
            let eye = cam.translation();
            wishes.sort_by(|a, b| a.pos.distance_squared(eye).total_cmp(&b.pos.distance_squared(eye)));
        }
    }
    for (l, mut light, mut tf, mut vis) in &mut lights {
        match wishes.get(l.0).filter(|_| l.0 < budget) {
            Some(w) => {
                light.intensity = w.lumens;
                light.color = w.color;
                // Out to where it adds about 1% of full sunlight.
                light.range = (w.lumens / (4.0 * std::f32::consts::PI * 1_000.0)).sqrt().max(1.0);
                tf.translation = w.pos;
                show(&mut vis, true);
            }
            None => show(&mut vis, false),
        }
    }
}
