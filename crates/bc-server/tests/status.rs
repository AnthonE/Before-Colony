//! The HTTP side the Elo Pros store and launcher read: `/status.json` in their shape (people
//! connected, slots, the tick), answered to the store's origin and nobody else's, beside the
//! `/status` metrics and the `/cert-hash` a self-signed server's page pins.

use std::time::Duration;

use bc_bot::{BotClient, BotConfig};
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};

async fn get(
    url: &str,
    origin: Option<&str>,
) -> anyhow::Result<(reqwest::header::HeaderMap, serde_json::Value)> {
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
    let bot = BotConfig {
        server: base.clone(),
        name: "Agent-1".into(),
        frame: FrameId::Argo,
        faction: Faction::Colonies,
    };
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
    // One session, flying (arcade): it holds one of the twelve flight slots.
    let sessions = &status["game"]["sessions"];
    assert_eq!((sessions["live"].as_u64(), sessions["max"].as_u64()), (Some(1), Some(512)), "{sessions}");
    assert_eq!(sessions["flight_slots_free"], 11, "{sessions}");
    let pilot = &status["game"]["pilots"][0];
    assert_eq!(pilot["name"], "Agent-1");
    assert!(pilot["session"].as_u64().is_some_and(|id| id > 0), "{pilot}");
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

/// With a certificate from files (`--cert`/`--key`, a CA's in production), nothing is pinned: the
/// page dials it as it is, and `/cert-hash` says so with a null hash.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_certificate_from_files_is_not_pinned() -> anyhow::Result<()> {
    bc_server::install_crypto_provider();
    let dir = std::env::temp_dir().join(format!("bc-status-tls-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let identity = wtransport::Identity::self_signed(["localhost"])?;
    let (cert, key) = (dir.join("fullchain.pem"), dir.join("privkey.pem"));
    identity.certificate_chain().store_pemfile(&cert).await?;
    identity.private_key().store_secret_pemfile(&key).await?;
    let cfg = Config {
        mode: Mode::Echo,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        tls: Some((cert, key)),
        ..Config::default()
    };
    let server = bc_server::start(cfg).await?;
    assert_eq!(server.cert_hash, *identity.certificate_chain().as_slice()[0].hash().as_ref());
    let (_, doc) = get(&format!("http://{}/cert-hash", server.http_addr), None).await?;
    assert!(doc["hash"].is_null(), "{doc}");
    assert_eq!(doc["port"], server.wt_port);
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `/online.json`: the signed-in wallets, for the store's playtime. A guest isn't in it; an agent
/// with a wallet is, marked as one; a second session for the same wallet takes the entry over, and
/// leaving takes it away.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_store_reads_who_is_on() -> anyhow::Result<()> {
    let cfg = Config {
        mode: Mode::Game,
        rules: Ruleset::Arcade,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 0,
        ..Config::default()
    };
    let server = bc_server::start(cfg).await?;
    let base = format!("http://{}", server.http_addr);
    let bot = |name: &str| BotConfig {
        server: base.clone(),
        name: name.into(),
        frame: FrameId::Argo,
        faction: Faction::Colonies,
    };
    let mut secret = [0u8; 32];
    secret[31] = 1;
    let wallet = bc_auth::LocalWallet::from_secret(&secret).expect("key one");

    let guest = BotClient::connect(&bot("Guest-1")).await?;
    let mut keyed = BotClient::connect_as(&bot("Keyed-1"), Some(&wallet)).await?;
    keyed.run_for(Duration::from_millis(300), &mut |_| bc_proto::InputCmd::default()).await?;
    let (_, doc) = get(&format!("{base}/online.json"), None).await?;
    let on = doc["online"].as_array().expect("a list");
    assert_eq!(on.len(), 1, "only the wallet: {doc}");
    assert_eq!(on[0]["wallet"], "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf");
    assert_eq!(on[0]["name"], "Keyed-1");
    assert_eq!(on[0]["agent"], true);
    assert!(on[0]["since"].as_u64().is_some_and(|t| t > 0) && doc["at"].as_u64().is_some());

    // The same wallet again: the newer session holds the entry, the older one leaving doesn't drop it.
    let mut again = BotClient::connect_as(&bot("Keyed-2"), Some(&wallet)).await?;
    again.run_for(Duration::from_millis(300), &mut |_| bc_proto::InputCmd::default()).await?;
    keyed.close().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (_, doc) = get(&format!("{base}/online.json"), None).await?;
    assert_eq!(doc["online"].as_array().map(Vec::len), Some(1), "{doc}");
    assert_eq!(doc["online"][0]["name"], "Keyed-2");

    again.close().await;
    guest.close().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (_, doc) = get(&format!("{base}/online.json"), None).await?;
    assert_eq!(doc["online"], serde_json::json!([]), "{doc}");
    server.shutdown();
    Ok(())
}
