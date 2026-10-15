//! Suits: each a tree of bone entities carrying its frame's meshes (`bc_model`, baked in
//! `model`), posed from its [`SuitDrive`]: the network's interpolated or predicted state, or a
//! showcase script.

use bc_model::rig::{self, BONES, Bone};
use bc_model::{Lod, paint};
use bc_proto::snapshot::ent_flags;
use bc_proto::{Faction, FrameId, Part, PilotKind, WeaponKind};
use bc_sim::content::{SpecialKind, frame};
use bc_sim::ground::STANCE;
use bevy::light::NotShadowCaster;
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use crate::anim::{Anim, striking};
use crate::assets::{MeshLib, Palette};
use crate::beams::{BeamMaterial, Ribbons, beam_tag, plume_tag};
use crate::camera::MainCamera;
use crate::damage::Damage;
use crate::materials::{HullMaterial, HullTag, Surfaces};
use crate::model::SuitMeshLib;
use crate::view::{SuitDrive, VisTime};

/// Suits switch to their far models beyond this distance (m), and back within `LOD_NEAR`.
const LOD_FAR: f32 = 550.0;
const LOD_NEAR: f32 = 450.0;
/// A suit on a body has a contact shadow under it this wide (m), standing, or in the air with its
/// feet no higher than `SHADOW_HEIGHT` (m) off the surface.
const SHADOW_RADIUS: f32 = 4.0;
const SHADOW_HEIGHT: f32 = 30.0;

/// A suit root's built visual: which occupant it was built for, its bones and livery, and its
/// toggled children.
#[derive(Component)]
pub struct SuitVisual {
    generation: u8,
    frame: FrameId,
    /// Bone entities, in [`Bone`] order.
    pub bones: [Entity; BONES],
    /// The livery, as every bone's hull tag (damage is applied on top: `damage`).
    tag: HullTag,
    lod: Lod,
    plumes: Vec<Entity>,
    /// Blade glows: in the left hand, and on the right hand's weapon.
    blades: [Entity; 2],
    /// The Talon's cable, from the wrist to the head (on the right forearm).
    pub cable: Option<Entity>,
    /// The Sagittarius's power cable, from its generator to its cannon (on the root, stretched
    /// between them as they're posed: `anim`).
    pub power: Option<Entity>,
    /// A Corona's wards as they fly (on the root, hidden while racked), and its field's shell.
    pub wards: Vec<Entity>,
    pub field: Option<Entity>,
    /// The arcs jumping between neighbouring ward discs while they hold the field, each in
    /// `anim::ARC_KINKS` pieces.
    pub arcs: Vec<Entity>,
    aura: Entity,
    /// The Eclipse's shimmer.
    shimmer: Entity,
    /// The contact shadow under it on a body.
    shadow: Entity,
}

impl SuitVisual {
    /// The livery's hull tag.
    pub fn tag(&self) -> HullTag {
        self.tag
    }
}

/// One of a suit's bones (which is which: [`SuitVisual::bones`]).
#[derive(Component)]
pub struct SuitBone;

/// Marks the toggled children (thruster plumes, blades, the Orrery aura, jammer shimmer).
#[derive(Component)]
pub struct SuitPartMarker;

/// A blade's glow igniting and going out: how far out it reaches, 0..1, and where it was last
/// held (its hilt, direction and look), so it can draw back into the hilt after the stroke.
#[derive(Component, Default)]
pub struct BladeGlow {
    reach: f32,
    last: Option<(Vec3, Vec3, f32, f32)>,
}

/// How fast a blade ignites and goes out (fractions of its length per second).
const IGNITE: f32 = 12.0;
const RETRACT: f32 = 7.0;

/// Eye colours (the hull shader's order).
const EYE_GREEN: u8 = 0;
const EYE_PINK: u8 = 1;

/// Body, trim and accent paint, and eye colour, for a frame in a faction's livery.
pub fn livery(frame: FrameId, faction: Faction) -> (u8, u8, u8, u8) {
    use paint::*;
    match (frame, faction) {
        (FrameId::Altair | FrameId::Aquila, _) => (WHITE, BLUE, RED, EYE_GREEN),
        (FrameId::Antares, _) => (WHITE, RED, DARK, EYE_GREEN),
        (FrameId::Algol, _) => (DARK, BLUE, RED, EYE_GREEN),
        (FrameId::Canopus, _) => (ALLIANCE_TAN, WHITE, RED, EYE_GREEN),
        (FrameId::Thuban, _) => (WHITE, ALMAGEST_GREEN, YELLOW, EYE_GREEN),
        (FrameId::Corvus, _) => (CORVUS_WHITE, CORVUS_BLUE, RED, EYE_PINK),
        (FrameId::Cetus, _) => (CETUS_OLIVE, ALMAGEST_GREY, DARK, EYE_PINK),
        // The prototype pair, in the colours they were rolled out in: blue and red, grey vents.
        (FrameId::Sagittarius, _) => (BLUE, HULL, DARK, EYE_PINK),
        (FrameId::Corona, _) => (RED, HULL, DARK, EYE_PINK),
        (FrameId::Argo, Faction::Almagest) => (ALMAGEST_GREEN, ALMAGEST_GREY, DARK, EYE_PINK),
        (FrameId::Argo, Faction::Colonies) => (CORVUS_BLUE, WHITE, RED, EYE_GREEN),
        (FrameId::Argo, Faction::Alliance) => (ALLIANCE_TAN, DARK, RED, EYE_PINK),
    }
}

