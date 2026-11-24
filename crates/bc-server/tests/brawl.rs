//! Open season on foot (`docs/OPEN_CITY.md`, "Heat and the Watch"; `bc_sim::colony::brawl`), over
//! real WebTransport: blows thrown in the ring's town put a pilot down, everyone sees them lying
//! there, they can't walk off, and one more knocks them out and up to their bay; hitting back is
//! free of the Watch, and the first blow isn't; a newly arrived pilot can't be touched; a car run at
//! a pilot knocks them down by the server's own reckoning, a friend lifts them up, and knocked down
//! again so soon they're out; and a pilot who's hurt stays hurt over a reconnect, and one who leaves
//! while down wakes in their bay.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_econ::wire::{Place, Request};
use bc_proto::presence::{PersonPose, RIDE_CAR, RING_DECK};
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::brawl::{self, BLOW, MAX_HP, SHIELD_TICKS, UP_HP};
use bc_sim::colony::{deck, town};
use glam::Vec3;

fn config(data_dir: Option<PathBuf>) -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
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

/// Down the lift to the town.
async fn down(b: &mut BotClient) -> anyhow::Result<()> {
    b.wait_until(5.0, "the hangar", |c| c.hangar.place.is_some() && c.hangar.view.is_some()).await?;
    if !b.core.hangar.in_town() {
        b.enter_town().await?;
    }
    Ok(())
}

/// Open ground near the step out of Hub Gate's lift: nothing within 5 m of it at chest height.
/// `(x, s)` on the ring's floor.
fn open_ground() -> (f32, f32) {
    let (x0, t) = town::out_of_the_lift(0);
    let s0 = town::deck_s(t);
    let open = |x: f32, s: f32| {
        let (min, max) = (Vec3::new(x - 5.0, 0.5, -s - 5.0), Vec3::new(x + 5.0, 1.8, -s + 5.0));
        !deck::solid(deck::RING, min, max)
    };
    (0..400)
        .map(|k| (x0 - 20.0 + 2.0 * (k / 20) as f32, s0 - 20.0 + 2.0 * (k % 20) as f32))
        .filter(|&(x, s)| open(x, s))
        .min_by(|a, b| (a.0 - x0).hypot(a.1 - s0).total_cmp(&(b.0 - x0).hypot(b.1 - s0)))
        .expect("open ground by Hub Gate's lift")
}

/// Standing on the town's floor `dx` across and `ds` round from `at`, facing `yaw`.
fn standing(at: (f32, f32), dx: f32, ds: f32, yaw: f32) -> PersonPose {
    PersonPose { strip: RING_DECK, x: at.0 + dx, s: at.1 + ds, yaw, grounded: true, ..PersonPose::default() }
}

/// The way that faces from `a` to `b` (the walker's yaw: 0 faces −s, τ/4 faces +x).
fn yaw_to(a: &PersonPose, b: &PersonPose) -> f32 {
    (b.x - a.x).atan2(-(b.s - a.s))
}

/// Steps them all for `secs`.
async fn all(bots: &mut [&mut BotClient], secs: f64) -> anyhow::Result<()> {
    let end = Instant::now() + Duration::from_secs_f64(secs);
    while Instant::now() < end {
        for b in bots.iter_mut() {
            b.step(&mut |_| Default::default()).await?;
        }
    }
    Ok(())
}

/// Walks each from the lift's step to where they're to stand, over a couple of seconds.
async fn walk_out(bots: &mut [(&mut BotClient, PersonPose)]) -> anyhow::Result<()> {
    let (lx, lt) = town::out_of_the_lift(0);
    let from =
        PersonPose { strip: RING_DECK, x: lx, s: town::deck_s(lt), grounded: true, ..PersonPose::default() };
    for k in 1..=8 {
        let u = k as f32 / 8.0;
        for (b, to) in bots.iter_mut() {
            b.set_pose(PersonPose {
                x: from.x + (to.x - from.x) * u,
                s: from.s + (to.s - from.s) * u,
                ..*to
            });
        }
        let end = Instant::now() + Duration::from_millis(300);
        while Instant::now() < end {
            for (b, _) in bots.iter_mut() {
                b.step(&mut |_| Default::default()).await?;
            }
        }
    }
    Ok(())
}

