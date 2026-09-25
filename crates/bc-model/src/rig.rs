//! The suit skeleton: bones, where their joints sit at rest, and which hit-box part each belongs
//! to. Every frame shares it. The rest pose follows the simulation's humanoid capsules
//! (`bc_sim::content::frames`), so what's drawn is where the server's shots land.

use bc_proto::Part;
use glam::Vec3;

/// A bone. Parents come before their children.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bone {
    /// The root: the lower torso, at the suit's origin.
    Torso,
    Chest,
    Head,
    /// Hips and skirt armour.
    Waist,
    ThighL,
    ShinL,
    FootL,
    ThighR,
    ShinR,
    FootR,
    /// Shoulder armour, which rides the shoulder joint.
    ShoulderL,
    UpperArmL,
    ForearmL,
    HandL,
    ShoulderR,
    UpperArmR,
    ForearmR,
    HandR,
    Backpack,
    WingL,
    WingR,
    /// The main weapon, in the right hand.
    Weapon,
    /// A shield on the left forearm.
    Shield,
    /// Things that float free round the suit (the Virgo's Planet Defensors).
    Props,
}

pub const BONES: usize = 24;

/// A bone's place in the skeleton.
#[derive(Clone, Copy, Debug)]
pub struct BoneDef {
    pub parent: Option<Bone>,
    /// The joint, in the suit's frame at rest (x right, y up, z forward; origin at the torso).
    pub joint: Vec3,
    pub part: Part,
}

const fn def(parent: Option<Bone>, joint: [f32; 3], part: Part) -> BoneDef {
    BoneDef { parent, joint: Vec3::new(joint[0], joint[1], joint[2]), part }
}

use Bone::*;

/// Every bone, in [`Bone`] order.
pub const ALL: [Bone; BONES] = [
    Torso, Chest, Head, Waist, ThighL, ShinL, FootL, ThighR, ShinR, FootR, ShoulderL, UpperArmL, ForearmL,
    HandL, ShoulderR, UpperArmR, ForearmR, HandR, Backpack, WingL, WingR, Weapon, Shield, Props,
];

const DEFS: [BoneDef; BONES] = [
    def(None, [0.0, 0.0, 0.0], Part::Torso),
    def(Some(Torso), [0.0, 2.2, 0.0], Part::Torso),
    def(Some(Chest), [0.0, 5.9, 0.15], Part::Head),
    def(Some(Torso), [0.0, 0.2, 0.0], Part::Torso),
    // Legs: hip, knee, ankle.
    def(Some(Waist), [-1.3, -0.7, 0.0], Part::Legs),
    def(Some(ThighL), [-1.3, -4.5, 0.3], Part::Legs),
    def(Some(ShinL), [-1.3, -8.0, 0.1], Part::Legs),
    def(Some(Waist), [1.3, -0.7, 0.0], Part::Legs),
    def(Some(ThighR), [1.3, -4.5, 0.3], Part::Legs),
    def(Some(ShinR), [1.3, -8.0, 0.1], Part::Legs),
    // Arms: shoulder armour and joint, elbow, wrist; along the arm capsules.
    def(Some(Chest), [-3.1, 4.5, 0.0], Part::ArmL),
    def(Some(ShoulderL), [-3.3, 4.2, 0.0], Part::ArmL),
    def(Some(UpperArmL), [-3.4, 2.4, 0.5], Part::ArmL),
    def(Some(ForearmL), [-3.5, 0.6, 1.1], Part::ArmL),
    def(Some(Chest), [3.1, 4.5, 0.0], Part::ArmR),
    def(Some(ShoulderR), [3.3, 4.2, 0.0], Part::ArmR),
    def(Some(UpperArmR), [3.4, 2.4, 0.5], Part::ArmR),
    def(Some(ForearmR), [3.5, 0.6, 1.1], Part::ArmR),
    def(Some(Chest), [0.0, 4.1, -2.1], Part::Backpack),
    def(Some(Backpack), [-1.2, 4.9, -3.0], Part::Backpack),
    def(Some(Backpack), [1.2, 4.9, -3.0], Part::Backpack),
    def(Some(HandR), [3.5, 0.3, 1.3], Part::ArmR),
    def(Some(ForearmL), [-4.3, 1.5, 0.8], Part::ArmL),
    def(Some(Torso), [0.0, 0.0, 0.0], Part::Torso),
];

impl Bone {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn def(self) -> &'static BoneDef {
        &DEFS[self.index()]
    }

    /// The joint's rest offset from its parent's joint (the bone's rest translation).
    pub fn rest(self) -> Vec3 {
        let d = self.def();
        d.joint - d.parent.map_or(Vec3::ZERO, |p| p.def().joint)
    }
}

/// Left or right, for building and posing both sides from one description of the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    L,
    R,
}

impl Side {
    pub const BOTH: [Side; 2] = [Side::L, Side::R];

    /// -1 on the left, 1 on the right.
    pub fn sign(self) -> f32 {
        match self {
            Side::L => -1.0,
            Side::R => 1.0,
        }
    }

    pub fn pick<T>(self, left: T, right: T) -> T {
        match self {
            Side::L => left,
            Side::R => right,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parents_come_first_and_indices_match() {
        for (i, b) in ALL.iter().enumerate() {
            assert_eq!(b.index(), i);
            if let Some(p) = b.def().parent {
                assert!(p.index() < i, "{b:?} comes before its parent");
            }
        }
    }

    #[test]
    fn sides_mirror() {
        for (l, r) in
            [(ThighL, ThighR), (ShinL, ShinR), (FootL, FootR), (HandL, HandR), (UpperArmL, UpperArmR)]
        {
            let (a, b) = (l.def().joint, r.def().joint);
            assert_eq!(a, Vec3::new(-b.x, b.y, b.z));
        }
    }
}
