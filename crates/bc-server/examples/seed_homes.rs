//! Seeds a data directory's register of homes for the browser's `home` suite (`scripts/e2e.sh
//! home`, `e2e/tests/home.spec.ts`): the stub wallet's test keys 1 and 2 (`bc-auth`'s `sign`
//! example) each rent one of two neighbouring flats on strip 0, a bed in the far corner of each,
//! and are asleep in it, so both browsers wake at home without a walk across the city. Prints the
//! two flats (`{"a": {"id", "address"}, "b": …}`).
//!
//! ```sh
//! cargo run -p bc-server --example seed_homes -- /tmp/bc-data > seed.json
//! ```

use bc_auth::LocalWallet;
use bc_econ::Hangar;
use bc_econ::homes::{Homes, address};
use bc_sim::colony::city::{CITY, ROWS, Stage, block, lots};
use bc_sim::colony::homes::{self, Home, Kind, Plan};
use bc_sim::content::homes::{Furniture, bought, fits, piece_box};

/// Test key `k`'s record (`pilots::key`).
fn pilot(k: u8) -> String {
    let mut secret = [0u8; 32];
    secret[31] = k;
    let wallet = LocalWallet::from_secret(&secret).expect("a valid test key");
    bc_server::pilots::key(&wallet.address())
}

/// The first building of flats on strip 0 with two flats to let side by side on its ground floor.
fn neighbours() -> (Plan, Home, Home) {
    (CITY.0..CITY.1)
        .flat_map(|bx| (1..=ROWS).filter_map(move |row| block(0, bx, row, Stage(0))))
        .find_map(|b| {
            lots(&b).as_slice().iter().find_map(|bd| {
                let p = homes::plan(&b, bd).filter(|p| p.kind == Kind::Flats)?;
                let (a, n) = (p.home(0, 0)?, p.home(0, 1)?);
                (!bought(&a) && !bought(&n)).then_some((p, a, n))
            })
        })
        .expect("two flats side by side")
}

/// Where a bed goes in `home`: where it fits, as far from the door as it can be (dm from the
/// home's floor's corner, quarter turns).
fn far_corner(plan: &Plan, home: &Home) -> ((u16, u16), u8) {
    let (w, l) = ((home.rect.width() * 10.0) as u16, (home.rect.length() * 10.0) as u16);
    let door = (home.inside.0 - home.rect.s0, home.inside.1 - home.rect.x0);
    let mut best = None;
    for s in (0..w).step_by(5) {
        for x in (0..l).step_by(5) {
            for turn in 0..2 {
                let at = (f32::from(s) / 10.0, f32::from(x) / 10.0);
                let b = piece_box(home, Furniture::Bed, at, turn, 0);
                if fits(home, plan, Furniture::Bed, &b, 0, core::iter::empty()).is_err() {
                    continue;
                }
                let far = (at.0 - door.0).hypot(at.1 - door.1);
                if best.is_none_or(|(f, _, _)| far > f) {
                    best = Some((far, (s, x), turn));
                }
            }
        }
    }
    let (_, at, turn) = best.expect("room for a bed");
    (at, turn)
}

fn main() -> anyhow::Result<()> {
    let dir = std::env::args().nth(1).ok_or_else(|| anyhow::anyhow!("usage: seed_homes <data dir>"))?;
    std::fs::create_dir_all(&dir)?;
    let (plan, a, b) = neighbours();
    let mut reg = Homes::new();
    let now = bc_server::pilots::unix_now();
    for (k, name, home) in [(1, "E2E-Ada", &a), (2, "E2E-Bo", &b)] {
        let who = pilot(k);
        let mut bay = Hangar { credits: 100_000, ..Hangar::starter() };
        reg.rent(home, &who, name, 2, &mut bay, now).map_err(anyhow::Error::msg)?;
        let (at, turn) = far_corner(&plan, home);
        reg.furnish(home.id, &who, Furniture::Bed, at, turn, 0, &mut bay).map_err(anyhow::Error::msg)?;
        reg.sleep(home.id, &who, name, (0.0, 0.0), 0).ok_or_else(|| anyhow::anyhow!("no bed to sleep in"))?;
    }
    std::fs::write(std::path::Path::new(&dir).join("homes.json"), serde_json::to_vec(&reg)?)?;
    let line = |h: &Home| serde_json::json!({ "id": h.id.0, "address": address(h) });
    println!("{}", serde_json::json!({ "a": line(&a), "b": line(&b) }));
    Ok(())
}
