//! Suits inside the colony (`docs/SUITS_INSIDE.md`): an interior sector's world is the colony's
//! own frame ([`super::frame`]), where the city stands still. A suit there feels the spin as
//! gravity (the centrifugal pull, 1 g at the floor), is turned aside by Coriolis, and flies through
//! air; it's kept inside the hull and the end caps and out of the city's boxes. With its grip
//! armed it lands and walks on the city as on any body (`crate::bodies::Body::City`): [`probe`] is
//! the city's surface, and [`ground_under`] what's straight under a suit.
//!
//! All of it is closed forms of where the suit is and how it moves, deterministic and
//! allocation-free, run by the server and the owner's prediction alike.

use glam::Vec3;

use super::city::{CityBox, MAX_HEIGHT, Rect, Stage, each_solid};
use super::frame::{CityPos, SPIN_RATE, Under, from_colony, s_scale, strip_edge, up_at};
use bc_proto::InputCmd;

use crate::bodies::{Probe, STANCE};
use crate::content::FrameSpec;
use crate::flight::{FlightMods, FlightOut, FlightState};
use crate::math::{cos, length, normalize_or, sin, sqrt};
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
/// How far a suit's hull keeps from the end caps, m.
pub const MARGIN: f32 = 6.0;
/// How far a suit's origin keeps from the hull (the floor and the glass) and above the roofs, m:
/// its stance, so a suit flown down onto them comes to rest where it would stand on them (and one
/// letting go of them doesn't jump).
pub const FLOOR_CLEAR: f32 = STANCE;
/// How far [`probe`] looks for the city's boxes, m: past a grip's reach over the ground (a suit
/// lets go 40 m up, its origin a stance higher), so whatever a suit in the city's grip is near is
/// seen. Past it, the distance [`probe`] gives is no more than this: never more than the true one,
/// which is all sphere tracing asks.
pub const PROBE_REACH: f32 = 64.0;
/// Boxes this tall are the walkers' walls (past a strip's edge, the end caps): the glass and the
/// caps themselves hold suits instead.
const WALKERS_WALL: f32 = 2_000.0;

/// The inner launch gate, at the docking hub's end near the axis: where suits come in from the
/// bays (nose down the colony), and where they dock again.
pub const INNER_GATE: Vec3 = Vec3::new(-COLONY_HALF_LENGTH + 200.0, 300.0, 0.0);
/// The inner gate's ring, m: at rest within it, a suit docks.
pub const INNER_GATE_RADIUS: f32 = 120.0;
/// The port in the end cap's inner face behind the inner gate, where suits launched in from the
/// bays come out (the client draws it, and draws them coming out of it), and its radius, m.
pub const INNER_PORT: Vec3 = Vec3::new(-COLONY_HALF_LENGTH, INNER_GATE.y, INNER_GATE.z);
pub const INNER_PORT_RADIUS: f32 = 70.0;
/// How fast a suit comes out of the inner gate, m/s.
pub const INNER_LAUNCH_SPEED: f32 = 30.0;

/// What acts on a free suit at `pos` moving at `vel` in the colony's frame, of mass `mass_kg`: the
/// spin's pull ([`pull`]) and the air's drag ([`drag`]). m/s².
#[inline]
pub fn accel(pos: Vec3, vel: Vec3, mass_kg: f32) -> Vec3 {
    pull(pos, vel) + drag(vel, mass_kg)
}

/// The spin's pull on anything at `pos` moving at `vel` in the colony's frame: centrifugal and
/// Coriolis, m/s². Only the frame's turning: a pilot falls free under it, and doesn't feel it.
#[inline]
pub fn pull(pos: Vec3, vel: Vec3) -> Vec3 {
    let w = SPIN_RATE;
    // ω = w·X: the centrifugal pull is ω²·r out from the axis, Coriolis is −2 ω × v.
    let centrifugal = Vec3::new(0.0, pos.y, pos.z) * (w * w);
    let coriolis = Vec3::new(0.0, 2.0 * w * vel.z, -2.0 * w * vel.y);
    centrifugal + coriolis
}

