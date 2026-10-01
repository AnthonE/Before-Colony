//! Suits left in hide spots outlive the server (survival). A signed-in pilot whose suit was left
//! asleep in a landmark's hide spot finds it there after a restart, worn and laden as it was; it
//! was out there to be hunted all along, and `/status` counts it without saying where. One saved
//! against other landmarks is towed home instead, and one destroyed while its pilot was away
//! doesn't come back with the next run.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_client_core::Identity;
use bc_econ::wire::{Outcome, Place};
use bc_econ::{Bay, Hangar};
use bc_proto::buttons::GRIP;
use bc_proto::snapshot::footing;
use bc_proto::{BodyRef, Faction, FrameId, InputCmd, Part, PilotKind};
use bc_sector::{Control, Reparked};
use bc_server::net::NetStats;
use bc_server::net::game::GameRuntime;
use bc_server::pilots::{FileStore, ParkedSuit, PilotRecord, PilotStore, Sleeper, key};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::bodies::Body;
use bc_sim::content::landmarks::LANDMARKS_VERSION;
use bc_sim::ground::CROUCH_STANCE;
use bc_sim::sim::{Gone, ParkRecord, SleeperFate};
use bc_sim::{Sim, SimConfig};
use glam::{Quat, Vec3};

fn wallet(k: u8) -> LocalWallet {
    let mut secret = [0u8; 32];
    secret[31] = k;
    LocalWallet::from_secret(&secret).expect("a valid key")
}

fn bot(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Leo, faction: Faction::Colonies }
}

fn config(dir: &Path) -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 0,
        max_clients: 8,
        data_dir: Some(dir.to_path_buf()),
        ..Config::default()
    })
}

/// A fresh data directory for one test.
fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bc-hide-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn store(dir: &Path) -> FileStore {
    FileStore::new(dir.join("pilots")).expect("a pilot store")
}

/// A Leo crouched in MO-II's Aft Well with ore in its hold, a bounty and a wounded arm, as the
/// sector records it when its pilot leaves.
fn hidden_leo() -> ParkRecord {
    let mut sim = Sim::new(SimConfig { target_dolls: 0, ..SimConfig::default() });
    let id = sim
        .spawn_at(
            FrameId::Leo,
            Faction::Colonies,
            PilotKind::Agent,
            Vec3::new(0.0, 5_000.0, 9_000.0),
            Quat::IDENTITY,
        )
        .expect("a suit");
    assert!(sim.place_on(id, Body::Landmark(0), Vec3::new(-1.0, 0.02, 0.02)), "into the Aft Well");
    let i = id.idx();
    sim.suits.cargo_kg[i] = [80, 0, 12, 0];
    sim.suits.credits[i] = 600;
    sim.suits.part_hp[i][bc_proto::Part::ArmR as usize] *= 0.4;
    let crouch = InputCmd { thrust: [0, -127, 0], buttons: GRIP, ..sim.suits.input[i] };
    for _ in 0..30 {
        sim.set_input(id, InputCmd { tick: sim.next_tick(), ..crouch });
        sim.step();
    }
    assert!(sim.sleep(id));
    let rec = sim.park_record(i).expect("parked in a hide spot");
    assert_eq!(rec.stance, CROUCH_STANCE);
    rec
}

/// The record of `w`'s pilot, `name`, whose suit is out, left asleep in a hide spot in a server
/// run before this one.
fn left_hidden(w: &LocalWallet, name: &str, rec: &ParkRecord, landmarks_version: u16) -> PilotRecord {
    let mut r = PilotRecord::new(&w.address());
    r.name = name.into();
    r.frame = "leo".into();
    let mut hangar = Hangar::starter();
    hangar.bay = match std::mem::take(&mut hangar.bay) {
        Bay::Docked { suit } | Bay::Out { suit } => Bay::Out { suit },
        Bay::Empty => unreachable!("the starter kit has a suit"),
    };
    r.hangar = Some(hangar);
    r.sleeper = Some(Sleeper { run: 1, suit: 40, generation: 2, since_unix: 1_000 });
    r.parked = Some(ParkedSuit { landmarks_version, ..ParkedSuit::new(rec, 1_000) });
    r
}

