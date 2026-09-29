//! Survival rules over real WebTransport: a pilot starts on foot in their hangar with a worn Leo,
//! works the bay (fits and strips parts, trades on the exchange, queues the fabricator), launches
//! the suit, flies it back into the dock and docks, and finds it standing in the bay again. A
//! signed-in pilot's hangar is theirs: it's there when they come back, even after the server
//! restarts, when the server keeps its records in a data directory.

use std::path::PathBuf;
use std::time::Duration;

use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_client_core::Identity;
use bc_econ::Bay;
use bc_econ::exchange::Side;
use bc_econ::item::{Item, Material};
use bc_econ::suit::Slot;
use bc_econ::wire::{Outcome, Place, Request};
use bc_proto::buttons::FLIGHT_ASSIST;
use bc_proto::{Faction, FrameId, InputCmd, Part, WeaponKind};
use bc_server::{Config, Mode, Ruleset};

fn config(data_dir: Option<PathBuf>) -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 0,
        max_clients: 8,
        craft_speed: 20.0,
        data_dir,
        ..Config::default()
    })
}

fn bot(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Leo, faction: Faction::Colonies }
}

fn wallet(k: u8) -> LocalWallet {
    let mut secret = [0u8; 32];
    secret[31] = k;
    LocalWallet::from_secret(&secret).expect("a valid key")
}

/// The suit standing in the bay, as the pilot was last told.
fn docked(b: &BotClient) -> Option<bc_econ::Suit> {
    match &b.core.hangar.view.as_ref()?.bay {
        Bay::Docked { suit } => Some(suit.clone()),
        _ => None,
    }
}

