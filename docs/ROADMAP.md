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
| The mech games, 15: doom, ejecting and the self-destruct | built |
| P1's rest, P2, the mech games' rest below | planned |

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

