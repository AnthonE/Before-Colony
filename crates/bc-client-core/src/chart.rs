//! The chart's view (`bc-client`'s `chart.rs` draws it): an orbit camera that zooms from a suit's
//! length out to the whole Earth Sphere without a cut, and the screen-space work the chart does
//! (what's under the cursor, keeping labels from piling up, the scale and its readouts).
//!
//! - **Orbit, pan, zoom.** Dragging turns the view about its focus; panning slides the focus
//!   across the screen; zooming in closes on the point under the cursor, as a map does. The focus
//!   can be held on something that moves (the pilot's suit), which panning lets go of.
//! - **Seamless to the Earth Sphere.** Zooming out past the sector pulls the focus toward the
//!   Earth Sphere's middle, a little each step, so the sector slides off toward L1 as Earth, the
//!   Moon and the Lagrange points come into view; zooming back in at the sector brings it home.
//! - **Eased.** Every change sets a goal the view eases to (exactly, at any frame rate), so a
//!   jump across 300,000 km is a flight, not a cut.

use glam::{Vec2, Vec3};

use crate::sphere;

/// How close the view comes to its focus, and how far out it goes, m.
pub const MIN_DIST: f32 = 60.0;
pub const MAX_DIST: f32 = 2.6e9;
/// Zooming out between these distances (m), the focus is pulled toward the Earth Sphere's middle.
pub const SPHERE_FROM: f32 = 60_000.0;
pub const SPHERE_TO: f32 = 4.0e8;
/// The view's field of view (vertical), rad.
pub const FOV: f32 = 0.82;
/// One step of the wheel zooms by this much.
pub const ZOOM_STEP: f32 = 1.25;
/// How fast the view eases to its goal (1/s).
const EASE: f32 = 7.0;
/// How far the view tilts, up or down, rad.
const PITCH_LIMIT: f32 = 1.5;
/// Radians of turn per pixel dragged.
const TURN_PER_PX: f32 = 0.0055;

/// The Earth Sphere's middle as the chart frames it: the barycentre, about which the Moon and the
/// Lagrange points lie.
pub fn sphere_centre() -> Vec3 {
    sphere::barycentre()
}

/// Where the view is, or is going.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// What it turns about, m.
    pub focus: Vec3,
    /// Round the sector's up, from +Z toward +X, rad.
    pub yaw: f32,
    /// Above the focus (positive) or below, rad.
    pub pitch: f32,
    /// How far from the focus, m.
    pub dist: f32,
}

impl Pose {
    /// The way from the focus to the eye.
    pub fn back(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(cp * sy, sp, cp * cy)
    }

    pub fn eye(&self) -> Vec3 {
        self.focus + self.back() * self.dist
    }
}

/// The chart's camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartCam {
    /// Where it is this frame.
    pub now: Pose,
    /// Where it's easing to.
    pub goal: Pose,
    /// The focus is held on something that moves (the caller says where it is each frame).
    pub tracking: bool,
}

impl Default for ChartCam {
    /// Over the pilot's shoulder, 9 km out, looking down a little across the sector.
    fn default() -> Self {
        let pose = Pose { focus: Vec3::ZERO, yaw: 0.6, pitch: 0.55, dist: 9_000.0 };
        Self { now: pose, goal: pose, tracking: true }
    }
}

