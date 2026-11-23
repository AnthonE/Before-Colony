# Before Colony · NOW.md — what next

The backlog, by lane: what a builder can pick up, what the house is building this round, and what
waits on the owner. An item says what's left and where to start, not how it got here; the story is
in git and the area docs. Written 2026-11-23 from `PHASES.md`'s 0.2 and 0.3 and the area docs'
open items, each checked against its status table that day.

- **Labels are stable.** Code, commits and pull requests cite `NOW.md §<label>`. A label never
  moves to other work; a closed one is listed under §Labels.
- **Done work is deleted, not ticked.** A pull request names the label it closes on its first
  line; the house deletes the item when it merges (`CLAUDE.md`: outside work doesn't edit `docs/`).
- **The area doc wins on conflict.** An item points into its area's doc and doesn't restate it.
  Where they disagree, the area doc is right and this file is stale: say so in the PR.
- **A new label** appears only where an area doc has none of its own. The area docs' labels are
  reused as they are: `fr-` for `FREE_ROAM.md`'s phases, `ring-` for `RING.md`'s, `oc-` for
  `OPEN_CITY.md`'s, `peers-` for `PEERS.md`'s numbered list.

Each item has a machine check and a look check (`PHASES.md`, "How we build it"); it's done when
both pass. **Wire: yes** means the change moves the protocol: `PROTOCOL_VERSION` in
`crates/bc-proto/src/lib.rs` goes up in the same PR, and new `Request` and `Update` variants
(`crates/bc-econ/src/wire.rs`) go at the end of their enums. `scripts/quick.sh` is the floor for
every item; `CONTRIBUTING.md` has the check for each kind of change.

---

# In flight (house) — not offered this round

The owner's answers of 2026-11-23 (`DECISIONS.md`) are being built now by the house's own sessions,
each on its own branch. A pull request that does one of these is closed as a duplicate; build
beside it, not on it. Each is deleted from here when it lands, and what it leaves becomes an item
below.

## pvp-range · Suits fight pilots only away from the colony *(sim + server lane · house)*

Pilot against pilot in suits outside a range round the ring (5 km by default: `DECISIONS.md`,
§open); inside it, weapons safe between pilots, and the colony itself never takes fire.
`PEERS.md` 10; `PHASES.md` 0.3.

## foot-pvp · On foot, open season anywhere *(town lane · house)*

Pilots on foot can hurt each other wherever they meet, and the Watch answers it as a crime where
it holds (`OPEN_CITY.md`, "Heat and the Watch").

## voice · Proximity voice *(client + server lane · house)*

On foot, and over a suit's radio (`DECISIONS.md`, §open, for the ranges). Gates' voice crate is
the model (`PHASES.md`, "What we take").

## rep · Player reports *(client lane · house)*

A key that writes what happened, a screenshot and a fingerprint, the player's words fenced as
untrusted (`PHASES.md`, "What we take": Gates' report). Until it lands, a report is a GitHub issue
(`.github/ISSUE_TEMPLATE/bug_report.md`).

## snd-foot · Sound on foot: the town and the bay *(sound lane · house)*

The town's streets, traffic and crowd, the bay, a car, a lift (`PHASES.md`, the Sound track's
0.2 row; `crates/bc-sound`).

## elo · The Elo Pros contract *(server lane · house)*

What a listed game owes the store: its status file, the server's key, its stats
(`DECISIONS.md`, 2026-11-23: the business is Elo Pros').

## livery · Liveries and cosmetics *(econ + client lane · house)*

`PEERS.md` 7: colours chosen at the suit's console, carried on the suit, drawn for everyone.

## wt-pin · The wtransport pin and the rewind's floor *(server lane · house)*

Check our wtransport against the remote panic oneshot's pin avoids, and the shot's rewind depth
from the link the server measures (`PHASES.md`, "What we take": the rewind and the pin).

## e2e-red · The red nightly suites *(e2e lane · house)*

The nightly browser suites (`.github/workflows/e2e.yml`) have failed every night since 2026-11-19.
0.2's exit bar wants them green seven nights running.

---

# Buildable now — 0.2 · The First Hour

`PHASES.md`, 0.2: a new pilot's first hour as good as anything in the genre. These come first.

## fr-v1 · The floating origin for the ring and the town *(client lane)*

- **Read first:** `FREE_ROAM.md` §6 (V); `SCALE.md`'s E.
- **Left:** V1's one open part. The cylinder's city has a floating origin (`RenderOrigin`, recentred
  every kilometre, in `crates/bc-client/src/city.rs`); the ring and the town, drawn 3 km and more
  from the colony's middle, have none yet.
- **Machine check:** both wasm clippies (`scripts/quick.sh`); `scripts/e2e.sh town webgl2` and
  `scripts/e2e.sh perf webgl2` still pass.
- **Look check:** `scripts/pt showcase town` and a walk and a drive far round the ring
  (`scripts/pt`), with no shimmer or jitter in the far blocks, before and after.
- **Wire:** no.

## ring-r4 · The base's buildings, one at a time, the briefing room first *(ring lane)*

- **Read first:** `RING.md`, its status table and "R4 on" in its build order.
- **Left:** every building after Bay Row and the test hall: in order, the briefing room, the
  armoury, the mess, the Proving Ground, the militia's hangar. One building a PR.
- **Start:** `crates/bc-sim/src/content/base.rs` (the prefab, from Gates' pieces:
  `crates/bc-sim/src/pieces`), `crates/bc-client-core/src/muster.rs` and `piece_mesh.rs` (the station
  language), `crates/bc-client/src/base.rs` (drawn in the drum).
- **Machine check:** `scripts/quick.sh bc-sim` (`PIECES_GOLDEN` moves only with the layout, named
  in the commit); a step in `scripts/e2e.sh base webgl2` walking into the building.
- **Look check:** a camera in `?showcase=base` on it, and screenshots in the PR: R4's recipe
  wants them before it merges.
- **Wire:** yes if the building puts a desk on the deck (its terminal's requests), else no.

## peers-p0 · The objective's door marked in the view, on foot *(client lane)*

- **Read first:** `PEERS.md`, "What we take", P0 item 3 (its last line); `UI.md`'s rules.
- **Left:** flying, the objective has a waypoint in the view (◆, held at the edge: `map.rs`); on
  foot only the map marks the door (`bc_client_core::objectives::waypoint_at`). Mark it in the view
  on foot too, in the town and on Bay Row, by the same rules.
- **Machine check:** `scripts/quick.sh bc-client-core`; `scripts/e2e.sh town webgl2`.
- **Look check:** `scripts/pt` on the street with each of the street's objectives (TAKE A CAR,
  CARRY A FARE, GO UP TO YOUR SUIT): `scripts/pt shot` with the marker on its place.
- **Wire:** no.

## perf-ref · 60 fps at High on the reference machine *(client lane)*

- **Read first:** `PHASES.md`, "What AAA means" 2; `DECISIONS.md`, §open (the reference machine:
  Iris Xe graphics in Chrome until the owner names one); `FREE_ROAM.md` §6.
- **Left:** 60 fps at High on the town's crowded camera (`?showcase=town&cam=9&quality=high`) and
  on the launch, on that machine or its like.
- **Machine check:** **numbers from real hardware**: the GPU, the browser and its version, the
  build (WebGL2 or WebGPU), and `?perf=1`'s frame time and counts before and after, in the PR body.
  `scripts/e2e.sh perf webgl2` is a smoke test only: it runs on SwiftShader, which is neither the
  look nor the speed (`CLAUDE.md`'s traps), so its numbers prove nothing about the machine.
- **Look check:** the same camera's screenshot before and after: faster, not plainer.
- **Wire:** no.

## wasm-size · The browser build's size, and a cold start under 30 s *(client lane)*

- **Read first:** `README.md`, "Known limitations"; `PHASES.md` 0.2's exit bar (cold start to the
  street in under 30 s).
- **Left:** trim Bevy's features and run `wasm-opt` (`BC_WEB_OPT=1` in `scripts/build-web.sh`).
  Measure first: the README's figure is from an earlier build.
- **Machine check:** `scripts/build-web.sh webgl2` and `scripts/build-web.sh webgpu` sizes, raw and
  brotli, before and after in the PR; `scripts/e2e.sh gfx webgl2` still clean.
- **Look check:** a cold start timed on a named machine and link, title to the street.
- **Wire:** no.

## webgpu · The WebGPU build on real hardware *(client lane)*

- **Read first:** `README.md`, "Known limitations"; `CLAUDE.md`, rule 6 (derivatives at the top
  of `fragment()`).
- **Left:** the WebGPU build compiles and has never been seen working: the sandbox's software
  WebGPU loses its device. Run it on a real GPU, fix what breaks, name the device.
- **Machine check:** `scripts/quick.sh` (its WebGPU clippy); `scripts/build-web.sh webgpu`.
- **Look check:** the showcase's scenes on WebGPU beside WebGL2, the GPU and browser named.
- **Wire:** no.

## browsers · Firefox and Safari *(client lane)*

- **Read first:** `README.md`, "Quick start" (its last paragraph).
- **Left:** only Chromium has been tested. Firefox and Safari 26.4+ ship WebTransport; the dev
  server's self-signed certificate rides `serverCertificateHashes`, which may not work there.
- **Machine check:** what's fixed passes `scripts/quick.sh`; a Playwright project for the browser
  if it can run headless.
- **Look check:** title to the street and a launch, on a named browser and OS.
- **Wire:** no.

## ui-next · What `UI.md`'s "Next" lists *(page lane)*

- **Read first:** `UI.md`, its rules and "Next".
- **Left:** the HUD's scale and opacity as settings (`crates/bc-client-core/src/settings.rs`'s
  `KNOBS`); the Orrery's panel as instruments; the objective's marker in the view kept clear of the
  focus tags. One a PR.
- **Machine check:** `scripts/build-web.sh page`, `scripts/e2e.sh gallery`, and
  `scripts/quick.sh bc-client-core` (`page_contract.rs` reads `web/ui.js` against the page's types).
- **Look check:** the gallery's shots of the screens it touched, before and after.
- **Wire:** no.

## launch-clock · The launch shot's lurch *(client lane)*

- **Read first:** `ROADMAP.md`, "The loop's loose ends", its "Next".
- **Left:** on the throw out of the bay door the ring is drawn on the view clock and the own suit
  on the input clock, which can show as a lurch of a door's width if the release lands after the
  cut (`crates/bc-client/src/launch_shot.rs`). See it first; then draw the colony on the own clock
  during the shot.
- **Machine check:** both wasm clippies; `scripts/e2e.sh hangar webgl2`.
- **Look check:** the launch filmed before and after (`scripts/pt`).
- **Wire:** no.

## sec-token · A resume token a `Debug` would print *(proto lane)*

- **Read first:** `SECURITY.md`, promise 4; `crates/bc-proto/src/auth.rs` (a signature's `Debug`
  hides its bytes).
- **Left:** `bc_proto::ControlMsg` derives `Debug`, so a `{:?}` of a `Hello` or a `Token` would
  print a resume token. Nothing logs one today; give the token the signature's treatment so
  nothing can. Land it after this round's wire lanes (protocol 49): `crates/bc-proto` is busy.
- **Machine check:** a case in `crates/bc-proto/tests/secrets.rs`; `scripts/quick.sh bc-proto`,
  its encodings unmoved.
- **Look check:** none: it prints nothing new.
- **Wire:** no.

---

# Buildable now — 0.3 · Reasons to Stay

`PHASES.md`, 0.3: always a next thing, and people to do it with. These can start now; 0.2's
items come first in review.

## peers-6 · Contracts beyond supply and patrols *(econ lane)*

- **Read first:** `PEERS.md` 6; `DESIGN.md`'s roadmap, "Contracts"; `crates/bc-econ/src/charter.rs`.
- **Left:** haul this, clear that claim, escort a hauler home, recover a wreck; some shady. One
  kind a PR, the reward held as the Exchange holds its orders.
- **Machine check:** `scripts/quick.sh bc-econ bc-server`; a test that a contract is posted, taken,
  done and paid at the desk.
- **Look check:** its terminal tab in the gallery, and one flown with `scripts/pt`.
- **Wire:** yes.

## peers-8 · Drone offensives and a carrier *(sim lane)*

- **Read first:** `PEERS.md` 8; `crates/bc-sim/src/content/salvage.rs` (the bounties already pay).
- **Left:** squads that scale with the pilots out, an offensive now and then against the dock or
  Fornax, and a carrier that launches drones.
- **Machine check:** `scripts/quick.sh bc-sim bc-sector` (the `no_alloc` proofs at 0, the
  determinism golden moved only with the rule, named in the commit).
- **Look check:** `scripts/pt` out at the dock during one.
- **Wire:** yes if the carrier is a new kind of entity.

## peers-9 · Agents as population *(bot lane)*

- **Read first:** `PEERS.md` 9; `crates/bc-bot/examples/flaneur.rs`; `DESIGN.md`'s roadmap, "Traffic".
- **Left:** flaneurs that walk the town and go into its places (today's walks back and forth), and
  the server's own tugs, haulers and miners in the lanes.
- **Machine check:** `scripts/quick.sh bc-bot`; a test of the agent reaching where it set out for.
- **Look check:** `scripts/pt` where they go.
- **Wire:** no: agents speak the same protocol as people.

## peers-28 · Crews *(sim + server lane)*

- **Read first:** `PEERS.md` 28; `ROADMAP.md`'s status (crews: open); `PHASES.md` 0.3.
- **Left:** gunner seats on a carrier or a transport under a captain's fire policy; a crew's hangar
  and shared stores. Thin and joined end to end first: two pilots, one suit.
- **Machine check:** `scripts/quick.sh bc-sim bc-sector bc-server`, the `no_alloc` proofs at 0.
- **Look check:** two browsers in one suit (`scripts/pt`).
- **Wire:** yes.

## oc-c1 · Cars you own *(town lane)*

- **Read first:** `OPEN_CITY.md`, its status table (C1) and the design it points at;
  `FREE_ROAM.md`.
- **Left:** any car can be taken in the ring's town today; none is yours. A car bought, parked where
  you left it, and locked against others.
- **Machine check:** `scripts/quick.sh bc-econ bc-server`; `scripts/e2e.sh town webgl2`.
- **Look check:** `scripts/pt` buying one and finding it again.
- **Wire:** yes.

## oc-b1 · Businesses *(town lane)*

- **Read first:** `OPEN_CITY.md`, B1.
- **Left:** shopfronts leased like flats, on the ring's buildings.
- **Machine check:** `scripts/quick.sh bc-econ bc-server`.
- **Look check:** `scripts/pt` at the shopfront.
- **Wire:** yes.

## fr-w1 · The Watch's patrols *(town lane)*

- **Read first:** `FREE_ROAM.md` §4 (W) and its phases (W1: done but patrols); `OPEN_CITY.md` X1.
- **Left:** units that walk and drive a beat when nothing's called in. Start after foot-pvp lands:
  it changes what the Watch answers.
- **Machine check:** `scripts/quick.sh bc-sim bc-server` (`STREET_GOLDEN` and `TOWN_GOLDEN` move only
  with the rule); `scripts/e2e.sh town webgl2`.
- **Look check:** `scripts/pt showcase town` with a patrol in shot.
- **Wire:** yes if a patrol rides the wire as more than a unit.

## aquila-anim · Altair's change into Aquila, animated *(model lane)*

- **Read first:** `README.md`, "Known limitations"; `PHASES.md`, the Look track's 0.3 row (suits
  animated, not swapped).
- **Left:** the model swaps with a flash. Animate the fold from `crates/bc-model`'s two designs.
- **Machine check:** `scripts/quick.sh bc-model`; the gfx suite's scenes still clean.
- **Look check:** `?showcase=lodestars` filmed through the change.
- **Wire:** no.

---

# Not yet — 0.4 on, pointers only

Not offered: they wait on 0.2 and 0.3, or on the owner. Ask in a draft PR before starting one.

## ring-r15 · The cylinder's inside cleared *(ring lane)*

`RING.md`, R1.5 and R1.6: its generic parts split out, homes refunded, the rest deleted. Four
browser suites walk it (`hangar`, `colony`, `inside`, `home`); when it goes is the owner's call.

## fr-d1 · The town split into district sectors *(server lane)*

`FREE_ROAM.md`, D1, and `SCALE.md`'s D: when one sector isn't enough.

## store-db · Pilots and stores in a database *(server lane)*

`PHASES.md` 0.4: a database, backups and restores, behind the `PilotStore` trait the file store
(`pilots::FileStore`) implements today.

---

# OP · the owner's lane — no builder can pick these

- **The board's price** and posting the `before-colony-pr` lane on elopros.com (`DECISIONS.md`,
  §open; `CONTRIBUTING.md`), with the store's listing spelling the repo as GitHub does,
  AnthonE/Before-Colony: a delivery's link is checked against it letter for letter.
- **The security bounty:** its award per severity, and turning on GitHub's private vulnerability
  reporting for the repo (`SECURITY.md`).
- **The repo's settings:** Actions approval for first-time contributors, and the CI checks required
  on `main`.
- **The knobs still open** (`DECISIONS.md`): the reference machine, the PvP range, what a kill
  outside it owes, the voice's ranges.
- **Sign-offs on the look and the feel**, on a real GPU, once a phase at least (`PHASES.md`).
- **Publishing the page and deploying** (`CLAUDE.md`, operator-only).
- **Merging**, which on the board is the act that pays (`AGENTS.md`, "The deal").

---

# Labels

Every label above is used once (`scripts/refs.sh` checks it). Closed labels, with the commit that
closed them, are listed here: none yet.
