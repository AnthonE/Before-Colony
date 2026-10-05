//! The hangar bay, drawn: the pilot's bay in the colony's docking hub, where they walk on foot.
//!
//! - Its solid geometry is `bc_client_core::bay`'s (what the walker collides with), plated with the
//!   hull shader: the deck, the walls, the catwalk across the suit's chest and the stairs to it,
//!   the gantry's pillars, the fabricator, the stores' racks, the consoles and crates.
//! - Dressed round that: the bay doors (two halves that slide apart for a launch) and the launch
//!   tunnel beyond them, open to space at its far end; the airlock's door; hazard stripes, deck
//!   markings, cable runs, the crane overhead; flood lights on the suit, lamps in the ceiling, and
//!   red alarm beacons that turn while the bay cycles.
//! - The pilot's suit stands in its gantry as their hangar says: its line, the parts fitted (and
//!   as worn as they are), its main weapon only if one's fitted; its eyes dark until they board.
//!
//! The bay sits far above the sector ([`BAY_ORIGIN`]), where nothing in space comes near it, and
//! while the pilot is in it ([`Indoors`]) the Sun, the dust and the flare are off and the camera's
//! exposure is set for lamps, not sunlight.

use bc_client_core::bay::{
    self, CATWALK_Y, DOOR_HALF_WIDTH, DOOR_HEIGHT, GANTRY_TOP, HALF_LENGTH, HALF_WIDTH, HEIGHT, Layout, Look,
    SUIT_AT,
};
use bc_econ::Suit;
use bc_model::paint;
use bc_model::rig::Bone;
use bc_proto::snapshot::ent_flags;
use bc_proto::{Faction, FrameId, Part};
use bc_sim::content::{ArmSlot, frame};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::camera::FillLight;
use crate::materials::{HullMaterial, HullTag, Surfaces};
use crate::shade::{ShadeMaterial, Shades};
use crate::suits_vis::SuitVisual;
use crate::view::{SuitDrive, VisTime};

/// Where the bay is in the world: far above anything in the sector.
pub const BAY_ORIGIN: Vec3 = Vec3::new(0.0, 20_000.0, 0.0);
/// The entity slot the suit in the bay is drawn under (no suit in the sector has it).
pub const BAY_SLOT: u16 = 1_022;
/// Camera exposure indoors (lamps, not the Sun).
pub const INDOOR_EV100: f32 = 8.0;
/// How long the bay doors take to open or close, s.
pub const DOOR_SECS: f32 = 3.5;
/// How far the launch tunnel runs past the bay doors, m.
const TUNNEL: f32 = 220.0;
/// The ceiling lamps and the flood lights on the suit, lumens.
const LAMP: f32 = 1.6e6;
const FLOOD: f32 = 2.6e6;
/// Light that bounces round the bay, indoors (the ambient fill): little, so the lamps and floods
/// pool their light and the corners keep their shade.
pub const INDOOR_AMBIENT: f32 = 45.0;

/// Whether the pilot is in the bay (and the view with them).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Indoors(pub bool);

/// What the bay shows, set by whoever drives it (the game's hangar state, or a showcase).
#[derive(Resource, Clone, Debug, Default)]
pub struct BayState {
    /// The suit standing in the gantry.
    pub suit: Option<Suit>,
    /// Its pilot is aboard: its eyes are lit.
    pub boarded: bool,
    /// The bay doors, and the launch tunnel's outer doors, 0 shut .. 1 open (the cycle eases them).
    pub doors: f32,
    pub outer: f32,
    /// The bay is cycling (venting or pressurising): the alarm beacons turn, the lamps go red.
    pub alarm: bool,
    /// The airlock's door, 0 shut .. 1 open.
    pub airlock: f32,
    /// Where the suit is, from its place in the gantry (a launch throws it down the tunnel; a
    /// homecoming glides it in), m, and how fast it's going.
    pub offset: Vec3,
    pub vel: Vec3,
    /// What its thrusters are doing, in its own frame (-1..1 each axis), and whether it's boosting.
    pub thrust: Vec3,
    pub boost: bool,
}

