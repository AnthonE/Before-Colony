//! Two-bone inverse kinematics for the legs: where a thigh and a shin turn so the ankle reaches a
//! point (a foot planted on the ground), the knee bending toward a pole (forward).
//!
//! Plain glam math, for drawing: the server never sees a leg.

use glam::{Mat3, Quat, Vec3};

use crate::rig::Bone;

/// The thigh (hip to knee), the shin (knee to ankle), and the ankle to the sole, m.
pub const THIGH: f32 = 3.712;
pub const SHIN: f32 = 5.457;
pub const ANKLE_TO_SOLE: f32 = 1.47;

/// A leg reaching from `hip` for `target` (where the ankle should be), its knee bending toward
/// `knee_pole`: the thigh's rotation (in the frame `hip` is in) and the shin's (relative to the
/// thigh), each taking a straight leg's bone, pointing down (-y) with its knee to the front (+z),
/// to where it reaches. Beyond the leg's reach it points straight at the target; nearer than the
/// fold allows, it folds as far as it goes.
pub fn two_bone(hip: Vec3, knee_pole: Vec3, target: Vec3, l1: f32, l2: f32) -> (Quat, Quat) {
    let to = target - hip;
    let dir = to.normalize_or(Vec3::NEG_Y);
    let reach = to.length().clamp((l1 - l2).abs() + 1e-4, l1 + l2);
    // The plane the leg bends in: the target's line, and the pole off it.
    let pole = knee_pole - hip;
    let fwd = (pole - dir * pole.dot(dir)).try_normalize().unwrap_or_else(|| dir.any_orthonormal_vector());
    // The angle at the hip between the target's line and the thigh (law of cosines).
    let cos_hip = ((l1 * l1 + reach * reach - l2 * l2) / (2.0 * l1 * reach)).clamp(-1.0, 1.0);
    let sin_hip = (1.0 - cos_hip * cos_hip).sqrt();
    let thigh = dir * cos_hip + fwd * sin_hip;
    let knee = hip + thigh * l1;
    let shin = (hip + dir * reach - knee).normalize_or(dir);
    // The thigh's frame: its bone down -y, its knee to +z, the hinge along x.
    let down = thigh;
    let front = (fwd - down * fwd.dot(down)).normalize_or(Vec3::Z);
    let thigh_rot = Quat::from_mat3(&Mat3::from_cols((-down).cross(front), -down, front)).normalize();
    // The shin turns about the knee's hinge (x), back from the thigh's line.
    let s = thigh_rot.inverse() * shin;
    let shin_rot = Quat::from_rotation_x((-s.z).atan2(-s.y));
    (thigh_rot, shin_rot)
}

/// The rig's leg lengths, from its joints: hip to knee, and knee to ankle.
pub fn rig_legs() -> (f32, f32) {
    (Bone::ShinL.rest().length(), Bone::FootL.rest().length())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where the knee and ankle of a leg posed `(thigh, shin)` from `hip` are.
    fn joints(hip: Vec3, (thigh, shin): (Quat, Quat)) -> (Vec3, Vec3) {
        let knee = hip + thigh * Vec3::NEG_Y * THIGH;
        let ankle = knee + thigh * shin * Vec3::NEG_Y * SHIN;
        (knee, ankle)
    }

    #[test]
    fn two_bone_reaches_bends_forward_and_clamps_beyond_reach() {
        let hip = Vec3::new(-1.3, -0.7, 0.0);
        let pole = hip + Vec3::Z * 10.0;
        let mut seen = 0;
        for k in 0..400 {
            let a = k as f32 * 0.37;
            let d = 0.6 + (k % 9) as f32 * 0.8;
            let target = hip + Vec3::new(a.sin() * 0.4, -1.0, a.cos() * 0.5).normalize() * d;
            let leg = two_bone(hip, pole, target, THIGH, SHIN);
            let (knee, ankle) = joints(hip, leg);
            assert!((knee.distance(hip) - THIGH).abs() < 1e-3);
            let reach = target.distance(hip);
            if reach < THIGH + SHIN - 1e-3 && reach > (THIGH - SHIN).abs() + 1e-3 {
                // In reach: there.
                assert!(ankle.distance(target) < 1e-3, "{k}: {} m short", ankle.distance(target));
                // The knee forward of the hip-ankle line (toward the pole).
                let line = (target - hip).normalize();
                let off = knee - hip - line * (knee - hip).dot(line);
                assert!(off.dot(Vec3::Z) > -1e-4, "{k}: the knee bends back");
                seen += 1;
            } else if reach >= THIGH + SHIN {
                // Too far: straight, pointing at it.
                assert!(ankle.distance(hip + (target - hip).normalize() * (THIGH + SHIN)) < 1e-3, "{k}");
            }
            // The shin turns only about the knee's hinge.
            assert!(leg.1.xyz().cross(Vec3::X).length() < 1e-5);
        }
        assert!(seen > 200);
        // At rest, with the ankle straight below the hip at full stretch, nothing turns.
        let (thigh, shin) = two_bone(hip, pole, hip - Vec3::Y * (THIGH + SHIN), THIGH, SHIN);
        assert!(thigh.angle_between(Quat::IDENTITY) < 1e-3 && shin.angle_between(Quat::IDENTITY) < 1e-3);
    }

    #[test]
    fn the_lengths_are_the_rigs() {
        let (thigh, shin) = rig_legs();
        assert!((thigh - THIGH).abs() < 0.01 && (shin - SHIN).abs() < 0.01, "{thigh} {shin}");
        // The sole sits ANKLE_TO_SOLE under the ankle joint.
        assert!((Bone::FootL.def().joint.y - ANKLE_TO_SOLE + 9.07).abs() < 1e-3);
    }
}
