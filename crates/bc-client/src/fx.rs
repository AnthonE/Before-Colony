//! Weapons and battle effects, drawn from the view model (the [`BeamFeed`], [`FxEvent`]s and the
//! suits' [`SuitDrive`]s):
//! - beams, and the tracers of stream weapons (gatlings, machine guns, vulcans: drawn from the
//!   firing flags at each weapon's speed and colour), as glowing ribbons (pooled, see `beams`);
//! - muzzle flashes, impacts, blade clashes, the Twin Buster Rifle's charge, ring and ionised
//!   trail, the flamethrower's jet, missile bursts, attitude jets and suits exploding, as
//!   particles (`particles`), shells and armour chips (`blast`);
//! - and the point lights those effects cast on the suits around them.

use std::collections::HashMap;

use bc_model::rig::Bone;
use bc_proto::WeaponKind;
use bc_proto::snapshot::ent_flags;
use bc_sim::config::DT;
use bc_sim::content::{ArmSlot, Mount, Replication, SpecialKind, WeaponClass, frame, weapon};
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use crate::anim::Anim;
use crate::beams::{BeamMaterial, Ribbons, place_ribbon};
use crate::blast::Blasts;
use crate::camera::MainCamera;
use crate::gfx::Gfx;
use crate::materials::{HullTag, paint};
use crate::model::SuitMeshLib;
use crate::particles::{At, Particles};
use crate::suits_vis::{bone_point, drawn_blades, flame_nozzle, plume_power};
use crate::view::{BeamFeed, FxEvent, FxEvents, MissileFeed, SuitDrive, VisTime};

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
    life: f64,
    weapon: WeaponKind,
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
    /// When each suit's mount last put out a tracer, by (slot, mount).
    last_tracer: HashMap<(u16, u8), f64>,
}

