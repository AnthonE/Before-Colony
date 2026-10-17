//! The HTTP side the Elo Pros store and launcher read: `/status.json` in their shape (people
//! connected, slots, the tick), answered to the store's origin and nobody else's, beside the
//! `/status` metrics and the `/cert-hash` a self-signed server's page pins.

use std::time::Duration;

use bc_bot::{BotClient, BotConfig};
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};

async fn get(url: &str, origin: Option<&str>) -> anyhow::Result<(reqwest::header::HeaderMap, serde_json::Value)> {
    let mut req = reqwest::Client::new().get(url);
    if let Some(o) = origin {
        req = req.header("Origin", o);
    }
    let res = req.send().await?.error_for_status()?;
    let headers = res.headers().clone();
    Ok((headers, res.json().await?))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_store_reads_players_slots_and_tick() -> anyhow::Result<()> {
    let cfg = Config {
        mode: Mode::Game,
        rules: Ruleset::Arcade,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 2,
        max_clients: 12,
        ..Config::default()
    };
    let server = bc_server::start(cfg).await?;
    let base = format!("http://{}", server.http_addr);

    // An agent flying: it isn't a player (drones neither).
    let bot = BotConfig { server: base.clone(), name: "Agent-1".into(), frame: FrameId::Argo, faction: Faction::Colonies };
    let mut agent = BotClient::connect(&bot).await?;
    agent.run_for(Duration::from_millis(400), &mut |_| bc_proto::InputCmd::default()).await?;

    let (headers, doc) = get(&format!("{base}/status.json"), Some("https://elopros.com")).await?;
    assert_eq!(doc["players"], 0, "{doc}");
    assert_eq!(doc["agents"], 1, "{doc}");
    assert_eq!(doc["max_players"], 12);
    assert!(doc["tick"].as_u64().is_some_and(|t| t > 0), "the sector ticks: {doc}");
    assert_eq!(doc["proto"], bc_proto::PROTOCOL_VERSION);
    assert_eq!(headers["access-control-allow-origin"], "https://elopros.com");

    // Another site's page reads nothing (no CORS header); a same-origin fetch needs none.
    let (headers, _) = get(&format!("{base}/status.json"), Some("https://elsewhere.example")).await?;
    assert!(headers.get("access-control-allow-origin").is_none());
    let (headers, _) = get(&format!("{base}/status.json"), None).await?;
    assert!(headers.get("access-control-allow-origin").is_none());

    // The metrics route stays, with the store's origin too.
    let (headers, status) = get(&format!("{base}/status"), Some("https://elopros.com")).await?;
    assert_eq!(status["mode"], "game");
    assert_eq!(status["net"]["agents_active"], 1);
    assert_eq!(headers["access-control-allow-origin"], "https://elopros.com");

    // Self-signed: the page pins this hash.
    let (headers, cert) = get(&format!("{base}/cert-hash"), Some("https://elopros.com")).await?;
    assert_eq!(cert["hash"], hex(&server.cert_hash));
    assert_eq!(cert["port"], server.wt_port);
    assert_eq!(headers["access-control-allow-origin"], "https://elopros.com");

    // Gone: the count gives the place back.
    agent.close().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (_, doc) = get(&format!("{base}/status.json"), None).await?;
    assert_eq!(doc["agents"], 0, "{doc}");
    server.shutdown();
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
