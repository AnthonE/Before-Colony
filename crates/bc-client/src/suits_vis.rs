//! Mobile suits: each a tree of bone entities carrying its frame's meshes (`bc_model`, baked in
//! `model`), posed from its [`SuitDrive`]: the network's interpolated or predicted state, or a
//! showcase script.

use bc_model::rig::{self, BONES, Bone};
use bc_model::{Lod, paint};
use bc_proto::snapshot::ent_flags;
use bc_proto::{Faction, FrameId, PilotKind};
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use crate::assets::{MeshLib, Palette};
use crate::beams::{Ribbons, beam_tag, plume_tag};
use crate::camera::MainCamera;
use crate::materials::{HullMaterial, HullTag, Surfaces};
use crate::model::SuitMeshLib;
use crate::view::{SuitDrive, VisTime};

/// Suits switch to their far models beyond this distance (m), and back within `LOD_NEAR`.
const LOD_FAR: f32 = 550.0;
const LOD_NEAR: f32 = 450.0;

/// A suit root's built visual: which occupant it was built for, its bones and livery, and its
/// toggled children.
#[derive(Component)]
pub struct SuitVisual {
    generation: u8,
    frame: FrameId,
    /// Bone entities, in [`Bone`] order.
    pub bones: [Entity; BONES],
    /// The livery, as every bone's hull tag (the wreck state is applied on top).
    tag: HullTag,
    wreck: bool,
    lod: Lod,
    plumes: Vec<Entity>,
    saber: Entity,
    aura: Entity,
}

/// One of a suit's bones (which is which: [`SuitVisual::bones`]).
#[derive(Component)]
pub struct SuitBone;

/// Marks the toggled children (thruster plumes, saber, ZERO aura).
#[derive(Component)]
pub struct SuitPartMarker;

/// Eye colours (the hull shader's order).
const EYE_GREEN: u8 = 0;
const EYE_PINK: u8 = 1;

/// Body, trim and accent paint, and eye colour, for a frame in a faction's livery.
fn livery(frame: FrameId, faction: Faction) -> (u8, u8, u8, u8) {
    use paint::*;
    match (frame, faction) {
        (FrameId::WingZero, _) => (WHITE, BLUE, RED, EYE_GREEN),
        (FrameId::Taurus, _) => (TAURUS_WHITE, TAURUS_BLUE, RED, EYE_PINK),
        (FrameId::Virgo, _) => (VIRGO_OLIVE, OZ_GREY, DARK, EYE_PINK),
        (FrameId::Leo, Faction::Oz) => (OZ_GREEN, OZ_GREY, DARK, EYE_PINK),
        (FrameId::Leo, Faction::Colonies) => (TAURUS_BLUE, WHITE, RED, EYE_GREEN),
        (FrameId::Leo, Faction::Alliance) => (ALLIANCE_TAN, DARK, RED, EYE_PINK),
    }
}

/// Where a point on a bone is in the world, with the suit at rest (bones unposed).
pub fn rest_point(d: &SuitDrive, bone: Bone, local: Vec3) -> Vec3 {
    d.pos + d.rot * (bone.def().joint + local)
}

/// How hard the main thrusters burn, 0..1: with forward thrust, hardest on boost.
pub fn plume_power(d: &SuitDrive) -> f32 {
    if d.flags & ent_flags::WRECK != 0 {
        return 0.0;
    }
    let boost = if d.flags & ent_flags::BOOST != 0 { 0.45 } else { 0.0 };
    (d.thrust.z.max(0.0) * 0.75 + boost).min(1.0)
}

/// Builds a suit's bones, meshes and toggled children under `root`.
#[allow(clippy::too_many_arguments)]
fn build_suit(
    commands: &mut Commands,
    root: Entity,
    lib: &SuitMeshLib,
    shapes: &MeshLib,
    pal: &Palette,
    hull: &Handle<HullMaterial>,
    ribbons: &Ribbons,
    d: &SuitDrive,
) {
    let (body, trim, accent, eye) = livery(d.frame, d.faction);
    let tag = HullTag::livery(body, trim, accent, eye, (d.slot as u8).wrapping_mul(37));
    let model = lib.model(d.frame, Lod::Near);
    let sockets = lib.sockets(d.frame);
    let mut bones = [Entity::PLACEHOLDER; BONES];
    for bone in rig::ALL {
        let parent = bone.def().parent.map_or(root, |p| bones[p.index()]);
        let mut e = commands.spawn((
            SuitBone,
            Transform::from_translation(bone.rest()),
            Visibility::default(),
            ChildOf(parent),
        ));
        if let Some(mesh) = &model.bones[bone.index()] {
            e.insert((Mesh3d(mesh.clone()), MeshMaterial3d(hull.clone()), tag.tag()));
        }
        bones[bone.index()] = e.id();
    }
    // A plume from every main nozzle, streaming out along its exhaust.
    let plumes = sockets
        .nozzles
        .iter()
        .map(|&(pos, dir)| {
            commands
                .spawn((
                    Mesh3d(ribbons.mesh.clone()),
                    MeshMaterial3d(ribbons.plume.clone()),
                    plume_tag(0.0, d.slot as u8),
                    Transform::from_translation(pos)
                        .with_rotation(Quat::from_rotation_arc(Vec3::Y, dir))
                        .with_scale(Vec3::new(1.0, 6.0, 1.0)),
                    SuitPartMarker,
                    Visibility::Hidden,
                    ChildOf(bones[Bone::Backpack.index()]),
                ))
                .id()
        })
        .collect();
    // The beam saber's blade, from the hilt in the left hand.
    let (hilt, dir) = sockets.saber;
    let blade = &ribbons.saber;
    let saber = commands
        .spawn((
            Mesh3d(ribbons.mesh.clone()),
            MeshMaterial3d(blade.material.clone()),
            beam_tag(d.slot as u8, true),
            Transform::from_translation(hilt)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, dir))
                .with_scale(Vec3::new(blade.half_width, blade.length, 1.0)),
            SuitPartMarker,
            Visibility::Hidden,
            ChildOf(bones[Bone::HandL.index()]),
        ))
        .id();
    // The ZERO aura: a shell glowing at its rim (its MeshTag sets how bright).
    let aura = commands
        .spawn((
            Mesh3d(shapes.sphere.clone()),
            MeshMaterial3d(pal.zero_aura.clone()),
            MeshTag(170),
            Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::splat(11.0)),
            SuitPartMarker,
            Visibility::Hidden,
            ChildOf(root),
        ))
        .id();
    commands.entity(root).insert((
        SuitVisual {
            generation: d.generation,
            frame: d.frame,
            bones,
            tag,
            wreck: false,
            lod: Lod::Near,
            plumes,
            saber,
            aura,
        },
        Name::new(format!("suit-{}", d.slot)),
    ));
}