/// The bay's moving parts.
#[derive(Resource)]
pub struct BayScene {
    pub root: Entity,
    doors: [Entity; 2],
    /// The launch tunnel's outer doors.
    outer: [Entity; 2],
    airlock: Entity,
    alarms: Vec<Entity>,
    lamps: Vec<(Entity, f32)>,
    suit: Entity,
    /// What the suit was last built as, to rebuild it when that changes.
    built: Option<(FrameId, [u8; Part::COUNT], [bool; 3])>,
    generation: u8,
}

/// A red beacon that turns while the bay cycles.
#[derive(Component)]
struct Beacon {
    phase: f32,
}

/// One of the flood lights on the suit (the first casts shadows where the tier has them).
#[derive(Component)]
pub struct Flood(pub usize);

pub struct HangarPlugin;

impl Plugin for HangarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Indoors>()
            .init_resource::<BayState>()
            .add_systems(Update, (show_bay, drive_bay).chain());
    }
}

/// Paint for a block, and whether it's bare metal.
fn finish(look: Look) -> (u8, bool) {
    match look {
        Look::Floor => (paint::GUNMETAL, false),
        Look::Ceiling => (paint::HULL_DARK, false),
        Look::Wall => (paint::HULL, false),
        Look::BayDoor | Look::AirlockDoor => (paint::OZ_GREY, false),
        Look::Catwalk | Look::Stair => (paint::GUNMETAL, true),
        Look::Rail => (paint::YELLOW, false),
        Look::Pillar => (paint::YELLOW, false),
        Look::Machine => (paint::OZ_GREY, false),
        Look::Rack => (paint::HULL_DARK, true),
        Look::Console => (paint::DARK, false),
        Look::Crate => (paint::VIRGO_OLIVE, false),
        Look::Suit => (paint::DARK, false),
    }
}

fn emissive(materials: &mut Assets<StandardMaterial>, rgb: [f32; 3]) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(rgb[0], rgb[1], rgb[2]),
        ..default()
    })
}

