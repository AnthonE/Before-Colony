//! Game mode's producer for the view model: turns the client core's world (predicted own suit,
//! interpolated contacts, beams, hits, kills) into [`SuitDrive`]s, the [`BeamFeed`], [`FxEvent`]s
//! and the [`CameraTarget`].

use std::collections::{HashMap, HashSet};

use bc_client_core::FeedLine;
use bc_client_core::interp::Pose;
use bc_client_core::world::ObjectMotion;
use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY};
use bc_proto::events::BurstCause;
use bc_proto::snapshot::{ent_flags, own_flags, zero_mode};
use bc_sim::TICK_HZ;
use bc_sim::bodies::sweep_landmarks;
use bc_sim::content::{SPECIAL_MOUNT, SpecialKind, frame};
use bc_sim::ground::{GRIP_ACCEL, LAND_SPEED_MAX};
use bc_sim::world::COLONY_CENTER;
use bevy::prelude::*;

use crate::input::Aim;
use crate::net::{GameClient, now_s};
use crate::view::{
    BeamFeed, BeamView, CameraTarget, ChaseTarget, DrawnBodies, FxEvent, FxEvents, MissileFeed, MissileView,
    SuitDrive, SuitGround, SuitIndex, VisTime,
};

/// Remembers which one-shot events were already turned into effects.
#[derive(Default)]
pub struct Seen {
    hits: HashSet<(u32, u16, u8)>,
    kills: HashSet<(u32, u16)>,
    /// Beams that have splashed on the colony's hull or a rock, by (shooter, shot).
    splashes: HashSet<(u16, u8)>,
    /// Beams whose muzzle flash has been shown, by (shooter, shot).
    fired: HashSet<(u16, u8)>,
    clashes: HashSet<(u32, u16, u16)>,
    rock_breaks: HashSet<(u32, u16)>,
    /// Missile bursts shown, by (tick, missile).
    bursts: HashSet<(u32, u16)>,
    /// Missiles already seen in flight, by (id, generation): a new one was just launched.
    missiles: HashSet<(u16, u8)>,
    /// Each suit's form last frame, by slot (a change of form flashes).
    forms: HashMap<u16, (u8, bc_proto::FrameId)>,
    /// Smoothed thrust estimates for other suits, by slot.
    thrust: HashMap<u16, Vec3>,
}