/// A suit's ranged mounts firing this frame: the loadout's guns by their firing flags, and during
/// Full Open Attack every gun, the special mounts' too. Mounts are numbered like the simulation's
/// (0, 1: the loadout's guns; 3, 4: the special mounts).
pub fn firing_mounts(d: &SuitDrive) -> impl Iterator<Item = (u8, Mount)> + '_ {
    let spec = frame(d.frame);
    let wreck = d.flags & ent_flags::WRECK != 0;
    let full_open = d.flags & ent_flags::SPECIAL != 0 && matches!(spec.special, SpecialKind::FullOpen { .. });
    let loadout =
        [(0u8, ent_flags::FIRING_PRIMARY), (1, ent_flags::FIRING_SECONDARY)].into_iter().filter_map(
            move |(k, bit)| spec.loadout[usize::from(k)].filter(|_| d.flags & bit != 0).map(|m| (k, m)),
        );
    let special = (0..2u8).filter_map(move |k| spec.special_mounts[usize::from(k)].map(|m| (k + 3, m)));
    loadout
        .chain(special.filter(move |_| full_open))
        .filter(move |(_, m)| !wreck && weapon(m.weapon).class != WeaponClass::Melee)
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
    lib: Res<SuitMeshLib>,
    feed: Res<BeamFeed>,
    mut events: ResMut<FxEvents>,
    suits: Query<(&SuitDrive, Option<&Anim>)>,
    cams: Query<&GlobalTransform, With<MainCamera>>,
    mut state: ResMut<FxState>,
    mut particles: ResMut<Particles>,
    mut blasts: ResMut<Blasts>,
    mut beams: Query<
        (&BeamVis, &mut Transform, &mut Visibility, &mut MeshMaterial3d<BeamMaterial>),
        Without<TracerVis>,
    >,
    mut tracers: Query<
        (&TracerVis, &mut Transform, &mut Visibility, &mut MeshMaterial3d<BeamMaterial>),
        Without<BeamVis>,
    >,
    field: Option<Res<crate::rocks::VisField>>,
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

    // --- Stream weapons: tracers at each weapon's own speed and colour, from the firing flags
    // (the simulation sends no event per round). The flamethrower's jet. ---
    for (d, anim) in &suits {
        let sockets = lib.sockets(d.frame);
        for (k, m) in firing_mounts(d) {
            let w = weapon(m.weapon);
            let main = k == 0 && m.arm == ArmSlot::Right;
            // The main weapon's drawn muzzle (the simulation's is at the hand, inside the gun), the
            // dragon's mouth for the flamethrower.
            let origin = if main {
                bone_point(d, anim, Bone::Weapon, sockets.muzzle)
            } else if w.cone.is_some() {
                flame_nozzle(d, anim, &lib, m.arm.muzzle())
            } else {
                d.pos + d.rot * m.arm.muzzle()
            };
            let dir = d.aim.normalize_or(d.rot * Vec3::Z);
            if let Some(cone) = w.cone {
                particles.flame(cap, At { pos: origin, vel: d.vel }, dir, w.range, cone.half_angle, time.dt);
                continue;
            }
            if w.replication != Replication::Stream {
                continue;
            }
            // About as often as it fires, but no more than every other frame at 30 fps.
            let every = (f64::from(w.cooldown) * f64::from(DT)).max(0.06);
            let last = state.last_tracer.get(&(d.slot, k)).copied().unwrap_or(f64::NEG_INFINITY);
            if now - last < every {
                continue;
            }
            state.last_tracer.insert((d.slot, k), now);
            let look = ribbons.look(m.weapon);
            let life = f64::from(w.range / w.speed).min(1.5);
            state.tracers.push(Tracer {
                origin,
                vel: d.vel + dir * w.speed,
                born: now,
                life,
                weapon: m.weapon,
            });
            particles.muzzle(cap, At { pos: origin, vel: d.vel }, dir, look.color, 0.7);
            state.flashes.push(Flash {
                pos: origin,
                born: now,
                life: 0.06,
                lumens: 2.0e7,
                color: color(look.color),
            });
        }
    }
    state.tracers.retain(|tr| now - tr.born < tr.life);
    state.last_tracer.retain(|_, t| now - *t < 1.0);
    let n = state.tracers.len();
    if n > TRACERS {
        state.tracers.drain(..n - TRACERS);
    }
    for (vis, mut tf, mut v, mut mat) in &mut tracers {
        match state.tracers.get(vis.0) {
            Some(tr) => {
                let look = ribbons.look(tr.weapon);
                let head = tr.origin + tr.vel * (now - tr.born) as f32;
                let travelled = head.distance(tr.origin);
                let dir = tr.vel.normalize_or(Vec3::Z);
                place_ribbon(&mut tf, head, dir, look.length.min(travelled.max(1.0)), look.half_width);
                if mat.0 != look.material {
                    mat.0 = look.material.clone();
                }
                show(&mut v, true);
            }
            None => show(&mut v, false),
        }
    }

    // --- The main thrusters' fire, seen end-on (the plume ribbons vanish from straight behind). ---
    for (d, anim) in &suits {
        let power = plume_power(d);
        if power > 0.03 {
            for &(pos, dir) in &lib.sockets(d.frame).nozzles {
                let at = At { pos: bone_point(d, anim, Bone::Backpack, pos + dir * 0.4), vel: d.vel };
                particles.glow(cap, at, 0.6 + 1.2 * power, Vec3::new(2.0, 2.8, 5.0) * power);
            }
        }
    }

    // --- Attitude jets: sideways and vertical thrust vents vapour the other way. ---
    for (d, _) in &suits {
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

    // --- Blades cutting rock: sparks and molten rock spray from where a blade goes in. ---
    if let Some(field) = &field {
        for (d, anim) in &suits {
            for (a, b, _) in drawn_blades(d, anim, &lib, &ribbons).into_iter().flatten() {
                let Some((f, i)) = field.0.sweep(a, b, 0.4) else { continue };
                let rock = field.0.rocks()[i];
                let at = a + (b - a) * f;
                let ore = crate::materials::ore_colour(usize::from(rock.ore));
                particles.cutting(cap, At { pos: at, vel: Vec3::ZERO }, rock.normal(at, 0.0), ore, time.dt);
                state.flashes.push(Flash {
                    pos: at,
                    born: now,
                    life: 0.05,
                    lumens: 5.0e7,
                    color: Color::srgb(1.0, 0.55, 0.2),
                });
            }
        }
    }

    // --- The Twin Buster Rifle drawing in energy while it charges. ---
    for (d, anim) in &suits {
        if d.flags & ent_flags::CHARGING != 0 && d.flags & ent_flags::WRECK == 0 {
            // At the drawn muzzle, which the arm is holding on the aim.
            let muzzle = bone_point(d, anim, Bone::Weapon, lib.sockets(d.frame).muzzle);
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
            FxEvent::Hit { pos, weapon, normal, .. } => {
                let look = ribbons.look(weapon);
                let big = weapon == WeaponKind::TwinBusterRifle;
                // Sparks fly off the struck surface (or back toward the camera's side of it).
                let normal = normal.unwrap_or_else(|| (eye - pos).normalize_or(Vec3::Y));
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
                blasts.chips(pos, Vec3::ZERO, 14, HullTag { heat: 20, ..HullTag::paint(paint::DARK, 0) });
                state.flashes.push(Flash {
                    pos,
                    born: now,
                    life: 1.4,
                    lumens: 1.6e9,
                    color: Color::srgb(1.0, 0.62, 0.3),
                });
            }
            FxEvent::Muzzle { pos, dir, vel, weapon, shooter } => {
                // At the drawn muzzle when it's the shooter's main weapon (the simulation's is at
                // the hand, inside the gun).
                let pos = shooter
                    .and_then(|slot| suits.iter().find(|(d, _)| d.slot == slot))
                    .filter(|(d, _)| {
                        frame(d.frame).loadout[0]
                            .is_some_and(|m| m.weapon == weapon && m.arm == ArmSlot::Right)
                    })
                    .map_or(pos, |(d, anim)| bone_point(d, anim, Bone::Weapon, lib.sockets(d.frame).muzzle));
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
            FxEvent::RockBreak { pos, radius, ore } => {
                particles.rock_burst(cap, At { pos, vel: Vec3::ZERO }, radius, ore);
                state.flashes.push(Flash {
                    pos,
                    born: now,
                    life: 0.6,
                    lumens: 4.0e8,
                    color: Color::srgb(1.0, 0.8, 0.55),
                });
            }
            FxEvent::Transform { pos, vel, rot } => {
                // A flash of feathers and a ring round the waist as the frame folds or unfolds.
                let at = At { pos, vel };
                particles.muzzle(cap, at, rot * Vec3::Y, Vec3::new(6.0, 7.0, 9.0), 5.0);
                blasts.ring(pos, vel, rot * Vec3::Z, 16.0);
                state.flashes.push(Flash {
                    pos,
                    born: now,
                    life: 0.3,
                    lumens: 1.5e8,
                    color: Color::srgb(0.85, 0.9, 1.0),
                });
            }
            FxEvent::MissileBurst { pos, kind, struck } => {
                let at = At { pos, vel: Vec3::ZERO };
                // A micro-missile's warhead is half a homing missile's.
                let scale = if kind == WeaponKind::MicroMissile { 0.12 } else { 0.2 };
                particles.explosion(cap, at, if struck { scale * 1.4 } else { scale });
                blasts.shockwave(pos, Vec3::ZERO, if struck { 16.0 } else { 10.0 });
                state.flashes.push(Flash {
                    pos,
                    born: now,
                    life: 0.35,
                    lumens: if struck { 4.0e8 } else { 2.0e8 },
                    color: Color::srgb(1.0, 0.7, 0.35),
                });
            }
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
    lib: Res<SuitMeshLib>,
    state: Res<FxState>,
    feed: Res<BeamFeed>,
    missiles: Res<MissileFeed>,
    suits: Query<(&SuitDrive, Option<&Anim>)>,
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
        for (d, anim) in &suits {
            // The middle of each glowing blade.
            for (a, b, c) in drawn_blades(d, anim, &lib, &ribbons).into_iter().flatten() {
                wishes.push(LightWish { pos: (a + b) * 0.5, color: color(c), lumens: 3.0e7 });
            }
            for (_, m) in firing_mounts(d) {
                if weapon(m.weapon).cone.is_some() {
                    // The middle of the flame.
                    let pos =
                        flame_nozzle(d, anim, &lib, m.arm.muzzle()) + d.aim * weapon(m.weapon).range * 0.4;
                    wishes.push(LightWish { pos, color: Color::srgb(1.0, 0.55, 0.2), lumens: 2.5e8 });
                }
            }
        }
        for m in &missiles.0 {
            wishes.push(LightWish { pos: m.pos, color: Color::srgb(1.0, 0.7, 0.4), lumens: 1.5e7 });
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
