//! Zodiac's aces over real WebTransport (survival rules; `docs/DESIGN.md`, "Aces"): an ace fielded
//! among the Dolls goes on every pilot's roster by its name, and on the Charter Board's Most Wanted
//! as out, with the news; a pilot sets their terms (the bounty, or its wreck). The sector's tests
//! (`bc-sector/tests/aces_net.rs`) down one and claim its wreck.

use std::time::Duration;

use bc_bot::{BotClient, BotConfig};
use bc_econ::wire::Request;
use bc_proto::{Faction, FrameId, PilotKind};
use bc_server::{Config, Mode, Ruleset};

fn bot(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Leo, faction: Faction::Colonies }
}

fn named(c: &bc_client_core::ClientCore, ace: &str) -> bool {
    c.world.roster.values().any(|(n, p)| n == ace && *p == PilotKind::MobileDoll)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_ace_out_is_named_on_the_roster_and_on_the_most_wanted() -> anyhow::Result<()> {
    let server = bc_server::start(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 2,
        ace_every: Duration::from_secs(1),
        max_clients: 4,
        ..Config::default()
    })
    .await?;
    let http = format!("http://{}", server.http_addr);

    let mut a = BotClient::connect(&bot(&http, "Wufei")).await?;
    a.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    a.request(&Request::WatchBoard { on: true }).await?;
    // On the roster by its name, a Doll still.
    a.wait_until(10.0, "ARIES on the roster", |c| named(c, "ARIES")).await?;
    // Out on the Most Wanted, and in the news.
    a.wait_until(10.0, "ARIES out on the board", |c| {
        c.hangar.charter.as_ref().is_some_and(|v| {
            v.wanted.first().is_some_and(|w| w.name == "ARIES" && w.out)
                && v.wanted.iter().filter(|w| w.out).count() == 1
        })
    })
    .await?;
    a.wait_until(5.0, "the news", |c| {
        c.hangar.news.iter().any(|n| n == "ZODIAC'S ARIES IS OUT AMONG THE DOLLS · 1,500 CR ON IT")
    })
    .await?;
    // Paid, until the pilot takes its wreck instead.
    assert!(!a.core.hangar.charter.as_ref().unwrap().salvage_terms);
    let text = a.ask(&Request::AceTerms { salvage: true }).await?;
    assert!(text.contains("WRECK"), "{text}");
    a.wait_until(5.0, "the terms on the board", |c| {
        c.hangar.charter.as_ref().is_some_and(|v| v.salvage_terms)
    })
    .await?;
    // A pilot who comes in later hears its name too.
    let mut b = BotClient::connect(&bot(&http, "Trowa")).await?;
    b.wait_until(5.0, "ARIES on the newcomer's roster", |c| named(c, "ARIES")).await?;
    a.close().await;
    b.close().await;
    server.shutdown();
    Ok(())
}
