//! Missiles in flight, from the view model's [`MissileFeed`]: a slim body along its velocity (red
//! when it's tracking the pilot), its motor glowing at the tail, and exhaust left behind in space
//! as a trail (`particles`). Their bursts are one-shot effects (`fx`). The bodies are pooled.

use bc_proto::WeaponKind;
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use crate::gfx::Gfx;
use crate::materials::{HullTag, Surfaces, paint};
use crate::particles::{At, Particles};
use crate::view::{MissileFeed, VisTime};

/// More than a snapshot lists (12): the client keeps a missile a few ticks after it drops off.
const BODIES: usize = 48;
/// A homing missile's length (m); micro-missiles are smaller.
const LENGTH: f32 = 2.4;

#[derive(Component)]
pub struct MissileBody(usize);

pub fn setup_missiles(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, surfaces: Res<Surfaces>) {
    let mesh = meshes.add(Capsule3d::new(0.22, LENGTH - 0.44).mesh().rings(2).latitudes(6).longitudes(8));
    for i in 0..BODIES {
        commands.spawn((
            MissileBody(i),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(surfaces.armour.clone()),
            HullTag::paint(paint::WHITE, i as u8).tag(),
            Transform::default(),
            Visibility::Hidden,
        ));
    }
}

pub fn update_missiles(
    time: Res<VisTime>,
    gfx: Res<Gfx>,
    feed: Res<MissileFeed>,
    mut particles: ResMut<Particles>,
    mut bodies: Query<(&MissileBody, &mut Transform, &mut Visibility, &mut MeshTag)>,
) {
    let cap = gfx.settings.particles;
    for (b, mut tf, mut vis, mut tag) in &mut bodies {
        let Some(m) = feed.0.get(b.0) else {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        };
        let dir = m.vel.normalize_or(Vec3::Z);
        let scale = if m.kind == WeaponKind::MicroMissile { 0.6 } else { 1.0 };
        tf.translation = m.pos;
        tf.rotation = Quat::from_rotation_arc(Vec3::Y, dir);
        tf.scale = Vec3::splat(scale);
        if *vis != Visibility::Visible {
            *vis = Visibility::Visible;
        }
        let paint = if m.targets_you { paint::RED } else { paint::WHITE };
        let want = HullTag::paint(paint, b.0 as u8).tag();
        if *tag != want {
            *tag = want;
        }
        let tail = m.pos - dir * LENGTH * 0.5 * scale;
        particles.exhaust(cap, At { pos: tail, vel: m.vel }, -dir, scale, time.dt);
    }
}
