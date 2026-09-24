//! Turns numbers into the named buckets Jev reasons about well.

use bc_proto::{FrameId, Part, PilotKind};
use bc_sim::content::frame_name;
use bc_sim::zero::advice::ThreatBrief;
use bc_sim::zero::{N_HYP, hypotheses::Maneuver};
use serde_json::{Value, json};

use crate::TacticalPicture;

pub(crate) fn distance(d: f32) -> &'static str {
    match d {
        d if d < 200.0 => "knife range (under 200 m)",
        d if d < 1_000.0 => "close (under 1 km)",
        d if d < 3_000.0 => "medium (1 to 3 km)",
        _ => "long (over 3 km)",
    }
}

pub(crate) fn closing(v: f32) -> &'static str {
    match v {
        v if v > 150.0 => "closing fast",
        v if v > 30.0 => "closing",
        v if v > -30.0 => "holding range",
        _ => "opening range",
    }
}

pub(crate) fn bearing(deg: f32) -> &'static str {
    match deg {
        d if d < 15.0 => "dead ahead",
        d if d < 60.0 => "off the nose",
        d if d < 120.0 => "on the flank",
        _ => "behind",
    }
}

pub(crate) fn hull(f: f32) -> &'static str {
    match f {
        f if f > 0.7 => "intact",
        f if f > 0.3 => "damaged",
        _ => "critical",
    }
}

fn level(f: f32, names: [&'static str; 3]) -> &'static str {
    match f {
        f if f > 0.66 => names[2],
        f if f > 0.25 => names[1],
        _ => names[0],
    }
}

fn pilot(p: PilotKind) -> &'static str {
    match p {
        PilotKind::Human => "human pilot",
        PilotKind::Agent => "AI agent",
        PilotKind::MobileDoll => "Mobile Doll (unmanned, predictable, no G limit)",
    }
}

fn thrust(accel_g: f32) -> &'static str {
    match accel_g {
        g if g < 0.3 => "coasting",
        g if g < 2.0 => "light thrust",
        _ => "hard burn",
    }
}

fn damaged_parts(parts: &[f32; Part::COUNT]) -> Vec<String> {
    let names =
        ["head (sensors)", "torso", "left arm", "right arm (weapon)", "legs", "backpack (main thrusters)"];
    parts
        .iter()
        .zip(names)
        .filter_map(|(f, n)| match *f {
            f if f <= 0.0 => Some(format!("{n} destroyed")),
            f if f < 0.4 => Some(format!("{n} badly damaged")),
            _ => None,
        })
        .collect()
}

/// The local oracle's best guesses, as words ("most likely LEFT, then COAST").
fn local_guess(probs: &[f32; N_HYP]) -> String {
    let mut idx: Vec<usize> = (0..N_HYP).collect();
    idx.sort_by(|a, b| probs[*b].total_cmp(&probs[*a]));
    format!(
        "most likely {}, then {}",
        Maneuver::from_index(idx[0]).label(),
        Maneuver::from_index(idx[1]).label()
    )
}

pub(crate) fn threat_line(t: &ThreatBrief) -> String {
    format!(
        "{} flown by a {}, {}, {}, {}, hull {}{}{}",
        frame_name(t.frame),
        match t.pilot {
            PilotKind::MobileDoll => "Mobile Doll",
            PilotKind::Agent => "AI agent",
            PilotKind::Human => "human",
        },
        distance(t.distance),
        closing(t.closing_speed),
        bearing(t.bearing_deg),
        hull(t.hull),
        if t.firing { ", firing" } else { "" },
        if t.aiming_at_me { ", aiming at us" } else { "" },
    )
}

/// The `state` document for one pilot.
pub(crate) fn state(p: &TacticalPicture) -> Value {
    let frame = if p.frame == FrameId::WingZero {
        "Wing Gundam Zero (gundanium armour, ZERO System)"
    } else {
        frame_name(p.frame)
    };
    let threats: Vec<Value> = p
        .threats()
        .iter()
        .enumerate()
        .map(|(k, t)| {
            json!({
                "id": format!("t{}", k + 1),
                "frame": frame_name(t.frame),
                "pilot": pilot(t.pilot),
                "distance": distance(t.distance),
                "range_rate": closing(t.closing_speed),
                "bearing": bearing(t.bearing_deg),
                "hull": hull(t.hull),
                "firing": t.firing,
                "aiming_at_us": t.aiming_at_me,
                "maneuvering": thrust(t.accel_g),
                "tracker_estimate": local_guess(&t.local_probs),
            })
        })
        .collect();
    json!({
        "situation": "Zero-gravity mobile suit combat near an L1 space colony. Beams take a fraction of a second to arrive, so maneuvering matters.",
        "pilot": {
            "suit": frame,
            "hull": hull(p.parts[Part::Torso as usize]),
            "damage": damaged_parts(&p.parts),
            "heat": level(p.heat, ["cool", "warm", "near overheating"]),
            "energy": level(p.energy, ["almost empty", "partial", "full"]),
            "propellant": level(p.propellant, ["low", "half", "full"]),
            "g_strain": level(p.g_strain, ["none", "rising", "near blackout"]),
            "zero_strain": level(p.zero_strain, ["low", "high", "near seizure"]),
            "speed": match p.speed { s if s < 20.0 => "nearly stationary", s if s < 200.0 => "cruising", _ => "fast" },
        },
        "threats": threats,
    })
}
