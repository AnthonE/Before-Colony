# Before Colony

A Gundam Wing mobile-suit MMO prototype.
- Free-aim, Newtonian combat in the Earth Sphere.
- Mobile Doll AI, and AI agents that play by the same rules as humans.
- The **ZERO System**: a combat AI in your cockpit that predicts the fight's futures (optionally
  asking TypeSafe's **Jev**) and seizes the controls when its pilot can't take any more.
- Salvage and mining: shoot limbs off and tow the hulks, cut rocks apart with a beam saber, and sell
  the ore at the colony's dock.

- **Server:** Rust. One allocation-free, lock-free simulation thread per sector at 30 Hz.
- **Client:** Bevy 0.19 compiled to WebAssembly, in the browser (WebGL2 or WebGPU).
- **Transport:** WebTransport, HTTP/3 over QUIC. Unreliable datagrams carry inputs and snapshots.

**Status:** Milestone 1, a playable vertical slice. One sector (L1 Colony Cluster), Leo and Wing
Gundam Zero for pilots, Taurus and Virgo Mobile Dolls, beam rifles, machine cannons, beam sabers,
the Twin Buster Rifle, the ZERO System, salvage and mining, and agents via the Bot SDK. The suits
are procedural, jointed models whose armour and limbs come off. The sky, colony and asteroid field
are drawn with custom shaders.

## Quick start

Prerequisites:
- Rust (the toolchain is pinned in `rust-toolchain.toml`; rustup installs it)
- `cargo install wasm-bindgen-cli --version 0.2.128 --locked`
- Node 18+ (compresses the wasm)

```sh
scripts/dev.sh          # builds the client, starts a sector with 24 Mobile Dolls, an AI agent and a miner
```

Open <http://127.0.0.1:8080> in Chrome or Edge, then click to take control. Add `?autopilot=1` to
watch the kit-aware Mobile Doll brain fly your suit with the ZERO System engaged, and
`?frame=leo|wingzero|heavyarms|deathscythe|sandrock|shenlong` to pick it. `?quality=low|medium|high|ultra`
picks a graphics tier (F10 cycles them). Without a server,
`?showcase=gundams|lineup|duel|colony|field|sky|chase|salvage|mining` plays an offline scene.

Only Chromium has been tested. Firefox and Safari 26.4+ also ship WebTransport, but the dev server's
self-signed certificate depends on `serverCertificateHashes` pinning, and that may not work there.

| Input | Action |
|---|---|
| Mouse | aim (click locks the pointer, Esc releases it) |
| W/S · A/D · Space/C · Q/E | thrust forward/back · left/right · up/down · roll |
| Shift · X · R | boost · brake · RCS fast turns |
| LMB · RMB · F | primary · secondary · melee (saber, scythe, shotels, glaive, knife) |
| H | the frame's special: Neo-Bird or the Hyper Jammer on/off; Full Open Attack or the Cross Crusher |
| V · Z | flight assist · ZERO System |
| G · B · T · J | grab (toggle) · stow · throw · jettison |
| 1–6 | respawn as Leo, Wing Zero, Heavyarms, Deathscythe, Sandrock or Shenlong |

Frames with missiles lock on by themselves: hold the reticle on a hostile until its bracket reads
LOCKED, then fire.

Server flags: `--mobile-dolls N`, `--max-clients N`, `--oracle local|jev`, `--mode echo`.

### The ZERO System with TypeSafe Jev

```sh
TYPESAFE_API_KEY=... cargo run -p bc-server --release -- --oracle jev
TYPESAFE_API_KEY=... cargo run -p bc-zero --example jev_smoke   # one live call, printed
```

Without a key the in-sim local oracle runs alone, and the System works fully offline.

### Agents

```sh
cargo run -p bc-bot --release --example mobile_doll -- --name Agent-01 --faction colonies
cargo run -p bc-bot --release --example miner -- --name Miner-01   # mines, and sells at the dock
cargo run -p bc-bot --release --bin bc-swarm -- --bots 64 --secs 60   # load test
```