async fn until(what: &str, secs: f64, mut ok: impl FnMut() -> bool) -> anyhow::Result<()> {
    let end = tokio::time::Instant::now() + Duration::from_secs_f64(secs);
    while !ok() {
        anyhow::ensure!(tokio::time::Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Ok(())
}

fn count(status: &serde_json::Value, k: &str) -> u64 {
    status["game"][k].as_u64().unwrap_or(u64::MAX)
}

/// The names `/status` lists as asleep.
fn sleepers(status: &serde_json::Value) -> Vec<String> {
    let list = status["game"]["sleepers"].as_array().cloned().unwrap_or_default();
    list.iter().filter_map(|s| s["name"].as_str().map(str::to_string)).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hidden_suit_is_restored_at_boot_and_wakes_in_place() -> anyhow::Result<()> {
    let dir = data_dir("wakes");
    let w = wallet(21);
    let rec = hidden_leo();
    store(&dir).save(left_hidden(&w, "Wufei", &rec, LANDMARKS_VERSION)).await?;

    let server = bc_server::start(config(&dir)?).await?;
    // Out there before anyone connects: parked, and on the roster asleep.
    until("the suit back in the Aft Well", 5.0, || count(&server.status(), "sleepers_parked") == 1).await?;
    assert_eq!(sleepers(&server.status()), ["Wufei"]);
    let on_disk = store(&dir).load(&key(&w.address())).await?.expect("the record");
    let sleeper = on_disk.sleeper.expect("asleep in this run");
    assert_ne!(sleeper.run, 1, "the sleeper of this run");
    assert!(on_disk.parked.is_some(), "kept, should the server restart again");

    // Its pilot is back: awake in it, crouched in the Aft Well where it was left, with its hold.
    let http = format!("http://{}", server.http_addr);
    let me = Identity::Wallet { address: w.address(), resume: None };
    let mut b = BotClient::connect_with(&bot(&http, "Wufei"), me, Some(&w)).await?;
    assert!(b.core.welcome.is_some_and(|w| w.woke), "it should wake in its suit");
    let mut grip = |_: &bc_client_core::InputContext| InputCmd { buttons: GRIP, ..InputCmd::default() };
    for _ in 0..500 {
        if b.world().own_sent().is_some() && b.place() == Some(Place::Space) {
            break;
        }
        b.step(&mut grip).await?;
    }
    let own = *b.world().own_sent().expect("its own suit");
    assert_eq!(own.slot, sleeper.suit, "the suit put back");
    let surface = own.surface.expect("on a body");
    assert_eq!((surface.footing, surface.body), (footing::GROUNDED, BodyRef::Landmark(0)));
    assert_eq!(f32::from(surface.stance_q) / 16.0, CROUCH_STANCE, "crouched");
    assert!(own.pos.distance(rec.local) < 0.01, "{} m from where it was left", own.pos.distance(rec.local));
    assert_eq!(own.cargo_kg, [80, 0, 12, 0]);
    assert!(matches!(b.core.hangar.view.as_ref().map(|v| &v.bay), Some(Bay::Out { .. })));
    let on_disk = store(&dir).load(&key(&w.address())).await?.expect("the record");
    assert_eq!((on_disk.sleeper, on_disk.parked), (None, None), "awake: nothing left out there");

    // Gone again, still in the spot: kept for the next run once more.
    b.close().await;
    let mut kept = None;
    for _ in 0..200 {
        kept = store(&dir).load(&key(&w.address())).await?.and_then(|r| r.parked);
        if kept.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let kept = kept.expect("left in the Aft Well again");
    assert_eq!(
        (kept.landmark, kept.landmarks_version, kept.cargo_kg),
        (0, LANDMARKS_VERSION, [80, 0, 12, 0])
    );
    assert!(Vec3::from_array(kept.local).distance(rec.local) < 0.01);
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_stale_parked_record_is_towed_home() -> anyhow::Result<()> {
    let dir = data_dir("stale");
    let w = wallet(22);
    // Left on landmarks that have changed since.
    store(&dir).save(left_hidden(&w, "Quatre", &hidden_leo(), LANDMARKS_VERSION + 1)).await?;

    let server = bc_server::start(config(&dir)?).await?;
    let on_disk = store(&dir).load(&key(&w.address())).await?.expect("the record");
    assert_eq!(on_disk.parked, None, "forgotten at boot");
    tokio::time::sleep(Duration::from_millis(200)).await;
    let status = server.status();
    assert_eq!((count(&status, "sleepers_parked"), count(&status, "suits_alive")), (0, 0));
    assert!(sleepers(&status).is_empty());

    // Its pilot is back to the colony's tugs having brought it in.
    let http = format!("http://{}", server.http_addr);
    let me = Identity::Wallet { address: w.address(), resume: None };
    let mut b = BotClient::connect_with(&bot(&http, "Quatre"), me, Some(&w)).await?;
    assert!(!b.core.welcome.is_some_and(|w| w.woke));
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && !c.hangar.sorties.is_empty()).await?;
    let (outcome, text) = b.core.hangar.sorties.last().cloned().expect("a sortie");
    assert_eq!(outcome, Outcome::Recovered);
    assert!(text.contains("TUGS"), "{text}");
    assert!(matches!(b.core.hangar.view.as_ref().map(|v| &v.bay), Some(Bay::Docked { .. })));
    b.close().await;
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_restored_sleeper_destroyed_is_not_restored_again() -> anyhow::Result<()> {
    let dir = data_dir("destroyed");
    let w = wallet(23);
    store(&dir).save(left_hidden(&w, "Trowa", &hidden_leo(), LANDMARKS_VERSION)).await?;
    let cfg = config(&dir)?;
    let boot = || GameRuntime::start(&cfg, Arc::new(NetStats::default()), "127.0.0.1".into());

    let game = boot()?;
    let shared = game.shared();
    assert_eq!(shared.restore_parked(cfg.max_sleepers).await, 1);
    let sleeper = store(&dir).load(&key(&w.address())).await?.and_then(|r| r.sleeper).expect("asleep");
    // Hunted down while its pilot is away: the sector tells the server so, as it does for any
    // sleeper destroyed.
    let gone = Gone::Destroyed { killer: 7 };
    let fate = SleeperFate { suit: sleeper.suit, generation: sleeper.generation, gone, tick: 900 };
    shared.sector.notes.push(fate).expect("room for the news");
    let mut cleared = false;
    for _ in 0..200 {
        let r = store(&dir).load(&key(&w.address())).await?.expect("the record");
        if r.parked.is_none() {
            assert_eq!(r.sleeper, Some(sleeper), "the rest of the record stands");
            assert!(matches!(r.hangar.map(|h| h.bay), Some(Bay::Out { .. })));
            cleared = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(cleared, "the hidden suit is still kept");
    game.stop();

    // The next run has nothing to put back.
    let game = boot()?;
    assert_eq!(game.shared().restore_parked(cfg.max_sleepers).await, 0);
    let view = game.status_view();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(view.json()["sleepers_parked"], 0);
    game.stop();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn status_counts_hidden_sleepers_without_positions() -> anyhow::Result<()> {
    let dir = data_dir("status");
    let w = wallet(24);
    store(&dir).save(left_hidden(&w, "Duo", &hidden_leo(), LANDMARKS_VERSION)).await?;
    let server = bc_server::start(config(&dir)?).await?;
    // In sight while it powers down (8 s from boot), then dark: counted as hidden.
    let status = server.status();
    assert_eq!(count(&status, "sleepers_hidden"), 0, "still powering down");
    until("the suit to go dark", 15.0, || count(&server.status(), "sleepers_hidden") == 1).await?;
    let status = server.status();
    let counts = ["sleepers_parked", "suits_grounded", "suits_aloft", "suits_hidden", "sleepers_hidden"];
    assert_eq!(counts.map(|k| count(&status, k)), [1, 1, 0, 1, 1]);
    // Who is asleep, but never where.
    let list = status["game"]["sleepers"].as_array().cloned().unwrap_or_default();
    let [entry] = list.as_slice() else { panic!("{list:?}") };
    let mut keys: Vec<&str> = entry.as_object().expect("an entry").keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["address", "name", "suit"]);
    assert_eq!(entry["name"], "Duo");
    assert!(status["game"]["pilots"].as_array().is_some_and(Vec::is_empty), "nobody flying");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hidden_suit_comes_back_as_it_was_left_by_its_hunters() -> anyhow::Result<()> {
    let dir = data_dir("hunted");
    let w = wallet(25);
    let rec = hidden_leo();
    store(&dir).save(left_hidden(&w, "Heero", &rec, LANDMARKS_VERSION)).await?;
    let cfg = config(&dir)?;
    let boot = || GameRuntime::start(&cfg, Arc::new(NetStats::default()), "127.0.0.1".into());

    let game = boot()?;
    let shared = game.shared();
    assert_eq!(shared.restore_parked(cfg.max_sleepers).await, 1);
    let sleeper = store(&dir).load(&key(&w.address())).await?.and_then(|r| r.sleeper).expect("asleep");
    // Found while its pilot is away, and its arms shot off: the sector says what's left of it.
    let mut hit = rec;
    hit.home.parts[Part::ArmL as usize] = 0.0;
    hit.home.parts[Part::ArmR as usize] = 0.0;
    hit.home.mounts = 0;
    let (suit, generation) = (sleeper.suit, sleeper.generation);
    shared.sector.reparked.push(Reparked { suit, generation, tick: 900, rec: hit }).expect("room");
    let mut kept = None;
    for _ in 0..200 {
        kept = store(&dir).load(&key(&w.address())).await?.and_then(|r| r.parked).filter(|p| p.tick == 900);
        if kept.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let kept = kept.expect("the record still keeps it as its pilot left it");
    assert_eq!((kept.parts, kept.mounts), (hit.home.parts, 0));
    game.stop();

    // The next run puts it back as its hunters left it: the arms aren't there to take again.
    let game = boot()?;
    assert_eq!(game.shared().restore_parked(cfg.max_sleepers).await, 1);
    let again = store(&dir).load(&key(&w.address())).await?.and_then(|r| r.parked).expect("kept");
    assert_eq!((again.parts, again.mounts, again.tick), (hit.home.parts, 0, 0));
    game.stop();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_suit_put_back_after_boot_stopped_waiting_goes_again() -> anyhow::Result<()> {
    // A sector slow enough at boot answers a restore after the server has given up on it (and let
    // its record go, so its pilot's suit is towed home): that suit doesn't stay out there too.
    let dir = data_dir("late");
    let cfg = config(&dir)?;
    let game = GameRuntime::start(&cfg, Arc::new(NetStats::default()), "127.0.0.1".into())?;
    let shared = game.shared();
    assert_eq!(shared.restore_parked(cfg.max_sleepers).await, 0);
    shared.sector.control.push(Control::Restore { key: 0, rec: hidden_leo() }).expect("room");
    let view = game.status_view();
    tokio::time::sleep(Duration::from_millis(1_000)).await;
    let status = view.json();
    assert_eq!((&status["sleepers_parked"], &status["suits_alive"]), (&0.into(), &0.into()));
    game.stop();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
