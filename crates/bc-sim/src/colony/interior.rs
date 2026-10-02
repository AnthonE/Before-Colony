//! Suits inside the colony (`docs/SUITS_INSIDE.md`): an interior sector's world is the colony's
//! own frame ([`super::frame`]), where the city stands still. A suit there feels the spin as
//! gravity (the centrifugal pull, 1 g at the floor), is turned aside by Coriolis, and flies through
//! air; it's kept inside the hull and the end caps and out of the city's boxes.
//!
//! All of it is closed forms of where the suit is and how it moves, deterministic and
//! allocation-free, run by the server and the owner's prediction alike.

use glam::Vec3;

use super::city::{CityBox, MAX_HEIGHT, Rect, Stage, each_solid};
use super::frame::{CityPos, SPIN_RATE, Under, from_colony, s_scale, strip_edge, up_at};
use bc_proto::InputCmd;

use crate::content::FrameSpec;
use crate::flight::{FlightMods, FlightOut, FlightState};
use crate::math::{cos, sin, sqrt};
use crate::world::{COLONY_HALF_LENGTH, COLONY_RADIUS};

/// Which world a sector simulates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorldKind {
    /// Space round the colony (the sector's frame).
    #[default]
    Space,
    /// The colony's inside, in its own frame: weapons safe by the colony's law.
    Interior,
}

/// Air: ½ ρ C_d A for a suit, kg/m (ρ 1.2 kg/m³, C_d·A 15 m²). A Leo falling flat out comes to
/// about 95 m/s.
pub const DRAG_KG_M: f32 = 0.5 * 1.2 * 15.0;
/// How far a suit's hull keeps from the floor, the glass and the caps, m.
pub const MARGIN: f32 = 6.0;

/// The inner launch gate, at the docking hub's end near the axis: where suits come in from the
/// bays (nose down the colony), and where they dock again.
pub const INNER_GATE: Vec3 = Vec3::new(-COLONY_HALF_LENGTH + 200.0, 300.0, 0.0);
/// The inner gate's ring, m: at rest within it, a suit docks.
pub const INNER_GATE_RADIUS: f32 = 120.0;
/// How fast a suit comes out of the inner gate, m/s.
pub const INNER_LAUNCH_SPEED: f32 = 30.0;

/// The pull a free suit feels at `pos` moving at `vel` in the colony's frame, of mass `mass_kg`:
/// the spin's centrifugal pull, Coriolis, and the air's drag. m/s².
#[inline]
pub fn accel(pos: Vec3, vel: Vec3, mass_kg: f32) -> Vec3 {
    let w = SPIN_RATE;
    // ω = w·X: the centrifugal pull is ω²·r out from the axis, Coriolis is −2 ω × v.
    let centrifugal = Vec3::new(0.0, pos.y, pos.z) * (w * w);
    let coriolis = Vec3::new(0.0, 2.0 * w * vel.z, -2.0 * w * vel.y);
    let speed = vel.length();
    let drag = vel * (-DRAG_KG_M * speed / mass_kg.max(1.0));
    centrifugal + coriolis + drag
}

/// Across the strip, at height `h`: the direction `s` grows in, in the colony's frame.
fn across(strip: u8, s: f32) -> Vec3 {
    let a = strip_edge(strip as usize) + s / COLONY_RADIUS;
    Vec3::new(0.0, -sin(a), cos(a))
}

