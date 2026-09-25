//! Mobile suits, built from primitives, following the interpolated (or predicted) state.

use std::collections::HashMap;

use bc_proto::snapshot::{ent_flags, own_flags};
use bc_proto::{Faction, FrameId, PilotKind};
use bevy::prelude::*;

use crate::assets::{MeshLib, Palette};
use crate::net::{GameClient, now_s};

/// Visual for one entity slot.
#[derive(Component)]
pub struct SuitVisual {
    pub generation: u8,
    pub frame: FrameId,
    thruster: Entity,
    saber: Entity,
    charge: Entity,
    aura: Entity,
}

#[derive(Resource, Default)]
pub struct SuitIndex(pub HashMap<u16, Entity>);

/// What a visual should show this frame.
struct Pose {
    pos: Vec3,
    rot: Quat,
    frame: FrameId,
    faction: Faction,
    generation: u8,
    boost: bool,
    saber: bool,
    charging: bool,
    zero: bool,
    wreck: bool,
}

fn part(
    commands: &mut ChildSpawnerCommands,
    mesh: &Handle<Mesh>,
    mat: &Handle<StandardMaterial>,
    t: Transform,
) -> Entity {
    commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), t)).id()
}

fn at(x: f32, y: f32, z: f32) -> Transform {
    Transform::from_xyz(x, y, z)
}

fn livery(
    pal: &Palette,
    frame: FrameId,
    faction: Faction,
) -> (Handle<StandardMaterial>, Handle<StandardMaterial>, Handle<StandardMaterial>) {
    match (frame, faction) {
        (FrameId::WingZero, _) => (pal.white.clone(), pal.blue.clone(), pal.eye_green.clone()),
        (FrameId::Taurus, _) => (pal.taurus_white.clone(), pal.taurus_blue.clone(), pal.eye_pink.clone()),
        (FrameId::Virgo, _) => (pal.virgo_olive.clone(), pal.oz_grey.clone(), pal.eye_pink.clone()),
        (FrameId::Leo, Faction::Oz) => (pal.oz_green.clone(), pal.oz_grey.clone(), pal.eye_pink.clone()),
        (FrameId::Leo, Faction::Colonies) => {
            (pal.taurus_blue.clone(), pal.white.clone(), pal.eye_green.clone())
        }
        (FrameId::Leo, Faction::Alliance) => {
            (pal.alliance_tan.clone(), pal.dark.clone(), pal.eye_pink.clone())
        }
    }
}