async fn ask(b: &mut BotClient, req: Request) -> anyhow::Result<String> {
    let n = b.core.hangar.notes.len();
    b.request(&req).await?;
    b.wait_until(5.0, "an answer", |c| c.hangar.notes.len() > n).await.or_else(|e| {
        // A refusal is an answer too.
        let (why, _) = b.core.hangar.notes.last().cloned().unwrap_or_default();
        if b.core.hangar.notes.len() > n { Ok(()) } else { Err(e.context(why)) }
    })?;
    let (text, ok) = b.core.hangar.notes[n].clone();
    anyhow::ensure!(ok, "{req:?} refused: {text}");
    Ok(text)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pilot_works_the_bay_launches_and_docks() -> anyhow::Result<()> {
    let server = bc_server::start(config(None)?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut b = BotClient::connect(&bot(&http, "Duo")).await?;
    assert!(b.survival());
    b.wait_until(5.0, "the hangar", |c| {
        c.hangar.in_hangar() && c.hangar.view.is_some() && c.hangar.market.is_some()
    })
    .await?;
    // The starter kit: a worn Leo with no beam rifle, and 2,000 credits.
    let suit = docked(&b).expect("a suit in the bay");
    assert_eq!(suit.line, FrameId::Leo);
    assert_eq!(suit.mounts, [false, true, true]);
    assert_eq!(b.core.hangar.credits(), 2_000);
    let status = server.status();
    assert_eq!(status["game"]["rules"], "survival");
    assert_eq!(status["game"]["hangars"][0]["place"], "hangar");

    // The bay: the left arm comes off with its weapons, and goes back on.
    let text = ask(&mut b, Request::Strip { slot: Slot::Part { part: Part::ArmL } }).await?;
    assert!(text.contains("OFF"), "{text}");
    assert_eq!(docked(&b).unwrap().mounts, [false, false, false]);
    ask(&mut b, Request::Fit { item: Item::Part(FrameId::Leo, Part::ArmL) }).await?;
    ask(&mut b, Request::Fit { item: Item::Weapon(WeaponKind::MachineCannon) }).await?;
    ask(&mut b, Request::Fit { item: Item::Weapon(WeaponKind::BeamSaber) }).await?;
    assert_eq!(docked(&b).unwrap().mounts, [false, true, true]);
    // A refusal says why.
    assert!(ask(&mut b, Request::Fit { item: Item::Weapon(WeaponKind::BeamRifle) }).await.is_err());

    // The exchange: steel sold to the colony pays; an order above its bid waits in the book.
    let steel = Item::Material(Material::Steel);
    ask(&mut b, Request::Order { item: steel, side: Side::Sell, price: 1, qty: 100, rest: false }).await?;
    assert!(b.core.hangar.credits() > 2_000);
    ask(&mut b, Request::Order { item: steel, side: Side::Sell, price: 90_000, qty: 50, rest: true }).await?;
    b.wait_until(5.0, "the order on the market", |c| {
        c.hangar.market.as_ref().is_some_and(|m| m.orders.iter().any(|o| o.item == steel && o.qty == 50))
    })
    .await?;
    let id = b.core.hangar.market.as_ref().unwrap().orders[0].id;
    ask(&mut b, Request::CancelOrder { id }).await?;

    // The fabricator wants volatiles for munitions, and there are none; the colony sells
    // propellant, though.
    assert!(
        ask(&mut b, Request::Craft { item: Item::Material(Material::Munitions), batches: 1 }).await.is_err()
    );
    let propellant = Item::Material(Material::Propellant);
    ask(&mut b, Request::Order { item: propellant, side: Side::Buy, price: 3_000, qty: 200, rest: false })
        .await?;

    // Launch: out of the hub, flying.
    b.launch().await?;
    assert_eq!(b.place(), Some(Place::Space));
    let own = b.world().own.expect("own suit");
    assert!(own.alive && own.frame == FrameId::Leo);
    assert_eq!(own.weapon_ready & 0b001, 0, "no beam rifle fitted");
    // Brake to rest in the dock, and dock.
    for _ in 0..100 {
        b.step(&mut |_| InputCmd { buttons: FLIGHT_ASSIST, ..InputCmd::default() }).await?;
    }
    b.dock().await?;
    assert_eq!(b.place(), Some(Place::Hangar));
    let (outcome, _) = b.core.hangar.sorties.last().cloned().expect("a sortie");
    assert_eq!(outcome, Outcome::Docked);
    let back = docked(&b).expect("the suit is back in the bay");
    assert_eq!(back.parts, suit.parts, "nothing hit it");
    assert!(back.propellant > 0 && back.propellant <= back.tank());
    b.close().await;
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_signed_in_pilots_hangar_outlives_the_server() -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("bc-hangar-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let w = wallet(9);
    {
        let server = bc_server::start(config(Some(dir.clone()))?).await?;
        let http = format!("http://{}", server.http_addr);
        let mut b = BotClient::connect_with(
            &bot(&http, "Heero"),
            Identity::Wallet { address: w.address(), resume: None },
            Some(&w),
        )
        .await?;
        b.wait_until(5.0, "the hangar", |c| c.hangar.view.is_some()).await?;
        // The head off, and melted down.
        ask(&mut b, Request::Strip { slot: Slot::Part { part: Part::Head } }).await?;
        ask(&mut b, Request::Scrap { item: Item::Part(FrameId::Leo, Part::Head) }).await?;
        assert!(docked(&b).unwrap().parts[Part::Head as usize].is_none());
        b.close().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        server.shutdown();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Another run of the server, the same data directory.
    let server = bc_server::start(config(Some(dir.clone()))?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut b = BotClient::connect_with(
        &bot(&http, "Heero"),
        Identity::Wallet { address: w.address(), resume: None },
        Some(&w),
    )
    .await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.view.is_some()).await?;
    let suit = docked(&b).expect("the suit is still in the bay");
    assert!(suit.parts[Part::Head as usize].is_none(), "still without its head");
    assert!(
        b.core.hangar.view.as_ref().unwrap().stock.iter().any(|(i, _)| *i == Item::Material(Material::Steel))
    );
    b.close().await;
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_suit_left_out_there_is_woken_in_or_towed_home() -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("bc-hangar-out-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let w = wallet(10);
    let me = Identity::Wallet { address: w.address(), resume: None };
    {
        let server = bc_server::start(config(Some(dir.clone()))?).await?;
        let http = format!("http://{}", server.http_addr);
        let mut b = BotClient::connect_with(&bot(&http, "Trowa"), me, Some(&w)).await?;
        b.wait_until(5.0, "the hangar", |c| c.hangar.view.is_some()).await?;
        b.launch().await?;
        let slot = b.world().own.unwrap().slot;
        // Gone mid-sortie: the suit sleeps out there, and the bay says it's out.
        b.drop_link();
        tokio::time::sleep(Duration::from_millis(500)).await;
        let mut b = BotClient::connect_with(&bot(&http, "Trowa"), me, Some(&w)).await?;
        assert!(b.core.welcome.unwrap().woke);
        b.wait_until(5.0, "back in the cockpit", |c| {
            c.hangar.place == Some(Place::Space) && c.world.own.is_some()
        })
        .await?;
        assert_eq!(b.world().own.unwrap().slot, slot, "the same suit");
        assert!(matches!(b.core.hangar.view.as_ref().unwrap().bay, Bay::Out { .. }));
        b.drop_link();
        tokio::time::sleep(Duration::from_millis(500)).await;
        server.shutdown();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    // The server restarted: the sleeper is gone, and the colony's tugs brought the suit in.
    let server = bc_server::start(config(Some(dir.clone()))?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut b = BotClient::connect_with(&bot(&http, "Trowa"), me, Some(&w)).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    assert_eq!(b.core.hangar.sorties.last().map(|s| s.0), Some(Outcome::Recovered));
    assert!(docked(&b).is_some(), "standing in the bay");
    b.close().await;
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
