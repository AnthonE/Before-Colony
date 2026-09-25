//! Static sector geometry: the sector box and the L1 colony cylinder. Shared by the server and the
//! client's prediction, so suits bounce off the colony hull identically on both.

use glam::Vec3;

use crate::config::SECTOR_LIMIT;
use crate::flight::FlightState;

/// O'Neill cylinder "L1 Colony Cluster, Colony 03": axis along X, below the combat zone.
pub const COLONY_CENTER: Vec3 = Vec3::new(0.0, -4_200.0, 0.0);
pub const COLONY_RADIUS: f32 = 3_200.0;
pub const COLONY_HALF_LENGTH: f32 = 16_000.0;
/// Hull clearance for suits, m.
const HULL_MARGIN: f32 = 12.0;

/// Keeps a suit inside the sector and outside the colony hull (inelastic contact).
pub fn constrain(s: &mut FlightState) {
    for i in 0..3 {
        if s.pos[i] > SECTOR_LIMIT {
            s.pos[i] = SECTOR_LIMIT;
            s.vel[i] = s.vel[i].min(0.0);
        } else if s.pos[i] < -SECTOR_LIMIT {
            s.pos[i] = -SECTOR_LIMIT;
            s.vel[i] = s.vel[i].max(0.0);
        }
    }
    let rel = s.pos - COLONY_CENTER;
    if rel.x.abs() > COLONY_HALF_LENGTH {
        return;
    }
    let radial = Vec3::new(0.0, rel.y, rel.z);
    let r2 = radial.length_squared();
    let limit = COLONY_RADIUS + HULL_MARGIN;
    if r2 < limit * limit {
        let r = crate::math::sqrt(r2);
        let n = if r > 1e-3 { radial / r } else { Vec3::Y };
        s.pos = COLONY_CENTER + Vec3::new(rel.x, 0.0, 0.0) + n * limit;
        let vn = s.vel.dot(n);
        if vn < 0.0 {
            s.vel -= n * vn;
        }
    }
}

/// Whether a point is inside the colony's solid hull.
pub fn inside_colony(p: Vec3) -> bool {
    let rel = p - COLONY_CENTER;
    rel.x.abs() <= COLONY_HALF_LENGTH && rel.y * rel.y + rel.z * rel.z <= COLONY_RADIUS * COLONY_RADIUS
}
