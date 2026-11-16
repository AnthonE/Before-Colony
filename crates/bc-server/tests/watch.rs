//! The Watch's heat held by the server, over real WebTransport (`docs/FREE_ROAM.md`, 4;
//! `docs/OPEN_CITY.md`, "Heat and the Watch" and "A huge deal"): a pilot at the wheel in the ring's
//! town runs somebody down, and the server's heat on them rises; a second pilot sees that level in
//! the pose the plaza relays and the incident in the town's list; nothing a client says lowers it; a
//! stop charges the fine of the server's level and clears it; a pilot with too little to pay is sent
//! up to their bay; and the dev HTTP's hook raises an incident (only with the dev hooks on).

use bc_bot::{BotClient, BotConfig};
use bc_econ::wire::{Request, watch_fine};
use bc_proto::presence::{PersonPose, RIDE_CAR, RING_DECK};
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::deck::{RING, round_from};
use bc_sim::colony::watch::Offence;
use bc_sim::colony::{street, town};

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

/// The heat `watcher` sees on `id` in the town (none: not seen).
fn seen_heat(watcher: &BotClient, id: u16) -> Option<u8> {
    watcher.people().iter().find(|(p, _, _)| *p == id).map(|(_, _, pose)| pose.heat)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn somebody_run_down_heats_the_driver_for_everyone_to_see_until_the_watch_stops_them()
-> anyhow::Result<()> {
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = down(&http, "Maren").await?;
    let mut b = down(&http, "Pagan").await?;
    a.set_pose(standing(0.0, 0.0));
    b.set_pose(standing(-3.0, 2.0));
    both(&mut a, &mut b, 1.0).await?;
    let maren = b.people().first().map(|p| p.0).expect("Pagan sees Maren");
    assert_eq!((a.core.hangar.heat, seen_heat(&b, maren)), (0, Some(0)));
    assert!(a.core.hangar.incidents.is_empty() && b.core.hangar.incidents.is_empty());

    // Nothing she says of herself heats her: a hit and reckless driving are the server's to see.
    a.request(&Request::Offence { what: Offence::HitSomeone }).await?;
    a.request(&Request::Offence { what: Offence::Reckless }).await?;
    a.request(&Request::Sighted).await?;
    both(&mut a, &mut b, 0.6).await?;
    assert_eq!((a.core.hangar.heat, seen_heat(&b, maren)), (0, Some(0)));
    // And a stop with nothing on her is refused.
    let (_, ok) = ask(&mut a, Request::Stopped).await?;
    assert!(!ok);

    // Somebody standing about in the lift's square; Maren drives over and runs them down.
    let (lx, lt) = town::out_of_the_lift(0);
    let tick = server.status()["game"]["tick"].as_u64().unwrap() as u32;
    let mut who = None;
    street::each_person(lx - 110.0, lx - 15.0, lt + 15.0, lt + 110.0, tick, 0.0, &mut |p| {
        if p.pose == street::Pose::Stand {
            who = who.or(Some(*p));
        }
    });
    let p = who.expect("somebody standing in Hub Gate's square");
    let (dx, ds) = (p.x - lx, town::deck_s(p.t) - town::deck_s(lt));
    let steps = ((dx.hypot(ds) / 20.0).ceil() as usize).max(2);
    for k in 0..=steps {
        let u = k as f32 / steps as f32;
        a.set_pose(PersonPose { ride: RIDE_CAR, speed: 12.0, ..standing(dx * u, ds * u) });
        both(&mut a, &mut b, 0.7).await?;
    }
    // Stopped beside them (no faster than a walk from here on).
    a.set_pose(PersonPose { ride: RIDE_CAR, ..standing(dx, ds) });
    both(&mut a, &mut b, 0.7).await?;
    a.request(&Request::Hit { person: p.id }).await?;
    a.wait_until(5.0, "the Watch's word", |c| c.hangar.heat >= 2).await?;
    let level = a.core.hangar.heat;
    assert!(level >= 2, "somebody run down is two levels at least: {level}");
    // Pagan sees it on her, in the pose the server relays; and the incident, at her side.
    b.wait_until(5.0, "her incident", |c| c.hangar.incidents.iter().any(|i| i.offender == maren)).await?;
    both(&mut a, &mut b, 0.8).await?;
    assert_eq!(seen_heat(&b, maren), Some(level));
    let incident = *b.core.hangar.incidents.iter().find(|i| i.offender == maren).expect("her incident");
    // (As bad as her heat's gone; raised by the hit, or by how she drove there.)
    assert_eq!(incident.level, level);
    assert!(incident.cause == Offence::HitSomeone || incident.cause == Offence::Reckless, "{incident:?}");
    let apart = (incident.x - p.x).hypot(round_from(RING, town::deck_s(p.t), incident.s));
    assert!(apart < 40.0, "{incident:?} vs {p:?}");
    assert_eq!(a.core.hangar.incidents, b.core.hangar.incidents, "the same list for everyone in the town");
    let status = server.status();
    assert_eq!(
        (status["game"]["town"]["down"].as_u64(), status["game"]["town"]["incidents"].as_u64()),
        (Some(1), Some(1))
    );

    // What her client says can't take it down: a knock reported adds, a sighting holds, and a pose
    // that says nothing of heat (none ever does) changes nothing.
    let before = a.core.hangar.heat;
    a.request(&Request::Offence { what: Offence::Knock }).await?;
    a.request(&Request::Sighted).await?;
    a.set_pose(PersonPose { ride: RIDE_CAR, heat: 0, ..standing(dx, ds) });
    // (And Pagan claiming the worst for herself in her pose is nothing either.)
    b.set_pose(PersonPose { heat: 5, ..standing(-3.0, 2.0) });
    both(&mut a, &mut b, 1.0).await?;
    assert!(a.core.hangar.heat >= before, "{} < {before}", a.core.hangar.heat);
    let pagan = a.people().iter().find(|(_, n, _)| n == "Pagan").map(|p| p.0).expect("Maren sees Pagan");
    assert_eq!(seen_heat(&a, pagan), Some(0));
    assert_eq!(seen_heat(&b, maren), Some(a.core.hangar.heat));

    // Stopped by the Watch: the fine of the server's own level, and the heat's gone.
    let level = a.core.hangar.heat;
    let credits = a.core.hangar.credits();
    let (text, ok) = ask(&mut a, Request::Stopped).await?;
    let fine = watch_fine(level).min(credits);
    assert!(ok && text.starts_with("THE WATCH HAS YOU"), "{text}");
    assert!(text.contains(&bc_econ::item::thousands(fine)), "{text}: {fine} at level {level}");
    a.wait_until(5.0, "the fine", |c| c.hangar.credits() == credits - fine && c.hangar.heat == 0).await?;
    b.wait_until(5.0, "her incident over", |c| c.hangar.incidents.iter().all(|i| i.offender != maren))
        .await?;
    both(&mut a, &mut b, 0.8).await?;
    assert_eq!(seen_heat(&b, maren), Some(0));
    // Stopped again, there's nothing on her.
    let (_, ok) = ask(&mut a, Request::Stopped).await?;
    assert!(!ok);
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    Ok(())
}

/// What only a client sees (one of the Watch's own cars taken) it reports, and the server takes it,
/// once a second: three of them are the top of the scale, which is news to everyone. A stop at the
/// top costs more than a new pilot has: all of it, and the Watch sends them up to their bay, where
/// it has nothing on them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_watchs_car_taken_is_news_and_a_broke_pilot_is_sent_up_to_their_bay() -> anyhow::Result<()> {
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = down(&http, "Maren").await?;
    let mut b = down(&http, "Pagan").await?;
    a.set_pose(standing(0.0, 0.0));
    b.set_pose(standing(-3.0, 2.0));
    both(&mut a, &mut b, 1.0).await?;
    let credits = a.core.hangar.credits();
    assert!(credits < watch_fine(5), "{credits} cr");
    // Said three times at once, it's taken once.
    for _ in 0..3 {
        a.request(&Request::Offence { what: Offence::TookTheWatchs }).await?;
    }
    a.wait_until(5.0, "the Watch's word", |c| c.hangar.heat == 3).await?;
    both(&mut a, &mut b, 0.5).await?;
    assert_eq!(a.core.hangar.heat, 3);
    assert_eq!(a.core.hangar.heat_cause, Some(Offence::TookTheWatchs));
    // A second apart, twice more: the top.
    for _ in 0..2 {
        both(&mut a, &mut b, 1.1).await?;
        a.request(&Request::Offence { what: Offence::TookTheWatchs }).await?;
    }
    a.wait_until(5.0, "the top", |c| c.hangar.heat == 5).await?;
    // Everyone hears of it (the news is said once a second).
    b.wait_until(5.0, "the news", |c| c.hangar.news.iter().any(|n| n.contains("THE WATCH WANTS MAREN")))
        .await?;
    let said: Vec<&String> =
        b.core.hangar.news.iter().filter(|n| n.contains("THE WATCH WANTS MAREN")).collect();
    assert!(
        said.iter().any(|n| n.contains("ONE OF THE WATCH'S OWN CARS TAKEN IN ") && n.ends_with("◆◆◆◆◆")),
        "{said:?}"
    );
    let incident = b.core.hangar.incidents.iter().find(|i| i.offender != 0).copied().expect("her incident");
    assert_eq!((incident.level, incident.cause), (5, Offence::TookTheWatchs));
    assert_eq!(b.core.hangar.incidents.len(), 1, "one incident for her, raised as she went");

    // Stopped: all she has, and up to her bay.
    let (text, ok) = ask(&mut a, Request::Stopped).await?;
    assert!(ok && text.ends_with("WAS ALL YOU HAD · SENT UP TO YOUR BAY"), "{text}");
    a.wait_until(5.0, "the bay", |c| c.hangar.in_hangar() && c.hangar.heat == 0).await?;
    assert_eq!(a.core.hangar.credits(), 0);
    let (_, ok) = ask(&mut a, Request::Stopped).await?;
    assert!(!ok, "up there the Watch has nothing on her");
    // Up there, a report is nothing either.
    a.request(&Request::Offence { what: Offence::Knock }).await?;
    a.run_for(std::time::Duration::from_secs(1), &mut |_| Default::default()).await?;
    assert_eq!(a.core.hangar.heat, 0);
    Ok(())
}

