//! The cockpit, as the pilot sees it from the seat: the main screen ahead in its frame (the
//! panoramic view of space, the world drawn behind it), a ring of monitors round it, panels closing
//! in each side, the canopy's ribs overhead, the console with its round radar sphere below, the
//! armrests with their switches, and the two control grips. It's built in the camera's own space
//! (the eye at the origin, looking along −z, up +y, right +x), so the client hangs it on the camera
//! and it turns with the view. The grips are their own meshes ([`Cockpit::grip`]), so the client
//! moves them with the pilot's hands.
//!
//! Everything sits outside a clear box round the crosshair ([`CLEAR_YAW`] × [`CLEAR_PITCH`]): the
//! middle of the view is the panoramic monitor, which is simply the world drawn behind the frame.
//! The monitors' faces are [`Screen`]s: quads the client paints with its instruments, each a
//! region of one shared texture.
//!
//! Its look is the HUD's (`docs/UI.md`): dark gunmetal, the monitors slim slabs of glass on their
//! mounts, and the suit's own light in thin lines (its eye colour, `Paint::Eye`): a line under each
//! monitor, the panoramic window's four corners lit as the HUD's panels have their ticks, the radar's
//! quarter marks; dimmer seams of cold light along the canopy's ribs.

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
    /// The Orrery and the target, across the top.
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
    /// A control grip's handle, built about its pivot (its foot, the handle rising along +y, the
    /// thumb button toward +z), and where each sits: left, then right.
    pub grip: MeshData,
    pub grips: [Affine3A; 2],
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
    let at = |x: f32, y: f32, z: f32| Affine3A::from_translation(Vec3::new(x, y, z));
    let monitors = monitors();
    let mut b = Builder::default();
    let mut screens = Vec::new();
    let frame = Paint::Fixed(paint::DARK);
    let metal = Paint::Metal(paint::GUNMETAL);
    // The suit's own light, in thin lines, and the dimmer seams.
    let light = Paint::Eye;
    let seam = Paint::Glow(paint::DARK);
    for (k, m) in monitors.iter().enumerate() {
        b.seed = 0.13 * k as f32;
        let xf = facing(m.yaw, m.pitch, m.dist, m.tilt);
        // A slim housing, a gunmetal lip round the glass, and the screen's face just proud of it.
        b.extrude(&chamfered(m.w + 0.05, m.h + 0.05, 0.035), 0.07, frame, xf * at(0.0, 0.0, -0.035));
        b.extrude(&chamfered(m.w + 0.014, m.h + 0.014, 0.018), 0.012, metal, xf * at(0.0, 0.0, 0.008));
        // A line of light under the glass, the status lamp at its corner, and the mount behind.
        b.cube(Vec3::new(m.w * 0.3, 0.003, 0.004), 0.0, light, xf * at(0.0, -m.h * 0.5 - 0.016, 0.004));
        b.cube(
            Vec3::new(0.022, 0.012, 0.012),
            0.0,
            Paint::Glow(paint::YELLOW),
            xf * at(-m.w * 0.5 + 0.02, m.h * 0.5 + 0.016, 0.004),
        );
        b.cube(Vec3::new(0.08, 0.08, 0.12), 0.01, metal, xf * at(0.0, 0.0, -0.12));
        screens.push(m.screen());
    }
    // The frame the monitors hang on: struts running out from the view's edge to the corners, an
    // arch overhead, and bulkheads down each side.
    b.seed = 0.7;
    for side in [-1.0f32, 1.0] {
        // Bulkheads, well out to the side (seen at a wide field of view): a plate, and an inset panel
        // on it with its vents.
        let bulkhead = facing(side * deg(62.0), 0.0, 1.3, 0.0);
        b.extrude(&chamfered(0.9, 1.9, 0.2), 0.12, frame, bulkhead);
        b.extrude(&chamfered(0.6, 1.2, 0.12), 0.03, metal, bulkhead * at(side * 0.05, 0.05, 0.07));
        for k in 0..5 {
            b.cube(
                Vec3::new(0.42, 0.022, 0.02),
                0.004,
                frame,
                bulkhead * at(side * 0.05, -0.35 + 0.07 * k as f32, 0.09),
            );
        }
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
        // A grip's console arm (the handle is its own mesh, `Cockpit::grip`).
        let grip = grip_at(side);
        b.cube(Vec3::new(0.38, 0.12, 0.3), 0.02, metal, grip * at(0.0, -0.08, -0.05));
        // The main screen's frame (Wing's cockpits: a window ahead, closed in round it): a pillar
        // each side of the clear view, and the canopy's ribs sweeping up and out from its corners,
        // each with its seam of light.
        b.cube(Vec3::new(0.07, 1.16, 0.08), 0.015, frame, facing(side * MAIN_YAW, 0.0, 1.5, 0.0));
        let rib = facing(side * deg(38.0), deg(36.0), 1.45, 0.0) * Affine3A::from_rotation_z(side * 0.75);
        b.cube(Vec3::new(0.06, 0.95, 0.07), 0.015, metal, rib);
        b.cube(Vec3::new(0.01, 0.8, 0.01), 0.0, seam, rib * at(0.0, 0.0, 0.04));
        // The window's corners, lit: the HUD's ticks, round the view of space.
        for up in [-1.0f32, 1.0] {
            let corner = facing(side * CORNER_YAW, up * CORNER_PITCH, 1.47, 0.0);
            let arm = 0.13;
            b.cube(Vec3::new(arm, 0.006, 0.006), 0.0, light, corner * at(-side * arm * 0.5, 0.0, 0.0));
            b.cube(Vec3::new(0.006, arm, 0.006), 0.0, light, corner * at(0.0, -up * arm * 0.5, 0.0));
        }
        // A panel closing in each side behind the monitors, and an armrest below it with its
        // switches lit.
        b.extrude(&chamfered(1.05, 1.5, 0.18), 0.06, frame, facing(side * deg(50.0), deg(-2.0), 1.48, 0.0));
        let arm = facing(side * deg(56.0), deg(-46.0), 1.3, deg(30.0));
        b.cube(Vec3::new(0.32, 0.1, 0.45), 0.025, metal, arm);
        for (k, colour) in [paint::YELLOW, paint::RED, paint::BLUE, paint::YELLOW].into_iter().enumerate() {
            let x = -0.09 + 0.06 * k as f32;
            b.cube(Vec3::new(0.035, 0.02, 0.035), 0.0, Paint::Glow(colour), arm * at(x, 0.06, -0.12));
        }
    }
    // The main screen's top and foot across the clear view.
    let span = 2.0 * 1.5 * MAIN_YAW.sin() + 0.1;
    b.cube(Vec3::new(span, 0.07, 0.08), 0.015, frame, facing(0.0, MAIN_PITCH, 1.5, 0.0));
    b.cube(Vec3::new(span, 0.07, 0.08), 0.015, frame, facing(0.0, -MAIN_PITCH, 1.5, 0.0));
    // A strip of switches along the console's top, lit.
    for k in 0..9 {
        let yaw = deg(-14.0 + 3.5 * k as f32);
        let colour = [paint::YELLOW, paint::BLUE, paint::RED][k % 3];
        b.cube(
            Vec3::new(0.03, 0.018, 0.03),
            0.0,
            Paint::Glow(colour),
            facing(yaw, deg(-34.5), 1.22, deg(35.0)),
        );
    }
    b.cube(Vec3::new(1.5, 0.08, 0.1), 0.02, metal, facing(0.0, deg(38.5), 1.3, 0.0));
    // The console below the view, the radar's bezel set into it, ticked round, its quarters lit.
    let radar_at = dir(0.0, deg(-28.0)) * 1.3;
    let radar = Radar { centre: radar_at, radius: 0.17 };
    b.extrude(&chamfered(1.3, 0.5, 0.12), 0.1, frame, facing(0.0, deg(-40.0), 1.25, deg(35.0)));
    let bezel = facing(0.0, deg(-28.0), 1.3, 0.0);
    b.lathe(
        &[(0.185, -0.03), (0.215, -0.03), (0.215, 0.03), (0.19, 0.035)],
        36,
        metal,
        bezel * Affine3A::from_rotation_x(std::f32::consts::FRAC_PI_2),
    );
    for k in 0..16 {
        let a = k as f32 * std::f32::consts::TAU / 16.0;
        let quarter = k % 4 == 0;
        b.cube(
            Vec3::new(if quarter { 0.026 } else { 0.014 }, 0.005, 0.006),
            0.0,
            if quarter { light } else { seam },
            bezel * at(a.cos() * 0.2, a.sin() * 0.2, 0.038) * Affine3A::from_rotation_z(a),
        );
    }
    crate::ao::bake(std::slice::from_mut(&mut b), &[Vec3::ZERO]);
    // A grip's handle, about its foot: a trigger guard, the handle with a band of light round it,
    // and the thumb button.
    let mut g = Builder::default();
    g.lathe(
        &[(0.0, 0.0), (0.045, 0.0), (0.05, 0.1), (0.042, 0.22), (0.048, 0.28), (0.0, 0.3)],
        12,
        frame,
        Affine3A::IDENTITY,
    );
    g.lathe(&[(0.0505, 0.12), (0.0505, 0.128)], 12, light, Affine3A::IDENTITY);
    g.cube(Vec3::new(0.03, 0.03, 0.02), 0.0, Paint::Glow(paint::RED), at(0.0, 0.29, 0.03));
    g.cube(Vec3::new(0.012, 0.07, 0.03), 0.004, metal, at(0.0, 0.17, -0.05));
    let grips = [-1.0f32, 1.0].map(|side| grip_at(side) * at(0.0, -0.02, 0.0));
    Cockpit { shell: b.finish(), screens, radar, light: Vec3::new(0.0, -0.2, -0.6), grip: g.finish(), grips }
}

/// The panoramic window's lit corners: just inside its frame, outside the clear box.
const CORNER_YAW: f32 = 0.445;
const CORNER_PITCH: f32 = 0.345;

/// Where the main screen's frame stands off the crosshair: its pillars' yaw and its top and foot's
/// pitch (radians), just outside the clear box.
const MAIN_YAW: f32 = 0.47;
const MAIN_PITCH: f32 = 0.37;

/// Where a control grip sits, `side` −1 the left and 1 the right: low and to the side, tilted back.
fn grip_at(side: f32) -> Affine3A {
    facing(side * 33f32.to_radians(), (-40f32).to_radians(), 1.1, 20f32.to_radians())
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
        // The grips, where they move, too.
        for at in c.grips {
            for p in c.grip.positions.iter().map(|p| at.transform_point3(Vec3::from(*p))) {
                let d = p.normalize();
                assert!(p.length() >= NEAREST);
                assert!(d.y.asin().abs() >= CLEAR_PITCH, "a grip reaches into the view at {p}");
            }
        }
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
