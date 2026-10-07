# Roadmap: how PEERS.md's list gets built

`PEERS.md` says what the nearest games teach and what comes first. This is how each item gets built:
where it lands in the code, what goes on the wire, and how it's tested. It follows the house rule
from `PEERS.md`: **connect before deepening**, so every item sends pilots along the chain (the city,
the bay, space, home).

## Status

| Item | State |
|---|---|
| Lock-on: fighting on the ground in space (`LOCK.md`) | built |
| The weapons pass: the hit-rate harness, charged beams, true cones, lunges that home, the burst step | built |
| P0: a floor under loss, text chat, objectives along the chain, The Arrival's seats | built |
| P1: the Proving Ground: its course, the Blast Hall, its live fire, trainers boarded there, the drill and the board (`TRAINING.md`, phases 1 to 5) | built |
| Living in the colony, L0: seats, the body and its meals (`LIFE.md`) | built (a framework: nothing on the wire yet) |
| The loop's loose ends, first pass: the colony open by default, launching out of the pilot's own bay door, landing on the docking hub and walking in, hints along the way (below) | built |
| Propulsion: bigger tanks, the extended tank, propellant grades, the ion drive (`DESIGN.md`) | built |
| The mech games, 15: doom, ejecting and the self-destruct | built |
| The mech games, 16: stagger | built |
| The mech games, 17: specials charged by the fight | built |
| The mech games, 18: the debrief | built |
| The mech games, 19: the enemy's gun | built |
| The mech games, 20: a test range | built |
| The mech games, 21: aces, pay or salvage | built |
| The mech games, 22: staying up under abuse | built |
| P1's rest, P2 below, the loose ends left | planned |

## The loop's loose ends

Players walking the loop (the bay, a launch, space, home, the city) found its seams: the colony was
shut unless the server was told to open it, so the bay was a dead end; and a launch played the
bay's catapult, cut to black and opened on a suit somewhere off the hub with nothing tying the two
together. The first pass (built):

- **The colony open by default** (survival): `--no-colony` closes it (`BC_COLONY=0` for
  `dev.sh`); arcade rules have none, and the server says which at start-up. Closed, the airlock
  says the cap lifts are closed and the bay's terminal has no Proving Ground tab.
