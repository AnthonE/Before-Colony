//! The colony's people over real WebTransport: two agents ride down to the same strip's Hub Gate
//! and each sees the other, by name, where the other says it is; a pose that couldn't be (a
//! kilometre in a moment) isn't passed on; going back up takes a pilot out of the others' view;
//! the plaza keeps an agent's clock within a tick of the sector's.

use bc_bot::{BotClient, BotConfig};
use bc_client_core::city::pose_of;
use bc_client_core::walker::Walker;
use bc_proto::presence::PersonPose;
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::city::place_door;
use bc_sim::colony::frame::CityPos;
use bc_sim::content::city::PLACES;
use glam::Vec3;

fn config() -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 0,
        max_clients: 8,
        colony: true,
        ..Config::default()
    })
}

async fn down(http: &str, name: &str) -> anyhow::Result<BotClient> {
    let cfg =
        BotConfig { server: http.into(), name: name.into(), frame: FrameId::Leo, faction: Faction::Colonies };
    let mut b = BotClient::connect(&cfg).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar()).await?;
    b.enter_city(0).await?;
    Ok(b)
}

/// Standing `ahead` metres up the avenue from Hub Gate's door and `aside` across.
fn standing(ahead: f32, aside: f32) -> PersonPose {
    let ((s, x), (ds, dx)) = place_door(&PLACES[0]);
    let at = CityPos::new(0, x - dx * ahead, s - ds * ahead + aside, 0.0);
    let mut w = Walker::at(at.walker(), Vec3::X);
    w.grounded = true;
    pose_of(0, &w)
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_pilots_see_each_other_at_hub_gate() -> anyhow::Result<()> {
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = down(&http, "Heero").await?;
    let mut b = down(&http, "Duo").await?;
    a.set_pose(standing(8.0, 0.0));
    b.set_pose(standing(12.0, 3.0));
    both(&mut a, &mut b, 1.5).await?;

    let seen_by_a = a.people();
    let seen_by_b = b.people();
    assert_eq!(seen_by_a.len(), 1, "{seen_by_a:?}");
    assert_eq!(seen_by_a[0].1, "Duo");
    assert_eq!(seen_by_b.len(), 1, "{seen_by_b:?}");
    assert_eq!(seen_by_b[0].1, "Heero");
    let want = standing(12.0, 3.0);
    let got = seen_by_a[0].2;
    assert!((got.x - want.x).hypot(got.s - want.s) < 0.05, "{got:?} vs {want:?}");
    let status = server.status();
    assert_eq!(status["game"]["city"]["people"], 2);
    assert_eq!(status["game"]["city"]["by_strip"][0], 2);

    // The plaza's tick keeps the clock: within a tick of the sector's.
    let sector = f64::from(status["game"]["tick"].as_u64().unwrap() as u32);
    let est = a.core.clock.server_now(a.now());
    assert!((est - sector).abs() < 2.0, "clock {est:.1} vs sector {sector}");

    // A kilometre in a moment isn't passed on: Heero still sees Duo where he was.
    b.set_pose(standing(1_000.0, 3.0));
    both(&mut a, &mut b, 1.0).await?;
    let got = a.people()[0].2;
    assert!((got.x - want.x).abs() < 0.05, "a teleport was relayed: {got:?}");
    assert!(server.status()["game"]["city"]["refused_poses"].as_u64().unwrap() > 0);

    // Duo goes back up: gone from Heero's plaza.
    b.leave_city().await?;
    both(&mut a, &mut b, 1.5).await?;
    assert!(a.people().is_empty(), "{:?}", a.people());
    assert_eq!(server.status()["game"]["city"]["people"], 1);
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    Ok(())
}
