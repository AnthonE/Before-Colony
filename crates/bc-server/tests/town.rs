//! The ring's town over real WebTransport (`docs/FREE_ROAM.md`): under survival rules two pilots
//! ride a spoke's lift down from their bays to Hub Gate's square and each sees the other there; one
//! takes a car from the street and drives off; a suit can't be boarded from the town; and a pilot
//! rides back up only from a lift's door, on foot. `/status` counts them, never where.

use bc_bot::{BotClient, BotConfig};
use bc_econ::wire::Request;
use bc_proto::presence::{PersonPose, RIDE_CAR, RING_DECK};
use bc_proto::{Faction, FrameId};
use bc_server::config::Start;
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

/// With the game's own start (`--start town`), a new pilot at a keyboard wakes on the street: at
/// Hub Gate's lift, told where their suit is, and rides up to their bay by its door. An agent still
/// wakes in its bay.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_new_pilot_wakes_in_the_town_and_an_agent_in_its_bay() -> anyhow::Result<()> {
    let server = bc_server::start(Config { start: Start::Town, ..config()? }).await?;
    let http = format!("http://{}", server.http_addr);
    let cfg = |name: &str| BotConfig {
        server: http.clone(),
        name: name.into(),
        frame: FrameId::Argo,
        faction: Faction::Colonies,
    };
    let mut person = BotClient::connect_person(&cfg("Maren")).await?;
    person.wait_until(5.0, "the town", |c| c.hangar.in_town() && c.hangar.view.is_some()).await?;
    assert!(
        person.core.hangar.news.iter().any(|n| n.contains("UP THE SPOKE'S LIFT")),
        "{:?}",
        person.core.hangar.news
    );
    let mut agent = BotClient::connect(&cfg("Pagan")).await?;
    agent.wait_until(5.0, "its bay", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    assert_eq!(server.status()["game"]["town"]["people"], 1);
    // She stands where the lift let her out, and rides it up.
    person.set_pose(standing(0.0, 0.0));
    both(&mut person, &mut agent, 1.0).await?;
    person.leave_town().await?;
    assert_eq!(server.status()["game"]["town"]["people"], 0);
    // Back down by her airlock's lift; and next time, no longer new... a guest is new every time.
    person.enter_town().await?;
    assert_eq!(server.status()["game"]["town"]["people"], 1);
    Ok(())
}

/// One pilot at the wheel runs down one of the town's people (someone standing about in Hub Gate's
/// square); the server takes their word (that
/// person was there) and tells everyone in the town, who see the same person down in the same place;
/// a hit on somebody far off is nobody's.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn somebody_run_down_is_down_for_everyone_in_the_town() -> anyhow::Result<()> {
    use bc_sim::colony::street;
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = down(&http, "Maren").await?;
    let mut b = down(&http, "Pagan").await?;
    a.set_pose(standing(0.0, 0.0));
    b.set_pose(standing(-3.0, 2.0));
    both(&mut a, &mut b, 1.0).await?;
    assert!(a.core.hangar.street.is_empty() && b.core.hangar.street.is_empty());

    // Somebody standing about in the lift's square, fore of it (open ground from its door).
    let (lx, lt) = town::out_of_the_lift(0);
    let tick = server.status()["game"]["tick"].as_u64().unwrap() as u32;
    let mut who = None;
    street::each_person(lx - 110.0, lx - 15.0, lt + 15.0, lt + 110.0, tick, 0.0, &mut |p| {
        if p.pose == street::Pose::Stand {
            who = who.or(Some(*p));
        }
    });
    let p = who.expect("somebody standing in Hub Gate's square");
    // A hit claimed from the lift's door, on foot and far off: nothing.
    b.request(&Request::Hit { person: p.id }).await?;
    both(&mut a, &mut b, 0.6).await?;
    assert!(a.core.hangar.street.is_empty(), "{:?}", a.core.hangar.street);
    // Pagan takes a car and drives over to them, a car's pace at a time, and says so again. (Said to
    // be at a crawl: faster where people walk, they'd run from it, out of the crowd: `crowd`.)
    let (dx, ds) = (p.x - lx, town::deck_s(p.t) - town::deck_s(lt));
    let steps = ((dx.hypot(ds) / 20.0).ceil() as usize).max(2);
    for k in 0..=steps {
        let u = k as f32 / steps as f32;
        b.set_pose(PersonPose {
            ride: RIDE_CAR,
            speed: 4.0,
            ..standing(-3.0 + (dx + 3.0) * u, 2.0 + (ds - 2.0) * u)
        });
        both(&mut a, &mut b, 0.7).await?;
    }
    b.request(&Request::Hit { person: p.id }).await?;
    a.wait_until(5.0, "the street's word", |c| c.hangar.street.iter().any(|d| d.id == p.id)).await?;
    b.wait_until(5.0, "the street's word", |c| c.hangar.street.iter().any(|d| d.id == p.id)).await?;
    let (da, db) = (a.core.hangar.street[0], b.core.hangar.street[0]);
    assert_eq!(da, db, "the same person down in the same place for both");
    assert!((da.x - p.x).abs() < 0.01 && (da.s - town::deck_s(p.t)).abs() < 0.05, "{da:?} vs {p:?}");
    // Anyone else of the crowd's near them runs from it, out of the crowd, and both see them run.
    let tick = server.status()["game"]["tick"].as_u64().unwrap() as u32;
    let mut round = 0;
    street::each_person(p.x - 20.0, p.x + 20.0, p.t - 20.0, p.t + 20.0, tick, 0.0, &mut |q| {
        round += usize::from(q.id != p.id && (q.x - p.x).hypot(q.t - p.t) < 18.0);
    });
    if round > 0 {
        let running =
            |c: &bc_client_core::ClientCore| c.hangar.crowd.0.iter().any(|q| q.scared && q.id != p.id);
        a.wait_until(5.0, "those round them running", running).await?;
        b.wait_until(5.0, "those round them running", running).await?;
        assert!(server.status()["game"]["town"]["scared"].as_u64().unwrap() >= 1);
    }
    // Said again, they're no more down than they were.
    b.request(&Request::Hit { person: p.id }).await?;
    both(&mut a, &mut b, 0.6).await?;
    assert_eq!(a.core.hangar.street.len(), 1);
    Ok(())
}

