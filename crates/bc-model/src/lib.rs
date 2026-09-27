//! Procedural mobile suits as plain mesh data: a modelling kit (`kit`), one skeleton for every
//! frame (`rig`) and each frame's design (`frames`). The client bakes each frame into one mesh per
//! bone, per level of detail, and builds a suit as a tree of bone entities, so it can be posed and
//! broken apart.

pub mod frames;
mod gundams;
pub mod kit;
pub mod paint;
pub mod rig;

use bc_proto::FrameId;
use glam::{Affine3A, Vec2, Vec3};

use kit::{Builder, MeshData, Paint};
use rig::{BONES, Bone};

/// How much detail a suit is built with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lod {
    /// Bevels, greebles, and round parts in full segments.
    Near,
    /// A few hundred metres off: no bevels or greebles, half the segments.
    Far,
}

pub const LODS: [Lod; 2] = [Lod::Near, Lod::Far];

/// Where things attach to a frame, in their bone's space. Directions are unit vectors; what a
/// frame's kit lacks stays at its default (None, or empty).
#[derive(Clone, Debug, Default)]
pub struct Sockets {
    /// The main weapon's muzzle, on [`Bone::Weapon`].
    pub muzzle: Vec3,
    /// Main thruster nozzles on [`Bone::Backpack`]: position and exhaust direction.
    pub nozzles: Vec<(Vec3, Vec3)>,
    /// The beam saber's hilt on [`Bone::HandL`], and the blade's direction.
    pub saber: (Vec3, Vec3),
    /// A blade in the left hand, on [`Bone::HandL`]: where it starts (the top of its hilt, or a
    /// polearm's emitter) and the way it points. Heavyarms' army knife, Sandrock's left heat
    /// shotel, Shenlong's beam glaive.
    pub blade_left: Option<(Vec3, Vec3)>,
    /// A blade in the right hand, on [`Bone::Weapon`]: where it starts (the top of its hilt, or
    /// the beam emitter) and the way it points. Deathscythe's beam scythe, Sandrock's right heat
    /// shotel.
    pub blade_right: Option<(Vec3, Vec3)>,
    /// Shenlong's Dragon Fang, on [`Bone::HandR`]: the tip of the dragon head's snout and the way
    /// it faces (where the head flies out on its cable).
    pub fang: Option<(Vec3, Vec3)>,
    /// The flamethrower's nozzle in the dragon's mouth, on [`Bone::HandR`], and the way it fires.
    pub flame: Option<(Vec3, Vec3)>,
    /// Missile launch points (the pods' hatches), each on the bone it rides: shoulder pods, leg
    /// pods.
    pub missiles: Vec<(Bone, Vec3)>,
}

/// One frame at one level of detail.
#[derive(Clone, Debug)]
pub struct Model {
    /// A mesh per bone, in [`Bone`] order (None where the bone has nothing).
    pub bones: Vec<Option<MeshData>>,
    pub sockets: Sockets,
}

impl Model {
    pub fn triangles(&self) -> usize {
        self.bones.iter().flatten().map(|m| m.indices.len() / 3).sum()
    }
}

/// Builds `frame` at `lod`.
pub fn build(frame: FrameId, lod: Lod) -> Model {
    let mut d = Designer::new(lod);
    frames::design(frame, &mut d);
    let bones = d.bones.into_iter().map(|b| (!b.is_empty()).then(|| b.finish())).collect();
    Model { bones, sockets: d.sockets }
}

/// Builds one frame: shapes are given in the suit's frame at rest (x right, y up, z forward) and
/// land in their bone's own space.
pub struct Designer {
    bones: Vec<Builder>,
    pub lod: Lod,
    pub sockets: Sockets,
}

impl Designer {
    fn new(lod: Lod) -> Self {
        Self { bones: (0..BONES).map(|_| Builder::default()).collect(), lod, sockets: Sockets::default() }
    }

    /// Shapes on `bone`.
    pub fn on(&mut self, bone: Bone) -> On<'_> {
        let to_bone = Affine3A::from_translation(-bone.def().joint);
        On { b: &mut self.bones[bone.index()], to_bone, lod: self.lod }
    }

    pub fn near(&self) -> bool {
        self.lod == Lod::Near
    }

    /// A point in the suit's frame at rest, in `bone`'s space (for sockets).
    pub fn local(bone: Bone, p: Vec3) -> Vec3 {
        p - bone.def().joint
    }
}

/// Shapes going onto one bone.
pub struct On<'a> {
    b: &'a mut Builder,
    to_bone: Affine3A,
    lod: Lod,
}

