# Before Colony

A space mech MMO prototype.
- Free-aim, Newtonian combat in Earth-Moon space.
- Drone AI, and AI agents that play by the same rules as humans.
- **The Orrery**: a combat AI in your cockpit that predicts the fight's futures (optionally
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
  damaged suit coughs, leaks, scrams, jams and smokes. Fourteen equipment modules (a G-seat, an
  auxiliary tank, an ion drive, radiators, plating, damage control…) trade one stat for another,
  and three grades of propellant go further the purer they are.
- **Survival:** you build your own suit. You start on foot in your hangar bay in the colony's
  docking hub, with a worn-out Argo. Walk the bay in first person, fabricate parts from what you
  mine and salvage, and trade on the **Colony Exchange** (order books, with the colony as a market
  maker whose prices follow its stock). A Lodestar takes tonnes of exotic metals and siderium that
  only the colony's zero-G foundry can make. Launch out of your bay's own door on the spinning bay
  ring, and come home through the docking hub (at rest in the dock's ring of lights, or landed on
  the hub's end face and walked in at its deck hatch); a suit destroyed out there is gone. Your
  bay's airlock leads down the cap lifts (one to each of its three strips) into the colony's
  city, where you walk, ride its trams and drive its cars (or the Markab, its rocket car, which
  drives up walls and along ceilings on its fan and flies on its jets), and the Proving Ground trains you. Its
  desks each do one thing only they do: an order that waits on the book at the Exchange floor,
  contracts, patrols, the charter and the Charter Board's pay at its desk (a flight certificate
  earns your worn Argo its missing beam rifle there), the Almagest's Most Wanted at The Arrival, and the
  test range at the Proving Ground. The objectives walk a new Arrival from desk to desk, home to
  arm, and out.

- **Server:** Rust. One allocation-free, lock-free simulation thread per sector at 30 Hz.
- **Client:** Bevy 0.19 compiled to WebAssembly, in the browser (WebGL2 or WebGPU).
- **Transport:** WebTransport, HTTP/3 over QUIC. Unreliable datagrams carry inputs and snapshots.