/// A car driven fast at someone standing about in Hub Gate's square: they run from it, out of the
/// crowd, and both pilots down in the town are told so (`Update::Crowd`), with them where the server
/// has them; `/status` counts them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_car_driven_fast_at_the_crowd_has_it_run_for_everyone() -> anyhow::Result<()> {
    use bc_sim::colony::street;
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = down(&http, "Maren").await?;
    let mut b = down(&http, "Pagan").await?;
    a.set_pose(standing(0.0, 0.0));
    b.set_pose(standing(-3.0, 2.0));
    both(&mut a, &mut b, 1.0).await?;
    let (lx, lt) = town::out_of_the_lift(0);
    let tick = server.status()["game"]["tick"].as_u64().unwrap() as u32;
    let mut who = None;
    street::each_person(lx - 110.0, lx - 15.0, lt + 15.0, lt + 110.0, tick, 0.0, &mut |p| {
        if p.pose == street::Pose::Stand {
            who = who.or(Some(*p));
        }
    });
    let p = who.expect("somebody standing in Hub Gate's square");
    assert_eq!(server.status()["game"]["town"]["promoted"], 0);
    // Over at a crawl to 7 m short of them, then at them at 12 m/s.
    let (dx, ds) = (p.x - lx, town::deck_s(p.t) - town::deck_s(lt));
    let far = dx.hypot(ds);
    let short = 1.0 - 7.0 / far;
    let steps = ((far / 20.0).ceil() as usize).max(2);
    let yaw = dx.atan2(-ds);
    for k in 0..=steps {
        let u = short * k as f32 / steps as f32;
        let speed = if k == steps { 12.0 } else { 4.0 };
        b.set_pose(PersonPose {
            ride: RIDE_CAR,
            speed,
            yaw,
            ..standing(-3.0 + (dx + 3.0) * u, 2.0 + (ds - 2.0) * u)
        });
        both(&mut a, &mut b, 0.7).await?;
    }
    let running = |c: &bc_client_core::ClientCore| {
        let (near, mid, _) = &c.hangar.crowd;
        near.iter().any(|q| q.id == p.id && q.scared) || mid.iter().any(|q| q.0 == p.id)
    };
    b.wait_until(5.0, "them running from the car", running).await?;
    a.wait_until(5.0, "them running, seen from the lift", running).await?;
    let town = &server.status()["game"]["town"];
    assert!(town["promoted"].as_u64().unwrap() >= 1 && town["scared"].as_u64().unwrap() >= 1, "{town}");
    // Where the server has them, near where the crowd had them, and moving off.
    let q = b.core.hangar.crowd.0.iter().find(|q| q.id == p.id).copied().expect("near Pagan, whole");
    assert!((q.x - p.x).hypot(q.s - town::deck_s(p.t)) < 15.0 && q.speed > 0.5, "{q:?} vs {p:?}");
    Ok(())
}

