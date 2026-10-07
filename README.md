# Before Colony

A Gundam Wing mobile-suit MMO prototype.
- Free-aim, Newtonian combat in the Earth Sphere.
- Mobile Doll AI, and AI agents that play by the same rules as humans.
- The **ZERO System**: a combat AI in your cockpit that predicts the fight's futures (optionally
  asking TypeSafe's **Jev**) and seizes the controls when its pilot can't take any more.
- Salvage and mining: shoot limbs off and tow the hulks, cut rocks apart with a beam saber, and
  bring it all home.
- **The chart (M):** a holographic 3D map you fly the view through, from a single rock out to
  Earth, the Moon and the five Lagrange points at their true distances. Pick anywhere, set a course
  (it goes round the colony and the landmarks), and the auto-nav flies it.
- **The year before the colony calendar begins.** The first colony at L1 has just opened, and the
  pilots, people and AI agents alike, are Arrivals from a world where After Colony is a story
  they already know (`docs/STORY.md`).
- **Suits break from the inside.** Behind each part's armour are its systems (reactor, tank,
  cockpit, thrusters, sensors, actuators): blows through thinning armour damage them, and a
  damaged suit coughs, leaks, scrams, jams and smokes. Twelve equipment modules (a G-seat, an
  auxiliary tank, radiators, plating, damage control…) trade one stat for another.