/// Where a point on a bone is in the world: on the posed bones when the suit has been animated,
/// else at rest.
pub fn bone_point(d: &SuitDrive, anim: Option<&Anim>, bone: Bone, local: Vec3) -> Vec3 {
    match anim {
        Some(a) => a.point(d, bone, local),
        None => d.pos + d.rot * (bone.def().joint + local),
    }
}

/// The blades a suit is striking with, as drawn: each glowing blade's hilt and tip in the world,
/// and its colour.
pub fn drawn_blades(
    d: &SuitDrive,
    anim: Option<&Anim>,
    lib: &SuitMeshLib,
    ribbons: &Ribbons,
) -> [Option<(Vec3, Vec3, Vec3)>; 2] {
    let strike =
        if d.flags & ent_flags::SABER != 0 && d.flags & ent_flags::WRECK == 0 { striking(d) } else { None };
    let sockets = lib.sockets(d.frame);
    [false, true].map(|right| {
        let (weapon, ..) = strike.filter(|s| s.2.holds(right))?;
        let (bone, (hilt, dir)) = match (right, weapon) {
            (false, WeaponKind::BeamSaber) => (Bone::HandL, sockets.saber),
            (false, _) => (Bone::HandL, sockets.blade_left?),
            (true, _) => (Bone::Weapon, sockets.blade_right?),
        };
        let look = ribbons.blade(weapon)?;
        let a = bone_point(d, anim, bone, hilt);
        let b = bone_point(d, anim, bone, hilt + dir * look.length);
        Some((a, b, look.color))
    })
}

