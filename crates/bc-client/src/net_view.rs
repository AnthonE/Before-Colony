//! Game mode's producer for the view model: turns the client core's world (predicted own suit,
//! interpolated contacts, beams, hits, kills) into [`SuitDrive`]s, the [`BeamFeed`], [`FxEvent`]s
//! and the [`CameraTarget`].

use std::collections::{HashMap, HashSet};

use bc_client_core::FeedLine;
use bc_proto::buttons::FIRE_SECONDARY;
use bc_proto::snapshot::{ent_flags, own_flags, zero_mode};
use bc_sim::TICK_HZ;
use bc_sim::content::frame;
use bevy::prelude::*;

use crate::input::Aim;
use crate::net::{GameClient, now_s};
use crate::view::{
    BeamFeed, BeamView, CameraTarget, ChaseTarget, FxEvent, FxEvents, SuitDrive, SuitIndex, VisTime,
};

/// Remembers which one-shot events were already turned into effects, and when each wreck died.
#[derive(Default)]
pub struct Seen {
    hits: HashSet<(u32, u16, u8)>,
    kills: HashSet<(u32, u16)>,
    /// Beams that have splashed on the colony's hull, by (shooter, shot).
    splashes: HashSet<(u16, u8)>,
    /// Slot → (generation, time it was first seen as a wreck), so wrecks tumble from where they died.
    wrecked: HashMap<u16, (u8, f64)>,
    /// Beams whose muzzle flash has been shown, by (shooter, shot).
    fired: HashSet<(u16, u8)>,
    clashes: HashSet<(u32, u16, u16)>,
    /// Smoothed thrust estimates for other suits, by slot.
    thrust: HashMap<u16, Vec3>,
}

/// A suit's thrust demand in its own frame (-1..1 per axis), estimated from its acceleration
/// between two interpolated samples a tick apart.
fn thrust_estimate(frame_id: bc_proto::FrameId, rot: Quat, vel_now: Vec3, vel_before: Vec3) -> Vec3 {
    let spec = frame(frame_id);
    let mass = spec.dry_mass + spec.propellant_cap * 0.5;
    let accel = (vel_now - vel_before) * TICK_HZ as f32;
    let local = rot.inverse() * accel;
    let forward = if local.z >= 0.0 { spec.main_thrust } else { spec.retro_thrust };
    Vec3::new(local.x * mass / spec.side_thrust, local.y * mass / spec.side_thrust, local.z * mass / forward)
        .clamp(Vec3::splat(-1.0), Vec3::splat(1.0))
}

/// Game mode's clock: the page's.
pub fn tick_vis_time(mut t: ResMut<VisTime>, time: Res<Time<Real>>) {
    t.now = now_s();
    t.dt = time.delta_secs();
}

