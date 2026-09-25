//! Battle damage on the suits, from each part's armour ([`SuitDrive::parts`], in eighths) and the
//! hits they take:
//! - every bone shades by its part's armour (scorching, burnt-through paint, bare frame) and glows
//!   where it was just hit;
//! - plates come away as a part wears down;
//! - a part shot to nothing breaks away: its bones fly off as one piece, and the stump sparks;
//! - a suit that dies goes up in secondary blasts and comes apart.
//!
//! All of it follows the replicated part states and seeds, so every client sees the same pieces
//! go. (For now the pieces are visual; the salvage milestones make them objects.)

use bc_model::rig::{self, BONES, Bone};
use bc_proto::Part;
use bc_proto::snapshot::ent_flags;
use bc_sim::math::Rng;
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use crate::anim::Anim;
use crate::blast::Blasts;
use crate::gfx::Gfx;
use crate::materials::HullTag;
use crate::particles::{At, Particles};
use crate::suits_vis::{SuitVisual, bone_point};
use crate::view::{FxEvent, FxEvents, SuitDrive, VisTime};

/// Seconds a part stays hot after a hit.
const HEAT_SECS: f32 = 1.6;
/// Seconds a broken-off piece flies before it's gone, and a stump sparks.
const DEBRIS_SECS: f32 = 30.0;
const STUMP_SECS: f64 = 5.0;

/// The bones that come away with each part (and everything below them).
fn breaks(part: Part) -> &'static [Bone] {
    match part {
        Part::Head => &[Bone::Head],
        Part::ArmL => &[Bone::ShoulderL],
        Part::ArmR => &[Bone::ShoulderR],
        Part::Legs => &[Bone::ThighL, Bone::ThighR],
        Part::Backpack => &[Bone::Backpack],
        Part::Torso => &[],
    }
}

/// Whether `bone` hangs below `root` (or is it).
fn under(bone: Bone, root: Bone) -> bool {
    let mut b = Some(bone);
    while let Some(k) = b {
        if k == root {
            return true;
        }
        b = k.def().parent;
    }
    false
}

/// A suit's damage, between frames.
#[derive(Component)]
pub struct Damage {
    /// Armour per part last seen, in eighths.
    parts: [u8; Part::COUNT],
    /// How hot each part is from recent hits, 0..1.
    heat: [f32; Part::COUNT],
    /// Bones that have come away (they're no longer the suit's to pose).
    pub lost: [bool; BONES],
    wreck: bool,
    /// Whether the suit's state when first seen has been taken in: a suit that turns up already
    /// damaged just looks it, without its losses playing out again.
    primed: bool,
    /// Secondary blasts still to come, and stumps still sparking: (until when, where).
    blasts: Vec<(f64, Bone)>,
    stumps: Vec<(f64, Bone)>,
    rng: Rng,
}

impl Damage {
    pub fn new(slot: u16) -> Self {
        Self {
            parts: [7; Part::COUNT],
            heat: [0.0; Part::COUNT],
            lost: [false; BONES],
            wreck: false,
            primed: false,
            blasts: Vec::new(),
            stumps: Vec::new(),
            rng: Rng::new(0xDA3A_6E00 ^ u64::from(slot)),
        }
    }
}

/// A piece broken off a suit, flying free.
#[derive(Component)]
pub struct Debris {
    vel: Vec3,
    spin: Vec3,
    born: f64,
}

/// Breaks `root` (and the bones under it) off the suit as a free piece.
#[allow(clippy::too_many_arguments)]
fn break_off(
    commands: &mut Commands,
    d: &SuitDrive,
    anim: &Anim,
    v: &SuitVisual,
    dmg: &mut Damage,
    root: Bone,
    kick: f32,
    now: f64,
) {
    if dmg.lost[root.index()] {
        return;
    }
    for b in rig::ALL {
        if under(b, root) {
            dmg.lost[b.index()] = true;
        }
    }
    // Where the bone is now, and which way it's turned.
    let pos = bone_point(d, Some(anim), root, Vec3::ZERO);
    let rot = d.rot * anim.world_rot(root);
    let out = (pos - d.pos).normalize_or(Vec3::Y);
    let r = &mut dmg.rng;
    let jitter = Vec3::new(r.signed(), r.signed(), r.signed());
    let piece = commands
        .spawn((
            Debris {
                vel: d.vel + (out + jitter * 0.4).normalize_or(out) * kick,
                spin: Vec3::new(r.signed(), r.signed(), r.signed()) * 1.5,
                born: now,
            },
            Transform::from_translation(pos).with_rotation(rot),
            Visibility::default(),
        ))
        .id();
    commands.entity(v.bones[root.index()]).insert((ChildOf(piece), Transform::IDENTITY));
    dmg.stumps.push((now + STUMP_SECS, root.def().parent.unwrap_or(Bone::Torso)));
}