- **Survival:** you build your own suit. You start on foot in your hangar bay in the colony's
  docking hub, with a worn-out Leo. Walk the bay in first person, fabricate parts from what you
  mine and salvage, and trade on the **Colony Exchange** (order books, with the colony as a market
  maker whose prices follow its stock). A Gundam takes tonnes of exotic metals and gundanium that
  only the colony's zero-G foundry can make. Launch out of your bay's own door on the spinning bay
  ring, and come home through the docking hub (at rest in the dock's ring of lights, or landed on
  the hub's end face and walked in at its deck hatch); a suit destroyed out there is gone. Your
  bay's airlock leads down the cap lift into the colony's city, where you walk, ride its trams and
  drive its cars, and the Proving Ground trains you.

- **Server:** Rust. One allocation-free, lock-free simulation thread per sector at 30 Hz.
- **Client:** Bevy 0.19 compiled to WebAssembly, in the browser (WebGL2 or WebGPU).
- **Transport:** WebTransport, HTTP/3 over QUIC. Unreliable datagrams carry inputs and snapshots.

**Status:** Milestone 4, wear and tear: the systems inside the parts, statuses, equipment
modules and overhauls, and the world bible (`docs/STORY.md`). Milestone 3 brought survival: the
hangar bay on foot, building suits, and the Colony Exchange (see `docs/DESIGN.md`). Milestone 2
brought the five Gundams. One sector (L1 Colony
Cluster). Pilots and agents fly
the Leo, Wing Gundam Zero (which folds into Neo-Bird), Heavyarms, Deathscythe, Sandrock and
Shenlong, against Taurus and Virgo Mobile Dolls. Each Gundam brings its kit and its signature:
- **Heavyarms:** lock-on homing missiles, and the Full Open Attack.
- **Deathscythe:** the Hyper Jammer, which hides it from enemy sensors, dolls, ZERO and missile
  seekers, and a beam scythe.
- **Sandrock:** twin heat shotels, and the Cross Crusher.
- **Shenlong:** the Dragon Fang, a claw that strikes 35 m out, and a flamethrower that overheats its
  target.
- **Wing Zero:** Neo-Bird, and the Twin Buster Rifle; the ZERO System in both forms.

Salvage and mining, and agents via the Bot SDK, carry over from Milestone 1. The suits are
procedural, jointed models whose armour and limbs come off. The sky, colony and asteroid field are
drawn with custom shaders, and every sound is synthesised at boot (`bc-sound`).

## Quick start

Prerequisites:
- Rust (the toolchain is pinned in `rust-toolchain.toml`; rustup installs it)
- `cargo install wasm-bindgen-cli --version 0.2.128 --locked`
- Node 18+ (compresses the wasm)

```sh
scripts/dev.sh                   # builds the client, starts a survival sector with 24 Mobile Dolls, an AI agent and a miner
BC_RULES=arcade scripts/dev.sh   # the arcade rules instead: any frame, free respawns
```

Open <http://127.0.0.1:8080> in Chrome or Edge, enter a callsign and LAUNCH. You come in through
your bay's airlock: click the game to look around, walk with W/A/S/D, and press E at the
fabricator, the stores' racks, the exchange terminal, the suit's console or (up the stairs, on the
catwalk) the cockpit hatch. There E launches you into space: the catapult throws your suit out of
your bay's own door on the spinning bay ring, at the ring's 125 m/s. Q launches it into the colony
instead, out of the port by its inner gate. Out there, go home through the docking hub on the
colony's axis: come to rest inside the dock's ring of lights off the hub's mouth and press Enter, or
arm the grip (L), land on the hub's end face near its middle, walk onto the deck hatch and press
Enter. On foot, E at the airlock rides the cap lift down into the colony's city (M for its map, the
trams stop at the platforms, E at a motor pool takes a car), and E at Hub Gate's lift brings you
back up. The colony is open by default; `BC_COLONY=0 scripts/dev.sh` (the server's `--no-colony`)
closes it. Esc opens the menu and F1 lists the controls. `?autoplay=1`
skips the title screen. Add `?autopilot=1` to watch the kit-aware Mobile Doll brain fly your suit
with the ZERO System engaged (it walks to the cockpit and launches first), and, under arcade rules,
`?frame=leo|wingzero|heavyarms|deathscythe|sandrock|shenlong` to pick it.
`?quality=low|medium|high|ultra` picks a graphics tier for the visit (F10 cycles them; the settings
keep the choice). Without a server,
`?showcase=gundams|lineup|duel|colony|field|sky|chase|salvage|mining|hangar` plays an offline scene
(`?hz=20` runs its clock slower, to see effects at a low frame rate). `?look=0` turns off the game's
own look (its grade, vignette, lit smoke and the bay's haze) and `?tonemap=agx|aces` swaps the
tonemapper, for comparing.

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
| Enter | dock: at rest inside the dock's ring of lights, or standing on the hub's deck hatch, into your bay |
| 1–6 | arcade rules: respawn as Leo, Wing Zero, Heavyarms, Deathscythe, Sandrock or Shenlong |
| Esc · F1 · F10 | menu · controls · graphics quality |

On foot: the mouse looks, W/A/S/D walk, Shift runs, Space jumps, E uses what you look at.

Frames with missiles lock on by themselves: hold the reticle on a hostile until its bracket reads
LOCKED, then fire.

**Signing in.** CONNECT WALLET on the title screen signs you in with an Ethereum wallet (MetaMask
or any `window.ethereum` extension). The wallet shows a Sign-In with Ethereum message (EIP-4361)
that proves the address is yours; it authorizes nothing and moves no funds. Guests can fly too, but
only a signed-in pilot's hangar (and suit) is theirs to come back to. A dropped link, or a reload, reconnects
without asking the wallet again; signing in from a second window takes the pilot over.

**Logging off.** Signed in, SLEEP & DISCONNECT (or just closing the tab) leaves your Gundam in the
sector with you asleep in the cockpit, drifting on as it was; you wake in it when you're back. Rest
against an asteroid first (the HUD reads PARKED) and it stays put there, hidden from sensors beyond
400 m. Mobile Dolls leave sleepers alone, but other pilots can hunt them.

Server flags: `--rules survival|arcade` (default survival), `--no-colony` (close the colony: no cap
lifts down into its city, no suits inside it; open by default under survival rules), `--data-dir DIR` (keep pilot records,
their hangars, and the exchange in files there; otherwise they last one run), `--craft-speed X`
(the fabricator works X times faster, for testing), `--mobile-dolls N`, `--max-clients N`,
`--oracle local|jev`, `--mode echo`, `--siwe-domain HOST` (the host pages are served from, which
wallets sign in to; defaults to `--http`), `--require-auth` (no human guests). Records go through a
`PilotStore` trait a Redis or Mongo store can implement.

### The ZERO System with TypeSafe Jev

```sh
TYPESAFE_API_KEY=... cargo run -p bc-server --release -- --oracle jev
TYPESAFE_API_KEY=... cargo run -p bc-zero --example jev_smoke   # one live call, printed
```

Without a key the in-sim local oracle runs alone, and the System works fully offline.

### Agents

```sh
cargo run -p bc-bot --release --example mobile_doll -- --name Agent-01 --faction colonies
cargo run -p bc-bot --release --example miner -- --name Miner-01   # mines, docks, and sells its ore on the exchange
cargo run -p bc-bot --release --bin bc-swarm -- --bots 64 --secs 60   # load test
```

Agents speak the same protocol through the same client core as the browser. They are shown in-game
as **MD** pilots. Two brains come with it: `DollBrain` (the Mobile Doll AI) and `MinerBrain`. Or
write your own brain as a closure: see `crates/bc-bot/src/lib.rs`.

## What's been measured

| Claim | Result |
|---|---|
| The sector tick never allocates | 0 heap operations over 1,000 ticks of 64 pilots + 256 Mobile Dolls, with the Gundams duelling (jammers, Neo-Birds, Full Opens), and with 500 missiles in the air; and in the live server under a 64-bot swarm (`/status` → `hot_path_allocations`) |
| Tick cost (64 pilots + 256 dolls in a dense fight) | p50 1.1 ms, p99 2.1–3.0 ms across runs (budget 33 ms) |
| Tick cost with 64 Gundam pilots duelling in every playable frame (about 200 missiles in the air) + 256 dolls | p50 0.84 ms, p99 2.5 ms |
| Native and browser simulate identically | the same golden state hashes on x86_64 and wasm32, for the slice and for a duel of every Gundam |
| Own-suit prediction over a bad link (100 ms RTT, 5% loss) | error p99 0.2 mm, changing into Neo-Bird and back included; a suit with failing systems (coughing thrusters, a leaking tank, a hurt pilot) and equipment fitted, p99 under 0.1 mm |
| Blows that reach a suit's systems (a Leo's torso, over its life) | machine cannon 2.1, beam rifle 1.7, beam saber 1.2; Mobile Dolls about 0.6× |
| Clock sync when inputs come in bursts (sent twice a second, same link) | RTT estimate within 10 ms of true; 1 of 600 ticks without a command |
| Salvage on the same link | chunks drawn exactly where the server has them (bit-identical at every tick, across bounces); towing a 6 t hulk, prediction error p99 0.1 mm; coasting through a shattered rock, p99 below 0.1 mm |
| A miner agent on the same link | breaks a rock with its saber, stows the ore, and sells it at the dock 178 s after spawning |
| ZERO reads Mobile Dolls | top-1 maneuver prediction 87% (chance 14%); calibration error 0.06 |
| The kit-aware pilot flies each Gundam (simulation, against three Taurus and a Virgo) | Heavyarms: Full Open, 22 missiles in the air at once, 32 missile hits; Deathscythe: jams 9 times, 3 scythe hits; Sandrock: 23 missile hits; Shenlong: 7 fang and glaive hits, 24 flame hits; Wing Zero: out as Neo-Bird and back, hits with both |
| Swarm: 64 agents (all six frames) vs 256 dolls over real WebTransport | 30 snapshots/s each, ≤ 1,100 B datagrams, tick p99 ≤ 4 ms, 0 hot-path allocations; 0–53 of 115,200 snapshots dropped at the egress ring across three runs, with the server and all 64 bots sharing 4 cores |
| The browser client, end to end (headless Chromium, software rendering) | connects, sees 20+ contacts and the MD agent, draws ZERO futures; the autopilot lands server-confirmed hits within 30 s; 0 hot-path allocations. Each Gundam, flown by the autopilot against 24 dolls, shows its mechanics to the server and the client: Heavyarms in 22 s, Deathscythe 23 s, Sandrock 12 s, Shenlong 41 s, Wing Zero (out as Neo-Bird and back) 1.6 min |

## Known limitations

- Only Chromium has been tested (see Quick start), and only the WebGL2 build. The WebGPU build
  (`scripts/build-web.sh webgpu`) compiles, but it is untested: in the test sandbox, Chromium's
  software WebGPU loses its device at startup, even on a bare WebGPU page with no Bevy.
- The browser build is large: 49 MiB of wasm, 7 MiB over the wire with brotli. Trimming Bevy
  features and running `wasm-opt` (`BC_WEB_OPT=1`) are the next steps.
- Wing Zero's change into Neo-Bird isn't animated yet (the model swaps, with a flash), and
  missiles can't be shot down yet.
- One sector, and one colony (one exchange). Without `--data-dir`, pilot records and hangars are
  kept in memory, so a server restart forgets them; a Redis or Mongo `PilotStore` is the next step.
  Suits left asleep in the sector don't survive a restart (their pilots' suits are towed home).
  The roadmap is in `docs/DESIGN.md`.

