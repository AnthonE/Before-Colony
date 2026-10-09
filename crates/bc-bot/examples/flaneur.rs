//! An agent on foot in the colony (survival rules, the colony open): it rides the cap lift down to
//! a strip's Hub Gate and strolls up and down the avenue in front of it, so a pilot coming down
//! finds someone there; or (`--sit`) walks over to The Arrival and sits on one of its seats out
//! front. It walks with the browser's own legs (`bc_client_core::walker`) among the city's walls
//! (`bc_client_core::city`), so the server takes every step.
//!
//!   cargo run -p bc-bot --release --example flaneur -- --server http://127.0.0.1:8080 --name Flaneur-01
//!   cargo run -p bc-bot --release --example flaneur -- --name Flaneur-02 --sit

use std::time::{Duration, Instant};

use bc_bot::{BotClient, BotConfig};
use bc_client_core::city::{CityGround, pose_of};
use bc_client_core::city_nav;
use bc_client_core::walker::{Guide, Stride, Walker};
use bc_proto::presence::{PersonPose, RIDE_SEATED};
use bc_proto::{Faction, FrameId};
use bc_sim::colony::city::{ARRIVAL_SEATS, Seat, Stage, arrival_seats, place_door};
use bc_sim::colony::frame::CityPos;
use bc_sim::content::city::{PLACES, PlaceKind};
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    server: String,
    #[arg(long, default_value = "Flaneur-01")]
    name: String,
    /// The land strip to go down to (0 Charter, 1 Canal, 2 Gardens).
    #[arg(long, default_value_t = 0)]
    strip: u8,
    /// How far up the avenue from Hub Gate's door it strolls, m.
    #[arg(long, default_value_t = 60.0)]
    reach: f32,
    /// Seconds to stay (0 = until interrupted).
    #[arg(long, default_value_t = 0)]
    secs: u64,
    /// Walk to The Arrival and sit on a free seat out front (on strip 0) instead of strolling.
    #[arg(long)]
    sit: bool,
    /// Which of its seats to try first (agents started together each take their own).
    #[arg(long, default_value_t = 0)]
    seat: usize,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .with_target(false)
        .init();
    let a = Args::parse();
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
    bot.wait_until(10.0, "the hangar", |c| c.hangar.in_hangar()).await?;
    anyhow::ensure!(
        bot.core.welcome.as_ref().is_some_and(|w| w.colony),
        "the colony is closed (--no-colony)"
    );
    let strip = a.strip % 3;
    bot.enter_city(strip).await?;
    let gate = PLACES.iter().find(|p| p.kind == PlaceKind::HubGate && p.strip == strip).expect("a Hub Gate");
    let ((s, x), (ds, dx)) = place_door(gate);
    // Out of the door and a few metres to one side, facing up the avenue.
    let start = CityPos::new(strip, x - dx * 6.0, s - ds * 6.0 + 4.0, 0.0);
    let along = CityPos::new(strip, start.x + 1.0, start.s, 0.0).walker() - start.walker();
    let mut walker = Walker::at(start.walker(), along.normalize());
    let ground = CityGround { strip, stage: Stage(0) };
    if a.sit {
        return sit(bot, walker, &ground, a.seat, a.secs).await;
    }
    tracing::info!(name = %a.name, strip, "down in the colony, strolling");
    let started = Instant::now();
    let mut last = Instant::now();
    let mut last_report = started;
    loop {
        let dt = last.elapsed().as_secs_f32().min(0.25);
        last = Instant::now();
        // Up the avenue to `reach`, then back down to the door, and again.
        let at = CityPos::from_walker(strip, walker.feet);
        let out = walker.heading().dot(along.normalize()) > 0.0;
        if (out && at.x > start.x + a.reach) || (!out && at.x < start.x) {
            walker.turn(std::f32::consts::PI, 0.0);
        }
        walker.step(&ground, &Stride { forward: 1.0, ..Stride::default() }, dt);
        bot.set_pose(pose_of(strip, &walker));
        bot.step(&mut |_| bc_proto::InputCmd::default()).await?;
        if last_report.elapsed() > Duration::from_secs(10) {
            last_report = Instant::now();
            let people: Vec<String> = bot.people().into_iter().map(|(_, n, _)| n).collect();
            tracing::info!(x = at.x as i32, ?people, "strolling");
        }
        if a.secs > 0 && started.elapsed() > Duration::from_secs(a.secs) {
            break;
        }
    }
    bot.leave_city().await?;
    bot.close().await;
    Ok(())
}

/// Walks to a free seat out in front of The Arrival, sits on it until `secs` are up (0: until
/// interrupted), then stands and goes back up.
async fn sit(
    mut bot: BotClient,
    mut walker: Walker,
    ground: &CityGround,
    first: usize,
    secs: u64,
) -> anyhow::Result<()> {
    let strip = ground.strip;
    let started = Instant::now();
    // Someone already sitting there: the next seat along.
    let taken = |bot: &BotClient, t: &Seat| {
        bot.people().iter().any(|(_, _, p)| p.seated() && (p.s - t.s).hypot(p.x - t.x) < 0.5)
    };
    let mut seats = arrival_seats();
    seats.rotate_left(first % ARRIVAL_SEATS);
    let Some(seat) = seats.iter().find(|t| t.strip == strip && !taken(&bot, t)).copied() else {
        anyhow::bail!("no free seat at The Arrival on strip {strip}");
    };
    let at = CityPos::from_walker(strip, walker.feet);
    let mut guide = Guide::new(city_nav::route(strip, (at.s, at.x), (seat.s, seat.x)), None);
    tracing::info!(strip, "walking over to The Arrival, to sit");
    let mut last = Instant::now();
    while !guide.arrived() {
        let dt = last.elapsed().as_secs_f32().min(0.25);
        last = Instant::now();
        let stride = guide.steer(&mut walker, dt);
        walker.step(ground, &stride, dt);
        bot.set_pose(pose_of(strip, &walker));
        bot.step(&mut |_| bc_proto::InputCmd::default()).await?;
        anyhow::ensure!(started.elapsed() < Duration::from_secs(600), "never got to the seat");
    }
    let seated = PersonPose {
        s: seat.s,
        x: seat.x,
        yaw: seat.yaw,
        speed: 0.0,
        running: false,
        ride: RIDE_SEATED,
        ..pose_of(strip, &walker)
    };
    tracing::info!(strip, "sitting outside The Arrival");
    while secs == 0 || started.elapsed() < Duration::from_secs(secs) {
        bot.set_pose(seated);
        bot.step(&mut |_| bc_proto::InputCmd::default()).await?;
    }
    bot.set_pose(pose_of(strip, &walker));
    bot.step(&mut |_| bc_proto::InputCmd::default()).await?;
    bot.leave_city().await?;
    bot.close().await;
    Ok(())
}
