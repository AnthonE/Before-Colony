# Before Colony — notes for AI assistants

A Gundam Wing space MMO (free aim, Newtonian 6DOF). Rust server, Bevy 0.19 client compiled to wasm,
WebTransport (QUIC) between them. See `docs/ARCHITECTURE.md` and `docs/DESIGN.md`; `docs/CONTROLS.md`
surveys what players of similar games expect of the controls (read it before changing a binding);
`docs/PEERS.md` is what the nearest games teach us, with priorities (read it before adding a system players will
compare with theirs), and `docs/ROADMAP.md` how each is built;
`docs/STORY.md` is the world bible (setting, factions, eras, voice, and the secret it shares with Gates: read it before writing in-game text);
`docs/COLONY.md` is the plan for the First Colony's inside (Milestone 5), `docs/SUITS_INSIDE.md` the design for
suits inside it, `docs/TRAINING.md` the Proving Ground (training inside it, after X-Wing and TIE Fighter), `docs/COLONY_LOOK.md` its look (Rust's realism, Phantasy Star Online's clean colony, anime behind)
and the passes towards it; `docs/LIFE.md` is living in it (jobs as seats worked by Arrivals or the colony's staff, the
body and its meals on the wall clock, the great works as the server's raid); `docs/LOCK.md` is the lock-on (it moves the suit about its target, never the aim: what it decides
travels in the command as `bc_proto::LockOn`, so prediction stays exact).

## Hot-path rules (non-negotiable)
- The sector tick (`bc-sim` step + `bc-sector` input drain/encode) must not allocate or lock.
  Storage is sized at construction (`Box<[T]>`); `bc-sim/src/storage.rs` is the only allocation site.
- `bc-proto` is `no_std` with no `alloc` — it cannot allocate by construction.
- `bc-sim` and `bc-sector` have `clippy.toml` bans (Mutex/RwLock/String/HashMap/`vec!`/`format!`/`Box::new`…).
- Threads talk only through preallocated lock-free queues (`rtrb`, crossbeam `ArrayQueue`).
  The sector thread never wakes tokio; it `unpark()`s the egress thread.