/// Keeps a suit of hull radius `r` inside the colony: within the end caps, off the floor and the
/// glass, and out of the city's boxes (inelastic: what carries it into them is taken away).
/// Whether it had to move it.
pub fn constrain(f: &mut FlightState, r: f32) -> bool {
    let mut moved = false;
    // The end caps.
    let cap = COLONY_HALF_LENGTH - r - MARGIN;
    if f.pos.x > cap {
        f.pos.x = cap;
        f.vel.x = f.vel.x.min(0.0);
        moved = true;
    } else if f.pos.x < -cap {
        f.pos.x = -cap;
        f.vel.x = f.vel.x.max(0.0);
        moved = true;
    }
    // The city's boxes, near enough the floor to have any, then the floor and the glass (the
    // hull, from inside): a push across the strip, being straight, leaves the curve by a little,
    // and the floor may put it back against a box.
    for k in 0..8 {
        let mut again = false;
        // Wedged between boxes that meet (a building on its podium, its neighbour), it's pushed
        // out of the shallowest face of each in turn and back again: after a couple of goes, up
        // and over the lot.
        if let Under::Land(c) = from_colony(f.pos)
            && c.h - r <= MAX_HEIGHT + 20.0
            && let Some((delta, normal)) = push_out(&c, r, k >= 2)
        {
            f.pos += delta;
            let vn = f.vel.dot(normal);
            if vn < 0.0 {
                f.vel -= normal * vn;
            }
            again = true;
        }
        let rr = sqrt(f.pos.y * f.pos.y + f.pos.z * f.pos.z);
        let most = COLONY_RADIUS - r - MARGIN;
        if rr > most {
            let out = Vec3::new(0.0, f.pos.y / rr, f.pos.z / rr);
            f.pos -= out * (rr - most);
            let vn = f.vel.dot(out);
            if vn > 0.0 {
                f.vel -= out * vn;
            }
            again = true;
        }
        moved |= again;
        if !again {
            break;
        }
    }
    moved
}

/// The shallowest way out of the deepest box a sphere of radius `r` at `c` overlaps (`up_only`: up
/// out of its top): the move, and the face's outward normal, in the colony's frame.
fn push_out(c: &CityPos, r: f32, up_only: bool) -> Option<(Vec3, Vec3)> {
    let rs = r / s_scale(c.h).max(0.1);
    let area = Rect::new(c.s - rs, c.s + rs, c.x - r, c.x + r);
    // (depth, along: 0 x / 1 s / 2 h, sign)
    let mut best: Option<(f32, usize, f32)> = None;
    let mut deepest = 0.0f32;
    each_solid(c.strip, &area, Stage(0), |b: &CityBox| {
        // The sky-high walls are the walkers' (past the strip's edge, the end caps): the glass and
        // the caps hold suits instead.
        if b.h1 > 2_000.0 {
            return false;
        }
        if !(b.rect.overlaps(&area) && c.h - r < b.h1 && b.h0 < c.h + r) {
            return false;
        }
        // Distances out through each face (the sphere's extent counted).
        let outs = [
            (b.rect.x1 - (c.x - r), 0, 1.0),
            ((c.x + r) - b.rect.x0, 0, -1.0),
            ((b.rect.s1 - (c.s - rs)) * s_scale(c.h), 1, 1.0),
            (((c.s + rs) - b.rect.s0) * s_scale(c.h), 1, -1.0),
            (b.h1 - (c.h - r), 2, 1.0),
            // Never down through the floor, out of a box that stands on it.
            (if b.h0 < 1.0 { f32::INFINITY } else { (c.h + r) - b.h0 }, 2, -1.0),
        ];
        let mut way = outs[4];
        if !up_only {
            for o in outs {
                if o.0 < way.0 {
                    way = o;
                }
            }
        }
        if way.0 > deepest {
            deepest = way.0;
            best = Some(way);
        }
        false
    });
    let (depth, along, sign) = best?;
    let p = c.to_colony();
    let dir = match along {
        0 => Vec3::X,
        1 => across(c.strip, c.s),
        _ => up_at(p),
    } * sign;
    Some((dir * depth, dir))
}

/// A tick of a suit's flight inside the colony: the flight model (with `mods.interior` set, the
/// spin's pull, Coriolis and the air), then the hull, the caps and the city's boxes. The server
/// and the owner's prediction both fly it so.
pub fn step(f: &mut FlightState, cmd: &InputCmd, spec: &FrameSpec, mods: &FlightMods, dt: f32) -> FlightOut {
    let out = crate::flight::integrate(f, cmd, spec, mods, dt);
    constrain(f, spec.radius);
    out
}

