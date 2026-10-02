//! Suits inside the colony over real WebTransport (survival, `--colony`): a pilot launches their
//! suit from the bay into the colony through the inner gate (the inside's own sector, a Welcome
//! to it), flies it there with the weapons safe, docks back at the inner gate, and is welcomed back
//! to their bay with the suit in it. A pilot who leaves while inside finds the suit towed home.

use std::time::Duration;

use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_econ::Bay;
use bc_econ::wire::{Place, Request};
use bc_proto::buttons::{FIRE_PRIMARY, FLIGHT_ASSIST};
use bc_proto::{Faction, FrameId, InputCmd};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::interior::{INNER_GATE, INNER_GATE_RADIUS};
use glam::Vec3;

fn config() -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        colony: true,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 0,
        max_clients: 8,
        ..Config::default()
    })
}

fn bot(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Leo, faction: Faction::Colonies }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_suit_flies_into_the_colony_and_docks_back_out() -> anyhow::Result<()> {
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut b = BotClient::connect(&bot(&http, "Quatre")).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    let hangar_slot = b.core.welcome.unwrap().client_slot;

    b.launch_inside().await?;
    let own = b.world().own.expect("own suit");
    assert!(own.pos.distance(INNER_GATE) < 200.0, "at the inner gate: {}", own.pos);
    assert_eq!(b.core.welcome.unwrap().field_rocks, 0);
    assert!(b.core.predict.interior());

    // Weapons safe: firing does nothing; flying does.
    let start = own.pos;
    for k in 0..90 {
        b.step(&mut |_| InputCmd {
            buttons: FLIGHT_ASSIST | FIRE_PRIMARY,
            thrust: [0, 0, if k < 45 { 100 } else { 0 }],
            aim: Vec3::X,
            ..InputCmd::default()
        })
        .await?;
    }
    let own = b.world().own.expect("own suit");
    assert!(own.pos.x > start.x + 20.0, "it flew down the colony: {} -> {}", start.x, own.pos.x);
    assert_eq!(b.world().my_hits, 0);
    assert!(b.world().beams.is_empty());
    // Not at rest in the ring: refused.
    assert!(b.dock().await.is_err() || b.place() == Some(Place::Space));

    // Back to the gate on flight assist, at rest, and dock.
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
    assert_eq!(b.core.welcome.unwrap().client_slot, hangar_slot);
    assert!(matches!(b.core.hangar.view.as_ref().unwrap().bay, Bay::Docked { .. }));
    // And out into space again, as ever.
    b.launch().await?;
    assert!(!b.core.predict.interior());
    b.close().await;
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pilot_who_leaves_inside_finds_the_suit_towed_home() -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("bc-inside-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let server = bc_server::start(Config { data_dir: Some(dir.clone()), ..config()? }).await?;
    let http = format!("http://{}", server.http_addr);
    let mut secret = [0u8; 32];
    secret[31] = 9;
    let wallet = LocalWallet::from_secret(&secret).expect("a valid key");
    let mut b = BotClient::connect_as(&bot(&http, "Wufei"), Some(&wallet)).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    b.launch_inside().await?;
    b.close().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let status = server.status();
    assert_eq!(status["game"]["inside"]["suits"], 0, "{}", status["game"]["inside"]);
    let mut b = BotClient::connect_as(&bot(&http, "Wufei"), Some(&wallet)).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    assert!(matches!(b.core.hangar.view.as_ref().unwrap().bay, Bay::Docked { .. }), "towed home");
    assert!(!b.core.welcome.unwrap().interior);
    b.close().await;
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = Request::Dock;
    Ok(())
}
