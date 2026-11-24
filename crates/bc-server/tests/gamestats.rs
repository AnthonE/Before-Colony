//! Stats reported to Elo Pros, against a stand-in for it that checks every request's signature the
//! way `meter/serverkeys.py` does: a signed-in pilot who trades on the Exchange and flies a sortie
//! is reported under their wallet, an agent as one and with no board scores; a guest never is; and
//! a server with no key reports nothing at all.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::{HeaderMap, StatusCode};
use base64::Engine;
use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_econ::exchange::Side;
use bc_econ::item::{Item, Material};
use bc_econ::wire::Request;
use bc_proto::{Faction, FrameId};
use bc_server::serverkey::{ServerKey, server_text};
use bc_server::{Config, Mode, Ruleset};
use ring::signature::{ED25519, UnparsedPublicKey};

const SEED: [u8; 32] = [9; 32];

/// What the stand-in took: each report's body, and each refusal's reason.
#[derive(Clone, Default)]
struct Took {
    reports: Arc<Mutex<Vec<serde_json::Value>>>,
    refused: Arc<Mutex<Vec<String>>>,
    checks: Arc<Mutex<u32>>,
}

/// `serverkeys.verify_parts`, for the one key the stand-in knows.
fn verify(public: &[u8], path: &str, headers: &HeaderMap, body: &[u8]) -> Result<(), String> {
    let h = |k: &str| headers.get(k).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let (who, at, nonce, sig) = (h("x-elo-server"), h("x-elo-time"), h("x-elo-nonce"), h("x-elo-signature"));
    if who != "before-colony/sector-1" {
        return Err(format!("signed as {who:?}"));
    }
    let at: u64 = at.parse().map_err(|_| "no time".to_string())?;
    if bc_server::pilots::unix_now().abs_diff(at) > 300 {
        return Err("stale".into());
    }
    if !(8..=64).contains(&nonce.len())
        || !nonce.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(format!("nonce {nonce:?}"));
    }
    let sig = base64::engine::general_purpose::STANDARD.decode(sig).map_err(|_| "not base64".to_string())?;
    let text = server_text("POST", path, &who, at, body, Some(&nonce));
    UnparsedPublicKey::new(&ED25519, public)
        .verify(text.as_bytes(), &sig)
        .map_err(|_| "bad signature".to_string())
}

/// A stand-in for Elo Pros on a free port: its origin.
async fn elo_pros(took: Took) -> anyhow::Result<String> {
    let public = ServerKey::from_seed("before-colony", "sector-1", &SEED).map_err(anyhow::Error::msg)?;
    let public = base64::engine::general_purpose::STANDARD.decode(public.public_b64())?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("http://{}", listener.local_addr()?);
    let (p1, p2) = (public.clone(), public);
    let (t1, t2) = (took.clone(), took);
    let app = axum::Router::new()
        .route(
            "/api/gamestats/before-colony/report",
            axum::routing::post(move |headers: HeaderMap, body: axum::body::Bytes| async move {
                match verify(&p1, "/gamestats/before-colony/report", &headers, &body) {
                    Ok(()) => {
                        let doc: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
                        t1.reports.lock().unwrap().push(doc);
                        (StatusCode::OK, "{\"ok\":true}")
                    }
                    Err(why) => {
                        t1.refused.lock().unwrap().push(why);
                        (StatusCode::UNAUTHORIZED, "{\"error\":\"refused\"}")
                    }
                }
            }),
        )
        .route(
            "/api/serverkeys/check",
            axum::routing::post(move |headers: HeaderMap, body: axum::body::Bytes| async move {
                match verify(&p2, "/serverkeys/check", &headers, &body) {
                    Ok(()) => {
                        *t2.checks.lock().unwrap() += 1;
                        (StatusCode::OK, "{\"ok\":true}")
                    }
                    Err(why) => {
                        t2.refused.lock().unwrap().push(why);
                        (StatusCode::UNAUTHORIZED, "{\"error\":\"refused\"}")
                    }
                }
            }),
        );
    tokio::spawn(async move { axum::serve(listener, app).await });
    Ok(origin)
}