/// A fare: stopped by someone standing in Hub Gate's square, a pilot is told where they want to go
/// and what it pays; everyone sees them gone from the square; they can't be set down anywhere but
/// the square they asked for; and a pilot who rides the lift up leaves them behind, unpaid.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fare_is_taken_in_a_lifts_square_and_lost_up_the_lift() -> anyhow::Result<()> {
    use bc_sim::colony::street;
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = down(&http, "Maren").await?;
    let mut b = down(&http, "Pagan").await?;
    a.set_pose(standing(0.0, 0.0));
    b.set_pose(standing(-3.0, 2.0));
    both(&mut a, &mut b, 1.0).await?;
    let (lx, lt) = town::out_of_the_lift(0);
    let tick = server.status()["game"]["tick"].as_u64().unwrap() as u32;
    let mut who = None;
    street::each_person(lx - 110.0, lx - 15.0, lt + 15.0, lt + 110.0, tick, 0.0, &mut |p| {
        if p.pose == street::Pose::Stand {
            who = who.or(Some(*p));
        }
    });
    let p = who.expect("somebody standing in Hub Gate's square");
    // Pagan drives over and stops beside them.
    let (dx, ds) = (p.x + 3.0 - lx, town::deck_s(p.t) - town::deck_s(lt));
    let steps = ((dx.hypot(ds) / 20.0).ceil() as usize).max(2);
    for k in 0..=steps {
        let u = k as f32 / steps as f32;
        let speed = if k == steps { 0.0 } else { 12.0 };
        b.set_pose(PersonPose {
            ride: RIDE_CAR,
            speed,
            ..standing(-3.0 + (dx + 3.0) * u, 2.0 + (ds - 2.0) * u)
        });
        both(&mut a, &mut b, 0.7).await?;
    }
    let (credits, _) = (b.core.hangar.credits(), ());
    let (text, ok) = ask(&mut b, Request::TakeFare { person: p.id }).await?;
    assert!(ok && text.starts_with("A FARE FOR "), "{text}");
    let (to, pay) = b.core.hangar.fare.expect("told where to");
    assert!(to != 0 && pay >= 120, "to lift {to} for {pay} cr");
    // Maren sees them gone from the square (away, not down).
    a.wait_until(5.0, "the street's word", |c| c.hangar.street.iter().any(|d| d.id == p.id && d.away))
        .await?;
    // Not here: they asked for another gate's square.
    let (text, ok) = ask(&mut b, Request::DropFare).await?;
    assert!(!ok && text.contains("square"), "{text}");
    assert_eq!(b.core.hangar.credits(), credits, "nothing paid yet");
    // Back to the lift's door on foot and up: the fare's left behind, back on the street.
    for k in (0..=steps).rev() {
        let u = k as f32 / steps as f32;
        b.set_pose(PersonPose {
            ride: RIDE_CAR,
            speed: 12.0,
            ..standing(-3.0 + (dx + 3.0) * u, 2.0 + (ds - 2.0) * u)
        });
        both(&mut a, &mut b, 0.7).await?;
    }
    b.set_pose(standing(-3.0, 2.0));
    both(&mut a, &mut b, 0.7).await?;
    b.leave_town().await?;
    assert_eq!(b.core.hangar.fare, None);
    a.wait_until(5.0, "them back", |c| c.hangar.street.is_empty()).await?;
    assert_eq!(b.core.hangar.credits(), credits);
    Ok(())
}

/// The town's quick way round the ring: from a lift's door on foot, up its spoke, across the hub
/// and down another's, out into that square, where whoever else came that way sees them. Not from
/// a walk away from a door, and not to the square they're in.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pilot_rides_across_the_hub_from_one_lifts_square_to_another() -> anyhow::Result<()> {
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = down(&http, "Maren").await?;
    let mut b = down(&http, "Pagan").await?;
    a.set_pose(standing(0.0, 0.0));
    b.set_pose(standing(-3.0, 2.0));
    both(&mut a, &mut b, 1.0).await?;
    // A walk away from Hub Gate's door: refused.
    for k in 1..=3 {
        a.set_pose(standing(-5.0 * k as f32, 0.0));
        both(&mut a, &mut b, 1.0).await?;
    }
    let (text, ok) = ask(&mut a, Request::RideAcross { to: 3 }).await?;
    assert!(!ok && text.contains("lift's door"), "{text}");
    for k in (0..3).rev() {
        a.set_pose(standing(-5.0 * k as f32, 0.0));
        both(&mut a, &mut b, 1.0).await?;
    }
    // At its door, to its own square: refused; to The Basin's (lift 3, across the ring): across.
    let (text, ok) = ask(&mut a, Request::RideAcross { to: 0 }).await?;
    assert!(!ok && text.contains("already"), "{text}");
    let (text, ok) = ask(&mut a, Request::RideAcross { to: 3 }).await?;
    assert!(ok && text.starts_with("ACROSS THE HUB · DOWN TO "), "{text}");
    assert_eq!(a.core.hangar.across, Some(3));
    let (text, ok) = ask(&mut b, Request::RideAcross { to: 3 }).await?;
    assert!(ok, "{text}");
    // Out of lift 3's door, both: each passed on, and seen by the other there.
    let (x, t) = town::out_of_the_lift(3);
    let at = |dx: f32, ds: f32| PersonPose {
        strip: RING_DECK,
        x: x + dx,
        s: town::deck_s(t) + ds,
        grounded: true,
        ..PersonPose::default()
    };
    a.set_pose(at(0.0, 0.0));
    b.set_pose(at(-3.0, 2.0));
    both(&mut a, &mut b, 1.5).await?;
    let seen = b.people();
    let there = seen.iter().find(|p| p.1 == "Maren").map(|p| p.2).expect("Pagan sees Maren");
    assert!((there.x - x).hypot(there.s - town::deck_s(t)) < 0.05, "{there:?}");
    assert_eq!(server.status()["game"]["town"]["people"], 2);
    Ok(())
}
