//! Mobile suits, built from primitives and posed from their [`SuitDrive`]: the network's
//! interpolated or predicted state, or a showcase script.

use bc_proto::snapshot::ent_flags;
use bc_proto::{Faction, FrameId, PilotKind};
use bevy::prelude::*;

use crate::assets::{MeshLib, Palette};
use crate::view::{SuitDrive, VisTime};

/// A suit root's built visual: which occupant it was built for, and its toggled children.
#[derive(Component)]
pub struct SuitVisual {
    generation: u8,
    frame: FrameId,
    thruster: Entity,
    saber: Entity,
    charge: Entity,
    aura: Entity,
}

/// Marks the toggled children (thruster, saber, charge glow, ZERO aura).
#[derive(Component)]
pub struct SuitPartMarker;

fn part(
    commands: &mut ChildSpawnerCommands,
    mesh: &Handle<Mesh>,
    mat: &Handle<StandardMaterial>,
    t: Transform,
) -> Entity {
    commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), t)).id()
}

/// A child shown only in some states; it starts hidden.
fn toggled(
    commands: &mut ChildSpawnerCommands,
    mesh: &Handle<Mesh>,
    mat: &Handle<StandardMaterial>,
    t: Transform,
) -> Entity {
    commands
        .spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), t, SuitPartMarker, Visibility::Hidden))
        .id()
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

/// Builds a ~17 m humanoid under `root` (+Z forward, +Y up, origin at the torso centre, like the
/// sim hitboxes).
fn build_suit(commands: &mut Commands, root: Entity, lib: &MeshLib, pal: &Palette, d: &SuitDrive) {
    let frame = d.frame;
    let (body, trim, eye) = livery(pal, frame, d.faction);
    let mut ids = (Entity::PLACEHOLDER, Entity::PLACEHOLDER, Entity::PLACEHOLDER, Entity::PLACEHOLDER);
    commands.entity(root).with_children(|c| {
        let bulky = if frame == FrameId::Virgo { 1.2 } else { 1.0 };
        // Torso, chest, waist.
        part(c, &lib.cube, &body, at(0.0, 2.8, 0.0).with_scale(Vec3::new(4.6 * bulky, 4.2, 3.0 * bulky)));
        part(c, &lib.cube, &trim, at(0.0, 3.4, 1.4).with_scale(Vec3::new(3.0, 1.8, 0.6)));
        part(c, &lib.cube, &trim, at(0.0, 0.2, 0.0).with_scale(Vec3::new(3.2, 1.2, 2.2)));
        // Head and sensor.
        part(c, &lib.sphere, &body, at(0.0, 6.6, 0.2).with_scale(Vec3::splat(1.25)));
        let visor = if frame == FrameId::WingZero { 1.4 } else { 0.7 };
        part(c, &lib.cube, &eye, at(0.0, 6.7, 1.3).with_scale(Vec3::new(visor, 0.35, 0.2)));
        // Shoulders, arms and legs.
        for side in [-1.0f32, 1.0] {
            part(c, &lib.cube, &trim, at(3.2 * side, 4.4, 0.0).with_scale(Vec3::new(2.0, 1.8, 2.4)));
            part(c, &lib.capsule, &body, at(3.5 * side, 1.9, 0.5).with_scale(Vec3::new(1.9, 2.3, 1.9)));
            part(c, &lib.capsule, &body, at(1.2 * side, -4.4, 0.0).with_scale(Vec3::new(2.3, 3.4, 2.3)));
            part(c, &lib.cube, &trim, at(1.2 * side, -8.4, 0.5).with_scale(Vec3::new(1.8, 1.0, 3.0)));
        }
        // Backpack and its thruster glow.
        part(c, &lib.cube, &trim, at(0.0, 4.0, -2.4).with_scale(Vec3::new(3.0, 2.8, 1.8)));
        let thruster = toggled(
            c,
            &lib.cone,
            &pal.thruster,
            at(0.0, 3.2, -4.6)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::new(1.4, 3.0, 1.4)),
        );
        // Rifle in the right hand.
        let rifle_len = match frame {
            FrameId::WingZero => 11.0,
            FrameId::Virgo => 9.0,
            _ => 7.0,
        };
        part(c, &lib.cube, &pal.dark, at(3.6, 0.5, 3.4).with_scale(Vec3::new(0.8, 1.0, rifle_len)));
        let charge = toggled(
            c,
            &lib.sphere,
            &pal.charge,
            at(3.6, 0.5, 3.4 + rifle_len * 0.5).with_scale(Vec3::splat(1.6)),
        );
        // Beam saber in the left hand.
        let saber = toggled(
            c,
            &lib.capsule,
            &pal.saber,
            at(-3.6, 5.0, 3.0).with_rotation(Quat::from_rotation_x(0.9)).with_scale(Vec3::new(1.0, 9.0, 1.0)),
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
        let aura = toggled(c, &lib.sphere, &pal.zero_aura, at(0.0, 0.0, 0.0).with_scale(Vec3::splat(11.0)));
        ids = (thruster, saber, charge, aura);
    });
    commands.entity(root).insert((
        SuitVisual {
            generation: d.generation,
            frame,
            thruster: ids.0,
            saber: ids.1,
            charge: ids.2,
            aura: ids.3,
        },
        Name::new(format!("suit-{}", d.slot)),
    ));
}

/// Builds visuals for new suit roots (and rebuilds them when the slot's occupant changes).
pub fn build_suits(
    mut commands: Commands,
    lib: Res<MeshLib>,
    pal: Res<Palette>,
    roots: Query<(Entity, &SuitDrive, Option<&SuitVisual>)>,
) {
    for (e, d, vis) in &roots {
        match vis {
            Some(v) if v.generation == d.generation && v.frame == d.frame => {}
            Some(_) => {
                commands.entity(e).despawn_children();
                build_suit(&mut commands, e, &lib, &pal, d);
            }
            None => build_suit(&mut commands, e, &lib, &pal, d),
        }
    }
}

/// Poses every suit from its drive and switches its thruster, saber, charge glow and ZERO aura.
pub fn pose_suits(
    time: Res<VisTime>,
    mut suits: Query<(&SuitDrive, &SuitVisual, &mut Transform), Without<SuitPartMarker>>,
    mut parts: Query<(&mut Transform, &mut Visibility), With<SuitPartMarker>>,
) {
    let flicker = 0.8 + 0.2 * ((time.now * 40.0).sin() as f32);
    for (d, v, mut tf) in &mut suits {
        tf.translation = d.pos;
        tf.rotation = d.rot;
        let has = |f: u16| d.flags & f != 0;
        let wreck = has(ent_flags::WRECK);
        let toggles = [
            (v.thruster, has(ent_flags::BOOST) && !wreck, Vec3::new(1.4, 3.0 + 3.0 * flicker, 1.4)),
            (v.saber, has(ent_flags::SABER) && !wreck, Vec3::new(1.0, 9.0, 1.0)),
            (v.charge, has(ent_flags::CHARGING) && !wreck, Vec3::splat(1.2 + 1.8 * flicker)),
            (
                v.aura,
                !d.own && has(ent_flags::ZERO | ent_flags::SEIZED) && !wreck,
                Vec3::splat(11.0 + 0.6 * flicker),
            ),
        ];
        for (child, on, scale) in toggles {
            if let Ok((mut ctf, mut cv)) = parts.get_mut(child) {
                let want = if on { Visibility::Inherited } else { Visibility::Hidden };
                if *cv != want {
                    *cv = want;
                }
                if on {
                    ctf.scale = scale;
                }
            }
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
