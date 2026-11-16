//! The Watch's units, the server's, over real WebTransport (`docs/FREE_ROAM.md`, 4, W1): an incident
//! raised by hand (the dev HTTP's hook) brings the nearest station's units by road, which everyone
//! in the town is told of as they come, and they hold a cordon round it in the time their route
//! takes; and a wanted pilot standing in the road is stopped by them, and fined, with no word from
//! their client.

use bc_bot::{BotClient, BotConfig};
use bc_econ::wire::Request;
use bc_econ::wire::{UnitState, watch_fine};
use bc_proto::presence::{PersonPose, RIDE_CAR, RING_DECK};
use bc_proto::{Faction, FrameId};
use bc_server::dispatch::{SEND_NEAR, STATIONS};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::deck::{RING, round_from};
use bc_sim::colony::roads::{self, MOST, Waypoint};
use bc_sim::colony::town;
use bc_sim::colony::watch::{self as tables, Offence, STATION_UNITS, UnitKind};

fn config() -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 0,
        max_clients: 8,
        dev_hooks: true,
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

/// On the town's floor at `(x, t)`: standing, or at the wheel going `speed`.
fn pose(x: f32, t: f32, car: Option<f32>) -> PersonPose {
    PersonPose {
        strip: RING_DECK,
        x,
        s: town::deck_s(t),
        grounded: true,
        ride: if car.is_some() { RIDE_CAR } else { 0 },
        speed: car.unwrap_or(0.0),
        ..PersonPose::default()
    }
}

/// Drives `b` along `path` (`(x, t)` on the town's floor) at `speed` m/s, in real time.
async fn drive(b: &mut BotClient, path: &[(f32, f32)], speed: f32) -> anyhow::Result<()> {
    for leg in path.windows(2) {
        let (a, z) = (leg[0], leg[1]);
        let far = (z.0 - a.0).hypot(z.1 - a.1);
        let start = std::time::Instant::now();
        loop {
            let u = (start.elapsed().as_secs_f32() * speed / far.max(1e-3)).min(1.0);
            b.set_pose(pose(a.0 + (z.0 - a.0) * u, a.1 + (z.1 - a.1) * u, Some(speed)));
            b.step(&mut |_| Default::default()).await?;
            if u >= 1.0 {
                break;
            }
        }
    }
    Ok(())
}

