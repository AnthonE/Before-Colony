//! Suits inside the colony over real WebTransport (survival, `--colony`): a pilot launches their
//! suit from the bay into the colony through the inner gate (the inside's own sector, a Welcome
//! to it), flies it there with the weapons safe, docks back at the inner gate, and is welcomed back
//! to their bay with the suit in it. A pilot who leaves while inside finds the suit towed home. The
//! inside keeps space's tick, so the colony has one clock; a suit flown down over Hub Gate sees the
//! people walking there, and they see it (each on-foot pilot watches the inside's suits near them).
//! With its grip armed, a suit lands on the avenue and walks it, predicted as the server has it.

use std::time::Duration;

use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_client_core::city::pose_of;
use bc_client_core::walker::Walker;
use bc_econ::Bay;
use bc_econ::wire::{Place, Request};
use bc_proto::buttons::{FIRE_PRIMARY, FLIGHT_ASSIST, GRIP};
use bc_proto::snapshot::footing;
use bc_proto::{BodyRef, Faction, FrameId, InputCmd};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::city::place_door;
use bc_sim::colony::frame::CityPos;
use bc_sim::colony::interior::{INNER_GATE, INNER_GATE_RADIUS};
use bc_sim::content::city::PLACES;
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

/// A command flying the suit toward `to` on flight assist, no faster than `top` m/s, from `own`.
fn toward(own: &bc_proto::snapshot::OwnState, to: Vec3, top: f32) -> InputCmd {
    let d = to - own.pos;
    let want = d.normalize_or_zero() * (d.length() * 0.3).min(top);
    let local = own.rot.conjugate() * (want - own.vel);
    let q = |v: f32| (v * 6.0).clamp(-127.0, 127.0) as i8;
    InputCmd {
        buttons: FLIGHT_ASSIST,
        thrust: [q(local.x), q(local.y), q(local.z)],
        aim: Vec3::X,
        ..InputCmd::default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_suit_inside_and_the_people_below_see_each_other_on_one_clock() -> anyhow::Result<()> {
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    // Someone strolling outside Hub Gate's door on the Charter strip.
    let mut walker = BotClient::connect(&bot(&http, "Relena")).await?;
    walker.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar()).await?;
    walker.enter_city(0).await?;
    let ((s, x), (ds, dx)) = place_door(&PLACES[0]);
    let feet = CityPos::new(0, x - dx * 10.0, s - ds * 10.0, 0.0);
    let mut w = Walker::at(feet.walker(), Vec3::X);
    w.grounded = true;
    let pose = pose_of(0, &w);
    walker.set_pose(pose);

    // A suit in by the inner gate: the inside's sector keeps space's tick.
    let mut pilot = BotClient::connect(&bot(&http, "Quatre")).await?;
    pilot.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    pilot.launch_inside().await?;
    let status = server.status();
    let (tick, inside) =
        (status["game"]["tick"].as_i64().unwrap(), status["game"]["inside"]["tick"].as_i64().unwrap());
    assert!((tick - inside).abs() <= 1, "space at {tick}, the inside at {inside}");

    // Down from the gate toward 300 m over the walker, until the suit's pilot makes them out.
    let over = CityPos::new(0, feet.x, feet.s, 300.0).to_colony();
    let mut seen = None;
    for _ in 0..30 * 120 {
        let own = pilot.world().own.expect("own suit");
        pilot.step(&mut |_| toward(&own, over, 150.0)).await?;
        walker.step(&mut |_| InputCmd::default()).await?;
        if let Some(p) = pilot.people().into_iter().find(|p| p.1 == "Relena") {
            seen = Some(p);
            break;
        }
    }
    let (_, _, p) = seen.expect("the walker, seen from the suit");
    assert!((p.x - pose.x).hypot(p.s - pose.s) < 0.05, "{p:?} vs {pose:?}");
    assert_eq!(pilot.core.plaza.strip, Some(0), "the strip under the suit");
    let own = pilot.world().own.expect("own suit");
    assert!(own.pos.distance(feet.to_colony()) < 1_500.0, "seen from {}", own.pos);

    // The walker watches it too: the one suit near them, where the pilot flies it.
    walker
        .wait_until(5.0, "the suit, seen from the street", |c| c.world.entities.iter().flatten().count() == 1)
        .await?;
    let watched: Vec<_> = walker.world().entities.iter().flatten().map(|t| t.latest).collect();
    let own = pilot.world().own.expect("own suit");
    assert_eq!(watched[0].frame, FrameId::Leo);
    assert!(watched[0].pos.distance(own.pos) < 60.0, "{} vs {}", watched[0].pos, own.pos);
    assert!(walker.world().own.is_none(), "no suit of their own");
    assert_eq!(server.status()["game"]["inside"]["watchers"], 1);
    // And its clock is the plaza's: the sector's tick, kept by snapshots and plaza alike.
    let est = pilot.core.clock.server_now(pilot.now());
    let tick = f64::from(server.status()["game"]["tick"].as_u64().unwrap() as u32);
    assert!((est - tick).abs() < 3.0, "clock {est:.1} vs {tick}");
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);

    // Back up the lift, the walker stops watching: the suit is forgotten, the slot handed back.
    walker.leave_city().await?;
    walker.wait_until(5.0, "nothing watched", |c| c.world.entities.iter().flatten().count() == 0).await?;
    pilot.step(&mut |_| InputCmd { buttons: FLIGHT_ASSIST, aim: Vec3::X, ..InputCmd::default() }).await?;
    assert_eq!(server.status()["game"]["inside"]["watchers"], 0);
    walker.close().await;
    pilot.close().await;
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_suit_inside_lands_on_the_avenue_and_walks_it() -> anyhow::Result<()> {
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    // Someone on foot by Hub Gate's door, to watch.
    let ((s, x), (ds, dx)) = place_door(&PLACES[0]);
    let mut walker = BotClient::connect(&bot(&http, "Catherine")).await?;
    walker.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar()).await?;
    walker.enter_city(0).await?;
    let mut w = Walker::at(CityPos::new(0, x - dx * 10.0, s - ds * 10.0, 0.0).walker(), Vec3::X);
    w.grounded = true;
    walker.set_pose(pose_of(0, &w));

    let mut b = BotClient::connect(&bot(&http, "Trowa")).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    b.launch_inside().await?;

    // Down from the gate to 22 m over the avenue, 60 m up it from Hub Gate's door.
    let spot = |ahead: f32, h: f32| CityPos::new(0, x - dx * ahead, s - ds * ahead, h).to_colony();
    let over = spot(60.0, 22.0);
    for _ in 0..30 * 200 {
        let own = b.world().own.expect("own suit");
        if own.pos.distance(over) < 6.0 && own.vel.length() < 2.0 {
            break;
        }
        b.step(&mut |_| toward(&own, over, 150.0)).await?;
    }
    let own = b.world().own.expect("own suit");
    assert!(own.pos.distance(over) < 6.0, "over the avenue: {} m off", own.pos.distance(over));

    // The grip armed: caught by the city, down, and on its feet.
    let up = (spot(100.0, 0.0) - spot(60.0, 0.0)).normalize();
    let hold = InputCmd { buttons: FLIGHT_ASSIST | GRIP, aim: up, ..InputCmd::default() };
    let standing =
        |b: &BotClient| b.world().own.and_then(|o| o.surface).is_some_and(|s| s.footing == footing::GROUNDED);
    for _ in 0..30 * 30 {
        if standing(&b) {
            break;
        }
        b.step(&mut |_| hold).await?;
    }
    assert!(standing(&b), "on its feet: {:?}", b.world().own.and_then(|o| o.surface));
    let own = b.world().own.expect("own suit");
    assert_eq!(own.surface.map(|s| s.body), Some(BodyRef::City));
    let start = own.pos;

    // Up the avenue at a walk for two seconds: on the ground all the way, and predicted as flown.
    b.run_for(Duration::from_secs(2), &mut |_| InputCmd { thrust: [0, 0, 127], ..hold }).await?;
    b.run_for(Duration::from_millis(500), &mut |_| hold).await?;
    let own = b.world().own.expect("own suit");
    assert_eq!(own.surface.map(|s| s.footing), Some(footing::GROUNDED));
    let walked = (own.pos - start).dot(up);
    assert!(walked > 10.0, "walked {walked} m up the avenue");
    assert!(b.core.stats.prediction_error < 0.05, "predicted {} m off", b.core.stats.prediction_error);
    let watched = server.status();
    assert_eq!(watched["game"]["hot_path_allocations"], 0);

    // The one on foot watches it standing there: on the city, where its pilot has it. (They haven't
    // stepped while it came down and walked, so what they hear first is that backlog: it shows the
    // suit standing where it landed before it shows it where it is.)
    let at = b.world().own.expect("own suit").pos;
    let seen = |c: &bc_client_core::ClientCore| {
        c.world
            .entities
            .iter()
            .flatten()
            .map(|t| t.latest)
            .find(|e| e.on.is_some_and(|on| on.body == BodyRef::City))
    };
    let r = walker
        .wait_until(5.0, "the suit, standing on the city where its pilot has it", |c| {
            seen(c).is_some_and(|e| e.on.is_some_and(|on| !on.aloft) && e.pos.distance(at) < 2.0)
        })
        .await;
    let e = seen(&walker.core);
    assert!(r.is_ok(), "seen at {:?}, stands at {at}", e.map(|e| e.pos));
    assert_eq!(walker.world().stats.unresolved_bodies, 0);
    walker.close().await;

    // Letting go: flying again.
    b.run_for(Duration::from_secs(1), &mut |_| InputCmd {
        buttons: FLIGHT_ASSIST,
        aim: up,
        ..InputCmd::default()
    })
    .await?;
    assert!(b.world().own.is_some_and(|o| o.surface.is_none()), "flying again");

    // And back up to the inner gate, 2.9 km against the colony's pull, to dock into the bay.
    let t0 = std::time::Instant::now();
    for _ in 0..30 * 300 {
        let own = b.world().own.expect("own suit");
        if own.pos.distance(INNER_GATE) < INNER_GATE_RADIUS * 0.5 && own.vel.length() < 2.0 {
            break;
        }
        b.step(&mut |_| toward(&own, INNER_GATE, 150.0)).await?;
    }
    let own = b.world().own.expect("own suit");
    assert!(
        own.pos.distance(INNER_GATE) < INNER_GATE_RADIUS,
        "at the gate: {} m off",
        own.pos.distance(INNER_GATE)
    );
    println!("up to the gate in {:.0} s", t0.elapsed().as_secs_f32());
    b.run_for(Duration::from_secs(2), &mut |_| InputCmd {
        buttons: FLIGHT_ASSIST,
        aim: up,
        ..InputCmd::default()
    })
    .await?;
    b.dock().await?;
    b.wait_until(5.0, "the bay", |c| c.hangar.in_hangar() && c.welcome.is_some_and(|w| !w.interior)).await?;
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
