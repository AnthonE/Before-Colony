//! The cockpit, as the pilot sees it from the seat: a ring of monitors round the panoramic view of
//! space, a round radar sphere at the bottom, the console and the two control grips. It's built in
//! the camera's own space (the eye at the origin, looking along −z, up +y, right +x), so the client
//! hangs it on the camera and it turns with the view.
//!
//! Everything sits outside a clear box round the crosshair ([`CLEAR_YAW`] × [`CLEAR_PITCH`]): the
//! middle of the view is the panoramic monitor, which is simply the world drawn behind the frame.
//! The monitors' faces are [`Screen`]s: quads the client paints with its instruments, each a
//! region of one shared texture.

use glam::{Affine3A, Mat3, Quat, Vec2, Vec3};

use crate::kit::{Builder, MeshData, Paint};
use crate::paint;

/// Half-angles (radians) of the box round the crosshair that nothing of the cockpit enters.
pub const CLEAR_YAW: f32 = 0.40;
pub const CLEAR_PITCH: f32 = 0.30;
/// The nearest the cockpit comes to the eye (m): beyond the camera's near plane.
pub const NEAREST: f32 = 0.9;
/// The screens' shared texture, in pixels: a strip across the top (1024 × 256) for the top
/// monitor, then two columns of two panels of 512 × 384. Each screen has its region's shape, so
/// what's drawn on it isn't stretched.
pub const TEXTURE: [u32; 2] = [1024, 1024];

/// What a screen shows (the client lays its instruments out by this).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    /// The ZERO System and the target, across the top.
    Top,
    /// The suit's status and its damage silhouette (upper left).
    Suit,
    /// Arms: the weapons and the special (upper right).
    Arms,
    /// Flight: speed, G, propellant, energy and heat (lower left).
    Flight,
    /// Salvage, credits and the kill feed (lower right).
    Log,
}

/// A monitor's face: where it is, which way it faces (its local +z toward the eye), its size (m)
/// and the region of the shared screen texture it shows (u0, v0, u1, v1).
#[derive(Clone, Copy, Debug)]
pub struct Screen {
    pub show: Show,
    pub centre: Vec3,
    pub rotation: Quat,
    pub size: Vec2,
    pub region: [f32; 4],
}

/// The radar: a sphere at the bottom of the view, in its bezel.
#[derive(Clone, Copy, Debug)]
pub struct Radar {
    pub centre: Vec3,
    pub radius: f32,
}

#[derive(Clone, Debug)]
pub struct Cockpit {
    pub shell: MeshData,
    pub screens: Vec<Screen>,
    pub radar: Radar,
    /// Where the monitors' glow lights the cockpit from.
    pub light: Vec3,
}

/// A direction from the eye, `yaw` right and `pitch` up of straight ahead (radians).
pub fn dir(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos())
}

/// A placement at `yaw`, `pitch` and `dist` from the eye, facing it (local +z toward the eye, +y as
/// near up as it can be), then tilted back by `tilt` (radians, about its own x).
fn facing(yaw: f32, pitch: f32, dist: f32, tilt: f32) -> Affine3A {
    let d = dir(yaw, pitch);
    let z = -d;
    let x = Vec3::Y.cross(z).normalize_or(Vec3::X);
    let y = z.cross(x);
    let rot = Mat3::from_cols(x, y, z) * Mat3::from_rotation_x(tilt);
    Affine3A::from_mat3_translation(rot, d * dist)
}

/// A monitor: a housing of `w` × `h` m with chamfered corners, its screen inset on the face toward
/// the eye.
struct Monitor {
    show: Show,
    yaw: f32,
    pitch: f32,
    dist: f32,
    tilt: f32,
    w: f32,
    h: f32,
    region: [f32; 4],
}

/// An outline with its corners cut (c m).
fn chamfered(w: f32, h: f32, c: f32) -> Vec<Vec2> {
    let (x, y) = (w * 0.5, h * 0.5);
    vec![
        Vec2::new(-x + c, -y),
        Vec2::new(x - c, -y),
        Vec2::new(x, -y + c),
        Vec2::new(x, y - c),
        Vec2::new(x - c, y),
        Vec2::new(-x + c, y),
        Vec2::new(-x, y - c),
        Vec2::new(-x, -y + c),
    ]
}

