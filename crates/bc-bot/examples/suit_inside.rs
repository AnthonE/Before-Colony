//! An agent in a suit inside the colony (survival rules, the colony open): it launches its suit
//! from the bay into the colony by the inner gate and flies it down to `--up` metres over a place
//! (`--over`, Hub Gate by default: `--ahead` metres out from its door); with `--land`, it then
//! comes down onto the street there, its grip armed, and stands. Every few seconds it says who it
//! sees below. For the e2e suites: a suit inside for a pilot on foot to watch.
//!
//!   cargo run -p bc-bot --release --example suit_inside -- --server http://127.0.0.1:8080 --land

use std::time::{Duration, Instant};

use bc_bot::{BotClient, BotConfig};
use bc_proto::buttons::{FLIGHT_ASSIST, GRIP};
use bc_proto::snapshot::{OwnState, footing};
use bc_proto::{BodyRef, Faction, FrameId, InputCmd};
use bc_sim::colony::city::{place, place_door};
use bc_sim::colony::frame::CityPos;
use clap::Parser;
use glam::Vec3;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    server: String,
    #[arg(long, default_value = "Wufei")]
    name: String,
    /// The place to fly over, by its slug.
    #[arg(long, default_value = "hub_gate_1")]
    over: String,
    /// How far out from its door, m.
    #[arg(long, default_value_t = 60.0)]
    ahead: f32,
    /// How high over it, m.
    #[arg(long, default_value_t = 250.0)]
    up: f32,
    /// Then come down onto the street there, the grip armed, and stand.
    #[arg(long)]
    land: bool,
    /// Seconds to stay (0 = until interrupted).
    #[arg(long, default_value_t = 0)]
    secs: u64,
}

/// A command flying the suit toward `to` on flight assist, no faster than `top` m/s.
fn toward(own: &OwnState, to: Vec3, top: f32, buttons: u16) -> InputCmd {
    let d = to - own.pos;
    let want = d.normalize_or_zero() * (d.length() * 0.3).min(top);
    let local = own.rot.conjugate() * (want - own.vel);
    let q = |v: f32| (v * 6.0).clamp(-127.0, 127.0) as i8;
    InputCmd {
        buttons: FLIGHT_ASSIST | buttons,
        thrust: [q(local.x), q(local.y), q(local.z)],
        aim: Vec3::X,
        ..InputCmd::default()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .with_target(false)
        .init();
    let a = Args::parse();
    let (_, p) = place(&a.over).ok_or_else(|| anyhow::anyhow!("no place {}", a.over))?;
    let ((s, x), (ds, dx)) = place_door(p);
    let spot = |h: f32| CityPos::new(p.strip, x - dx * a.ahead, s - ds * a.ahead, h).to_colony();
    let cfg =
        BotConfig { server: a.server, name: a.name.clone(), frame: FrameId::Leo, faction: Faction::Colonies };
    let mut bot = loop {
        match BotClient::connect(&cfg).await {
            Ok(b) => break b,
            Err(e) => {
                tracing::warn!("connect failed: {e:#}; retrying");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    };
    anyhow::ensure!(bot.survival(), "the colony's inside is survival's");
    bot.wait_until(10.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    anyhow::ensure!(bot.core.welcome.as_ref().is_some_and(|w| w.colony), "the colony isn't open (--colony)");
    bot.launch_inside().await?;
    tracing::info!(name = %a.name, over = %a.over, "inside the colony, flying down");

    let started = Instant::now();
    let mut said = Instant::now();
    let mut there = false;
    loop {
        if a.secs > 0 && started.elapsed() > Duration::from_secs(a.secs) {
            break;
        }
        let Some(own) = bot.world().own else {
            bot.step(&mut |_| InputCmd::default()).await?;
            continue;
        };
        let standing = own.surface.is_some_and(|s| s.footing == footing::GROUNDED && s.body == BodyRef::City);
        let over = spot(a.up);
        if !there && own.pos.distance(over) < 8.0 && own.vel.length() < 2.0 {
            there = true;
            tracing::info!(up = a.up, "over {}", a.over);
        }
        let cmd = match (there, a.land) {
            // On its way down from the gate.
            (false, _) => toward(&own, over, 150.0, 0),
            // Down toward the street, the grip armed: it catches the suit within 25 m of the
            // ground, and then, hands off, brings it down onto its feet, where it stands.
            (true, true) if own.surface.is_some() => {
                InputCmd { buttons: FLIGHT_ASSIST | GRIP, aim: Vec3::X, ..InputCmd::default() }
            }
            (true, true) => toward(&own, spot(15.0), 30.0, GRIP),
            (true, false) => toward(&own, over, 150.0, 0),
        };
        bot.step(&mut |_| cmd).await?;
        if said.elapsed() > Duration::from_secs(3) {
            said = Instant::now();
            let names: Vec<String> = bot.people().into_iter().map(|p| p.1).collect();
            let footing = own.surface.map_or(footing::FREE, |s| s.footing);
            tracing::info!(standing, footing, pos = %own.pos, "sees [{}]", names.join(", "));
        }
    }
    bot.close().await;
    Ok(())
}