/// Applies damage to every suit: shading, heat, plates, breakage and death.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn damage_suits(
    mut commands: Commands,
    time: Res<VisTime>,
    gfx: Res<Gfx>,
    events: Res<FxEvents>,
    mut particles: ResMut<Particles>,
    mut blasts: ResMut<Blasts>,
    mut suits: Query<(&SuitDrive, &SuitVisual, &Anim, &mut Damage)>,
    mut tags: Query<&mut MeshTag>,
) {
    let now = time.now;
    let dt = time.dt;
    let cap = gfx.settings.particles;
    for (d, v, anim, mut dmg) in &mut suits {
        let dmg = &mut *dmg;
        let fresh = !dmg.primed;
        if fresh {
            // First sight: parts already gone are simply not there.
            dmg.primed = true;
            dmg.parts = d.parts;
            dmg.wreck = d.flags & ent_flags::WRECK != 0;
            for part in Part::ALL {
                if d.parts[part as usize] == 0 || dmg.wreck {
                    for &root in breaks(part) {
                        for b in rig::ALL {
                            if under(b, root) {
                                dmg.lost[b.index()] = true;
                            }
                        }
                        commands.entity(v.bones[root.index()]).insert(Visibility::Hidden);
                    }
                }
            }
        }
        // Heat from hits on this suit, cooling.
        for h in &mut dmg.heat {
            *h = (*h - dt / HEAT_SECS).max(0.0);
        }
        for ev in &events.0 {
            if let FxEvent::Hit { target: Some((slot, part)), .. } = *ev
                && slot == d.slot
            {
                dmg.heat[part as usize] = 1.0;
            }
        }
        // Plates come away as each part wears through its eighths.
        // (A fresh suit's shading is set from its state as first seen.)
        let mut changed = fresh;
        for part in Part::ALL {
            let (was, now_eighths) = (dmg.parts[part as usize], d.parts[part as usize]);
            if now_eighths < was {
                changed = true;
                let bone = breaks(part).first().copied().unwrap_or(Bone::Chest);
                let at = bone_point(d, Some(anim), bone, Vec3::ZERO);
                for threshold in [5, 3, 1] {
                    if was > threshold && now_eighths <= threshold {
                        blasts.chips(at, d.vel, 3, HullTag { heat: 12, ..v.tag() });
                    }
                }
                if now_eighths == 0 {
                    particles.impact(
                        cap,
                        At { pos: at, vel: d.vel },
                        (at - d.pos).normalize_or(Vec3::Y),
                        Vec3::new(9.0, 5.0, 1.5),
                        2.0,
                    );
                    for &root in breaks(part) {
                        break_off(&mut commands, d, anim, v, dmg, root, 18.0, now);
                    }
                }
            }
            dmg.parts[part as usize] = now_eighths;
        }
        // Death: secondary blasts through the frame, then it comes apart.
        let wreck = d.flags & ent_flags::WRECK != 0;
        if wreck && !dmg.wreck {
            for (k, bone) in [Bone::Backpack, Bone::ShoulderR, Bone::Chest].into_iter().enumerate() {
                dmg.blasts.push((now + 0.12 + 0.2 * k as f64, bone));
            }
            for part in [Part::ArmL, Part::ArmR, Part::Legs, Part::Head, Part::Backpack] {
                for &root in breaks(part) {
                    let kick = 25.0 + dmg.rng.next_f32() * 20.0;
                    break_off(&mut commands, d, anim, v, dmg, root, kick, now);
                }
            }
            changed = true;
        }
        if wreck != dmg.wreck {
            dmg.wreck = wreck;
            changed = true;
        }
        let blasts_due: Vec<Bone> = dmg.blasts.iter().filter(|(t, _)| *t <= now).map(|(_, b)| *b).collect();
        dmg.blasts.retain(|(t, _)| *t > now);
        for bone in blasts_due {
            let at = bone_point(d, Some(anim), bone, Vec3::ZERO);
            particles.explosion(cap, At { pos: at, vel: d.vel }, 0.35);
        }
        // Stumps spark and arc.
        dmg.stumps.retain(|(until, _)| *until > now);
        for &(_, bone) in &dmg.stumps {
            if dmg.rng.next_f32() < dt * 6.0 {
                let at = bone_point(d, Some(anim), bone, Vec3::ZERO);
                let n = Vec3::new(dmg.rng.signed(), dmg.rng.signed(), dmg.rng.signed()).normalize_or(Vec3::Y);
                particles.impact(cap, At { pos: at, vel: d.vel }, n, Vec3::new(6.0, 7.0, 12.0), 0.5);
            }
        }
        // Shade every bone by its part's armour and heat.
        let hot = dmg.heat.iter().any(|h| *h > 0.0);
        if changed || hot {
            for bone in rig::ALL {
                let part = bone.def().part as usize;
                let t = HullTag {
                    armour: dmg.parts[part],
                    heat: (dmg.heat[part] * 31.0) as u8,
                    wreck,
                    ..v.tag()
                };
                if let Ok(mut m) = tags.get_mut(v.bones[bone.index()]) {
                    *m = t.tag();
                }
            }
        }
    }
}

/// Moves and spins the broken-off pieces, and clears them away in time.
pub fn update_debris(
    mut commands: Commands,
    time: Res<VisTime>,
    mut pieces: Query<(Entity, &Debris, &mut Transform)>,
) {
    let dt = time.dt;
    for (e, p, mut tf) in &mut pieces {
        if (time.now - p.born) as f32 > DEBRIS_SECS {
            commands.entity(e).despawn();
            continue;
        }
        tf.translation += p.vel * dt;
        tf.rotation = (Quat::from_scaled_axis(p.spin * dt) * tf.rotation).normalize();
    }
}
