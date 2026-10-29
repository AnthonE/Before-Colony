# Before Colony — notes for AI assistants

A space mech MMO (free aim, Newtonian 6DOF). Rust server, Bevy 0.19 client compiled to wasm,
WebTransport (QUIC) between them. See `docs/ARCHITECTURE.md` and `docs/DESIGN.md`; `docs/CONTROLS.md`
surveys what players of similar games expect of the controls (read it before changing a binding);
`docs/NETCODE.md` is the netcode of record (what we keep, what we took from whom, what we turned down, and the road to one world of many sectors: read it before changing the wire, replication or how sectors meet), `docs/SCALE.md` what stops the world growing, with numbers;
`docs/PEERS.md` is what the nearest games teach us, with priorities (read it before adding a system players will
compare with theirs), and `docs/ROADMAP.md` how each is built;
`docs/STORY.md` is the world bible (setting, factions, eras, voice, and the secret it shares with Gates: read it before writing in-game text; every name is the game's own, never another franchise's: its "Voice and naming");
`docs/RING.md` is the plan of record for the First Colony as a 1 km ring, Gundam Wing's look in our own designs (painted anime, the base in the hub, buildings from Gates' pieces): it supersedes the cylinder of `COLONY.md` and `COLONY_LOOK.md` where they disagree, so read it before touching the colony;
`docs/OPEN_CITY.md` is the open city (homes at real addresses, not instanced, on Rust's rules taken from Gates; then the street in the
tick, cars you own, heat and the Watch, intrusion, raids, businesses: read it before touching homes, crime or the street);
`docs/FLOOR.md` is the Exchange floor seen from the Elo Pros store (one room, two clients: read it before touching the floor or `/floor.json`);
`docs/COLONY.md` is the plan for the First Colony's inside (Milestone 5), `docs/SUITS_INSIDE.md` the design for
suits inside it, `docs/TRAINING.md` the Proving Ground (training inside it, after X-Wing and TIE Fighter), `docs/COLONY_LOOK.md` its look (Rust's realism, Phantasy Star Online's clean colony, anime behind)
and the passes towards it; `docs/LIFE.md` is living in it (jobs as seats worked by Arrivals or the colony's staff, the
body and its meals on the wall clock, the great works as the server's raid); `docs/UI.md` is the UI's look (what the nearest mech games teach, and the rules every panel and readout keeps to: read it before adding one); `docs/LOCK.md` is the lock-on (it moves the suit about its target, never the aim: what it decides
travels in the command as `bc_proto::LockOn`, so prediction stays exact); the shooter controls, the default
(`bc_client_core::upright`: the view upright, W where the pilot looks), ride the same `LockOn` with nothing to hold to.

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
  `city_facade.wgsl`) are checked by `city_atlas.rs`'s tests; `station.wgsl`'s surfaces, flags and uniform by
  `bc_client_core::station`'s, which also hold its derivatives above its surface branch and none in `anime.wgsl`
  (`bc::anime`, the painted look the hull and station shaders share). In `city.wgsl` every derivative is taken at the top
  of `fragment()`, before the surface branch (WebGPU rejects them under it; naga doesn't catch it).
- The Corona's Ward (`sim::ward`) stops hostile beams where they cross into its field: the tick keeps the fields
  standing in a fixed list (`MAX_WARDS`), and the client stops a beam at a field its SPECIAL flag shows, alike
  (`bc_sim::collide::sphere_entry`).
- Never log signatures or resume tokens (addresses shortened: `pilots::short`). `bc_proto::auth::Signature`'s
  `Debug` hides its bytes on purpose.

## Commands
- `scripts/ci.sh` — everything CI runs (`BC_E2E=1` adds the browser tests).
- `scripts/dev.sh` — build the web client, run a survival sector with drones, an AI agent and a miner (`BC_RULES=arcade` for the arcade rules, `BC_FLIGHT=real` for the simulator's flight instead of anime rules, `BC_DATA=dir` to keep pilots and the exchange; the colony's inside is open by default under survival rules, `BC_COLONY=0` closes it: the server's `--no-colony`).
- `cargo test --workspace --release` — all native tests (bc-client is a no-op natively; release
  because the simulation-heavy tests are slow unoptimised).
- `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo clippy -p bc-client --target wasm32-unknown-unknown -- -D warnings` and
  `cargo clippy -p bc-client --target wasm32-unknown-unknown --features webgpu -- -D warnings`
- `scripts/build-web.sh [webgl2] [webgpu]` — browser build into `web/dist/` (needs wasm-bindgen-cli 0.2.128).
- The page around the game (`web/ui.js`: menus, terminals, desks) is drawn from `bc_client_core::page`'s types
  (`View`, `Init`, `SettingsView`, `Cmd`) and `terminal`'s JSON; `tests/page_contract.rs` reads `ui.js` and fails on a
  field or command one side has and the other doesn't. A new screen or tab gets a scene in `bc_client_core::gallery`:
  `scripts/build-web.sh page` (no wasm needed) then `/?gallery` shows every screen with made-up data, `[`/`]` to step;
  `scripts/e2e.sh gallery` photographs them all into `e2e/artifacts/gallery/`. Look at it before and after a change to
  the page's look.
- `cargo run -p bc-server --release` then open http://127.0.0.1:8080
- `scripts/publish-web.sh` — the browser build onto the Elo Pros origin (`/data/apps/scry-data/before-colony-web/<build>/`, `current` flipped last; served at elopros.com/games/before-colony/play/). Its `config.js` names the game server (`web/config.js`: empty, the page's own server; `?server=host:port`, `?http=url` override it), since that page dials game.elopros.com from another site. The page has no inline script (the store's CSP); the server's `--siwe-domain` lists both hosts, `--cors-origin` (default https://elopros.com) may read `/status`, `/status.json` (the launcher's `{players, max_players, tick}`), `/online.json` (the signed-in wallets, for the store's playtime: `bc_server::online`), `/floor.json` (who stands on the Exchange floor and its boards, for the store's trade terminal: `bc_server::floor`, `docs/FLOOR.md`) and `/cert-hash`.
- `scripts/pt` — play the game from a shell, to playtest it and look for what needs polish (`.claude/skills/playtest/SKILL.md`; the `playtester` agent, `.claude/agents/`, takes an area each): `scripts/pt up` starts a world (survival, the colony open, drones, an agent, a miner, strollers), `scripts/pt open` a browser kept open on it (`PT_SESSION` names each), then `shot` (a PNG to read), `around`, `ui` (the page's words), `walk <spot>`, `use`, `key`, `turn <yaw> [pitch]` (the `look` dev hook: the mouse can't steer), `profile <secs>`, `cmd '<bcInbox json>'`, `wait '<js over s = __bc>'`, `note`; out in `e2e/artifacts/playtest/<session>/`.
- `scripts/trailer/make.sh` — the alpha trailer (`out/before-colony-alpha.mp4`), cut like Gates' to the title theme from
  showcase footage and the game's own sounds (`bc-sound`'s `bank` example); `make.sh stills` the store's frames. `film.mjs`
  films a shot list (`shots/*.json`) a frame at a time through the showcase's film hook (`window.__bcFilm`, `showcase::film`:
  the clock stepped and held, the camera turned, zoomed or kept on a suit), so SwiftShader's pace never shows.