#[allow(clippy::too_many_arguments)]
pub fn sync_view(
    mut commands: Commands,
    game: NonSend<GameClient>,
    aim: Res<Aim>,
    vis: Res<VisTime>,
    mut index: ResMut<SuitIndex>,
    mut drives: Query<&mut SuitDrive>,
    mut beams: ResMut<BeamFeed>,
    mut events: ResMut<FxEvents>,
    mut target: ResMut<CameraTarget>,
    mut seen: Local<Seen>,
) {
    let game = game.borrow();
    let core = &game.core;
    let world = &core.world;
    let now = vis.now;
    let t_render = core.render_tick(now);
    let t_input = core.clock.server_now(now) + core.clock.lead;
    let own_slot = world.own_slot();

    // --- Suits. ---
    let mut want: Vec<SuitDrive> = Vec::with_capacity(world.entities.len().min(64) + 1);
    if let Some(own) = world.own {
        let mut flags = 0;
        for (own_bit, ent_bit) in [
            (own_flags::BOOSTING, ent_flags::BOOST),
            (own_flags::SABER_ACTIVE, ent_flags::SABER),
            (own_flags::CHARGING, ent_flags::CHARGING),
            (own_flags::OVERHEAT, ent_flags::OVERHEAT),
        ] {
            if own.flags & own_bit != 0 {
                flags |= ent_bit;
            }
        }
        if core.last_cmd.buttons & FIRE_SECONDARY != 0 && own.weapon_ready & 2 != 0 {
            flags |= ent_flags::FIRING_SECONDARY;
        }
        let (pos, rot, vel) = if own.alive {
            let s = &core.predict.state;
            (core.predict.render_pos(), s.rot, s.vel)
        } else {
            // The own wreck, where the server says it drifts.
            flags = ent_flags::WRECK;
            (own.pos, own.rot, own.vel)
        };
        let t = core.last_cmd.thrust;
        let thrust = if own.alive {
            Vec3::new(f32::from(t[0]), f32::from(t[1]), f32::from(t[2])) / 127.0
        } else {
            Vec3::ZERO
        };
        want.push(SuitDrive {
            slot: own.slot,
            frame: own.frame,
            faction: core.cfg.faction,
            generation: own.generation,
            own: true,
            pos,
            rot,
            vel,
            aim: if own.alive { aim.dir } else { own.rot * Vec3::Z },
            flags,
            thrust,
        });
    }
    for (slot, track) in world.entities.iter().enumerate() {
        let Some(track) = track else { continue };
        let e = &track.latest;
        let p = track.sample(t_render);
        let before = track.sample(t_render - 1.0);
        let raw = thrust_estimate(e.frame, p.rot, p.vel, before.vel);
        let smooth = seen.thrust.entry(slot as u16).or_insert(raw);
        *smooth += (raw - *smooth) * (1.0 - (-vis.dt * 8.0).exp());
        let thrust = if e.flags & ent_flags::WRECK != 0 { Vec3::ZERO } else { *smooth };
        want.push(SuitDrive {
            slot: slot as u16,
            frame: e.frame,
            faction: e.faction,
            generation: e.generation,
            own: false,
            pos: p.pos,
            rot: p.rot,
            vel: p.vel,
            aim: p.aim,
            flags: e.flags,
            thrust,
        });
    }
    seen.thrust.retain(|slot, _| world.entities.get(*slot as usize).is_some_and(Option::is_some));
    // Wrecks tumble from the moment they died, not from a global phase.
    seen.wrecked.retain(|slot, _| want.iter().any(|d| d.slot == *slot && d.flags & ent_flags::WRECK != 0));
    for d in &mut want {
        if d.flags & ent_flags::WRECK == 0 {
            continue;
        }
        let entry = seen.wrecked.entry(d.slot).or_insert((d.generation, now));
        if entry.0 != d.generation {
            *entry = (d.generation, now);
        }
        let since = (now - entry.1) as f32;
        d.rot *= Quat::from_rotation_x(since * 0.7) * Quat::from_rotation_z(since * 0.23);
    }

    let mut keep: HashSet<u16> = HashSet::with_capacity(want.len());
    for d in want {
        keep.insert(d.slot);
        let existing = index.0.get(&d.slot).copied();
        match existing.and_then(|e| drives.get_mut(e).ok().map(|x| (e, x))) {
            // Same occupant: update in place.
            Some((_, mut cur)) if cur.generation == d.generation && cur.frame == d.frame => *cur = d,
            other => {
                if let Some((e, _)) = other {
                    commands.entity(e).despawn();
                }
                let tf = Transform::from_translation(d.pos).with_rotation(d.rot);
                let e = commands.spawn((d.clone(), tf, Visibility::default())).id();
                index.0.insert(d.slot, e);
            }
        }
    }
    index.0.retain(|slot, e| {
        let alive = keep.contains(slot);
        if !alive {
            commands.entity(*e).despawn();
        }
        alive
    });

    // --- Beams: the own suit's on the input clock (drawn the moment they're fired), others on the
    // render clock. ---
    // The server removes beams that strike the colony without telling anyone, so they end, and
    // splash, at its hull here.
    beams.0.clear();
    for b in &world.beams {
        let t = if Some(b.shooter) == own_slot { t_input } else { t_render };
        if !b.alive_at(t) {
            continue;
        }
        let head = b.pos_at(t);
        let dir = b.velocity.normalize_or(Vec3::Z);
        let travelled = head.distance(b.origin);
        if let Some(hull) = crate::colony::ray_hit(b.origin, dir)
            && travelled >= hull
        {
            if seen.splashes.insert((b.shooter, b.shot_seq)) {
                events.0.push(FxEvent::Hit { pos: b.origin + dir * hull, weapon: b.weapon });
            }
            continue;
        }
        if seen.fired.insert((b.shooter, b.shot_seq)) {
            let vel = if Some(b.shooter) == own_slot {
                core.predict.state.vel
            } else {
                world.pose(b.shooter, t_render).map_or(Vec3::ZERO, |p| p.vel)
            };
            events.0.push(FxEvent::Muzzle { pos: b.origin, dir, vel, weapon: b.weapon });
        }
        beams.0.push(BeamView { head, dir, travelled, weapon: b.weapon });
    }
    seen.fired
        .retain(|&(shooter, shot)| world.beams.iter().any(|b| b.shooter == shooter && b.shot_seq == shot));
    seen.splashes
        .retain(|&(shooter, shot)| world.beams.iter().any(|b| b.shooter == shooter && b.shot_seq == shot));

    // --- One-shot effects. ---
    for h in &world.hits {
        if seen.hits.insert((h.tick, h.target, h.part as u8)) {
            events.0.push(FxEvent::Hit { pos: h.pos, weapon: h.weapon });
        }
    }
    for line in &world.feed {
        if let FeedLine::Kill { tick, victim, .. } = *line
            && seen.kills.insert((tick, victim))
        {
            let pos = world
                .pose(victim, t_render)
                .map(|p| p.pos)
                .or(world.own.filter(|o| o.slot == victim).map(|o| o.pos));
            if let Some(pos) = pos {
                events.0.push(FxEvent::Kill { pos });
            }
        }
    }
    for line in &world.feed {
        if let FeedLine::Clash { tick, a, b } = *line
            && seen.clashes.insert((tick, a, b))
        {
            let at = |slot: u16| {
                world
                    .pose(slot, t_render)
                    .map(|p| p.pos)
                    .or(world.own.filter(|o| o.slot == slot).map(|o| o.pos))
            };
            if let (Some(pa), Some(pb)) = (at(a), at(b)) {
                events.0.push(FxEvent::Clash { pos: (pa + pb) * 0.5 });
            }
        }
    }
    seen.clashes.retain(|&(tick, a, b)| {
        world
            .feed
            .iter()
            .any(|l| matches!(*l, FeedLine::Clash { tick: t, a: x, b: y } if t == tick && x == a && y == b))
    });
    // Forget keys the world no longer holds (it keeps 2 s of hits and 8 feed lines), so the sets
    // stay small without ever replaying an effect.
    seen.hits.retain(|&(tick, target, part)| {
        world.hits.iter().any(|h| h.tick == tick && h.target == target && h.part as u8 == part)
    });
    seen.kills.retain(|&(tick, victim)| {
        world
            .feed
            .iter()
            .any(|l| matches!(*l, FeedLine::Kill { tick: t, victim: v, .. } if t == tick && v == victim))
    });

    // --- Camera. ---
    target.0 = world.own.map(|own| ChaseTarget {
        pos: if own.alive { core.predict.render_pos() } else { own.pos },
        up: core.predict.state.rot * Vec3::Y,
        aim: aim.dir,
        g_strain: own.g_strain.clamp(0.0, 1.0),
        seized: own.zero_mode == zero_mode::SEIZED,
    });
}