/// Where the flamethrower's nozzle is (the dragon's mouth, as drawn), or the simulation's muzzle.
pub fn flame_nozzle(d: &SuitDrive, anim: Option<&Anim>, lib: &SuitMeshLib, sim_muzzle: Vec3) -> Vec3 {
    match lib.sockets(d.frame).flame {
        Some((p, _)) => bone_point(d, anim, Bone::HandR, p),
        None => d.pos + d.rot * sim_muzzle,
    }
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
    let tag = HullTag {
        weathering: d.weathering,
        ..HullTag::livery(body, trim, accent, eye, (d.slot as u8).wrapping_mul(37))
    };
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
    // Blade glows (placed and dressed as each strike begins: `pose_suits`), from the left hand and
    // the right hand's weapon.
    let blades = [Bone::HandL, Bone::Weapon].map(|bone| {
        commands
            .spawn((
                Mesh3d(ribbons.mesh.clone()),
                MeshMaterial3d(ribbons.saber.material.clone()),
                beam_tag(d.slot as u8, true),
                Transform::default(),
                SuitPartMarker,
                BladeGlow::default(),
                Visibility::Hidden,
                ChildOf(bones[bone.index()]),
            ))
            .id()
    });
    // The Talon's cable, drawn out as the head flies (`anim`).
    let cable = sockets.talon.map(|_| {
        commands
            .spawn((
                Mesh3d(shapes.cylinder.clone()),
                MeshMaterial3d(hull.clone()),
                HullTag::paint(bc_model::paint::DARK, d.slot as u8).tag(),
                Transform::from_translation(Bone::HandR.rest()).with_scale(Vec3::new(0.25, 0.001, 0.25)),
                ChildOf(bones[Bone::ForearmR.index()]),
            ))
            .id()
    });
    // The Sagittarius's power cable, stretched between generator and cannon as they're posed.
    let power = sockets.cable.map(|_| {
        commands
            .spawn((
                Mesh3d(shapes.cylinder.clone()),
                MeshMaterial3d(hull.clone()),
                HullTag::paint(bc_model::paint::DARK, d.slot as u8).tag(),
                Transform::from_scale(Vec3::new(0.16, 0.001, 0.16)),
                ChildOf(root),
            ))
            .id()
    });
    // A Corona's wards, flown out of their racks and home again (`anim`), and the field they
    // hold.
    let wards = sockets
        .wards
        .iter()
        .enumerate()
        .map(|(k, &(pos, face))| {
            commands
                .spawn((
                    Mesh3d(lib.ward_disc.clone()),
                    MeshMaterial3d(hull.clone()),
                    HullTag {
                        weathering: d.weathering,
                        ..HullTag::livery(body, trim, accent, eye, k as u8 * 23)
                    }
                    .tag(),
                    Transform::from_translation(pos).with_rotation(Quat::from_rotation_arc(Vec3::Z, face)),
                    Visibility::Hidden,
                    ChildOf(root),
                ))
                .id()
        })
        .collect();
    let field = matches!(frame(d.frame).special, SpecialKind::Ward { .. }).then(|| {
        commands
            .spawn((
                Mesh3d(shapes.fine_sphere.clone()),
                MeshMaterial3d(pal.field.clone()),
                MeshTag(0),
                Transform::from_scale(Vec3::splat(bc_sim::content::WARD_RADIUS)),
                NotShadowCaster,
                Visibility::Hidden,
                ChildOf(root),
            ))
            .id()
    });
    let arcs = (0..sockets.wards.len() * crate::anim::ARC_KINKS)
        .map(|k| {
            commands
                .spawn((
                    Mesh3d(ribbons.mesh.clone()),
                    MeshMaterial3d(ribbons.arc.material.clone()),
                    beam_tag((d.slot as u8).wrapping_add(k as u8 * 29), true),
                    Transform::default(),
                    NotShadowCaster,
                    Visibility::Hidden,
                    ChildOf(root),
                ))
                .id()
        })
        .collect();
    // The Orrery aura: a shell glowing at its rim (its MeshTag sets how bright).
    let aura = commands
        .spawn((
            Mesh3d(shapes.sphere.clone()),
            MeshMaterial3d(pal.orrery_aura.clone()),
            MeshTag(170),
            Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::splat(11.0)),
            SuitPartMarker,
            Visibility::Hidden,
            ChildOf(root),
        ))
        .id();
    let shimmer = commands
        .spawn((
            Mesh3d(shapes.sphere.clone()),
            MeshMaterial3d(pal.jammer.clone()),
            MeshTag(0),
            Transform::from_scale(Vec3::splat(10.0)),
            SuitPartMarker,
            Visibility::Hidden,
            ChildOf(root),
        ))
        .id();
    let shadow = commands
        .spawn((
            Mesh3d(shapes.disc.clone()),
            MeshMaterial3d(pal.contact.clone()),
            Transform::default(),
            NotShadowCaster,
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
            lod: Lod::Near,
            plumes,
            blades,
            cable,
            power,
            wards,
            field,
            arcs,
            aura,
            shimmer,
            shadow,
        },
        Anim::default(),
        Damage::new(d.slot),
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

/// Poses every suit from its drive and switches its thrusters, saber and the Orrery aura.
#[allow(clippy::too_many_arguments)]
pub fn pose_suits(
    time: Res<VisTime>,
    lib: Res<SuitMeshLib>,
    ribbons: Res<Ribbons>,
    mut suits: Query<(&SuitDrive, &SuitVisual, &mut Transform), Without<SuitPartMarker>>,
    mut parts: Query<(&mut Transform, &mut Visibility), With<SuitPartMarker>>,
    mut looks: Query<&mut MeshMaterial3d<BeamMaterial>, With<SuitPartMarker>>,
    mut glows: Query<&mut BladeGlow>,
    mut tags: Query<&mut MeshTag>,
) {
    let dt = time.dt.min(0.1);
    for (d, v, mut tf) in &mut suits {
        // Each suit's fire flickers to its own beat.
        let flicker = 0.8 + 0.2 * ((time.now * 40.0 + f64::from(d.slot) * 1.7).sin() as f32);
        tf.translation = d.pos;
        tf.rotation = d.rot;
        let has = |f: u16| d.flags & f != 0;
        let wreck = has(ent_flags::WRECK);
        let intact = |p: Part| d.parts[p as usize] > 0;
        // The main thrusters: longer and brighter with forward thrust, brightest on boost.
        let power = plume_power(d);
        let lit = power > 0.03 && intact(Part::Backpack);
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
        // The blades glow while they strike.
        let strike = if has(ent_flags::SABER) && !wreck { striking(d) } else { None };
        let sockets = lib.sockets(d.frame);
        for (k, &blade) in v.blades.iter().enumerate() {
            let right = k == 1;
            let arm = if right { Part::ArmR } else { Part::ArmL };
            let lit =
                strike.filter(|(_, _, hands)| hands.holds(right) && intact(arm)).and_then(|(weapon, ..)| {
                    let socket = match (right, weapon) {
                        (false, WeaponKind::BeamSaber) => Some(sockets.saber),
                        (false, _) => sockets.blade_left,
                        (true, _) => sockets.blade_right,
                    };
                    Some((socket?, ribbons.blade(weapon)?))
                });
            // The blade springs out of the hilt as the stroke begins and draws back in after it.
            let Ok(mut glow) = glows.get_mut(blade) else { continue };
            if let Some(((hilt, dir), look)) = lit {
                glow.last = Some((hilt, dir, look.half_width, look.length));
                glow.reach = (glow.reach + IGNITE * dt).min(1.0);
                if let Ok(mut m) = looks.get_mut(blade)
                    && m.0 != look.material
                {
                    m.0 = look.material.clone();
                }
            } else {
                glow.reach = (glow.reach - RETRACT * dt).max(0.0);
            }
            if let Ok((mut btf, mut bv)) = parts.get_mut(blade) {
                let shown = glow.reach > 0.0 && !wreck && intact(if right { Part::ArmR } else { Part::ArmL });
                set_visible(&mut bv, shown);
                if let (true, Some((hilt, dir, half_width, length))) = (shown, glow.last) {
                    // Eased: quick to reach full length, slowing as it gets there.
                    let out = 1.0 - (1.0 - glow.reach) * (1.0 - glow.reach);
                    *btf = Transform::from_translation(hilt)
                        .with_rotation(Quat::from_rotation_arc(Vec3::Y, dir))
                        .with_scale(Vec3::new(half_width, (length * out).max(0.05), 1.0));
                }
            }
        }
        let aura = !d.own && has(ent_flags::ORRERY | ent_flags::SEIZED) && !wreck;
        if let Ok((mut atf, mut av)) = parts.get_mut(v.aura) {
            set_visible(&mut av, aura);
            if aura {
                atf.scale = Vec3::splat(11.0 + 0.6 * flicker);
            }
        }
        // The contact shadow: on the surface under the suit, spreading as it rises.
        if let Ok((mut stf, mut sv)) = parts.get_mut(v.shadow) {
            let near =
                d.ground.map(|g| (g, (g.height - STANCE).max(0.0))).filter(|(_, h)| *h < SHADOW_HEIGHT);
            set_visible(&mut sv, near.is_some() && !wreck);
            if let Some((g, h)) = near {
                let inv = d.rot.inverse();
                // A hand's breadth off the ground, so it never fights it for the same pixels.
                let at = d.pos - g.up * (g.height - 0.15);
                let spread = 1.0 + h / SHADOW_HEIGHT;
                *stf = Transform::from_translation(inv * (at - d.pos))
                    .with_rotation(inv * Quat::from_rotation_arc(Vec3::Y, g.up))
                    .with_scale(Vec3::new(SHADOW_RADIUS * spread, 1.0, SHADOW_RADIUS * spread));
            }
        }
        // Jamming: a restless shimmer, brightening and fading as it crawls.
        let jamming = has(ent_flags::SPECIAL)
            && !wreck
            && matches!(frame(d.frame).special, SpecialKind::Eclipse { .. });
        if let Ok((mut stf, mut sv)) = parts.get_mut(v.shimmer) {
            set_visible(&mut sv, jamming);
            if jamming {
                let phase = time.now * 7.0 + f64::from(d.slot);
                stf.scale = Vec3::new(9.5, 10.5, 9.5) * (1.0 + 0.04 * phase.sin() as f32);
                if let Ok(mut t) = tags.get_mut(v.shimmer) {
                    *t = MeshTag((150.0 + 90.0 * (phase * 2.3).sin()) as u32);
                }
            }
        }
    }
}

/// Swaps suits between their near and far models by distance from the camera.
pub fn suit_lod(
    lib: Res<SuitMeshLib>,
    view: Res<crate::city::CityView>,
    origin: Res<crate::city::RenderOrigin>,
    cams: Query<&GlobalTransform, With<MainCamera>>,
    mut suits: Query<(&SuitDrive, &mut SuitVisual)>,
    mut meshes: Query<&mut Mesh3d, With<SuitBone>>,
) {
    let Ok(cam) = cams.single() else { return };
    // Inside the colony the camera is drawn relative to the render origin; suits are where they are.
    let eye =
        if view.active { (origin.0 + cam.translation().as_dvec3()).as_vec3() } else { cam.translation() };
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

/// Pilot kind label for HUD brackets ("DR" for machine pilots, as in the lore).
pub fn pilot_tag(p: PilotKind) -> &'static str {
    match p {
        PilotKind::Human => "",
        PilotKind::Agent | PilotKind::Drone => " DR",
    }
}
