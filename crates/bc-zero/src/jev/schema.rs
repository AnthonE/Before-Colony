//! Tolerant parsing of Jev answers into [`TacticalAdvice`].
//!
//! - choice answers: `{ "choice": label, "confidence": c, "probabilities": { label: p } }`
//! - score answers: `probabilities` keyed by level index as a string ("0".."3")
//! - noul answers: `{ "noul": p }` only (no confidence)
//!
//! Missing answers mean "no opinion" (zeros), which the blend ignores.

use bc_sim::zero::N_HYP;
use serde_json::Value;

use super::questions::{MANEUVER_LABELS, OWN_LABELS};
use crate::{OracleError, TacticalAdvice, TacticalPicture};

fn probs(answer: Option<&Value>, labels: &[&str]) -> Option<Vec<f32>> {
    let p = answer?.get("probabilities")?.as_object()?;
    let mut out: Vec<f32> =
        labels.iter().map(|l| p.get(*l).and_then(Value::as_f64).unwrap_or(0.0) as f32).collect();
    let sum: f32 = out.iter().sum();
    if sum <= 0.0 || !sum.is_finite() {
        return None;
    }
    for o in &mut out {
        *o = (*o / sum).clamp(0.0, 1.0);
    }
    Some(out)
}

pub fn parse_advice(p: &TacticalPicture, json: &Value) -> Result<TacticalAdvice, OracleError> {
    let answers = json
        .get("answers")
        .and_then(Value::as_object)
        .ok_or_else(|| OracleError::Malformed("no answers".into()))?;
    let mut a = TacticalAdvice {
        pilot: p.pilot,
        computed_at_tick: p.tick,
        from_jev: true,
        ..TacticalAdvice::default()
    };
    let threats = p.threats();
    let labels: Vec<String> = (1..=threats.len()).map(|k| format!("t{k}")).collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    if let Some(tp) = probs(answers.get("target"), &label_refs) {
        a.n_targets = threats.len() as u8;
        for (k, t) in threats.iter().enumerate() {
            a.target_slots[k] = t.slot;
            a.target_probs[k] = tp[k];
        }
    }
    for (k, t) in threats.iter().enumerate().take(2) {
        if let Some(mp) = probs(answers.get(&format!("next_t{}", k + 1)), &MANEUVER_LABELS) {
            a.maneuver_slots[k] = t.slot;
            a.maneuver_probs[k].copy_from_slice(&mp[..N_HYP]);
        }
    }
    if let Some(op) = probs(answers.get("evade"), &OWN_LABELS) {
        a.own_action_probs.copy_from_slice(&op[..N_HYP]);
    }
    if let Some(sp) = probs(answers.get("threat"), &["0", "1", "2", "3"]) {
        a.threat_probs.copy_from_slice(&sp[..4]);
    }
    if let Some(n) = answers.get("flanked").and_then(|f| f.get("noul")).and_then(Value::as_f64) {
        a.flanked = (n as f32).clamp(0.0, 1.0);
    }
    Ok(a)
}
