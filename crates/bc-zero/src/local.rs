//! A picture-only reference oracle. It is useful for tests and for agents that have no simulation
//! (the in-sim local oracle has more information and is what normally runs).

use bc_sim::math::softmax;
use bc_sim::zero::N_HYP;
use bc_sim::zero::advice::{PICTURE_THREATS, THREAT_LEVELS};

use crate::{OracleError, TacticalAdvice, TacticalOracle, TacticalPicture};

#[derive(Clone, Copy, Debug, Default)]
pub struct LocalOracle;

impl LocalOracle {
    pub fn advise(p: &TacticalPicture) -> TacticalAdvice {
        let mut a = TacticalAdvice { pilot: p.pilot, computed_at_tick: p.tick, ..TacticalAdvice::default() };
        let threats = p.threats();
        let mut scores = [0.0f32; PICTURE_THREATS];
        for (k, t) in threats.iter().enumerate() {
            let mut s = 1.0 / (1.0 + t.distance / 1_200.0);
            if t.aiming_at_me {
                s *= 2.2;
            }
            if t.firing {
                s *= 1.5;
            }
            scores[k] = s * (0.3 + 0.7 * t.hull);
            a.target_slots[k] = t.slot;
        }
        let n = threats.len();
        a.n_targets = n as u8;
        let mut probs = scores;
        softmax(&mut probs[..n], 0.35);
        a.target_probs[..n].copy_from_slice(&probs[..n]);
        for (k, t) in threats.iter().enumerate().take(2) {
            a.maneuver_slots[k] = t.slot;
            a.maneuver_probs[k] = t.local_probs;
        }
        let total: f32 = scores[..n].iter().sum();
        let mut lv = [0.0f32; THREAT_LEVELS];
        for (k, c) in [0.1f32, 0.6, 1.4, 2.6].iter().enumerate() {
            lv[k] = -(total - c).abs();
        }
        softmax(&mut lv, 0.35);
        a.threat_probs = lv;
        a.flanked = threats.iter().filter(|t| t.bearing_deg > 60.0 && t.distance < 2_500.0).count().min(3)
            as f32
            / 3.0;
        a.own_action_probs = [0.0; N_HYP];
        a
    }
}

impl TacticalOracle for LocalOracle {
    fn name(&self) -> &'static str {
        "local"
    }
    async fn assess(&self, picture: &TacticalPicture) -> Result<TacticalAdvice, OracleError> {
        Ok(Self::advise(picture))
    }
}
