# Before Colony — notes for AI assistants

A Gundam Wing space MMO (free aim, Newtonian 6DOF). Rust server, Bevy 0.19 client compiled to wasm,
WebTransport (QUIC) between them. See `docs/ARCHITECTURE.md` and `docs/DESIGN.md`; `docs/CONTROLS.md`
surveys what players of similar games expect of the controls (read it before changing a binding);
`docs/STORY.md` is the world bible (setting, factions, eras, voice: read it before writing in-game text);
`docs/COLONY.md` is the plan for the First Colony's inside (Milestone 5), `docs/SUITS_INSIDE.md` the design for
suits inside it.

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
- The colony's inside (`bc_sim::colony`: frames, day, mirrors, the city) is closed forms of the tick and of where
  you ask, deterministic and allocation-free (`CITY_GOLDEN`, `no_alloc`); nothing of it is stored or sent. Shaders
  paint the city from its block atlas (`bc::city`) and never re-implement the layout; a layout change bumps
  `content::city::CITY_VERSION` with the protocol.
- Never log signatures or resume tokens (addresses shortened: `pilots::short`). `bc_proto::auth::Signature`'s
  `Debug` hides its bytes on purpose.

## Commands
- `scripts/ci.sh` — everything CI runs (`BC_E2E=1` adds the browser tests).
- `scripts/dev.sh` — build the web client, run a survival sector with Mobile Dolls, an AI agent and a miner (`BC_RULES=arcade` for the arcade rules, `BC_FLIGHT=real` for the simulator's flight instead of anime rules, `BC_DATA=dir` to keep pilots and the exchange, `BC_COLONY=1` to open the colony's inside: the server's `--colony`).
- `cargo test --workspace --release` — all native tests (bc-client is a no-op natively; release
  because the simulation-heavy tests are slow unoptimised).
- `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo clippy -p bc-client --target wasm32-unknown-unknown -- -D warnings`
- `scripts/build-web.sh [webgl2] [webgpu]` — browser build into `web/dist/` (needs wasm-bindgen-cli 0.2.128).
- `cargo run -p bc-server --release` then open http://127.0.0.1:8080
- `scripts/e2e.sh spike|slice|gfx|frames|ui|login|hangar|surface|colony|chart|inside [webgl2|webgpu]` — Playwright against a real server (`frames`: the autopilot flies each Gundam; `gfx`: every showcase scene, screenshots in `e2e/artifacts/`; `ui`: the page around the game; `login`: wallet sign-in with a stub wallet; `hangar`: survival on foot, the terminals, launching and docking; `surface`: the lander autopilot lands in MO-II's Aft Well, hides, parks and wakes there; `colony`: down the cap lift into the city, to its Exchange floor and back up; `chart`: the 3D chart out to the Earth Sphere, a course to MO-II's Aft Well and the auto-nav flying it; `inside`: Q at the cockpit launches the suit into the colony by the inner gate, it flies there and docks back). The suits' suites run `--rules arcade` (`BC_RULES` overrides); the server's default is survival. Arguments after the project go to Playwright (`scripts/e2e.sh gfx webgl2 --grep "duel|hangar"`); `BC_GFX_EXTRA="look=0"` adds query parameters to every gfx shot (and its file name).
- A suit's stats are `bc_sim::tuning` (frame × systems × equipment), built identically by the server and the
  owner's prediction: anything that changes flight goes through it, from state the own snapshot carries.
  The sector's flight rules (`tuning::FlightRules`: anime, the default, or real) go through it too; the
  Welcome's ANIME flag tells the client.
- Survival's economy is `bc-econ` (off the hot path); the hangar's JSON messages are `bc_econ::wire` on control-stream frames tagged 11 (`docs/PROTOCOL.md`).
- Never set `RUSTFLAGS` (it would drop the `web_sys_unstable_apis` cfg from `.cargo/config.toml`).
