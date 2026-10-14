//! Sides over real WebTransport: the side a pilot flies for is the server's to say. The Almagest is the
//! drones' own, and they never hunt it: an agent claiming it flies for the Colonies, from
//! their spawn, is told so in its Welcome (its friends and foes go by it), and is seen so by the
//! pilot beside it; an agent asking for the Alliance flies for it.

use bc_bot::{BotClient, BotConfig};
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};
use glam::Vec3;

/// Where each side's suits come in (`bc_sim::sim`'s spawn points; each a little off it).
const COLONIES: Vec3 = Vec3::new(-4_200.0, 700.0, -3_200.0);
const ALLIANCE: Vec3 = Vec3::new(4_200.0, 700.0, -3_200.0);

fn agent(http: &str, name: &str, faction: Faction) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Argo, faction }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_claim_to_almagest_flies_for_the_colonies() -> anyhow::Result<()> {
    let server = bc_server::start(Config {
        mode: Mode::Game,
        rules: Ruleset::Arcade,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 0,
        max_clients: 8,
        ..Config::default()
    })
    .await?;
    let http = format!("http://{}", server.http_addr);
    let mut spy = BotClient::connect(&agent(&http, "Viktor", Faction::Almagest)).await?;
    spy.wait_until(5.0, "its suit", |c| c.world.own.is_some()).await?;
    let own = spy.world().own.expect("own suit");
    assert!(own.pos.distance(COLONIES) < 400.0, "at the Colonies' spawn: {}", own.pos);
    // Its Welcome said so: it takes the Colonies for friends and the drones for foes, as the
    // server does.
    assert_eq!(spy.world().faction, Faction::Colonies);
    assert_eq!(spy.core.cfg.faction, Faction::Colonies, "a reconnect asks for it");
    // Beside a pilot of the Colonies, who sees it flying for them.
    let mut mate = BotClient::connect(&agent(&http, "Howard", Faction::Colonies)).await?;
    mate.wait_until(5.0, "Viktor on sensors", |c| c.world.entity(own.slot).is_some()).await?;
    assert_eq!(mate.world().entity(own.slot).map(|t| t.latest.faction), Some(Faction::Colonies));
    // An agent keeps the side it asked for, of the others.
    let mut other = BotClient::connect(&agent(&http, "Sally", Faction::Alliance)).await?;
    other.wait_until(5.0, "its suit", |c| c.world.own.is_some()).await?;
    let at = other.world().own.expect("own suit").pos;
    assert!(at.distance(ALLIANCE) < 400.0, "at the Alliance's spawn: {at}");
    assert_eq!(other.world().faction, Faction::Alliance);
    spy.close().await;
    mate.close().await;
    other.close().await;
    server.shutdown();
    Ok(())
}
