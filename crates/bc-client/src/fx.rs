//! Beams, machine-cannon tracers, hit sparks and explosions (pooled entities, no per-frame spawns).

use std::collections::HashSet;

use bc_client_core::FeedLine;
use bc_proto::WeaponKind;
use bc_proto::snapshot::{ent_flags, own_flags};
use bc_sim::content::frame;
use bevy::prelude::*;

use crate::assets::{MeshLib, Palette};
use crate::net::{GameClient, now_s};

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
    seen_hits: HashSet<(u32, u16, u8)>,
    seen_kills: HashSet<(u32, u16)>,
    last_tracer: std::collections::HashMap<u16, f64>,
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

#[allow(clippy::type_complexity)]
pub fn update_fx(
    game: NonSend<GameClient>,
    pal: Res<Palette>,
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
    let game = game.borrow();
    let core = &game.core;
    let world = &core.world;
    let now = now_s();
    let t_render = core.render_tick(now);
    let t_input = core.clock.server_now(now) + core.clock.lead;
    let own_slot = world.own_slot();

    // --- Beams. ---
    let live: Vec<_> = world
        .beams
        .iter()
        .filter_map(|b| {
            let t = if Some(b.shooter) == own_slot { t_input } else { t_render };
            b.alive_at(t).then_some((b, t))
        })
        .take(BEAMS)
        .collect();
    for (vis, mut tf, mut v, mut mat) in &mut beams {
        match live.get(vis.0) {
            Some((b, t)) => {
                let (m, r, len) = beam_style(&pal, b.weapon);
                let head = b.pos_at(*t);
                let dir = b.velocity.normalize_or(Vec3::Z);
                let travelled = head.distance(b.origin);
                place_streak(&mut tf, head, dir, len.min(travelled.max(1.0)), r);
                mat.0 = m;
                *v = Visibility::Visible;
            }
            None => *v = Visibility::Hidden,
        }
    }

    // --- Machine-cannon tracers from anyone firing their secondary. ---
    let mut shooters: Vec<(u16, Vec3, Vec3, Vec3)> = Vec::new();
    if let Some(own) = world.own
        && own.alive
        && core.last_cmd.buttons & bc_proto::buttons::FIRE_SECONDARY != 0
        && own.weapon_ready & 2 != 0
    {
        let spec = frame(own.frame);
        if let Some(m) = spec.loadout[1] {
            let s = &core.predict.state;
            shooters.push((own.slot, s.pos + s.rot * m.arm.muzzle(), s.vel, core.last_cmd.aim));
        }
    }
    let _ = own_flags::BOOSTING;
    for (slot, track) in world.entities.iter().enumerate() {
        let Some(track) = track else { continue };
        if track.latest.flags & ent_flags::FIRING_SECONDARY == 0 {
            continue;
        }
        let p = track.sample(t_render);
        if let Some(m) = frame(track.latest.frame).loadout[1] {
            shooters.push((slot as u16, p.pos + p.rot * m.arm.muzzle(), p.vel, p.aim));
        }
    }
    for (slot, muzzle, vel, aim) in shooters {
        let last = state.last_tracer.get(&slot).copied().unwrap_or(0.0);
        if now - last > 0.1 {
            state.last_tracer.insert(slot, now);
            state.tracers.push(Tracer { origin: muzzle, vel: vel + aim * 1_200.0, born: now });
        }
    }
    state.tracers.retain(|tr| now - tr.born < 1.2);
    let n = state.tracers.len();
    if n > TRACERS {
        state.tracers.drain(..n - TRACERS);
    }
    for (vis, mut tf, mut v) in &mut tracers {
        match state.tracers.get(vis.0) {
            Some(tr) => {
                let head = tr.origin + tr.vel * (now - tr.born) as f32;
                place_streak(&mut tf, head, tr.vel.normalize_or(Vec3::Z), 18.0, 0.25);
                *v = Visibility::Visible;
            }
            None => *v = Visibility::Hidden,
        }
    }

    // --- Sparks on hits, blasts on kills. ---
    for h in &world.hits {
        if state.seen_hits.insert((h.tick, h.target, h.part as u8)) {
            let size = if h.weapon == WeaponKind::TwinBusterRifle { 26.0 } else { 6.0 };
            state.flashes.push(Flash { pos: h.pos, born: now, size, life: 0.35, blast: false });
        }
    }
    for line in &world.feed {
        if let FeedLine::Kill { tick, victim, .. } = *line
            && state.seen_kills.insert((tick, victim))
        {
            let pos = world
                .pose(victim, t_render)
                .map(|p| p.pos)
                .or(world.own.filter(|o| o.slot == victim).map(|o| o.pos));
            if let Some(pos) = pos {
                state.flashes.push(Flash { pos, born: now, size: 55.0, life: 1.4, blast: true });
            }
        }
    }
    if state.seen_hits.len() > 4_000 {
        state.seen_hits.clear();
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
                mat.0 = if f.blast { pal.blast.clone() } else { pal.spark.clone() };
                *v = Visibility::Visible;
            }
            None => *v = Visibility::Hidden,
        }
    }
}