/// A suit's thrust demand in its own frame (-1..1 per axis), estimated from its acceleration
/// between two interpolated samples a tick apart. On a body, what its thrusters do is told apart
/// from what the body does: standing on it, they're idle (the legs carry it); in the air in its
/// grip, the acceleration is taken relative to the body, less the grip's pull and its free brake
/// on a fall.
fn thrust_estimate(frame_id: bc_proto::FrameId, rot: Quat, now: &Pose, before: &Pose) -> Vec3 {
    let accel = match (now.ground, before.ground) {
        (Some(g), _) if !g.aloft => return Vec3::ZERO,
        (Some(g), Some(b)) if b.body == g.body => {
            // The grip's pull, or over the colony's city, the colony's.
            let pull = match g.body {
                bc_sim::bodies::Body::City => {
                    let r = Vec2::new(now.pos.y, now.pos.z).length();
                    bc_sim::colony::frame::gravity(bc_sim::world::COLONY_RADIUS - r)
                }
                _ => GRIP_ACCEL,
            };
            let a = (g.rel_vel - b.rel_vel) * TICK_HZ as f32 + g.up * pull;
            // Falling at the brake's limit, nothing pushes along the normal.
            if g.rel_vel.dot(g.up) <= -(LAND_SPEED_MAX - 0.25) { a - g.up * a.dot(g.up) } else { a }
        }
        _ => (now.vel - before.vel) * TICK_HZ as f32,
    };
    let spec = frame(frame_id);
    let mass = spec.dry_mass + spec.propellant_cap * 0.5;
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

/// The sector's bodies as drawn this frame: on the view clock, at the very moment the own suit is
/// drawn on its body (else the render clock's now).
pub fn track_bodies(game: NonSend<GameClient>, vis: Res<VisTime>, mut bodies: ResMut<DrawnBodies>) {
    let game = game.borrow();
    let core = &game.core;
    bodies.t = core.own_view().map_or_else(|| core.render_tick(vis.now), |v| v.t_view);
    if !std::sync::Arc::ptr_eq(&bodies.set.field, &core.world.bodies.field)
        || bodies.set.landmarks().len() != core.world.bodies.landmarks().len()
    {
        bodies.set = core.world.bodies.clone();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn sync_view(
    mut commands: Commands,
    game: NonSend<GameClient>,
    aim: Res<Aim>,
    vis: Res<VisTime>,
    bodies: Res<DrawnBodies>,
    mut index: ResMut<SuitIndex>,
    mut drives: Query<&mut SuitDrive>,
    mut beams: ResMut<BeamFeed>,
    mut missiles: ResMut<MissileFeed>,
    mut events: ResMut<FxEvents>,
    mut target: ResMut<CameraTarget>,
    mut seen: Local<Seen>,
) {
    let game = game.borrow();
    let core = &game.core;
    let world = &core.world;
    let now = vis.now;
    // Everyone else, and every body, on the view clock.
    let t_render = bodies.t;
    // The own suit as drawn this frame (between the ticks predicted), and the time it's drawn at.
    let drawn = core.own_view().copied();
    let t_own = drawn.map_or(core.clock.own_tick(now) - 1.0, |v| v.t);
    let own_slot = world.own_slot();

    // --- Suits. ---
    // Who is holding a chunk, and in which hand (true: the right), as drawn.
    let holders: HashMap<u16, bool> = world
        .objects
        .iter()
        .flatten()
        .filter_map(|o| match o.motion_at(t_render) {
            ObjectMotion::Held { holder, right, .. } => Some((holder, right)),
            ObjectMotion::Free(_) => None,
        })
        .collect();
    let mut want: Vec<SuitDrive> = Vec::with_capacity(world.entities.len().min(64) + 1);
    if let (Some(own), Some(view)) = (world.own, drawn) {
        let mut flags = 0;
        // Boost shows while the boosted main thrusters are actually burning.
        if view.boosting && view.throttle.z > 0.05 {
            flags |= ent_flags::BOOST;
        }
        for (own_bit, ent_bit) in
            [(own_flags::CHARGING, ent_flags::CHARGING), (own_flags::OVERHEAT, ent_flags::OVERHEAT)]
        {
            if own.flags & own_bit != 0 {
                flags |= ent_bit;
            }
        }
        // What's broken inside shows on the own suit as it does on everyone's.
        {
            use bc_sim::content::systems::{DAMAGED, FAILED, OK};
            let gone = bc_sim::tuning::own_gone(&own);
            let systems = bc_sim::content::Systems(own.systems);
            flags |= match systems.worst(gone) {
                FAILED => ent_flags::SMOKING,
                DAMAGED => ent_flags::SPARKING,
                _ => 0,
            };
            if own.alive && systems.level(bc_sim::content::System::Tank, gone) != OK {
                flags |= ent_flags::VENTING;
            }
        }
        let spec = frame(view.frame);
        let buttons = core.last_cmd.buttons;
        if buttons & FIRE_PRIMARY != 0 && own.weapon_ready & 1 != 0 {
            flags |= ent_flags::FIRING_PRIMARY;
        }
        if buttons & FIRE_SECONDARY != 0 && own.weapon_ready & 2 != 0 {
            flags |= ent_flags::FIRING_SECONDARY;
        }
        // The strike as predicted, so the swing starts with the lunge: the special's melee move,
        // a blade in a gun slot (the Dragon Fang), or the F weapon.
        flags |= match view.strike {
            Some(SPECIAL_MOUNT) => ent_flags::SABER | ent_flags::SPECIAL,
            Some(0 | 1) => ent_flags::SABER | ent_flags::MELEE_ALT,
            Some(_) => ent_flags::SABER,
            None => 0,
        };
        // Any other special engaged (the melee move is the strike's): the jammer, Full Open, a
        // change of form.
        if own.flags & (own_flags::SPECIAL_ACTIVE | own_flags::TRANSFORMING) != 0
            && spec.melee_mount(SPECIAL_MOUNT).is_none()
        {
            flags |= ent_flags::SPECIAL;
            // Full Open Attack fires everything.
            if matches!(spec.special, SpecialKind::FullOpen { .. }) {
                flags |= ent_flags::FIRING_PRIMARY | ent_flags::FIRING_SECONDARY;
            }
        }
        if !view.alive {
            // The own wreck, drifting where the server says.
            flags = ent_flags::WRECK;
        }
        // What the thrusters are doing (not what the stick says: flight assist brakes by itself).
        let thrust =
            if view.alive { view.throttle.clamp(Vec3::splat(-1.0), Vec3::splat(1.0)) } else { Vec3::ZERO };
        want.push(SuitDrive {
            slot: own.slot,
            // The form drawn (a change of form shows as the suit reaches it).
            frame: view.frame,
            faction: core.cfg.faction,
            generation: own.generation,
            own: true,
            pos: view.pos,
            rot: view.rot,
            vel: view.flight_vel,
            aim: if own.alive { aim.dir } else { own.rot * Vec3::Z },
            flags,
            thrust,
            // Eighths, like everyone else's: any armour left shows as at least one.
            parts: own.parts.map(|p| if p <= 0.0 { 0 } else { (p * 7.0).ceil().clamp(1.0, 7.0) as u8 }),
            // The own hand, as of the newest news (the suit is drawn in the present).
            holding: world.objects.get(usize::from(own.held)).and_then(Option::as_ref).and_then(|o| match o
                .motion
            {
                ObjectMotion::Held { holder, right, .. } if holder == own.slot => Some(right),
                _ => None,
            }),
            ground: view.ground.and_then(|g| SuitGround::of(&g, &bodies)),
            weathering: bc_client_core::weathering(world, own.slot),
        });
    }
    for (slot, track) in world.entities.iter().enumerate() {
        let Some(track) = track else { continue };
        let e = &track.latest;
        let state = track.state_at(t_render);
        let p = track.sample(t_render, &world.bodies);
        let before = track.sample(t_render - 1.0, &world.bodies);
        let raw = thrust_estimate(e.frame, p.rot, &p, &before);
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
            // Flags and armour as of the drawn moment, not the newest snapshot's.
            flags: state.flags,
            parts: state.parts,
            thrust,
            holding: holders.get(&(slot as u16)).copied(),
            ground: p.ground.and_then(|g| SuitGround::of(&g, &bodies)),
            weathering: bc_client_core::weathering(world, slot as u16),
        });
    }
    seen.thrust.retain(|slot, _| world.entities.get(*slot as usize).is_some_and(Option::is_some));

    let mut keep: HashSet<u16> = HashSet::with_capacity(want.len());
    for d in want {
        keep.insert(d.slot);
        // The same occupant in its other form: it just changed.
        let before = seen.forms.insert(d.slot, (d.generation, d.frame));
        if let Some((generation, frame_before)) = before
            && generation == d.generation
            && frame_before != d.frame
            && frame(frame_before).special.transforms_to() == Some(d.frame)
        {
            events.0.push(FxEvent::Transform { pos: d.pos, vel: d.vel, rot: d.rot });
        }
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
    seen.forms.retain(|slot, _| keep.contains(slot));
    index.0.retain(|slot, e| {
        let alive = keep.contains(slot);
        if !alive {
            commands.entity(*e).despawn();
        }
        alive
    });

    // --- Beams: the own suit's on its own clock (leaving the muzzle as the suit is drawn there),
    // others on the render clock. ---
    // The server removes beams that strike the colony, a rock or a landmark without telling
    // anyone, so they end, and splash, there (a landmark as it's drawn). Inside the colony only
    // the Blast Hall's training rounds fly: they end at its walls, its doors, or a target.
    let inside = core.inside();
    beams.0.clear();
    for b in &world.beams {
        let t = if Some(b.shooter) == own_slot { t_own } else { t_render };
        if !b.alive_at(t) {
            continue;
        }
        let head = b.pos_at(t);
        let dir = b.velocity.normalize_or(Vec3::Z);
        let travelled = head.distance(b.origin);
        let field = &core.predict.field;
        let radius = bc_sim::content::weapon(b.weapon).radius;
        if inside {
            use bc_sim::colony::hall::{Stop, shot_end, target};
            let (k, frac) = (t.max(0.0).floor(), (t - t.max(0.0).floor()) as f32);
            if let Some((f, what)) = shot_end(b.origin, head, radius, k as u32, frac) {
                let at = b.origin + (head - b.origin) * f;
                let normal = match what {
                    Stop::Target(i) => (at - target(usize::from(i), k as u32, frac)).normalize_or(-dir),
                    Stop::Wall => -dir,
                };
                if seen.splashes.insert((b.shooter, b.shot_seq)) {
                    events.0.push(FxEvent::Hit {
                        pos: at,
                        weapon: b.weapon,
                        normal: Some(normal),
                        target: None,
                    });
                }
                continue;
            }
        }
        let rock = field.sweep(b.origin, head, radius).map(|(t, i)| (t * travelled, i)).filter(|_| !inside);
        let hull = crate::colony::ray_hit(b.origin, dir).filter(|d| !inside && travelled >= *d);
        let k = bodies.t.max(0.0).floor();
        let landmark =
            sweep_landmarks(bodies.set.landmarks(), b.origin, head, radius, k as u32, (bodies.t - k) as f32)
                .map(|(s, id)| (s * travelled, id))
                .filter(|_| !inside);
        let solid =
            [rock.map(|(d, _)| d), hull, landmark.map(|(d, _)| d)].into_iter().flatten().reduce(f32::min);
        let stop = solid.map(|d| {
            let at = b.origin + dir * d;
            let normal = match (rock, landmark) {
                (Some((r, i)), _) if r == d => field.rocks()[i].normal(at, radius),
                (_, Some((l, id))) if l == d => bodies
                    .pose(bc_sim::bodies::Body::Landmark(id))
                    .zip(bodies.set.shape(bc_sim::bodies::Body::Landmark(id)))
                    .map_or(-dir, |(p, shape)| p.rot * shape.probe(p.to_local(at)).normal),
                _ => {
                    let rel = at - COLONY_CENTER;
                    Vec3::new(0.0, rel.y, rel.z).normalize_or(Vec3::Y)
                }
            };
            (at, normal)
        });
        if let Some((at, normal)) = stop {
            if seen.splashes.insert((b.shooter, b.shot_seq)) {
                events.0.push(FxEvent::Hit { pos: at, weapon: b.weapon, normal: Some(normal), target: None });
            }
            continue;
        }
        if seen.fired.insert((b.shooter, b.shot_seq)) {
            let vel = if Some(b.shooter) == own_slot {
                drawn.map_or(Vec3::ZERO, |v| v.flight_vel)
            } else {
                world.pose(b.shooter, t_render).map_or(Vec3::ZERO, |p| p.vel)
            };
            events.0.push(FxEvent::Muzzle {
                pos: b.origin,
                dir,
                vel,
                weapon: b.weapon,
                shooter: Some(b.shooter),
            });
        }
        beams.0.push(BeamView { head, dir, travelled, weapon: b.weapon });
    }
    seen.fired
        .retain(|&(shooter, shot)| world.beams.iter().any(|b| b.shooter == shooter && b.shot_seq == shot));
    seen.splashes
        .retain(|&(shooter, shot)| world.beams.iter().any(|b| b.shooter == shooter && b.shot_seq == shot));

    // --- Missiles, on the render clock, and their bursts. ---
    missiles.0.clear();
    for m in world.missiles() {
        missiles.0.push(MissileView {
            pos: m.pos_at(t_render),
            vel: m.latest.vel,
            kind: m.latest.kind,
            targets_you: m.latest.targets_you,
        });
    }
    // Launches: a missile not seen before. The pilot's own are those that start at their suit.
    let own_pos = drawn.filter(|v| v.alive).map(|v| v.pos);
    for m in world.missiles() {
        let key = (m.latest.id, m.latest.generation);
        if seen.missiles.insert(key) {
            let pos = m.pos_at(t_render);
            let own = m.latest.friendly && own_pos.is_some_and(|p| p.distance(pos) < 80.0);
            events.0.push(FxEvent::MissileLaunch { pos, own });
        }
    }
    seen.missiles.retain(|&(id, generation)| {
        world.missiles().any(|m| m.latest.id == id && m.latest.generation == generation)
    });
    for b in &world.missile_bursts {
        if seen.bursts.insert((b.tick, b.id)) {
            events.0.push(FxEvent::MissileBurst {
                pos: b.pos,
                kind: b.kind.unwrap_or(bc_proto::WeaponKind::HomingMissile),
                struck: matches!(b.cause, BurstCause::Hit | BurstCause::Proximity),
            });
        }
    }
    seen.bursts.retain(|&(tick, id)| world.missile_bursts.iter().any(|b| b.tick == tick && b.id == id));

    // --- One-shot effects. ---
    for h in &world.hits {
        if seen.hits.insert((h.tick, h.target, h.part as u8)) {
            // On the hit part's armour where the shot's line meets it, as drawn now.
            let posed = |slot: u16| {
                if Some(slot) == own_slot {
                    drawn.map(|v| (v.frame, v.pos, v.rot))
                } else {
                    world.entity(slot).map(|t| {
                        let p = t.sample(t_render, &world.bodies);
                        (t.latest.frame, p.pos, p.rot)
                    })
                }
            };
            let (pos, normal) = match posed(h.target) {
                Some((f, pos, rot)) => {
                    let (at, n) = h.impact(f, pos, rot, posed(h.shooter).map(|(_, p, _)| p));
                    (at, Some(n))
                }
                None => (h.pos, None),
            };
            events.0.push(FxEvent::Hit { pos, weapon: h.weapon, normal, target: Some((h.target, h.part)) });
            if Some(h.target) == own_slot {
                events.0.push(FxEvent::Struck { weapon: h.weapon });
            }
        }
    }
    for line in &world.feed {
        if let FeedLine::Kill { tick, victim, .. } = *line
            && seen.kills.insert((tick, victim))
        {
            let at = world
                .pose(victim, t_render)
                .map(|p| (p.pos, p.vel))
                .or(drawn.filter(|_| Some(victim) == own_slot).map(|v| (v.pos, v.vel)))
                .or(world.own.filter(|o| o.slot == victim).map(|o| (o.pos, Vec3::ZERO)));
            if let Some((pos, vel)) = at {
                events.0.push(FxEvent::Kill { pos, vel, victim: Some(victim) });
            }
        }
    }
    for &(tick, rock) in &world.rock_breaks {
        if seen.rock_breaks.insert((tick, rock))
            && let Some(r) = core.predict.field.rocks().get(usize::from(rock))
        {
            let ore = crate::materials::ore_colour(usize::from(r.ore));
            events.0.push(FxEvent::RockBreak { pos: r.pos, radius: r.radius, ore });
        }
    }
    for line in &world.feed {
        if let FeedLine::Clash { tick, a, b } = *line
            && seen.clashes.insert((tick, a, b))
        {
            let at = |slot: u16| {
                world
                    .pose(slot, t_render)
                    .map(|p| (p.pos, p.vel))
                    .or(drawn.filter(|_| Some(slot) == own_slot).map(|v| (v.pos, v.vel)))
            };
            if let (Some((pa, va)), Some((pb, vb))) = (at(a), at(b)) {
                events.0.push(FxEvent::Clash { pos: (pa + pb) * 0.5, vel: (va + vb) * 0.5 });
            }
        }
    }
    seen.rock_breaks.retain(|k| world.rock_breaks.contains(k));
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

    // --- Camera: on the suit as drawn, and the pilot's body as predicted. ---
    target.0 = world.own.zip(drawn).map(|(own, view)| ChaseTarget {
        pos: view.pos,
        vel: view.vel,
        up: view.rot * Vec3::Y,
        aim: aim.dir,
        cut: view.cut,
        ground: view.alive && view.ground.is_some(),
        boost: view.alive && view.boosting,
        g_strain: view.g_strain.clamp(0.0, 1.0),
        blackout: view.alive && view.blackout,
        zero: own.alive && own.zero_mode == zero_mode::ACTIVE,
        zero_strain: own.zero_strain.clamp(0.0, 1.0),
        seized: own.alive && own.zero_mode == zero_mode::SEIZED,
    });
}