- Looking at a suit's design outside the game: `cargo run -q -p bc-model --example suit_json -- argo > argo.json`
  (add `head=altair`, `arms=corvus`… to swap sections, `far` for the far level of detail; `sagittarius` and
  `corona`, the Almagest's prototype pair, render in their own liveries), then
  `python3 scripts/suit-render.py argo.json out/argo [sheet art figure head …] [--turn Head:0,40,0]` renders it in
  Blender (`pip install bpy==4.2.0`, Python 3.11). Compare against the reference art before and after a design change.
  `--look cel|kit|real|worn` tries other surfaces on it (anime cel shading and ink, a model kit, realistic painted armour with decals, battle-worn: `scripts/suit_looks.py`).
- `scripts/e2e.sh spike|slice|gfx|frames|lockon|ui|login|hangar|base|surface|colony|chart|inside|home [webgl2|webgpu]` — Playwright against a real server (`frames`: the autopilot flies each Lodestar; `lockon`: Y locks on to a drone, W carries the suit in; `gfx`: every showcase scene, screenshots in `e2e/artifacts/` (`ring` is the Ring Deck, the colony's station language at a suit's scale as a test set: `bc_client_core::ring_deck`; `base` the Muster, Bay Row and the test hall in it at a person's: `muster`, `piece_mesh`; `docs/RING.md`); `ui`: the page around the game; `login`: wallet sign-in with a stub wallet; `hangar`: survival on foot, the terminals (the bay's pointing at the city's desks), launching and docking; `base`: out of the bay's airlock onto Bay Row, the base's deck in the hub (`docs/RING.md`), into the test hall to its counter and back in, and two browsers meeting on the row; `surface`: the lander autopilot lands in Fornax's Aft Well, hides, parks and wakes there; `colony`: down the cap lift into the city, in to the Proving Ground's desk, the Exchange floor's counter (an order resting on the book) and the Charter Board's desk (a delivery for standing, a contract posted and withdrawn), to a sight, The Arrival's seats and its counter's Most Wanted (salvage terms), each desk's objective done in turn, and back up, a tram, a car, a Markab (the rocket car, `bc_client_core::markab`: backed up Hub Gate's terminal on its fan, dropped off it, a leap), the Canal strip's cap lift (Q at the airlock) and back up, and two browsers: two pilots meet at Hub Gate, one rides home, and an agent's suit (`suit_inside`) comes down onto the avenue by the other; `chart`: the 3D chart out to Earth-Moon space, a course to Fornax's Aft Well and the auto-nav flying it; `inside`: Q at the cockpit launches the suit into the colony by the inner gate, it flies a way down it and docks back; another lands on the avenue with the grip armed, sees the stroller there and walks it; a third flies into the Blast Hall and its training rounds score on its targets; a fourth walks to the hall's gantry, boards a trainer, starts the drill and docks back on foot; `home`: two pilots signed in with stub wallets wake in neighbouring flats (a register seeded by `bc-server`'s `seed_homes` example), one sets her code and puts a sofa down, the other knocks, is shocked, enters the code and comes in, and she logs out asleep in her bed and wakes there). The suits' suites (`slice`, `frames`, `lockon`, `ui`, `login`, `surface`, `chart`) run `--rules arcade` (`BC_RULES` overrides); `hangar`, `base`, `colony`, `inside` and `home` always run survival, the server's default. Arguments after the project go to Playwright (`scripts/e2e.sh gfx webgl2 --grep "duel|hangar"`); `BC_GFX_EXTRA="look=0"` adds query parameters to every gfx shot (and its file name).
- A suit's stats are `bc_sim::tuning` (frame × systems × equipment), built identically by the server and the
  owner's prediction: anything that changes flight goes through it, from state the own snapshot carries.
  The sector's flight rules (`tuning::FlightRules`: anime, the default, or real) go through it too; the
  Welcome's ANIME flag tells the client.
- Survival's economy is `bc-econ` (off the hot path); the hangar's JSON messages are `bc_econ::wire` on control-stream frames tagged 11 (`docs/PROTOCOL.md`).
- Never set `RUSTFLAGS` (it would drop the `web_sys_unstable_apis` cfg from `.cargo/config.toml`).
