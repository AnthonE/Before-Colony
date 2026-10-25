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
| Objectives that lead somewhere, desks worth the walk: nineteen objectives from the cap lift to siderium, each city desk's one thing, the Charter Board's pay at its desk, a flight certificate's beam rifle, The Arrival's Most Wanted (below) | built |
| P1: the Proving Ground: its course, the Blast Hall, its live fire, trainers boarded there, the drill and the board (`TRAINING.md`, phases 1 to 5) | built |
| Living in the colony, L0: seats, the body and its meals (`LIFE.md`) | built (a framework: nothing on the wire yet) |
| Living in the colony, L1: meals at The Arrival (`LIFE.md`) | built |
| The loop's loose ends, first pass: the colony open by default, launching out of the pilot's own bay door, landing on the docking hub and walking in, hints along the way (below) | built |
| The loop's loose ends, since: a strip chosen at the airlock, second keys for F1 and F10, the nightly browser suites, the auto-nav on an ion drive's crawl (below) | built |
| Propulsion: bigger tanks, the extended tank, propellant grades, the ion drive (`DESIGN.md`) | built |
| The mech games, 15: doom, ejecting and the self-destruct | built |
| The mech games, 16: stagger | built |
| The mech games, 17: specials charged by the fight | built |
| The mech games, 18: the debrief | built |
| The mech games, 19: the enemy's gun | built |
| The mech games, 20: a test range | built |
| The mech games, 21: aces, pay or salvage | built |
| The mech games, 22: staying up under abuse | built |
| Mecha Royale, 23: guns that shoot missiles down | built |
| Mecha Royale, 24: the HUD says why | built |
| Mecha Royale, 25: a death report | built |
| Mecha Royale, 26: a boot that says when it's stuck | built |
| Mecha Royale, 27: wrecks that die their own way | built |
| Mecha Royale, 28: crews | open |
| Contracts on the Charter Board: supply contracts, patrols, the great works and the charter vote (P1, below) | begun |
| The open city (`OPEN_CITY.md`): homes not instanced (H0 to H3), the street in the tick, cars you own, heat and the Watch, intrusion, raids, businesses | homes built; the rest designed |
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
  and is thrown out at 40 m/s (`BAY_LAUNCH_SPEED`) on top of the ring's own 125 m/s; the camera
  opens outside by the open door and watches it go (`bc-client`'s `launch_shot`). Into the colony
  (Q), it comes out of a lit port in the end cap's inner face behind the inner gate. The cockpit's
  prompt says which key goes where (Q only with the colony open).
- **Home by landing on the hub.** Home stays on the axis, where nothing moves (a door on the ring
  would take 0.7 g of thrust to hold): the dock's ring of lights as before, or the grip armed, a
  landing on the docking hub's end face near its middle (a landmark now, turning with the colony),
  and a walk onto its deck hatch.
- **Hints along the loop:** the airlock and the cap lift, the city (its map, trams, cars and the
  lift back up), the hub's landing, and the inner gate.

Built since, each a PR of its own:

- **A strip chosen at the airlock.** The airlock leads to three cap lifts, one down each strip's
  end cap (`bc_client_core::bay::Lift`; built as a cut, with no lobby: COLONY.md 2.2). E rides
  lift 1 to the Charter strip, Q chooses the next (2 Canal, 3 Gardens), the prompt names it, and
  `EnterCity { strip }` takes the one chosen; the choice goes back to the Charter strip's once the
  airlock is out of view. On another strip than an objective's place, the objective says the way
  there, and the city's map names the place and its strip. Tests: `bay`'s and `objectives`'
  units, and a step in the colony e2e (the Canal strip's lift down to The Depots and back up).
- **Second keys for F1 and F10:** `?` and O (`CONTROLS.md`, "Where we break convention" 6).
- **A nightly CI job for the browser suites** (`.github/workflows/e2e.yml`): one job builds the
  server, its agents and the web client, and each suite runs on its own runner from that build,
  nightly and by hand.
- **Auto-nav on an ion drive's crawl.** `nav::planned_braking` plans on the thrust the suit has
  (`Predictor::mods`): under the real rules a dry tank brakes on the drive, and a tank too low for
  the trip plans on what's left of it and then the drive. Tests: `nav`'s units (a dry Argo with a
  drive stops short of a point 5 km off, where it used to go 4.2 km past).

- **The Arrival's menu** (`LIFE.md`, L1: eating). *Built.* Its door opens the Most Wanted, pinned
  up behind the bar (`Panel::Arrival`, the terminals' `wanted` tab), and the menu is its second
  tab, `menu`, at the same door: the colony's staff's dishes, their hours and prices, and how fed
  the pilot is (`Request::Eat`, `Update::Body`; with the colony shut, the bay's terminal serves
  them). The on-foot line names the condition.

Next, each a PR of its own:

- **The Blast Hall's floor markings and the gantry's stairs** (`TRAINING.md`'s first ten minutes).
- **The launch shot's clocks.** The ring is drawn on the view clock and the own suit on the input
  clock; on the throw the suit's drawn place blends between them (`own.rs`), which shows as a lurch
  of a door's width if the release lands after the cut. Draw the colony on the own clock during the
  shot if it shows.

Later:

- **Landing at your own door**, with an approach assist that matches the ring's spin (or a
  tractor at the door), so a pilot can come home the way they left.
- **Flying out of the colony through the axis port** into space (`SUITS_INSIDE.md`'s handoff).
- **Solid end structures** (COLONY.md 1.3; a protocol bump of its own).
- **Trams and cars that people can't walk through** (COLONY.md phases 4 and 5's known gaps).
- **A server gate on the bay's own work asked from a city desk** (fitting, crafting, repairs):
  today only the page keeps a desk's terminal to its one tab.
- **A line counting toward siderium** on the objective (the titanium alloy and exotics for a
  foundry batch): two counts, where the panel shows one.
- **A first-class certificate that pays**, if the owner wants a class to earn more than the
  Board's rifle; today it only shows (`TRAINING.md`, "Rewards").
- **A broker's fee on trades made away from the Exchange floor** (from a bay's terminal), so the
  walk to the floor pays for itself.

## P0: before more players arrive

**A floor under loss.** *Built.* A signed-in pilot whose only suit is gone, with less than a
torso's worth of credits, stores and parts (at the colony's values, `catalogue::value`), finds a
worn Argo in the gantry from the Charter Board, at most once every 30 minutes (`Hangar::reissue`,
with a `reissued_at`; `bc-server` `session.rs::enter`, the return from a loss, and once a second
on the session's tick while the pilot is on foot with the bay empty and no wreck coming home, so
it comes the moment it's due; `reissue` checks the half hour before it values the stores, so
asking is cheap). The news says so. `STORY.md`'s "debt" is now "an advance". Tests: `bc-econ`
(lost, broke, reissued; not again within the half hour, however often asked; not while there's a
torso to build on or a torso's worth) and `bc-server/tests/hangar.rs` (over a server restart, from
the pilot's record; and in the bay as the half hour runs out). Known, and not made worse: a new
wallet is a new starter kit.

**Text chat.** *Built* (`bc-server` `radio.rs`, `bc-client` `chat.rs`). `Request::Say { text }` and `Update::Said { from, text }` on control-stream frames
tagged 11 (`bc_econ::wire`, `PROTOCOL.md`). One channel, the colony's radio, for everyone connected:
a ring in the server's shared state, read by each session on its 100 ms tick (off the hot path).
At most 5 lines in 10 s and 160 characters a line, control characters stripped, never logged (only
counted on `/status`). `/` opens it, and so does Enter wherever it docks nothing: on foot, and in
a suit away from where it docks (`bc_client_core::dock::EnterKey` routes each press once a frame);
keys don't reach the suit while typing. Agents: `bc-bot` `say` and `heard`. Tests: a two-bot
server test; the page's `ui` e2e types a line, and opens the line with Enter in flight.

**Objectives along the chain.** *Built* (`bc_client_core::objectives`, appended so their bits
stay; `DESIGN.md`, "Objectives and the chart"). Nineteen, in survival's order after
`TRAINING.md`'s first ten minutes: ride the cap lift down, report to the Proving Ground and earn a
flight certificate, a straight walk down the avenue to the Exchange floor and the Charter Board's
desk, standing with the colony, home to arm the suit, then out (Fornax, the Aft Well, ore home and
sold, the drones, Hermit, five drones), and on to the Most Wanted at The Arrival, one of the Almagest's
aces, a new Lodestar on the test range and siderium. The colony's (the cap lift, the city's, selling
on the Exchange) need the colony open (the Welcome's COLONY flag), standing, arming, the aces and
siderium survival, and arcade has none of the last eight.
Each has a realm (space, the bay, the city, or any terminal on foot): flying, the HUD shows the
first space one not done, so a pilot who launches first is never pulled home; on foot, the first
in order, passing over a space one flown past, and back to the Charter Board's desk while the
Board holds something for the pilot there. Each how-line starts with the way there from where
the pilot is, and the city's map marks the door with a `◆` (Hub Gate's, for the bay's and
space's). Tests: `objectives`' units (the
bits, the new Arrival's chain, flying first, a skipped hide, arcade, the colony closed, every
how-line's prefix, the ace's waypoint, a rifle waiting at the Board's desk), and the colony e2e
walking the desks in turn. The ◆ is on the door in the city view itself too, with its distance, and
G walks the pilot there (`bc-client/src/signs.rs`, `onfoot.rs`).

**The Arrival does something.** *Built.* Seats by its door (`bc_sim::colony::city::arrival_seats`, a
closed form of the door; benches drawn there), E to sit, others see you sit (the presence's
`ride` 15 is seated: no new bits, wire v16 for the meaning; the plaza takes it only on a seat), and
what's said on the radio over the heads of those near you. Flaneurs come and sit (`flaneur
--sit`, two of them whenever `dev.sh` opens the colony, as it does unless `BC_COLONY=0`). Tests:
the seats' geometry, the plaza's rules for sitting, and a step in the colony e2e.

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
- Tests: the rings against the city's walls; the run's rules; an Argo flying the whole course in the
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
seats the pilot in one of the Charter Board's Argos, or the build the test range picked (item 20,
below; the hatch's prompt names it), with `Control::Board` (`Sim::launch_at` with
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

**Contracts on the Charter Board.** *Begun* (`bc_econ::charter`; `DESIGN.md`, "The Charter Board").
One board per colony, kept with the exchange: a tab on every terminal in the bay, and the desk in
its hall on Charter Square (its door opens `Panel::Board`). Built:
- *Supply* contracts: a pilot's holds its reward in escrow from the moment it's posted, the
  colony posts its own as its stocks run low, and anyone but the issuer delivers part of one from
  their stores and is paid pro rata on the spot.
- *Patrols*, once the militia's hangar is built: down drones for so much in bounties within the
  hour, counted as each sortie ends.
- The great works and the charter vote, paid in credits and in standing.
- *Desks worth the walk* (the colony open): posting, taking a patrol and signing are the Charter
  Board's desk's (`Request::desk`, refused elsewhere with where to go: `bc-server`'s `desk.rs`
  checks the pilot stands in the place's room); an order resting on the book is the Exchange
  floor's (elsewhere it's downgraded to trade at once); ace terms are The Arrival's; the test
  range's pick is the Blast Hall's. The bay's terminals point at them. With the colony closed the
  bay does it all, as before.
- *The Board's pay waits at its desk* (the colony open): patrols' rewards, aces' bounties and a
  flight certificate's beam rifle (`Board::certify`, once a record or a guest's visit; the
  colony's desk out of rifles, a rifle's worth with no standing) wait in its `at_desk` account,
  shown as the board's `waiting`, and are paid out on walking into its hall. Patrol and ace pay are
  lines on the sortie's debrief. Salvage terms need standing.
- Wire: `Request::{Post, Withdraw, Deliver, TakePatrol, DropPatrol, Contribute, Sign, WatchBoard}`,
  `Update::Charter` (with `waiting`).
- Tests: `bc-econ`'s units (the colony's postings, pro rata, what's unpaid going back, the works,
  a patrol, the certificate's two paths, pay at the desk, salvage terms and standing), the
  ledger's property test with the board in it (`tests/ledger.rs`, certificates and aces' pay
  included), `bc-server/tests/charter.rs` (post, deliver, build, and the board over a restart,
  the colony closed), `bc-server/tests/desks.rs` (the bay pointed at the desks; a certified pilot
  walked to each desk and paid at the Board's), and the colony e2e opening the board at its desk
  (a delivery to the second foundry, a contract posted and withdrawn) and the Most Wanted at The
  Arrival.

Still to come: more jobs flown in space (bring a wreck home, escort a hauler, clear a claim), with
their briefings on the chart; the building site that the works build out, whose `Stage`
(`bc_sim::colony::city::Stage`) is still `Stage(0)` everywhere, and which becomes world state the
Welcome carries and every client draws from (today the works change the world only through their
effects: the foundry's fee, the patrols); and the Board's own standing patrol (one, paying 500 CR
on 500 of bounties, issued by THE CHARTER BOARD), so that patrols, the desk's TAKE and the
debrief's `PATROL` line work before the militia's hangar is built (patrols come only with it
today, so a fresh server has none). It's a new faucet and undercuts the hangar's point: the
owner's call.

**Liveries.** Body, trim, accent and eye colours chosen at the suit's maintenance console from a
palette, carried on the suit (`bc_econ::suit::Suit.paint`), paid for in credits (a sink), and drawn
for everyone: sent with the roster on the control stream (it changes rarely), so the snapshot pays
nothing; `suits_vis::livery` takes it when there is one. Tests: econ (paint costs, survives strip and
refit), the hangar e2e paints a suit.

**Drone offensives.** The patrol scales with the pilots out (`SimConfig::target_drones` as a base plus
a share per awake pilot, within the sector's suits), and now and then an offensive: a wing of drones
from one direction against the dock or Fornax, announced by the Charter Board, with a bounty on top.
Then something big: a carrier that launches drones (a new frame: content, model, a kit). All in the
sector tick, so no allocation and determinism goldens for the new scenarios.

**Agents as population.** Flaneurs that walk routes (`city_nav`) between the places, ride the trams
and sit at The Arrival; the server's own tugs, haulers and miners flying the lanes (agents on the
same protocol, started with the server). Tests: the plaza test with a walking flaneur; a sector
test with traffic.

**Where PvP lives: a decision for the owner.** *Inside the colony, decided (October 2026): crime
anywhere, answered by the Watch, homes raidable in the raid hours (`OPEN_CITY.md`). In space it's
still open.* Today every human pilot flies for the Colonies (the
server decides, whatever the client asks, and the Welcome says so: protocol v29) and friendly fire
is off, so people can't harm each other, and `DESIGN.md`'s hunted sleepers can be hunted only by
agents of another faction (the Alliance: nobody but the drones flies for the Almagest). The options:
1. As now: pilots against the Consortium's drones, never each other.
2. Lawless space past the colony's reach (say, beyond 8 km of the hull): PvP there, the colony's law
   inside it (Star Citizen's armistice zones; EVE's security levels).
3. Opt-in: a challenge sent with the lock-on and accepted.
4. Factions for pilots (`STORY.md`'s eras): sides chosen, and fought over.

**Living in the colony** (`LIFE.md`). Jobs in the colony as seats, worked by Arrivals or, when none
sits down, by the colony's staff; the pilot's body, fed on the wall clock while awake; meals cooked at
The Arrival, carried up the cap lift and felt in flight. **L0 is built** (`bc_econ::{seats, body, food}`,
`bc_sim::content::body`), and so is L1: a meal eaten at The Arrival (`Request::Eat`, the body on the
pilot's record). Its phases L2 to L9 say how the rest lands, the first being the condition felt in
flight.

## P2: depth, once the above is in

- **Cost brackets** (the licensed 6v6 shooter, `PEERS.md`): a suit's cost from its stat sheet or its parts' prices,
  used to tier contracts and to size the drones sent after a pilot.
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
  `self_destruct`, and the drone agent ejects when doomed.
- Tests: `bc-sim/tests/doom.rs` (doom, blows cutting it short, drones and sleepers without it,
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
  blades and Full Salvo wait it out.
- The owner's prediction flies it from the own state's ticks (`Predictor::stagger`), with the
  arms clock held as through a change of form.
- `bc_proto`: the events' second extension, and the own state's impact, stagger and the
  designated target's impact (the doom now in steps of 3 ticks, so 37 free suits still fit a
  datagram).
- The client: the flight panel's `ATT` gauge, the `STAGGERED` banner, the target's gauge on its
  bracket, the attitude jets firing wild and the sparks (`damage`), the burst, clang and shake.
  `bc-bot`: `staggered` and `impact`.
- Tests: `bc-sim/tests/stagger.rs` (impact building to a stagger, a tumble with no shots and a
  quarter of the thrust, direct hits, draining, drones, a suit on its feet stumbling to a stop),
  `bc-client-core/tests/stagger_predict.rs` (an Argo under both flight rules and an Antares pressing
  for its Full Salvo, staggered again and again, predicted exactly), `no_alloc`, the proto round
  trips and budgets. Five determinism goldens are re-recorded, native and wasm alike: their fights
  now stagger (the surface scenario's Antares, staggered by the drones, fires once it's steady).

**17. Specials charged by the fight.** *Built* (protocol v25; `DESIGN.md`, "Specials charged by the
fight").
- `bc_sim::content::specials`: a blow dealt takes 1/450 of a special's whole cooldown off what's
  left, a blow taken 1/300 (`DEALT_FULL`, `TAKEN_FULL`). Full Salvo takes 45 s by itself (it was
  30 s), the Pincer Strike 8 s.
- `Sim::charge_special`, from `damage_step`: never from the special's own blows, nor while Full
  Salvo is under way or locked out. `Sim::special_charge` is what the own state carries, in 255ths
  (it was the cooldown in ticks ÷ 4). The state hash covers a special under way or charging.
- The HUD's `CHARGING ||||···· 52%`; `bc-bot`'s `special_charge`. The pilot's client hears of the
  fight's share from its own state, a snapshot late, as it hears of the blow.
- Tests: `full_salvo.rs` (its barrage charges nothing; dealt and taken take their shares; the own
  state's charge; a hard fight charges it in full), `melee.rs` (the Pincer Strike's own blows).
  The Lodestars duel's golden is re-recorded, and the lock-on scenario's for the hash's new reach.

**18. The debrief.** *Built* (`DESIGN.md`, "Sorties").
- `bc_econ::debrief`: a sortie's lines at the colony's values (`catalogue::value`): earned (bounties,
  the hold's ore, the salvage in hand, by what it adds to the stores) and spent (propellant burnt,
  rounds fired or lost with their mount, the rack used, the armour's repair, what was shot off; a
  loss writes the suit off whole), and the net.
- `Hangar::came_home_debriefed` and `lost_debriefed`, from the suit as it went out (`Bay::Out`) and
  as it came home; the session sends the sheet with the sortie (`Update::Sortie`'s `debrief`, an
  optional field: no new version).
- The Charter Board's pay for the sortie (`Hangar::board_paid`, kept in the record's `sortie_pay`
  till the sheet is made, cleared by the next launch): `PATROL #3` and `ACE LYRA`, with ` · AT THE
  BOARD` when it waits at its desk, put among the earnings before the costs (`Debrief::paid`).
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
- Tests: `bc-sim/tests/held_gun.rs` (which limbs carry a gun; a Corvus's rifle fires from the left
  hand, on its own cooldown, and the Argo's machine cannon is back once it lets go; a machine cannon
  picked up has half a load, and fired dry it waits), `bc-client-core/tests/held_gun_predict.rs`
  (seeded at every snapshot, the prediction keeps time with the server's arms and pose; told
  nothing of the gun, it doesn't). The reference golden is re-recorded: its scripted pilots grab
  arms and fire them.

**20. A test range.** *Built* (`TRAINING.md`, "Phase 3"; `DESIGN.md`).
- `bc_econ::proving::Trainer` (the Board's Argo, the bay's build new and full, a new suit of any
  line) and `Request::Trainer`; the session keeps the pick, the board's view carries it, and
  boarding sends its frame and loadout in `Control::Board`.
- The page: `PICK A SUIT · THE TEST RANGE` at the Blast Hall's desk, and only there (`Request::desk`: the server
  refuses a pick from anywhere but the hall's room). `bc-bot`: `board_trainer_as`.
- Tests: `proving`'s units, `training_net.rs`, and `bc-server/tests/proving.rs` (an Antares).

**21. Aces, pay or salvage.** *Built* (`DESIGN.md`, "Aces: the Most Wanted").
- `bc_sim::content::aces`: the nine, their bounties, `ACE_EVERY` (5 min), the Argo they fly and its
  armour (`ACE_ARMOUR`, half as much again). `Sim::spawn_ace` fields the next on the list while
  none is out (`SimConfig::ace_every`); `Suits::ace` says which a drone is. `Sim::ace_out` is the
  one in the sector, flying or downed, until its slot is let go. The state hash covers it.
- `bc_sector`: `SectorShared::ace`, a word saying which is out, as which suit, and whether it
  flies (`ace_word`); `Report::AceDown` to the session of the pilot who downs one, with its wreck;
  `Control::Claim` sends the tugs for it (`TOW_TICKS` on, a claim like an ejected pilot's own),
  and `Report::Towed` says it was the ace's.
- `bc_econ::charter`: the Most Wanted (who downed each last), the ladder, each pilot's terms
  (`Request::AceTerms`, settled at The Arrival's counter with the colony open; salvage terms only
  for pilots of standing); `Board::ace_downed`, and `pay_ace` (the colony's money: `colony_paid`
  and standing; into the hangar, or, the colony open, waiting at the Board's desk).
  `Hangar::towed_ace` takes the wreck in as salvage.
- `bc-server`: the notes task's `AceWatch` puts the ace on the roster by its name and announces
  it; the session pays, or claims the wreck (unless the tugs are out for the pilot's own), and
  marks which one flies in the board's view. `--ace-every`.
- The page: MOST WANTED at The Arrival's counter, with the terms (under the bay's Charter Board
  tab with the colony closed; with it open, the tab points there). The chart names the ace.
- Tests: `bc-sim/tests/aces.rs` (one out at a time round the list, an Argo standing more, named
  until its slot is let go), `bc-sector/tests/aces_net.rs` (the word; downed by a pilot, their
  session hears with the wreck, and the claim's tugs bring it home as the ace's; downed by nobody
  here, nobody hears), `no_alloc_sector` (aces downed and claimed), the board's and the hangar's
  units, the wire's JSON, and `bc-server/tests/aces.rs` (on the roster by name, out on the Most
  Wanted, the news, the terms, salvage refused until the pilot has standing, over real
  WebTransport).

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

## Mecha Royale

`PEERS.md`, "Mecha Royale (2026)": what a browser mech battle royale's build teaches, in its list's
order (23 to 28).

**23. Guns that shoot missiles down.** *Built* (protocol v28).
- `WeaponSpec::shoots_down`: the stream weapons' rounds (head guns, gatlings, machine guns; a const
  check keeps it to projectiles).
- `Sim::round_meets_missile`, from `projectile_step`: the first hostile missile (not the round's
  owner's, nor its faction's without friendly fire) within the round's radius and the missile's
  body, both swept over the tick (the missiles fly after the rounds). It must come before the
  round's suit and blocker. `Sim::shot_down` bursts it (`BurstCause::ShotDown`, the cause's 3 bits).
  Not inside the colony, and not in a remote pilot's lag-compensated start (missiles fly in the
  present). No allocation; nothing when no missile flies.
- Tests: `bc-sim/tests/missiles.rs` (a head-gun stream across a salvo's path bursts it; the owner's
  own rounds and a friendly's don't), the proto round trips.

**24. The HUD says why.** *Built.*
- `bc_client_core::holdup`: why a loadout weapon won't fire, in the server's order (safe inside
  the colony, staggered, changing form, overheated, empty, its arm lost, its hand full, low energy,
  cycling for more than half a second), from the own state; and `drain`, the alarm for a leaking
  tank or a suit run hot, with the rack's kit for it. `World::hand_holding` says which hand holds.
- The HUD: the reason after the weapon's line, the drain on the alert line.
- Tests: `holdup`'s units.

**25. A death report.** *Built* (protocol v28).
- `Event::Kill`'s `weapon`: the breaching blow's (`Doom::with`, kept from the blow that doomed it,
  through `destroy`), none for a pilot who ejected whole. Five bits, 0 for none.
- `World::my_loss_by` (named as it happened), `FeedLine::Kill`'s weapon; the HUD's death screen
  (`BY WING-07 · BEAM RIFLE`) and kill feed.
- Tests: the proto round trips (a weapon, none, an unknown weapon refused).

**26. A boot that says when it's stuck.** *Built.*
- `web/index.html`: the watchdog, a plain script ahead of everything (`window.bcBoot`): the step's
  line and meter, a stall timer per step re-armed by progress, script load failures, errors and
  rejections, the last console errors (a panic's message), and on failure the reasons and RELOAD,
  TRY WEBGL 2 and COPY DETAILS.
- `web/loader.js`: the steps; the wasm fetched with its bytes counted (`fetchCounted`, a meter
  when it isn't compressed in flight); the loading screen up until the first frame (`bcHideBoot`
  from `dev_hooks`); the WebGPU adapter request given 4 s.

**27. Wrecks that die their own way.** *Built.*
- `bc_client_core::wreck::plan`: from the suit's slot and life, three to five secondary blasts at
  the parts still on it, one in six ending with the reactor's, and the burn's length; `damage`
  plays it (the reactor's with a ring and its armour).
- Tests: `wreck`'s units (the same suit the same way, others not; blasts in order, on what's left).

**28. Crews.** Open. Gunner seats on a carrier or a transport, under a captain's fire policy
(Mecha Royale's "designate & lock" or free fire). It needs a vehicle with seats first.
