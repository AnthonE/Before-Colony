//! Living in the colony over real WebTransport (survival, the colony open; `docs/LIFE.md`): the
//! pilot's body is on their record and runs on the wall clock, a quarter as fast while they're
//! away. The Arrival's counter feeds them from the bay's credits (the bay's terminal points there),
//! never more than they can eat; and sleep never starves anyone. How fed they are reaches the suit
//! they fly, in either sector: a starving pilot bears a G less, and a meal puts it right.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_econ::body::{ARRIVAL_FOOD_S, FULL_S};
use bc_econ::food::{Dish, HOUR};
use bc_econ::wire::Request;
use bc_proto::buttons::FLIGHT_ASSIST;
use bc_proto::{Faction, FrameId, InputCmd};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::interior::{INNER_GATE, INNER_GATE_RADIUS};
use bc_sim::content::body::{Fed, STARVING_G};
use bc_sim::content::city::{PLACES, PlaceKind};
use bc_sim::tuning::own_tuning;
use glam::Vec3;

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

fn wallet(k: u8) -> LocalWallet {
    let mut secret = [0u8; 32];
    secret[31] = k;
    LocalWallet::from_secret(&secret).expect("a valid key")
}

/// Key place `kind`'s index in `PLACES`.
fn place(kind: PlaceKind) -> usize {
    PLACES.iter().position(|p| p.kind == kind).expect("a key place")
}

fn data_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bc-life-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// The signed-in pilot's record in `dir`, as JSON, and where it is.
fn record(dir: &Path) -> anyhow::Result<(PathBuf, serde_json::Value)> {
    let file = std::fs::read_dir(dir.join("pilots"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "json"))
        .expect("the pilot's record");
    let rec = serde_json::from_slice(&std::fs::read(&file)?)?;
    Ok((file, rec))
}

/// Edits the record's body in `dir` between two server runs (the server holds it while the pilot
/// is in).
fn edit_body(dir: &Path, edit: impl FnOnce(&mut serde_json::Value)) -> anyhow::Result<()> {
    let (file, mut rec) = record(dir)?;
    edit(&mut rec["body"]);
    std::fs::write(&file, serde_json::to_vec(&rec)?)?;
    Ok(())
}

fn credits(b: &BotClient) -> u64 {
    b.core.hangar.view.as_ref().map_or(0, |v| v.credits)
}

/// Asks, and waits for the note that answers: its text, and whether it was done.
async fn ask(b: &mut BotClient, req: Request) -> anyhow::Result<(String, bool)> {
    let n = b.core.hangar.notes.len();
    b.request(&req).await?;
    b.wait_until(5.0, "an answer", |c| c.hangar.notes.len() > n).await?;
    Ok(b.core.hangar.notes[n].clone())
}

async fn refused(b: &mut BotClient, req: Request, why: &str) -> anyhow::Result<()> {
    let (text, ok) = ask(b, req.clone()).await?;
    anyhow::ensure!(!ok && text.contains(why), "{req:?}: {ok} {text}");
    Ok(())
}

async fn done(b: &mut BotClient, req: Request) -> anyhow::Result<String> {
    let (text, ok) = ask(b, req.clone()).await?;
    anyhow::ensure!(ok, "{req:?} refused: {text}");
    Ok(text)
}