/// Builds a ~17 m humanoid (+Z forward, +Y up, origin at the torso centre, like the sim hitboxes).
fn spawn_suit(
    commands: &mut Commands,
    lib: &MeshLib,
    pal: &Palette,
    slot: u16,
    generation: u8,
    frame: FrameId,
    faction: Faction,
) -> Entity {
    let (body, trim, eye) = livery(pal, frame, faction);
    let mut ids = (Entity::PLACEHOLDER, Entity::PLACEHOLDER, Entity::PLACEHOLDER, Entity::PLACEHOLDER);
    let root = commands
        .spawn((Transform::default(), Visibility::default()))
        .with_children(|c| {
            let bulky = if frame == FrameId::Virgo { 1.2 } else { 1.0 };
            // Torso, chest, waist.
            part(c, &lib.cube, &body, at(0.0, 2.8, 0.0).with_scale(Vec3::new(4.6 * bulky, 4.2, 3.0 * bulky)));
            part(c, &lib.cube, &trim, at(0.0, 3.4, 1.4).with_scale(Vec3::new(3.0, 1.8, 0.6)));
            part(c, &lib.cube, &trim, at(0.0, 0.2, 0.0).with_scale(Vec3::new(3.2, 1.2, 2.2)));
            // Head and sensor.
            part(c, &lib.sphere, &body, at(0.0, 6.6, 0.2).with_scale(Vec3::splat(1.25)));
            part(
                c,
                &lib.cube,
                &eye,
                at(0.0, 6.7, 1.3).with_scale(Vec3::new(
                    if frame == FrameId::WingZero { 1.4 } else { 0.7 },
                    0.35,
                    0.2,
                )),
            );
            // Shoulders and arms.
            for side in [-1.0f32, 1.0] {
                part(c, &lib.cube, &trim, at(3.2 * side, 4.4, 0.0).with_scale(Vec3::new(2.0, 1.8, 2.4)));
                part(c, &lib.capsule, &body, at(3.5 * side, 1.9, 0.5).with_scale(Vec3::new(1.9, 2.3, 1.9)));
                part(c, &lib.capsule, &body, at(1.2 * side, -4.4, 0.0).with_scale(Vec3::new(2.3, 3.4, 2.3)));
                part(c, &lib.cube, &trim, at(1.2 * side, -8.4, 0.5).with_scale(Vec3::new(1.8, 1.0, 3.0)));
            }
            // Backpack and its thruster glow.
            part(c, &lib.cube, &trim, at(0.0, 4.0, -2.4).with_scale(Vec3::new(3.0, 2.8, 1.8)));
            let thruster = part(
                c,
                &lib.cone,
                &pal.thruster,
                at(0.0, 3.2, -4.6)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                    .with_scale(Vec3::new(1.4, 3.0, 1.4)),
            );
            // Rifle in the right hand.
            let rifle_len = if frame == FrameId::WingZero {
                11.0
            } else if frame == FrameId::Virgo {
                9.0
            } else {
                7.0
            };
            part(c, &lib.cube, &pal.dark, at(3.6, 0.5, 3.4).with_scale(Vec3::new(0.8, 1.0, rifle_len)));
            let charge = part(
                c,
                &lib.sphere,
                &pal.charge,
                at(3.6, 0.5, 3.4 + rifle_len * 0.5).with_scale(Vec3::splat(1.6)),
            );
            // Beam saber in the left hand.
            let saber = part(
                c,
                &lib.capsule,
                &pal.saber,
                at(-3.6, 5.0, 3.0)
                    .with_rotation(Quat::from_rotation_x(0.9))
                    .with_scale(Vec3::new(1.0, 9.0, 1.0)),
            );
            match frame {
                FrameId::WingZero => {
                    // V-fin and the wing binders.
                    for side in [-1.0f32, 1.0] {
                        part(
                            c,
                            &lib.cube,
                            &pal.yellow,
                            at(0.55 * side, 7.7, 0.9)
                                .with_rotation(Quat::from_rotation_z(-0.55 * side))
                                .with_scale(Vec3::new(0.25, 2.2, 0.25)),
                        );
                        for (k, h) in [(0.0f32, 6.0f32), (1.0, 4.4)] {
                            part(
                                c,
                                &lib.cube,
                                &pal.white,
                                at(2.6 * side, 5.6 + k * 1.4, -3.6)
                                    .with_rotation(Quat::from_rotation_z((0.65 + 0.25 * k) * side))
                                    .with_scale(Vec3::new(1.4, h, 0.35)),
                            );
                        }
                    }
                    part(c, &lib.cube, &pal.red, at(0.0, 1.2, 1.6).with_scale(Vec3::new(1.2, 0.8, 0.4)));
                }
                FrameId::Leo | FrameId::Taurus => {
                    part(c, &lib.cube, &trim, at(-4.6, 2.4, 1.0).with_scale(Vec3::new(0.5, 5.2, 3.2)));
                }
                FrameId::Virgo => {
                    // Planet Defensor discs.
                    for k in 0..4 {
                        let a = k as f32 * std::f32::consts::FRAC_PI_2 + 0.785;
                        part(
                            c,
                            &lib.cylinder,
                            &pal.oz_grey,
                            at(5.5 * a.cos(), 3.0 + 2.5 * a.sin(), -1.5)
                                .with_rotation(Quat::from_rotation_x(1.2))
                                .with_scale(Vec3::new(2.4, 0.3, 2.4)),
                        );
                    }
                }
            }
            let aura = part(c, &lib.sphere, &pal.zero_aura, at(0.0, 0.0, 0.0).with_scale(Vec3::splat(0.001)));
            ids = (thruster, saber, charge, aura);
        })
        .id();
    commands.entity(root).insert((
        SuitVisual { generation, frame, thruster: ids.0, saber: ids.1, charge: ids.2, aura: ids.3 },
        Name::new(format!("suit-{slot}")),
    ));
    root
}