/// The monitors: a strip across the top of the screen texture, then two columns of two.
fn monitors() -> [Monitor; 5] {
    let deg = |d: f32| d.to_radians();
    [
        Monitor {
            show: Show::Top,
            yaw: 0.0,
            pitch: deg(30.0),
            dist: 1.4,
            tilt: deg(-8.0),
            w: 1.0,
            h: 0.25,
            region: [0.0, 0.0, 1.0, 0.25],
        },
        Monitor {
            show: Show::Suit,
            yaw: deg(-42.0),
            pitch: deg(12.0),
            dist: 1.3,
            tilt: 0.0,
            w: 0.5,
            h: 0.375,
            region: [0.0, 0.25, 0.5, 0.625],
        },
        Monitor {
            show: Show::Arms,
            yaw: deg(42.0),
            pitch: deg(12.0),
            dist: 1.3,
            tilt: 0.0,
            w: 0.5,
            h: 0.375,
            region: [0.5, 0.25, 1.0, 0.625],
        },
        Monitor {
            show: Show::Flight,
            yaw: deg(-42.0),
            pitch: deg(-14.0),
            dist: 1.25,
            tilt: deg(12.0),
            w: 0.5,
            h: 0.375,
            region: [0.0, 0.625, 0.5, 1.0],
        },
        Monitor {
            show: Show::Log,
            yaw: deg(42.0),
            pitch: deg(-14.0),
            dist: 1.25,
            tilt: deg(12.0),
            w: 0.5,
            h: 0.375,
            region: [0.5, 0.625, 1.0, 1.0],
        },
    ]
}

impl Monitor {
    fn screen(&self) -> Screen {
        let (_, rot, t) = facing(self.yaw, self.pitch, self.dist, self.tilt).to_scale_rotation_translation();
        Screen {
            show: self.show,
            centre: t + rot * Vec3::Z * 0.022,
            rotation: rot,
            size: Vec2::new(self.w, self.h),
            region: self.region,
        }
    }
}

/// The monitors' faces alone (what the client lays its instruments out by), without building
/// the shell.
pub fn build_screens() -> Vec<Screen> {
    monitors().iter().map(Monitor::screen).collect()
}

