//! Waking where you left the ring's town (`docs/FREE_ROAM.md`, O1), over real WebTransport and two
//! server restarts: a signed-in pilot who logs out down in the town wakes there next time, on foot
//! where they stood (and the server takes them as there); one whose spot is in a wall wakes out of
//! it; and one who rode a lift up to the bay before logging out wakes in the bay, as ever.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_econ::wire::Request;
use bc_proto::presence::{PersonPose, RING_DECK};
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::deck::{self, RING};
use bc_sim::colony::town;

fn config(data_dir: PathBuf) -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 0,
        max_clients: 8,
        data_dir: Some(data_dir),
        ..Config::default()
    })
}

fn wallet() -> LocalWallet {
    let mut secret = [0u8; 32];
    secret[31] = 91;
    LocalWallet::from_secret(&secret).expect("a valid key")
}

async fn connect(http: &str) -> anyhow::Result<BotClient> {
    let cfg = BotConfig {
        server: http.into(),
        name: "Maren".into(),
        frame: FrameId::Argo,
        faction: Faction::Colonies,
    };
    BotClient::connect_as(&cfg, Some(&wallet())).await
}

/// Standing on the town's floor at `x` across and `s` round the ring's deck.
fn standing(x: f32, s: f32) -> PersonPose {
    PersonPose { strip: RING_DECK, x, s, grounded: true, ..PersonPose::default() }
}

/// `dx` across and `ds` round from the step out of Hub Gate's lift.
fn by_the_lift(dx: f32, ds: f32) -> PersonPose {
    let (x, t) = town::out_of_the_lift(0);
    standing(x + dx, town::deck_s(t) + ds)
}

async fn run(b: &mut BotClient, secs: f64) -> anyhow::Result<()> {
    b.run_for(Duration::from_secs_f64(secs), &mut |_| Default::default()).await
}

/// Asks, and waits for the note that answers: its text, and whether it was done.
async fn ask(b: &mut BotClient, req: Request) -> anyhow::Result<(String, bool)> {
    let n = b.core.hangar.notes.len();
    b.request(&req).await?;
    b.wait_until(5.0, "an answer", |c| c.hangar.notes.len() > n).await?;
    Ok(b.core.hangar.notes[n].clone())
}

/// Where the store keeps the pilot's record.
fn record_file(dir: &Path) -> PathBuf {
    let key = bc_server::pilots::key(&wallet().address().into());
    dir.join("pilots").join(format!("{key}.json"))
}

/// The pilot's record, as the store keeps it.
fn record(dir: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&std::fs::read(record_file(dir)).ok()?).ok()
}

/// Waits for the record to keep where in the town they left it.
async fn left_in_town(dir: &Path) -> anyhow::Result<(f32, f32)> {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(t) = record(dir).and_then(|r| r.get("town").cloned()) {
            return Ok((t["x"].as_f64().unwrap_or(f64::NAN) as f32, t["s"].as_f64().unwrap_or(f64::NAN) as f32));
        }
        anyhow::ensure!(Instant::now() < end, "the record doesn't keep the town: {:?}", record(dir));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Where the place's wake has the pilot get up in the town: `(x, s)`.
fn woke_at(b: &BotClient) -> (f32, f32) {
    let w = b.core.hangar.wake.expect("a wake");
    assert_eq!(w.home, town::WAKE_IN_TOWN, "{w:?}");
    let (x, t) = town::unpack_spot((w.x, w.s));
    (x, town::deck_s(t))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pilot_who_logs_out_in_the_town_wakes_there_and_one_who_rode_up_wakes_in_the_bay() -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("bc-town-wake-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    // Down the lift, and out across Hub Gate's square, 50 m fore of its door; logged out there.
    let server = bc_server::start(config(dir.clone())?).await?;
    let mut b = connect(&format!("http://{}", server.http_addr)).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    b.enter_town().await?;
    for k in 0..=5 {
        b.set_pose(by_the_lift(-10.0 * k as f32, 3.0));
        run(&mut b, 1.0).await?;
    }
    let there = by_the_lift(-50.0, 3.0);
    b.close().await;
    let (x, s) = left_in_town(&dir).await?;
    assert!((x - there.x).abs() < 1e-3 && (s - there.s).abs() < 1e-2, "kept at ({x}, {s})");
    server.shutdown();

    // After a restart they wake there, on foot: the server has them there, too far from a lift to
    // ride it up.
    let server = bc_server::start(config(dir.clone())?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut b = connect(&http).await?;
    b.wait_until(5.0, "the town", |c| c.hangar.in_town() && c.hangar.wake.is_some()).await?;
    let (wx, ws) = woke_at(&b);
    assert!((wx - there.x).abs() < 0.05 && deck::round_from(RING, there.s, ws).abs() < 0.2, "({wx}, {ws})");
    assert!(record(&dir).is_some_and(|r| r.get("town").is_none()), "awake: no longer kept there");
    let status = server.status();
    assert_eq!(status["game"]["town"]["people"], 1);
    assert_eq!(status["game"]["hangars"][0]["place"], "town");
    b.set_pose(standing(wx, ws));
    run(&mut b, 1.0).await?;
    let (text, ok) = ask(&mut b, Request::LeaveTown).await?;
    assert!(!ok && text.contains("lift"), "{text}");
    // Logged out there again, the record says so; and while they're away it comes to say they
    // were inside the lift's pier.
    b.close().await;
    left_in_town(&dir).await?;
    server.shutdown();
    let mut r = record(&dir).expect("the record");
    let (door_x, door_t) = town::lift_door(0);
    let wall = (door_x + 0.5, town::deck_s(door_t));
    assert!(!town::stands_clear(wall.0, door_t));
    r["town"] = serde_json::json!({ "x": wall.0, "s": wall.1 });
    std::fs::write(record_file(&dir), serde_json::to_vec(&r)?)?;

    // They wake out of the pier, by the lift's door; ride up; log out in the bay; and wake there.
    let server = bc_server::start(config(dir.clone())?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut b = connect(&http).await?;
    b.wait_until(5.0, "the town", |c| c.hangar.in_town() && c.hangar.wake.is_some()).await?;
    let (wx, ws) = woke_at(&b);
    assert!(town::stands_clear(wx, town::town_t(ws)), "({wx}, {ws}) is in a wall");
    assert!((wx - wall.0).hypot(deck::round_from(RING, wall.1, ws)) < 3.0, "({wx}, {ws})");
    b.set_pose(standing(wx, ws));
    run(&mut b, 1.0).await?;
    b.leave_town().await?;
    b.close().await;
    let mut b = connect(&http).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    assert_eq!(b.core.hangar.wake, None);
    assert_eq!(server.status()["game"]["town"]["people"], 0);
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    b.close().await;
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
