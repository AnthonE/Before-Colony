//! The colony's desks over real WebTransport (survival, the colony open; `docs/COLONY.md`, 2.6):
//! what only a desk in the city does is done there. From the bay a pilot's terminal points them at
//! it: posting, patrols and the charter at the Charter Board's desk, an ace's terms at The
//! Arrival's counter, the test range at the Proving Ground's desk; an order at their own price
//! trades at once instead, and what didn't fill comes back. Everything else still works from the
//! bay. A pilot whose record has a time at the Proving Ground has a flight certificate: the Charter
//! Board issues a beam rifle for it, once, and pays it out at its desk, where they walk to collect
//! it, past The Arrival and the Exchange floor, each doing its own one thing.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_econ::Bay;
use bc_econ::charter::{Task, Work};
use bc_econ::exchange::Side;
use bc_econ::item::{Item, Material};
use bc_econ::proving::Trainer;
use bc_econ::wire::{FLOOR_ONLY, Request};
use bc_proto::{Faction, FrameId, InputCmd, WeaponKind};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::content::city::{PLACES, PlaceKind};

const STEEL: Item = Item::Material(Material::Steel);
const RIFLE: Item = Item::Weapon(WeaponKind::BeamRifle);
/// The Charter Board's news of a pilot's flight certificate.
const CERTIFIED: &str = "HAS YOUR FLIGHT CERTIFICATE";

fn config(data_dir: Option<PathBuf>) -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        colony: true,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 0,
        max_clients: 8,
        data_dir,
        ..Config::default()
    })
}

fn bot(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Argo, faction: Faction::Colonies }
}

fn wallet() -> LocalWallet {
    let mut secret = [0u8; 32];
    secret[31] = 91;
    LocalWallet::from_secret(&secret).expect("a valid key")
}

/// Key place `kind`'s index in `PLACES` (the first, for Hub Gate).
fn place(kind: PlaceKind) -> usize {
    PLACES.iter().position(|p| p.kind == kind).expect("a key place")
}

fn stock(b: &BotClient, item: Item) -> u64 {
    b.core.hangar.view.as_ref().map_or(0, |v| v.stock.iter().find(|(i, _)| *i == item).map_or(0, |(_, q)| *q))
}

/// Asks, and waits for the note that answers: its text, and whether it was done.
async fn ask(b: &mut BotClient, req: Request) -> anyhow::Result<(String, bool)> {
    let n = b.core.hangar.notes.len();
    b.request(&req).await?;
    b.wait_until(5.0, "an answer", |c| c.hangar.notes.len() > n).await?;
    Ok(b.core.hangar.notes[n].clone())
}

/// Asks, and expects it refused with `why` in the reason.
async fn refused(b: &mut BotClient, req: Request, why: &str) -> anyhow::Result<()> {
    let (text, ok) = ask(b, req.clone()).await?;
    anyhow::ensure!(!ok && text.contains(why), "{req:?}: {ok} {text}");
    Ok(())
}

/// Asks, and expects it done. Its note.
async fn done(b: &mut BotClient, req: Request) -> anyhow::Result<String> {
    let (text, ok) = ask(b, req.clone()).await?;
    anyhow::ensure!(ok, "{req:?} refused: {text}");
    Ok(text)
}

