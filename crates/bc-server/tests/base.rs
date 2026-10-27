//! The base's deck in the hub over real WebTransport (`docs/RING.md`, "The base"): under survival
//! rules (the old inside shut, as it is by default) two pilots step out of their bays' airlocks onto
//! Bay Row and each sees the other there; a suit can't be boarded from the row; and a pilot goes
//! back in only from their own airlock's door. `/status` counts them, never where.

use bc_bot::{BotClient, BotConfig};
use bc_econ::wire::Request;
use bc_proto::presence::{BASE_DECK, PersonPose};
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::deck::{DeckPos, out_of_the_airlock};

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

async fn out(http: &str, name: &str) -> anyhow::Result<BotClient> {
    let cfg = BotConfig {
        server: http.into(),
        name: name.into(),
        frame: FrameId::Argo,
        faction: Faction::Colonies,
    };
    let mut b = BotClient::connect(&cfg).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    b.enter_base().await?;
    Ok(b)
}

/// Standing on the deck at `at`.
fn standing(at: DeckPos) -> PersonPose {
    PersonPose { strip: BASE_DECK, x: at.x, s: at.s, h: at.h, grounded: true, ..PersonPose::default() }
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
async fn pilots_step_out_onto_bay_row_see_each_other_and_go_back_in_by_their_airlocks() -> anyhow::Result<()>
{
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = out(&http, "Maren").await?;
    let mut b = out(&http, "Pagan").await?;
    let (bay_a, bay_b) = (a.core.hangar.bay, b.core.hangar.bay);
    assert_ne!(bay_a, bay_b);
    a.set_pose(standing(out_of_the_airlock(bay_a)));
    b.set_pose(standing(out_of_the_airlock(bay_b)));
    both(&mut a, &mut b, 1.5).await?;

    // Each sees the other, by name, at their airlock: the next bay's along the row.
    let seen = a.people();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!((seen[0].1.as_str(), seen[0].2.strip), ("Pagan", BASE_DECK));
    let want = out_of_the_airlock(bay_b);
    assert!((seen[0].2.x - want.x).hypot(seen[0].2.s - want.s) < 0.05, "{:?} vs {want:?}", seen[0].2);
    assert_eq!(b.people().len(), 1);
    let status = server.status();
    assert_eq!(status["game"]["base"]["people"], 2);
    assert_eq!(status["game"]["city"]["people"], 0);
    let places: Vec<&str> =
        status["game"]["hangars"].as_array().unwrap().iter().filter_map(|h| h["place"].as_str()).collect();
    assert_eq!(places, ["base", "base"]);

    // No boarding from the row.
    let (text, ok) = ask(&mut a, Request::Launch).await?;
    assert!(!ok && text.contains("airlock"), "{text}");

    // In only by their own door: Maren, 9 m out across the row from it, can't; back at it, she can.
    // (Each a walk the plaza takes: no faster than anyone runs.)
    let door = out_of_the_airlock(bay_a);
    a.set_pose(standing(DeckPos { x: door.x + 9.0, ..door }));
    both(&mut a, &mut b, 1.5).await?;
    let (text, ok) = ask(&mut a, Request::LeaveBase).await?;
    assert!(!ok && text.contains("airlock"), "{text}");
    a.set_pose(standing(door));
    both(&mut a, &mut b, 1.5).await?;
    a.leave_base().await?;
    both(&mut a, &mut b, 1.5).await?;
    assert!(b.people().is_empty(), "{:?}", b.people());
    assert_eq!(server.status()["game"]["base"]["people"], 1);
    let (_, ok) = ask(&mut a, Request::LeaveBase).await?;
    assert!(!ok, "not out on the row any more");
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    Ok(())
}
