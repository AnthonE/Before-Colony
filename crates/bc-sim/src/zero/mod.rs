//! The ZERO System: predicts the future of a fight and advises (and, past its limit, overrides)
//! its pilot.
//!
//! - [`hypotheses`]: the maneuvers a threat can make next (thrust along each body axis, or coast).
//! - [`rollout`]: forward-simulates each hypothesis 1.5 s ahead. The browser runs the same code to
//!   draw the ghost trails, so only probabilities travel on the wire.
//! - [`fire_control`]: intercept solutions and the hit probability across hypotheses.
//! - [`local_oracle`]: the built-in tactical oracle (softmax over utilities, Jev-style confidence).
//! - [`advice`]: the typed picture/advice exchanged with external oracles such as TypeSafe Jev.
//! - [`strain`]: ZERO strain, seizure and lockout.

pub mod advice;
pub mod fire_control;
pub mod hypotheses;
pub mod local_oracle;
pub mod rollout;
pub mod strain;

use bc_proto::NO_SLOT;
use glam::Vec3;

pub use advice::{ADVICE_TTL, TacticalAdvice, TacticalPicture, ThreatBrief};
pub use hypotheses::{Maneuver, N_HYP};
pub use strain::ZeroState;

/// Threats with full predictions per ZERO pilot.
pub const ZERO_THREATS: usize = 2;

/// Chance a suit is still flying the same maneuver over the prediction horizon. Used to turn the
/// filtered "what is it doing now" posterior into a predictive "what will it do next" distribution,
/// which is what the pilot is shown (and what calibration is measured against).
pub const PERSISTENCE: f32 = 0.75;

/// One threat's maneuver distribution.
#[derive(Clone, Copy, Debug)]
pub struct ThreatTrack {
    pub slot: u16,
    /// Predictive distribution over its next maneuver (shown to the pilot).
    pub probs: [f32; N_HYP],
    /// Filtered posterior over its current maneuver (carried to the next update).
    pub post: [f32; N_HYP],
}

impl Default for ThreatTrack {
    fn default() -> Self {
        Self { slot: NO_SLOT, probs: [1.0 / N_HYP as f32; N_HYP], post: [1.0 / N_HYP as f32; N_HYP] }
    }
}

/// Predictive distribution from a filtered posterior under the persistence model.
pub fn predictive(post: &[f32; N_HYP]) -> [f32; N_HYP] {
    let mut out = [0.0; N_HYP];
    for (o, p) in out.iter_mut().zip(post) {
        *o = PERSISTENCE * p + (1.0 - PERSISTENCE) / N_HYP as f32;
    }
    out
}

/// The ZERO System's latest conclusions for its pilot.
#[derive(Clone, Copy, Debug)]
pub struct ZeroOut {
    pub computed_at: u32,
    pub n_threats: u8,
    pub threats: [ThreatTrack; ZERO_THREATS],
    pub rec_target: u16,
    pub rec_target_p: f32,
    /// Recommended own maneuver ([`Maneuver`] index).
    pub rec_maneuver: u8,
    pub rec_maneuver_p: f32,
    /// 0 low .. 3 lethal.
    pub threat_level: u8,
    pub threat_confidence: f32,
    pub flanked: f32,
    pub has_solution: bool,
    pub solution: Vec3,
    pub solution_target: u16,
    pub hit_p: f32,
    pub source_jev: bool,
    pub advice_age: u8,
}

impl Default for ZeroOut {
    fn default() -> Self {
        Self {
            computed_at: 0,
            n_threats: 0,
            threats: [ThreatTrack::default(); ZERO_THREATS],
            rec_target: NO_SLOT,
            rec_target_p: 0.0,
            rec_maneuver: 0,
            rec_maneuver_p: 0.0,
            threat_level: 0,
            threat_confidence: 0.0,
            flanked: 0.0,
            has_solution: false,
            solution: Vec3::Z,
            solution_target: NO_SLOT,
            hit_p: 0.0,
            source_jev: false,
            advice_age: 0,
        }
    }
}
