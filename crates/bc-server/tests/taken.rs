//! The ring's town's taken set over real WebTransport (`docs/FREE_ROAM.md`, P2): what's taken out
//! of its crowd is the server's, shared by district. Two pilots by Hub Gate and a third waking
//! districts round the ring: one takes a car from its kerb, and the other's street has that kerb
//! empty while the far one is never sent that district; driven off and left, the other sees it
//! where it was left and takes it; and a car of the traffic taken is gone for all three, since a
//! lane's places go all the way round. (Somebody knocked down is down for both by Hub Gate: the
//! town's own test, over the same districts.)

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_client_core::town as town_foot;
use bc_client_core::vehicle::Vehicle;
use bc_client_core::walker::Guide;
use bc_econ::wire::Request;
use bc_proto::presence::{PersonPose, RIDE_CAR, RING_DECK};
use bc_proto::{Faction, FrameId};
use bc_server::street::{district_at, near};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::deck::{RING, round_from};
use bc_sim::colony::{street, town};

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

fn bot(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Argo, faction: Faction::Colonies }
}

async fn down(http: &str, name: &str) -> anyhow::Result<BotClient> {
    let mut b = BotClient::connect(&bot(http, name)).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    b.enter_town().await?;
    Ok(b)
}

fn wallet() -> LocalWallet {
    let mut secret = [0u8; 32];
    secret[31] = 47;
    LocalWallet::from_secret(&secret).expect("a valid key")
}

/// Where the store keeps the far pilot's record.
fn record_file(dir: &Path) -> PathBuf {
    let key = bc_server::pilots::key(&wallet().address().into());
    dir.join("pilots").join(format!("{key}.json"))
}

/// Steps both for `secs`.
async fn both(a: &mut BotClient, b: &mut BotClient, secs: f64) -> anyhow::Result<()> {
    let end = Instant::now() + Duration::from_secs_f64(secs);
    while Instant::now() < end {
        a.step(&mut |_| Default::default()).await?;
        b.step(&mut |_| Default::default()).await?;
    }
    Ok(())
}

/// Walks `who` through `poses` (a few metres apart: a run's pace), the other stepping along.
async fn walk(who: &mut BotClient, other: &mut BotClient, poses: &[PersonPose]) -> anyhow::Result<()> {
    for p in poses {
        who.set_pose(*p);
        both(who, other, 0.4).await?;
    }
    Ok(())
}

/// The way a walker goes from the step out of Hub Gate's lift to the side of the parked car nearest
/// it (`bc_client_core::town::route`): their poses a few metres apart, and the car.
fn to_a_car() -> (Vec<PersonPose>, Vehicle) {
    let mut w = town_foot::out_of_the_lift(0);
    let (there, facing) = town_foot::route("car", w.feet).expect("a car near Hub Gate");
    let mut guide = Guide::new(there, Some(facing));
    let mut poses = vec![town_foot::pose_of(&w)];
    for _ in 0..120 * 60 {
        let stride = guide.steer(&mut w, 1.0 / 60.0);
        w.step(&town_foot::GROUND, &stride, 1.0 / 60.0);
        let (p, last) = (town_foot::pose_of(&w), poses[poses.len() - 1]);
        if (p.x - last.x).hypot(round_from(RING, last.s, p.s)) >= 4.0 {
            poses.push(p);
        }
        if guide.arrived() {
            break;
        }
    }
    poses.push(PersonPose { speed: 0.0, ..town_foot::pose_of(&w) });
    let (car, left) = town_foot::car_at_hand(town_foot::feet(&w), &[], &[]).expect("a car at hand");
    assert_eq!(left, None);
    (poses, car)
}