/// The id `watcher` knows `name` by in the plaza.
fn id_of(watcher: &BotClient, name: &str) -> Option<u16> {
    watcher.people().iter().find(|(_, n, _)| n == name).map(|(id, _, _)| *id)
}

/// How `watcher` sees `id` drawn now.
fn seen(watcher: &BotClient, id: u16) -> Option<PersonPose> {
    watcher.people().iter().find(|(p, _, _)| *p == id).map(|(_, _, pose)| *pose)
}

/// A blow from `b`, standing at `me`, at `id`.
fn strike(id: u16, me: &PersonPose) -> Request {
    Request::Strike {
        pilot: id,
        x_cm: (me.x * 100.0).round() as i32,
        s_cm: (me.s * 100.0).round() as i32,
        yaw_mrad: (me.yaw * 1_000.0).round() as i32,
    }
}

/// The newest note `b` has had since `n`, if any.
fn note_since(b: &BotClient, n: usize) -> Option<(String, bool)> {
    b.core.hangar.notes.get(n..).and_then(|ns| ns.last().cloned())
}

fn hp(b: &BotClient) -> Option<u8> {
    b.core.hangar.hurt.as_ref().map(|h| h.hp)
}

fn is_down(b: &BotClient) -> bool {
    b.core.hangar.hurt.as_ref().is_some_and(|h| h.down())
}

