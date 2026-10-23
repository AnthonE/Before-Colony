//! Sessions without a flight slot, over real WebTransport (survival): a session has its own id
//! from its Hello, and holds one of the sector's flight slots (`--max-clients`) only while its
//! pilot's suit is out in space. More pilots than slots walk the city together; space holds as
//! many suits as there are slots, and a slot comes back when a suit docks, or once the tugs have
//! brought a pilot's wreck home.

use std::collections::HashSet;
use std::time::Duration;

use bc_bot::{BotClient, BotConfig};
use bc_client_core::city::pose_of;
use bc_client_core::walker::Walker;
use bc_econ::wire::{Place, Request};
use bc_proto::buttons::{FLIGHT_ASSIST, GRIP};
use bc_proto::presence::PersonPose;
use bc_proto::{Faction, FrameId, InputCmd};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::city::place_door;
use bc_sim::colony::frame::CityPos;
use bc_sim::content::city::PLACES;
use glam::Vec3;

fn config(slots: usize, colony: bool) -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        colony,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 0,
        max_clients: slots,
        ..Config::default()
    })
}

async fn arrive(http: &str, name: &str) -> anyhow::Result<BotClient> {
    let cfg = BotConfig {
        server: http.into(),
        name: name.into(),
        frame: FrameId::Argo,
        faction: Faction::Colonies,
    };
    let mut b = BotClient::connect(&cfg).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    Ok(b)
}

/// Standing `ahead` metres up the avenue from Hub Gate's door and `aside` across.
fn standing(ahead: f32, aside: f32) -> PersonPose {
    let ((s, x), (ds, dx)) = place_door(&PLACES[0]);
    let at = CityPos::new(0, x - dx * ahead, s - ds * ahead + aside, 0.0);
    let mut w = Walker::at(at.walker(), Vec3::X);
    w.grounded = true;
    pose_of(0, &w)
}

/// Hands off, the grip held: a suit in its bay's cradle stays there.
fn hold_on() -> InputCmd {
    InputCmd { buttons: GRIP | FLIGHT_ASSIST, ..InputCmd::default() }
}

/// Boards the suit in the bay and holds on in its cradle (the grip held), out in the sector.
async fn launch_holding(b: &mut BotClient) -> anyhow::Result<()> {
    b.request(&Request::Launch).await?;
    let mut hold = |_: &bc_client_core::InputContext| hold_on();
    for _ in 0..300 {
        b.step(&mut hold).await?;
        if b.place() == Some(Place::Space) && b.world().own.is_some_and(|o| o.alive) {
            // A few more, so the sector has heard the grip.
            for _ in 0..10 {
                b.step(&mut hold).await?;
            }
            return Ok(());
        }
    }
    anyhow::bail!("never launched: {:?}", b.core.hangar.notes.last())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn more_pilots_than_flight_slots_walk_the_city() -> anyhow::Result<()> {
    let server = bc_server::start(config(2, true)?).await?;
    let http = format!("http://{}", server.http_addr);
    let names = ["Kai", "Nico", "Rafi", "Tove", "Ines", "Jun"];
    let mut pilots = Vec::new();
    for (k, name) in names.iter().enumerate() {
        let mut b = arrive(&http, name).await?;
        b.enter_city(0).await?;
        b.set_pose(standing(8.0 + 2.0 * k as f32, 0.0));
        pilots.push(b);
    }
    // Each has an id of its own, none of them 0 nor the crowd's own place.
    let ids: HashSet<u16> = pilots.iter().map(|b| b.core.welcome.unwrap().client_slot).collect();
    assert_eq!(ids.len(), names.len(), "{ids:?}");
    assert!(ids.iter().all(|&id| id != 0 && id != u16::MAX), "{ids:?}");
    assert_eq!(server.status()["game"]["sessions"]["live"], names.len());
    // What the launcher reads: room for everyone connected, not only the suits flying.
    let doc: serde_json::Value = reqwest::get(format!("{http}/status.json")).await?.json().await?;
    assert_eq!(doc["max_players"], 512, "{doc}");

    // All six stand at Hub Gate and see the other five, by name.
    let end = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < end {
        for b in &mut pilots {
            b.step(&mut |_| InputCmd::default()).await?;
        }
    }
    for b in &pilots {
        let seen: HashSet<String> = b.people().into_iter().map(|(_, name, _)| name).collect();
        assert_eq!(seen.len(), names.len() - 1, "{seen:?}");
        assert!(seen.iter().all(|n| names.contains(&n.as_str())), "{seen:?}");
    }
    assert_eq!(server.status()["game"]["city"]["people"], names.len());

    // A pilot gone gives its id back.
    pilots.pop().unwrap().close().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(server.status()["game"]["sessions"]["live"], names.len() - 1);
    for b in pilots {
        b.close().await;
    }
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn space_holds_as_many_suits_as_flight_slots() -> anyhow::Result<()> {
    let server = bc_server::start(config(2, false)?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = arrive(&http, "Kai").await?;
    let mut b = arrive(&http, "Nico").await?;
    let mut c = arrive(&http, "Rafi").await?;

    // Kai holds on in the cradle, Nico flies out: both slots are taken.
    launch_holding(&mut a).await?;
    b.launch().await?;
    let why = c.ask(&Request::Launch).await.expect_err("space is full");
    assert!(why.to_string().contains("space is full"), "{why}");
    assert_eq!(c.place(), Some(Place::Hangar));
    assert!(
        matches!(c.core.hangar.view.as_ref().map(|v| &v.bay), Some(bc_econ::Bay::Docked { .. })),
        "the suit is back in its cradle"
    );

    // Kai docks from the cradle: the slot comes back, and Rafi flies.
    let mut hold = |_: &bc_client_core::InputContext| hold_on();
    a.step(&mut hold).await?;
    a.dock().await?;
    assert_eq!(a.place(), Some(Place::Hangar));
    // (The slot is free once the sector has let go of the suit: a tick or two.)
    tokio::time::sleep(Duration::from_millis(200)).await;
    c.launch().await?;
    assert_eq!(c.place(), Some(Place::Space));
    // And now Kai is the one turned away.
    let why = a.ask(&Request::Launch).await.expect_err("space is full again");
    assert!(why.to_string().contains("space is full"), "{why}");

    for p in [a, b, c] {
        p.close().await;
    }
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_tow_keeps_the_slot() -> anyhow::Result<()> {
    let server = bc_server::start(config(1, false)?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = arrive(&http, "Kai").await?;
    let mut b = arrive(&http, "Nico").await?;

    // Kai ejects: the wreck clears and Kai is back in the bay, but the tugs are out for it on
    // Kai's slot (what they bring home is Kai's, and nobody else's).
    a.launch().await?;
    a.eject().await?;
    a.wait_until(10.0, "back in the hangar", |c| c.hangar.in_hangar()).await?;
    let why = b.ask(&Request::Launch).await.expect_err("the tugs hold the slot");
    assert!(why.to_string().contains("space is full"), "{why}");

    // Kai goes: the wreck comes in now, and the slot with it.
    a.close().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    b.launch().await?;
    assert_eq!(b.place(), Some(Place::Space));
    b.close().await;
    server.shutdown();
    Ok(())
}
