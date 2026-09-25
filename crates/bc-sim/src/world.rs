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
    if let Some((at, n)) = hull_contact(s.pos, HULL_MARGIN) {
        s.pos = at;
        let vn = s.vel.dot(n);
        if vn < 0.0 {
            s.vel -= n * vn;
        }
    }
}

/// Whether a sphere of radius `r` at `p` touches the colony: if so, the nearest point out of it (on
/// the hull or an end cap, grown by `r`) and the outward normal there.
pub fn hull_contact(p: Vec3, r: f32) -> Option<(Vec3, Vec3)> {
    let rel = p - COLONY_CENTER;
    let cap = COLONY_HALF_LENGTH + r;
    if rel.x.abs() >= cap {
        return None;
    }
    let radial = Vec3::new(0.0, rel.y, rel.z);
    let r2 = radial.length_squared();
    let limit = COLONY_RADIUS + r;
    if r2 >= limit * limit {
        return None;
    }
    let d = crate::math::sqrt(r2);
    // Out through the nearer face: the curved hull, or an end cap.
    if cap - rel.x.abs() < limit - d {
        let n = if rel.x < 0.0 { -Vec3::X } else { Vec3::X };
        return Some((Vec3::new(COLONY_CENTER.x + n.x * cap, p.y, p.z), n));
    }
    let n = if d > 1e-3 { radial / d } else { Vec3::Y };
    Some((COLONY_CENTER + Vec3::new(rel.x, 0.0, 0.0) + n * limit, n))
}

/// Whether a point is inside the colony's solid hull.
pub fn inside_colony(p: Vec3) -> bool {
    let rel = p - COLONY_CENTER;
    rel.x.abs() <= COLONY_HALF_LENGTH && rel.y * rel.y + rel.z * rel.z <= COLONY_RADIUS * COLONY_RADIUS
}