/// How far a route by the roads is, m.
fn by_road(from: (f32, f32), to: (f32, f32)) -> f32 {
    let mut way = [Waypoint::default(); MOST];
    let n = roads::route(from, to, &mut way).min(MOST);
    roads::length(&way[..n])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_incident_raised_by_hand_brings_the_nearest_stations_units_to_a_cordon_round_it()
-> anyhow::Result<()> {
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = down(&http, "Maren").await?;
    let (lx, lt) = town::out_of_the_lift(0);
    a.set_pose(pose(lx, lt, None));
    a.run_for(std::time::Duration::from_secs(1), &mut |_| Default::default()).await?;
    assert!(a.core.hangar.units.is_empty());
    assert_eq!(server.status()["game"]["town"]["units"], 0);

    // On the boulevard 150 m round from district 0's station: its own units come, nearest by road.
    let (dx, dt) = town::watch_door(0);
    let goal = (dx - 15.0, dt + 150.0);
    let nearest = (0..STATIONS as u16)
        .min_by(|&p, &q| by_road(town::watch_door(p), goal).total_cmp(&by_road(town::watch_door(q), goal)));
    assert_eq!(nearest, Some(0));
    assert!((goal.1 - lt).abs() + 200.0 < SEND_NEAR, "near enough Maren to be told of them");
    let res = reqwest::Client::new()
        .post(format!("{http}/dev/incident"))
        .json(&serde_json::json!({ "x": goal.0, "s": town::deck_s(goal.1), "level": 1, "cause": "knock" }))
        .send()
        .await?;
    assert!(res.status().is_success(), "{}", res.status());
    let sent = std::time::Instant::now();
    a.wait_until(5.0, "the units", |c| !c.hangar.units.is_empty()).await?;
    let units = a.core.hangar.units.clone();
    let level = tables::answer(1);
    assert_eq!(units.len(), usize::from(level.cars + level.hounds), "{units:?}");
    assert!(units.iter().all(|u| usize::from(u.id) / STATION_UNITS == 0), "{units:?}");
    assert_eq!(units.iter().filter(|u| u.kind == UnitKind::Car).count(), usize::from(level.cars));
    assert!(units.iter().all(|u| u.state == UnitState::Driving));
    assert_eq!(server.status()["game"]["town"]["units"], units.len());

    // There, round it, within their routes' length at their speed (and a little for the wire).
    let slowest = UnitKind::Car.speed() * level.haste;
    let longest = [-1.0f32, 1.0]
        .map(|side| by_road(town::watch_door(0), (goal.0 + side * level.cordon, goal.1)))
        .into_iter()
        .fold(0.0, f32::max);
    let due = longest / slowest + 4.0;
    a.wait_until(f64::from(due), "the cordon", |c| {
        c.hangar.units.len() == 2 && c.hangar.units.iter().all(|u| u.state == UnitState::Cordon)
    })
    .await?;
    let took = sent.elapsed().as_secs_f32();
    assert!(took <= due, "{took} s, due in {due}");
    for u in &a.core.hangar.units {
        let off = (u.x - goal.0).hypot(round_from(RING, town::deck_s(goal.1), u.s));
        assert!(off < level.cordon + 40.0, "{u:?}: {off} m off");
        assert!(roads::drivable(u.x, town::town_t(u.s)), "{u:?} off the roads");
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_wanted_pilot_in_the_road_is_stopped_and_fined_by_the_units_with_no_word_from_her()
-> anyhow::Result<()> {
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = down(&http, "Maren").await?;
    let (lx, lt) = town::out_of_the_lift(0);
    a.set_pose(pose(lx, lt, None));
    a.run_for(std::time::Duration::from_secs(1), &mut |_| Default::default()).await?;
    // Out of Hub Gate's square in a car, and along the boulevard to just past district 0's station.
    let (_, dt) = town::watch_door(0);
    let stand = (24.0, dt + 40.0);
    drive(&mut a, &[(lx, lt), (lx, lt + 13.0), (stand.0, lt + 13.0), stand], 30.0).await?;
    a.set_pose(pose(stand.0, stand.1, Some(0.0)));
    a.run_for(std::time::Duration::from_secs(1), &mut |_| Default::default()).await?;
    a.set_pose(pose(stand.0, stand.1, None));
    a.run_for(std::time::Duration::from_secs(1), &mut |_| Default::default()).await?;
    assert_eq!(a.core.hangar.place, Some(bc_econ::wire::Place::Town));

    // One of the Watch's own cars taken (her word): wanted, and its units come out of the station.
    let credits = a.core.hangar.credits();
    a.request(&Request::Offence { what: Offence::TookTheWatchs }).await?;
    a.wait_until(5.0, "the Watch's word", |c| c.hangar.heat >= 3).await?;
    let level = a.core.hangar.heat;
    let at = town::deck_s(stand.1);
    a.wait_until(20.0, "a unit at her side", |c| {
        c.hangar
            .units
            .iter()
            .any(|u| (u.x - stand.0).hypot(round_from(RING, at, u.s)) < UnitKind::Hound.reach())
    })
    .await?;
    // She never says she's stopped: the units have her, and the server takes its fine.
    let fine = watch_fine(level).min(credits);
    a.wait_until(15.0, "stopped", |c| c.hangar.heat == 0 && c.hangar.credits() == credits - fine).await?;
    let said = a.core.hangar.notes.iter().any(|(n, ok)| *ok && n.starts_with("THE WATCH HAS YOU"));
    assert!(said, "{:?}", a.core.hangar.notes);
    let status = server.status();
    assert_eq!(status["game"]["town"]["stops"], 1);
    assert_eq!(status["game"]["town"]["incidents"], 0, "her incident's over");
    assert_eq!(status["game"]["hot_path_allocations"], 0);
    Ok(())
}
