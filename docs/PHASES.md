# Phases: from the slice to a game people pay for

> The owner's ask (November 2026): "let's really lay out some sort of phases of how we can build
> this game ... AI really has a chance at making a really AAA quality game. And we have a good,
> decent setup when it comes to the other repos around": Gates and oneshot, to take from.

This is the plan of record for the whole game: the order the big pieces come in, the bar each phase
has to clear before the next starts, and what we take from the sibling games instead of building
it again. The area plans stay the plans of their areas and are cited, not repeated: `FREE_ROAM.md`
(the town), `RING.md` (the ring and the base), `OPEN_CITY.md` (homes, the street, the Watch),
`LIFE.md` (jobs and meals), `TRAINING.md`, `SCALE.md` (what stops the world growing), and
`ROADMAP.md` with `PEERS.md` (the ranked list of what the nearest games teach). Where this document
orders things differently from one of them, this one wins.

## What "AAA" means for us

Not a 300-person studio's hours of hand-made content: we can't out-make them, and we don't have to.
It means five things a player can feel, each of which is a bar we can check:

1. **It feels finished.** Every place has something to do, every verb answers at once, every
   transition is staged, nothing says "TODO", nothing a player meets is a placeholder.
2. **It runs well.** 60 fps at High on the reference machine (`DECISIONS.md`'s default till the
   owner names one: a 2021 laptop on Iris Xe graphics, in Chrome), a cold start to playing in under
   30 s, no hitches when the town streams in.
3. **It stays up and stays fair.** Hundreds of pilots in one world, nobody loses what they earned,
   nobody wins by cheating.
