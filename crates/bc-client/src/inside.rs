//! Flying a suit inside the colony (`bc_sim::colony::interior`, the colony's own sector): the
//! camera sees the city's layer (`city.rs`), so the suits are put on it too, the render origin
//! stays at the colony's axis (its frame's numbers are the suits', within 16 km: plenty for `f32`),
//! and the inner gate's ring of lights shows where to dock. The flying itself is the cockpit's as
//! ever: the snapshots are the inside sector's, in the colony's frame.

use bc_sim::colony::interior::{INNER_GATE, INNER_GATE_RADIUS};
use bevy::camera::visibility::RenderLayers;
use bevy::math::DVec3;
use bevy::prelude::*;

use crate::city::{CITY_LAYER, Placed, RenderOrigin};
use crate::net::GameClient;
use crate::suits_vis::SuitVisual;

pub struct InsidePlugin;

impl Plugin for InsidePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_gate).add_systems(
            Update,
            (inside_view.after(crate::view::Vis::Suits).before(crate::view::Vis::Camera), publish),
        );
    }
}

/// The inner gate's ring of lights.
#[derive(Component)]
struct GateRing;

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

/// `window.__bc.interior`: the pilot's sector is the colony's inside.
fn publish(game: NonSend<GameClient>, status: Option<ResMut<crate::dev_hooks::DevStatus>>) {
    if let Some(mut status) = status {
        status.set("interior", game.borrow().core.welcome.is_some_and(|w| w.interior));
    }
}

/// While the pilot flies inside the colony: the render origin at the axis, every suit (and all of
/// its pieces) on the city's layer, and the gate's ring shown. Out again, the suits go back to
/// space's layer.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn inside_view(
    game: NonSend<GameClient>,
    mut origin: ResMut<RenderOrigin>,
    mut commands: Commands,
    suits: Query<Entity, With<SuitVisual>>,
    children: Query<&Children>,
    layers: Query<Option<&RenderLayers>>,
    mut ring: Query<&mut Visibility, With<GateRing>>,
    mut was: Local<bool>,
) {
    let inside = {
        let g = game.borrow();
        g.core.hangar.place == Some(bc_econ::wire::Place::Space) && g.core.welcome.is_some_and(|w| w.interior)
    };
    if inside && origin.0 != DVec3::ZERO {
        origin.0 = DVec3::ZERO;
    }
    for mut v in &mut ring {
        v.set_if_neq(if inside { Visibility::Inherited } else { Visibility::Hidden });
    }
    if !inside && !*was {
        return;
    }
    *was = inside;
    let want = if inside { RenderLayers::layer(CITY_LAYER) } else { RenderLayers::layer(0) };
    for root in &suits {
        for e in std::iter::once(root).chain(children.iter_descendants(root)) {
            if layers.get(e).ok().flatten() != Some(&want) {
                commands.entity(e).insert(want.clone());
            }
        }
    }
}