#[allow(clippy::too_many_arguments)]
pub fn sync_suits(
    mut commands: Commands,
    game: NonSend<GameClient>,
    lib: Res<MeshLib>,
    pal: Res<Palette>,
    mut index: ResMut<SuitIndex>,
    mut suits: Query<(&SuitVisual, &mut Transform), Without<SuitPartMarker>>,
    mut parts: Query<(&mut Transform, &mut Visibility), With<SuitPartMarker>>,
    time: Res<Time<Real>>,
) {
    let game = game.borrow();
    let core = &game.core;
    let world = &core.world;
    let t = core.render_tick(now_s());
    let mut want: Vec<(u16, Pose)> = Vec::new();
    if let Some(own) = world.own
        && own.alive
    {
        want.push((
            own.slot,
            Pose {
                pos: core.predict.render_pos(),
                rot: core.predict.state.rot,
                frame: own.frame,
                faction: core.cfg.faction,
                generation: own.generation,
                boost: own.flags & own_flags::BOOSTING != 0,
                saber: own.flags & own_flags::SABER_ACTIVE != 0,
                charging: own.flags & own_flags::CHARGING != 0,
                // The pilot sees ZERO through the HUD and the seizure effects, not a shell.
                zero: false,
                wreck: false,
            },
        ));
    }
    for (slot, track) in world.entities.iter().enumerate() {
        let Some(track) = track else { continue };
        let e = &track.latest;
        let p = track.sample(t);
        let wreck = e.flags & ent_flags::WRECK != 0;
        let spin = if wreck { Quat::from_rotation_x(time.elapsed_secs() * 0.7) } else { Quat::IDENTITY };
        want.push((
            slot as u16,
            Pose {
                pos: p.pos,
                rot: p.rot * spin,
                frame: e.frame,
                faction: e.faction,
                generation: e.generation,
                boost: e.flags & ent_flags::BOOST != 0,
                saber: e.flags & ent_flags::SABER != 0,
                charging: e.flags & ent_flags::CHARGING != 0,
                zero: e.flags & (ent_flags::ZERO | ent_flags::SEIZED) != 0,
                wreck,
            },
        ));
    }
    let mut keep: Vec<u16> = Vec::with_capacity(want.len());
    for (slot, pose) in want {
        keep.push(slot);
        let entity = match index.0.get(&slot).copied() {
            Some(e) => match suits.get(e) {
                Ok((v, _)) if v.generation == pose.generation && v.frame == pose.frame => e,
                _ => {
                    commands.entity(e).despawn();
                    let e = spawn_suit(
                        &mut commands,
                        &lib,
                        &pal,
                        slot,
                        pose.generation,
                        pose.frame,
                        pose.faction,
                    );
                    index.0.insert(slot, e);
                    e
                }
            },
            None => {
                let e =
                    spawn_suit(&mut commands, &lib, &pal, slot, pose.generation, pose.frame, pose.faction);
                index.0.insert(slot, e);
                e
            }
        };
        if let Ok((v, mut tf)) = suits.get_mut(entity) {
            tf.translation = pose.pos;
            tf.rotation = pose.rot;
            let flicker = 0.8 + 0.2 * (time.elapsed_secs() * 40.0).sin();
            let toggles = [
                (v.thruster, pose.boost && !pose.wreck, Vec3::new(1.4, 3.0 + 3.0 * flicker, 1.4)),
                (v.saber, pose.saber && !pose.wreck, Vec3::new(1.0, 9.0, 1.0)),
                (v.charge, pose.charging, Vec3::splat(1.2 + 1.8 * flicker)),
                (v.aura, pose.zero && !pose.wreck, Vec3::splat(11.0 + 0.6 * flicker)),
            ];
            for (child, on, scale) in toggles {
                if let Ok((mut ctf, mut cv)) = parts.get_mut(child) {
                    *cv = if on { Visibility::Inherited } else { Visibility::Hidden };
                    ctf.scale = scale;
                }
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
}

/// Marks the toggled children (thruster, saber, charge glow, ZERO aura).
#[derive(Component)]
pub struct SuitPartMarker;

/// Tags new suits' toggled children so `sync_suits` can reach them.
pub fn tag_parts(mut commands: Commands, added: Query<&SuitVisual, Added<SuitVisual>>) {
    for v in &added {
        for e in [v.thruster, v.saber, v.charge, v.aura] {
            commands.entity(e).insert((SuitPartMarker, Visibility::Hidden));
        }
    }
}

/// Pilot kind label for HUD brackets ("MD" for machine pilots, as in the lore).
pub fn pilot_tag(p: PilotKind) -> &'static str {
    match p {
        PilotKind::Human => "",
        PilotKind::Agent | PilotKind::MobileDoll => " MD",
    }
}
