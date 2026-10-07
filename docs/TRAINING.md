# The Proving Ground: training inside the First Colony

The owner's ask (October 2026): somewhere inside the colony for pilots to train, "some sort of
workshop", or "a giant blast chamber": a tutorial area where a new pilot walks around freely,
then gets into a Gundam and learns to fly it. They also asked for the feel of X-Wing (1993) and
TIE Fighter (1994). What those games did, and what we take from them, is in `PEERS.md` ("X-Wing and
TIE Fighter"). This document is the design built from that ask. Phases 1 to 5 are built, and the
drill (X-Wing's Maze, in the hall).

## Status

| Phase | State |
|---|---|
| 1: the course. Rings in the colony's air from the inner gate down to a pad on Hub Gate's square, flown in your own suit | built |
| 2: the Blast Hall. A hall off Hub Gate's square, walked on foot and flown into | built |
| 3: boarding in the hall. One of the Charter Board's trainers, boarded at the hall's gantry, flown out through the blast doors and docked back there | built |
| 4: the board. Times checked by the server, the day's best on the hall's back wall and at its desk, a pilot's bests on their record | built |
| 5: live fire in the hall: the colony's law's one exception, the owner's call. Training rounds that touch no suit and score on its targets | built |
| The drill: the hall's targets lit one at a time against a clock, X-Wing's Maze | built |
| The test range: any build boarded at the gantry (the Board's Leo, the bay's build, any line's), Armored Core VI's test mode | built |
| 6: more courses (the hall's own among them), and a level ladder | planned |

## What we take from X-Wing and TIE Fighter

- **Training is a place in the hub, not a menu.** X-Wing's concourse had the Proving Ground
  behind its left hangar door, beside Historical Combat's, with the Tour of Duty desk on the right.
  TIE Fighter's atrium had the Training Simulator's door beside the Combat Chamber's. Ours is a
  door off Hub Gate's square, the first place a pilot reaches from the cap lift (phase 2). In
  phase 1 it begins at the inner gate, where a suit first comes in.
- **Rings in order, against a clock, and safe to hit.** X-Wing's Maze was gates on platforms
  floating in space, flown in order against a clock that got shorter every level. The manual says
  its gates "are actually holographic projections with which you can safely collide." Ours are
  rings of light: nothing in them is solid, and the colony's air and walls are the hazard.
- **It teaches the machine, not just the stick.** The official guide shows the Maze was really a
  lesson in energy management: lasers into shields, shields front to back. Ours teaches what the
  colony's inside does to a suit:
  - no pull near the axis, and a full g at the floor;
  - Coriolis on the way down;
  - air, which caps speed and makes hovering cost thrust;
  - walls that stop you;
  - and, at the end, the grip.
- **A reason in the world.** TIE Fighter's manual: "Although thousands of TIE fighters are in
  use… that does not mean they are expendable." Ours: the Charter Board's Leos are second-hand and
  dear, and it won't send an Arrival out to the Consortium's Dolls untried.
- **Something to show for it.** X-Wing gave a flight badge for clearing eight levels; TIE Fighter
  gave a patch per craft and a bronze, silver or gold medallion. Ours is the Charter Board's flight
  certificate (first, second or third class against the par), and a place on the hall's board, both
  from the times the server checks (phase 4); a signed-in pilot's bests are on their record.
- **Recommended, never required.** X-Wing's manual: Maze, then Historical Combat, then the Tours,
  "recommended, but not required." TIE Fighter kept green pilots out of battles that mattered. Ours
  is pointed to by an objective, and gates nothing.
- **A tutorial that talks to you while you fly.** TIE Fighter's Combat Chamber missions carried
  in-flight lessons from your wingmen and command. Ours is the HUD's panel: one plain line for the
  ring ahead (`STORY.md`: told through notices, never exposition dumps).

## The fiction

The colony's builders tested thrusters and suit engines in a hall at the foot of the docking hub's
end cap. It was built to take a burn at full throttle without cracking its walls: **the Blast
Hall**. Now that the docks are open, the Charter Board runs it as **the Proving Ground**, where an
Arrival shows they can fly a Leo before the Board trusts them with one. The course runs from the
hall out through the colony's air. In phase 1 it runs the other way: from the inner gate down to
the square, which the hall will open onto.

## A new pilot's first ten minutes, once it's all built

1. Wake in the bay (as now). The hints teach walking and using things, and the objectives point
   to the cap lift.
2. Ride the cap lift down to Hub Gate. A new objective, **REPORT TO THE PROVING GROUND**, marks
   the hall's door with a `◆` on the square.
3. **On foot in the hall: the tutorial.** It's a floor to walk freely across: walk, run, jump,
   crouch. At the instructor's desk, E opens the board: the day's best round the course and
   through the drill, the pars, the pilot's own bests. This is X-Wing's Ready Room, with its
   viewscreen; the same board hangs high on the hall's back wall. *(Built, but for the floor
   markings that teach running, jumping and crouching, and the gantry's stairs.)*
4. **Board the trainer** at the gantry's hatch (built). It's a Leo of the Board's, not the pilot's
   own: nothing of theirs is at stake, and it can't be lost inside, where no blow lands.
5. **Fly it** (built).
   - In the hall: weapons free, and the drill, from the gantry or anywhere on the firing line.
   - Lift off, out through the blast doors over the square, up to the course's start ring by the
     inner gate, and round the course down to the pad by the hall.
   - Back in through the doors to dock on the gantry, then on foot again.
6. **The certificate** and the board, and on into the chain: the bay, the launch tunnel and space.

## Phase 1: the course (built)

Everything is a closed form, like the rest of the colony. The rings are content
(`bc_sim::colony::course`), and whether a move went through one is geometry. Nothing of it is
stored or sent, and the server knows nothing of it yet.

**The rings.** 13 of them, then the pad. They run over the Charter strip, where the browser's cap
lift comes down.

| Stretch | Rings | What it teaches |
|---|---|---|
| START | 1, 300 m straight on from the inner gate's mouth, 300 m off the axis | no pull: the suit floats as in space |
| DESCENT | 2 to 4, out over the first window towards the strip, from 800 m off the axis to 900 m off the floor | the pull grows all the way down, and Coriolis pushes a falling suit against the spin |
| AVENUE | 5 and 6, down onto the avenue between the towers | flying low among walls, in air that caps speed |
| SLALOM | 7 to 9, 32 m over one carriageway, then the other, then the first | small corrections, low |
| CLIMB | 10, 280 m up, facing up and on | climbing against a full g |
| OVER THE TOP | 11, 620 m up, facing home | turning a suit round |
| HOME | 12 and 13, back along the avenue and in low (60 m) just short of the square | a long descent, then slowing |
| THE PAD | a 30 m ring on Hub Gate's square | coming to rest and setting down with the grip (L) |

The rings are 20 to 60 m in radius; the slalom's are the tightest. Each faces either a set way or
along the course (from the ring before it to the one after).

**The run** (`bc_sim::colony::course::Run`) is kept by the pilot's own client for its HUD, and
by the server for the board (phase 4), alike:
- **The clock starts** when the suit flies through the start ring the right way.
- **The rings count in order.** A ring out of order counts for nothing.
- **It finishes** when the suit stands on the pad, on the city, after every ring.
- **Starting over:** flying the start ring again starts the clock again.
- **Lapsing:** two minutes without a ring lets the run lapse.
- **Timing** is on the suit's own clock, with the crossing worked out to a fraction of a tick. A
  frame rate can't buy a tenth: the client steps its run with the prediction's ticks one by one
  (`Predictor::sample`), as the server steps its own with the sector's.
- **No teleporting through rings:** a move longer than 300 m (a dock, a launch, a correction)
  crosses nothing.
- **Leaving** the colony starts the next run afresh.

**Par and certificates.** The par is 2:00 (`PAR_S`).
- **First class:** within par.
- **Second class:** within half as long again.
- **Third class:** flown at all.

The test's Leo flies through every ring's middle by rote, at a steady 80 m/s on flight assist, and
lands carefully. It takes 2:25.7: second class. A Leo flies level at about 190 m/s (240 on boost),
so a pilot who flies a line beats two minutes. The best time is kept with the settings
(`course_best_ms`), as the sights found are.

**On the screen** (`bc-client/src/course.rs`):
- **The rings** are drawn on the city's layer: the next one bright and pulsing, the rest dim, and
  the ones flown gone. Pilots on foot in the city see them too, over Hub Gate.
- **The panel.** While flying inside, the objectives' panel shows the course:
  - not started: `FLY THE START RING`, with its range;
  - running: the ring number, the stretch, the clock and the range;
  - finished: the time and the class.
- **The marker.** The waypoint `◆` is on the next ring (the inner gate's when not running).
- **Sound.** A chime for each ring.
- **News.** Finishing puts the time and the certificate in the middle of the screen
  (`COURSE FLOWN · 1:58.2 · FIRST CLASS`).

**By the real flight rules** (`BC_FLIGHT=real`) a Leo's tank runs dry soon after the turn over the
top. It's holding itself up in air, and every newton of that burns propellant. The course is the
anime rules' (the server's default). Under the real rules it's a lesson in propellant, which may
be the point. See the open questions.

**Tests:**
- `bc_sim::colony::course`'s units, against the city's real walls (`interior::probe`): every ring
  is inside the colony and clear of the city by more than a hull; the straight way between rings
  is clear by two hulls; each ring faces the way the course comes through it; the pad is open
  ground on Hub Gate's square; a ring counts only when flown through the right way.
- `course`'s run: it counts rings in order, starts over at the start ring, lapses, ignores a jump,
  and reads its clock (`bc_sim`); and **a Leo flies the whole course in the server's interior
  simulation**: launched at the inner gate under the anime rules, it threads every ring, lands on
  the pad with the grip, and earns second class (`bc_client_core::course`).

## Phase 2: the Blast Hall (built)

- **Where.** A key place on the Charter strip, the first off Hub Gate's square from the cap lift:
  the block on the square's far side from the avenue (`bx` 5, row 3), its blast doors facing the
  square (`content::city::PLACES`, `proving_ground`). A block is 104 m between its streets (94 m
  inside its sidewalks): as big as the city's grid allows without closing a street, and closing one
  would break the traffic's rings (`ARCHITECTURE.md`, "The colony inside").
- **The hall** is the rooms' machinery (`colony::city::room`) at a suit's scale:
  - `PlaceKind::Proving`, appended to `PLACES` (the other places keep their indices);
  - a room 86 m deep, 84 m wide and 60 m to its ceiling, under a roof at 72 m;
  - a door size per kind (`content::city::door_size`): the hall's blast doors are 40 m wide and
    45 m high, and people walk through them too;
  - a counter width per kind (`counter_width`): the instructor's desk is 8 m long;
  - the street's lamps keep clear across the whole of the doors (`furniture`), not just 6 m round
    their middle.
  - Its solids are the hall's (`Building::solids`), so the walker, the plaza's checks and suits
    inside (`interior::constrain`, `probe`) all have it, and a suit lands on its floor with the
    grip as anywhere on the city.
  - A layout change: `CITY_VERSION` 4, protocol v21, and `CITY_GOLDEN` moved (the traffic's, the
    people's and the interior's didn't).
- **Inside** (`city.wgsl`, the rooms' kind 4):
  - bare blast concrete in 6 m panels, scorched low down, over a band of yellow and black
    chevrons a suit's knee high;
  - a floor of thruster-scarred slabs ruled in yellow every 10 m;
  - steel trusses and floodlights 60 m up;
  - on the back wall, the course drawn in light: its rings strung along its line, with a pulse
    running down them.
  - It's lit by its own lamps, and the eye adapts going in, as in the other rooms.
- **The desk** (E at it): the pilot's best round the course and its certificate, the par, and how
  to bring a suit in (from the bay, Q at the cockpit, by the inner gate).
- **The objective** `REPORT TO THE PROVING GROUND` (`Objective::ProvingGround`, appended, its bit
  kept): with the colony open, the first on foot after the cap lift, done at the hall's desk. On
  the city's map (M) a `◆` marks the doors.
- **Tests:**
  - the rooms' tests cover it as they do the others: a walker from the street through the doors to
    the desk; the front wall either side of the doors, the side and back walls, the ceiling; who's
    in it and who isn't;
  - a Leo flies in through the blast doors from over the square without touching a wall, and
    lands on the hall's floor with the grip (`bc-sim/tests/interior.rs`);
  - the objectives' chain runs through it;
  - the `colony` e2e walks across the square and in to the desk, and uses it.

**Not yet:** floor markings that teach running, jumping and crouching.

## Phase 3: boarding in the hall (built)

**The gantry** (`bc_sim::colony::hall`): a pad on the hall's floor in its firing line, to one side
of the way in from the blast doors (`GANTRY`: 26 m from the doors' middle, 22 m in), drawn as two
rings of light (`bc-client/src/course.rs`), bright for a trainer's pilot. Its hatch, where a pilot
on foot boards, is 14 m from the pad's middle toward the hall's (`hatch`).

**Boarding.**
- On foot at the hatch, facing the pad, the prompt says `E  BOARD A TRAINER`. E asks
  `Request::BoardTrainer` (`board_trainer`). The screen goes dark as it does boarding in the bay.
- The server checks the pilot stands at the hatch: the plaza's last pose of them, within 12 m.
- It takes a slot in the inside's sector and seats them with `Control::Board`: the suit the pilot
  picked at the desk (the test range, below; by default a Leo with everything fitted and loaded,
  `Loadout::full`), put on the gantry by `Sim::launch_at` with `LaunchAt::Gantry`. It stands on the pad facing the targets, gripping until its pilot is first
  heard from.
- Nothing of the pilot's hangar goes with it: their own suit stays in their bay.
- They're off the street: out of the plaza, no longer watching the inside from it. They're
  welcomed to the inside's sector (Welcome sector 2) as a launch through the inner gate is, and
  their `place` update says `trainer`.

**Docking back.**
- A trainer (`Suits::trainer`) docks only on its gantry: at rest within 14 m of the pad's middle,
  no higher than 30 m and slower than 4 m/s (`hall::in_gantry`, through `Sim::docked`).
- A trainer at the inner gate doesn't dock, and neither does a suit from the bays on the gantry.
- Enter docks it there. The session puts the pilot back on foot at the hatch: the plaza takes their
  first pose there (`Plaza::enter_at`), rather than at Hub Gate. They're welcomed back to their
  bay's sector and watch the inside again. Nothing comes home: the Board keeps its suit.
- A pilot who leaves while flying one loses nothing: it goes, and their bay is untouched.

**On the HUD:**
- the waypoint and the dock marker point at the gantry (`GANTRY … ENTER: climb out`);
- `ON THE GANTRY  ENTER: climb out` while it's docked there;
- the news `A TRAINER OF THE CHARTER BOARD'S · WEAPONS FREE IN THE HALL`.

**Tests:**
- `bc-sim/tests/interior.rs`: a trainer stands on the gantry till its pilot is heard from and docks
  only there; one flies out through the blast doors and back, never in a wall, and docks on the
  gantry; there's no gantry in space.
- `hall`'s units: the pad is open floor in the firing line, the hatch beside it facing it.
- `bc-sector/tests/training_net.rs`: boarded, cleared the drill and docked back through the
  sector's queues; refused outside the colony.
- `bc-server/tests/proving.rs`: over real WebTransport, walking in from Hub Gate; and a pilot gone
  while flying one wakes in their bay, their own suit there.
- The `inside` e2e: boards at the hatch, fires the drill, and docks back.

**The test range** (`bc_econ::proving::Trainer`; Armored Core VI's test mode, `PEERS.md`):
- At the desk, under the board, `THE TEST RANGE` lists what the gantry can ready: the Board's Leo,
  `YOUR BAY'S BUILD`, and a new suit of each line the colony builds. Picking one sends
  `Request::Trainer { build }`; the session keeps it and says so (`THE GANTRY READIES A NEW
  HEAVYARMS`), and the board's view carries it back (`trainer`), so the page shows which.
- The bay's build is the suit standing in the pilot's bay as built (its parts, weapons and
  equipment) but new and full: no wear, no faults, a full tank and full loads, nothing in its
  rack. Out on a sortie, or with an empty bay, there's nothing to try.
- Boarding takes the pick's frame and loadout to `Control::Board`. It flies, fights the drill and
  docks back as any trainer does; nothing of the pilot's is taken, and nothing comes home.
- Tests: the trainers' units (`proving`), `bc-sector/tests/training_net.rs` (any build boards at the
  gantry with what its loadout says), `bc-server/tests/proving.rs` (a Heavyarms boarded at the
  hatch).

**Not built** of the plan: the course's start moved to the blast doors, a second course for the
hall (phase 6). Today a trainer flies up to the inner gate's start ring; the course comes home to
the pad by the hall.

## Phase 4: the board (built)

**The server checks the times, in the interior sector's tick** (`Sector::watch_training`), not by
sampling `Metrics` in the session as first planned.
- Each pilot's run (`course::Run`, moved into `bc_sim` beside the rings) is stepped with where
  their suit stands at the end of every tick. That's the place their prediction has for the same
  tick, so the server's time and the client's agree to the millisecond (`training_net`).
- Their drill (below) is fed their own rounds' `TargetHit` events and the clock.
- It costs a couple of closed forms a pilot a tick, and allocates nothing (`training_net` counts
  the heap).
- A course flown or a drill cleared is reported on the slot's report ring: `Report::Course { ms }`,
  `Report::Drill { ms }`.

**The board** (`bc_econ::proving::Board`, kept by the server's `proving.rs`):
- the day's best round the course and through the drill (days in UTC), each pilot once with their
  best, ten kept;
- the best ever;
- in the data directory (`proving.json`), saved every minute and on shutdown, and turned over at
  midnight by the server's clock.

**A pilot's bests** (`Bests`) are on a signed-in pilot's record (`PilotRecord.proving`); a guest's
last the visit.

**Told:**
- To the pilot: `THE BOARD · THE DRILL 0:21.3 · FIRST CLASS · 2ND TODAY` (or `· THE BEST EVER`,
  or `· YOUR BEST`).
- To everyone in the colony (in its city or flying inside it): `Update::Proving(BoardView)`,
  whenever the board changes (at most every 2 s) and when they come in. It carries names and
  times, their own rows marked, and nobody's key.
- `/status`: `proving`, the day's callsigns and times.

**Shown:**
- **On the hall's back wall** (`bc-client/src/board.rs`): 64 by 20 m, from 38 m up to 58 m, over
  the course drawn in light. It holds the day's best, eight a column, and the best ever. The UI
  lays it out into a texture of its own, drawn by a camera of its own for a few frames each time
  the board changes, as the cockpit's monitors are.
- **At the desk:** E opens the terminals' panel on its PROVING GROUND tab, with the day's lists, the
  records, the pars, the pilot's own bests, and how to board a trainer.
- The client keeps its own bests with its settings too (`course_best_ms`, `drill_best_ms`).

**Tests:**
- `bc-econ`'s units: the board's lists, ties, the ten kept, the day's turn, the records.
- `training_net`: the sector's times are the pilot's, and it allocates nothing.
- `bc-server/tests/proving.rs`: a drill cleared over the wire goes on the board, with the same
  time; to the pilot, in `/status` and on their record; and the board and the record outlive a
  restart.
- The `colony` e2e opens the board at the desk.

## The drill (built)

X-Wing's Maze in the Blast Hall (`hall::Drill`, after phase 5's live fire).
- **Its targets light one at a time,** in a set order of twenty (`DRILL`), so every pilot flies the
  same drill: the four still ones by the back wall first, then the bobbing, then the sweeping,
  then a mix. The aim swings from side to side, and never stays on one target twice in a row.
- **The clock starts on the first,** with 12 s on it; every lit target struck after it puts 3 s back
  (`DRILL_START_S`, `DRILL_BONUS_S`). Only the pilot's own rounds on their lit target count.
- **Cleared** when the last is struck before the clock runs out. The time is from the first to the
  last; par 25 s (`DRILL_PAR_S`), first, second or third class as the course's.
- **Out:** the clock runs out on the tick after its deadline, and a strike after that ends it
  rather than counting. Strike the lit target to go again.
- **Kept twice, alike:** by the sector for the board, and by the client's world (`World::drill`)
  for the HUD. The client is fed its own `TargetHit` events and each snapshot's tick, so its clock
  runs out on the server's tick.

**On the screen:**
- The lit target pulses cyan for its pilot (only theirs: others' drills are theirs).
- The panel reads `THE DRILL   TARGET 7/20`, the time left and the time run.
- A chime for each target struck.
- The news `DRILL CLEARED · 0:21.3 · FIRST CLASS`, or `THE DRILL · TIME · 12 OF 20 STRUCK`.
- The `aim_hostile` hook aims at the lit target.

**The par is a pilot's, not a machine's.** A Leo on the gantry turning to each target on perfect aim
clears it in 3.8 s in the sim, and in 6.8 s over the wire with an agent's aim. The rest of a
pilot's time is finding the lit target and putting the crosshair on it: par is a second and a
quarter each. The trainer's machine cannon carries 400 rounds, so a pilot who sprays runs dry:
board again for a fresh one.

**Tests:**
- `hall`'s units: the clock starts, runs down, takes time back and runs out, and the order.
- `bc-sim/tests/interior.rs`: a trainer on the gantry clears it, the clock never near running out.
- `training_net` and `bc-server/tests/proving.rs`: through the sector and over the wire.
- The `inside` e2e: a pilot in a browser starts it and strikes its lit targets.

## Phase 5: live fire in the hall (built)

The colony's law says nothing fires inside. A blast chamber begs for shooting, and **the owner
made the hall the law's one exception** (the alternatives were keeping the law and teaching aim at
a range off the dock, or a simulator in the browser alone). It's `bc_sim::colony::hall`: closed
forms, which the server's interior sector and each pilot's prediction share.

- **Where weapons are free:** in the hall's room, under its roof (`hall::weapons_free`).
  - Anywhere else inside, the tick clears a suit's weapons' buttons before it runs (`Sim::colony_law`,
    from where the suit is as the tick starts). The pilot's prediction clears them the same way,
    so a suit's busy arms and lunges are predicted exactly.
  - Nothing is shown ready to fire outside (the own snapshot's ready bits), so the client predicts
    no shot there.
  - A Full Open Attack begun in the hall stops firing at its doors.
- **Training rounds** are the suit's own weapons: beams, guns, missiles, flame, blades. Inside
  the colony, though:
  - **They touch no suit.** Shots, missiles and flame pass suits by, and no blow lands
    (`queue_damage` does nothing inside), so a sabre fight in the hall is sparring: only a clash
    of blades parries.
  - **They stop at the hall's bounds:** its walls, its floor, its roof, and a curtain across its
    open blast doors. Nothing fired in the hall leaves it (`hall::shot_end`, `first_blocker`'s
    interior branch).
- **The targets:** twelve holograms 8 m across, hung in the hall's air by the tick
  (`hall::target`), all in its back half: its front 40 m, inside the blast doors, is the firing
  line (`hall::FIRING_LINE`).
  - Four stand still by the back wall, two low and two at a suit's head.
  - Four bob over the middle of the floor; four sweep across it high up, each on its own beat.
  - None comes within three radii of another.
  - Shots leave a suit's arm a few metres off its pilot's crosshair line (they fly parallel to
    it), so a target is a suit's shoulders across: a crosshair on its middle scores from the
    rifle's arm.
- **Scoring:** a round that meets a target stops there and scores. The server says so with a new
  event, `TargetHit` (extension sub-kind 3: target, shooter), sent to the shooter and to anyone
  within 5 km of the hall, and counts it (`SuitStats::targets`).
- **On the screen:**
  - the targets glow amber on the city's layer for anyone in the city, and flash white when
    struck;
  - inside the hall the panel reads `WEAPONS FREE   TARGETS n`, and the arms panel
    `WEAPONS FREE · THE BLAST HALL · TRAINING ROUNDS`;
  - beams and tracers end at the hall's walls, its doors and its targets;
  - the weapons' effects (beams, tracers, flashes, sparks, blasts, missiles) are drawn on the
    city's layer while the pilot flies inside. Pilots on foot don't see them yet.
- **Tests:**
  - `hall`'s units: weapons free only in the room; the targets clear of the walls and of each
    other; a round stops at a target or the bounds and never leaves the hall.
  - `bc-sim/tests/interior.rs`:
    - a Leo in the hall scores on a target, and counts it;
    - rounds pass through a suit in the way, which takes nothing, and score beyond it;
    - rounds fired at the blast doors stop there;
    - on the square before the doors, the trigger does nothing and nothing is shown ready.
  - Every existing golden is unchanged, the interior's included: outside the hall it runs exactly
    as it did. A new one, `HALL_GOLDEN` (native and wasm), has four suits firing at the targets in
    turn, one wandering out through the doors and back.
  - `no_alloc`: eight suits in the hall firing their beams, guns and missiles, among the 64 flying
    the interior, cost the tick nothing on the heap.
  - The `inside` e2e flies a suit down to the blast doors and in through them, holds the trigger
    on the nearest target (the `aim_hostile` hook aims at the hall's nearest target when there's
    no hostile), and sees its rounds score.

**Not yet:** the fire seen from on foot. (The drill with its clock, and its times on the board,
are built: above.)

## Phase 6: more courses

- **The hall's course:** one that starts and ends at the blast doors, for trainers (phase 3's plan).
- **A level ladder:** the same rings with less time, as X-Wing's eight levels to a badge and TIE
  Fighter's "after Level 8, the course stays the same, but you have five seconds less". The drill
  too: less on the clock, or less put back.
- **A course for each strip:** Canal's runs under its bridges, Gardens' along its terraces.
- **Speed rings** that boost, as X-Wing Alliance's did.
- **Several pilots racing at once:** the rings are everyone's already.
- **A ZERO trial** for the Wing Zero.

## Open questions

- **Live fire** (phase 5).
- **The real rules inside:** whether propellant should burn more slowly in air, or the course stay
  a lesson in it (`SUITS_INSIDE.md`'s open question about flight in air).
- **Rewards:** whether a first-class certificate pays (the Charter Board's bonus) or only shows.
  The server checks the times now (phase 4), so it could pay; today it only shows, on the board and
  the pilot's record.
- **The board's reach:** the day's best, and the best ever. A week's, or a season's, and whether a
  guest belongs on it.
- **Mandatory or not:** recommended, never required, as X-Wing's was.
