//! Beams, machine-cannon tracers, hit sparks and explosions (pooled entities, no per-frame spawns),
//! drawn from the view model: the [`BeamFeed`], [`FxEvent`]s and the suits' [`SuitDrive`]s.

use std::collections::HashMap;

use bc_proto::WeaponKind;
use bc_proto::snapshot::ent_flags;
use bc_sim::content::frame;
use bevy::prelude::*;

use crate::assets::{MeshLib, Palette};
use crate::view::{BeamFeed, FxEvent, FxEvents, SuitDrive, VisTime};

const BEAMS: usize = 96;
const TRACERS: usize = 96;
const FLASHES: usize = 48;

#[derive(Component)]
pub struct BeamVis(usize);
#[derive(Component)]
pub struct TracerVis(usize);
#[derive(Component)]
pub struct FlashVis(usize);

struct Tracer {
    origin: Vec3,
    vel: Vec3,
    born: f64,
}

struct Flash {
    pos: Vec3,
    born: f64,
    size: f32,
    life: f64,
    blast: bool,
}

#[derive(Resource, Default)]
pub struct FxState {
    tracers: Vec<Tracer>,
    flashes: Vec<Flash>,
    last_tracer: HashMap<u16, f64>,
}

pub fn setup_fx(mut commands: Commands, lib: Res<MeshLib>, pal: Res<Palette>) {
    for i in 0..BEAMS {
        commands.spawn((
            BeamVis(i),
            Mesh3d(lib.capsule.clone()),
            MeshMaterial3d(pal.beam_rifle.clone()),
            Transform::default(),
            Visibility::Hidden,
        ));
    }
    for i in 0..TRACERS {
        commands.spawn((
            TracerVis(i),
            Mesh3d(lib.capsule.clone()),
            MeshMaterial3d(pal.tracer.clone()),
            Transform::default(),
            Visibility::Hidden,
        ));
    }
    for i in 0..FLASHES {
        commands.spawn((
            FlashVis(i),
            Mesh3d(lib.sphere.clone()),
            MeshMaterial3d(pal.spark.clone()),
            Transform::default(),
            Visibility::Hidden,
        ));
    }
}

fn beam_style(pal: &Palette, w: WeaponKind) -> (Handle<StandardMaterial>, f32, f32) {
    match w {
        WeaponKind::TwinBusterRifle => (pal.buster.clone(), 4.5, 420.0),
        WeaponKind::BeamCannon => (pal.beam_cannon.clone(), 1.1, 120.0),
        _ => (pal.beam_rifle.clone(), 0.55, 90.0),
    }
}

fn place_streak(tf: &mut Transform, head: Vec3, dir: Vec3, len: f32, radius: f32) {
    tf.translation = head - dir * (len * 0.5);
    tf.rotation = Quat::from_rotation_arc(Vec3::Y, dir);
    tf.scale = Vec3::new(radius * 2.0, len * 0.5, radius * 2.0);
}

/// Shows or hides a pooled entity, writing only on change (no change-detection churn).
fn show(v: &mut Visibility, on: bool) {
    let want = if on { Visibility::Visible } else { Visibility::Hidden };
    if *v != want {
        *v = want;
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_fx(
    time: Res<VisTime>,
    pal: Res<Palette>,
    feed: Res<BeamFeed>,
    mut events: ResMut<FxEvents>,
    suits: Query<&SuitDrive>,
    mut state: ResMut<FxState>,
    mut beams: Query<
        (&BeamVis, &mut Transform, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>),
        (Without<TracerVis>, Without<FlashVis>),
    >,
    mut tracers: Query<(&TracerVis, &mut Transform, &mut Visibility), (Without<BeamVis>, Without<FlashVis>)>,
    mut flashes: Query<
        (&FlashVis, &mut Transform, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>),
        (Without<BeamVis>, Without<TracerVis>),
    >,
) {
    let now = time.now;

    // --- Beams. ---
    for (vis, mut tf, mut v, mut mat) in &mut beams {
        match feed.0.get(vis.0) {
            Some(b) => {
                let (m, r, len) = beam_style(&pal, b.weapon);
                place_streak(&mut tf, b.head, b.dir, len.min(b.travelled.max(1.0)), r);
                if mat.0 != m {
                    mat.0 = m;
                }
                show(&mut v, true);
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
            state.tracers.push(Tracer { origin, vel: d.vel + d.aim * 1_200.0, born: now });
        }
    }
    state.tracers.retain(|tr| now - tr.born < 1.2);
    state.last_tracer.retain(|_, t| now - *t < 1.0);
    let n = state.tracers.len();
    if n > TRACERS {
        state.tracers.drain(..n - TRACERS);
    }
    for (vis, mut tf, mut v) in &mut tracers {
        match state.tracers.get(vis.0) {
            Some(tr) => {
                let head = tr.origin + tr.vel * (now - tr.born) as f32;
                place_streak(&mut tf, head, tr.vel.normalize_or(Vec3::Z), 18.0, 0.25);
                show(&mut v, true);
            }
            None => show(&mut v, false),
        }
    }

    // --- Sparks on hits, blasts on kills. ---
    for ev in events.0.drain(..) {
        let flash = match ev {
            FxEvent::Hit { pos, weapon } => {
                let size = if weapon == WeaponKind::TwinBusterRifle { 26.0 } else { 6.0 };
                Flash { pos, born: now, size, life: 0.35, blast: false }
            }
            FxEvent::Kill { pos } => Flash { pos, born: now, size: 55.0, life: 1.4, blast: true },
        };
        state.flashes.push(flash);
    }
    state.flashes.retain(|f| now - f.born < f.life);
    let n = state.flashes.len();
    if n > FLASHES {
        state.flashes.drain(..n - FLASHES);
    }
    for (vis, mut tf, mut v, mut mat) in &mut flashes {
        match state.flashes.get(vis.0) {
            Some(f) => {
                let age = ((now - f.born) / f.life) as f32;
                tf.translation = f.pos;
                tf.scale = Vec3::splat(f.size * (0.3 + age) * (1.0 - age * 0.5));
                let m = if f.blast { &pal.blast } else { &pal.spark };
                if mat.0 != *m {
                    mat.0 = m.clone();
                }
                show(&mut v, true);
            }
            None => show(&mut v, false),
        }
    }
}
