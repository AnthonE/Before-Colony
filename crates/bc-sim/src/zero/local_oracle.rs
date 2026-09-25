//! The built-in tactical oracle: fast utility scores turned into calibrated-looking distributions
//! with a softmax, and the same confidence statistic Jev reports. It runs inside the tick,
//! deterministically and without allocating, so the ZERO System works with no network at all.

use bc_proto::NO_SLOT;
use glam::Vec3;

use super::advice::THREAT_LEVELS;
use super::hypotheses::{self, Maneuver, N_HYP};
use crate::content::{FrameSpec, frame, weapon};
use crate::math::{angle_between, exp, length, normalize_or, softmax};
use crate::perception::{Contact, SelfView};

/// How dangerous a contact is to us right now.
pub fn threat_score(c: &Contact) -> f32 {
    if !c.hostile || c.hull <= 0.0 {
        return 0.0;
    }
    let proximity = 1.0 / (1.0 + c.dist / 1_200.0);
    let mut intent = 1.0;
    if c.aiming_at_me {
        intent += 1.2;
    }
    if c.locked_on_me {
        intent += 0.6;
    }
    if c.firing {
        intent += 0.5;
    }
    let lethality = frame(c.frame).ai.threat;
    proximity * intent * lethality * (0.3 + 0.7 * c.hull)
}

/// Distribution over up to four targets (threat plus opportunity). Returns how many were filled.
pub fn target_distribution(contacts: &[Contact], slots: &mut [u16; 4], probs: &mut [f32; 4]) -> usize {
    let mut best: [(f32, u16); 4] = [(f32::NEG_INFINITY, NO_SLOT); 4];
    for c in contacts.iter().filter(|c| c.hostile && c.hull > 0.0) {
        let score = threat_score(c) * 1.5 + (1.0 - c.hull) * 0.6 + 1.0 / (1.0 + c.dist / 2_000.0);
        let mut k = 3;
        if score <= best[k].0 {
            continue;
        }
        best[k] = (score, c.slot);
        while k > 0 && best[k].0 > best[k - 1].0 {
            best.swap(k, k - 1);
            k -= 1;
        }
    }
    let n = best.iter().take_while(|b| b.1 != NO_SLOT).count();
    let mut scores = [0.0f32; 4];
    for i in 0..n {
        slots[i] = best[i].1;
        scores[i] = best[i].0;
    }
    softmax(&mut scores[..n], 0.35);
    for i in 0..4 {
        probs[i] = if i < n { scores[i] } else { 0.0 };
        if i >= n {
            slots[i] = NO_SLOT;
        }
    }
    n
}

/// Likelihood of each hypothesis given an observed acceleration.
pub fn maneuver_likelihood(observed: Vec3, hyp_accel: &[Vec3; N_HYP], scale: f32) -> [f32; N_HYP] {
    let sigma = 0.35 * scale + 3.0;
    let mut out = [0.0f32; N_HYP];
    for (o, a) in out.iter_mut().zip(hyp_accel) {
        let d = observed - *a;
        *o = exp(-d.dot(d) / (2.0 * sigma * sigma)).max(1e-6);
    }
    out
}

/// Bayesian update: `posterior ∝ prior · likelihood`.
pub fn posterior(prior: &[f32; N_HYP], likelihood: &[f32; N_HYP]) -> [f32; N_HYP] {
    let mut out = [0.0f32; N_HYP];
    let mut sum = 0.0;
    for k in 0..N_HYP {
        out[k] = prior[k] * likelihood[k];
        sum += out[k];
    }
    if sum > 0.0 {
        for o in &mut out {
            *o /= sum;
        }
    } else {
        out = [1.0 / N_HYP as f32; N_HYP];
    }
    out
}

/// Accelerations of every hypothesis for a contact.
pub fn hypothesis_accels(c: &Contact) -> [Vec3; N_HYP] {
    let spec = frame(c.frame);
    let mut out = [Vec3::ZERO; N_HYP];
    for (k, o) in out.iter_mut().enumerate() {
        *o = hypotheses::accel(spec, c.rot, Maneuver::from_index(k));
    }
    out
}

/// Which way should *we* break? Scores each own maneuver by how far it carries us off each
/// threat's linear lead by the time its shot arrives (lateral displacement only: running straight
/// at or away from a shooter doesn't make it miss).
pub fn own_maneuver_distribution(me: &SelfView, spec: &FrameSpec, contacts: &[Contact]) -> [f32; N_HYP] {
    let mut utility = [0.0f32; N_HYP];
    let mut any = false;
    for c in contacts.iter().filter(|c| c.hostile && c.hull > 0.0) {
        let w = threat_score(c);
        if w <= 0.01 {
            continue;
        }
        any = true;
        let speed = frame(c.frame).loadout[0].map_or(4_000.0, |m| weapon(m.weapon).speed);
        let t = c.dist / speed.max(1.0);
        let los = normalize_or(me.pos - c.pos, Vec3::Z);
        for (k, u) in utility.iter_mut().enumerate() {
            let a = hypotheses::accel(spec, me.rot, Maneuver::from_index(k));
            let disp = a * (0.5 * t * t);
            let lateral = disp - los * disp.dot(los);
            let miss = length(lateral);
            // Danger falls off once we're a suit-width or two off their lead.
            *u -= w * exp(-(miss / 8.0) * (miss / 8.0));
        }
    }
    if !any {
        // Nothing shooting at us: hold, or push toward the fight.
        utility = [0.3, 0.2, -0.2, -0.1, -0.1, -0.1, -0.1];
    }
    for (k, u) in utility.iter_mut().enumerate() {
        if k != Maneuver::Coast as usize {
            *u -= 0.05; // propellant
        }
    }
    softmax(&mut utility, 0.25);
    utility
}

/// Soft threat-level distribution (low, moderate, high, lethal).
pub fn threat_levels(contacts: &[Contact]) -> [f32; THREAT_LEVELS] {
    let total: f32 = contacts.iter().map(threat_score).sum();
    const CENTERS: [f32; THREAT_LEVELS] = [0.1, 0.6, 1.4, 2.6];
    let mut s = [0.0f32; THREAT_LEVELS];
    for (o, c) in s.iter_mut().zip(CENTERS) {
        *o = -(total - c).abs();
    }
    softmax(&mut s, 0.35);
    s
}

/// Probability that we're being flanked: hostiles close by and outside our forward arc.
pub fn flanked(me: &SelfView, contacts: &[Contact]) -> f32 {
    let fwd = me.forward();
    let mut w = 0.0;
    for c in contacts.iter().filter(|c| c.hostile && c.dist < 2_500.0) {
        let off = angle_between(fwd, normalize_or(c.pos - me.pos, fwd));
        if off > 60f32.to_radians() {
            w += 1.0 / (1.0 + c.dist / 1_000.0);
        }
    }
    1.0 - exp(-0.8 * w)
}
