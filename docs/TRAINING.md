# The Proving Ground: training inside the First Colony

The owner's ask (October 2026): somewhere inside the colony for pilots to train, "some sort of
workshop", or "a giant blast chamber": a tutorial area where a new pilot walks around freely,
then gets into a Gundam and learns to fly it. They also asked for the feel of X-Wing (1993) and
TIE Fighter (1994). What those games did, and what we take from them, is in `PEERS.md` ("X-Wing and
TIE Fighter"). This document is the design built from that ask. Phase 1, the course, is built.

## Status

| Phase | State |
|---|---|
| 1: the course. Rings in the colony's air from the inner gate down to a pad on Hub Gate's square, flown in your own suit, timed by your client | built |
| 2: the Blast Hall. A hall off Hub Gate's square, walked on foot and flown into, its desk the course's | built |
| 3: boarding in the hall. The Charter Board's trainer, flown out through the blast doors and docked back | planned |
| 4: the board. Times checked by the server, and the day's best on the hall's wall | planned |
| 5: live fire in the hall: the colony's law's one exception, the owner's call. Training rounds that touch no suit and score on its targets | built |
| 6: more courses, and a level ladder | planned |

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
  certificate (first, second or third class against the par), on the pilot's record once the
  server checks times (phase 4).
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
   crouch, climb the gantry's stairs. At the instructor's counter, E opens the board: the
   course's map, the par and the day's best times. This is X-Wing's Ready Room, with its
   viewscreen.
4. **Board the trainer** at the gantry's hatch. It's a Leo of the Board's, not the pilot's own:
   nothing of theirs is at stake, it carries nothing, and it can't be lost inside, where nothing
   fires and nothing strikes.
5. **Fly it.**
   - Inside the hall: hover, set down on its floor pad with the grip, walk it, lift off.
   - Out through the blast doors over the square, and round the course.
   - Back in through the doors to dock in the gantry, then on foot again.
6. **The certificate**, and on into the chain: the bay, the launch tunnel and space.

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

**The run** (`bc_client_core::course::Run`) is the pilot's own client's:
- **The clock starts** when the suit flies through the start ring the right way.
- **The rings count in order.** A ring out of order counts for nothing.
- **It finishes** when the suit stands on the pad, on the city, after every ring.
- **Starting over:** flying the start ring again starts the clock again.
- **Lapsing:** two minutes without a ring lets the run lapse.
- **Timing** is on the suit's own clock, the prediction's ticks, with the crossing worked out to a
  fraction of a tick. A frame rate can't buy a tenth.
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
- `bc_client_core::course`'s units:
  - the run counts rings in order, starts over at the start ring, lapses, ignores a jump, and
    reads its clock;
  - **a Leo flies the whole course in the server's interior simulation**: launched at the inner
    gate under the anime rules, it threads every ring, lands on the pad with the grip, and earns
    second class.

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

**Not yet:** the trainer's gantry and its floor pad (phase 3), and floor markings that teach
running, jumping and crouching.

## Phase 3: boarding in the hall

- **The wire:** `Request::Launch { into: Proving }`, a serde default as `into: Colony` was. The
  session hands the pilot's slot to the interior sector (Welcome sector 2) with a trainer: a Leo
  spawned at the hall's gantry. It isn't drawn from the pilot's hangar, their stores are
  untouched, and arcade pilots and guests can fly it too.
- **Docking it back** in the gantry's ring returns the pilot on foot to the hall, not to the bay.
- **The course's start moves to the blast doors** (a second course, "the hall's", keeps phase 1's
  for suits coming in from the bays).
- **Tests:** a server test boards in the hall, flies out through the doors and docks back; an e2e
  boards and launches.

## Phase 4: the board

- **The server checks the times.** Each session already knows where its pilot's suit is
  (`Metrics::pilots[slot].pos`, written by the sector every tick). The session's 100 ms tick runs
  the same `Run` on it, so `Run` moves into `bc_sim` beside the rings.
  - At 120 m/s a sample is 12 m apart, which the segment test handles.
  - The pad needs the suit's footing in `Metrics` as well.
  - All of it is off the hot path.
- **The board:** the day's best times on the hall's back wall and at its counter, like X-Wing's
  high-score table. A signed-in pilot's best and certificate go on their record (`pilots.rs`).

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

**Not yet:** a drill with a clock (X-Wing's Maze added time per target; TIE Fighter's simulator took
two seconds off), the targets' scores on the board (phase 4), and the fire seen from on foot.

## Phase 6: more courses

- **A level ladder:** the same rings with less time, as X-Wing's eight levels to a badge and TIE
  Fighter's "after Level 8, the course stays the same, but you have five seconds less".
- **A course for each strip:** Canal's runs under its bridges, Gardens' along its terraces.
- **Speed rings** that boost, as X-Wing Alliance's did.
- **Several pilots racing at once:** the rings are everyone's already.
- **A ZERO trial** for the Wing Zero.

## Open questions

- **Live fire** (phase 5).
- **The real rules inside:** whether propellant should burn more slowly in air, or the course stay
  a lesson in it (`SUITS_INSIDE.md`'s open question about flight in air).
- **Rewards:** whether a first-class certificate pays (the Charter Board's bonus) or only shows.
  Only showing until the server checks times (phase 4) is the safe start: anything the client
  decides can be forged.
- **Mandatory or not:** recommended, never required, as X-Wing's was.