/// The air's drag on a suit of mass `mass_kg` moving at `vel` through it, m/s²: a push its pilot
/// feels, as they feel thrust.
#[inline]
pub fn drag(vel: Vec3, mass_kg: f32) -> Vec3 {
    let speed = vel.length();
    vel * (-DRAG_KG_M * speed / mass_kg.max(1.0))
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
            && c.h - FLOOR_CLEAR <= MAX_HEIGHT + 20.0
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
        let most = COLONY_RADIUS - FLOOR_CLEAR;
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

/// The shallowest way out of the deepest box a suit of hull radius `r` at `c` overlaps (`up_only`:
/// up out of its top): the move, and the face's outward normal, in the colony's frame. The suit
/// reaches `r` round its origin and above it, and its stance ([`FLOOR_CLEAR`]) below.
fn push_out(c: &CityPos, r: f32, up_only: bool) -> Option<(Vec3, Vec3)> {
    let rs = r / s_scale(c.h).max(0.1);
    let area = Rect::new(c.s - rs, c.s + rs, c.x - r, c.x + r);
    let below = FLOOR_CLEAR;
    // (depth, along: 0 x / 1 s / 2 h, sign)
    let mut best: Option<(f32, usize, f32)> = None;
    let mut deepest = 0.0f32;
    each_solid(c.strip, &area, Stage(0), |b: &CityBox| {
        if b.h1 > WALKERS_WALL {
            return false;
        }
        if !(b.rect.overlaps(&area) && c.h - below < b.h1 && b.h0 < c.h + r) {
            return false;
        }
        // Distances out through each face (the suit's extent counted).
        let outs = [
            (b.rect.x1 - (c.x - r), 0, 1.0),
            ((c.x + r) - b.rect.x0, 0, -1.0),
            ((b.rect.s1 - (c.s - rs)) * s_scale(c.h), 1, 1.0),
            (((c.s + rs) - b.rect.s0) * s_scale(c.h), 1, -1.0),
            (b.h1 - (c.h - below), 2, 1.0),
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

/// -1 or 1: the side of zero `x` is on (1 at zero).
#[inline]
fn sign(x: f32) -> f32 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

/// The colony's inside as a body ([`crate::bodies::Body::City`]): the signed distance from `p` (its
/// own frame) to the nearest solid, negative inside one, and the outward normal there. The solids
/// are the hull from inside (the floor, and the glass), the end caps, and the city's boxes but the
/// walkers' walls. The hull's and the caps' distances are exact; a box's is measured in city
/// coordinates (plumb is radial, and a metre across is `s_scale(h)` metres where `p` is), its edges
/// rounded ([`BOX_ROUND`]): exact over a roof, as over the floor, and true to the curve's sag
/// (millimetres) beside a wall. Boxes further than [`PROBE_REACH`] aren't looked for, and the
/// distance is then capped there.
pub fn probe(p: Vec3) -> Probe {
    let rr = sqrt(p.y * p.y + p.z * p.z);
    let up = up_at(p);
    let mut best = Probe { dist: COLONY_RADIUS - rr, normal: up };
    let cap = COLONY_HALF_LENGTH - p.x.abs();
    if cap < best.dist {
        best = Probe { dist: cap, normal: Vec3::X * -sign(p.x) };
    }
    let Under::Land(c) = from_colony(p) else { return best };
    // Only a box nearer than what's found already matters, and none past the reach is looked for.
    let reach = best.dist.min(PROBE_REACH);
    // No box stands taller than the tallest building.
    let above = c.h - MAX_HEIGHT;
    if above >= reach {
        return Probe { dist: best.dist.min(above), ..best };
    }
    let k = s_scale(c.h).max(0.1);
    let area = Rect::new(c.s - reach / k, c.s + reach / k, c.x - reach, c.x + reach);
    let across = across(c.strip, c.s);
    let mut found = best;
    each_solid(c.strip, &area, Stage(0), |b: &CityBox| {
        // Further up or down than what's found already (less its rounding), it can't be nearer.
        let below = (b.h0 - c.h).max(c.h - b.h1) - BOX_ROUND;
        if b.h1 <= WALKERS_WALL && below < found.dist {
            let pr = box_probe(&c, k, b, across, up);
            if pr.dist < found.dist {
                found = pr;
            }
        }
        false
    });
    // Nothing nearer than the reach: whatever there is lies further.
    found.dist = found.dist.min(reach);
    found
}

/// How round the city's boxes are at their edges to [`probe`], m (but no more than nearly half
/// their thinnest side): a suit's feet, a centimetre past a kerb's edge as it steps up, then find
/// the edge's own normal there rather than a face's, as on any rounded body.
const BOX_ROUND: f32 = 0.5;

/// Box `b`'s signed distance and outward normal from `c` (city coordinates, a metre across `k`
/// metres there): the round box's formula ([`BOX_ROUND`]), its normal turned into the colony's
/// frame (`across` and `up` at `c`).
fn box_probe(c: &CityPos, k: f32, b: &CityBox, across: Vec3, up: Vec3) -> Probe {
    let centre = Vec3::new((b.rect.x0 + b.rect.x1) * 0.5, (b.rect.s0 + b.rect.s1) * 0.5, (b.h0 + b.h1) * 0.5);
    let half =
        Vec3::new((b.rect.x1 - b.rect.x0) * 0.5, (b.rect.s1 - b.rect.s0) * 0.5 * k, (b.h1 - b.h0) * 0.5);
    let round = BOX_ROUND.min(half.min_element() * 0.99).max(0.0);
    let rel = Vec3::new(c.x - centre.x, (c.s - centre.y) * k, c.h - centre.z);
    let q = rel.abs() - (half - Vec3::splat(round));
    let o = q.max(Vec3::ZERO);
    let m = q.max_element();
    let dist = length(o) + m.min(0.0) - round;
    let n = if m > 0.0 {
        normalize_or(o * Vec3::new(sign(rel.x), sign(rel.y), sign(rel.z)), Vec3::Z)
    } else if q.x >= q.y && q.x >= q.z {
        Vec3::X * sign(rel.x)
    } else if q.y >= q.z {
        Vec3::Y * sign(rel.y)
    } else {
        Vec3::Z * sign(rel.z)
    };
    Probe { dist, normal: Vec3::X * n.x + across * n.y + up * n.z }
}

/// How far straight down from `p` (its own frame) the first solid is, m: the top of the box under
/// it, or the floor (the glass, over a window). Down is the spin's, not toward the nearest wall.
/// None outside the hull, past the caps, or inside a box.
pub fn ground_under(p: Vec3) -> Option<f32> {
    if p.x.abs() > COLONY_HALF_LENGTH {
        return None;
    }
    let c = match from_colony(p) {
        Under::Window { h, .. } => return (h >= 0.0).then_some(h),
        Under::Land(c) => c,
    };
    if c.h < 0.0 {
        return None;
    }
    let mut top = 0.0f32;
    let mut inside = false;
    let e = 0.01;
    let area = Rect::new(c.s - e, c.s + e, c.x - e, c.x + e);
    each_solid(c.strip, &area, Stage(0), |b: &CityBox| {
        if b.h1 <= WALKERS_WALL && b.rect.contains(c.s, c.x) {
            if b.h1 <= c.h {
                top = top.max(b.h1);
            } else if b.h0 < c.h {
                inside = true;
            }
        }
        inside
    });
    (!inside).then_some(c.h - top)
}

/// A tick of a suit's flight inside the colony: the flight model (with `mods.interior` set, the
/// spin's pull, Coriolis and the air), then the hull, the caps and the city's boxes, which are a
/// crash to fly into (`crate::flight::crash`). The server and the owner's prediction both fly it
/// so.
pub fn step(f: &mut FlightState, cmd: &InputCmd, spec: &FrameSpec, mods: &FlightMods, dt: f32) -> FlightOut {
    let out = crate::flight::integrate(f, cmd, spec, mods, dt);
    let v = f.vel;
    constrain(f, spec.radius);
    crate::flight::crash(f, v, mods);
    out
}

/// Whether a suit at `pos` moving at `vel` is at rest in the inner gate's ring (to dock).
pub fn in_gate(pos: Vec3, vel: Vec3) -> bool {
    pos.distance(INNER_GATE) <= INNER_GATE_RADIUS && vel.length() < 25.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::city::solid_built;
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
            assert!(rr <= COLONY_RADIUS - FLOOR_CLEAR + 0.01);
            if let Under::Land(at) = from_colony(f.pos) {
                // Away from the strip's edges (the walkers' walls), the boxes are clear by a
                // shrunken sphere.
                if at.s > 3.0 * r && at.s < STRIP_WIDTH - 3.0 * r && at.h > -1.0 {
                    let k = 0.8 * r;
                    let min = Vec3::new(at.x - k, (at.h - k).max(0.5), -(at.s + k));
                    let max = Vec3::new(at.x + k, at.h + k, -(at.s - k));
                    if !solid_built(at.strip, min, max, Stage(0)) {
                        checked += 1;
                    } else {
                        // Pushed out of one box into another it straddles: allowed only where
                        // boxes meet (a building on its kerb).
                        let lift = Vec3::new(0.0, 3.0, 0.0);
                        assert!(!solid_built(at.strip, min + lift, max + lift, Stage(0)), "{at:?}");
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