/// Builds the bay (hidden until the pilot is in it).
pub fn setup_bay(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shade_materials: ResMut<Assets<ShadeMaterial>>,
    surfaces: Res<Surfaces>,
) {
    let shades = Shades::new(&mut meshes, &mut shade_materials);
    let layout = Layout::new();
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let plating: Handle<HullMaterial> = surfaces.plating.clone();
    let lamp = emissive(&mut materials, [14.0, 14.0, 15.0]);
    let amber = emissive(&mut materials, [12.0, 5.0, 0.6]);
    let red = emissive(&mut materials, [30.0, 1.5, 1.0]);
    let screen = emissive(&mut materials, [0.6, 3.4, 4.2]);
    let furnace = emissive(&mut materials, [16.0, 5.0, 1.2]);
    let guide = emissive(&mut materials, [3.0, 8.0, 12.0]);
    let green = emissive(&mut materials, [0.8, 9.0, 1.6]);
    let stripe_dark = HullTag::paint(paint::DARK, 3);
    let stripe_yellow = HullTag::paint(paint::YELLOW, 4);

    let root = commands
        .spawn((Name::new("hangar-bay"), Transform::from_translation(BAY_ORIGIN), Visibility::Hidden))
        .id();
    let piece = |commands: &mut Commands, at: Vec3, size: Vec3, tag: bevy::mesh::MeshTag| {
        commands
            .spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(plating.clone()),
                tag,
                Transform::from_translation(at).with_scale(size),
                ChildOf(root),
            ))
            .id()
    };

    // The solid bay (the doors are drawn apart, so they can move).
    for (k, b) in layout.blocks.iter().enumerate() {
        if matches!(b.look, Look::Suit | Look::BayDoor | Look::AirlockDoor) {
            continue;
        }
        let (paint, metal) = finish(b.look);
        let tag = HullTag { metal, ..HullTag::paint(paint, (k * 37) as u8) }.tag();
        piece(&mut commands, b.centre(), b.size(), tag);
        // What stands on the deck shades the deck round its foot.
        if b.min.y == 0.0
            && matches!(b.look, Look::Machine | Look::Rack | Look::Console | Look::Crate | Look::Pillar)
        {
            let foot = Vec2::new(b.size().x, b.size().z);
            let reach = (0.5 + foot.min_element() * 0.35).min(1.6);
            let at = Vec3::new(b.centre().x, 0.0, b.centre().z);
            shades.spawn(&mut commands, root, at, foot + Vec2::splat(reach * 2.0), reach, 0.7);
        }
    }
    // And the foot of every wall, where the deck meets it.
    for (at, size) in [
        (Vec3::new(0.0, 0.0, HALF_LENGTH), Vec2::new(HALF_WIDTH * 2.0, 3.0)),
        (Vec3::new(0.0, 0.0, -HALF_LENGTH), Vec2::new(HALF_WIDTH * 2.0, 3.0)),
        (Vec3::new(HALF_WIDTH, 0.0, 0.0), Vec2::new(3.0, HALF_LENGTH * 2.0)),
        (Vec3::new(-HALF_WIDTH, 0.0, 0.0), Vec2::new(3.0, HALF_LENGTH * 2.0)),
    ] {
        shades.spawn(&mut commands, root, at, size + Vec2::splat(1.0), 2.0, 0.55);
    }
    let glow = |commands: &mut Commands, at: Vec3, size: Vec3, m: &Handle<StandardMaterial>| {
        commands
            .spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(m.clone()),
                Transform::from_translation(at).with_scale(size),
                NotShadowCaster,
                ChildOf(root),
            ))
            .id()
    };

    // A pressure door's half (`side` −1: the left), striped where the halves meet and across its
    // foot.
    let door_half = |commands: &mut Commands, at: Vec3, size: Vec3, side: f32| {
        let half = commands
            .spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(plating.clone()),
                HullTag::paint(paint::OZ_GREY, if side < 0.0 { 11 } else { 12 }).tag(),
                Transform::from_translation(at).with_scale(size),
                ChildOf(root),
            ))
            .id();
        // Hazard stripes down the meeting edge, and a band across the foot.
        for k in 0..26 {
            let tag = if k % 2 == 0 { stripe_yellow } else { stripe_dark }.tag();
            commands.spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(plating.clone()),
                tag,
                Transform::from_xyz(-side * 0.46, -0.5 + (k as f32 + 0.5) / 26.0, 0.52)
                    .with_scale(Vec3::new(0.06, 1.0 / 26.0, 0.1)),
                ChildOf(half),
            ));
        }
        for k in 0..12 {
            let tag = if k % 2 == 0 { stripe_yellow } else { stripe_dark }.tag();
            commands.spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(plating.clone()),
                tag,
                Transform::from_xyz(-0.5 + (k as f32 + 0.5) / 12.0, -0.47, 0.52).with_scale(Vec3::new(
                    1.0 / 12.0,
                    0.05,
                    0.1,
                )),
                ChildOf(half),
            ));
        }
        half
    };
    // The bay doors: two halves meeting in the middle.
    let door_z = -HALF_LENGTH - 0.8;
    let doors = [-1.0f32, 1.0].map(|side| {
        let at = Vec3::new(side * DOOR_HALF_WIDTH / 2.0, DOOR_HEIGHT / 2.0, door_z);
        door_half(&mut commands, at, Vec3::new(DOOR_HALF_WIDTH, DOOR_HEIGHT, 0.8), side)
    });

    // The launch tunnel beyond the doors: open to space at its far end, lit along its floor.
    let (tw, th) = (DOOR_HALF_WIDTH + 4.0, DOOR_HEIGHT + 4.0);
    let tz = door_z - 0.5 - TUNNEL / 2.0;
    for (at, size) in [
        (Vec3::new(0.0, -0.5, tz), Vec3::new(tw * 2.0, 1.0, TUNNEL)),
        (Vec3::new(0.0, th + 0.5, tz), Vec3::new(tw * 2.0, 1.0, TUNNEL)),
        (Vec3::new(-tw - 0.5, th / 2.0, tz), Vec3::new(1.0, th + 2.0, TUNNEL)),
        (Vec3::new(tw + 0.5, th / 2.0, tz), Vec3::new(1.0, th + 2.0, TUNNEL)),
    ] {
        piece(&mut commands, at, size, HullTag::paint(paint::HULL_DARK, 21).tag());
    }
    // Its outer doors, at the far end: the tunnel is the bay's airlock.
    let outer_z = door_z - 0.5 - TUNNEL + 0.6;
    let outer = [-1.0f32, 1.0].map(|side| {
        let at = Vec3::new(side * tw / 2.0, th / 2.0, outer_z);
        door_half(&mut commands, at, Vec3::new(tw, th, 1.0), side)
    });
    // Pressure frames along it, striped.
    for k in 1..4 {
        let z = door_z - 55.0 * k as f32;
        for (at, size) in [
            (Vec3::new(-tw + 0.8, th / 2.0, z), Vec3::new(1.6, th, 1.6)),
            (Vec3::new(tw - 0.8, th / 2.0, z), Vec3::new(1.6, th, 1.6)),
        ] {
            piece(&mut commands, at, size, HullTag::paint(paint::HULL_DARK, 60 + k as u8).tag());
        }
        for j in 0..16 {
            let tag = if j % 2 == 0 { stripe_yellow } else { stripe_dark }.tag();
            let x = -tw + (j as f32 + 0.5) * (2.0 * tw / 16.0);
            piece(&mut commands, Vec3::new(x, th - 0.8, z), Vec3::new(2.0 * tw / 16.0, 1.6, 1.6), tag);
        }
    }
    for k in 0..22 {
        let z = door_z - 6.0 - k as f32 * 10.0;
        for side in [-1.0f32, 1.0] {
            glow(&mut commands, Vec3::new(side * (tw - 2.0), 0.08, z), Vec3::new(0.8, 0.12, 3.0), &guide);
            glow(
                &mut commands,
                Vec3::new(side * tw - side * 0.2, th * 0.6, z),
                Vec3::new(0.2, 1.2, 1.2),
                &amber,
            );
        }
    }

    // The airlock's door in the left wall, framed, with its lamp; the chamber behind it, and its
    // outer door onto the concourse.
    let airlock = Vec3::new(-HALF_WIDTH - 0.3, 1.5, -20.0);
    let airlock_door = commands
        .spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(plating.clone()),
            HullTag::paint(paint::DARK, 30).tag(),
            Transform::from_translation(airlock).with_scale(Vec3::new(0.4, 3.0, 2.6)),
            ChildOf(root),
        ))
        .id();
    for (dy, dz, h, w) in [(1.65, 0.0, 0.3, 3.2), (0.0, 1.45, 3.3, 0.3), (0.0, -1.45, 3.3, 0.3)] {
        let tag = HullTag::paint(paint::YELLOW, 31).tag();
        piece(&mut commands, airlock + Vec3::new(0.25, dy, dz), Vec3::new(0.2, h, w), tag);
    }
    glow(&mut commands, airlock + Vec3::new(0.35, 2.05, 0.0), Vec3::new(0.1, 0.18, 0.5), &green);
    // The bay's control room looks down through a window above the airlock: its consoles lit.
    let glass = emissive(&mut materials, [0.05, 0.14, 0.2]);
    glow(&mut commands, Vec3::new(-HALF_WIDTH + 0.05, 6.4, -17.0), Vec3::new(0.1, 2.4, 9.0), &glass);
    for (k, z) in [-20.2, -18.4, -15.6, -13.8].into_iter().enumerate() {
        let y = if k % 2 == 0 { 5.9 } else { 6.2 };
        glow(&mut commands, Vec3::new(-HALF_WIDTH + 0.1, y, z), Vec3::new(0.05, 0.5, 0.9), &screen);
    }
    for (at, size) in [
        (Vec3::new(-HALF_WIDTH + 0.2, 7.75, -17.0), Vec3::new(0.3, 0.3, 9.6)),
        (Vec3::new(-HALF_WIDTH + 0.2, 5.05, -17.0), Vec3::new(0.3, 0.3, 9.6)),
        (Vec3::new(-HALF_WIDTH + 0.2, 6.4, -21.65), Vec3::new(0.3, 2.9, 0.3)),
        (Vec3::new(-HALF_WIDTH + 0.2, 6.4, -12.35), Vec3::new(0.3, 2.9, 0.3)),
    ] {
        piece(&mut commands, at, size, HullTag::paint(paint::YELLOW, 33).tag());
    }
    let chamber = HullTag::paint(paint::HULL, 32).tag();
    for (at, size) in [
        (Vec3::new(-19.8, -0.25, -20.0), Vec3::new(4.0, 0.5, 3.6)),
        (Vec3::new(-19.8, 3.25, -20.0), Vec3::new(4.0, 0.5, 3.6)),
        (Vec3::new(-19.8, 1.5, -21.55), Vec3::new(4.0, 3.0, 0.5)),
        (Vec3::new(-19.8, 1.5, -18.45), Vec3::new(4.0, 3.0, 0.5)),
        (Vec3::new(-21.55, 1.5, -20.0), Vec3::new(0.5, 3.0, 2.6)),
    ] {
        piece(&mut commands, at, size, chamber.clone());
    }
    glow(&mut commands, Vec3::new(-19.8, 2.95, -20.0), Vec3::new(2.0, 0.08, 0.6), &lamp);

    // Deck markings: the suit's footprint, and walkways to the stations.
    let mark = |commands: &mut Commands, a: Vec3, b: Vec3| {
        let (min, max) = (a.min(b), a.max(b));
        let size = (max - min).max(Vec3::new(0.18, 0.02, 0.18));
        piece(commands, (min + max) / 2.0 + Vec3::Y * 0.011, size, stripe_yellow.tag())
    };
    for (a, b) in [
        (Vec3::new(-7.5, 0.0, 3.5), Vec3::new(7.5, 0.0, 3.5)),
        (Vec3::new(-7.5, 0.0, 10.0), Vec3::new(7.5, 0.0, 10.0)),
        (Vec3::new(-7.5, 0.0, 3.5), Vec3::new(-7.5, 0.0, 10.0)),
        (Vec3::new(7.5, 0.0, 3.5), Vec3::new(7.5, 0.0, 10.0)),
        (Vec3::new(-11.0, 0.0, -18.0), Vec3::new(-11.0, 0.0, 16.0)),
        (Vec3::new(10.5, 0.0, -18.0), Vec3::new(10.5, 0.0, 16.0)),
    ] {
        mark(&mut commands, a, b);
    }
    // The launch line: hazard stripes across the deck at the doors.
    for k in 0..24 {
        let tag = if k % 2 == 0 { stripe_yellow } else { stripe_dark }.tag();
        piece(
            &mut commands,
            Vec3::new(-DOOR_HALF_WIDTH + (k as f32 + 0.5) * 1.0, 0.012, -HALF_LENGTH + 0.6),
            Vec3::new(1.0, 0.02, 1.2),
            tag,
        );
    }
    // Hazard stripes along the catwalk's edge.
    for k in 0..48 {
        let tag = if k % 2 == 0 { stripe_yellow } else { stripe_dark }.tag();
        let x = -16.6 + (k as f32 + 0.5) * (25.6 / 48.0);
        piece(&mut commands, Vec3::new(x, CATWALK_Y - 0.2, 0.55), Vec3::new(25.6 / 48.0, 0.4, 0.1), tag);
    }

    // The gantry: braces between its pillars at the knee, the head and the top, the crane rail (the
    // catwalk crosses at the waist).
    for y in [7.4, 16.5, GANTRY_TOP] {
        for z in [4.5, 9.1] {
            piece(
                &mut commands,
                Vec3::new(0.0, y, z),
                Vec3::new(13.6, 0.5, 0.5),
                HullTag::paint(paint::YELLOW, 40).tag(),
            );
        }
        for x in [-6.5, 6.5] {
            piece(
                &mut commands,
                Vec3::new(x, y, 6.8),
                Vec3::new(0.5, 0.5, 4.6),
                HullTag::paint(paint::YELLOW, 41).tag(),
            );
        }
    }
    let crane = HullTag::paint(paint::HULL_DARK, 42);
    for x in [-9.0, 9.0] {
        piece(
            &mut commands,
            Vec3::new(x, HEIGHT - 2.5, 0.0),
            Vec3::new(0.8, 1.2, 2.0 * HALF_LENGTH),
            crane.tag(),
        );
    }
    piece(&mut commands, Vec3::new(0.0, HEIGHT - 3.5, 12.0), Vec3::new(19.0, 1.0, 1.4), crane.tag());
    piece(
        &mut commands,
        Vec3::new(2.0, HEIGHT - 5.0, 12.0),
        Vec3::new(2.0, 2.0, 2.0),
        HullTag::paint(paint::YELLOW, 43).tag(),
    );

    // Cable runs and ducts along the walls.
    for (x, z0, z1) in [(-HALF_WIDTH + 0.4, -17.0, 22.0), (HALF_WIDTH - 0.4, 15.0, 22.0)] {
        for (k, y) in [6.5, 7.1, 13.8].into_iter().enumerate() {
            piece(
                &mut commands,
                Vec3::new(x, y, (z0 + z1) / 2.0),
                Vec3::new(0.35, 0.35, z1 - z0),
                HullTag::paint(paint::DARK, 50 + k as u8).tag(),
            );
        }
    }
    for z in [-10.0, 0.0, 10.0, 20.0] {
        piece(
            &mut commands,
            Vec3::new(HALF_WIDTH - 0.5, HEIGHT / 2.0, z),
            Vec3::new(0.6, HEIGHT, 0.6),
            HullTag::paint(paint::HULL_DARK, 55).tag(),
        );
    }

    // The fabricator's furnace window and lamps; the consoles' screens; the racks' crates.
    glow(&mut commands, Vec3::new(13.35, 2.6, 8.0), Vec3::new(0.1, 0.8, 7.0), &furnace);
    for z in [3.0, 13.0] {
        glow(&mut commands, Vec3::new(13.35, 5.6, z), Vec3::new(0.1, 0.3, 0.3), &amber);
    }
    for spot in bay::Spot::ALL {
        if matches!(spot, bay::Spot::Cockpit | bay::Spot::Airlock) {
            continue;
        }
        let (feet, facing) = spot.stand();
        let at = spot.at() + Vec3::Y * 0.12;
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(screen.clone()),
            Transform::from_translation(at - facing * 0.1)
                .looking_to(feet.with_y(at.y) - at, Vec3::Y)
                .with_scale(Vec3::new(0.7, 0.45, 0.04)),
            NotShadowCaster,
            ChildOf(root),
        ));
    }
    let mut shade = 0u8;
    for level in 0..3 {
        for k in 0..8 {
            shade = shade.wrapping_add(29);
            let paint = [paint::VIRGO_OLIVE, paint::ALLIANCE_TAN, paint::OZ_GREY, paint::TAURUS_BLUE]
                [(shade % 4) as usize];
            let z = -17.2 + k as f32 * 1.45;
            let y = 0.55 + level as f32 * 1.35;
            piece(
                &mut commands,
                Vec3::new(16.3, y, z),
                Vec3::new(1.1, 0.9, 1.1),
                HullTag::paint(paint, shade).tag(),
            );
        }
    }

    // Light: lamps down the ceiling, flood lights on the suit, alarm beacons.
    let mut lamps = Vec::new();
    for z in [-16.0, -4.0, 8.0, 20.0] {
        for x in [-10.0, 10.0] {
            glow(&mut commands, Vec3::new(x, HEIGHT - 0.3, z), Vec3::new(4.0, 0.2, 1.0), &lamp);
        }
        let e = commands
            .spawn((
                PointLight { intensity: LAMP, range: 50.0, shadow_maps_enabled: false, ..default() },
                Transform::from_xyz(0.0, HEIGHT - 3.0, z),
                ChildOf(root),
            ))
            .id();
        lamps.push((e, LAMP));
    }
    for (k, (x, z)) in [(-12.0, -8.0), (12.0, -8.0)].into_iter().enumerate() {
        let e = commands
            .spawn((
                Flood(k),
                SpotLight {
                    intensity: FLOOD,
                    range: 60.0,
                    outer_angle: 0.42,
                    inner_angle: 0.25,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(x, 22.0, z).looking_at(Vec3::new(0.0, 12.0, 6.0), Vec3::Y),
                ChildOf(root),
            ))
            .id();
        lamps.push((e, FLOOD));
        glow(&mut commands, Vec3::new(x, 22.0, z), Vec3::new(1.2, 0.8, 1.2), &lamp);
    }
    let mut alarms = Vec::new();
    for (k, at) in [
        Vec3::new(-DOOR_HALF_WIDTH - 1.5, DOOR_HEIGHT - 1.0, -HALF_LENGTH + 0.6),
        Vec3::new(DOOR_HALF_WIDTH + 1.5, DOOR_HEIGHT - 1.0, -HALF_LENGTH + 0.6),
        Vec3::new(0.0, HEIGHT - 1.0, 18.0),
    ]
    .into_iter()
    .enumerate()
    {
        glow(&mut commands, at, Vec3::splat(0.7), &red);
        let e = commands
            .spawn((
                PointLight {
                    intensity: 0.0,
                    color: Color::srgb(1.0, 0.1, 0.05),
                    range: 30.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_translation(at + Vec3::Y * -0.8),
                Beacon { phase: k as f32 * 2.1 },
                ChildOf(root),
            ))
            .id();
        alarms.push(e);
    }

    // The suit, in its gantry.
    let suit = commands
        .spawn((Name::new("bay-suit"), Transform::from_translation(BAY_ORIGIN + SUIT_AT), Visibility::Hidden))
        .id();
    // The suit's feet shade the deck where it stands (and go with it when it launches).
    for b in layout.blocks.iter().filter(|b| b.look == Look::Suit && b.min.y == 0.0) {
        let at = Vec3::new(b.centre().x, 0.0, b.centre().z) - SUIT_AT;
        shades.spawn(
            &mut commands,
            suit,
            at,
            Vec2::new(b.size().x, b.size().z) + Vec2::splat(1.6),
            0.8,
            0.75,
        );
    }
    commands.insert_resource(BayScene {
        root,
        doors,
        outer,
        airlock: airlock_door,
        alarms,
        lamps,
        suit,
        built: None,
        generation: 0,
    });
}

/// Shows the bay when the pilot is in it (and puts out the camera's fill light, which is for
/// space: a metre from a wall it would blind them).
fn show_bay(
    indoors: Res<Indoors>,
    scene: Option<Res<BayScene>>,
    fill: Query<Entity, With<FillLight>>,
    mut vis: Query<&mut Visibility>,
) {
    let Some(scene) = scene else { return };
    if !indoors.is_changed() {
        return;
    }
    let (inside, outside) = if indoors.0 {
        (Visibility::Inherited, Visibility::Hidden)
    } else {
        (Visibility::Hidden, Visibility::Inherited)
    };
    for e in [scene.root, scene.suit] {
        if let Ok(mut v) = vis.get_mut(e) {
            *v = inside;
        }
    }
    for e in &fill {
        if let Ok(mut v) = vis.get_mut(e) {
            *v = outside;
        }
    }
}

/// Armour in eighths from a condition in percent (0: not fitted).
fn eighths(condition: Option<u8>) -> u8 {
    condition.map_or(0, |c| (u32::from(c) * 7).div_ceil(100).clamp(1, 7) as u8)
}

/// Opens the doors, turns the beacons, and keeps the suit in the gantry as the hangar says.
#[allow(clippy::too_many_arguments)]
fn drive_bay(
    mut commands: Commands,
    time: Res<VisTime>,
    state: Res<BayState>,
    indoors: Res<Indoors>,
    scene: Option<ResMut<BayScene>>,
    mut transforms: Query<&mut Transform>,
    mut points: Query<&mut PointLight>,
    mut spots: Query<&mut SpotLight>,
    beacons: Query<&Beacon>,
    mut drives: Query<&mut SuitDrive>,
    visuals: Query<&SuitVisual>,
    mut vis: Query<&mut Visibility>,
) {
    let Some(mut scene) = scene else { return };
    if !indoors.0 {
        return;
    }
    // The doors slide apart into the walls.
    let open = state.doors.clamp(0.0, 1.0);
    let ease = open * open * (3.0 - 2.0 * open);
    for (k, &door) in scene.doors.iter().enumerate() {
        let side = if k == 0 { -1.0 } else { 1.0 };
        if let Ok(mut tf) = transforms.get_mut(door) {
            tf.translation.x = side * (DOOR_HALF_WIDTH / 2.0 + ease * (DOOR_HALF_WIDTH + 1.0));
        }
    }
    let open = state.outer.clamp(0.0, 1.0);
    let ease = open * open * (3.0 - 2.0 * open);
    let tw = DOOR_HALF_WIDTH + 4.0;
    for (k, &door) in scene.outer.iter().enumerate() {
        let side = if k == 0 { -1.0 } else { 1.0 };
        if let Ok(mut tf) = transforms.get_mut(door) {
            tf.translation.x = side * (tw / 2.0 + ease * (tw + 1.0));
        }
    }
    // The airlock's door slides aside into the wall.
    let open = state.airlock.clamp(0.0, 1.0);
    if let Ok(mut tf) = transforms.get_mut(scene.airlock) {
        tf.translation.z = -20.0 + 2.7 * open * open * (3.0 - 2.0 * open);
    }
    // Cycling: the beacons turn, the lamps dim to let the red read.
    for &e in &scene.alarms {
        let phase = beacons.get(e).map_or(0.0, |b| b.phase);
        if let Ok(mut l) = points.get_mut(e) {
            let pulse = 0.5 + 0.5 * ((time.now * 5.0) as f32 + phase).sin();
            l.intensity = if state.alarm { 4.0e5 * pulse } else { 0.0 };
        }
    }
    let dim = if state.alarm { 0.35 } else { 1.0 };
    for &(e, full) in &scene.lamps {
        if let Ok(mut l) = points.get_mut(e) {
            l.intensity = full * dim;
        }
        if let Ok(mut l) = spots.get_mut(e) {
            l.intensity = full * dim;
        }
    }

    // The suit in the gantry.
    let Some(suit) = &state.suit else {
        if let Ok(mut v) = vis.get_mut(scene.suit) {
            *v = Visibility::Hidden;
        }
        if scene.built.is_some() {
            scene.built = None;
            commands.entity(scene.suit).remove::<SuitDrive>().despawn_children();
        }
        return;
    };
    if let Ok(mut v) = vis.get_mut(scene.suit) {
        *v = Visibility::Inherited;
    }
    let parts: [u8; Part::COUNT] = std::array::from_fn(|i| eighths(suit.parts[i]));
    let key = (suit.line, parts, suit.mounts);
    if scene.built != Some(key) {
        // Rebuilt from scratch (a part fitted comes back, which damage alone never does).
        scene.built = Some(key);
        scene.generation = scene.generation.wrapping_add(1);
        commands.entity(scene.suit).despawn_children();
        commands.entity(scene.suit).remove::<SuitVisual>();
        commands.entity(scene.suit).insert(SuitDrive {
            slot: BAY_SLOT,
            frame: suit.line,
            faction: Faction::Colonies,
            generation: scene.generation,
            own: true,
            pos: BAY_ORIGIN + SUIT_AT,
            rot: Quat::from_rotation_y(std::f32::consts::PI),
            vel: Vec3::ZERO,
            aim: -Vec3::Z,
            flags: ent_flags::ASLEEP,
            thrust: Vec3::ZERO,
            parts,
            holding: None,
            // Held in its gantry, not standing on a body: it doesn't kneel asleep.
            ground: None,
        });
        return;
    }
    if let Ok(mut d) = drives.get_mut(scene.suit) {
        let mut flags = if state.boarded { 0 } else { ent_flags::ASLEEP };
        if state.boost {
            flags |= ent_flags::BOOST;
        }
        let pos = BAY_ORIGIN + SUIT_AT + state.offset;
        if d.flags != flags || d.pos != pos || d.thrust != state.thrust {
            d.flags = flags;
            d.pos = pos;
            d.vel = state.vel;
            d.thrust = state.thrust;
        }
    }
    // The main weapon (in the right hand) shows only if it's fitted.
    if let Ok(v) = visuals.get(scene.suit) {
        let spec = frame(suit.line);
        let in_hand = spec
            .loadout
            .iter()
            .enumerate()
            .find(|(_, m)| m.is_some_and(|m| m.arm == ArmSlot::Right))
            .map(|(k, _)| k);
        let show = in_hand.is_none_or(|k| suit.mounts[k]);
        if let Ok(mut w) = vis.get_mut(v.bones[Bone::Weapon.index()]) {
            let want = if show { Visibility::Inherited } else { Visibility::Hidden };
            if *w != want {
                *w = want;
            }
        }
    }
}