fn config(origin: &str, key: bool) -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 0,
        max_clients: 8,
        elo_origin: Some(origin.to_string()),
        elo_key: key.then(|| Arc::new(ServerKey::from_seed("before-colony", "sector-1", &SEED).unwrap())),
        elo_flush: Duration::from_millis(200),
        ..Config::default()
    })
}

fn bot(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Argo, faction: Faction::Colonies }
}

/// Sells steel to the colony (a fill), and flies a sortie.
async fn trade_and_fly(b: &mut BotClient) -> anyhow::Result<()> {
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    let steel = Item::Material(Material::Steel);
    b.ask(&Request::Order { item: steel, side: Side::Sell, price: 1, qty: 100, rest: false }).await?;
    b.launch().await
}

/// Every report row the stand-in took, by wallet.
fn rows(took: &Took) -> Vec<serde_json::Value> {
    took.reports
        .lock()
        .unwrap()
        .iter()
        .flat_map(|d| d["reports"].as_array().cloned().unwrap_or_default())
        .collect()
}

/// A stat summed over every row for `wallet`.
fn total(rows: &[serde_json::Value], wallet: &str, stat: &str) -> u64 {
    rows.iter().filter(|r| r["wallet"] == wallet).filter_map(|r| r["stats"][stat].as_u64()).sum()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pilots_play_is_reported_signed_and_a_guests_is_not() -> anyhow::Result<()> {
    let took = Took::default();
    let origin = elo_pros(took.clone()).await?;
    let server = bc_server::start(config(&origin, true)?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut secret = [0u8; 32];
    secret[31] = 1;
    let wallet = LocalWallet::from_secret(&secret).expect("key one");
    let address = "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf";

    let mut keyed = BotClient::connect_as(&bot(&http, "Keyed-1"), Some(&wallet)).await?;
    let mut guest = BotClient::connect(&bot(&http, "Guest-1")).await?;
    trade_and_fly(&mut keyed).await?;
    trade_and_fly(&mut guest).await?;
    let mut got = Vec::new();
    for _ in 0..100 {
        keyed.run_for(Duration::from_millis(100), &mut |_| bc_proto::InputCmd::default()).await?;
        got = rows(&took);
        if total(&got, address, "sorties") >= 1 && total(&got, address, "fills") >= 1 {
            break;
        }
    }
    assert!(took.refused.lock().unwrap().is_empty(), "{:?}", took.refused.lock().unwrap());
    assert_eq!(total(&got, address, "sorties"), 1, "{got:?}");
    assert!(total(&got, address, "fills") >= 1, "{got:?}");
    // An agent flies it: it says so, and it's on no board.
    for r in got.iter().filter(|r| r["wallet"] == address) {
        assert_eq!(r["agent"], true, "{r}");
        assert!(r.get("scores").is_none(), "{r}");
    }
    // Only the signed-in pilot: a guest is nobody.
    assert!(got.iter().all(|r| r["wallet"] == address), "{got:?}");
    // What was asked of the key at start, and the counts on `/status`.
    assert_eq!(*took.checks.lock().unwrap(), 1);
    let net = &server.status()["net"];
    assert!(net["elo_sent"].as_u64().is_some_and(|n| n >= 1), "{net}");
    assert_eq!((net["elo_refused"].as_u64(), net["elo_failed"].as_u64()), (Some(0), Some(0)), "{net}");
    keyed.close().await;
    guest.close().await;
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn with_no_key_nothing_is_reported() -> anyhow::Result<()> {
    let took = Took::default();
    let origin = elo_pros(took.clone()).await?;
    let server = bc_server::start(config(&origin, false)?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut secret = [0u8; 32];
    secret[31] = 2;
    let wallet = LocalWallet::from_secret(&secret).expect("key two");
    let mut keyed = BotClient::connect_as(&bot(&http, "Keyed-2"), Some(&wallet)).await?;
    trade_and_fly(&mut keyed).await?;
    keyed.run_for(Duration::from_millis(1_500), &mut |_| bc_proto::InputCmd::default()).await?;
    assert!(took.reports.lock().unwrap().is_empty() && *took.checks.lock().unwrap() == 0);
    assert_eq!(server.status()["net"]["elo_sent"], 0);
    keyed.close().await;
    server.shutdown();
    Ok(())
}
