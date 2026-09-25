//! Calls the real TypeSafe Jev API with a sample tactical picture and prints the advice.
//!
//!   TYPESAFE_API_KEY=... cargo run -p bc-zero --example jev_smoke

use bc_proto::{FrameId, PilotKind};
use bc_sim::zero::advice::ThreatBrief;
use bc_zero::jev::{MANEUVER_LABELS, OWN_LABELS, THREAT_LEVEL_LABELS, build_request};
use bc_zero::{JevOracle, TacticalOracle, TacticalPicture};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let key = std::env::var("TYPESAFE_API_KEY").map_err(|_| anyhow::anyhow!("set TYPESAFE_API_KEY"))?;
    let mut p = TacticalPicture {
        pilot: 1,
        tick: 1,
        frame: FrameId::WingZero,
        speed: 120.0,
        ..TacticalPicture::default()
    };
    p.threats[0] = ThreatBrief {
        slot: 7,
        frame: FrameId::Taurus,
        pilot: PilotKind::MobileDoll,
        distance: 900.0,
        closing_speed: 160.0,
        bearing_deg: 10.0,
        firing: true,
        aiming_at_me: true,
        accel_g: 3.2,
        ..ThreatBrief::default()
    };
    p.threats[1] = ThreatBrief {
        slot: 8,
        frame: FrameId::Virgo,
        pilot: PilotKind::MobileDoll,
        distance: 2_200.0,
        bearing_deg: 110.0,
        ..ThreatBrief::default()
    };
    p.n = 2;
    println!("request:\n{}", serde_json::to_string_pretty(&build_request(&p, "jev-latest"))?);
    let started = std::time::Instant::now();
    let a = JevOracle::new(key).assess(&p).await?;
    println!("answered in {:?}", started.elapsed());
    println!(
        "target probabilities: {:?} for slots {:?}",
        &a.target_probs[..a.n_targets as usize],
        &a.target_slots[..a.n_targets as usize]
    );
    for (k, label) in MANEUVER_LABELS.iter().enumerate() {
        println!("  threat 1 next {label:>5}: {:.2}", a.maneuver_probs[0][k]);
    }
    for (k, label) in OWN_LABELS.iter().enumerate() {
        println!("  evade {label:>11}: {:.2}", a.own_action_probs[k]);
    }
    for (k, label) in THREAT_LEVEL_LABELS.iter().enumerate() {
        println!("  threat level {label:>8}: {:.2}", a.threat_probs[k]);
    }
    println!("  flanked: {:.2}", a.flanked);
    Ok(())
}