**Status:** Milestone 5, the First Colony's inside (`docs/COLONY.md`), most of it built: its city
at full scale, shared with other pilots, walked, ridden by tram, driven, and flown by suits; and
the Proving Ground's course, Blast Hall and drill (`docs/TRAINING.md`); and its desks, each with
its one thing, the objectives leading from one to the next. Alongside it (`docs/ROADMAP.md`): the
lock-on, the Charter Board's contracts and great works (and its pay at its desk), a floor under
loss, the colony's radio, bigger tanks, propellant grades and the ion drive, what the mech
games teach (doom and ejecting, stagger, a debrief, a test range, aces…), and what Mecha Royale
teaches (guns that shoot missiles down, the HUD saying why a weapon won't fire, a death report, a
boot that says when it's stuck, wrecks that die their own way). Milestone 4 brought
wear and tear: the systems inside the parts, statuses, equipment modules and overhauls, and the
world bible (`docs/STORY.md`). Milestone 3 brought survival: the hangar bay on foot, building
suits, and the Colony Exchange (see `docs/DESIGN.md`). Milestone 2 brought the five Lodestars. One
sector (L1 Colony Cluster). Pilots and agents fly
the Argo, Altair (which folds into Aquila), Antares, Algol, Canopus and
Thuban, against Corvus and Cetus drones. Each Lodestar brings its kit and its signature:
- **Antares:** lock-on homing missiles, and the Full Salvo.
- **Algol:** the Eclipse, which hides it from enemy sensors, drones, the Orrery and missile
  seekers, and a beam scythe.
- **Canopus:** twin heat blades, and the Pincer Strike.
- **Thuban:** the Talon, a claw that strikes 35 m out, and a flamethrower that overheats its
  target.
- **Altair:** Aquila, and the Heavy Beam Rifle; the Orrery in both forms.

Salvage and mining, and agents via the Bot SDK, carry over from Milestone 1. The suits are
procedural, jointed models whose armour and limbs come off. The sky, colony and asteroid field are
drawn with custom shaders, and every sound is synthesised at boot (`bc-sound`).

## Quick start

Prerequisites:
- Rust (the toolchain is pinned in `rust-toolchain.toml`; rustup installs it)
- `cargo install wasm-bindgen-cli --version 0.2.128 --locked`
- Node 18+ (compresses the wasm)

```sh
scripts/dev.sh                   # builds the client, starts a survival sector with 24 drones, an AI agent and a miner
BC_RULES=arcade scripts/dev.sh   # the arcade rules instead: any frame, free respawns
```

Open <http://127.0.0.1:8080> in Chrome or Edge, enter a callsign and LAUNCH. You come in through
your bay's airlock: click the game to look around, walk with W/A/S/D, and press E at the
fabricator, the stores' racks, the exchange terminal, the suit's console or (up the stairs, on the
catwalk) the cockpit hatch. There E launches you into space: the catapult throws your suit out of
your bay's own door on the spinning bay ring, at 40 m/s on top of the ring's own 125 m/s. Q
launches it into the colony instead, out of the port by its inner gate. Out there, go home through
the docking hub on the colony's axis: come to rest inside the dock's ring of lights off the hub's
mouth and press Enter, or arm the grip (L), land on the hub's end face near its middle, walk onto
the deck hatch and press Enter. Enter docks only where the HUD's marker says ENTER; anywhere else
it opens the radio. On foot, E at the airlock rides the cap lift down to the Charter strip's Hub
Gate (Q there first chooses the Canal or the Gardens strip's lift instead): M opens the city's map
(M or Esc closes it), the trams stop at the platforms, E at a motor pool takes a car (R a Markab:
F its fan, Space its lift jets, Shift its tail's, Q/E a roll in the air; `?showcase=markab` is its
stunt in the Blast Hall), and E at
Hub Gate's lift brings you back up. E at a desk's counter opens its terminal, with the one tab
that desk keeps: the Exchange floor's exchange, the Charter Board's (what it holds for you is paid
out as you walk into its hall), The Arrival's Most Wanted, the Proving Ground's board and test
range; the bay's terminals point at them. The colony is open by default; `BC_COLONY=0 scripts/dev.sh`
(the server's `--no-colony`) closes it. Esc opens the menu and F1 (or ?) lists the controls.
`?autoplay=1` skips the title screen. Add `?autopilot=1` to watch the kit-aware drone brain fly your suit
with the Orrery engaged (it walks to the cockpit and launches first), and, under arcade rules,
`?frame=argo|altair|antares|algol|canopus|thuban` to pick it.
`?quality=low|medium|high|ultra` picks a graphics tier for the visit (F10 or O cycles them; the settings
keep the choice). Without a server,
`?showcase=lodestars|lineup|duel|colony|field|sky|chase|salvage|mining|hangar|surface|city` plays an
offline scene (`?hz=20` runs its clock slower, to see effects at a low frame rate). `?look=0` turns off the game's
own look (its grade, vignette, lit smoke and the bay's haze) and `?tonemap=agx|aces` swaps the
tonemapper, for comparing.

Only Chromium has been tested. Firefox and Safari 26.4+ also ship WebTransport, but the dev server's
self-signed certificate depends on `serverCertificateHashes` pinning, and that may not work there.

| Input | Action |
|---|---|
| Mouse | aim (click locks the pointer, Esc releases it) |
| W/S · A/D · Space/C · Q/E | thrust forward/back · left/right · up/down · roll |
| Shift · X · R | boost · brake · RCS fast turns |
| L | grip: armed, coming in slow and close lands you on a rock or a landmark; again, let go |
| LMB · RMB · F | primary · secondary · melee (saber, scythe, blades, glaive, knife) |
| Y · middle click | lock on to the hostile nearest the crosshair (again: the next one; held: let go) |
| H | the frame's special: Aquila or the Eclipse on/off; Full Salvo or the Pincer Strike |
| V · Z | flight assist · the Orrery |
| Tab · mouse wheel | the cockpit (the default) or the chase camera |
| K · Alt (held) | in the cockpit: the framed monitors or the holo cockpit (see-through all round) · look round from the seat |
| M · N | the chart, out to Earth-Moon space (set a course on it) · the auto-nav on the course (on/off) |
| G · B · T · J | grab (toggle) · stow · throw · jettison |
| U | eject: doomed, a tap ejects and a hold blows the suit up; otherwise held a second |
| Enter | survival: dock where the HUD's marker says ENTER (into your bay: at rest inside the dock's ring of lights, or standing on the hub's deck hatch; inside the colony, at rest in the inner gate's ring; in a trainer, at rest on the Blast Hall's gantry); anywhere else, the radio |
| 1–4 | survival: the rack's patch kit · coolant flush · chaff · stim |
| 1–6 | arcade rules: respawn as Argo, Altair, Antares, Algol, Canopus or Thuban |
| / (or Enter) | talk on the colony's radio (Enter opens it on foot, and in a suit wherever there's nothing to dock): Enter says it, Esc closes |
| Esc · F1 or ? · F10 or O | menu · controls · graphics quality |

On foot: the mouse looks, W/A/S/D walk, Shift runs, Space jumps, E uses what you look at. With
the colony open, Q at the cockpit launches into it, and Q at the airlock chooses the next strip's
cap lift.

Frames with missiles lock on by themselves: hold the reticle on a hostile until its bracket reads
LOCKED, then fire.

**Signing in.** CONNECT WALLET on the title screen signs you in with an Ethereum wallet: any
browser extension (several make a picker), or a phone wallet where the Elo Pros store serves the
page. The wallet shows a Sign-In with Ethereum message (EIP-4361), the same one the Elo Pros
launcher signs, that proves the address is yours; it authorises nothing and moves no funds. On
elopros.com, signed in at the store is signed in here, and PLAY AS GUEST signs out of both. Guests can fly too, but
only a signed-in pilot's hangar (and suit) is theirs to come back to. A dropped link, or a reload, reconnects
without asking the wallet again; signing in from a second window takes the pilot over.

**Logging off.** Signed in, SLEEP & DISCONNECT (or just closing the tab) leaves your Lodestar in the
sector with you asleep in the cockpit, drifting on as it was; you wake in it when you're back. Rest
against an asteroid first (the HUD reads PARKED) and it stays put there, hidden from sensors beyond
400 m. Drones leave sleepers alone. Pilots of another side can hunt them, but every human
pilot flies for the Colonies (the server decides) and friendly fire is off, so today only an agent
flying for the Alliance could hunt yours (where PvP lives is an open decision: `docs/ROADMAP.md`).
Flying inside the colony nothing sleeps: DOCK & DISCONNECT, and its tugs bring the suit home to
your bay (you're told so when you're back). On foot, in the bay or the city, nothing is out, and
you wake in your bay.

Server flags: `--rules survival|arcade` (default survival), `--no-colony` (close the colony: no cap
lifts down into its city, no suits inside it; open by default under survival rules), `--data-dir DIR` (keep pilot records,
their hangars, the exchange, the Charter Board and the Proving Ground's board in files there;
otherwise they last one run), `--craft-speed X`
(the fabricator works X times faster, for testing), `--drones N`, `--max-clients N`,
`--oracle local|jev`, `--mode echo`, `--siwe-domain HOST` (the host pages are served from, which
wallets sign in to; defaults to `--http`), `--require-auth` (no human guests), `--flight anime|real`
(default anime: the tank is a boost gauge that refills; real: every newton burns propellant),
`--ace-every SECS` (between the Almagest's aces among the drones, default 300; 0: none), `--http ADDR`
(default 127.0.0.1:8080), `--wt-port N` (WebTransport's UDP port, default 4433), `--web-dir DIR`
(the built client, default `web/dist`), `--seed N` (the simulation's), `--max-connections N` and
`--per-address N` (connections taken in all and from one address, loopback excepted; default 512
and 8). Records go through a `PilotStore` trait a Redis or Mongo store can implement.

`scripts/dev.sh` takes its settings from the environment: `BC_RULES`, `BC_FLIGHT` (`anime|real`),
`BC_COLONY=0`, `BC_DATA=DIR` (the server's `--data-dir`), `BC_DRONES` (24), `BC_AGENTS` and
`BC_MINERS` (1 each), `BC_ORACLE` (`local|jev`).

### The Orrery with TypeSafe Jev

```sh
TYPESAFE_API_KEY=... cargo run -p bc-server --release -- --oracle jev
TYPESAFE_API_KEY=... cargo run -p bc-orrery --example jev_smoke   # one live call, printed
```

Without a key the in-sim local oracle runs alone, and the System works fully offline.
`TYPESAFE_BASE_URL` points the server at another Jev endpoint (a proxy, or a test's mock).

### Agents

```sh
cargo run -p bc-bot --release --example drone -- --name Agent-01 --faction colonies   # or alliance
cargo run -p bc-bot --release --example miner -- --name Miner-01   # mines, docks, and sells its ore on the exchange
cargo run -p bc-bot --release --bin bc-swarm -- --bots 64 --secs 60   # load test
```

Agents speak the same protocol through the same client core as the browser. They are shown in-game
as **DR** pilots. Two brains come with it: `DroneBrain` (the drone AI) and `MinerBrain`. Or
write your own brain as a closure: see `crates/bc-bot/src/lib.rs`. The side an agent flies for is
the server's to say, and its Welcome says it: `--faction` takes `colonies` or `alliance` (the Almagest is the
drones' own, and a claim to it flies for the Colonies, as every human pilot does).

An agent flies as a guest unless it's given a wallet: `--key agent.key` (drone, miner, flaneur,
suit_inside) signs it in with the key in that file, made there on first use and readable by its
owner only (`bc_bot::key`; never on the command line). It then has its own address, and its
hangar is kept like any signed-in pilot's. `scripts/dev.sh` does it for
every resident agent with `BC_AGENT_KEYS=dir`.

## What's been measured

| Claim | Result |
|---|---|
| The sector tick never allocates | 0 heap operations over 1,000 ticks of 64 pilots + 256 drones, with the Lodestars duelling (jammers, Aquilas, Full Salvos), and with 500 missiles in the air; and in the live server under a 64-bot swarm (`/status` → `hot_path_allocations`) |
| Tick cost (64 pilots + 256 drones in a dense fight) | p50 1.1 ms, p99 2.1–3.0 ms across runs (budget 33 ms) |
| Tick cost with 64 Lodestar pilots duelling in every playable frame (about 200 missiles in the air) + 256 drones | p50 0.84 ms, p99 2.5 ms |
| Native and browser simulate identically | the same golden state hashes on x86_64 and wasm32, for the slice and for a duel of every Lodestar |
| Own-suit prediction over a bad link (100 ms RTT, 5% loss) | error p99 0.2 mm, changing into Aquila and back included; a suit with failing systems (coughing thrusters, a leaking tank, a hurt pilot) and equipment fitted, p99 under 0.1 mm |
| Blows that reach a suit's systems (an Argo's torso, over its life) | machine cannon 2.1, beam rifle 1.7, beam saber 1.2; drones about 0.6× |
| Clock sync when inputs come in bursts (sent twice a second, same link) | RTT estimate within 10 ms of true; 1 of 600 ticks without a command |
| Salvage on the same link | chunks drawn exactly where the server has them (bit-identical at every tick, across bounces); towing a 6 t hulk, prediction error p99 0.1 mm; coasting through a shattered rock, p99 below 0.1 mm |
| A miner agent on the same link | breaks a rock with its saber, stows the ore, and sells it at the dock 178 s after spawning |
| The Orrery reads drones | top-1 maneuver prediction 87% (chance 14%); calibration error 0.06 |
| The kit-aware pilot flies each Lodestar (simulation, against three Corvus and a Cetus) | Antares: Full Salvo, 22 missiles in the air at once, 32 missile hits; Algol: jams 9 times, 3 scythe hits; Canopus: 23 missile hits; Thuban: 7 fang and glaive hits, 24 flame hits; Altair: out as Aquila and back, hits with both |
| Swarm: 64 agents (all six frames) vs 256 drones over real WebTransport | 30 snapshots/s each, ≤ 1,100 B datagrams, tick p99 ≤ 4 ms, 0 hot-path allocations; 0–53 of 115,200 snapshots dropped at the egress ring across three runs, with the server and all 64 bots sharing 4 cores |
| The browser client, end to end (headless Chromium, software rendering) | connects, sees 20+ contacts and the DR agent, draws the Orrery futures; the autopilot lands server-confirmed hits within 30 s; 0 hot-path allocations. Each Lodestar, flown by the autopilot against 24 drones, shows its mechanics to the server and the client: Antares in 22 s, Algol 23 s, Canopus 12 s, Thuban 41 s, Altair (out as Aquila and back) 1.6 min |

## Known limitations

- Only Chromium has been tested (see Quick start), and only the WebGL2 build. The WebGPU build
  (`scripts/build-web.sh webgpu`) compiles, but it is untested: in the test sandbox, Chromium's
  software WebGPU loses its device at startup, even on a bare WebGPU page with no Bevy.
- The browser build is large: 49 MiB of wasm, 7 MiB over the wire with brotli. Trimming Bevy
  features and running `wasm-opt` (`BC_WEB_OPT=1`) are the next steps.
- Altair's change into Aquila isn't animated yet (the model swaps, with a flash).
- One sector, and one colony (one exchange). Without `--data-dir`, pilot records and hangars are
  kept in memory, so a server restart forgets them; a Redis or Mongo `PilotStore` is the next step.
  Suits left asleep in the sector don't survive a restart, except, under survival rules with
  `--data-dir`, one left on its feet in a landmark's hide spot, which is put back where it was.
  Every other sleeper is cleared (under survival, towed home to its pilot's bay).
  The roadmap is in `docs/ROADMAP.md`.

## Repository

```
crates/bc-proto        wire protocol (no_std, no alloc)
crates/bc-sim          simulation core (no_std): flight, combat, lag comp, drones, the Orrery
crates/bc-sector       the hot loop: sector thread, lock-free queues, replication
crates/bc-orrery         tactical oracles: TypeSafe Jev, worker
crates/bc-client-core  client state machine shared by the browser and bots (and the bay's walker)
crates/bc-econ         the economy: items, recipes, stores, suit builds, faults and overhauls, jobs, the Colony Exchange
crates/bc-server       WebTransport server, dev HTTP (/cert-hash, /status)
crates/bc-bot          Bot SDK, drone and miner agents, bc-swarm
crates/bc-client       Bevy browser client (wasm32)
crates/bc-model        the suits' procedural designs and their sockets
crates/bc-sound        the generated sound bank, mixer, cockpit sounds and music
crates/bc-alloc        counting allocator for the zero-allocation proofs
crates/bc-auth         wallet sign-in (SIWE): the message both sides build, and the server's check
docs/                  DESIGN.md · ARCHITECTURE.md · PROTOCOL.md · STORY.md (the world) · CONTROLS.md
                       LOCK.md · PEERS.md · ROADMAP.md · COLONY.md · COLONY_LOOK.md · SUITS_INSIDE.md · TRAINING.md
                       LIFE.md
web/, scripts/, e2e/   page shell, build and dev scripts, Playwright tests
```

`scripts/ci.sh` runs everything CI does: format, clippy (the hot-path bans are errors), all tests,
wasm determinism, and a benchmark smoke run. `BC_E2E=1 scripts/ci.sh` adds the browser tests.
`scripts/e2e.sh SUITE [webgl2|webgpu]` runs one of them against a fresh server (the suites are
listed at its top): `BC_HTTP_PORT` moves the server off 8080 (the tests find it through `BC_URL`,
which the script sets) and `BC_WT_PORT` its WebTransport off 4433, and `BC_GFX_QUALITY` (the `gfx`
suite; default high) and `BC_QUALITY` (`slice`) pick the graphics tier. `scripts/build-web.sh` builds
with the cargo profile `BC_WEB_PROFILE` (default `wasm-release`). GitHub Actions runs the browser
tests nightly, each suite on its own runner (`.github/workflows/e2e.yml`; also by hand).

## Legal

Before Colony is an original game: its world, its suits and its names are its own
(`docs/STORY.md`, "Voice and naming"). All art is procedural. Code is MIT-licensed (see `LICENSE`).