/// Builds the cockpit.
pub fn build() -> Cockpit {
    let deg = |d: f32| d.to_radians();
    let monitors = monitors();
    let mut b = Builder::default();
    let mut screens = Vec::new();
    let frame = Paint::Fixed(paint::DARK);
    let metal = Paint::Metal(paint::GUNMETAL);
    let trim = Paint::Trim;
    for (k, m) in monitors.iter().enumerate() {
        b.seed = 0.13 * k as f32;
        let xf = facing(m.yaw, m.pitch, m.dist, m.tilt);
        // The housing, a bezel round the screen, and the screen's face just proud of it.
        b.extrude(
            &chamfered(m.w + 0.08, m.h + 0.08, 0.07),
            0.08,
            frame,
            xf * Affine3A::from_translation(Vec3::Z * -0.04),
        );
        b.extrude(
            &chamfered(m.w + 0.03, m.h + 0.03, 0.05),
            0.02,
            trim,
            xf * Affine3A::from_translation(Vec3::Z * 0.005),
        );
        // A status lamp in the bezel's corner.
        b.cube(
            Vec3::new(0.035, 0.02, 0.02),
            0.0,
            Paint::Glow(paint::YELLOW),
            xf * Affine3A::from_translation(Vec3::new(-m.w * 0.5 + 0.03, m.h * 0.5 + 0.005, 0.02)),
        );
        screens.push(m.screen());
    }
    // The frame the monitors hang on: struts running out from the view's edge to the corners, an
    // arch overhead, and bulkheads down each side.
    b.seed = 0.7;
    for side in [-1.0f32, 1.0] {
        // Bulkheads, well out to the side (seen at a wide field of view).
        b.extrude(&chamfered(0.9, 1.9, 0.2), 0.12, frame, facing(side * deg(62.0), 0.0, 1.3, 0.0));
        // Struts between the upper and lower monitors, and above the upper ones.
        b.cube(Vec3::new(0.6, 0.06, 0.08), 0.015, metal, facing(side * deg(40.0), deg(-1.5), 1.22, 0.0));
        b.cube(Vec3::new(0.08, 0.9, 0.08), 0.015, metal, facing(side * deg(55.0), 0.0, 1.22, 0.0));
        // The arch's legs, from the top monitor down to the bulkheads.
        b.cube(
            Vec3::new(0.9, 0.07, 0.08),
            0.015,
            metal,
            facing(side * deg(34.0), deg(31.0), 1.3, 0.0) * Affine3A::from_rotation_z(side * 0.35),
        );
        // A grip: the handle rising from its console arm, a trigger guard and a thumb button.
        let grip = facing(side * deg(33.0), deg(-40.0), 1.1, deg(20.0));
        b.cube(
            Vec3::new(0.38, 0.12, 0.3),
            0.02,
            metal,
            grip * Affine3A::from_translation(Vec3::new(0.0, -0.08, -0.05)),
        );
        b.lathe(
            &[(0.0, 0.0), (0.045, 0.0), (0.05, 0.1), (0.042, 0.22), (0.048, 0.28), (0.0, 0.3)],
            12,
            frame,
            grip * Affine3A::from_translation(Vec3::new(0.0, -0.02, 0.0)),
        );
        b.cube(
            Vec3::new(0.03, 0.03, 0.02),
            0.0,
            Paint::Glow(paint::RED),
            grip * Affine3A::from_translation(Vec3::new(0.0, 0.29, 0.03)),
        );
    }
    b.cube(Vec3::new(1.5, 0.08, 0.1), 0.02, metal, facing(0.0, deg(38.5), 1.3, 0.0));
    // The console below the view, the radar's bezel set into it.
    let radar_at = dir(0.0, deg(-28.0)) * 1.3;
    let radar = Radar { centre: radar_at, radius: 0.17 };
    b.extrude(&chamfered(1.3, 0.5, 0.12), 0.1, frame, facing(0.0, deg(-40.0), 1.25, deg(35.0)));
    let bezel = facing(0.0, deg(-28.0), 1.3, 0.0);
    b.lathe(
        &[(0.185, -0.03), (0.215, -0.03), (0.215, 0.03), (0.19, 0.035)],
        36,
        trim,
        bezel * Affine3A::from_rotation_x(std::f32::consts::FRAC_PI_2),
    );
    crate::ao::bake(std::slice::from_mut(&mut b), &[Vec3::ZERO]);
    Cockpit { shell: b.finish(), screens, radar, light: Vec3::new(0.0, -0.2, -0.6) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crosshair_stays_clear() {
        let c = build();
        for p in c.shell.positions.iter().map(|p| Vec3::from(*p)) {
            let d = p.normalize();
            let yaw = d.x.atan2(-d.z).abs();
            let pitch = d.y.asin().abs();
            assert!(p.length() >= NEAREST, "the cockpit comes {:.2} m from the eye", p.length());
            assert!(
                yaw >= CLEAR_YAW || pitch >= CLEAR_PITCH,
                "the cockpit reaches into the view round the crosshair at yaw {yaw:.2}, pitch {pitch:.2} ({p})"
            );
        }
        for s in &c.screens {
            let d = s.centre.normalize();
            assert!(d.x.atan2(-d.z).abs() >= CLEAR_YAW || d.y.asin().abs() >= CLEAR_PITCH);
            // Facing the eye.
            assert!((s.rotation * Vec3::Z).dot(-s.centre.normalize()) > 0.8, "{:?} faces away", s.show);
        }
        let r = c.radar.centre.normalize();
        assert!(r.y.asin().abs() - (c.radar.radius / c.radar.centre.length()) >= CLEAR_PITCH);
    }

    #[test]
    fn the_screens_share_one_texture_without_overlapping() {
        let c = build();
        let area: f32 =
            c.screens.iter().map(|s| (s.region[2] - s.region[0]) * (s.region[3] - s.region[1])).sum();
        assert!((area - 1.0).abs() < 1e-4, "the regions cover the texture once: {area}");
        assert!(c.shell.indices.len() / 3 < 6_000);
        for s in &c.screens {
            let px = [
                (s.region[2] - s.region[0]) * TEXTURE[0] as f32,
                (s.region[3] - s.region[1]) * TEXTURE[1] as f32,
            ];
            assert!((px[0] / px[1] - s.size.x / s.size.y).abs() < 0.01, "{:?} is stretched", s.show);
        }
    }
}