/// Waits out the shield a pilot has newly down the lift.
const SHIELD_SECS: f64 = SHIELD_TICKS as f64 / bc_sim::TICK_HZ as f64 + 0.5;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn blows_put_a_pilot_down_for_all_to_see_and_one_more_carries_them_up() -> anyhow::Result<()> {
    let server = bc_server::start(config(None)?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = BotClient::connect(&bot(&http, "Maren")).await?;
    let mut b = BotClient::connect(&bot(&http, "Pagan")).await?;
    let mut c = BotClient::connect(&bot(&http, "Ines")).await?;
    for x in [&mut a, &mut b, &mut c] {
        down(x).await?;
    }
    // Maren and Pagan face each other 1.5 m apart on open ground; Ines watches from 6 m.
    let at = open_ground();
    let mut pa = standing(at, 0.0, 0.0, 0.0);
    let pb = standing(at, 1.5, 0.0, 0.0);
    pa.yaw = yaw_to(&pa, &pb);
    let pb = PersonPose { yaw: yaw_to(&pb, &pa), ..pb };
    let pc = standing(at, 0.5, 6.0, 0.0);
    assert_eq!(brawl::lands(&pa, &pb), Some(BLOW), "the setup: a blow from the front");
    walk_out(&mut [(&mut a, pa), (&mut b, pb), (&mut c, pc)]).await?;
    all(&mut [&mut a, &mut b, &mut c], 0.5).await?;
    let pagan = id_of(&a, "Pagan").expect("Maren sees Pagan");
    let maren = id_of(&b, "Maren").expect("Pagan sees Maren");
    assert_eq!((hp(&a), hp(&b)), (Some(MAX_HP), Some(MAX_HP)));

    // Newly down the lift, Pagan can't be touched.
    let n = a.core.hangar.notes.len();
    a.request(&strike(pagan, &pa)).await?;
    a.wait_until(5.0, "an answer", |c| c.hangar.notes.len() > n).await?;
    let (text, ok) = note_since(&a, n).unwrap();
    assert!(!ok && text.contains("JUST GOT HERE"), "{text}");
    // A blow at one of the store's traders: nothing. From metres off where Maren stands: refused.
    a.request(&strike(bc_proto::presence::VISITOR_BASE + 1, &pa)).await?;
    all(&mut [&mut a, &mut b, &mut c], SHIELD_SECS).await?;
    let n = a.core.hangar.notes.len();
    let far = PersonPose { x: pa.x - 6.0, ..pa };
    a.request(&strike(pagan, &far)).await?;
    a.wait_until(5.0, "an answer", |c| c.hangar.notes.len() > n).await?;
    let (text, ok) = note_since(&a, n).unwrap();
    assert!(!ok && text.contains("not where"), "{text}");
    assert_eq!(hp(&b), Some(MAX_HP));

    // Pagan swings first: seen by Ines, the Watch minds it.
    let n = b.core.hangar.notes.len();
    b.request(&strike(maren, &pb)).await?;
    b.wait_until(5.0, "the blow", |c| c.hangar.notes.len() > n).await?;
    assert_eq!(note_since(&b, n), Some(("YOU HIT MAREN".into(), true)));
    a.wait_until(5.0, "the hurt", |c| c.hangar.hurt.as_ref().is_some_and(|h| h.hp == MAX_HP - BLOW)).await?;
    assert_eq!(a.core.hangar.hurt.as_ref().and_then(|h| h.by.clone()).as_deref(), Some("Pagan"));
    b.wait_until(5.0, "the Watch", |c| c.hangar.heat >= 1).await?;

    // Maren hits back till Pagan's down: self-defence, nothing to the Watch.
    for k in 1..=5 {
        let n = a.core.hangar.notes.len();
        a.request(&strike(pagan, &pa)).await?;
        a.wait_until(5.0, "the blow", |c| c.hangar.notes.len() > n).await?;
        let (text, ok) = note_since(&a, n).unwrap();
        assert!(ok, "blow {k}: {text}");
        if k < 5 {
            assert_eq!(text, "YOU HIT PAGAN");
        } else {
            assert_eq!(text, "YOU PUT PAGAN DOWN");
        }
        all(&mut [&mut a, &mut b, &mut c], 0.9).await?;
    }
    b.wait_until(5.0, "down", is_down_core).await?;
    assert!(b.core.hangar.hurt.as_ref().is_some_and(|h| h.down_s > 20 && h.hp == 0));
    assert_eq!(a.core.hangar.heat, 0, "hitting back is free");
    // Everyone sees Pagan lying there; walking off goes nowhere.
    b.set_pose(PersonPose { x: pb.x + 5.0, ..pb });
    all(&mut [&mut a, &mut b, &mut c], 1.0).await?;
    let lying = seen(&c, pagan).expect("Ines sees Pagan");
    assert!(lying.down(), "{lying:?}");
    assert!((lying.x - pb.x).abs() < 0.3, "still where they fell: {lying:?}");
    // And a blow while they're down knocks them out: carried up to the bay.
    let n = a.core.hangar.notes.len();
    a.request(&strike(pagan, &pa)).await?;
    a.wait_until(5.0, "the blow", |c| c.hangar.notes.len() > n).await?;
    assert_eq!(note_since(&a, n), Some(("YOU KNOCKED PAGAN OUT".into(), true)));
    b.wait_until(5.0, "the bay", |c| c.hangar.in_hangar()).await?;
    assert!(b.core.hangar.notes.iter().any(|(t, _)| t.contains("CARRIED UP TO YOUR BAY")));
    b.wait_until(5.0, "patched up", |c| c.hangar.hurt.as_ref().is_some_and(|h| h.hp == MAX_HP && !h.down()))
        .await?;
    let status = server.status();
    let brawl = &status["game"]["brawl"];
    assert_eq!((brawl["downs"].as_u64(), brawl["outs"].as_u64()), (Some(1), Some(1)), "{brawl}");
    assert!(brawl["blows"].as_u64() >= Some(5), "{brawl}");
    Ok(())
}

fn is_down_core(c: &bc_client_core::ClientCore) -> bool {
    c.hangar.hurt.as_ref().is_some_and(|h| h.down())
}

/// Drives `a` at `speed` m/s along +x from `from` until its middle is `stop` m short of `to`,
/// stepping `others` meanwhile; then asks the server to take the run-down of `id`.
async fn drive_at(
    a: &mut BotClient,
    others: &mut [&mut BotClient],
    from: PersonPose,
    to: &PersonPose,
    speed: f32,
    id: u16,
) -> anyhow::Result<()> {
    let start = Instant::now();
    loop {
        let x = from.x + speed * start.elapsed().as_secs_f32();
        let car = PersonPose { x, ride: RIDE_CAR, speed, yaw: std::f32::consts::FRAC_PI_2, ..from };
        a.set_pose(car);
        a.step(&mut |_| Default::default()).await?;
        for b in others.iter_mut() {
            b.step(&mut |_| Default::default()).await?;
        }
        if x >= to.x - 1.5 {
            break;
        }
    }
    a.request(&Request::RunDown { pilot: id }).await?;
    // At a stop beside them now.
    let stop =
        PersonPose { x: to.x - 1.5, ride: RIDE_CAR, speed: 0.0, yaw: std::f32::consts::FRAC_PI_2, ..from };
    a.set_pose(stop);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_car_knocks_a_pilot_down_a_friend_lifts_them_and_again_so_soon_they_re_out() -> anyhow::Result<()> {
    let server = bc_server::start(config(None)?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = BotClient::connect(&bot(&http, "Maren")).await?;
    let mut b = BotClient::connect(&bot(&http, "Pagan")).await?;
    let mut c = BotClient::connect(&bot(&http, "Ines")).await?;
    for x in [&mut a, &mut b, &mut c] {
        down(x).await?;
    }
    let at = open_ground();
    // Maren at the wheel 14 m back along +x from Pagan; Ines 2 m round from Pagan.
    let pb = standing(at, 4.0, 0.0, 0.0);
    let pc = standing(at, 4.0, 2.0, 0.0);
    let pa = standing(at, -10.0, 0.0, std::f32::consts::FRAC_PI_2);
    walk_out(&mut [(&mut a, pa), (&mut b, pb), (&mut c, pc)]).await?;
    a.set_pose(PersonPose { ride: RIDE_CAR, ..pa });
    all(&mut [&mut a, &mut b, &mut c], SHIELD_SECS).await?;
    let pagan = id_of(&a, "Pagan").expect("Maren sees Pagan");
    let maren = id_of(&c, "Maren").expect("Ines sees Maren");

    // Slow, it's refused (whatever the pose says of its speed): nothing happens.
    drive_at(&mut a, &mut [&mut b, &mut c], PersonPose { ride: RIDE_CAR, ..pa }, &pb, 3.0, pagan).await?;
    all(&mut [&mut a, &mut b, &mut c], 1.0).await?;
    assert!(!is_down(&b));
    // Back off, and at 12 m/s: down.
    for k in 1..=8 {
        a.set_pose(PersonPose { ride: RIDE_CAR, x: pb.x - 1.5 - 1.5 * k as f32, ..pa });
        all(&mut [&mut a, &mut b, &mut c], 0.15).await?;
    }
    drive_at(
        &mut a,
        &mut [&mut b, &mut c],
        PersonPose { ride: RIDE_CAR, x: pb.x - 13.5, ..pa },
        &pb,
        12.0,
        pagan,
    )
    .await?;
    b.wait_until(5.0, "run down", is_down_core).await?;
    assert_eq!(b.core.hangar.hurt.as_ref().and_then(|h| h.by.clone()).as_deref(), Some("Maren"));
    a.wait_until(5.0, "the Watch", |c| c.hangar.heat >= 1).await?;
    all(&mut [&mut a, &mut b, &mut c], 0.5).await?;
    assert!(seen(&c, pagan).is_some_and(|p| p.down()));
    assert!(seen(&c, maren).is_some_and(|p| p.driving()));

    // Ines kneels by Pagan and lifts them, six seconds on.
    let n = c.core.hangar.notes.len();
    c.request(&Request::Help { pilot: pagan, on: true }).await?;
    all(&mut [&mut a, &mut b, &mut c], 2.0).await?;
    assert!(c.core.hangar.notes[n..].iter().all(|(_, ok)| *ok), "{:?}", &c.core.hangar.notes[n..]);
    assert!(b.core.hangar.hurt.as_ref().and_then(|h| h.helper.clone()).as_deref() == Some("Ines"));
    assert!(is_down(&b), "not yet");
    b.wait_until(8.0, "helped up", |c| c.hangar.hurt.as_ref().is_some_and(|h| !h.down() && h.hp == UP_HP))
        .await?;
    assert!(b.core.hangar.notes.iter().any(|(t, ok)| *ok && t == "INES HELPED YOU UP"));
    all(&mut [&mut a, &mut b, &mut c], 0.5).await?;
    assert!(seen(&c, pagan).is_some_and(|p| !p.down()));

    // Run down again so soon after getting up: out, and up to the bay.
    for k in 1..=8 {
        a.set_pose(PersonPose { ride: RIDE_CAR, x: pb.x - 1.5 - 1.5 * k as f32, ..pa });
        all(&mut [&mut a, &mut b, &mut c], 0.15).await?;
    }
    drive_at(
        &mut a,
        &mut [&mut b, &mut c],
        PersonPose { ride: RIDE_CAR, x: pb.x - 13.5, ..pa },
        &pb,
        12.0,
        pagan,
    )
    .await?;
    b.wait_until(5.0, "the bay", |c| c.hangar.in_hangar()).await?;
    assert_eq!(b.place(), Some(Place::Hangar));
    Ok(())
}

fn wallet() -> LocalWallet {
    let mut secret = [0u8; 32];
    secret[31] = 113;
    LocalWallet::from_secret(&secret).expect("a valid key")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hurt_stays_hurt_over_a_reconnect_and_leaving_while_down_wakes_in_the_bay() -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("bc-brawl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let server = bc_server::start(config(Some(dir.clone()))?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = BotClient::connect(&bot(&http, "Maren")).await?;
    let mut b = BotClient::connect_as(&bot(&http, "Pagan"), Some(&wallet())).await?;
    down(&mut a).await?;
    down(&mut b).await?;
    let at = open_ground();
    let mut pa = standing(at, 0.0, 0.0, 0.0);
    let pb = standing(at, 1.5, 0.0, 0.0);
    pa.yaw = yaw_to(&pa, &pb);
    let pb = PersonPose { yaw: yaw_to(&pb, &pa), ..pb };
    walk_out(&mut [(&mut a, pa), (&mut b, pb)]).await?;
    all(&mut [&mut a, &mut b], SHIELD_SECS).await?;
    let pagan = id_of(&a, "Pagan").expect("Maren sees Pagan");
    a.request(&strike(pagan, &pa)).await?;
    b.wait_until(5.0, "hurt", |c| c.hangar.hurt.as_ref().is_some_and(|h| h.hp == MAX_HP - BLOW)).await?;

    // Gone and back a moment later: as hurt as they were, where they were in the town.
    b.close().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut b = BotClient::connect_as(&bot(&http, "Pagan"), Some(&wallet())).await?;
    b.wait_until(5.0, "the town", |c| c.hangar.in_town()).await?;
    b.wait_until(5.0, "hurt", |c| c.hangar.hurt.is_some()).await?;
    assert_eq!(hp(&b), Some(MAX_HP - BLOW), "a reconnect heals nothing");
    let (wx, ws) = {
        let w = b.core.hangar.wake.expect("a wake in the town");
        let (x, t) = town::unpack_spot((w.x, w.s));
        (x, town::deck_s(t))
    };
    let pb = PersonPose { x: wx, s: ws, ..pb };
    b.set_pose(pb);
    all(&mut [&mut a, &mut b], SHIELD_SECS).await?;
    let pagan = id_of(&a, "Pagan").expect("Maren sees Pagan again");

    // Knocked down (blows from where Maren stands, beside them now), then gone while down: they
    // wake in their bay, patched up.
    let near = standing((pb.x, pb.s), -1.5, 0.0, std::f32::consts::FRAC_PI_2);
    a.set_pose(PersonPose { yaw: yaw_to(&near, &pb), ..near });
    all(&mut [&mut a, &mut b], 0.5).await?;
    let pa = PersonPose { yaw: yaw_to(&near, &pb), ..near };
    for _ in 0..6 {
        if is_down(&b) {
            break;
        }
        a.request(&strike(pagan, &pa)).await?;
        all(&mut [&mut a, &mut b], 0.9).await?;
    }
    b.wait_until(5.0, "down", is_down_core).await?;
    b.close().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut b = BotClient::connect_as(&bot(&http, "Pagan"), Some(&wallet())).await?;
    b.wait_until(5.0, "the bay", |c| c.hangar.in_hangar() && c.hangar.hurt.is_some()).await?;
    assert_eq!(hp(&b), Some(MAX_HP));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