/// The dev HTTP's hook raises an incident everyone in the town is told of (and a bad one is
/// refused); without the dev hooks, the route isn't there.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_incident_raised_by_hand_reaches_the_town_only_with_the_dev_hooks_on() -> anyhow::Result<()> {
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = down(&http, "Maren").await?;
    let mut b = down(&http, "Pagan").await?;
    a.set_pose(standing(0.0, 0.0));
    b.set_pose(standing(-3.0, 2.0));
    both(&mut a, &mut b, 1.0).await?;
    let (x, s) = (-300.0f32, town::deck_s(town::CELL * 30.5));
    let post = |body: serde_json::Value| {
        let url = format!("{http}/dev/incident");
        async move { reqwest::Client::new().post(url).json(&body).send().await }
    };
    let res = post(serde_json::json!({ "x": x, "s": s, "level": 4, "cause": "took_the_watchs" })).await?;
    assert!(res.status().is_success(), "{}", res.status());
    let id = res.json::<serde_json::Value>().await?["id"].as_u64().expect("its id") as u32;
    // A level past the top, or no such offence: refused.
    let bad = post(serde_json::json!({ "x": x, "s": s, "level": 9 })).await?;
    assert_eq!(bad.status(), reqwest::StatusCode::BAD_REQUEST);
    let bad = post(serde_json::json!({ "x": x, "s": s, "level": 2, "cause": "loitering" })).await?;
    assert_eq!(bad.status(), reqwest::StatusCode::BAD_REQUEST);
    for p in [&mut a, &mut b] {
        p.wait_until(5.0, "the incident", |c| c.hangar.incidents.iter().any(|i| i.id == id)).await?;
        let i = p.core.hangar.incidents.iter().find(|i| i.id == id).copied().unwrap();
        assert_eq!((i.level, i.cause, i.offender), (4, Offence::TookTheWatchs, 0));
        assert!((i.x - x).abs() < 0.01 && (i.s - s).abs() < 0.01, "{i:?}");
        p.wait_until(5.0, "the news", |c| {
            c.hangar.news.iter().any(|n| n.starts_with("THE WATCH IS CALLED OUT"))
        })
        .await?;
    }
    assert_eq!(server.status()["game"]["town"]["incidents"], 1);
    // Nobody's heat moved for it.
    assert_eq!((a.core.hangar.heat, b.core.hangar.heat), (0, 0));

    // A server without the dev hooks has no such route.
    let plain = bc_server::start(Config { dev_hooks: false, ..config()? }).await?;
    let res = reqwest::Client::new()
        .post(format!("http://{}/dev/incident", plain.http_addr))
        .json(&serde_json::json!({ "x": x, "s": s, "level": 4 }))
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);
    assert_eq!(plain.status()["game"]["town"]["incidents"], 0);
    Ok(())
}