## Repository

```
crates/bc-proto        wire protocol (no_std, no alloc)
crates/bc-sim          simulation core (no_std): flight, combat, lag comp, Mobile Dolls, ZERO
crates/bc-sector       the hot loop: sector thread, lock-free queues, replication
crates/bc-zero         tactical oracles: TypeSafe Jev, worker
crates/bc-client-core  client state machine shared by the browser and bots (and the bay's walker)
crates/bc-econ         the economy: items, recipes, stores, suit builds, faults and overhauls, jobs, the Colony Exchange
crates/bc-server       WebTransport server, dev HTTP (/cert-hash, /status)
crates/bc-bot          Bot SDK, mobile_doll and miner agents, bc-swarm
crates/bc-client       Bevy browser client (wasm32)
crates/bc-model        the suits' procedural designs and their sockets
crates/bc-sound        the generated sound bank, mixer, cockpit sounds and music
crates/bc-alloc        counting allocator for the zero-allocation proofs
docs/                  DESIGN.md · ARCHITECTURE.md · PROTOCOL.md · STORY.md (the world) · CONTROLS.md
web/, scripts/, e2e/   page shell, build and dev scripts, Playwright tests
```

`scripts/ci.sh` runs everything CI does: format, clippy (the hot-path bans are errors), all tests,
wasm determinism, and a benchmark smoke run. `BC_E2E=1 scripts/ci.sh` adds the browser tests.

## Legal

Fan project. *Mobile Suit Gundam Wing* and all related names are © Sotsu · Sunrise. Canon names
live only behind the `canon-names` feature of `bc-sim`; without it the game builds with generic
names. All art is procedural. Code is MIT-licensed (see `LICENSE`).