impl On<'_> {
    /// Panel seed for what follows (0..1).
    pub fn seed(&mut self, s: f32) -> &mut Self {
        self.b.seed = s.fract();
        self
    }

    fn chamfer(&self, c: f32) -> f32 {
        if self.lod == Lod::Near { c } else { 0.0 }
    }

    fn segs(&self, n: u32) -> u32 {
        if self.lod == Lod::Near { n } else { (n / 2).max(6) }
    }

    pub fn block(
        &mut self,
        size: Vec3,
        top: Vec2,
        shift: Vec2,
        chamfer: f32,
        paint: Paint,
        xf: Affine3A,
    ) -> &mut Self {
        let c = self.chamfer(chamfer);
        self.b.block(size, top, shift, c, paint, self.to_bone * xf);
        self
    }

    pub fn cube(&mut self, size: Vec3, chamfer: f32, paint: Paint, xf: Affine3A) -> &mut Self {
        self.block(size, Vec2::ONE, Vec2::ZERO, chamfer, paint, xf)
    }

    pub fn lathe(&mut self, profile: &[(f32, f32)], segments: u32, paint: Paint, xf: Affine3A) -> &mut Self {
        let s = self.segs(segments);
        self.b.lathe(profile, s, paint, self.to_bone * xf);
        self
    }

    pub fn cylinder(
        &mut self,
        radius: f32,
        height: f32,
        segments: u32,
        paint: Paint,
        xf: Affine3A,
    ) -> &mut Self {
        let s = self.segs(segments);
        self.b.cylinder(radius, height, s, paint, self.to_bone * xf);
        self
    }

    pub fn sphere(&mut self, radius: f32, rings: u32, paint: Paint, xf: Affine3A) -> &mut Self {
        let r = self.segs(rings).max(4);
        self.b.sphere(radius, r, paint, self.to_bone * xf);
        self
    }

    pub fn extrude(&mut self, outline: &[Vec2], depth: f32, paint: Paint, xf: Affine3A) -> &mut Self {
        self.b.extrude(outline, depth, paint, self.to_bone * xf);
        self
    }

    /// Small detail, only up close.
    pub fn greeble(&mut self, f: impl FnOnce(&mut Self)) -> &mut Self {
        if self.lod == Lod::Near {
            f(self);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::WeaponKind;
    use bc_sim::content::{ArmSlot, Capsule, WeaponClass, weapon};

    #[test]
    fn every_frame_builds_within_budget() {
        for frame in FrameId::ALL {
            let near = build(frame, Lod::Near);
            let far = build(frame, Lod::Far);
            let (n, f) = (near.triangles(), far.triangles());
            assert!(n > 2_000 && n < 20_000, "{frame:?} near: {n} triangles");
            assert!(f * 2 < n, "{frame:?} far ({f}) should be well under near ({n})");
            for (i, m) in near.bones.iter().enumerate() {
                let Some(m) = m else { continue };
                assert_eq!(m.positions.len(), m.normals.len());
                assert_eq!(m.positions.len(), m.colors.len());
                assert!(m.indices.iter().all(|&k| (k as usize) < m.positions.len()), "bone {i}");
                assert!(m.normals.iter().all(|n| (Vec3::from(*n).length() - 1.0).abs() < 1e-3));
            }
            assert!(near.bones[Bone::Torso.index()].is_some() && near.bones[Bone::Weapon.index()].is_some());
            assert!(!near.sockets.nozzles.is_empty());
        }
    }

    /// Whatever a bone carries stays near its hit capsule, so what's drawn is what's hit.
    #[test]
    fn armour_stays_near_its_hitbox() {
        let dist = |p: Vec3, c: &Capsule| {
            let t = ((p - c.a).dot(c.b - c.a) / (c.b - c.a).length_squared()).clamp(0.0, 1.0);
            (p - (c.a + (c.b - c.a) * t)).length() - c.r
        };
        for frame in FrameId::ALL {
            // The simulation's capsules for this frame (bc_sim::content::frames), by part: the
            // humanoid ones, or Neo-Bird's as an aircraft.
            let caps = &bc_sim::content::frame(frame).capsules;
            let m = build(frame, Lod::Near);
            for bone in rig::ALL {
                // Weapons, shields, wings and props reach well beyond the body by design.
                if matches!(bone, Bone::Weapon | Bone::Shield | Bone::WingL | Bone::WingR | Bone::Props) {
                    continue;
                }
                let Some(mesh) = &m.bones[bone.index()] else { continue };
                let cap = &caps[bone.def().part as usize];
                let worst = mesh
                    .positions
                    .iter()
                    .map(|p| dist(Vec3::from(*p) + bone.def().joint, cap))
                    .fold(f32::MIN, f32::max);
                assert!(worst < 2.6, "{frame:?} {bone:?} reaches {worst:.1} m outside its hitbox");
            }
        }
    }

    /// Every frame is drawn as itself: no two build the same mesh.
    #[test]
    fn every_frame_has_its_own_design() {
        let models = FrameId::ALL.map(|f| (f, build(f, Lod::Near)));
        for (i, (a, ma)) in models.iter().enumerate() {
            for (b, mb) in &models[i + 1..] {
                let same = ma.bones.iter().zip(&mb.bones).all(|pair| match pair {
                    (Some(x), Some(y)) => {
                        x.positions == y.positions && x.indices == y.indices && x.colors == y.colors
                    }
                    (None, None) => true,
                    _ => false,
                });
                assert!(!same, "{a:?} and {b:?} build the same mesh");
            }
        }
    }

    /// A frame whose kit (the simulation's loadout and special mounts) has blades, a fang, a
    /// flamethrower or missiles says where they are, and every socket sits on its bone's mesh.
    #[test]
    fn kits_have_their_sockets() {
        let unit = |d: Vec3| (d.length() - 1.0).abs() < 1e-3;
        for frame in FrameId::ALL {
            let m = build(frame, Lod::Near);
            let s = &m.sockets;
            // Within half a metre of the bone's mesh bounds (sockets are in the bone's space).
            let on = |bone: Bone, p: Vec3| {
                m.bones[bone.index()].as_ref().is_some_and(|mesh| {
                    let (lo, hi) = mesh.positions.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), q| {
                        (lo.min(Vec3::from(*q)), hi.max(Vec3::from(*q)))
                    });
                    let margin = Vec3::splat(0.5);
                    p.cmpge(lo - margin).all() && p.cmple(hi + margin).all()
                })
            };
            let placed = |bone: Bone, socket: Option<(Vec3, Vec3)>| {
                socket.is_some_and(|(p, d)| on(bone, p) && unit(d))
            };
            let spec = bc_sim::content::frame(frame);
            for mount in spec.loadout.iter().chain(&spec.special_mounts).flatten() {
                let has = match (weapon(mount.weapon).class, mount.weapon, mount.arm) {
                    (WeaponClass::Melee, WeaponKind::BeamSaber, _) => {
                        on(Bone::HandL, s.saber.0) && unit(s.saber.1)
                    }
                    (WeaponClass::Melee, WeaponKind::DragonFang, _) => placed(Bone::HandR, s.fang),
                    (WeaponClass::Melee, _, ArmSlot::Left) => placed(Bone::HandL, s.blade_left),
                    (WeaponClass::Melee, _, ArmSlot::Right) => placed(Bone::Weapon, s.blade_right),
                    (WeaponClass::Melee, _, ArmSlot::Both) => {
                        placed(Bone::HandL, s.blade_left) && placed(Bone::Weapon, s.blade_right)
                    }
                    (WeaponClass::Cone, ..) => placed(Bone::HandR, s.flame),
                    // Leg pods on the legs; the others elsewhere (the shoulders).
                    (WeaponClass::Missile, _, arm) => s.missiles.iter().any(|&(bone, p)| {
                        (bone.def().part == bc_proto::Part::Legs) == (arm == ArmSlot::LegPods) && on(bone, p)
                    }),
                    _ => true,
                };
                assert!(has, "{frame:?} has no socket on its model for its {:?}", mount.weapon);
            }
            // The client turns the left arm from the saber's rest direction, whatever the kit.
            assert!(unit(s.saber.1), "{frame:?} saber direction");
            assert!(on(Bone::Weapon, s.muzzle), "{frame:?} muzzle");
            for &(p, d) in &s.nozzles {
                assert!(on(Bone::Backpack, p) && unit(d), "{frame:?} nozzle at {p}");
            }
            for &(bone, p) in &s.missiles {
                assert!(on(bone, p), "{frame:?} missile hatch at {p} is off {bone:?}");
            }
            for (bone, socket) in [
                (Bone::HandL, s.blade_left),
                (Bone::Weapon, s.blade_right),
                (Bone::HandR, s.fang),
                (Bone::HandR, s.flame),
            ] {
                if let Some((p, d)) = socket {
                    assert!(on(bone, p) && unit(d), "{frame:?} socket at {p} is off {bone:?}");
                }
            }
        }
    }
}