4. **It lasts.** Reasons to come back for months: things to build, people to build them with, a
   world that moves (`STORY.md`'s eras).
5. **It looks and sounds like one thing.** One art direction (the station language, painted like the
   anime: `RING.md`, "The look"), animation that sells weight, effects that read, a mix that tells
   you what's happening.

What makes it possible with agents, and is already in the repo:

- **No asset bottleneck.** Every suit, building, person and car is procedural (`bc-model`,
  `town_mesh`, `life_mesh`); a new frame is a row, a model and a kit.
- **A cheap world.** The colony, the town, its traffic and its crowd are closed forms of the tick
  (`CLAUDE.md`, rule 5): nothing to store, stream or keep in step until something's done to it
  (`FREE_ROAM.md`, P2).
- **Agents as population.** Pilots, traffic and the Watch's units play by the same rules over the
  same wire (`bc-bot`).
- **Checks a machine can run.** Goldens and `no_alloc` proofs on every push; the browser suites
  nightly; `scripts/pt` and the playtester agent for how it plays; the fx stage, its strips and the
  reference board for how an effect looks.

The pace so far, for sizing: the game began on 24 September 2026; everything above was built in two
months (478 commits, 76 pull requests). The sizes below are in that currency. Gates, built the same
way from 30 July, reached its browser build in seven weeks and released about once a week after
that (0.9 and 0.10 in October; 61 merges in four weeks, by its own count).

## Where we are (November 2026)

| Pillar | Built | Missing for AAA |
|---|---|---|
| Flight and combat | Newtonian 6DOF, two flight rules, free aim, lock-on, beams, missiles, melee, stagger, doom and ejecting, specials, aces, a debrief (`DESIGN.md`; `ROADMAP.md` 15–27) | Crews (28); suits drawn with animation, not mesh swaps; a second faction's prototypes |
| Survival and the economy | The bay on foot, fabricating, the Exchange, wear, the Charter Board, contracts begun (`DESIGN.md`, Milestones 3–4) | Contracts beyond supply and patrols; liveries; sinks and faucets measured |
| The ring and its town | The ring at 3 km, the town's plan, the painted look, traffic, a crowd that reacts, the Watch, fares, a start on foot (`FREE_ROAM.md`) | The floating origin; the base's buildings (`RING.md` R4); homes on the ring; the Watch's patrols; the crowd in the tick |
| Space | One sector at L1: the field, rocks, landmarks, salvage, mining, the Almagest's drones | Traffic in the lanes; drone offensives; more sectors (`SCALE.md` D) |
| People together | The colony's radio | Crews and guilds, friends, a crew's hangar, voice; where PvP lives in space (`PEERS.md` 10) |
| Accounts and persistence | Guests, wallets (SIWE, Solana), Elo Pros passes; pilots and stores in files (`pilots::FileStore`) | A database; backups; account recovery; a never-lose-progress guarantee |
| Platform | The browser: WebGL2 and WebGPU builds | A desktop build, Steam |
| Scale | One sector holds about 300 pilots in one fight (`SCALE.md`) | Sessions without flight slots, many sectors, a gateway, handoff |
| Presentation | Procedural suits and town, the anime look, the fx stage; a sound bank synthesised at boot for the cockpit and the fight, and music that follows it (`bc-sound`) | Animation; an art-direction lock with a reference board per place; any sound at all on foot (the town and the bay are silent); voice |
| Trust | Server authority, limits under abuse (`ARCHITECTURE.md`, "Under abuse") | A threat model, cheat detection, a bounty for breaking it |
| Running it | One process, published by the operator; logs; `/status` | Metrics and dashboards, alerts, deploys without downtime, player reports, moderation |

## The phases

Each phase ends in a **release**: a version, a name, notes posted, and a build the operator puts in
front of people. A phase isn't done until its exit bar is met. Everything not in the current
phase waits; the area plans keep their own order inside a phase.

### 0.2 · The First Hour

**A new pilot's first hour is as good as anything in the genre.** They wake on the street, take a
car, carry a fare, ride the spoke up to their bay, put a rifle on the Argo, launch out of their own
door, kill their first drone, come home and get paid, and every step of it is staged, readable and
quick.

- **Built from:** `FREE_ROAM.md`'s V1 rest (the floating origin) and O1 (the street's objectives);
  `RING.md` R4's first buildings on the Muster's deck; `PEERS.md`'s still-open P0 item (a marker on
  the door in the view itself); the hints and the boot that says when it's stuck; `UI.md`'s rules on
  every screen the first hour shows; sound for everything on foot (the town and the bay are silent
  now); a report key for playtesters (Gates', below).
- **Exit bar:**
  - five people who've never played, and a playtester agent per step, get from waking to home and
    paid in under an hour with no help; every place one of them got stuck is fixed and replayed;
  - 60 fps at High on the reference machine, on the town's crowded camera (`perf`) and the launch;
    cold start to the street in under 30 s;
  - the gallery's every screen and the showcase's every camera on the first hour's path looked at
    and signed off by the owner (as `RING.md`'s L1 was);
  - the nightly suites green seven nights running.
- **Size:** a few weeks.

### 0.3 · Reasons to Stay

**There's always a next thing, and people to do it with.** `PEERS.md`'s lesson: "reasons to cross
the chain again and again, and people to cross it with".

- **Built from:** contracts (`PEERS.md` 6: haul, clear a claim, escort a hauler, recover a wreck;
  some shady); liveries (7); drone offensives and a carrier (8); crews (28), with a crew's hangar
  and shared stores; cars you own and homes on the ring (`OPEN_CITY.md` C1, H0–H3 moved onto the
  ring's buildings); businesses (B1); the Watch's patrols and heat that means something (`OPEN_CITY.md`
  X1–X2); pilots fighting pilots, as the owner decided (`DECISIONS.md`, 2026-11-23): in suits only
  outside a range round the colony, which itself never takes fire, and on foot anywhere, the Watch
  answering it; proximity voice, on foot and over a suit's radio; agents as population (9):
  flaneurs, and the server's tugs, haulers and miners in the lanes.
- **Exit bar:**
  - a week of play with something new each day and no repeat that feels like one (playtesters and
    the owner);
  - the economy's faucets and sinks counted (`/status` and a dashboard) and within a band the owner
    sets;
  - a crew of four can do everything together that one pilot can, and more;
  - people who played 0.2 come back for 0.3: retention measured, not guessed.
- **Size:** about as much again as everything built so far.

### 0.4 · Hundreds

**The world holds a crowd, keeps what everyone earns and can't be cheated.**

- **Built from:** `SCALE.md`'s B (sessions without a flight slot), C (the crowd as a sector with
  interest tiers: `FREE_ROAM.md`'s P2 into the tick, `OPEN_CITY.md`'s S1), D (many sectors, a
  gateway, handoff; the town split by district, `FREE_ROAM.md` D1) and E (a client that streams);
  pilots, stores and the world in a database, with backups and restores; a threat model and what
  it asks for; metrics, alerts and deploys without dropping anyone.
- **Exit bar:**
  - 500 pilots (people and `bc-bot` agents) in one world for an hour: the tick's p99 under budget,
    `hot_path_allocations` 0, every client's bandwidth under its budget;
  - the server killed mid-fight and back with nothing lost that was acknowledged;
  - every threat in the model closed or knowingly accepted, and a standing bounty for breaking it;
  - a deploy with pilots on, none of them dropped.
- **Size:** months; it can start alongside 0.3, since it's mostly another lane's work.

### 0.5 · Early Access

**People can find it, install it, pay for it and tell us what's wrong.** The browser first
(`DECISIONS.md`, 2026-11-23): Early Access is the browser build on Elo Pros, the business Elo Pros'
(below); the desktop build and Steam follow it.

- **Built from:** a desktop build (the client native as well as wasm) on Steam beside the browser;
  the store page, the trailer (`scripts/trailer`), the notes; player reports from inside the game;
  moderation for the radio and names; account recovery; settings and accessibility (rebinding is
  `CONTROLS.md`'s; subtitles, colour-blind palettes, text size); the business the owner picks.
- **Steam and wallets don't mix.** Valve won't publish "applications built on blockchain technology
  that issue or allow exchange of cryptocurrencies or NFTs", and sales inside the game go through the
  Steam Wallet (Gates' `docs/STEAM.md`, quoting Valve, October 2026). Our wallet sign-in (`bc-auth`)
  and anything a pilot earns that could become a token are inside that line. Gates' answer, planned
  not built: a server-side economy switch, a build with the wallet code compiled out and a CI check
  for its strings, and Steam's own sign-in ticket checked by the server. Ours would be the same,
  decided before the Steam build starts (Open questions).
- **Exit bar:**
  - installs and runs on the reference machine and a low one from the store;
  - a report filed in the game reaches a person and an agent within the hour;
  - the first-hour bar of 0.2 still met with strangers from the store, not friends.
- **Size:** a few weeks of building, then as long as the store takes.

### 1.0 · The Charter

**The world moves because of what its players did.** `STORY.md`'s Era I: the Charter Board's great
works finished by deliveries, the charter vote, the colony's militia on contract, more rings begun.
The calendar starts.

- **Built from:** colony projects that change the world when done (a service, a price, a sector);
  the militia; the era's announcement to everyone; a content cadence (`PEERS.md`: "new suits are the
  cadence") of a frame or a place each release.
- **Exit bar:** the first era turned by players, not by hand; a release a month for three months
  running without a regression the nightly suites missed.

### After 1.0

`STORY.md`'s eras in order: II, the Cluster (more colonies, hauling between their exchanges); III,
the Spheres (L2 to L5, the Moon: `DESIGN.md`'s "Moons and planets"); IV, the Equinox Wars. Each era
is a phase of its own, planned when the one before it is under way.

## The business: Elo Pros'

The owner's call (`DECISIONS.md`, 2026-11-23): whatever Elo Pros needs; it's our home. The store is
`AnthonE/scry-forge` (elopros.com), and its frame of record is its `docs/GATES.md`. What follows is
what it asks of a game it lists, read from that repository and the live store in November 2026.

- **Where we stand.** Before Colony is a house title in alpha, on the store's coming-soon shelf. An
  alpha plays free on its own servers: no copy sold, no coin of its own, its progress wiped once, at
  listing (`GATES.md`, the alpha rule). The live server was still on protocol 44 when main was on 48.
- **What the store reads from us, built:** sign-in (a wallet's text on chain 4663 and its Solana form,
  and the Elo Pros pass: `bc-auth`), `/status.json` (who's on), `/online.json` (whose playtime: it
  drives card drops, the Pill's ten minutes and free trials), `/floor.json`, the Pill decal on the
  shoulder for a wallet that holds it.
- **What it makes money from:** the copy at listing (a ticket on Robinhood Chain); cosmetic items
  (for us, Solana items, one collection for the title, bought on the store or earned in play, and
  unlocked in the game for the wallet that holds them); hosting; and the platform's own coin. Players
  are paid from play through its cashier: for us, reviewed prizes from a season's boards, since our
  clock never resets.
- **The rule that binds us:** the store sells appearance only. Anything with stats enters the game
  through play, never through a purchase (`GATES.md`, no pay to win). Liveries, palettes and decals
  are ours to sell; frames, parts and credits are not.
- **What we build, by phase:** 0.2, `/status.json` counting agents as the store does, a coarse status
  line in `/online.json`, a server key and the signed calls, `elo.json` with our declared stats, and
  stats and boards reported (never pilot kills: alts would farm them); 0.3, liveries painted for
  credits and premium palettes and decals unlocked by owned items, the shop declared, cosmetics earned
  in play granted to the wallet; 0.5, at listing, the copy door (built disarmed first), bans and
  reports through the store, the launch wipe, and season prizes.
- **The operator's alone:** generating and registering the server key; letting our `elo.json` land
  (`repo_commits` on the listing, or a signed manifest); creating the title's Solana item collection;
  prices; the ticket at listing; deploying the server and the page; funding the board.
- **Steam, later:** a Steam build leaves the wallet and the store's items out (Valve's rule, in 0.5
  above).

## The tracks: how good, in every phase

The phases say what the game has. These say how good it is, and their bar rises with each release.
Every phase's exit includes its row of each.

| Track | 0.2 | 0.3 | 0.4 | 0.5 and on |
|---|---|---|---|---|
| **Look:** one art direction, animation, effects, light | The first hour's places on the reference board, signed off; the painted look everywhere it shows | Figures and suits animated, not swapped (`life_mesh`'s bank, `bc-model`); every effect judged on the fx stage | Streaming without pops; the far tiers indistinguishable at speed | A look per era; marketing shots straight from the showcase |
| **Sound:** the mix, music, voice | Every action in the first hour has its sound: the town's streets, traffic and crowd, the bay, a car, a lift | A positional mix for crowds and traffic; music for the town and the bay as `bc-sound`'s follows the fight | Holds up with hundreds near | Voice for the Board, the Watch and the Orrery, if the owner wants it |
| **Feel:** frame time, input, load | 60 fps High on the reference machine; cold start under 30 s | No frame over 33 ms on the crowded camera | Same, with 500 in the world | The low machine at Low |
| **Trust:** fairness, uptime, data | Nothing the client says is believed that can't be checked, but what's listed (the rewind's depth: below) | Exploits found by agents fixed before release | The threat model, the bounty, backups | Uptime measured and published |
| **Agents:** population and builders | Playtester agents walk the first hour every night; outside agents building on the paid board (`AGENTS.md`) | Agents fill the lanes and the streets when people are few | 500-pilot load tests from `bc-bot` | The board's lanes cover every track |

## How we build it

- **One lane per area, one doc per lane.** An agent picks up an item from the current phase in its
  area's doc, builds it thin and joined end to end (`PEERS.md`: "connect before deepening"), and
  leaves the doc's status true.
- **Every item has a machine check and a look check.** The machine check is a test, a golden or a
  suite; the look check is `scripts/pt`, a playtester agent's report, an fx strip or the board. An
  item is done when both pass.
- **The owner signs off looks and feel**, once a phase at least, on a real GPU: SwiftShader lies
  about the look (`CLAUDE.md`'s traps).
- **Releases are named and noted.** A version in `Cargo.toml`, a name, and notes a player would read.
  The version is typed once and gates the protocol and the oldest client a server takes, as Gates'
  does; a release script that's a dry run till told to go, and a check that names what a change
  takes to ship (client, server, wire), are Gates' `ci/release.sh` and `ci/deploy_kind.py`.
- **A phase's state is derived, never stamped** (Gates' `DESIGN.md`, "Milestones"): its exit bar is
  checked by running things, and the status tables say what was run.
- **The owner's decisions are kept in one place.** Gates' `docs/DECISIONS.md` has two tables: what
  the operator said (the date, their words, where it lands) and the knobs still open, with the
  default that ships until they speak. The area docs' "owner's decisions" sections would move there.

## What we take from Gates and oneshot

Gates (`AnthonE/Gates`) is a survival game in the same family, shipped and at 0.10, on the desktop and
in the browser; oneshot (`AnthonE/oneshot`) is a ranked shooter built from Gates'
parts, and its `docs/RIP.md` says what it took, file by file. We already take Gates' `fx` crate
(`scripts/fx-sync.sh`) and its building pieces (`bc_sim::pieces`). How each lifts: **copy** the
files and trim them, **port** them (rewrite here, to our rules), or take **the idea** only. Files
are copied, never depended on, as oneshot does: their crates are tied to their own simulations.

From oneshot:

| What | From | How | For |
|---|---|---|---|
| A RIP doc: every lift with its source, lines, destination, how, and what was left behind | oneshot `docs/RIP.md` | idea | now: this table grows into it |
| The rewind's depth chosen by the server from the link it measures, capped (160 ms there); ours believes the client's claim up to 8 ticks, 267 ms (`sim/combat.rs`, `MAX_REWIND_TICKS`) | oneshot `room/src/lib.rs` (`favour_for`), Gates `stats.rs` | port | 0.4 |
| Cheat flags, decoys and an aim model trained on players; a `SECURITY.md` of promises and a bounty by severity; anticheat staged by how many play | oneshot `room/src/watch.rs`, `decoy.rs`, `tools/aim`, `SECURITY.md`, `docs/ANTICHEAT.md` | idea, then port: our lock-on changes what an aimbot is | 0.4 |
| A leak gate (a test that no client is sent what it mustn't know) and an impaired-network gate | oneshot `room/tests/leaks.rs`, `client-net/tests/loopback.rs` | idea | 0.4 |
| The `wtransport` pin both siblings use (crates.io 0.7.1 has a remote panic; we're on 0.7.2, to be checked against it) | oneshot `docs/RIP.md` §1 | check | 0.2 |
| Ranked queues, playlists that open by who's online, cups, parties; ratings and signed results through Elo Pros | oneshot `server/src/queue.rs`, `playlist.rs`, `party.rs`, `eloqueue.rs`, `elo.rs` | port, for the Proving Ground's board and arena fights | 0.3 |
| Content as data compiled to checked-in Rust with a `--check`, and a design template a place is written in | oneshot `tools/maps/compile.mjs`, `docs/maps/cloudbreak.md` | idea | 0.3: the base's buildings, places |
| A load tool of bot clients | oneshot `tools/load` | port beside `bc-bot` | 0.4 |
| Reports that audit how it feels against the game it's measured by, with ranked causes | oneshot `reports/`, `research_notes/` | idea | every phase's look check |
| Paid contributions: a price for a merged PR, more for content that ships, paid to a `Wallet:` line | oneshot `CONTRIBUTING.md`, `AGENTS.md`; Gates' board | idea, the owner's call | 0.5 on |

From Gates (on Bevy 0.18: anything Bevy-side needs a port):

| What | From | How | For |
|---|---|---|---|
| The desktop build: the transport behind one `Wire` trait (native and web), release builds for Linux, Windows and macOS on a tag, packaged for the Elo Pros launcher; Discord and deep links. (Its Windows and macOS builds have never been run on those machines.) oneshot's port of it is the nearer model: one client crate as a `cdylib` and a `[[bin]]` (`crates/client/DESKTOP.md`, `client-net/src/native.rs`, `.github/workflows/desktop.yml`) | `crates/client/src/net/mod.rs`, `native.rs`, `web.rs`; `.github/workflows/release.yml`; `ci/depot.py` | port (both are on Bevy 0.18, we're on 0.19) | 0.5 |
| An asset pipeline without a person in the loop: headless Blender, rigging and retargeting animation, KTX2 textures, atlases, icons, licences in a manifest. Ours is procedural by rule (`DESIGN.md`), so this is for what procedure does badly: skinned animation for figures and suits | `ci/blender.py`, `ci/bl/`, `ci/rig_beast.py`, `ci/retarget_anim.py`, `ci/ktx_pack.py`, `assets/*/MANIFEST.md` | copy the scripts | 0.3 (animation) |
| A measured look bar: statistics of reference frames, scored in CI; the renderer's slices (sky and atmosphere, terrain, water, clouds, quality tiers, shader prewarm) | `docs/ART.md`, `ci/native_bar.py`, `docs/RENDER.md`, `crates/client/src/render/` | idea; port the prewarm and quality tiers | 0.2 on |
| Proximity voice, Opus, outside Bevy | `crates/voice` | vendor | 0.3 (crews) |
| Sound in an AudioWorklet, outside Bevy | `crates/sound-worklet` | idea, beside `bc-sound` | 0.2 |
| Player reports: a key that writes what happened, a screenshot and a fingerprint, the player's words fenced as untrusted; reports ranked by fingerprint; bots that file their own; crash reports (oneshot's `client/src/crash.rs`) | `crates/client/src/report.rs`, `ci/reports.py`, `crates/server/src/bugs.rs` | port | 0.2 (playtesters), 0.5 (everyone) |
| Running it: systemd units, certificates, a firewall rule; releases that warn the world before a restart; admin commands for allowlisted wallets; an anomaly log with a verdict script; trailers from replays | `ops/`, `ci/release.sh`, `crates/server/src/admin.rs`, `anomaly.rs`, `ci/anomaly_verdict.py`, `ci/film.sh` | copy | 0.4 |
| Cheat evidence (snap hits, hit rates, tracking through walls) logged, never acted on alone, and cheats bred to test it | `crates/server/src/cheat.rs`, `sim-core/src/genome.rs` | port | 0.4 |
| A cap table: each limit, what's used, what happens when it's full, what raising it takes | `docs/SCALE.md` | copy the shape into ours | 0.4 |
| Content as validated tables with a layout golden, and the rule to copy the reference game's numbers unless there's a stated reason not to | `content/*.toml`, `crates/content`, `docs/CONTENT.md` | port into `bc-econ` | 0.3 |
| Agents as players: their verbs a subset of a person's, personas, a population of resident bots | `docs/PLAYERS.md`, `crates/server/src/mind.rs`, `population.rs`, `ops/personas/` | idea, for `bc-bot` | 0.3 |
| Boards that remember (four standings and a hall), achievements in Steam's shape | `crates/server/src/standings.rs`, `achievements.rs` | port | 0.3, 0.5 |
| NPC minds and paths: a state machine, A* on a grid, all deterministic | `sim-core/src/brain.rs`, `nav.rs` | idea (ours run on roads and closed forms) | as needed |
| The process: a backlog by lane with stable labels and done work deleted; a mirror of what's built, re-derived by commands; the gate before strangers ("20 strangers, one wipe cycle, p99 tick under budget, zero silent failures") | `docs/NOW.md`, `HAVE.md`, `ALPHA.md` | idea | now |

Not taken: Gates' persistence (player and world files, like ours: no database to borrow), its wipes
(our clock never resets: `STORY.md`; its halls that remember across seasons carry over better), and
its terrain (an island from a seed: the idea's ours already).

The two games share a timeline (Gates' `docs/WORLD.md` §11, never said in either game: `STORY.md`'s
"Under everything"), and Gates' held items already borrow `bc-model`'s rules.

## The owner's answers

Asked in this document and answered on 2026-11-23 (`DECISIONS.md` has the words, and the knobs
still open with the defaults that ship meanwhile): the browser first; the business is Elo Pros'
(below); suits fight pilots away from the colony, and on foot it's open season anywhere; a paid
board, now; proximity voice. The reference machine is still open, and its default stands.
