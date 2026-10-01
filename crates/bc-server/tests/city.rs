//! The colony's inside over real WebTransport. With the colony open (`--colony`) a pilot rides the
//! cap lift down from their bay into a strip's city and back up, trades on the exchange from its
//! Exchange floor while they're there, and can't launch from it; `/status` follows them. Without
//! it, the lifts are closed.

use std::time::Duration;

use bc_bot::{BotClient, BotConfig};
use bc_econ::exchange::Side;
use bc_econ::item::{Item, Material};
use bc_econ::wire::Request;
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};

fn config(colony: bool) -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 0,
        max_clients: 8,
        colony,
        ..Config::default()
    })
}

fn bot(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Leo, faction: Faction::Colonies }
}

/// Asks, and waits for the note that answers: its text, and whether it was done.
async fn ask(b: &mut BotClient, req: Request) -> anyhow::Result<(String, bool)> {
    let n = b.core.hangar.notes.len();
    b.request(&req).await?;
    b.wait_until(5.0, "an answer", |c| c.hangar.notes.len() > n).await?;
    Ok(b.core.hangar.notes[n].clone())
}

async fn in_the_bay(b: &mut BotClient) -> anyhow::Result<()> {
    b.wait_until(5.0, "the hangar", |c| {
        c.hangar.in_hangar() && c.hangar.view.is_some() && c.hangar.market.is_some()
    })
    .await
}

/// Where `/status` has the pilot, once it says so (it's told a moment after the pilot is).
async fn status_place(server: &bc_server::ServerHandle, want: &str) -> String {
    let mut place = String::new();
    for _ in 0..40 {
        place = server.status()["game"]["hangars"][0]["place"].as_str().unwrap_or_default().to_string();
        if place == want {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    place
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pilot_rides_down_into_the_colony_trades_and_rides_back() -> anyhow::Result<()> {
    let server = bc_server::start(config(true)?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut b = BotClient::connect(&bot(&http, "Relena")).await?;
    in_the_bay(&mut b).await?;
    assert!(b.core.welcome.as_ref().is_some_and(|w| w.colony), "the Welcome says the colony's open");
    assert_eq!(server.status()["game"]["colony"], true);

    // Down the lift to the Canal strip's Hub Gate.
    b.request(&Request::EnterCity { strip: 1 }).await?;
    b.wait_until(5.0, "the city", |c| c.hangar.in_city()).await?;
    assert_eq!(b.core.hangar.strip, Some(1));
    assert_eq!(status_place(&server, "city").await, "city");

    // The Exchange floor trades on the same book as the bay's terminal.
    let steel = Item::Material(Material::Steel);
    let order = Request::Order { item: steel, side: Side::Sell, price: 1, qty: 100, rest: false };
    let (text, ok) = ask(&mut b, order).await?;
    assert!(ok, "{text}");
    assert!(b.core.hangar.credits() > 2_000);

    // A suit can't be boarded from down here.
    let (text, ok) = ask(&mut b, Request::Launch).await?;
    assert!(!ok && text.contains("lift"), "{text}");
    assert!(b.core.hangar.in_city());

    // Back up to the bay.
    b.request(&Request::LeaveCity).await?;
    b.wait_until(5.0, "the bay again", |c| c.hangar.in_hangar()).await?;
    assert_eq!(b.core.hangar.strip, None);
    assert_eq!(status_place(&server, "hangar").await, "hangar");
    let (_, ok) = ask(&mut b, Request::LeaveCity).await?;
    assert!(!ok, "not in the colony any more");
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn without_the_colony_its_lifts_are_closed() -> anyhow::Result<()> {
    let server = bc_server::start(config(false)?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut b = BotClient::connect(&bot(&http, "Dorothy")).await?;
    in_the_bay(&mut b).await?;
    assert!(b.core.welcome.as_ref().is_some_and(|w| !w.colony));
    assert_eq!(server.status()["game"]["colony"], false);
    let (text, ok) = ask(&mut b, Request::EnterCity { strip: 0 }).await?;
    assert!(!ok && text.contains("closed"), "{text}");
    assert!(b.core.hangar.in_hangar());
    Ok(())
}
