//! Full stack: a real server in game mode, two agents on opposing sides over real WebTransport.
//! They must find each other on sensors, see each other as agents, and trade hits.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bc_bot::{BotClient, BotConfig, DollBrain};
use bc_proto::{Faction, FrameId, MAX_DATAGRAM, PilotKind};
use bc_server::{Config, Mode};

struct Outcome {
    saw_agent: bool,
    hits: u32,
    snapshots: u64,
    max_snapshot: usize,
    prediction_error_max: f32,
}

async fn fly(
    http: String,
    name: &str,
    faction: Faction,
    done: Arc<AtomicBool>,
) -> anyhow::Result<(Outcome, BotClient)> {
    let mut bot =
        BotClient::connect(&BotConfig { server: http, name: name.into(), frame: FrameId::Leo, faction })
            .await?;
    let mut brain = DollBrain::new(name.len() as u32 * 31 + 7);
    let mut out =
        Outcome { saw_agent: false, hits: 0, snapshots: 0, max_snapshot: 0, prediction_error_max: 0.0 };
    let end = Instant::now() + Duration::from_secs(90);
    while Instant::now() < end && !done.load(Ordering::Acquire) {
        bot.step(&mut |ctx| brain.decide(ctx)).await?;
        let w = bot.world();
        out.saw_agent |= w
            .entities
            .iter()
            .flatten()
            .any(|t| t.latest.pilot == PilotKind::Agent && t.latest.faction != faction);
        out.hits = w.my_hits;
        if bot.core.stats.snapshots > 90 {
            out.prediction_error_max = out.prediction_error_max.max(bot.core.stats.prediction_error);
        }
        if out.saw_agent && out.hits > 0 {
            done.store(true, Ordering::Release);
        }
    }
    out.snapshots = bot.core.stats.snapshots;
    out.max_snapshot = bot.core.stats.max_snapshot;
    Ok((out, bot))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_agents_find_each_other_and_fight() -> anyhow::Result<()> {
    let cfg = Config {
        mode: Mode::Game,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 0,
        max_clients: 8,
        ..Config::default()
    };
    let server = bc_server::start(cfg).await?;
    let http = format!("http://{}", server.http_addr);
    let done = Arc::new(AtomicBool::new(false));
    let (a, b) = tokio::join!(
        fly(http.clone(), "Agent-A", Faction::Colonies, done.clone()),
        fly(http.clone(), "Agent-B", Faction::Alliance, done.clone())
    );
    let ((a, bot_a), (b, bot_b)) = (a?, b?);
    // Read the server's view while both are still connected.
    let status = server.status();
    bot_a.close();
    bot_b.close();
    println!(
        "A: saw {} hits {} snaps {} max {} B pred-err {:.4} m | B: saw {} hits {} snaps {} | server tick p99 ≤{} µs",
        a.saw_agent,
        a.hits,
        a.snapshots,
        a.max_snapshot,
        a.prediction_error_max,
        b.saw_agent,
        b.hits,
        b.snapshots,
        status["game"]["tick_us"]["p99"]
    );
    assert!(a.saw_agent && b.saw_agent, "both agents should see the other flagged as an agent");
    assert!(a.hits + b.hits >= 1, "someone should land a hit");
    assert!(a.max_snapshot <= MAX_DATAGRAM && b.max_snapshot <= MAX_DATAGRAM);
    assert!(a.snapshots > 150 && b.snapshots > 150);
    let pilots = status["game"]["pilots"].as_array().cloned().unwrap_or_default();
    assert!(pilots.iter().any(|p| p["name"] == "Agent-A"), "roster: {pilots:?}");
    server.shutdown();
    Ok(())
}
