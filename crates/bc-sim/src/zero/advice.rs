//! The typed contract between the simulation and an external tactical oracle (TypeSafe Jev).
//!
//! The sector sends a [`TacticalPicture`] off the hot path and later receives a [`TacticalAdvice`]:
//! plain fixed-size data both ways, so it crosses the lock-free rings without allocating. Advice
//! expires [`ADVICE_TTL`] ticks after it was computed; the local oracle always has an answer.

use bc_proto::{FrameId, NO_SLOT, Part, PilotKind};

use super::N_HYP;

/// Threats described per picture.
pub const PICTURE_THREATS: usize = 4;
/// Advice older than this many ticks is ignored.
pub const ADVICE_TTL: u32 = 15;
/// Threat-level buckets: low, moderate, high, lethal.
pub const THREAT_LEVELS: usize = 4;
/// Weight given to external advice when blending with the local oracle.
pub const EXTERNAL_WEIGHT: f32 = 0.5;

#[derive(Clone, Copy, Debug)]
pub struct ThreatBrief {
    pub slot: u16,
    pub frame: FrameId,
    pub pilot: PilotKind,
    pub distance: f32,
    /// Positive when closing, m/s.
    pub closing_speed: f32,
    /// Angle off our nose, degrees.
    pub bearing_deg: f32,
    pub hull: f32,
    pub firing: bool,
    pub aiming_at_me: bool,
    /// Observed acceleration, g.
    pub accel_g: f32,
    /// The local oracle's distribution over its next maneuver.
    pub local_probs: [f32; N_HYP],
}

impl Default for ThreatBrief {
    fn default() -> Self {
        Self {
            slot: NO_SLOT,
            frame: FrameId::Leo,
            pilot: PilotKind::MobileDoll,
            distance: 0.0,
            closing_speed: 0.0,
            bearing_deg: 0.0,
            hull: 1.0,
            firing: false,
            aiming_at_me: false,
            accel_g: 0.0,
            local_probs: [1.0 / N_HYP as f32; N_HYP],
        }
    }
}

/// What the oracle is told about one pilot's situation.
#[derive(Clone, Copy, Debug)]
pub struct TacticalPicture {
    pub pilot: u16,
    pub tick: u32,
    pub frame: FrameId,
    pub parts: [f32; Part::COUNT],
    pub heat: f32,
    pub energy: f32,
    pub propellant: f32,
    pub g_strain: f32,
    pub zero_strain: f32,
    pub speed: f32,
    pub n: u8,
    pub threats: [ThreatBrief; PICTURE_THREATS],
}

impl Default for TacticalPicture {
    fn default() -> Self {
        Self {
            pilot: NO_SLOT,
            tick: 0,
            frame: FrameId::Leo,
            parts: [1.0; Part::COUNT],
            heat: 0.0,
            energy: 1.0,
            propellant: 1.0,
            g_strain: 0.0,
            zero_strain: 0.0,
            speed: 0.0,
            n: 0,
            threats: [ThreatBrief::default(); PICTURE_THREATS],
        }
    }
}

impl TacticalPicture {
    pub fn threats(&self) -> &[ThreatBrief] {
        &self.threats[..self.n as usize]
    }
}

/// Typed answers with calibrated probabilities.
#[derive(Clone, Copy, Debug)]
pub struct TacticalAdvice {
    pub pilot: u16,
    /// The picture's tick (advice ages from there).
    pub computed_at_tick: u32,
    pub n_targets: u8,
    pub target_slots: [u16; PICTURE_THREATS],
    pub target_probs: [f32; PICTURE_THREATS],
    /// Next-maneuver distributions for up to two threats.
    pub maneuver_slots: [u16; 2],
    pub maneuver_probs: [[f32; N_HYP]; 2],
    /// Recommended own maneuver distribution (all zero = no opinion).
    pub own_action_probs: [f32; N_HYP],
    pub threat_probs: [f32; THREAT_LEVELS],
    pub flanked: f32,
    /// True when it came from Jev (shown on the HUD).
    pub from_jev: bool,
}

impl Default for TacticalAdvice {
    fn default() -> Self {
        Self {
            pilot: NO_SLOT,
            computed_at_tick: 0,
            n_targets: 0,
            target_slots: [NO_SLOT; PICTURE_THREATS],
            target_probs: [0.0; PICTURE_THREATS],
            maneuver_slots: [NO_SLOT; 2],
            maneuver_probs: [[0.0; N_HYP]; 2],
            own_action_probs: [0.0; N_HYP],
            threat_probs: [0.0; THREAT_LEVELS],
            flanked: 0.0,
            from_jev: false,
        }
    }
}

impl TacticalAdvice {
    pub fn fresh(&self, tick: u32) -> bool {
        self.pilot != NO_SLOT && tick <= self.computed_at_tick + ADVICE_TTL
    }

    /// Maneuver distribution the advice holds for `slot`, if any.
    pub fn maneuvers_for(&self, slot: u16) -> Option<&[f32; N_HYP]> {
        (0..2)
            .find(|&k| self.maneuver_slots[k] == slot && self.maneuver_probs[k].iter().sum::<f32>() > 0.5)
            .map(|k| &self.maneuver_probs[k])
    }
}

/// Geometric blend of two distributions: `p ∝ local^(1−w) · external^w`, renormalized in place.
pub fn blend(local: &mut [f32], external: &[f32], w: f32) {
    let mut sum = 0.0;
    for (l, e) in local.iter_mut().zip(external) {
        let v = libm::powf(l.max(1e-4), 1.0 - w) * libm::powf(e.max(1e-4), w);
        *l = v;
        sum += v;
    }
    if sum > 0.0 {
        for l in local.iter_mut() {
            *l /= sum;
        }
    }
}
