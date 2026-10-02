//! Full stack: lock-on over real WebTransport. One agent finds another on sensors, locks on to it
//! (`bc_client_core::lockon`), holds W and boost, and closes on it while it fights back and keeps
//! its distance as it likes to; the lock-on travels in its commands, and its prediction keeps to
//! the server's all the while.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bc_bot::{BotClient, BotConfig, DollBrain};
use bc_client_core::lockon::{self, Lock};
use bc_proto::buttons::{BOOST, FLIGHT_ASSIST};
use bc_proto::{Faction, FrameId, InputCmd};
use bc_server::{Config, Mode, Ruleset};
use glam::Vec3;

struct Outcome {
    locked: bool,
    /// How far off the quarry was when first locked, m.
    first: f32,
    lockon_cmds: u32,
    closest: f32,
    prediction_error_max: f32,
}

/// The hunter: flies like an agent until its quarry is in reach of a lock, then locks on and holds
/// W, the mouse on the quarry.
async fn hunt(http: String, done: Arc<AtomicBool>) -> anyhow::Result<(Outcome, BotClient)> {
    let mut bot = BotClient::connect(&BotConfig {
        server: http,
        name: "Hunter".into(),
        frame: FrameId::Leo,
        faction: Faction::Colonies,
    })
    .await?;
    let mut brain = DollBrain::new(7);
    let mut lock = Lock::default();
    let stop = lockon::stop_range(FrameId::Leo);
    let mut out =
        Outcome { locked: false, first: 0.0, lockon_cmds: 0, closest: f32::MAX, prediction_error_max: 0.0 };
    let (mut in_close, mut range) = (0u32, f32::MAX);
    let end = Instant::now() + Duration::from_secs(90);
    while Instant::now() < end && !done.load(Ordering::Acquire) {
        bot.step(&mut |ctx| {
            let me = ctx.predict.state.pos;
            if lock.validate(ctx.world, me, ctx.view_tick).is_some() || !lock.locked() {
                lock.tap(ctx.world, me, ctx.predict.state.rot * Vec3::Z, ctx.view_tick);
            }
            let Some(target) = lock.target(ctx.world, ctx.view_tick) else { return brain.decide(ctx) };
            let to = target.pos - me;
            range = to.length();
            let keys = InputCmd {
                aim: to.normalize_or(Vec3::Z),
                thrust: [0, 0, 127],
                buttons: FLIGHT_ASSIST | BOOST,
                ..InputCmd::default()
            };
            lockon::shape(keys, &mut lock, ctx)
        })
        .await?;
        let core = &bot.core;
        out.locked |= lock.locked();
        if core.last_cmd.lockon.is_some() {
            if out.lockon_cmds == 0 {
                out.first = range;
            }
            out.lockon_cmds += 1;
            out.closest = out.closest.min(range);
            in_close = if range < stop + 15.0 { in_close + 1 } else { 0 };
        }
        if core.stats.snapshots > 90 {
            out.prediction_error_max = out.prediction_error_max.max(core.stats.prediction_error);
        }
        if in_close > 60 {
            done.store(true, Ordering::Release);
        }
    }
    Ok((out, bot))
}

/// The quarry: an agent with the Mobile Doll's judgement, fighting back.
async fn quarry(http: String, done: Arc<AtomicBool>) -> anyhow::Result<BotClient> {
    let mut bot = BotClient::connect(&BotConfig {
        server: http,
        name: "Quarry".into(),
        frame: FrameId::Leo,
        faction: Faction::Alliance,
    })
    .await?;
    let mut brain = DollBrain::new(11);
    let end = Instant::now() + Duration::from_secs(90);
    while Instant::now() < end && !done.load(Ordering::Acquire) {
        bot.step(&mut |ctx| brain.decide(ctx)).await?;
    }
    Ok(bot)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_locked_on_agent_closes_in_and_predicts_it() -> anyhow::Result<()> {
    let cfg = Config {
        mode: Mode::Game,
        rules: Ruleset::Arcade,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 0,
        max_clients: 8,
        ..Config::default()
    };
    let server = bc_server::start(cfg).await?;
    let http = format!("http://{}", server.http_addr);
    let done = Arc::new(AtomicBool::new(false));
    let (a, b) = tokio::join!(hunt(http.clone(), done.clone()), quarry(http.clone(), done.clone()));
    let ((out, hunter), quarry) = (a?, b?);
    hunter.close().await;
    quarry.close().await;
    println!(
        "locked {} at {:.0} m, on for {} commands, closest {:.1} m, prediction error ≤ {:.4} m",
        out.locked, out.first, out.lockon_cmds, out.closest, out.prediction_error_max
    );
    assert!(out.locked && out.lockon_cmds > 100, "never flew locked on");
    assert!(out.closest < out.first * 0.5, "never closed in: {} m from {} m", out.closest, out.first);
    assert!(out.prediction_error_max < 0.05, "predicted {} m off the server", out.prediction_error_max);
    server.shutdown();
    Ok(())
}