/// Sells what doesn't wait on the book: the market as sent after it (none of the pilot's orders
/// resting), and the note.
async fn trade(b: &mut BotClient, req: Request) -> anyhow::Result<String> {
    b.core.hangar.market = None;
    let text = done(b, req).await?;
    b.wait_until(5.0, "the market", |c| c.hangar.market.is_some()).await?;
    Ok(text)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_bay_points_at_the_desks() -> anyhow::Result<()> {
    let server = bc_server::start(config(None)?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut b = BotClient::connect(&bot(&http, "Maren")).await?;
    b.wait_until(5.0, "the hangar", |c| {
        c.hangar.in_hangar() && c.hangar.view.is_some() && c.hangar.market.is_some()
    })
    .await?;
    assert!(b.core.welcome.is_some_and(|w| w.colony), "the colony's open");
    let steel = stock(&b, STEEL);
    assert_eq!(steel, 200, "the steel an Arrival comes with");

    // What only a desk does, the bay's terminal points at.
    let post = Request::Post { item: STEEL, qty: 10, reward: 50, hours: 1 };
    for req in [post, Request::TakePatrol { id: 1 }, Request::Sign] {
        refused(&mut b, req, "charter board's desk").await?;
    }
    refused(&mut b, Request::AceTerms { salvage: false }, "the arrival's counter").await?;
    refused(&mut b, Request::Trainer { build: Trainer::Board }, "proving ground's desk").await?;

    // An order at the pilot's own price trades at once from here, and what didn't fill comes back.
    let order = Request::Order { item: STEEL, side: Side::Sell, price: 90_000, qty: 50, rest: true };
    let text = trade(&mut b, order).await?;
    assert!(text.contains(FLOOR_ONLY) && text.contains("NOTHING MATCHED"), "{text}");
    assert!(b.core.hangar.market.as_ref().is_some_and(|m| m.orders.is_empty()), "nothing rests on the book");
    b.wait_until(5.0, "the steel back", |c| {
        c.hangar.view.as_ref().is_some_and(|v| v.stock.iter().any(|(i, q)| *i == STEEL && *q == steel))
    })
    .await?;
    // A sale at once is anyone's, anywhere.
    let sale = Request::Order { item: STEEL, side: Side::Sell, price: 1, qty: 50, rest: false };
    let text = trade(&mut b, sale).await?;
    assert!(text.contains("SOLD") && !text.contains(FLOOR_ONLY), "{text}");

    // The rest is the bay's as ever.
    refused(&mut b, Request::Withdraw { id: 999 }, "no such contract").await?;
    refused(&mut b, Request::DropPatrol { id: 999 }, "no such contract").await?;
    let text = done(&mut b, Request::Contribute { work: Work::SecondFoundry, item: STEEL, qty: 50 }).await?;
    assert!(text.contains("A SECOND FOUNDRY"), "{text}");
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    b.close().await;
    server.shutdown();
    Ok(())
}

fn data_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bc-desks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Writes a time at the Proving Ground on the signed-in pilot's record in `dir`: a veteran's, from
/// before the Board issued rifles for it.
fn flown(dir: &Path) -> anyhow::Result<()> {
    let file = std::fs::read_dir(dir.join("pilots"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "json"))
        .expect("the pilot's record");
    let mut rec: serde_json::Value = serde_json::from_slice(&std::fs::read(&file)?)?;
    rec["proving"] = serde_json::json!({ "drill_ms": 60_000 });
    std::fs::write(&file, serde_json::to_vec(&rec)?)?;
    Ok(())
}

/// What the Charter Board holds for the pilot at its desk, as they were last shown.
fn waiting(b: &BotClient) -> Option<bc_econ::charter::Waiting> {
    b.core.hangar.charter.as_ref().and_then(|v| v.waiting.clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_certified_pilot_is_paid_at_the_charter_boards_desk() -> anyhow::Result<()> {
    let dir = data_dir();
    let w = wallet();
    // A pilot who's been here before.
    {
        let server = bc_server::start(config(Some(dir.clone()))?).await?;
        let http = format!("http://{}", server.http_addr);
        let mut b = BotClient::connect_as(&bot(&http, "Sally"), Some(&w)).await?;
        b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
        b.close().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        server.shutdown();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    flown(&dir)?;

    let server = bc_server::start(config(Some(dir.clone()))?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut b = BotClient::connect_as(&bot(&http, "Sally"), Some(&w)).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    assert!(!b.core.hangar.news.iter().any(|n| n.contains(CERTIFIED)), "not in the bay");

    // Down in the colony, the Board has seen their time: a beam rifle waits at its desk.
    b.enter_city(0).await?;
    b.wait_until(5.0, "the certificate", |c| c.hangar.news.iter().any(|n| n.contains(CERTIFIED))).await?;
    b.request(&Request::WatchBoard { on: true }).await?;
    b.wait_until(5.0, "the rifle waiting", |c| {
        c.hangar
            .charter
            .as_ref()
            .and_then(|v| v.waiting.as_ref())
            .is_some_and(|d| d.goods.contains(&(RIFLE, 1)))
    })
    .await?;
    assert_eq!(stock(&b, RIFLE), 0, "not in the stores yet");

    // The Arrival: an ace's terms, and only those; salvage only for pilots of standing.
    b.walk_to_place(place(PlaceKind::Bar), 360.0).await?;
    let post = Request::Post { item: STEEL, qty: 10, reward: 50, hours: 1 };
    refused(&mut b, post.clone(), "charter board's desk").await?;
    let text = done(&mut b, Request::AceTerms { salvage: false }).await?;
    assert!(text.contains("PAID"), "{text}");
    refused(&mut b, Request::AceTerms { salvage: true }, "pilots of standing").await?;

    // The Exchange floor: an order at their own price waits on the book.
    b.walk_to_place(place(PlaceKind::Exchange), 240.0).await?;
    let order = Request::Order { item: STEEL, side: Side::Sell, price: 90_000, qty: 10, rest: true };
    let text = done(&mut b, order).await?;
    assert!(text.contains("WAITING") && !text.contains(FLOOR_ONLY), "{text}");
    b.wait_until(5.0, "the order on the book", |c| {
        c.hangar
            .market
            .as_ref()
            .is_some_and(|m| m.orders.iter().any(|o| o.item == STEEL && o.price == 90_000))
    })
    .await?;
    let id = b.core.hangar.market.as_ref().and_then(|m| m.orders.first()).map(|o| o.id).unwrap();
    done(&mut b, Request::CancelOrder { id }).await?;

    // The Charter Board's hall: what it held for them is paid out on entering, and contracts are
    // posted here.
    b.walk_to_place(place(PlaceKind::Charter), 240.0).await?;
    b.wait_until(5.0, "the rifle collected", |c| {
        c.hangar.view.as_ref().is_some_and(|v| v.stock.iter().any(|(i, q)| *i == RIFLE && *q == 1))
            && c.hangar.charter.as_ref().is_some_and(|v| v.waiting.is_none())
    })
    .await?;
    let text = done(&mut b, post).await?;
    assert!(text.contains("POSTED"), "{text}");
    let text = done(&mut b, Request::Contribute { work: Work::SecondFoundry, item: STEEL, qty: 100 }).await?;
    assert!(text.contains("A SECOND FOUNDRY"), "{text}");
    b.wait_until(5.0, "standing", |c| c.hangar.charter.as_ref().is_some_and(|v| v.standing > 0)).await?;
    let mine = b.core.hangar.charter.as_ref().and_then(|v| {
        v.contracts.iter().find(|c| c.mine && matches!(c.task, Task::Supply { item, .. } if item == STEEL))
    });
    let id = mine.map(|c| c.id).expect("their contract on the board");
    done(&mut b, Request::Withdraw { id }).await?;
    // Of standing now, but an ace's terms are The Arrival's.
    refused(&mut b, Request::AceTerms { salvage: true }, "the arrival's counter").await?;

    // Home, the Board's rifle on the Argo it came without.
    b.leave_city().await?;
    done(&mut b, Request::Fit { item: RIFLE }).await?;
    b.wait_until(5.0, "the rifle fitted", |c| {
        c.hangar
            .view
            .as_ref()
            .is_some_and(|v| matches!(&v.bay, Bay::Docked { suit } if suit.mounts == [true; 3]))
    })
    .await?;
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    b.close().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Once: back again, the Board has issued theirs, and holds nothing more for them.
    let mut b = BotClient::connect_as(&bot(&http, "Sally"), Some(&w)).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    b.enter_city(0).await?;
    b.request(&Request::WatchBoard { on: true }).await?;
    b.wait_until(5.0, "the board", |c| c.hangar.charter.is_some()).await?;
    b.run_for(Duration::from_secs(3), &mut |_| InputCmd::default()).await?;
    assert!(!b.core.hangar.news.iter().any(|n| n.contains(CERTIFIED)), "{:?}", b.core.hangar.news);
    assert_eq!(waiting(&b), None);
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    b.close().await;
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