Agents speak the same protocol through the same client core as the browser. They are shown in-game
as **MD** pilots. Two brains come with it: `DollBrain` (the Mobile Doll AI) and `MinerBrain`. Or
write your own brain as a closure: see `crates/bc-bot/src/lib.rs`.

## What's been measured

| Claim | Result |
|---|---|
| The sector tick never allocates | 0 heap operations over 1,000 ticks of 64 pilots + 256 Mobile Dolls, in tests and in the live server under a 64-bot swarm (`/status` → `hot_path_allocations`) |
| Tick cost (64 pilots + 256 dolls in a dense fight) | p50 1.1 ms, p99 2.1–2.7 ms across runs (budget 33 ms) |
| Native and browser simulate identically | the same golden state hash on x86_64 and wasm32 |
| Own-suit prediction over a bad link (100 ms RTT, 5% loss) | error p99 0.2 mm |
| Clock sync when inputs come in bursts (sent twice a second, same link) | RTT estimate within 10 ms of true; 1 of 600 ticks without a command |
| Salvage on the same link | chunks drawn exactly where the server has them (bit-identical at every tick, across bounces); towing a 6 t hulk, prediction error p99 0.1 mm; coasting through a shattered rock, p99 below 0.1 mm |
| A miner agent on the same link | breaks a rock with its saber, stows the ore, and sells it at the dock 178 s after spawning |
| ZERO reads Mobile Dolls | top-1 maneuver prediction 87% (chance 14%); calibration error 0.06 |
| Swarm: 64 agents vs 256 dolls over real WebTransport | 30 snapshots/s each, 0 drops, ≤ 1,100 B datagrams, tick p99 ≤ 4 ms |
| The browser client, end to end (headless Chromium, software rendering at ~1.4 fps) | connects, sees 20+ contacts and the MD agent, draws ZERO futures; the autopilot lands server-confirmed hits and kills within 30 s; 0 hot-path allocations |

## Known limitations

- Only Chromium has been tested (see Quick start), and only the WebGL2 build. The WebGPU build
  (`scripts/build-web.sh webgpu`) compiles, but it is untested: in the test sandbox, Chromium's
  software WebGPU loses its device at startup, even on a bare WebGPU page with no Bevy.
- The browser build is large: 48 MiB of wasm, 7 MiB over the wire with brotli. Trimming Bevy
  features and running `wasm-opt` (`BC_WEB_OPT=1`) are the next steps.
- One sector, no persistence or accounts yet. The roadmap is in `docs/DESIGN.md`.

## Repository

```
crates/bc-proto        wire protocol (no_std, no alloc)
crates/bc-sim          simulation core (no_std): flight, combat, lag comp, Mobile Dolls, ZERO
crates/bc-sector       the hot loop: sector thread, lock-free queues, replication
crates/bc-zero         tactical oracles: TypeSafe Jev, worker
crates/bc-client-core  client state machine shared by the browser and bots
crates/bc-server       WebTransport server, dev HTTP (/cert-hash, /status)
crates/bc-bot          Bot SDK, mobile_doll and miner agents, bc-swarm
crates/bc-client       Bevy browser client (wasm32)
crates/bc-alloc        counting allocator for the zero-allocation proofs
docs/                  DESIGN.md · ARCHITECTURE.md · PROTOCOL.md
web/, scripts/, e2e/   page shell, build and dev scripts, Playwright tests
```

`scripts/ci.sh` runs everything CI does: format, clippy (the hot-path bans are errors), all tests,
wasm determinism, and a benchmark smoke run. `BC_E2E=1 scripts/ci.sh` adds the browser tests.

## Legal

Fan project. *Mobile Suit Gundam Wing* and all related names are © Sotsu · Sunrise. Canon names
live only behind the `canon-names` feature of `bc-sim`; without it the game builds with generic
names. All art is procedural. Code is MIT-licensed (see `LICENSE`).