/// Whether a suit at `pos` moving at `vel` is at rest in the inner gate's ring (to dock).
pub fn in_gate(pos: Vec3, vel: Vec3) -> bool {
    pos.distance(INNER_GATE) <= INNER_GATE_RADIUS && vel.length() < 25.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::city::solid;
    use crate::colony::frame::{STRIP_WIDTH, gravity};

    #[test]
    fn the_spin_pulls_a_g_at_the_floor_and_less_up_high() {
        let floor = CityPos::new(0, 0.0, 1_000.0, 0.0).to_colony();
        let a = accel(floor, Vec3::ZERO, 8_000.0);
        assert!((a.length() - gravity(0.0)).abs() < 1e-3, "{a}");
        assert!((a.length() - 9.81).abs() < 0.2, "about 1 g: {a}");
        assert!(a.dot(up_at(floor)) < 0.0, "down, away from the axis");
        let high = CityPos::new(0, 0.0, 1_000.0, 1_600.0).to_colony();
        assert!((accel(high, Vec3::ZERO, 8_000.0).length() - gravity(1_600.0)).abs() < 1e-3);
        assert_eq!(accel(Vec3::new(5.0, 0.0, 0.0), Vec3::ZERO, 8_000.0), Vec3::ZERO);
    }

    #[test]
    fn coriolis_turns_a_falling_suit_against_the_spin_and_air_caps_its_speed() {
        // Dropped from 400 m: it lands behind where it was dropped (against the spin).
        let start = CityPos::new(1, 0.0, 1_500.0, 400.0);
        let mut f = FlightState { pos: start.to_colony(), ..FlightState::default() };
        let dt = crate::config::DT;
        let mut fastest = 0.0f32;
        for _ in 0..30 * 60 {
            f.vel += accel(f.pos, f.vel, 8_000.0) * dt;
            f.pos += f.vel * dt;
            fastest = fastest.max(f.vel.length());
            if constrain(&mut f, 10.0) && f.vel.length() < 0.5 {
                break;
            }
        }
        let Under::Land(at) = from_colony(f.pos) else { panic!("{}", f.pos) };
        assert!(at.h < 40.0, "it came down: {at:?}");
        assert!(at.s < start.s, "against the spin: {} < {}", at.s, start.s);
        assert!(fastest < 100.0, "drag: {fastest}");
    }

    #[test]
    fn constrain_keeps_suits_in_the_hull_and_out_of_every_box() {
        let r = 10.0;
        let mut rng = crate::math::Rng::new(7);
        let mut checked = 0;
        for _ in 0..4_000 {
            let c = CityPos::new(
                (rng.next_u32() % 3) as u8,
                -15_000.0 + rng.next_f32() * 30_000.0,
                rng.next_f32() * STRIP_WIDTH,
                -20.0 + rng.next_f32() * 300.0,
            );
            let mut f =
                FlightState { pos: c.to_colony(), vel: Vec3::new(1.0, -2.0, 3.0), ..Default::default() };
            constrain(&mut f, r);
            let rr = sqrt(f.pos.y * f.pos.y + f.pos.z * f.pos.z);
            assert!(rr <= COLONY_RADIUS - r - MARGIN + 0.01);
            if let Under::Land(at) = from_colony(f.pos) {
                // Away from the strip's edges (the walkers' walls), the boxes are clear by a
                // shrunken sphere.
                if at.s > 3.0 * r && at.s < STRIP_WIDTH - 3.0 * r && at.h > -1.0 {
                    let k = 0.8 * r;
                    let min = Vec3::new(at.x - k, (at.h - k).max(0.5), -(at.s + k));
                    let max = Vec3::new(at.x + k, at.h + k, -(at.s - k));
                    if !solid(at.strip, min, max, Stage(0)) {
                        checked += 1;
                    } else {
                        // Pushed out of one box into another it straddles: allowed only where
                        // boxes meet (a building on its kerb).
                        let lift = Vec3::new(0.0, 3.0, 0.0);
                        assert!(!solid(at.strip, min + lift, max + lift, Stage(0)), "{at:?}");
                    }
                }
            }
        }
        assert!(checked > 2_000, "{checked}");
    }

    #[test]
    fn the_inner_gate_is_inside_and_clear() {
        let mut f = FlightState { pos: INNER_GATE, ..FlightState::default() };
        assert!(!constrain(&mut f, 10.0));
        assert!(in_gate(INNER_GATE, Vec3::ZERO));
        assert!(!in_gate(INNER_GATE, Vec3::X * 40.0));
    }
}
