# CLAUDE.md — Before Colony

**MVP fast-iteration mode**, as in Gates (operator, 2026-11-04). Ship code.
This page is the whole process. The longer page it replaced (every system
described, every e2e suite narrated, the trailer, publishing, suit renders) is
in git: `git show 3779604:CLAUDE.md` (a shallow clone needs
`git fetch --unshallow` first). Grep it when something weird happens. Don't
read it up front.

## What this is

A space mech MMO in the browser: free aim, Newtonian 6DOF. A Rust server and a
Bevy 0.19 client compiled to wasm (a WebGL2 and a WebGPU build), with
WebTransport (QUIC) between them. `docs/ARCHITECTURE.md` and `docs/DESIGN.md`
are the system and the product.

| crate | what |
|---|---|
| `bc-sim` | the simulation core: `no_std`, an allocation-free tick shared by server and client. Flight (`tuning`), combat, bodies, ground contact, the colony's and the town's closed forms (`colony::*`) |
| `bc-sector` | the hot loop: input drain, step, encode; no tokio, no locks, no allocation after start-up |
| `bc-proto` | the wire: `no_std`, no `alloc`, bit-packed, one QUIC datagram |
| `bc-server` | WebTransport sessions, sector threads, the dev HTTP (`/status`, `/floor.json`, …) |
| `bc-client` | the browser client, Bevy 0.19 → wasm32 (**a no-op natively**) |
| `bc-client-core` | the client with no transport and no Bevy: handshake, clock, prediction, interpolation, the page's types (`page`), the gallery |
| `bc-econ` | the economy: stores, recipes, the bay's suit, the fabricator, the Exchange; off the hot path |
| `bc-model` | the mechs, procedurally: the kit, the skeleton, each frame's design, as plain mesh data |
| `bc-bot` | agents playing over the same protocol as people |
| `bc-auth` | sign-in, both sides: a wallet's text (SIWE, and its Solana form) and the check of an Elo Pros pass. A pilot's id is `bc_proto::auth::PlayerId` (`0x` lowercase, base58 as written, or `acct_`); `--elo-pubkey` takes passes |
| `bc-orrery` | the Orrery tactical oracle, off the hot path |
| `bc-sound` | the generated bank, the mixer, the cockpit, the music; no Bevy, no I/O |
| `bc-alloc` | the counting allocator that proves the tick never allocates |
| `fx` | effects with no engine (pool, sprites, recipes, the fx stage's contract), **vendored from Gates** (`scripts/fx-sync.sh`). Our particles are still `bc-client/src/particles.rs`; only the stage is shared so far |

## The loop

1. Write the code.
2. `scripts/quick.sh`: fmt, the native clippy and **both wasm clippies**
   (only they compile `bc-client`). Add crate names to run their tests too,
   in release (`scripts/quick.sh bc-sim bc-sector`). Then only what you
   touched:
   - **the page or a screen:** `scripts/build-web.sh page`, then `/?gallery`
     (`scripts/e2e.sh gallery` photographs every screen);
   - **an effect:** `scripts/fx.sh <name>` (the `fx` skill);
   - **how anything looks or plays:** `scripts/pt` (the `playtest` skill; the
     `playtester` agent takes an area each).
3. Commit with a short message. Push your branch.

CI runs `scripts/ci.sh`'s checks on every push: fmt, the three clippies,
`cargo test --workspace --release`, determinism on wasm and the tick bench.
The e2e suites run nightly (`.github/workflows/e2e.yml`).

## Don't

- Update `docs/` unless asked. Do read your area's doc before changing it
  (below).
- Write findings notes, research docs or long commit bodies.
- Add a test for every change. The goldens and the `no_alloc` proofs hold a
  lot; add one when the logic is tricky or has broken before.
- Read every doc up front. Read your area's, grep the rest:

  | area | doc |
  |---|---|
  | a binding | `CONTROLS.md` |
  | the wire, replication, how sectors meet | `NETCODE.md`, `SCALE.md` |
  | a system players will compare with another game's | `PEERS.md`, `ROADMAP.md` |
  | in-game text | `STORY.md` (setting, voice and naming) |
  | the ring, its town, people, police | `FREE_ROAM.md` (wins over `RING.md`), `RING.md` (wins over `COLONY.md` and `COLONY_LOOK.md`) |
  | homes, crime, the street | `OPEN_CITY.md` |
  | the Exchange floor, `/floor.json` | `FLOOR.md` |
  | the colony's inside | `COLONY.md`, `SUITS_INSIDE.md`, `TRAINING.md`, `LIFE.md` |
  | a panel or readout | `UI.md` |
  | the lock-on | `LOCK.md` |
- Edit `crates/fx`, `scripts/fxstrip.mjs` or `scripts/board.mjs`. They are
  vendored (Gates; the Elo Pros agent kit): fix them upstream, then sync.
- Set `RUSTFLAGS`. It drops the `web_sys_unstable_apis` cfg from
  `.cargo/config.toml`.
- Name anything after another franchise. Every name is the game's own
  (`STORY.md`, "Voice and naming").

## What CI will stop you on (so you know why)

1. **The sector tick neither allocates nor locks** (`bc-sim` step plus
   `bc-sector` input drain and encode). Storage is sized at construction
   (`Box<[T]>`); `bc-sim/src/storage.rs` is the only allocation site. The
   `no_alloc` tests count heap operations (`bc-alloc::CountingAlloc`) and must
   stay at 0.
2. **`bc-proto` is `no_std` with no `alloc`**, so it cannot allocate.
3. **`bc-sim` and `bc-sector` have `clippy.toml` bans** (Mutex, RwLock,
   String, HashMap, `vec!`, `format!`, `Box::new`…). Threads talk only through
   preallocated lock-free queues (`rtrb`, crossbeam `ArrayQueue`); the sector
   thread never wakes tokio, it `unpark()`s the egress thread.
4. **Determinism.** Trig goes through `bc_sim::math` (libm). Never glam
   `fast-math` or wasm `simd128`. Native and wasm32 hit the same golden
   (`--test determinism`).
5. **The colony and the town are closed forms** of the tick and of where you
   ask: deterministic, allocation-free, never stored or sent. Their goldens
   are `CITY_GOLDEN`, `TRAFFIC_GOLDEN`, `WALKERS_GOLDEN`, `HALL_GOLDEN`,
   `TOWN_GOLDEN` and `STREET_GOLDEN` (the town's traffic and people,
   `colony::street`). A layout change bumps `content::city::CITY_VERSION` or
   `content::town::TOWN_VERSION` with the protocol. People cross only where
   no car drives (`tests/life.rs`).
6. **Shaders paint, they never re-implement the layout** (`bc::city`'s block
   atlas). The numbers they keep are checked from Rust: `city_atlas.rs`,
   `bc_client_core::station`, `bc_client_core::life`. **Derivatives are taken
   at the top of `fragment()`**, before any branch on the surface; WebGPU
   rejects them under it and naga doesn't catch it, so the tests scan for it.
7. **The page and the client agree.** `tests/page_contract.rs` reads
   `web/ui.js` against `bc_client_core::page`'s types and `terminal`'s JSON.
8. **`bc-client` compiles only for wasm.** A green native workspace says
   nothing about it; the two wasm clippies do.
9. **`crates/fx` is Gates'** (`crates/fx/tests/vendored.rs`).

## Commands

```
scripts/quick.sh [crate…]                          # the local check
scripts/ci.sh                                      # what CI runs (BC_E2E=1 adds the browser suites)
cargo run -p bc-server --release                   # then http://127.0.0.1:8080
scripts/dev.sh                                     # web build + a survival sector with drones, an agent, a miner
scripts/build-web.sh [webgl2] [webgpu] [page]      # the browser build into web/dist/
scripts/e2e.sh <suite> [webgl2|webgpu]             # Playwright against a real server (e2e/tests/*.spec.ts)
scripts/e2e.sh gfx webgl2                          # every showcase scene, screenshots in e2e/artifacts/
scripts/pt up | open | shot | showcase …           # play it from a shell (the playtest skill)
scripts/fx.sh list | <name>…                       # the fx stage: one effect's strip → e2e/artifacts/fx/<name>/
node scripts/board.mjs look                        # the reference board (board/board.json): strips beside refs and marks
scripts/fx-sync.sh [GATES_DIR]                     # the fx crate and fxstrip from a Gates checkout
scripts/trailer/make.sh                            # the trailer and its cuts (scripts/trailer/)
cargo run -q -p bc-model --example suit_json -- argo > argo.json   # a suit's design; scripts/suit-render.py renders it
```

`dev.sh` and `pt up` take `BC_RULES=arcade`, `BC_FLIGHT=real`, `BC_DATA=dir`
and `BC_COLONY=1` (the retired cylinder's inside; the ring is built without
it).

## Traps that already cost real time

- **`RUSTFLAGS` set anywhere** silently drops the WebGPU API cfg.
- **`wasm-bindgen-cli` must be exactly 0.2.128**; `build-web.sh` checks it.
- **Tests in debug crawl.** The simulation-heavy ones are slow unoptimised;
  use `--release`.
- **SwiftShader lies about the look.** It is slow, and only `quality=high`
  is the real look; screenshots are 960×540 (the playtest skill's caveats).
- **The showcase's film hook sleeps the app** once its clock stands still.
  Wake it (`window.__bcWake`), or a waiting shooter hangs.
- **`scripts/pt showcase` needs `scripts/pt up` first.**
- **Some e2e suites pass `--colony`.** `hangar`, `colony`, `inside` and
  `home` walk the retired cylinder; `base` and `town` run with it closed.
- **The ring's town is reached from the bay:** Q at the airlock rides a
  spoke's lift down (`scripts/pt cmd '{"cmd":"hangar","req":{"t":"enter_town"}}'`,
  then `walk lift|boulevard|car`); `scripts/pt showcase town <1-8>` is its
  look at High.

## Fresh box

- `rustup show`: 1.98.1 with the wasm target (pinned).
- `cargo install wasm-bindgen-cli --version 0.2.128 --locked`.
- `cd e2e && pnpm i` for Playwright (a global one and `/opt/pw-browsers/chromium`
  also serve `scripts/fx.sh` and the board).
- Suit renders: `pip install bpy==4.2.0` (Python 3.11).
- A Gates checkout beside this one (`../Gates`) for `scripts/fx-sync.sh`.

## Rules that stay

- **Surface contact is `bc_sim::ground::move_step`,** shared by the server
  and the client's predictor. Body poses are closed forms in the integer tick
  (`bodies::landmark_pose(t)`), never replicated. A rider is body-local
  (`Anchor`); its world pose is derived every tick (`ARCHITECTURE.md`,
  "Bodies and frames").
- **A suit's stats are `bc_sim::tuning`** (frame × systems × equipment),
  built identically by the server and the owner's prediction. Anything that
  changes flight goes through it, from state the own snapshot carries; the
  sector's `FlightRules` too (the Welcome's ANIME flag tells the client).
- **The lock-on moves the suit about its target, never the aim.** What it
  decides travels in the command as `bc_proto::LockOn` (`LOCK.md`); the
  default shooter controls (`bc_client_core::upright`) ride the same `LockOn`.
- **Nothing fires inside the colony,** except in the Blast Hall
  (`colony::hall`, `HALL_GOLDEN`). The tick and the owner's prediction both
  clear the weapons of a suit outside it; its rounds touch no suit and never
  leave it.
- **The Corona's Ward stops hostile beams** where they cross its field
  (`sim::ward`, `MAX_WARDS`). The client stops a beam alike at a field its
  SPECIAL flag shows (`collide::sphere_entry`).
- **The colony's life draws with one `LifeMaterial`** that never changes
  after start-up; what changes rides in each `MeshTag`.
- **Never log signatures or resume tokens.** Shorten addresses
  (`pilots::short`); `bc_proto::auth::Signature`'s `Debug` hides its bytes on
  purpose.
- **The secret shared with Gates** (`STORY.md`) is never said in game.
- **Operator-only:** publishing the page (`scripts/publish-web.sh`) and
  deploying.