- Proof: the `no_alloc` tests count heap operations with `bc-alloc::CountingAlloc` and must stay at 0.
- Determinism: use `bc_sim::math` (libm) for trig; never enable glam `fast-math` or wasm `simd128`.
- Surface contact is `bc_sim::ground::move_step` (the server and the client's predictor share it); body
  poses are closed forms in the integer tick (`bodies::landmark_pose(t)`) and never replicated; a rider is
  body-local (`Anchor`) and its world pose is derived from it every tick (`docs/ARCHITECTURE.md`, "Bodies and frames").
- The colony's inside (`bc_sim::colony`: frames, day, mirrors, the city, its traffic and people: `traffic`,
  `walkers`) is closed forms of the tick and of where you ask, deterministic and allocation-free (`CITY_GOLDEN`,
  `TRAFFIC_GOLDEN`, `WALKERS_GOLDEN`, `no_alloc`); nothing of it is stored or sent. Its law (nothing fires inside)
  has one exception, the Blast Hall (`colony::hall`, `HALL_GOLDEN`): the tick and the owner's prediction both clear
  the weapons' buttons of any suit outside it, and its rounds touch no suit and never leave it. The interior sector
  times each pilot's Proving Ground course and drill in its tick (`Sector::watch_training`, `docs/TRAINING.md`) with the
  closed forms their client runs alike (`course::Run`, stepped on the predictor's samples; `hall::Drill`, fed their own
  `TargetHit`s). People cross only where no car drives (`tests/life.rs` holds it). The client draws the life with one `LifeMaterial` that never changes after
  start-up (what changes rides in each `MeshTag`; `life_lib.wgsl`'s numbers are checked by `bc_client_core::life`). Shaders
  paint the city from its block atlas (`bc::city`) and never re-implement the layout; a layout change bumps
  `content::city::CITY_VERSION` with the protocol. The rules' numbers the shaders keep (`city_lib.wgsl`,
  `city_facade.wgsl`) are checked by `city_atlas.rs`'s tests. In `city.wgsl` every derivative is taken at the top
  of `fragment()`, before the surface branch (WebGPU rejects them under it; naga doesn't catch it).
- Never log signatures or resume tokens (addresses shortened: `pilots::short`). `bc_proto::auth::Signature`'s
  `Debug` hides its bytes on purpose.

## Commands
- `scripts/ci.sh` — everything CI runs (`BC_E2E=1` adds the browser tests).
- `scripts/dev.sh` — build the web client, run a survival sector with Mobile Dolls, an AI agent and a miner (`BC_RULES=arcade` for the arcade rules, `BC_FLIGHT=real` for the simulator's flight instead of anime rules, `BC_DATA=dir` to keep pilots and the exchange; the colony's inside is open by default under survival rules, `BC_COLONY=0` closes it: the server's `--no-colony`).
- `cargo test --workspace --release` — all native tests (bc-client is a no-op natively; release
  because the simulation-heavy tests are slow unoptimised).
- `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo clippy -p bc-client --target wasm32-unknown-unknown -- -D warnings` and
  `cargo clippy -p bc-client --target wasm32-unknown-unknown --features webgpu -- -D warnings`
- `scripts/build-web.sh [webgl2] [webgpu]` — browser build into `web/dist/` (needs wasm-bindgen-cli 0.2.128).
- `cargo run -p bc-server --release` then open http://127.0.0.1:8080
- Looking at a suit's design outside the game: `cargo run -q -p bc-model --example suit_json -- leo > leo.json`
  (add `head=wingzero`, `arms=taurus`… to swap sections, `far` for the far level of detail), then
  `python3 scripts/suit-render.py leo.json out/leo [sheet art figure head …] [--turn Head:0,40,0]` renders it in
  Blender (`pip install bpy==4.2.0`, Python 3.11). Compare against the reference art before and after a design change.
  `--look cel|kit|real|worn` tries other surfaces on it (anime cel shading and ink, a model kit, realistic painted armour with decals, battle-worn: `scripts/suit_looks.py`).
- `scripts/e2e.sh spike|slice|gfx|frames|lockon|ui|login|hangar|surface|colony|chart|inside [webgl2|webgpu]` — Playwright against a real server (`frames`: the autopilot flies each Gundam; `lockon`: Y locks on to a Doll, W carries the suit in; `gfx`: every showcase scene, screenshots in `e2e/artifacts/`; `ui`: the page around the game; `login`: wallet sign-in with a stub wallet; `hangar`: survival on foot, the terminals, launching and docking; `surface`: the lander autopilot lands in MO-II's Aft Well, hides, parks and wakes there; `colony`: down the cap lift into the city, in to the Proving Ground's desk and its Exchange floor's counter, to a sight and back up, a tram, a car, and two browsers: two pilots meet at Hub Gate, one rides home, and an agent's suit (`suit_inside`) comes down onto the avenue by the other; `chart`: the 3D chart out to the Earth Sphere, a course to MO-II's Aft Well and the auto-nav flying it; `inside`: Q at the cockpit launches the suit into the colony by the inner gate, it flies a way down it and docks back; another lands on the avenue with the grip armed, sees the stroller there and walks it; a third flies into the Blast Hall and its training rounds score on its targets; a fourth walks to the hall's gantry, boards a trainer, starts the drill and docks back on foot). The suits' suites (`slice`, `frames`, `lockon`, `ui`, `login`, `surface`, `chart`) run `--rules arcade` (`BC_RULES` overrides); `hangar`, `colony` and `inside` always run survival, the server's default. Arguments after the project go to Playwright (`scripts/e2e.sh gfx webgl2 --grep "duel|hangar"`); `BC_GFX_EXTRA="look=0"` adds query parameters to every gfx shot (and its file name).
- A suit's stats are `bc_sim::tuning` (frame × systems × equipment), built identically by the server and the
  owner's prediction: anything that changes flight goes through it, from state the own snapshot carries.
  The sector's flight rules (`tuning::FlightRules`: anime, the default, or real) go through it too; the
  Welcome's ANIME flag tells the client.
- Survival's economy is `bc-econ` (off the hot path); the hangar's JSON messages are `bc_econ::wire` on control-stream frames tagged 11 (`docs/PROTOCOL.md`).
- Never set `RUSTFLAGS` (it would drop the `web_sys_unstable_apis` cfg from `.cargo/config.toml`).
