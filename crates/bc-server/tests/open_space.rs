//! Open space over real WebTransport (`bc_sim::colony::reach`): with `--pvp on` the sector keeps
//! `Law::OpenSpace` and every Welcome says so, and two agents of the Colonies, side by side at
//! their spawn out in open space, land rounds on each other; with it off (arcade's default) the
//! Welcome says nothing, and the same rounds pass a friend by. A survival server keeps open space
//! unless told otherwise, as `scripts/dev.sh`, `scripts/pt` and a deployment run it.

use std::time::{Duration, Instant};

use bc_bot::{BotClient, BotConfig};
use bc_proto::buttons::{FIRE_SECONDARY, FLIGHT_ASSIST};
use bc_proto::{Faction, FrameId, InputCmd, WeaponKind};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::content::{frame, weapon};
use glam::Vec3;

fn agent(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Argo, faction: Faction::Colonies }
}

/// A server under arcade rules (both spawn at the Colonies' base, past the colony's reach), PvP
/// as asked; `a` holds its machine cannon on `b` for up to `secs`. Whether both were told it's
/// open space, and the hits `a` landed.
async fn duel(pvp: bool, secs: u64) -> anyhow::Result<(bool, bool, u32)> {
    let server = bc_server::start(Config {
        mode: Mode::Game,
        rules: Ruleset::Arcade,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 0,
        max_clients: 8,
        pvp: Some(pvp),
        ..Config::default()
    })
    .await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = BotClient::connect(&agent(&http, "Agent-A")).await?;
    let mut b = BotClient::connect(&agent(&http, "Agent-B")).await?;
    a.wait_until(5.0, "its suit", |c| c.world.own.is_some()).await?;
    b.wait_until(5.0, "its suit", |c| c.world.own.is_some()).await?;
    let foe = b.world().own.expect("own suit").slot;
    a.wait_until(5.0, "Agent-B on sensors", |c| c.world.entity(foe).is_some()).await?;
    let speed = weapon(WeaponKind::MachineCannon).speed;
    let muzzle = frame(FrameId::Argo).loadout[1].map_or(Vec3::ZERO, |m| m.arm.muzzle());
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end && a.world().my_hits == 0 {
        a.step(&mut |ctx| {
            let f = &ctx.predict.state;
            let Some(track) = ctx.world.entity(foe) else { return InputCmd::default() };
            let t = ctx.resolve_tick.max(0.0);
            let s = track.sample(t, &ctx.world.bodies);
            let from = f.pos + f.rot * muzzle;
            let at = s.pos + (s.vel - f.vel) * (s.pos.distance(from) / speed);
            InputCmd {
                aim: (at - from).normalize_or(Vec3::Z),
                buttons: FLIGHT_ASSIST | FIRE_SECONDARY,
                ..InputCmd::default()
            }
        })
        .await?;
        b.step(&mut |_| InputCmd { buttons: FLIGHT_ASSIST, ..InputCmd::default() }).await?;
    }
    let told = |bot: &BotClient| bot.core.welcome.is_some_and(|w| w.open_space);
    let out = (told(&a), told(&b), a.world().my_hits);
    let law = server.status()["game"]["law"].clone();
    assert_eq!(law, if pvp { "open_space" } else { "sides" }, "/status says which");
    a.close().await;
    b.close().await;
    server.shutdown();
    Ok(out)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn with_pvp_on_one_side_fights_in_open_space() -> anyhow::Result<()> {
    let (told_a, told_b, hits) = duel(true, 30).await?;
    assert!(told_a && told_b, "the Welcome says open space");
    assert!(hits > 0, "Agent-A's rounds land on Agent-B");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn with_pvp_off_a_friend_is_never_hit() -> anyhow::Result<()> {
    let (told_a, told_b, hits) = duel(false, 6).await?;
    assert!(!told_a && !told_b, "the Welcome says nothing of open space");
    assert_eq!(hits, 0, "rounds pass a friend by");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_survival_server_keeps_open_space_unless_told() -> anyhow::Result<()> {
    for (pvp, open) in [(None, true), (Some(false), false)] {
        let server = bc_server::start(Config {
            mode: Mode::Game,
            wt_port: 0,
            http_addr: "127.0.0.1:0".parse()?,
            drones: 0,
            max_clients: 8,
            pvp,
            ..Config::default()
        })
        .await?;
        let http = format!("http://{}", server.http_addr);
        let mut a = BotClient::connect(&agent(&http, "Agent-A")).await?;
        a.wait_until(5.0, "its Welcome", |c| c.welcome.is_some()).await?;
        assert_eq!(a.core.welcome.is_some_and(|w| w.open_space), open, "--pvp {pvp:?}: the Welcome");
        let law = server.status()["game"]["law"].clone();
        assert_eq!(law, if open { "open_space" } else { "sides" }, "--pvp {pvp:?}: /status");
        a.close().await;
        server.shutdown();
    }
    Ok(())
}
