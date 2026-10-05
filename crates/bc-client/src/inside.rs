//! Flying a suit inside the colony (`bc_sim::colony::interior`, the colony's own sector): the
//! camera sees the city's layer (`city.rs`), so the suits are put on it too, the render origin
//! stays at the colony's axis (its frame's numbers are the suits', within 16 km: plenty for `f32`),
//! and the inner gate's ring of lights shows where to dock. The flying itself is the cockpit's as
//! ever: the snapshots are the inside sector's, in the colony's frame.
//!
//! On foot in the city the pilot watches the suits flying near them (a spectator's snapshots of the
//! inside's sector): they go on the city's layer too, placed relative to the render origin, which
//! follows the walker there, as everything of the city is.
//!
//! Weapons fire only in the Blast Hall (`bc_sim::colony::hall`), and what it puts out (beams,
//! tracers, flashes, sparks, blasts, missiles: [`WeaponFx`]) is drawn on the city's layer as well
//! while the pilot flies inside, where its numbers are the colony's frame's as the camera's are.

use bc_sim::colony::interior::{INNER_GATE, INNER_GATE_RADIUS};
use bevy::camera::visibility::RenderLayers;
use bevy::math::DVec3;
use bevy::prelude::*;

use crate::city::{CITY_LAYER, Placed, RenderOrigin};
use crate::net::GameClient;
use crate::suits_vis::SuitVisual;
use crate::view::SuitDrive;

pub struct InsidePlugin;

impl Plugin for InsidePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_gate)
            .add_systems(
                Update,
                (
                    inside_view.after(crate::view::Vis::Suits).before(crate::view::Vis::Camera),
                    publish,
                    fx_layers,
                ),
            )
            .init_resource::<FlyingInside>();
    }
}

/// The inner gate's ring of lights.
#[derive(Component)]
struct GateRing;

/// The pilot flies inside the colony this frame (the render origin at its axis).
#[derive(Resource, Default)]
pub struct FlyingInside(pub bool);

/// What weapons put out (beams, tracers, their lights, the particles, blasts, missiles): drawn on
/// the city's layer too while the pilot flies inside, where only the Blast Hall's training rounds
/// fly.
#[derive(Component)]
pub struct WeaponFx;

/// Puts the weapons' effects on the city's layer too while the pilot flies inside (the render origin
/// at the axis: their numbers are the colony's frame's), and back on space's alone when not. On foot
/// in the city, watching, they aren't drawn: their numbers aren't the render origin's.
fn fx_layers(
    game: Option<NonSend<GameClient>>,
    mut commands: Commands,
    fx: Query<Entity, With<WeaponFx>>,
    mut flying: ResMut<FlyingInside>,
) {
    let inside = game.is_some_and(|g| g.borrow().core.inside());
    if inside == flying.0 {
        return;
    }
    flying.0 = inside;
    let layers = if inside { RenderLayers::from_layers(&[0, CITY_LAYER]) } else { RenderLayers::layer(0) };
    for e in &fx {
        commands.entity(e).insert(layers.clone());
    }
}

fn setup_gate(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let ring = meshes.add(Mesh::from(Torus::new(INNER_GATE_RADIUS - 1.5, INNER_GATE_RADIUS + 1.5)));
    let lit = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.7, 0.25),
        emissive: LinearRgba::rgb(6.0, 3.6, 0.9),
        unlit: true,
        ..default()
    });
    commands.spawn((
        GateRing,
        Mesh3d(ring),
        MeshMaterial3d(lit),
        // Its axis down the colony: a suit comes to rest in it facing the end cap.
        Transform::from_rotation(Quat::from_rotation_z(core::f32::consts::FRAC_PI_2)),
        Placed(INNER_GATE.as_dvec3()),
        RenderLayers::layer(CITY_LAYER),
        Visibility::Hidden,
    ));
}

/// `window.__bc.interior`: the pilot's sector is the colony's inside. (A showcase has no game.)
fn publish(game: Option<NonSend<GameClient>>, status: Option<ResMut<crate::dev_hooks::DevStatus>>) {
    if let (Some(game), Some(mut status)) = (game, status) {
        status.set("interior", game.borrow().core.welcome.is_some_and(|w| w.interior));
    }
}

/// While the pilot flies inside the colony: the render origin at the axis, every suit (and all of
/// its pieces) on the city's layer, and the gate's ring shown. On foot in the city, the suits they
/// watch on the city's layer too, placed relative to the render origin (`city::place_all`). Out
/// again, the suits go back to space's layer. A showcase has no game, and places its own.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn inside_view(
    game: Option<NonSend<GameClient>>,
    mut origin: ResMut<RenderOrigin>,
    mut commands: Commands,
    mut suits: Query<(Entity, &SuitDrive, Option<&mut Placed>), With<SuitVisual>>,
    children: Query<&Children>,
    layers: Query<Option<&RenderLayers>>,
    mut ring: Query<&mut Visibility, With<GateRing>>,
    mut was: Local<bool>,
) {
    let Some(game) = game else { return };
    let (inside, watching) = {
        let g = game.borrow();
        (g.core.inside(), g.core.hangar.in_city())
    };
    if inside && origin.0 != DVec3::ZERO {
        origin.0 = DVec3::ZERO;
    }
    let city = inside || watching;
    for mut v in &mut ring {
        v.set_if_neq(if city { Visibility::Inherited } else { Visibility::Hidden });
    }
    if !city && !*was {
        return;
    }
    *was = city;
    let want = if city { RenderLayers::layer(CITY_LAYER) } else { RenderLayers::layer(0) };
    for (root, d, placed) in &mut suits {
        for e in std::iter::once(root).chain(children.iter_descendants(root)) {
            if layers.get(e).ok().flatten() != Some(&want) {
                commands.entity(e).insert(want.clone());
            }
        }
        match placed {
            Some(mut p) if watching => p.0 = d.pos.as_dvec3(),
            None if watching => {
                commands.entity(root).insert(Placed(d.pos.as_dvec3()));
            }
            Some(_) => {
                commands.entity(root).remove::<Placed>();
            }
            None => {}
        }
    }
}
