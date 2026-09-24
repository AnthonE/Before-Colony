//! The batched questions for one pilot.

use bc_sim::zero::N_HYP;
use bc_sim::zero::hypotheses::Maneuver;
use serde_json::{Map, Value, json};

use super::state;
use crate::TacticalPicture;

/// Choice labels for a threat's next maneuver, in hypothesis order.
pub const MANEUVER_LABELS: [&str; N_HYP] = ["COAST", "FWD", "BACK", "LEFT", "RIGHT", "UP", "DOWN"];
/// Choice labels for the pilot's own evasive action, in hypothesis order.
pub const OWN_LABELS: [&str; N_HYP] =
    ["HOLD", "PUSH", "BREAK-BACK", "BREAK-LEFT", "BREAK-RIGHT", "BREAK-HIGH", "BREAK-LOW"];
pub const THREAT_LEVEL_LABELS: [&str; 4] = ["low", "moderate", "high", "lethal"];

fn maneuver_criteria() -> Value {
    let mut m = Map::new();
    for (k, label) in MANEUVER_LABELS.iter().enumerate() {
        let desc = match Maneuver::from_index(k) {
            Maneuver::Coast => "stops thrusting and drifts",
            Maneuver::Forward => "burns toward its own nose",
            Maneuver::Back => "burns backward (retro)",
            Maneuver::Left => "burns to its own left",
            Maneuver::Right => "burns to its own right",
            Maneuver::Up => "burns toward its own head",
            Maneuver::Down => "burns toward its own feet",
        };
        m.insert((*label).into(), Value::String(desc.into()));
    }
    Value::Object(m)
}

fn own_criteria() -> Value {
    json!({
        "HOLD": "keep the current course and aim steady",
        "PUSH": "burn forward, toward the fight",
        "BREAK-BACK": "retro-burn away",
        "BREAK-LEFT": "break hard to the left",
        "BREAK-RIGHT": "break hard to the right",
        "BREAK-HIGH": "break hard upward",
        "BREAK-LOW": "break hard downward",
    })
}

/// The full `POST /v1/systemone` body.
pub fn build_request(p: &TacticalPicture, model: &str) -> Value {
    let mut q = Map::new();
    let threats = p.threats();
    if !threats.is_empty() {
        let mut crit = Map::new();
        for (k, t) in threats.iter().enumerate() {
            crit.insert(format!("t{}", k + 1), Value::String(state::threat_line(t)));
        }
        q.insert(
            "target".into(),
            json!({ "type": "choice", "instructions": "Which threat should the pilot engage first?", "criteria": crit }),
        );
        for (k, _) in threats.iter().enumerate().take(2) {
            q.insert(
                format!("next_t{}", k + 1),
                json!({
                    "type": "choice",
                    "instructions": format!("What will threat t{} most likely do during the next second?", k + 1),
                    "criteria": maneuver_criteria(),
                }),
            );
        }
        q.insert(
            "evade".into(),
            json!({ "type": "choice", "instructions": "Which way should the pilot move to spoil the enemies' aim?", "criteria": own_criteria() }),
        );
    }
    q.insert(
        "threat".into(),
        json!({ "type": "score", "instructions": "How dangerous is the pilot's situation right now?", "criteria": THREAT_LEVEL_LABELS }),
    );
    q.insert(
        "flanked".into(),
        json!({ "type": "noul", "instructions": "An enemy is attacking the pilot from outside their forward field of view." }),
    );
    json!({ "model": model, "state": state::state(p), "questions": q })
}
