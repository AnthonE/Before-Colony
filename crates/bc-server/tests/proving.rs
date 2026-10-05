//! The Proving Ground over real WebTransport (survival, `--colony`; `docs/TRAINING.md`). A pilot
//! signed in rides down to Hub Gate and walks into the Blast Hall to its gantry, and boards one of
//! the Charter Board's trainers there (their own suit stays in their bay). From the gantry they
//! clear the drill, and the time the server checked goes on the Proving Ground's board: told to
//! them, on the board every pilot in the colony is sent, in `/status` and on their record. Docked
//! back on the gantry, they climb out on foot in the hall, where the plaza takes their first step.
//! The board and their best outlive the server; a pilot gone while flying a trainer wakes in their
//! bay, their own suit there.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_econ::Bay;
use bc_econ::wire::{Place, Request};
use bc_proto::buttons::{FIRE_SECONDARY, FLIGHT_ASSIST, GRIP};
use bc_proto::snapshot::footing;
use bc_proto::{BodyRef, Faction, FrameId, InputCmd, WeaponKind};
use bc_server::pilots::{FileStore, PilotStore};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::frame::CityPos;
use bc_sim::colony::hall::{self, DRILL_PAR_S, DrillEvent};
use bc_sim::content::{frame, weapon};
use glam::Vec3;

fn wallet() -> LocalWallet {
    let mut secret = [0u8; 32];
    secret[31] = 77;
    LocalWallet::from_secret(&secret).expect("a valid key")
}

fn bot(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Leo, faction: Faction::Colonies }
}

fn config(dir: &Path) -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        colony: true,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 0,
        max_clients: 8,
        data_dir: Some(dir.to_path_buf()),
        ..Config::default()
    })
}

fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bc-proving-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Down the lift, and on foot from Hub Gate to the gantry's hatch in the Blast Hall.
async fn to_the_gantry(b: &mut BotClient) -> anyhow::Result<()> {
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    b.enter_city(hall::hall().strip).await?;
    b.walk_to(hall::hatch().0, 180.0).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pilot_boards_a_trainer_in_the_blast_hall_clears_the_drill_and_climbs_out_there()
-> anyhow::Result<()> {
    let dir = data_dir("drill");
    let server = bc_server::start(config(&dir)?).await?;
    let http = format!("http://{}", server.http_addr);
    let w = wallet();
    let mut b = BotClient::connect_as(&bot(&http, "Wufei"), Some(&w)).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    // Not from the bay: the trainers are boarded at the gantry, on foot.
    assert!(b.ask(&Request::BoardTrainer).await.is_err());

    to_the_gantry(&mut b).await?;
    // The board comes with the city.
    b.wait_until(5.0, "the board", |c| c.hangar.proving.is_some()).await?;
    b.board_trainer().await?;
    let own = b.world().own.expect("the trainer");
    assert!(hall::in_gantry(own.pos, Vec3::ZERO), "on the gantry: {}", own.pos);
    let on = b.world().own_sent().and_then(|o| o.surface).expect("standing");
    assert_eq!((on.body, on.footing), (BodyRef::City, footing::GROUNDED));
    // Their own suit never left the bay.
    assert!(matches!(b.core.hangar.view.as_ref().unwrap().bay, Bay::Docked { .. }));
    assert_eq!(server.status()["game"]["city"]["people"], 0, "off the street");

    // The drill, from the gantry: the machine cannon's trigger held on the lit target, where it
    // will be when the rounds get there (as the server resolves them), until it's cleared.
    let speed = weapon(WeaponKind::MachineCannon).speed;
    let muzzle = frame(FrameId::Leo).loadout[1].map_or(Vec3::ZERO, |m| m.arm.muzzle());
    let mut cleared = None;
    let end = std::time::Instant::now() + Duration::from_secs(90);
    while cleared.is_none() {
        anyhow::ensure!(std::time::Instant::now() < end, "the drill: {} struck", b.world().drill.struck());
        b.step(&mut |ctx| {
            let lit = usize::from(ctx.world.drill.lit());
            let f = &ctx.predict.state;
            let t = ctx.resolve_tick.max(0.0);
            let at = hall::target(lit, t as u32, 0.0);
            let ahead = at.distance(f.pos) / speed * bc_sim::TICK_HZ as f32;
            let at = hall::target(lit, t as u32 + ahead.round() as u32, (t.fract()) as f32);
            InputCmd {
                aim: (at - (f.pos + f.rot * muzzle)).normalize(),
                buttons: FLIGHT_ASSIST | GRIP | FIRE_SECONDARY,
                ..InputCmd::default()
            }
        })
        .await?;
        for e in b.core.world.drill_news.drain(..) {
            match e {
                DrillEvent::Cleared(secs) => cleared = Some(secs),
                DrillEvent::Out(n) => anyhow::bail!("the clock ran out at {n}"),
                _ => {}
            }
        }
    }
    let secs = cleared.unwrap();
    let ms = (secs * 1_000.0).round() as u32;
    println!("the drill over the wire: {secs:.1} s");
    assert!(secs < DRILL_PAR_S, "{secs} s");

    // The server's time: the same, on the board with the pilot's name, and theirs.
    b.wait_until(5.0, "the board's word", |c| {
        c.hangar.notes.iter().any(|(t, _)| t.starts_with("THE BOARD · THE DRILL"))
    })
    .await?;
    let note = b.core.hangar.notes.iter().find(|(t, _)| t.starts_with("THE BOARD")).unwrap().0.clone();
    assert!(note.contains("FIRST CLASS") && note.contains("THE BEST EVER"), "{note}");
    b.wait_until(5.0, "the board", |c| c.hangar.proving.as_ref().is_some_and(|v| !v.drill.is_empty()))
        .await?;
    let v = b.core.hangar.proving.clone().unwrap();
    assert_eq!((v.drill[0].name.as_str(), v.drill[0].ms, v.drill[0].you), ("Wufei", ms, true));
    assert_eq!(v.drill_record.as_ref().map(|r| r.ms), Some(ms));
    assert_eq!(v.mine.drill_ms, Some(ms));
    let status = server.status();
    assert_eq!(status["game"]["proving"]["drill"][0]["name"], "Wufei");
    assert_eq!(status["game"]["proving"]["drill"][0]["ms"], ms);

    // At rest on the gantry, docked: out of the trainer on foot in the hall, by its hatch.
    b.request(&Request::Dock).await?;
    b.wait_until(5.0, "out on foot in the hall", |c| {
        c.hangar.in_city() && c.welcome.is_some_and(|w| !w.interior)
    })
    .await?;
    assert!(b.core.hangar.off_a_trainer());
    assert!(matches!(b.core.hangar.view.as_ref().unwrap().bay, Bay::Docked { .. }), "the bay's untouched");
    let ((s, x), _) = hall::hatch();
    let strip = hall::hall().strip;
    let mut feet = bc_client_core::walker::Walker::at(CityPos::new(strip, x, s, 0.0).walker(), Vec3::X);
    feet.grounded = true;
    b.set_pose(bc_client_core::city::pose_of(strip, &feet));
    let refused = server.status()["game"]["city"]["refused_poses"].as_u64().unwrap();
    for _ in 0..30 {
        b.step(&mut |_| InputCmd::default()).await?;
    }
    let status = server.status();
    assert_eq!(status["game"]["city"]["people"], 1);
    assert_eq!(status["game"]["city"]["refused_poses"].as_u64().unwrap(), refused, "their first step taken");
    assert_eq!(status["game"]["hot_path_allocations"], 0);
    assert_eq!(b.place(), Some(Place::City));
    b.close().await;
    server.shutdown();

    // Their best is on their record, and the board outlives the server.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let record = FileStore::new(dir.join("pilots"))?.load(&bc_server::pilots::key(&w.address())).await?;
    assert_eq!(record.map(|r| r.proving.drill_ms), Some(Some(ms)));
    let server = bc_server::start(config(&dir)?).await?;
    let http = format!("http://{}", server.http_addr);
    assert_eq!(server.status()["game"]["proving"]["drill"][0]["ms"], ms);
    let mut b = BotClient::connect_as(&bot(&http, "Wufei"), Some(&w)).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar()).await?;
    b.enter_city(strip).await?;
    b.wait_until(5.0, "the board", |c| {
        c.hangar.proving.as_ref().is_some_and(|v| v.mine.drill_ms == Some(ms))
    })
    .await?;

    // Gone while flying a trainer: the Board's suit is the Board's again, and the pilot wakes in
    // their bay, their own suit there as they left it.
    b.walk_to(hall::hatch().0, 180.0).await?;
    b.board_trainer().await?;
    assert_eq!(server.status()["game"]["inside"]["suits"], 1);
    b.close().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let status = server.status();
    assert_eq!(status["game"]["inside"]["suits"], 0, "{}", status["game"]["inside"]);
    let mut b = BotClient::connect_as(&bot(&http, "Wufei"), Some(&w)).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    assert!(matches!(b.core.hangar.view.as_ref().unwrap().bay, Bay::Docked { .. }), "the bay's untouched");
    assert!(!b.core.hangar.trainer && !b.core.welcome.unwrap().interior);
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    b.close().await;
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
