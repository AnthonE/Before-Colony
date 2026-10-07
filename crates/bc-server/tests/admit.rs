//! The server under abuse, over real WebTransport (`net::admit`; `docs/ARCHITECTURE.md`, "Under
//! abuse"): an address past its share is refused at the door and let in again once it has room;
//! under load a client proves its address (a QUIC Retry) and comes in all the same; a session
//! flooding its hangar with requests is refused and then ended; `/status` counts it all.

use std::time::{Duration, Instant};

use bc_bot::{BotClient, BotConfig};
use bc_econ::wire::Request;
use bc_proto::{Faction, FrameId};
use bc_server::net::admit::{Limits, Requests};
use bc_server::{Config, Mode, Ruleset};

fn config(limits: Limits) -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Arcade,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 0,
        max_clients: 8,
        limits,
        ..Config::default()
    })
}

async fn pilot(http: &str, name: &str) -> anyhow::Result<BotClient> {
    let cfg =
        BotConfig { server: http.into(), name: name.into(), frame: FrameId::Leo, faction: Faction::Colonies };
    BotClient::connect(&cfg).await
}

/// Steps `bot` until it has its suit, for at most `secs`.
async fn flying(bot: &mut BotClient, secs: f64) -> anyhow::Result<bool> {
    let end = Instant::now() + Duration::from_secs_f64(secs);
    while Instant::now() < end {
        if bot.world().own.is_some() {
            return Ok(true);
        }
        bot.step(&mut |_| Default::default()).await?;
    }
    Ok(bot.world().own.is_some())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_address_past_its_share_is_refused_until_it_has_room() -> anyhow::Result<()> {
    // One connection an address, loopback included (as if the tests came from one household).
    let limits = Limits { per_address: 1, loopback_excepted: false, ..Limits::default() };
    let server = bc_server::start(config(limits)?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut heero = pilot(&http, "Heero").await?;
    assert!(flying(&mut heero, 3.0).await?, "the first never got its suit");
    assert!(pilot(&http, "Duo").await.is_err(), "a second connection from the address got in");
    assert_eq!(server.status()["net"]["refused_address"], 1);
    // Heero goes: the address has room again.
    drop(heero);
    let end = Instant::now() + Duration::from_secs(5);
    let mut duo = loop {
        match pilot(&http, "Duo").await {
            Ok(bot) => break bot,
            Err(e) if Instant::now() > end => return Err(e.context("never let back in")),
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    };
    assert!(flying(&mut duo, 3.0).await?);
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn under_load_a_client_proves_its_address_and_comes_in() -> anyhow::Result<()> {
    // Always under load: every address is asked to prove itself first.
    let limits = Limits { retry_above: 0, ..Limits::default() };
    let server = bc_server::start(config(limits)?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut trowa = pilot(&http, "Trowa").await?;
    assert!(flying(&mut trowa, 3.0).await?, "a Retry kept a real client out");
    let retried = server.status()["net"]["retried"].as_u64().unwrap_or(0);
    assert!(retried >= 1, "no Retry was sent");
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_session_flooding_its_hangar_is_refused_then_ended() -> anyhow::Result<()> {
    let server = bc_server::start(config(Limits::default())?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut quatre = pilot(&http, "Quatre").await?;
    assert!(flying(&mut quatre, 3.0).await?);
    // Far more than a burst, as fast as it can write them.
    let flood = Requests::BURST as u32 + Requests::ENDS_AFTER + 20;
    for _ in 0..flood {
        if quatre.request(&Request::Watch { item: None }).await.is_err() {
            break;
        }
    }
    // The server ends the session: the agent's link goes.
    let end = Instant::now() + Duration::from_secs(5);
    let mut ended = false;
    while Instant::now() < end {
        if quatre.step(&mut |_| Default::default()).await.is_err() {
            ended = true;
            break;
        }
    }
    assert!(ended, "the flooding session was never ended");
    let net = &server.status()["net"];
    assert_eq!(net["flooders_ended"], 1);
    assert!(net["requests_refused"].as_u64().unwrap_or(0) >= u64::from(Requests::ENDS_AFTER));
    // Others are none the worse for it.
    let mut wufei = pilot(&http, "Wufei").await?;
    assert!(flying(&mut wufei, 3.0).await?);
    server.shutdown();
    Ok(())
}
