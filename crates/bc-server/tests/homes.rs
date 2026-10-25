//! The register of homes over real WebTransport (survival, the colony open; `docs/OPEN_CITY.md`):
//! a home is rented, bought, locked and opened at its own door, in the city, and nowhere else
//! (H1); a pilot who logged out asleep in their bed wakes beside it, on foot in the city, and
//! logging out there again leaves them asleep in it, through a restart (H3). (The plaza's own
//! tests walk into a home its register keeps a stranger out of, `plaza.rs`; the browser's `home`
//! suite rents one and locks it.)

use std::path::PathBuf;

use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_econ::Hangar;
use bc_econ::homes::Homes;
use bc_econ::wire::{Request, Wake};
use bc_proto::presence::PersonPose;
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::city::{CITY, ROWS, Stage, block, lots};
use bc_sim::colony::homes::{self, Home, Kind};
use bc_sim::content::homes::Furniture;

fn config(data_dir: Option<PathBuf>) -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        colony: true,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 0,
        max_clients: 8,
        data_dir,
        ..Config::default()
    })
}

fn wallet() -> LocalWallet {
    let mut secret = [0u8; 32];
    secret[31] = 77;
    LocalWallet::from_secret(&secret).expect("a valid key")
}

/// A flat to let on strip 0.
fn a_flat() -> Home {
    (CITY.0..CITY.1)
        .flat_map(|bx| (1..=ROWS).filter_map(move |row| block(0, bx, row, Stage(0))))
        .find_map(|b| {
            lots(&b).as_slice().iter().find_map(|bd| {
                homes::plan(&b, bd).filter(|p| p.kind == Kind::Flats).and_then(|p| p.home(0, 0))
            })
        })
        .expect("a flat")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_home_is_taken_at_its_door_and_nowhere_else() -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("bc-homes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let server = bc_server::start(config(Some(dir.clone()))?).await?;
    let http = format!("http://{}", server.http_addr);
    let cfg =
        BotConfig { server: http, name: "Ada".into(), frame: FrameId::Argo, faction: Faction::Colonies };
    let mut b = BotClient::connect_as(&cfg, Some(&wallet())).await?;
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    let flat = a_flat();
    // From the bay, every home request points at the city.
    for req in [
        Request::Rent { home: flat.id.0, weeks: 1 },
        Request::Buy { home: flat.id.0 },
        Request::EnterCode { home: flat.id.0, code: 1234 },
        Request::SetCode { home: flat.id.0, guest: false, code: Some(1234) },
    ] {
        let why = b.ask(&req).await.expect_err("refused in the bay").to_string();
        assert!(why.contains("in the city"), "{req:?}: {why}");
    }
    // Down at Hub Gate, it's still not the home's door.
    b.enter_city(0).await?;
    b.wait_until(5.0, "the city", |c| c.hangar.in_city()).await?;
    let why =
        b.ask(&Request::Rent { home: flat.id.0, weeks: 1 }).await.expect_err("not at its door").to_string();
    assert!(why.contains("home's door"), "{why}");
    let status = server.status();
    assert_eq!(status["game"]["homes"]["held"], 0, "{status}");
    assert_eq!(status["game"]["hot_path_allocations"], 0);
    b.close().await;
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_registers_overlay_is_what_the_crew_and_strangers_see() -> anyhow::Result<()> {
    use bc_client_core::homes::{VIEW_HOMES, overlay_from};
    use bc_econ::wire::HomesView;
    use bc_sim::colony::homes::Overlay;
    let dir = std::env::temp_dir().join(format!("bc-homes-overlay-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    // A block of flats on strip 0, and Ada's flat on its ground floor: its door shut, a bed, a rug
    // and a lamp in it. Bo, next door, asleep in his own bed.
    let plan = (CITY.0..CITY.1)
        .flat_map(|bx| (1..=ROWS).filter_map(move |row| block(0, bx, row, Stage(0))))
        .find_map(|b| {
            lots(&b).as_slice().iter().find_map(|bd| {
                homes::plan(&b, bd)
                    .filter(|p| p.kind == Kind::Flats && p.storeys >= 2 && p.homes_per_storey() >= 2)
            })
        })
        .expect("a block of flats");
    let (flat, next_door) = (plan.home(0, 0).expect("a flat"), plan.home(0, 1).expect("another"));
    let ada = bc_server::pilots::key(&wallet().address());
    let bo_wallet = {
        let mut secret = [0u8; 32];
        secret[31] = 78;
        LocalWallet::from_secret(&secret).expect("a valid key")
    };
    let bo = bc_server::pilots::key(&bo_wallet.address());
    let mut reg = Homes::new();
    let mut bay = Hangar { credits: 100_000, ..Hangar::starter() };
    reg.rent(&flat, &ada, "Ada", 2, &mut bay, bc_server::pilots::unix_now()).map_err(anyhow::Error::msg)?;
    let mid = ((flat.rect.width() * 5.0) as u16, (flat.rect.length() * 5.0) as u16);
    reg.furnish(flat.id, &ada, Furniture::Bed, mid, 0, 0, &mut bay).map_err(anyhow::Error::msg)?;
    reg.furnish(flat.id, &ada, Furniture::Rug, mid, 1, 0, &mut bay).map_err(anyhow::Error::msg)?;
    let corners = [(5, 5), (mid.0 * 2 - 5, 5), (5, mid.1 * 2 - 5), (mid.0 * 2 - 5, mid.1 * 2 - 5)];
    anyhow::ensure!(
        corners.iter().any(|&at| reg.furnish(flat.id, &ada, Furniture::Lamp, at, 0, 0, &mut bay).is_ok()),
        "nowhere for a lamp"
    );
    let mut bos_bay = Hangar { credits: 100_000, ..Hangar::starter() };
    let now = bc_server::pilots::unix_now();
    reg.rent(&next_door, &bo, "Bo", 2, &mut bos_bay, now).map_err(anyhow::Error::msg)?;
    let bos_mid = ((next_door.rect.width() * 5.0) as u16, (next_door.rect.length() * 5.0) as u16);
    reg.furnish(next_door.id, &bo, Furniture::Bed, bos_mid, 0, 0, &mut bos_bay)
        .map_err(anyhow::Error::msg)?;
    reg.sleep(next_door.id, &bo, "Bo", (0.0, 0.0), 0).expect("a bed to sleep in");
    std::fs::write(dir.join("homes.json"), serde_json::to_vec(&reg)?)?;

    // The server's, from the register it keeps.
    let register = bc_server::homes::HomeRegister::open(Some(dir.join("homes.json")));
    let server_home = register.overlay_home(flat.id).expect("a held home");
    assert!(server_home.shut, "its door is shut");
    assert_eq!(server_home.solids().len(), 2, "the bed and the lamp; the rug is walked over");
    // The crew's, from what their view of the register says.
    let mut crew = Overlay::new(VIEW_HOMES);
    overlay_from(&HomesView::of(&reg, [flat.id.0].into_iter(), &ada), &mut crew);
    assert_eq!(crew.get(flat.id), Some(&server_home), "the crew's");

    // And a stranger's: Bo's, waking next door, from what the server tells him of the building.
    let server = bc_server::start(config(Some(dir.clone()))?).await?;
    let cfg = BotConfig {
        server: format!("http://{}", server.http_addr),
        name: "Bo".into(),
        frame: FrameId::Argo,
        faction: Faction::Colonies,
    };
    let mut b = BotClient::connect_as(&cfg, Some(&bo_wallet)).await?;
    b.wait_until(5.0, "waking next door", |c| c.hangar.in_city() && c.hangar.wake.is_some()).await?;
    b.wait_until(5.0, "the homes near", |c| c.hangar.homes.homes.iter().any(|l| l.id == flat.id.0)).await?;
    let mut stranger = Overlay::new(VIEW_HOMES);
    overlay_from(&b.core.hangar.homes, &mut stranger);
    assert_eq!(stranger.get(flat.id), Some(&server_home), "the stranger's");
    let leaf = homes::leaf(&flat);
    let (s, x) = leaf.rect.middle();
    let at_door = (
        glam::Vec3::new(x - 0.1, leaf.h0 + 0.1, -s - 0.1),
        glam::Vec3::new(x + 0.1, leaf.h0 + 1.0, -s + 0.1),
    );
    assert!(stranger.hits(0, at_door.0, at_door.1), "the shut door is a wall to them");
    assert!(stranger.leaves(0).any(|b| b == leaf), "and drawn where it is");
    b.close().await;
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn asleep_in_their_bed_at_home_they_wake_beside_it_and_sleep_there_again() -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("bc-homes-bed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    // Ada's flat, its bed, and Ada asleep in it, as the register keeps it.
    let flat = a_flat();
    let ada = bc_server::pilots::key(&wallet().address());
    let mut reg = Homes::new();
    let mut bay = Hangar { credits: 100_000, ..Hangar::starter() };
    reg.rent(&flat, &ada, "Ada", 2, &mut bay, bc_server::pilots::unix_now()).map_err(anyhow::Error::msg)?;
    let mid = ((flat.rect.width() * 5.0) as u16, (flat.rect.length() * 5.0) as u16);
    reg.furnish(flat.id, &ada, Furniture::Bed, mid, 0, 0, &mut bay).map_err(anyhow::Error::msg)?;
    let bed = reg.sleep(flat.id, &ada, "Ada", (0.0, 0.0), 0).expect("a bed to sleep in");
    std::fs::write(dir.join("homes.json"), serde_json::to_vec(&reg)?)?;

    let server = bc_server::start(config(Some(dir.clone()))?).await?;
    let http = format!("http://{}", server.http_addr);
    let cfg =
        BotConfig { server: http, name: "Ada".into(), frame: FrameId::Argo, faction: Faction::Colonies };
    let mut b = BotClient::connect_as(&cfg, Some(&wallet())).await?;
    let got = std::cell::Cell::new(None);
    b.wait_until(5.0, "waking at home", |c| {
        got.set(c.hangar.wake);
        c.hangar.in_city() && c.hangar.wake.is_some()
    })
    .await?;
    let Some(Wake { home, s, x, storey }) = got.get() else { unreachable!() };
    assert_eq!((home, storey), (flat.id.0, 0));
    // Beside the bed, in the flat; and up, they're no longer asleep in it.
    let (s, x) = (flat.rect.s0 + f32::from(s) / 10.0, flat.rect.x0 + f32::from(x) / 10.0);
    assert_eq!(homes::home_at(flat.strip, s, x, flat.floor + 1.0), Some(flat.id));
    let bed_at = (flat.rect.s0 + f32::from(mid.0) / 10.0, flat.rect.x0 + f32::from(mid.1) / 10.0);
    assert!((s - bed_at.0).hypot(x - bed_at.1) < 2.5, "beside the bed: {s} {x}");
    let status = server.status();
    assert_eq!(
        (&status["game"]["homes"]["held"], &status["game"]["homes"]["asleep"]),
        (&1.into(), &0.into())
    );
    let asleep = || {
        let kept = std::fs::read(dir.join("homes.json")).ok()?;
        let kept: Homes = serde_json::from_slice(&kept).ok()?;
        Some(kept.asleep_in(flat.id))
    };
    assert_eq!(asleep(), Some(vec![]), "written up");
    // Standing there (the door worked from inside), they log out: asleep in it again.
    b.set_pose(PersonPose {
        strip: flat.strip,
        x,
        s,
        h: flat.floor,
        grounded: true,
        ..PersonPose::default()
    });
    b.ask(&Request::Door { home: flat.id.0, open: true }).await?;
    b.ask(&Request::Door { home: flat.id.0, open: false }).await?;
    b.close().await;
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while asleep() != Some(vec![(bed, "Ada".to_string())]) {
        anyhow::ensure!(std::time::Instant::now() < end, "not asleep in the bed: {:?}", asleep());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    server.shutdown();

    // And after a restart, they wake there again.
    let server = bc_server::start(config(Some(dir.clone()))?).await?;
    let cfg = BotConfig { server: format!("http://{}", server.http_addr), ..cfg };
    let mut b = BotClient::connect_as(&cfg, Some(&wallet())).await?;
    b.wait_until(5.0, "waking at home again", |c| c.hangar.in_city() && c.hangar.wake.is_some()).await?;
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    b.close().await;
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
