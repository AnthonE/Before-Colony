//! Load test: many agents at once.
//!
//!   cargo run -p bc-bot --release --bin bc-swarm -- --bots 64 --secs 60
//!
//! With `--walkers N` (survival, the colony open), N more agents go down into the colony and
//! stroll the avenues by Hub Gate, spread over its three strips, sending their poses as a browser
//! does: the plaza's load.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bc_bot::{BotClient, BotConfig, DollBrain, Stroll};
use bc_proto::{Faction, FrameId, InputCmd};
use bc_sim::content::PLAYABLE_ORDER;
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    server: String,
    #[arg(long, default_value_t = 16)]
    bots: usize,
    #[arg(long, default_value_t = 30)]
    secs: u64,
    /// Agents on foot in the colony's city as well.
    #[arg(long, default_value_t = 0)]
    walkers: usize,
}

#[derive(Default)]
struct Totals {
    connected: AtomicU64,
    snapshots: AtomicU64,
    bytes: AtomicU64,
    max_snapshot: AtomicU64,
    hits: AtomicU64,
    kills: AtomicU64,
    errors: AtomicU64,
    pred_err_mm_max: AtomicU64,
    walking: AtomicU64,
    /// The most people one walker saw at once.
    seen_max: AtomicU64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let a = Args::parse();
    let totals = Arc::new(Totals::default());
    let mut tasks = Vec::new();
    for k in 0..a.bots {
        let totals = totals.clone();
        let server = a.server.clone();
        let secs = a.secs;
        tasks.push(tokio::spawn(async move {
            let cfg = BotConfig {
                server,
                name: format!("Swarm-{k:03}"),
                // Every frame pilots can fly, in turn.
                frame: PLAYABLE_ORDER[k % PLAYABLE_ORDER.len()],
                faction: if k % 2 == 0 { Faction::Colonies } else { Faction::Alliance },
            };
            let mut bot = match BotClient::connect(&cfg).await {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("bot {k}: {e:#}");
                    totals.errors.fetch_add(1, Ordering::Relaxed);
                    return;
                }
            };
            // Under survival rules they fly the starter Leo out of their hangars (run the server
            // with --rules arcade to fly every frame).
            if let Err(e) = bot.sortie().await {
                eprintln!("bot {k}: {e:#}");
                totals.errors.fetch_add(1, Ordering::Relaxed);
                return;
            }
            totals.connected.fetch_add(1, Ordering::Relaxed);
            let mut brain = DollBrain::new(k as u32 * 7919 + 1);
            let mut worst = 0.0f32;
            let end = std::time::Instant::now() + Duration::from_secs(secs);
            while std::time::Instant::now() < end {
                if bot.step(&mut |ctx| brain.decide(ctx)).await.is_err() {
                    totals.errors.fetch_add(1, Ordering::Relaxed);
                    break;
                }
                if bot.core.stats.snapshots > 60 {
                    worst = worst.max(bot.core.stats.prediction_error);
                }
            }
            let s = bot.core.stats;
            totals.snapshots.fetch_add(s.snapshots, Ordering::Relaxed);
            totals.bytes.fetch_add(s.bytes, Ordering::Relaxed);
            totals.max_snapshot.fetch_max(s.max_snapshot as u64, Ordering::Relaxed);
            totals.hits.fetch_add(u64::from(bot.world().my_hits), Ordering::Relaxed);
            totals.kills.fetch_add(u64::from(bot.world().my_kills), Ordering::Relaxed);
            totals.pred_err_mm_max.fetch_max((worst * 1000.0) as u64, Ordering::Relaxed);
            bot.close().await;
        }));
    }
    for k in 0..a.walkers {
        let totals = totals.clone();
        let server = a.server.clone();
        let secs = a.secs;
        tasks.push(tokio::spawn(async move {
            if let Err(e) = walk(k, &server, secs, &totals).await {
                eprintln!("walker {k}: {e:#}");
                totals.errors.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    for t in tasks {
        let _ = t.await;
    }
    let t = &totals;
    let secs = a.secs.max(1) as f64;
    let connected = t.connected.load(Ordering::Relaxed).max(1) as f64;
    println!("bots connected     {}/{}", t.connected.load(Ordering::Relaxed), a.bots);
    println!("snapshots/bot/s    {:.1}", t.snapshots.load(Ordering::Relaxed) as f64 / connected / secs);
    println!(
        "downlink/bot       {:.1} KB/s",
        t.bytes.load(Ordering::Relaxed) as f64 / connected / secs / 1024.0
    );
    println!("max snapshot       {} B", t.max_snapshot.load(Ordering::Relaxed));
    println!("hits / kills       {} / {}", t.hits.load(Ordering::Relaxed), t.kills.load(Ordering::Relaxed));
    println!("worst pred. error  {} mm", t.pred_err_mm_max.load(Ordering::Relaxed));
    if a.walkers > 0 {
        println!("walkers in the city {}/{}", t.walking.load(Ordering::Relaxed), a.walkers);
        println!("most people seen    {}", t.seen_max.load(Ordering::Relaxed));
    }
    println!("errors             {}", t.errors.load(Ordering::Relaxed));
    if let Ok(status) = reqwest::get(format!("{}/status", a.server)).await
        && let Ok(json) = status.json::<serde_json::Value>().await
    {
        let g = &json["game"];
        println!(
            "server tick        p50 ≤{} µs  p99 ≤{} µs  max {} µs  overruns {}  out drops {}  hot-path allocations {}",
            g["tick_us"]["p50"],
            g["tick_us"]["p99"],
            g["tick_us"]["max"],
            g["overruns"],
            g["out_drops"],
            g["hot_path_allocations"]
        );
        if a.walkers > 0 {
            println!("city               {}", g["city"]);
        }
    }
    Ok(())
}

/// One agent on foot: down the cap lift to a strip's Hub Gate, strolling the avenue for `secs`.
async fn walk(k: usize, server: &str, secs: u64, totals: &Totals) -> anyhow::Result<()> {
    let cfg = BotConfig {
        server: server.to_string(),
        name: format!("Walker-{k:03}"),
        frame: FrameId::Leo,
        faction: Faction::Colonies,
    };
    let mut bot = BotClient::connect(&cfg).await?;
    anyhow::ensure!(bot.survival(), "walkers need survival rules");
    bot.wait_until(10.0, "the hangar", |c| c.hangar.in_hangar()).await?;
    anyhow::ensure!(
        bot.core.welcome.as_ref().is_some_and(|w| w.colony),
        "the colony is closed (--no-colony)"
    );
    let strip = (k % 3) as u8;
    bot.enter_city(strip).await?;
    totals.walking.fetch_add(1, Ordering::Relaxed);
    // Spread across the avenue, each strolling its own distance.
    let mut stroll = Stroll::new(strip, 40.0 + (k % 7) as f32 * 15.0, -12.0 + (k / 3 % 9) as f32 * 3.0);
    let end = Instant::now() + Duration::from_secs(secs);
    let mut last = Instant::now();
    while Instant::now() < end {
        let dt = last.elapsed().as_secs_f32().min(0.25);
        last = Instant::now();
        let pose = stroll.step(dt);
        bot.set_pose(pose);
        bot.step(&mut |_| InputCmd::default()).await?;
        totals.seen_max.fetch_max(bot.people().len() as u64, Ordering::Relaxed);
    }
    bot.leave_city().await?;
    bot.close().await;
    Ok(())
}
