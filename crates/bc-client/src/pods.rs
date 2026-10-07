//! Pilots' capsules thrown clear of their suits (`Event::Eject`, `docs/DESIGN.md`, "Doom and
//! ejecting"): a small yellow pod coasting from where it left the cockpit, its motor burning for
//! the first moments, from the view model's [`PodFeed`]. The pilot's own is what their camera
//! follows once they're out (`net_view`). Pooled.

use bevy::prelude::*;

use crate::beams::{Ribbons, beam_tag, place_ribbon};
use crate::gfx::Gfx;
use crate::materials::{HullTag, Surfaces, paint};
use crate::particles::{At, Particles};
use crate::view::{PodFeed, VisTime};

/// Capsules drawn at once.
const PODS: usize = 8;
/// A capsule's length (m).
const LENGTH: f32 = 3.2;
/// How long its motor burns, s.
pub const BURN_SECS: f32 = 1.6;

#[derive(Component)]
pub struct PodBody(usize);
#[derive(Component)]
pub struct PodExhaust(usize);

pub fn setup_pods(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    surfaces: Res<Surfaces>,
    ribbons: Res<Ribbons>,
) {
    let mesh = meshes.add(Capsule3d::new(0.75, LENGTH - 1.5).mesh().rings(2).latitudes(8).longitudes(10));
    for i in 0..PODS {
        commands.spawn((
            PodBody(i),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(surfaces.armour.clone()),
            HullTag::paint(paint::YELLOW, i as u8).tag(),
            Transform::default(),
            Visibility::Hidden,
        ));
        commands.spawn((
            PodExhaust(i),
            Mesh3d(ribbons.mesh.clone()),
            MeshMaterial3d(ribbons.exhaust.material.clone()),
            beam_tag((i * 37) as u8, false),
            Transform::default(),
            Visibility::Hidden,
        ));
    }
}

pub fn update_pods(
    time: Res<VisTime>,
    gfx: Res<Gfx>,
    ribbons: Res<Ribbons>,
    feed: Res<PodFeed>,
    mut particles: ResMut<Particles>,
    mut bodies: Query<(&PodBody, &mut Transform, &mut Visibility), Without<PodExhaust>>,
    mut exhausts: Query<(&PodExhaust, &mut Transform, &mut Visibility), Without<PodBody>>,
) {
    let cap = gfx.settings.particles;
    let look = &ribbons.exhaust;
    for (b, mut tf, mut vis) in &mut bodies {
        let Some(p) = feed.0.get(b.0) else {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        };
        // Along its flight, tumbling slowly.
        let dir = (p.vel - p.suit_vel).normalize_or(Vec3::Y);
        tf.translation = p.pos;
        tf.rotation = Quat::from_rotation_arc(Vec3::Y, dir) * Quat::from_rotation_x(p.age * 0.7);
        if *vis != Visibility::Visible {
            *vis = Visibility::Visible;
        }
        if p.age < BURN_SECS {
            let tail = p.pos - dir * LENGTH * 0.5;
            particles.exhaust(cap, At { pos: tail, vel: p.vel }, 0.8, time.dt);
        }
    }
    for (e, mut tf, mut vis) in &mut exhausts {
        let Some(p) = feed.0.get(e.0).filter(|p| p.age < BURN_SECS) else {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        };
        let dir = (p.vel - p.suit_vel).normalize_or(Vec3::Y);
        let tail = p.pos - dir * LENGTH * 0.5;
        // The motor's flame shortens as it burns out.
        let left = 1.0 - p.age / BURN_SECS;
        place_ribbon(&mut tf, tail, dir, (look.length * 0.4 * left).max(1.0), look.half_width * 0.7);
        if *vis != Visibility::Visible {
            *vis = Visibility::Visible;
        }
    }
}