/// Smoothstep.
fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much of a zoom out at `dist` m pulls toward the Earth Sphere, 0..1 (by the log of the
/// distance: 0 inside the sector's scale, 1 at the whole sphere's).
pub fn sphere_pull(dist: f32) -> f32 {
    smooth(SPHERE_FROM.ln(), SPHERE_TO.ln(), dist.max(1.0).ln())
}

impl ChartCam {
    /// Turns the view by a drag of `d` pixels.
    pub fn orbit(&mut self, d: Vec2) {
        self.goal.yaw -= d.x * TURN_PER_PX;
        self.goal.pitch = (self.goal.pitch + d.y * TURN_PER_PX).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Slides the focus across the screen by a drag of `d` pixels, on a view `height` pixels
    /// high. Lets go of whatever it was tracking.
    pub fn pan(&mut self, d: Vec2, height: f32) {
        let per_px = 2.0 * self.goal.dist * (FOV * 0.5).tan() / height.max(1.0);
        let back = self.goal.back();
        let right = Vec3::Y.cross(back).normalize_or(Vec3::X);
        let up = back.cross(right).normalize_or(Vec3::Y);
        self.goal.focus += (-right * d.x + up * d.y) * per_px;
        self.tracking = false;
    }

    /// Zooms by `steps` of the wheel (positive: in), toward `at` (the point under the cursor, at
    /// the focus's depth) when zooming in. Zooming out past the sector pulls toward the Earth
    /// Sphere. Zooming in toward something else lets go of what it was tracking.
    pub fn zoom(&mut self, steps: f32, at: Option<Vec3>) {
        let old = self.goal.dist;
        let new = (old * ZOOM_STEP.powf(-steps)).clamp(MIN_DIST, MAX_DIST);
        self.goal.dist = new;
        if steps < 0.0 {
            // A good part of the way there each step out (half of it once there's no further out
            // to go), so the sphere is framed by the time the view is out at its scale.
            let ratio = (old / new).powi(3);
            let pull = sphere_pull(new) * if new > old { 1.0 - ratio } else { 0.5 };
            if pull > 1e-3 {
                self.goal.focus = self.goal.focus.lerp(sphere_centre(), pull);
                self.tracking = false;
            }
        } else if let Some(p) = at {
            // The point under the cursor stays under it.
            let moved = (p - self.goal.focus) * (1.0 - new / old);
            if moved.length() > old * 0.02 {
                self.goal.focus += moved;
                self.tracking = false;
            }
        }
    }

    /// Flies the view to look at `focus` from `dist` m (keeping its angle).
    pub fn fly_to(&mut self, focus: Vec3, dist: f32, track: bool) {
        self.goal.focus = focus;
        self.goal.dist = dist.clamp(MIN_DIST, MAX_DIST);
        self.tracking = track;
    }

    /// Turns the view to look from `back` (the way from the focus to the eye) toward the focus.
    pub fn look_from(&mut self, back: Vec3) {
        let b = back.normalize_or(Vec3::Y);
        self.goal.yaw = b.x.atan2(b.z);
        self.goal.pitch = b.y.clamp(-1.0, 1.0).asin().clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Looks straight down (or back to a three-quarter view).
    pub fn top_down(&mut self, on: bool) {
        self.goal.pitch = if on { PITCH_LIMIT } else { 0.55 };
    }

    /// Eases toward the goal over `dt` s; while tracking, the focus's goal is `held`.
    pub fn step(&mut self, dt: f32, held: Option<Vec3>) {
        if let (true, Some(p)) = (self.tracking, held) {
            // Keep up with it exactly (it may be moving fast), easing only what's left over.
            let lag = self.goal.focus - self.now.focus;
            self.goal.focus = p;
            self.now.focus = p - lag;
        }
        let k = 1.0 - (-EASE * dt.clamp(0.0, 0.5)).exp();
        let (a, b) = (self.now, self.goal);
        // The focus eases on a scale that keeps a jump across the sphere smooth: in log distance
        // while far, straight lines while near.
        self.now.focus = a.focus.lerp(b.focus, k);
        let turn =
            (b.yaw - a.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        self.now.yaw = a.yaw + turn * k;
        self.now.pitch = a.pitch + (b.pitch - a.pitch) * k;
        self.now.dist = (a.dist.ln() + (b.dist.ln() - a.dist.ln()) * k).exp();
        if (self.now.dist - b.dist).abs() < b.dist * 1e-4 {
            self.now.dist = b.dist;
        }
    }

    /// The near plane for this frame: close enough for what's at the focus, far enough to keep the
    /// depth buffer's precision, m.
    pub fn near(&self) -> f32 {
        (self.now.dist * 0.002).clamp(0.5, 1.0e6)
    }

    /// Metres a pixel covers at the focus, on a view `height` pixels high.
    pub fn per_px(&self, height: f32) -> f32 {
        2.0 * self.now.dist * (FOV * 0.5).tan() / height.max(1.0)
    }
}

/// Metres a pixel covers at `p`, seen from `eye`, on a view `height` pixels high.
pub fn per_px_at(eye: Vec3, p: Vec3, height: f32) -> f32 {
    2.0 * eye.distance(p) * (FOV * 0.5).tan() / height.max(1.0)
}

/// A step for a grid or a scale bar over `span` m: 1, 2 or 5 of a power of ten, about a tenth of
/// it.
pub fn nice_step(span: f32) -> f32 {
    let raw = (span / 10.0).max(1.0);
    let p = 10f32.powf(raw.log10().floor());
    let m = raw / p;
    p * if m < 1.5 {
        1.0
    } else if m < 3.5 {
        2.0
    } else if m < 7.5 {
        5.0
    } else {
        10.0
    }
}

/// A distance as the chart writes it: `850 M`, `12.4 KM`, `326,381 KM`, `1.00 AU`.
pub fn range(m: f32) -> String {
    let m = m.abs();
    if m < 1_000.0 {
        format!("{m:.0} M")
    } else if m < 100_000.0 {
        format!("{:.1} KM", m / 1_000.0)
    } else if m < 0.05 * sphere::AU {
        thousands((m / 1_000.0).round() as u64) + " KM"
    } else {
        format!("{:.2} AU", m / sphere::AU)
    }
}

/// `1234567` as `1,234,567`.
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A duration as the chart writes it: `0:24`, `12:05`, `1:04:00`.
pub fn clock(secs: f32) -> String {
    let s = secs.max(0.0).round() as u64;
    if s >= 3_600 {
        format!("{}:{:02}:{:02}", s / 3_600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Something on the screen the cursor can pick: where, how big (px), and how much it matters (a
/// higher rank wins a near tie).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pickable {
    pub at: Vec2,
    pub radius: f32,
    pub rank: u8,
}

/// The one under `cursor`, within `slop` px of its edge: the nearest to it (by its edge, so the
/// cursor inside one wins it), a pixel's favour a rank to the higher ranks.
pub fn pick(items: &[Pickable], cursor: Vec2, slop: f32) -> Option<usize> {
    items
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let d = p.at.distance(cursor) - p.radius;
            (d <= slop).then_some((i, d - f32::from(p.rank)))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// A label the chart would like to show: its box's top-left corner and size (px), and its rank.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Label {
    pub at: Vec2,
    pub size: Vec2,
    pub rank: u16,
}

/// Which labels to show: the highest ranks first, each only where it doesn't overlap one already
/// shown (with `gap` px between).
pub fn declutter(labels: &[Label], gap: f32) -> Vec<bool> {
    let mut order: Vec<usize> = (0..labels.len()).collect();
    order.sort_by(|&a, &b| labels[b].rank.cmp(&labels[a].rank).then(a.cmp(&b)));
    let mut shown = vec![false; labels.len()];
    let mut placed: Vec<(Vec2, Vec2)> = Vec::new();
    for i in order {
        let l = &labels[i];
        let (lo, hi) = (l.at - Vec2::splat(gap), l.at + l.size + Vec2::splat(gap));
        if placed.iter().all(|(a, b)| hi.x <= a.x || lo.x >= b.x || hi.y <= a.y || lo.y >= b.y) {
            placed.push((l.at, l.at + l.size));
            shown[i] = true;
        }
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle(c: &mut ChartCam, held: Option<Vec3>) {
        for _ in 0..600 {
            c.step(1.0 / 60.0, held);
        }
    }

    #[test]
    fn zooming_out_finds_the_earth_sphere_and_back_in_finds_the_sector() {
        let mut c = ChartCam::default();
        settle(&mut c, Some(Vec3::ZERO));
        // All the way out, a step at a time: the view ends up framing the Earth Sphere.
        for _ in 0..80 {
            c.zoom(-1.0, None);
        }
        settle(&mut c, Some(Vec3::ZERO));
        assert_eq!(c.now.dist, MAX_DIST);
        assert!(c.now.focus.distance(sphere_centre()) < 0.05 * sphere::EARTH_MOON, "{}", c.now.focus);
        assert!(!c.tracking);
        // Back in, pointing at L1 (the sector): it comes home.
        for _ in 0..80 {
            c.zoom(1.0, Some(Vec3::ZERO));
        }
        settle(&mut c, None);
        assert_eq!(c.now.dist, MIN_DIST);
        assert!(c.now.focus.length() < 1_000.0, "{}", c.now.focus);
        // Inside the sector's scale, zooming out doesn't wander.
        let mut c = ChartCam::default();
        c.zoom(-3.0, None);
        settle(&mut c, Some(Vec3::ZERO));
        assert!(c.tracking && c.now.focus.length() < 1.0);
    }

    #[test]
    fn the_view_eases_and_keeps_up_with_what_it_tracks() {
        let mut c = ChartCam::default();
        // A suit at 300 m/s: the focus stays on it to the metre, frame after frame.
        let mut p = Vec3::new(1_000.0, 0.0, 0.0);
        settle(&mut c, Some(p));
        for _ in 0..120 {
            p += Vec3::X * 5.0;
            c.step(1.0 / 60.0, Some(p));
            assert!(c.now.focus.distance(p) < 1.0);
        }
        // Panning lets go; the pose eases to the goal and stays there.
        c.pan(Vec2::new(100.0, 0.0), 800.0);
        assert!(!c.tracking);
        c.orbit(Vec2::new(300.0, -50.0));
        settle(&mut c, Some(p));
        assert!(c.now.focus.distance(c.goal.focus) < 1e-2 && (c.now.yaw - c.goal.yaw).abs() < 1e-4);
        assert!(c.now.focus.distance(p) > 100.0);
        // Looking from a way gets there.
        let mut d = ChartCam::default();
        let from = Vec3::new(-0.4, 0.7, 0.3).normalize();
        d.look_from(from);
        assert!(d.goal.back().distance(from) < 1e-4, "{}", d.goal.back());
        // Pitch stays short of straight up or down.
        c.orbit(Vec2::new(0.0, 10_000.0));
        assert!(c.goal.pitch <= PITCH_LIMIT);
        // Panning moves the focus across the screen: a drag right moves it left.
        let mut c = ChartCam::default();
        c.pan(Vec2::new(10.0, 0.0), 800.0);
        let right = Vec3::Y.cross(c.goal.back());
        assert!(c.goal.focus.dot(right) < 0.0);
    }

    #[test]
    fn picking_and_labels() {
        let items = [
            Pickable { at: Vec2::new(100.0, 100.0), radius: 6.0, rank: 0 },
            Pickable { at: Vec2::new(112.0, 100.0), radius: 6.0, rank: 3 },
            Pickable { at: Vec2::new(400.0, 300.0), radius: 20.0, rank: 0 },
        ];
        assert_eq!(pick(&items, Vec2::new(101.0, 100.0), 8.0), Some(0));
        // A near tie goes to the higher rank.
        assert_eq!(pick(&items, Vec2::new(105.0, 100.0), 8.0), Some(1));
        assert_eq!(pick(&items, Vec2::new(415.0, 300.0), 8.0), Some(2));
        assert_eq!(pick(&items, Vec2::new(250.0, 250.0), 8.0), None);
        let labels = [
            Label { at: Vec2::new(0.0, 0.0), size: Vec2::new(80.0, 14.0), rank: 1 },
            Label { at: Vec2::new(40.0, 6.0), size: Vec2::new(80.0, 14.0), rank: 5 },
            Label { at: Vec2::new(0.0, 40.0), size: Vec2::new(80.0, 14.0), rank: 0 },
        ];
        assert_eq!(declutter(&labels, 2.0), vec![false, true, true]);
    }

    #[test]
    fn readouts() {
        assert_eq!(range(850.4), "850 M");
        assert_eq!(range(12_440.0), "12.4 KM");
        assert_eq!(range(sphere::L1_TO_EARTH), "326,381 KM");
        assert_eq!(range(sphere::AU), "1.00 AU");
        assert_eq!(clock(24.4), "0:24");
        assert_eq!(clock(725.0), "12:05");
        assert_eq!(clock(3_840.0), "1:04:00");
        assert_eq!(nice_step(9_000.0), 1_000.0);
        assert_eq!(nice_step(23_000.0), 2_000.0);
        assert_eq!(nice_step(48_000.0), 5_000.0);
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