/// On foot at the side of a car at `(x, s)` facing `yaw` (its kerb side, as `route` stands one).
fn beside(x: f32, s: f32, yaw: f32) -> PersonPose {
    let (fx, fs) = bc_sim::car::forward(yaw);
    PersonPose { strip: RING_DECK, x: x + fs * 2.1, s: s - fx * 2.1, grounded: true, ..PersonPose::default() }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn whats_taken_from_the_towns_crowd_is_the_same_for_everyone_near_and_kept_by_district()
-> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("bc-taken-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let server = bc_server::start(config(dir.clone())?).await?;
    let http = format!("http://{}", server.http_addr);

    // The far pilot logged out once in the bay; their record then says they left the town by the
    // third spoke's lift, four districts round from Hub Gate, so they wake there.
    let mut c = BotClient::connect_as(&bot(&http, "Corvi"), Some(&wallet())).await?;
    c.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    c.close().await;
    let end = Instant::now() + Duration::from_secs(5);
    while !(record_file(&dir).exists()
        && server.status()["game"]["hangars"].as_array().is_some_and(|h| h.is_empty()))
    {
        anyhow::ensure!(Instant::now() < end, "no record kept");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut r: serde_json::Value = serde_json::from_slice(&std::fs::read(record_file(&dir))?)?;
    let (fx, ft) = town::out_of_the_lift(2);
    r["town"] = serde_json::json!({ "x": fx, "s": town::deck_s(ft) });
    std::fs::write(record_file(&dir), serde_json::to_vec(&r)?)?;
    let mut c = BotClient::connect_as(&bot(&http, "Corvi"), Some(&wallet())).await?;
    c.wait_until(5.0, "the town, far round", |c| c.hangar.in_town() && c.hangar.taken.districts.len() == 3)
        .await?;
    let far = near(town::deck_s(ft));
    let theirs: Vec<u16> = c.core.hangar.taken.districts.iter().map(|d| d.district).collect();
    assert_eq!(theirs, far, "the districts round where they woke");

    // The two by Hub Gate: each is sent the districts round it, nothing taken in them yet.
    let mut a = down(&http, "Maren").await?;
    let mut b = down(&http, "Pagan").await?;
    let (walk_there, car) = to_a_car();
    a.set_pose(walk_there[0]);
    b.set_pose(walk_there[0]);
    both(&mut a, &mut b, 1.0).await?;
    let here = near(car.s);
    assert!(here.iter().all(|d| !far.contains(d)), "{here:?} and {far:?}");
    for p in [&a, &b] {
        let districts: Vec<u16> = p.core.hangar.taken.districts.iter().map(|d| d.district).collect();
        assert_eq!(districts, near(walk_there[0].s));
        assert_eq!(p.core.hangar.taken.kerbs().count() + p.core.hangar.taken.left().count(), 0);
    }

    // Maren walks to the car nearest the lift and takes it from its kerb: Pagan's street has the
    // kerb empty. Corvi, districts away, is never sent that district.
    walk(&mut a, &mut b, &walk_there).await?;
    let kerb = town_foot::kerb_of(&car).expect("in a bay");
    // As a browser does: at the wheel in the frame it asks, so the pose is there first.
    a.set_pose(PersonPose {
        x: car.x,
        s: car.s,
        yaw: car.yaw,
        ride: RIDE_CAR,
        ..beside(car.x, car.s, car.yaw)
    });
    both(&mut a, &mut b, 0.3).await?;
    a.request(&Request::TakeParked { kerb }).await?;
    b.wait_until(5.0, "the kerb empty", |c| c.hangar.taken.kerbs().any(|k| k == kerb)).await?;
    assert_eq!(server.status()["game"]["town"]["kerbs"], 1);
    // Not twice, and not by Pagan from where she stands.
    let n = b.core.hangar.notes.len();
    b.request(&Request::TakeParked { kerb }).await?;
    b.wait_until(5.0, "a refusal", |c| c.hangar.notes.len() > n).await.ok();
    assert!(b.core.hangar.notes.get(n).is_some_and(|(_, ok)| !ok), "{:?}", b.core.hangar.notes);

    // She drives it 25 m along the street and leaves it there: Pagan sees it where it stands.
    let (fx_, fs_) = car.forward();
    let along = |d: f32| (car.x + fx_ * d, car.s + fs_ * d);
    for k in 0..=5 {
        let (x, s) = along(5.0 * k as f32);
        let speed = if k < 5 { 10.0 } else { 0.0 };
        a.set_pose(PersonPose { x, s, yaw: car.yaw, ride: RIDE_CAR, speed, ..beside(x, s, car.yaw) });
        both(&mut a, &mut b, 0.5).await?;
    }
    let (lx, ls) = along(25.0);
    let cm = |v: f32| (v * 100.0).round() as i32;
    let yaw_mrad = (car.yaw * 1_000.0).round() as i32;
    a.request(&Request::LeaveCar { x_cm: cm(lx), s_cm: cm(ls), yaw_mrad }).await?;
    a.set_pose(beside(lx, ls, car.yaw));
    b.wait_until(5.0, "the car left", |c| c.hangar.taken.left().count() == 1).await?;
    let left = *b.core.hangar.taken.left().next().unwrap();
    assert!((left.x - lx).abs() < 0.02 && round_from(RING, ls, left.s).abs() < 0.02, "{left:?}");
    assert_eq!((left.kerb, left.car), (Some(kerb), None), "it keeps its kerb's look");
    assert!(b.core.hangar.taken.kerbs().any(|k| k == kerb), "and its kerb stays empty");
    assert_eq!(server.status()["game"]["town"]["left"], 1);

    // Pagan walks the same way, then along the kerb to it, and takes it: gone for both.
    walk(&mut b, &mut a, &walk_there).await?;
    let to_it: Vec<PersonPose> = (1..=5)
        .map(|k| {
            let (x, s) = along(5.0 * k as f32);
            beside(x, s, car.yaw)
        })
        .collect();
    walk(&mut b, &mut a, &to_it).await?;
    b.request(&Request::TakeLeft { id: left.id }).await?;
    a.wait_until(5.0, "the car taken", |c| c.hangar.taken.left().count() == 0).await?;
    b.wait_until(5.0, "the car taken", |c| c.hangar.taken.left().count() == 0).await?;
    assert_eq!(b.core.hangar.notes.len(), n + 1, "nothing refused since: {:?}", b.core.hangar.notes);
    assert_eq!(server.status()["game"]["town"]["left"], 0);

    // Maren steps out into the street's lane and takes a car of the traffic stopped for her: it's
    // gone from the traffic for all three, Corvi too, round the ring.
    let sx = street_middle(car.x);
    let way = if car.yaw > 1.0 { 1.0 } else { -1.0 };
    let lane_x = sx + way * street::STREET_LANE;
    let kerb_side = beside(lx, ls, car.yaw);
    let across: Vec<PersonPose> = (1..=3)
        .map(|k| PersonPose { x: kerb_side.x + (lane_x - kerb_side.x) * k as f32 / 3.0, ..kerb_side })
        .collect();
    walk(&mut a, &mut b, &across).await?;
    let tick = server.status()["game"]["tick"].as_u64().unwrap() as u32;
    let t = town::town_t(ls);
    let mut coming = None;
    street::each_car(lane_x - 0.5, lane_x + 0.5, t - 1_200.0, t + 1_200.0, tick, 0.0, &mut |c| {
        let on = way * round_from(RING, ls, town::deck_s(c.t));
        if (20.0..700.0).contains(&on) && c.kind != street::Kind::Watch {
            coming = coming.or(Some(c.id));
        }
    });
    let jacked = coming.expect("a car coming along her lane");
    a.request(&Request::Jack { car: jacked }).await?;
    for p in [&mut a, &mut b, &mut c] {
        p.wait_until(5.0, "the car gone from the traffic", |c| c.hangar.taken.lanes.contains(&jacked))
            .await?;
    }
    assert_eq!(server.status()["game"]["town"]["jacked"], 1);

    // Corvi, all along, was sent only the districts round where she is: nothing of Hub Gate's.
    let theirs: Vec<u16> = c.core.hangar.taken.districts.iter().map(|d| d.district).collect();
    assert_eq!(theirs, far);
    assert!(!far.contains(&(district_at(car.s) as u16)));
    assert_eq!(c.core.hangar.taken.kerbs().count() + c.core.hangar.taken.left().count(), 0);
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    c.close().await;
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// The middle of the street a car parked at `x` is parked along (its kerb's bay is
/// `street::BAY_IN` in from the street's edge, which is half a street from its middle).
fn street_middle(x: f32) -> f32 {
    let half = 0.5 * town::STREET - street::BAY_IN;
    let mut best = (f32::MAX, x);
    for c in 0..town::COLUMNS {
        let (_, x1, _, hi) = town::column(c);
        if hi == town::STREET * 0.5 {
            for m in [x1 - half, x1 + half] {
                if (m - x).abs() < best.0 {
                    best = ((m - x).abs(), x1);
                }
            }
        }
    }
    best.1
}
