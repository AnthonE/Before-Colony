//! The ring's town over real WebTransport (`docs/FREE_ROAM.md`): under survival rules two pilots
//! ride a spoke's lift down from their bays to Hub Gate's square and each sees the other there; one
//! takes a car from the street and drives off; a suit can't be boarded from the town; and a pilot
//! rides back up only from a lift's door, on foot. `/status` counts them, never where.

use bc_bot::{BotClient, BotConfig};
use bc_econ::wire::Request;
use bc_proto::presence::{PersonPose, RIDE_CAR, RING_DECK};
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::town;

fn config() -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 0,
        max_clients: 8,
        ..Config::default()
    })
}

async fn down(http: &str, name: &str) -> anyhow::Result<BotClient> {
    let cfg = BotConfig {
        server: http.into(),
        name: name.into(),
        frame: FrameId::Argo,
        faction: Faction::Colonies,
    };
    let mut b = BotClient::connect(&cfg).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    b.enter_town().await?;
    Ok(b)
}

/// Standing in the town `dx` m across and `ds` m round from the step out of Hub Gate's lift.
fn standing(dx: f32, ds: f32) -> PersonPose {
    let (x, t) = town::out_of_the_lift(0);
    PersonPose {
        strip: RING_DECK,
        x: x + dx,
        s: town::deck_s(t) + ds,
        grounded: true,
        ..PersonPose::default()
    }
}

/// Steps both for `secs`.
async fn both(a: &mut BotClient, b: &mut BotClient, secs: f64) -> anyhow::Result<()> {
    let end = std::time::Instant::now() + std::time::Duration::from_secs_f64(secs);
    while std::time::Instant::now() < end {
        a.step(&mut |_| Default::default()).await?;
        b.step(&mut |_| Default::default()).await?;
    }
    Ok(())
}

/// Asks, and waits for the note that answers: its text, and whether it was done.
async fn ask(b: &mut BotClient, req: Request) -> anyhow::Result<(String, bool)> {
    let n = b.core.hangar.notes.len();
    b.request(&req).await?;
    b.wait_until(5.0, "an answer", |c| c.hangar.notes.len() > n).await?;
    Ok(b.core.hangar.notes[n].clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pilots_ride_a_spoke_down_to_the_town_see_each_other_drive_and_ride_back_up() -> anyhow::Result<()> {
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = down(&http, "Maren").await?;
    let mut b = down(&http, "Pagan").await?;
    a.set_pose(standing(0.0, 0.0));
    b.set_pose(standing(-3.0, 2.0));
    both(&mut a, &mut b, 1.5).await?;

    // Each sees the other, by name, in Hub Gate's square.
    let seen = a.people();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!((seen[0].1.as_str(), seen[0].2.strip), ("Pagan", RING_DECK));
    let want = standing(-3.0, 2.0);
    assert!((seen[0].2.x - want.x).hypot(seen[0].2.s - want.s) < 0.05, "{:?} vs {want:?}", seen[0].2);
    assert_eq!(b.people().len(), 1);
    let status = server.status();
    assert_eq!(status["game"]["town"]["people"], 2);
    assert_eq!(status["game"]["base"]["people"], 0);
    assert_eq!(status["game"]["city"]["people"], 0);
    let places: Vec<&str> =
        status["game"]["hangars"].as_array().unwrap().iter().filter_map(|h| h["place"].as_str()).collect();
    assert_eq!(places, ["town", "town"]);

    // No boarding from the town.
    let (text, ok) = ask(&mut a, Request::Launch).await?;
    assert!(!ok && text.contains("lift"), "{text}");

    // Pagan takes a car from the street and drives off along the square: Maren sees them at the
    // wheel, further off each moment, faster than anyone runs.
    b.set_pose(PersonPose { ride: RIDE_CAR, ..standing(-3.0, 4.0) });
    both(&mut a, &mut b, 0.6).await?;
    for k in 1..=5 {
        b.set_pose(PersonPose { ride: RIDE_CAR, speed: 12.0, ..standing(-3.0 - 12.0 * k as f32, 4.0) });
        both(&mut a, &mut b, 0.6).await?;
    }
    let seen = a.people();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].2.driving(), "{:?}", seen[0].2);
    assert!((seen[0].2.x - standing(-63.0, 4.0).x).abs() < 0.5, "{:?}", seen[0].2);
    // At the wheel, 60 m off, the lift doesn't take them.
    let (text, ok) = ask(&mut b, Request::LeaveTown).await?;
    assert!(!ok && text.contains("lift"), "{text}");

    // Maren, a walk away from the lift's door, can't ride up; back at it, she can.
    for k in 1..=3 {
        a.set_pose(standing(-5.0 * k as f32, 0.0));
        both(&mut a, &mut b, 1.0).await?;
    }
    let (text, ok) = ask(&mut a, Request::LeaveTown).await?;
    assert!(!ok && text.contains("lift"), "{text}");
    for k in (0..3).rev() {
        a.set_pose(standing(-5.0 * k as f32, 0.0));
        both(&mut a, &mut b, 1.0).await?;
    }
    a.leave_town().await?;
    both(&mut a, &mut b, 1.5).await?;
    assert!(b.people().is_empty(), "{:?}", b.people());
    assert_eq!(server.status()["game"]["town"]["people"], 1);
    let (_, ok) = ask(&mut a, Request::LeaveTown).await?;
    assert!(!ok, "not down in the town any more");
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    Ok(())
}