/// Signs in to a server on `dir` and waits for the bay and the body.
async fn come_in(dir: &Path, w: &LocalWallet) -> anyhow::Result<(bc_server::ServerHandle, BotClient)> {
    let server = bc_server::start(config(Some(dir.to_path_buf()))?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut b = BotClient::connect_as(&bot(&http, "Nell"), Some(w)).await?;
    b.wait_until(5.0, "the hangar and the body", |c| {
        c.hangar.in_hangar() && c.hangar.view.is_some() && c.hangar.body.is_some()
    })
    .await?;
    Ok((server, b))
}

/// Logs out and stops the server, so the record on disk is the last word.
async fn go(server: bc_server::ServerHandle, b: BotClient) {
    b.close().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    server.shutdown();
    tokio::time::sleep(Duration::from_millis(300)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pilot_eats_at_the_arrival_and_wakes_peckish() -> anyhow::Result<()> {
    let dir = data_dir("eats");
    let w = wallet(92);
    let (server, mut b) = come_in(&dir, &w).await?;
    // An Arrival comes in fed, a meal's worth in them.
    let body = b.core.hangar.body.expect("the body");
    assert_eq!(body.fed, Fed::Fed);
    assert!(body.food_s <= ARRIVAL_FOOD_S && body.food_s + 10 >= ARRIVAL_FOOD_S, "{body:?}");

    // The bay's terminal points at The Arrival's counter.
    refused(&mut b, Request::Eat { dish: Dish::Noodles }, "the arrival's counter").await?;

    // At the counter: a noodle bowl, paid for, fills them to the most they hold.
    b.enter_city(0).await?;
    b.walk_to_place(place(PlaceKind::Bar), 360.0).await?;
    let before = credits(&b);
    b.core.hangar.body = None;
    let text = done(&mut b, Request::Eat { dish: Dish::Noodles }).await?;
    assert_eq!(text, "NOODLE BOWL · 25 CR · FED");
    b.wait_until(5.0, "the bill and the body", |c| {
        c.hangar.view.as_ref().is_some_and(|v| v.credits + 25 == before) && c.hangar.body.is_some()
    })
    .await?;
    assert_eq!(b.core.hangar.body.map(|v| v.food_s), Some(FULL_S));
    // Not another bite, and nothing charged for it.
    refused(&mut b, Request::Eat { dish: Dish::FishSupper }, "another bite").await?;
    b.run_for(Duration::from_millis(500), &mut |_| bc_proto::InputCmd::default()).await?;
    assert_eq!(credits(&b), before - 25);
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    go(server, b).await;

    // On the record, asleep. Thirty hours away burn seven and a half of the eight in them.
    let (_, rec) = record(&dir)?;
    assert_eq!(rec["body"]["awake"], false, "{}", rec["body"]);
    edit_body(&dir, |body| {
        let at = body["at"].as_u64().expect("the body's clock");
        body["at"] = serde_json::json!(at - 30 * u64::from(HOUR));
    })?;
    let (server, b) = come_in(&dir, &w).await?;
    let body = b.core.hangar.body.expect("the body");
    assert_eq!(body.fed, Fed::Peckish, "{body:?}");
    let left = FULL_S - 30 * HOUR / 4;
    assert!(body.food_s <= left && body.food_s + 10 >= left, "{body:?}");
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    go(server, b).await;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sleep_never_starves_anyone() -> anyhow::Result<()> {
    let dir = data_dir("sleep");
    let w = wallet(93);
    let (server, b) = come_in(&dir, &w).await?;
    go(server, b).await;
    // Ten minutes of food left, and a week away.
    edit_body(&dir, |body| {
        let at = body["at"].as_u64().expect("the body's clock");
        body["food_s"] = serde_json::json!(600);
        body["at"] = serde_json::json!(at - 7 * 24 * u64::from(HOUR));
    })?;
    let (server, b) = come_in(&dir, &w).await?;
    let body = b.core.hangar.body.expect("the body");
    assert_eq!((body.fed, body.food_s), (Fed::Hungry, 0), "hungry, never starving: {body:?}");
    go(server, b).await;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Inside the colony: back to the inner gate on flight assist, to rest in its ring, and docked.
async fn dock_at_the_inner_gate(b: &mut BotClient) -> anyhow::Result<()> {
    for _ in 0..30 * 30 {
        let own = b.world().own.expect("own suit");
        let to = INNER_GATE - own.pos;
        if to.length() < INNER_GATE_RADIUS * 0.5 && own.vel.length() < 3.0 {
            break;
        }
        let speed = (to.length() * 0.3).min(40.0);
        let want = to.normalize_or_zero() * speed;
        let local = own.rot.conjugate() * (want - own.vel);
        let q = |v: f32| (v * 6.0).clamp(-127.0, 127.0) as i8;
        b.step(&mut |_| InputCmd {
            buttons: FLIGHT_ASSIST,
            thrust: [q(local.x), q(local.y), q(local.z)],
            aim: Vec3::X,
            ..InputCmd::default()
        })
        .await?;
    }
    for _ in 0..60 {
        b.step(&mut |_| InputCmd { buttons: FLIGHT_ASSIST, aim: Vec3::X, ..InputCmd::default() }).await?;
    }
    b.dock().await?;
    b.wait_until(5.0, "the bay", |c| c.hangar.in_hangar() && c.welcome.is_some_and(|w| !w.interior)).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hungry_pilot_flies_short_of_g_and_a_meal_puts_it_right() -> anyhow::Result<()> {
    let dir = data_dir("flies");
    let w = wallet(94);
    let (server, b) = come_in(&dir, &w).await?;
    go(server, b).await;
    // Nothing in them, and three hours awake on an empty stomach.
    edit_body(&dir, |body| {
        body["food_s"] = serde_json::json!(0);
        body["empty_s"] = serde_json::json!(3 * HOUR);
    })?;
    let (server, mut b) = come_in(&dir, &w).await?;
    assert_eq!(b.core.hangar.fed(), Some(Fed::Starving));

    // Into the colony through the inner gate: the inside's sector has the pilot starving.
    b.launch_inside().await?;
    b.wait_until(5.0, "starving in the cockpit", |c| {
        c.world.own.is_some_and(|o| o.fed == Fed::Starving as u8)
    })
    .await?;
    let starving = own_tuning(&b.world().own.expect("own suit")).g_tolerance;
    dock_at_the_inner_gate(&mut b).await?;

    // Down to The Arrival for a fish supper, and back up.
    b.enter_city(0).await?;
    b.walk_to_place(place(PlaceKind::Bar), 360.0).await?;
    let text = done(&mut b, Request::Eat { dish: Dish::FishSupper }).await?;
    assert_eq!(text, "CANAL FISH SUPPER · 40 CR · FED");
    b.wait_until(5.0, "fed", |c| c.hangar.fed() == Some(Fed::Fed)).await?;
    b.leave_city().await?;

    // Out into space: fed, and the G back.
    b.launch().await?;
    b.wait_until(5.0, "fed in the cockpit", |c| c.world.own.is_some_and(|o| o.fed == Fed::Fed as u8)).await?;
    let fed = own_tuning(&b.world().own.expect("own suit")).g_tolerance;
    assert!((fed + STARVING_G - starving).abs() < 1e-5, "starving {starving} g, fed {fed} g");
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    go(server, b).await;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
