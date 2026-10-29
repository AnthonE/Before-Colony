//! The avatars the Elo Pros store's traders wear on the Exchange floor (`docs/FLOOR.md`), where
//! pilots wear their flight suits: anime looks, and now and then an android. Each is a model under
//! `web/assets/avatars/` (made by `scripts/avatars/prep.py`; where each comes from and its licence:
//! `web/assets/avatars/LICENSE.md`), loaded only once someone wearing it is in view.
//!
//! Who wears which is the slot and the name alone, so every screen dresses a trader alike: a
//! trader's slot is one of the store's (`bc_proto::presence::VISITOR_BASE` up), and the look is
//! their name's.

use bc_proto::presence::VISITOR_BASE;

/// One look: its model, how it's scaled and turned to stand as a figure does (1 m a unit, feet at
/// the origin, facing +z), and the animation it plays standing about, if it has one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Under the page's `assets/`.
    pub path: &'static str,
    pub scale: f32,
    /// About the vertical, rad: VRoid's models (VRM 0) face −z.
    pub turn: f32,
    /// The glTF animation it idles with.
    pub idle: Option<usize>,
}

const HALF_TURN: f32 = core::f32::consts::PI;

/// Every look, the anime models first. Their arms are let down into a relaxed stance by
/// `prep.py` (they have no animation); the android plays its own Idle.
pub const LOOKS: [Look; 6] = [
    Look { path: "avatars/avatarsample_d.glb", scale: 1.0, turn: HALF_TURN, idle: None },
    Look { path: "avatars/avatarsample_e.glb", scale: 1.0, turn: HALF_TURN, idle: None },
    Look { path: "avatars/avatarsample_f.glb", scale: 1.0, turn: HALF_TURN, idle: None },
    Look { path: "avatars/avatarsample_g.glb", scale: 1.0, turn: HALF_TURN, idle: None },
    Look { path: "avatars/sendagaya_shino.glb", scale: 1.0, turn: HALF_TURN, idle: None },
    // Quaternius's robot, 4.8 units tall: 1.7 m. Its animations: Dance, Death, Idle (2), Jump, …
    Look { path: "avatars/robotexpressive.glb", scale: 0.36, turn: 0.0, idle: Some(2) },
];

/// FNV-1a over a name: the same look on every screen.
fn hash(name: &str) -> u32 {
    name.bytes().fold(0x811c_9dc5_u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193))
}

/// Which of [`LOOKS`] someone in slot `id` named `name` wears: a store trader one by their name;
/// anyone else (a pilot) none, their flight suit.
pub fn look(id: u16, name: &str) -> Option<usize> {
    (id >= VISITOR_BASE).then(|| (hash(name) % LOOKS.len() as u32) as usize)
}

/// A standing figure's sway at `t` seconds, its own by `phase`: a lean (rad, about x and z) and a
/// breath (a scale on its height), so a crowd standing about never stands as one.
pub fn sway(t: f32, phase: f32) -> (f32, f32, f32) {
    let lean_x = 0.012 * (t * 0.7 + phase).sin();
    let lean_z = 0.018 * (t * 0.45 + phase * 1.7).sin();
    let breath = 1.0 + 0.006 * (t * 1.6 + phase * 2.3).sin();
    (lean_x, lean_z, breath)
}

/// A name's phase for [`sway`], 0..τ.
pub fn phase(name: &str) -> f32 {
    (hash(name) >> 8) as f32 / (1u32 << 24) as f32 * core::f32::consts::TAU
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traders_wear_looks_by_name_and_pilots_their_suits() {
        assert_eq!(look(1, "Vega"), None);
        assert_eq!(look(VISITOR_BASE - 1, "Vega"), None);
        let a = look(VISITOR_BASE, "Vega").unwrap();
        assert_eq!(look(VISITOR_BASE + 7, "Vega"), Some(a), "a name's look, whatever the slot");
        // Over a crowd, every look is worn.
        let worn: std::collections::HashSet<usize> =
            (0..200).filter_map(|i| look(VISITOR_BASE, &format!("trader {i}"))).collect();
        assert_eq!(worn.len(), LOOKS.len());
    }

    #[test]
    fn every_look_is_a_model_beside_the_page() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/assets");
        for l in LOOKS {
            let p = root.join(l.path);
            let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            assert_eq!(&bytes[..4], b"glTF", "{} is a glb", l.path);
            assert!(bytes.len() < 3_000_000, "{} is small enough to send", l.path);
        }
        assert!(root.join("avatars/LICENSE.md").exists());
    }

    #[test]
    fn a_sway_is_slight() {
        for i in 0..100 {
            let (x, z, b) = sway(i as f32 * 0.37, phase(&format!("n{i}")));
            assert!(x.abs() < 0.02 && z.abs() < 0.02 && (b - 1.0).abs() < 0.01);
        }
    }
}