/// Builds visuals for new suit roots (and rebuilds them when the slot's occupant changes).
#[allow(clippy::too_many_arguments)]
pub fn build_suits(
    mut commands: Commands,
    lib: Res<SuitMeshLib>,
    shapes: Res<MeshLib>,
    pal: Res<Palette>,
    surfaces: Res<Surfaces>,
    ribbons: Res<Ribbons>,
    roots: Query<(Entity, &SuitDrive, Option<&SuitVisual>)>,
) {
    for (e, d, vis) in &roots {
        match vis {
            Some(v) if v.generation == d.generation && v.frame == d.frame => {}
            Some(_) => {
                commands.entity(e).despawn_children();
                build_suit(&mut commands, e, &lib, &shapes, &pal, &surfaces.armour, &ribbons, d);
            }
            None => build_suit(&mut commands, e, &lib, &shapes, &pal, &surfaces.armour, &ribbons, d),
        }
    }
}

/// Poses every suit from its drive and switches its thrusters, saber and ZERO aura.
pub fn pose_suits(
    time: Res<VisTime>,
    mut suits: Query<(&SuitDrive, &mut SuitVisual, &mut Transform), Without<SuitPartMarker>>,
    mut parts: Query<(&mut Transform, &mut Visibility), With<SuitPartMarker>>,
    mut tags: Query<&mut MeshTag>,
) {
    let flicker = 0.8 + 0.2 * ((time.now * 40.0).sin() as f32);
    for (d, mut v, mut tf) in &mut suits {
        tf.translation = d.pos;
        tf.rotation = d.rot;
        let has = |f: u16| d.flags & f != 0;
        let wreck = has(ent_flags::WRECK);
        // A wreck's armour chars and smoulders.
        if wreck != v.wreck {
            v.wreck = wreck;
            let t = HullTag { wreck, ..v.tag }.tag();
            for e in v.bones {
                if let Ok(mut m) = tags.get_mut(e) {
                    *m = t.clone();
                }
            }
        }
        // The main thrusters: longer and brighter with forward thrust, brightest on boost.
        let power = plume_power(d);
        let lit = power > 0.03;
        for &p in &v.plumes {
            if let Ok((mut ptf, mut pv)) = parts.get_mut(p) {
                set_visible(&mut pv, lit);
                if lit {
                    ptf.scale = Vec3::new(0.6 + 0.5 * power, 3.0 + 12.0 * power * flicker, 1.0);
                }
            }
            if lit && let Ok(mut t) = tags.get_mut(p) {
                *t = plume_tag(power, d.slot as u8);
            }
        }
        if let Ok((_, mut sv)) = parts.get_mut(v.saber) {
            set_visible(&mut sv, has(ent_flags::SABER) && !wreck);
        }
        let aura = !d.own && has(ent_flags::ZERO | ent_flags::SEIZED) && !wreck;
        if let Ok((mut atf, mut av)) = parts.get_mut(v.aura) {
            set_visible(&mut av, aura);
            if aura {
                atf.scale = Vec3::splat(11.0 + 0.6 * flicker);
            }
        }
    }
}

/// Swaps suits between their near and far models by distance from the camera.
pub fn suit_lod(
    lib: Res<SuitMeshLib>,
    cams: Query<&GlobalTransform, With<MainCamera>>,
    mut suits: Query<(&SuitDrive, &mut SuitVisual)>,
    mut meshes: Query<&mut Mesh3d, With<SuitBone>>,
) {
    let Ok(cam) = cams.single() else { return };
    let eye = cam.translation();
    for (d, mut v) in &mut suits {
        let dist = if d.own { 0.0 } else { d.pos.distance(eye) };
        let want = match v.lod {
            Lod::Near if dist > LOD_FAR => Lod::Far,
            Lod::Far if dist < LOD_NEAR => Lod::Near,
            lod => lod,
        };
        if want == v.lod {
            continue;
        }
        v.lod = want;
        let model = lib.model(v.frame, want);
        for bone in rig::ALL {
            if let (Some(mesh), Ok(mut m)) =
                (&model.bones[bone.index()], meshes.get_mut(v.bones[bone.index()]))
            {
                m.0 = mesh.clone();
            }
        }
    }
}

fn set_visible(v: &mut Visibility, on: bool) {
    let want = if on { Visibility::Inherited } else { Visibility::Hidden };
    if *v != want {
        *v = want;
    }
}

/// Pilot kind label for HUD brackets ("MD" for machine pilots, as in the lore).
pub fn pilot_tag(p: PilotKind) -> &'static str {
    match p {
        PilotKind::Human => "",
        PilotKind::Agent | PilotKind::MobileDoll => " MD",
    }
}
