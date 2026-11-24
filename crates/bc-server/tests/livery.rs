//! Liveries over real WebTransport (survival rules): a pilot paints the suit in their bay for
//! credits, in what their wallet holds on Elo Pros (read once as they sign in, from a stand-in for
//! its items route), and everyone sees the paint on the roster once it flies. A pilot whose wallet
//! holds nothing is refused the premium paints, and pays nothing for it. A read that fails keeps
//! what the wallet held when last read.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_client_core::Identity;
use bc_econ::Bay;
use bc_econ::livery::{PAINT_CR, Paint};
use bc_econ::wire::Request;
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::content::cosmetics::{Cosmetic, Owned, default_livery, eye, paint};

fn wallet(k: u8) -> LocalWallet {
    let mut secret = [0u8; 32];
    secret[31] = k;
    LocalWallet::from_secret(&secret).expect("a valid key")
}

fn id(w: &LocalWallet) -> String {
    format!("0x{}", hex::encode(w.address().0))
}

/// How the stand-in for Elo Pros answers: as it should; failing (503); up but unable to look
/// (`reachable: false`); or with `rich` holding nothing now.
const UP: u8 = 0;
const FAILING: u8 = 1;
const BLIND: u8 = 2;
const SOLD: u8 = 3;

/// A stand-in for `GET /api/items/sol/of/<wallet>`, answering as `mode` says: `rich` holds the
/// Pill, Coalsack and the Lodestar decal (and another title's Albireo), everyone else nothing.
async fn items_origin(rich: String, mode: Arc<AtomicU8>) -> String {
    let app = Router::new().route(
        "/api/items/sol/of/{wallet}",
        get(move |axum::extract::Path(wallet): axum::extract::Path<String>| {
            let (rich, mode) = (rich.clone(), mode.load(Ordering::SeqCst));
            async move {
                let items = if wallet == rich && mode != SOLD {
                    serde_json::json!([
                        {"title": "before-colony", "item": "pill", "name": "The Pill"},
                        {"title": "before-colony", "item": "coalsack", "name": "Coalsack"},
                        {"title": "before-colony", "item": "lodestar-decal", "name": "Lodestar decal"},
                        {"title": "gates", "item": "albireo"},
                    ])
                } else {
                    serde_json::json!([])
                };
                match mode {
                    FAILING => StatusCode::SERVICE_UNAVAILABLE.into_response(),
                    BLIND => axum::Json(serde_json::json!({"reachable": false, "items": []})).into_response(),
                    _ => axum::Json(serde_json::json!({"reachable": true, "wallet": wallet, "items": items}))
                        .into_response(),
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{addr}")
}

fn bot(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Argo, faction: Faction::Colonies }
}

/// What the server answered `req` with: its note, and whether it was done.
async fn ask(b: &mut BotClient, req: Request) -> anyhow::Result<(String, bool)> {
    let n = b.core.hangar.notes.len();
    b.request(&req).await?;
    b.wait_until(5.0, "an answer", |c| c.hangar.notes.len() > n).await?;
    Ok(b.core.hangar.notes[n].clone())
}

fn painted(b: &BotClient) -> Option<Paint> {
    match &b.core.hangar.view.as_ref()?.bay {
        Bay::Docked { suit } => suit.paint,
        _ => None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_suit_is_painted_in_what_its_wallet_holds_and_everyone_sees_it() -> anyhow::Result<()> {
    let (rich, poor) = (wallet(31), wallet(32));
    let cfg = Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 0,
        max_clients: 8,
        elo_origin: Some(items_origin(id(&rich), Arc::new(AtomicU8::new(UP))).await),
        ..Config::default()
    };
    let server = bc_server::start(cfg).await?;
    let http = format!("http://{}", server.http_addr);
    let argo = default_livery(FrameId::Argo, Faction::Colonies);

    let me = Identity::Wallet { address: rich.address(), resume: None };
    let mut a = BotClient::connect_with(&bot(&http, "Vela"), me, Some(&rich)).await?;
    a.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    let held = a.core.hangar.view.as_ref().unwrap().cosmetics;
    assert_ne!(held, 0, "the wallet's cosmetics, the server's read");
    let credits = a.core.hangar.credits();
    // Black glass and gunmetal (Coalsack) with the Lodestar decal: the body, the accent, the decal.
    let want = Paint { body: paint::GLASS, accent: paint::GUNMETAL, decal: 2, ..Paint::of(argo, 0) };
    let (text, ok) = ask(&mut a, Request::Paint { paint: want }).await?;
    assert!(ok, "{text}");
    a.wait_until(5.0, "the paint in the bay", |c| {
        c.hangar
            .view
            .as_ref()
            .is_some_and(|v| matches!(&v.bay, Bay::Docked { suit } if suit.paint == Some(want)))
    })
    .await?;
    assert_eq!(a.core.hangar.credits(), credits - PAINT_CR.body - PAINT_CR.accent - PAINT_CR.decal);
    // Albireo is another title's here: cyan sensors are refused, and cost nothing.
    let (why, ok) = ask(&mut a, Request::Paint { paint: Paint { eye: eye::CYAN, ..want } }).await?;
    assert!(!ok && why.contains("Albireo"), "{why}");
    assert_eq!(painted(&a), Some(want));

    // A wallet with nothing: the standard paints, never the premium ones.
    let me = Identity::Wallet { address: poor.address(), resume: None };
    let mut b = BotClient::connect_with(&bot(&http, "Lyra"), me, Some(&poor)).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    let poor_credits = b.core.hangar.credits();
    let (why, ok) = ask(&mut b, Request::Paint { paint: want }).await?;
    assert!(!ok && why.contains("Coalsack"), "{why}");
    assert_eq!(b.core.hangar.credits(), poor_credits);
    let yellow = Paint { body: paint::YELLOW, ..Paint::of(argo, 0) };
    let (text, ok) = ask(&mut b, Request::Paint { paint: yellow }).await?;
    assert!(ok, "{text}");

    // Out in the sector, each suit's paint is on everyone's roster.
    a.launch().await?;
    b.launch().await?;
    let (sa, sb) = (a.world().own.unwrap().slot, b.world().own.unwrap().slot);
    let seen = |c: &bc_client_core::ClientCore, slot: u16, body: u8, decal: u8| {
        let l = bc_client_core::look_of(&c.world, slot);
        l.livery.is_some_and(|l| l.body == body) && l.decal == decal
    };
    b.wait_until(5.0, "the other suit's paint", |c| seen(c, sa, paint::GLASS, 2)).await?;
    a.wait_until(5.0, "the other suit's paint", |c| seen(c, sb, paint::YELLOW, 0)).await?;
    let mine = bc_client_core::look_of(b.world(), sa).livery.unwrap();
    assert_eq!((mine.trim, mine.accent, mine.eye), (argo.trim, paint::GUNMETAL, argo.eye));

    a.close().await;
    b.close().await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    server.shutdown();
    Ok(())
}

fn cosmetics(b: &BotClient) -> Option<u32> {
    b.core.hangar.view.as_ref().map(|v| v.cosmetics)
}

/// Elo Pros failing, or unable to look, as a pilot signs in is no answer: what their wallet held
/// when last read stands (what they may paint in, the paint shown, the Pill). Only an answer
/// replaces it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_read_keeps_what_the_wallet_held_when_last_read() -> anyhow::Result<()> {
    let w = wallet(33);
    let mode = Arc::new(AtomicU8::new(UP));
    let cfg = Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 0,
        max_clients: 8,
        elo_origin: Some(items_origin(id(&w), mode.clone()).await),
        ..Config::default()
    };
    let server = bc_server::start(cfg).await?;
    let http = format!("http://{}", server.http_addr);
    let argo = default_livery(FrameId::Argo, Faction::Colonies);
    let me = Identity::Wallet { address: w.address(), resume: None };

    let mut b = BotClient::connect_with(&bot(&http, "Deneb"), me, Some(&w)).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    let held = cosmetics(&b).unwrap();
    assert!(Owned(held).has(Cosmetic::Pill) && Owned(held).has(Cosmetic::Coalsack), "{held:#b}");
    let glass = Paint { body: paint::GLASS, decal: 2, ..Paint::of(argo, 0) };
    let (text, ok) = ask(&mut b, Request::Paint { paint: glass }).await?;
    assert!(ok, "{text}");
    b.close().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Failing, then unable to look: as last read.
    for (m, why) in [(FAILING, "a failed read"), (BLIND, "a read that couldn't look")] {
        mode.store(m, Ordering::SeqCst);
        let mut b = BotClient::connect_with(&bot(&http, "Deneb"), me, Some(&w)).await?;
        b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
        assert_eq!(cosmetics(&b), Some(held), "{why}");
        assert_eq!(painted(&b), Some(glass), "{why}");
        if m == BLIND {
            // Out in the sector, the paint and the Pill on the roster as before.
            b.launch().await?;
            let s = b.world().own.unwrap().slot;
            b.wait_until(5.0, "the paint and the Pill", |c| {
                let l = bc_client_core::look_of(&c.world, s);
                l.livery.is_some_and(|l| l.body == paint::GLASS)
                    && l.decal == 2
                    && bc_client_core::has_pill(&c.world, s)
            })
            .await?;
            b.drop_link();
            tokio::time::sleep(Duration::from_millis(500)).await;
        } else {
            b.close().await;
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    // An answer that it holds nothing now: the frame's own look, and no Pill (the paint is kept).
    mode.store(SOLD, Ordering::SeqCst);
    let mut b = BotClient::connect_with(&bot(&http, "Deneb"), me, Some(&w)).await?;
    b.wait_until(5.0, "back in the cockpit", |c| c.world.own.is_some() && c.hangar.view.is_some()).await?;
    assert_eq!(cosmetics(&b), Some(0));
    let s = b.world().own.unwrap().slot;
    b.wait_until(5.0, "the frame's own look", |c| {
        bc_client_core::look_of(&c.world, s) == bc_proto::control::Look::default()
            && c.world.roster_flags.contains_key(&s)
            && !bc_client_core::has_pill(&c.world, s)
    })
    .await?;
    b.drop_link();
    tokio::time::sleep(Duration::from_millis(200)).await;
    server.shutdown();
    Ok(())
}
