//! The Charter Board over real WebTransport (survival rules): the colony posts its supply
//! contracts; one pilot posts a contract of their own with the reward in escrow, another delivers
//! to it and is paid on the spot, and the goods wait for the first in their bay; a delivery to the
//! colony's great works pays at a premium and in standing; the charter can't be signed before its
//! works are finished. With a data directory the board outlives the server.

use std::path::PathBuf;

use bc_bot::{BotClient, BotConfig};
use bc_econ::charter::{COLONY_SUPPLY, Task, WORKS_PCT, Work};
use bc_econ::item::{Item, Material};
use bc_econ::wire::Request;
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};

fn config(data_dir: Option<PathBuf>) -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 0,
        max_clients: 8,
        data_dir,
        ..Config::default()
    })
}

fn bot(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Leo, faction: Faction::Colonies }
}

fn stock(b: &BotClient, item: Item) -> u64 {
    b.core.hangar.view.as_ref().map_or(0, |v| v.stock.iter().find(|(i, _)| *i == item).map_or(0, |(_, q)| *q))
}

async fn arrive(http: &str, name: &str) -> anyhow::Result<BotClient> {
    let mut b = BotClient::connect(&bot(http, name)).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    b.request(&Request::WatchBoard { on: true }).await?;
    b.wait_until(5.0, "the board", |c| {
        c.hangar
            .charter
            .as_ref()
            .is_some_and(|v| v.contracts.iter().filter(|c| c.colony).count() >= COLONY_SUPPLY)
    })
    .await?;
    Ok(b)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pilots_post_deliver_and_build_the_colony() -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("bc-charter-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let server = bc_server::start(config(Some(dir.clone()))?).await?;
    let http = format!("http://{}", server.http_addr);
    let steel = Item::Material(Material::Steel);

    let mut a = arrive(&http, "Heero").await?;
    let mut b = arrive(&http, "Duo").await?;
    let view = a.core.hangar.charter.clone().unwrap();
    assert_eq!(view.era, 0);
    assert_eq!(view.works.len(), Work::ALL.len());

    // Heero posts for 400 kg of steel, 2,000 cr in escrow.
    let c0 = a.core.hangar.credits();
    let text = a.ask(&Request::Post { item: steel, qty: 400, reward: 2_000, hours: 1 }).await?;
    assert!(text.contains("POSTED"), "{text}");
    a.wait_until(5.0, "the escrow", |c| c.hangar.credits() == c0 - 2_000).await?;
    b.wait_until(5.0, "the contract on Duo's board", |c| {
        c.hangar.charter.as_ref().is_some_and(|v| v.contracts.iter().any(|k| k.issuer == "Heero"))
    })
    .await?;
    let id =
        b.core.hangar.charter.as_ref().unwrap().contracts.iter().find(|k| k.issuer == "Heero").unwrap().id;

    // Duo delivers a quarter of it from the starter's steel.
    let d0 = b.core.hangar.credits();
    let text = b.ask(&Request::Deliver { id, qty: 100 }).await?;
    assert!(text.contains("PAID 500 CR"), "{text}");
    b.wait_until(5.0, "the pay", |c| c.hangar.credits() == d0 + 500).await?;
    assert!(a.ask(&Request::Deliver { id, qty: 1 }).await.is_err(), "not to one's own contract");
    // The steel waits for Heero in the bay.
    let s0 = stock(&a, steel);
    a.wait_until(5.0, "the steel", |c| {
        c.hangar.view.as_ref().is_some_and(|v| v.stock.iter().any(|(i, q)| *i == steel && *q == s0 + 100))
    })
    .await?;
    // Withdrawn, the rest of the escrow comes back.
    a.ask(&Request::Withdraw { id }).await?;
    a.wait_until(5.0, "the refund", |c| c.hangar.credits() == c0 - 500).await?;

    // The second foundry: 100 kg of steel, paid at the colony's value and a premium.
    let d1 = b.core.hangar.credits();
    let text = b.ask(&Request::Contribute { work: Work::SecondFoundry, item: steel, qty: 100 }).await?;
    assert!(text.contains("A SECOND FOUNDRY"), "{text}");
    let pay = bc_econ::catalogue::worth(steel, bc_econ::catalogue::value(steel), 100) * WORKS_PCT / 100;
    b.wait_until(5.0, "the colony's pay", |c| c.hangar.credits() == d1 + pay).await?;
    b.wait_until(5.0, "standing", |c| c.hangar.charter.as_ref().is_some_and(|v| v.standing == pay)).await?;
    assert!(b.ask(&Request::Sign).await.is_err(), "the vote isn't open yet");

    let status = server.status();
    assert_eq!(status["game"]["charter"]["era"], 0);
    assert_eq!(status["game"]["charter"]["works"]["second_foundry"]["kg"], 100);
    a.close().await;
    b.close().await;
    server.shutdown();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // The board outlives the server.
    let server = bc_server::start(config(Some(dir.clone()))?).await?;
    let http = format!("http://{}", server.http_addr);
    let c = arrive(&http, "Trowa").await?;
    let v = c.core.hangar.charter.as_ref().unwrap();
    let foundry = v.works.iter().find(|w| w.work == Work::SecondFoundry).unwrap();
    assert_eq!(foundry.needs.iter().find(|(i, _, _)| *i == steel).unwrap().2, 100);
    assert_eq!(foundry.top, vec![("Duo".to_string(), pay)]);
    assert!(v.contracts.iter().all(|k| matches!(k.task, Task::Supply { .. })));
    c.close().await;
    server.shutdown();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