- **Out of your own bay door.** The suit rides its bay's catapult cradle in the door on the
  spinning bay ring (`Body::Bay(n)`, `colony::hub`; protocol 27) until the catapult's cut ends,
  and is thrown out with the ring's 125 m/s; the camera opens outside by the open door and watches
  it go (`bc-client`'s `launch_shot`). Into the colony (Q), it comes out of a lit port in the end
  cap's inner face behind the inner gate. The cockpit's prompt says which key goes where.
- **Home by landing on the hub.** Home stays on the axis, where nothing moves (a door on the ring
  would take 0.7 g of thrust to hold): the dock's ring of lights as before, or the grip armed, a
  landing on the docking hub's end face near its middle (a landmark now, turning with the colony),
  and a walk onto its deck hatch.
- **Hints along the loop:** the airlock and the cap lift, the city (its map, trams, cars and the
  lift back up), the hub's landing, and the inner gate.

Next, each a PR of its own:

- **Choose a strip at the airlock.** The lift always goes down to strip 0's Hub Gate
  (`onfoot.rs`, `EnterCity { strip: 0 }`), though the server and the agents take any of the three
  (COLONY.md 2.2's lift lobby, lifts 1 to 3).
- **The ◆ on the door in the city view itself**, not only on the map (P0's objectives, above).
- **The Arrival's door opens its menu** (`LIFE.md`, L1: eating), rather than a toast that the bar's
  quiet.
- **Second keys for F1 and F10** (no function row on Mac laptops and 60% keyboards: `CONTROLS.md`).
- **The Blast Hall's floor markings and the gantry's stairs** (`TRAINING.md`'s first ten minutes).
- **A nightly CI job for the browser suites** (`BC_E2E=1 scripts/ci.sh`): GitHub Actions runs none
  of them today, so the loop's seams are found by players.
- **The launch shot's clocks.** The ring is drawn on the view clock and the own suit on the input
  clock; on the throw the suit's drawn place blends between them (`own.rs`), which shows as a lurch
  of a door's width if the release lands after the cut. Draw the colony on the own clock during the
  shot if it shows.
- **Auto-nav on an ion drive's crawl.** The auto-nav plans its braking on the chemical thrusters
  (`nav::planned_braking`); a dry suit crawling home on its drive (0.1 g) brakes far later than it
  can, and overshoots. Plan on the drive's thrust when the tank is dry.

Later:

- **Landing at your own door**, with an approach assist that matches the ring's spin (or a
  tractor at the door), so a pilot can come home the way they left.
- **Flying out of the colony through the axis port** into space (`SUITS_INSIDE.md`'s handoff).
- **Solid end structures** (COLONY.md 1.3; a protocol bump of its own).
- **Trams and cars that people can't walk through** (COLONY.md phases 4 and 5's known gaps).

## P0: before more players arrive

**A floor under loss.** *Built.* A signed-in pilot whose only suit is gone, with less than a
torso's worth of credits, stores and parts (at the colony's values, `catalogue::value`), finds a
worn Leo in the gantry from the Charter Board, at most once every 30 minutes (`Hangar::reissue`,
with a `reissued_at`; `bc-server` `session.rs::enter` and the return from a loss). The news says
so. `STORY.md`'s "debt" is now "an advance". Tests: `bc-econ` (lost, broke, reissued; not again
within the half hour; not while there's a torso to build on or a torso's worth) and
`bc-server/tests/hangar.rs` (over a server restart, from the pilot's record). Known, and not made
worse: a new wallet is a new starter kit.

**Text chat.** *Built* (`bc-server` `radio.rs`, `bc-client` `chat.rs`). `Request::Say { text }` and `Update::Said { from, text }` on control-stream frames
tagged 11 (`bc_econ::wire`, `PROTOCOL.md`). One channel, the colony's radio, for everyone connected:
a ring in the server's shared state, read by each session on its 100 ms tick (off the hot path).
At most 5 lines in 10 s and 160 characters a line, control characters stripped, never logged (only
counted on `/status`). `/` opens it (Enter too, on foot, where Enter docks nothing); keys don't reach
the suit while typing. Agents: `bc-bot` `say` and `heard`. Tests: a two-bot server test; the page's
`ui` e2e types a line.

**Objectives along the chain.** *Built* (`bc_client_core::objectives`, appended so their bits
stay): ride the cap lift down, find the Exchange floor, sell on the Exchange (with the colony
open, the Welcome's COLONY flag). On foot the panel shows the first of them not done, with the
range to the place's door, and the city's map marks the door with a `◆`. Still open: a marker
on the door in the city view itself.

**The Arrival does something.** *Built.* Seats by its door (`bc_sim::colony::city::arrival_seats`, a
closed form of the door; benches drawn there), E to sit, others see you sit (the presence's
`ride` 15 is seated: no new bits, wire v16 for the meaning; the plaza takes it only on a seat), and
what's said on the radio over the heads of those near you. Flaneurs come and sit (`flaneur
--sit`, two of them under `BC_COLONY=1`). Tests: the seats' geometry, the plaza's rules for
sitting, and a step in the colony e2e.

## P1: what makes the chain worth walking

**The Proving Ground.** `TRAINING.md` is its design, in six phases. **Phase 1 is built:** the
course (`bc_sim::colony::course`). It's 13 rings in the colony's air, from the inner gate down over
the first window to the Charter strip's avenue, a slalom, a climb and a turn over the top, home
along the avenue and onto a pad on Hub Gate's square.
- The pilot's client times it on the predicted suit (`bc_sim::colony::course::Run`, stepped tick
  by tick): the clock runs from the start ring to standing on the pad.
- The best time is kept with the settings (`course_best_ms`), and since phase 4 on the server's
  board.
- `bc-client/src/course.rs` draws the rings (pilots on foot see them too), and the objectives'
  panel and waypoint show the course while flying inside.
- Tests: the rings against the city's walls; the run's rules; a Leo flying the whole course in the
  interior sector's simulation and landing on the pad.

**Phase 2 is built too:** the Blast Hall (`PlaceKind::Proving`), a key place off Hub Gate's
square with a room at a suit's scale (86 by 84 m, 60 m high) behind 40 m blast doors. People walk
in to its desk and suits fly in and land on its floor. `CITY_VERSION` 4 with protocol v21; the
objective `REPORT TO THE PROVING GROUND`.

**Phase 5, live fire in the hall, is built:** the owner made the hall the colony's law's one
exception (`bc_sim::colony::hall`). Suits in it fire training rounds that touch no suit, never
leave it and score on its twelve targets (the `TargetHit` event); outside it the tick clears the
weapons' buttons, as the pilot's prediction does. Tests: `hall`'s units, the interior's, a new
`HALL_GOLDEN`, `no_alloc`, and the `inside` e2e scoring on a target.

**Phase 3, boarding in the hall, is built:** on foot at the gantry's hatch, `Request::BoardTrainer`
seats the pilot in one of the Charter Board's Leos (`Control::Board`, `Sim::launch_at` with
`LaunchAt::Gantry`), standing on the gantry's pad. Their hangar is untouched. Docked back at rest
on the gantry, they climb out at its hatch.

**Phase 4, the board, is built, with the drill.**
- The interior sector times every pilot's course and drill in its tick
  (`Sector::watch_training`) with the same closed forms as their client, and reports the times
  (`Report::Course`, `Report::Drill`).
- The session puts them on the day's board (`bc_econ::proving`, `proving.json`) and a signed-in
  pilot's bests on their record.
- The board is sent to everyone in the colony (`Update::Proving`). It's drawn on the hall's back
  wall and opened at its desk.
- The drill is X-Wing's Maze: twenty targets lit in turn, the clock starting on the first and each
  one struck putting time back.
- Tests: the sim's, `bc-sector/tests/training_net.rs`, `bc-server/tests/proving.rs`, and the
  `inside` and `colony` e2e.

Next: the hall's own course and a level ladder (phase 6).

**Contracts on the Charter Board.** `bc-econ` `contracts.rs`: jobs the colony posts (deliver this
much ore or these parts to the dock; down Dolls over the field; bring a wreck home), each with a
reward held in escrow as the Exchange holds orders. Taken only at the Charter Board's hall in the
city (its door opens `Panel::Terminal(Spot::Charter)`), done in space, paid at the dock on the way
home (`Homecoming`, bounties). Wire: `Request::{Contracts, Take, Abandon}`, `Update::Contracts`.
Colony projects are contracts too: deliveries to the building site move its `Stage`
(`bc_sim::colony::city::Stage`, today `Stage(0)` everywhere), which becomes world state the Welcome
carries and every client draws from. Tests: econ unit tests (escrow balances, as the Exchange's
property test does), a server test that takes, flies and is paid; the colony e2e opens the board.

**Liveries.** Body, trim, accent and eye colours chosen at the suit's maintenance console from a
palette, carried on the suit (`bc_econ::suit::Suit.paint`), paid for in credits (a sink), and drawn
for everyone: sent with the roster on the control stream (it changes rarely), so the snapshot pays
nothing; `suits_vis::livery` takes it when there is one. Tests: econ (paint costs, survives strip and
refit), the hangar e2e paints a suit.

**Doll offensives.** The patrol scales with the pilots out (`SimConfig::target_dolls` as a base plus
a share per awake pilot, within the sector's suits), and now and then an offensive: a wing of Dolls
from one direction against the dock or MO-II, announced by the Charter Board, with a bounty on top.
Then something big: a carrier that launches Dolls (a new frame: content, model, a kit). All in the
sector tick, so no allocation and determinism goldens for the new scenarios.

**Agents as population.** Flaneurs that walk routes (`city_nav`) between the places, ride the trams
and sit at The Arrival; the server's own tugs, haulers and miners flying the lanes (agents on the
same protocol, started with the server). Tests: the plaza test with a walking flaneur; a sector
test with traffic.

**Where PvP lives: a decision for the owner.** Today every browser pilot is on the Colonies' side
and friendly fire is off, so people can't harm each other, and `DESIGN.md`'s hunted sleepers can be
hunted only by agents of another faction. The options:
1. As now: pilots against the Consortium's Dolls, never each other.
2. Lawless space past the colony's reach (say, beyond 8 km of the hull): PvP there, the colony's law
   inside it (Star Citizen's armistice zones; EVE's security levels).
3. Opt-in: a challenge sent with the lock-on and accepted.
4. Factions for pilots (`STORY.md`'s eras): sides chosen, and fought over.

**Living in the colony** (`LIFE.md`). Jobs in the colony as seats, worked by Arrivals or, when none
sits down, by the colony's staff; the pilot's body, fed on the wall clock while awake; meals cooked at
The Arrival, carried up the cap lift and felt in flight. **L0 is built** (`bc_econ::{seats, body, food}`,
`bc_sim::content::body`); its phases L1 to L9 say how the rest lands, the first being a meal eaten at
The Arrival (`Request::Eat`, the body on the pilot's record).

## P2: depth, once the above is in

- **Cost brackets** (Battle Operation 2): a suit's cost from its stat sheet or its parts' prices,
  used to tier contracts and to size the Dolls sent after a pilot.
- **Close quarters outside the law:** solid structures at the axis port and the building site's
  frames (`COLONY.md` 1.3's colliders first), where free aim is learnt.
- **Suits inside the colony** (`SUITS_INSIDE.md`), once the building site gives them work.
- **Low gravity near the axis** as a place to play.

## The mech games

`PEERS.md`, "The mech games": what Titanfall, Armored Core VI, MechWarrior and BattleTech, Steel
Battalion, Mecha BREAK and Daemon X Machina teach, in its list's order (15 to 22).

**15. Doom and ejecting.** *Built* (protocol v23; `DESIGN.md`, "Doom and ejecting").
- `bc_sim::sim::doom`: a pilot's breached suit is doomed for `DOOM_TICKS` (3 s), less
  `DOOM_PER_TORSO` a blow. Then `destroy`, which also takes the old kill code out of
  `damage_step`. `Sim::eject` ejects (a hulk, `Event::Eject`) or, doomed, blows the reactor
  (`Event::Blast`, `WeaponKind::Reactor`'s hits on hostile suits within its reach, no hulk).
  `Sim::tow` takes a free hulk out of the sector.
- `bc_sector`: `Control::Eject` and `Control::Tow`. A claim per slot on the hulk of the suit its
  pilot ejected from is towed `TOW_TICKS` (45 s) on, or at once when the session goes
  (`Report::Towed`). `Report::Lost` says how (`Loss`).
- `bc_econ`: `Request::Eject` (any rules), and `Hangar::towed`: salvage, with a torso that wasn't
  doomed as a part.
- The client: U (`input::eject_key`: doomed, a tap ejects and a hold blows the suit up;
  otherwise a hold), the HUD's `DOOMED` countdown, a doomed suit burning (`damage`), the capsule
  (`pods`) the camera follows, the blast (`fx`), and the kill feed. `bc-bot`: `eject`,
  `self_destruct`, and the Mobile Doll agent ejects when doomed.
- Tests: `bc-sim/tests/doom.rs` (doom, blows cutting it short, Dolls and sleepers without it,
  ejecting doomed or whole, a wreck in hand not towed, the blast, nobody ejecting inside), the
  sector's `survival_net` (the loss and the tugs, towing at once, nothing to tow after a blast),
  the hangar's `the_tugs_bring_an_ejected_pilots_wreck_home`, the proto round trips. Every
  determinism golden is unchanged.

**16. Stagger.** *Built* (protocol v24; `DESIGN.md`, "Stagger").
- `bc_sim::content::stagger`: each frame's stability, each weapon's impact, the stagger's second
  and its cut in thrust, the direct hit's half again, and how a suit steadies.
- `bc_sim::sim::stagger`: `damage_step` adds each blow's impact (`Sim::impact`); past the frame's
  stability the suit is staggered (`Event::Staggered`), with a spin knocked into it if it's free.
  `stagger_step` runs it down and drains the impact of suits left alone. `FlightMods::staggered`
  takes the attitude control (`flight::integrate`) and the legs (`ground`), and the weapons,
  blades and Full Open wait it out.
- The owner's prediction flies it from the own state's ticks (`Predictor::stagger`), with the
  arms clock held as through a change of form.
- `bc_proto`: the events' second extension, and the own state's impact, stagger and the
  designated target's impact (the doom now in steps of 3 ticks, so 37 free suits still fit a
  datagram).
- The client: the flight panel's `ATT` gauge, the `STAGGERED` banner, the target's gauge on its
  bracket, the attitude jets firing wild and the sparks (`damage`), the burst, clang and shake.
  `bc-bot`: `staggered` and `impact`.
- Tests: `bc-sim/tests/stagger.rs` (impact building to a stagger, a tumble with no shots and a
  quarter of the thrust, direct hits, draining, Dolls, a suit on its feet stumbling to a stop),
  `bc-client-core/tests/stagger_predict.rs` (a Leo under both flight rules and a Heavyarms pressing
  for its Full Open, staggered again and again, predicted exactly), `no_alloc`, the proto round
  trips and budgets. Five determinism goldens are re-recorded, native and wasm alike: their fights
  now stagger (the surface scenario's Heavyarms, staggered by the Dolls, fires once it's steady).

**17. Specials charged by the fight.** *Built* (protocol v25; `DESIGN.md`, "Specials charged by the
fight").
- `bc_sim::content::specials`: a blow dealt takes 1/450 of a special's whole cooldown off what's
  left, a blow taken 1/300 (`DEALT_FULL`, `TAKEN_FULL`). Full Open takes 45 s by itself (it was
  30 s), the Cross Crusher 8 s.
- `Sim::charge_special`, from `damage_step`: never from the special's own blows, nor while Full
  Open is under way or locked out. `Sim::special_charge` is what the own state carries, in 255ths
  (it was the cooldown in ticks ÷ 4). The state hash covers a special under way or charging.
- The HUD's `CHARGING ||||···· 52%`; `bc-bot`'s `special_charge`. The pilot's client hears of the
  fight's share from its own state, a snapshot late, as it hears of the blow.
- Tests: `full_open.rs` (its barrage charges nothing; dealt and taken take their shares; the own
  state's charge; a hard fight charges it in full), `melee.rs` (the Cross Crusher's own blows).
  The Gundams duel's golden is re-recorded, and the lock-on scenario's for the hash's new reach.

**18. The debrief.** *Built* (`DESIGN.md`, "Sorties").
- `bc_econ::debrief`: a sortie's lines at the colony's values (`catalogue::value`): earned (bounties,
  the hold's ore, the salvage in hand, by what it adds to the stores) and spent (propellant burnt,
  rounds fired or lost with their mount, the rack used, the armour's repair, what was shot off; a
  loss writes the suit off whole), and the net.
- `Hangar::came_home_debriefed` and `lost_debriefed`, from the suit as it went out (`Bay::Out`) and
  as it came home; the session sends the sheet with the sortie (`Update::Sortie`'s `debrief`, an
  optional field: no new version).
- The client: under the news for 12 s (`#debrief`), and line by line in the terminals' log;
  `HangarState::last_debrief` for agents.
- Tests: the debrief's and the hangar's units, and `bc-server/tests/hangar.rs` (docked, the stim the
  rack used is on it; ejected, the suit is written off).

**19. The enemy's gun.** *Built* (protocol v26; `DESIGN.md`, "The enemy's gun").
- `bc_sim::content::salvage::held_gun`: the beam or solid-round gun an arm chunk carried in its hand
  (no blades, launchers or guns that charge). Grabbed, the suit's `held_gun` state takes its
  rounds (`HELD_ROUNDS`, half a load).
- `Sim::gun_in_hand`; the weapons step fires it on FIRE_SECONDARY (`trigger_in_hand`, a plain shot
  from the hand's muzzle, the `HELD_SLOT`), holding the suit's own secondary off. The own state's
  secondary is the gun's while it's held; the state hash covers it.
- The arms clock's `in_hand` (told by the client from the held chunk, `World::gun_in_hand`), let go
  of after the tick's guns when GRAB is released or the chunk thrown, as the server's salvage step
  does. The HUD's secondary line; `bc-bot`'s `gun_in_hand`.
- Tests: `bc-sim/tests/held_gun.rs` (which limbs carry a gun; a Taurus's rifle fires from the left
  hand, on its own cooldown, and the Leo's machine cannon is back once it lets go; a machine cannon
  picked up has half a load, and fired dry it waits), `bc-client-core/tests/held_gun_predict.rs`
  (seeded at every snapshot, the prediction keeps time with the server's arms and pose; told
  nothing of the gun, it doesn't). The reference golden is re-recorded: its scripted pilots grab
  arms and fire them.

**20. A test range.** *Built* (`TRAINING.md`, "Phase 3"; `DESIGN.md`).
- `bc_econ::proving::Trainer` (the Board's Leo, the bay's build new and full, a new suit of any
  line) and `Request::Trainer`; the session keeps the pick, the board's view carries it, and
  boarding sends its frame and loadout in `Control::Board`.
- The page: `THE TEST RANGE` at the Blast Hall's desk. `bc-bot`: `board_trainer_as`.
- Tests: `proving`'s units, `training_net.rs`, and `bc-server/tests/proving.rs` (a Heavyarms).

**21. Aces, pay or salvage.** *Built* (`DESIGN.md`, "Aces: the Most Wanted").
- `bc_sim::content::aces`: the nine, their bounties, `ACE_EVERY` (5 min), the Leo they fly and its
  armour (`ACE_ARMOUR`, half as much again). `Sim::spawn_ace` fields the next on the list while
  none is out (`SimConfig::ace_every`); `Suits::ace` says which a Doll is. `Sim::ace_out` is the
  one in the sector, flying or downed, until its slot is let go. The state hash covers it.
- `bc_sector`: `SectorShared::ace`, a word saying which is out, as which suit, and whether it
  flies (`ace_word`); `Report::AceDown` to the session of the pilot who downs one, with its wreck;
  `Control::Claim` sends the tugs for it (`TOW_TICKS` on, a claim like an ejected pilot's own),
  and `Report::Towed` says it was the ace's.
- `bc_econ::charter`: the Most Wanted (who downed each last), the ladder, each pilot's terms
  (`Request::AceTerms`); `Board::ace_downed`, and `pay_ace` (the colony's money: `colony_paid`
  and standing). `Hangar::towed_ace` takes the wreck in as salvage.
- `bc-server`: the notes task's `AceWatch` puts the ace on the roster by its name and announces
  it; the session pays, or claims the wreck (unless the tugs are out for the pilot's own), and
  marks which one flies in the board's view. `--ace-every`.
- The page: MOST WANTED under the Charter Board, with the terms. The chart names the ace.
- Tests: `bc-sim/tests/aces.rs` (one out at a time round the list, a Leo standing more, named
  until its slot is let go), `bc-sector/tests/aces_net.rs` (the word; downed by a pilot, their
  session hears with the wreck, and the claim's tugs bring it home as the ace's; downed by nobody
  here, nobody hears), `no_alloc_sector` (aces downed and claimed), the board's and the hangar's
  units, the wire's JSON, and `bc-server/tests/aces.rs` (on the roster by name, out on the Most
  Wanted, the news, the terms, over real WebTransport).

**22. Staying up.** *Built* (`ARCHITECTURE.md`, "Under abuse").
- The review found inputs, poses and the radio limited, sign-ins waiting on a wallet capped, and
  the Hello on a deadline, but nothing on connections themselves (one address could open as many
  as it liked, each a task, each handshake as slow as QUIC's idle timeout allowed), and nothing on
  the hangar's requests (each can write the pilot's record to disk).
- `bc-server` `net::admit`: the accept loop asks `Admission` of each attempt, before any
  handshake. Under load (32 handshakes under way) an unvalidated address gets a QUIC Retry. An
  address (IPv4, or IPv6 by its /64) holds at most 8 connections (`--per-address`; loopback
  excepted) and the server 512 (`--max-connections`), and both handshakes are on 5 s deadlines.
  A connection's `Pass` gives its place back when it's dropped.
- `admit::Requests`: hangar requests and respawns at 10 a second, in bursts of 40; past that
  refused, and 200 refused in a row end the session.
- `/status` counts it all: refused (full, by address), retried, handshakes timed out, requests
  refused, sessions ended.
- Tests: `admit`'s units (the share, the ceiling, the Retry under load, the flood),
  `bc-server/tests/admit.rs` (an address past its share is refused and let back in once it has
  room; under constant load a real client proves its address and comes in; a session flooding its
  hangar is refused, then ended, and others are none the worse).

