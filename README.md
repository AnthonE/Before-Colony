# Before Colony

A Gundam Wing mobile-suit MMO prototype.
- Free-aim, Newtonian combat in the Earth Sphere.
- Mobile Doll AI, and AI agents that play by the same rules as humans.
- The **ZERO System**: a combat AI in your cockpit that predicts the fight's futures (optionally
  asking TypeSafe's **Jev**) and seizes the controls when its pilot can't take any more.

- **Server:** Rust. One allocation-free, lock-free simulation thread per sector at 30 Hz.
- **Client:** Bevy 0.19 compiled to WebAssembly, in the browser (WebGL2 or WebGPU).
- **Transport:** WebTransport, HTTP/3 over QUIC. Unreliable datagrams carry inputs and snapshots.

**Status:** Milestone 1, a playable vertical slice. One sector (L1 Colony Cluster), Leo and Wing
Gundam Zero for pilots, Taurus and Virgo Mobile Dolls, beam rifles, machine cannons, beam sabers,
the Twin Buster Rifle, the ZERO System, and agents via the Bot SDK.

## Quick start

Prerequisites:
- Rust (the toolchain is pinned in `rust-toolchain.toml`; rustup installs it)
- `cargo install wasm-bindgen-cli --version 0.2.128 --locked`
- Node 18+ (compresses the wasm)

```sh
scripts/dev.sh          # builds the client, starts a sector with 24 Mobile Dolls and one AI agent
```

Open <http://127.0.0.1:8080> in Chrome or Edge, then click to take control. Add `?autopilot=1` to
watch the Mobile Doll brain fly your Wing Zero with the ZERO System engaged, or `?frame=leo` to fly
a Leo.

Only Chromium has been tested. Firefox and Safari 26.4+ also ship WebTransport, but the dev server's
self-signed certificate depends on `serverCertificateHashes` pinning, and that may not work there.

| Input | Action |
|---|---|
| Mouse | aim (click locks the pointer, Esc releases it) |
| W/S · A/D · Space/C · Q/E | thrust forward/back · left/right · up/down · roll |
| Shift · X · R | boost · brake · RCS fast turns |
| LMB · RMB · F | primary · secondary · beam saber |
| V · Z | flight assist · ZERO System |
| 1 / 2 | respawn as Leo / Wing Gundam Zero |

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
cargo run -p bc-bot --release --bin bc-swarm -- --bots 64 --secs 60   # load test
```

Agents speak the same protocol through the same client core as the browser. They are shown in-game
as **MD** pilots. Write your own brain as a closure: see `crates/bc-bot/src/lib.rs`.

## What's been measured

| Claim | Result |
|---|---|
| The sector tick never allocates | 0 heap operations over 1,000 ticks of 64 pilots + 256 Mobile Dolls, in tests and in the live server under a 64-bot swarm (`/status` → `hot_path_allocations`) |
| Tick cost (64 pilots + 256 dolls in a dense fight) | p50 1.15 ms, p99 2.9 ms (budget 33 ms) |
| Native and browser simulate identically | the same golden state hash on x86_64 and wasm32 |
| Own-suit prediction over a bad link (100 ms RTT, 5% loss) | error p99 0.2 mm |
| Clock sync when inputs come in bursts (sent twice a second, same link) | RTT estimate within 10 ms of true; 1 of 600 ticks without a command |
| ZERO reads Mobile Dolls | top-1 maneuver prediction 87% (chance 14%); calibration error 0.06 |
| Swarm: 64 agents vs 256 dolls over real WebTransport | 30 snapshots/s each, 0 drops, ≤ 1,100 B datagrams, tick p99 ≤ 4 ms |
| The browser client, end to end (headless Chromium, software rendering at ~1.4 fps) | connects, sees 20+ contacts and the MD agent, draws ZERO futures; the autopilot lands server-confirmed hits and kills within 30 s; 0 hot-path allocations |

## Known limitations

- Only Chromium has been tested (see Quick start).
- The browser build is large: 46 MiB of wasm, 7 MiB over the wire with brotli. Trimming Bevy
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
crates/bc-bot          Bot SDK, mobile_doll agent, bc-swarm
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
