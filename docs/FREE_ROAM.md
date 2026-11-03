# Free roam: a town at GTA's scale

> You wake in a flat on the ring's floor. The street outside rises ahead of you into the haze, past
> towers in clean grey panels, and the traffic runs on round the ring. Your suit is in a bay up the
> spoke. You'll get to it.

The owner's ask (November 2026): "let's start making all this more free roam. I don't know if I even
care for you to get a mobile suit pretty quick. It might be better to work on the scale of a Grand Theft
Auto game. And then the looks: we had a newer look with some sort of white panels, they look pretty
good ... it doesn't have to be white but there's a certain aesthetic. So it's more looks and performance
with scale. Eventually we need lots of NPCs and all that, and definitely some police force that's going
to come in and do something. But not so much game mechanics: more the infra for all this."

This is the plan of record for that turn. `RING.md` stays the plan for the ring and the base; where the
two disagree, this one wins. `OPEN_CITY.md` (homes, the street in the tick, heat and the Watch),
`LIFE.md` (seats, meals, minds by level of detail) and `SCALE.md` (what stops the world growing) are the
designs it builds the infrastructure for. The mechanics in them come after the infrastructure, not
before.

## The owner's decisions

Asked in planning (November 2026), and answered:

- **A bigger ring.** The floor goes from a 1 km radius to 3 km, and from 600 m wide to 2 km: 37.7 km² of
  floor against the old ring's 3.8 (GTA V's whole map is about 75 to 80 km², most of it hills and sea).
  It still turns for 1 g at the floor, once every 109.9 s.
- **A new pilot starts on foot in the town.** The bay and the suit come later, the way GTA holds back the
  jets. The suit is still the game's reason, but the first hour is the street.
- **The station language everywhere.** The clean panels of the Ring Deck and Bay Row (`RING.md`, "The
  station language") become the town's look: its towers, streets and halls. Each district takes its own
  paint and accent over the same panels, so it is not all white. Wear stays a hint.

## Defaults taken in planning

The owner may overturn any of these:

| Question | Default |
|---|---|
| The ceiling | r 2,500 m: 500 m over the floor |
| The hub | Turns on its own bearing at the spine, as fast as before (63.4 s a turn): the bays and the Muster keep their 0.4 g, and the drum, the bays, the dock and the lanes stay where they are in the world. Only the ring and its spokes turn at the ring's slower rate |
| Where the ring sits | Its forward face 50 m behind the drum's back, as before: its middle moves 700 m along +X |
| The field | Unchanged: the rocks, Fornax and the hiding places stay where pilots know them |
| Gravity in the hub | 0.4 g, unchanged (it would be 0.13 g if the drum turned with the ring) |

## What GTA's scale means here

| | Numbers |
|---|---|
| The floor | 18.85 km round, 2 km wide: 37.7 km² |
| The rim | Moves at 171 m/s: still far too fast to land on from space |
| Sight | From the floor, a line grazes the ceiling 1.66 km ahead and meets the rising floor 3.3 km ahead. Nothing farther round the ring is ever drawn: at most about a third of the floor (6.6 km of 18.85, by 2 km) |
| A car at 60 km/h | Round the ring in 19 minutes; across it in 2 |
| On foot | Round it in nearly 4 hours at a walk; across in 25 minutes |

That sight line is the budget for everything below: the client draws what lies within 3.3 km round the
ring and builds the rest's level of detail by distance, and the server's interest management never sends
anything a pilot couldn't see.

## The infrastructure

Seven pieces, each paying on its own, built in roughly this order. None is a game mechanic; each is what the
mechanics in `OPEN_CITY.md` and `LIFE.md` need under them.

### 1. The ring at its new size (F1)

`bc_sim::world`'s ring and `colony::hub`'s drum, spine and spokes, with the hub's own turn
(`colony::hub::HUB_SPIN_PERIOD_TICKS`). The hub's drum is a body of revolution, so to suits it is the same
solid at either turn; it stays the docking hub's landmark, on the drum's turn, so that a suit holding
its face turns with it, and the spine and spokes become a landmark of their own (`content::landmarks::SPOKES`)
turning with the ring. The louvres, the exterior's slabs and lamps, and the showcase's colony cameras
scale with it. Protocol 42.

### 2. The town's plan as a closed form (T1)

The cylinder's city taught the way: a town is a closed form of where you ask (`colony::city` was 32 km of
it, allocation-free and deterministic, its goldens checked native and wasm). The ring's town is a new
closed form on its deck (`colony::deck::RING`), parametric in the deck's size so a later change of the
ring's numbers moves it without rewriting it:

- **The ring road** round the floor, on both edges, and **avenues** round it between districts.
- **Districts** round the ring, twelve of 30° (1.57 km each), each with its kind, its paint and its accent
  (the station language's per-district colour), a spoke's lift at every other boundary.
- **Blocks and lots** on a grid in each district, the cross streets on it, the heights by district and by
  how far from a lift.
- **Solids** for the walker and the car through `deck::solid`, as the base's buildings already are, so
  nothing new walks it.
- **Hand-built places** (the Exchange floor, The Arrival, the Watch's station, the homes' blocks) are Gates'
  pieces (`bc_sim::pieces`) placed on reserved lots, as the Muster's buildings are.

A `TOWN_GOLDEN` like `CITY_GOLDEN`, and a version (`TOWN_VERSION`) that moves with the protocol. The
plaza's pose codec spans 4,096 m of `s` (`bc_proto::presence`), enough for the base's deck but not the
ring's 18.85 km round, so the first pose on the town's floor widens it: a wire change with T1.

### 3. People and traffic in tiers (P)

`LIFE.md` 7's minds by level of detail, made into the population's infrastructure. The point is that the
crowd costs nothing until somebody touches it:

| Tier | What it is | Cost |
|---|---|---|
| **Ambient** | Closed forms of the tick and the place, as the cylinder's `walkers` and `traffic` were: lines and slots on the town's plan, the same on every screen, never stored or sent | Nothing on the server; the client draws the nearest under a budget, instanced |
| **Promoted** | When something acts on an ambient person or car (hit, shot at, taken, scared off, spoken to), the server makes it an entity of the interior sector's tick at the closed form's state on that tick, and marks its slot taken so every client stops drawing the ghost | A slot in a pool sized at start-up; a brain from `bc_sim::ai`; replicated like any other entity |
| **Released** | A promoted person out of everyone's sight and settled goes back: the slot's mark is cleared on the next tick its closed form is out of view too | Frees the pool slot |
| **Players** | Entities always (`OPEN_CITY.md`'s S1) | As now |

What it needs, in order:

- **A census** (`LIFE.md` 7.2): every ambient slot hashes to a stable identity (a name, a home, a job), a
  closed form.
- **The taken set:** a fixed bit set of the slots promoted, on the server; on the wire as a compact list
  per district, sent when it changes (a version per district, as the homes' register is).
- **Promotion and release** in the interior sector's tick, from fixed pools (`no_alloc`, as every tick).
- **Interest tiers on the wire** (`SCALE.md` C): near entities whole and delta-coded; mid as position and
  heading in a few bits; far as counts.
- **A budget the client keeps**: so many people and cars drawn at full detail, so many as instanced
  impostors, the rest not at all; pooled figures, as `life` pools its people.

### 4. The Watch as infrastructure (W)

The police force, before any heat: the dispatch, the units and their movement, so that the moment X1 adds
heat, something already comes.

- **Stations** on reserved lots of the town, one or more a district, each with its units' pool: hounds (the
  Sirius drones), cars, and later the Watch's suits (`OPEN_CITY.md`'s Aldebaran, Regulus and Fomalhaut).
- **Patrols are ambient** while nothing happens: Watch cars on the traffic's rings and hounds on fixed
  beats, closed forms like the rest.
- **Incidents** are a fixed queue on the server: a place, a time, a level (`OPEN_CITY.md`'s 0 to 5) and
  what raised it. For now the only way in is a test hook and an admin command.
- **Dispatch** takes the nearest free units by the town's own roads (a route on the plan's graph, a closed
  form), promotes them (tier 3, above) and sends them. They drive there, hold a cordon round the place for
  as long as the level says, and go back to patrol: released.
- **Escalation is data:** what each level sends, how fast, from where (a table like the heat table), so the
  mechanics can tune it later without code.
- **Seen by everyone:** an incident is news, as `OPEN_CITY.md`'s X2 wants (the radio, the map), on the
  control stream.

### 5. Many sectors for one town (D)

One interior sector holds the town at first (a sector is a 65 km cube; the ring fits). When the crowd
outgrows one thread, the town splits by district along spoke lines: each district its own sector on the
same clock (`spawn_follower` already makes one), with a band either side of a border replicated read-only
to the neighbour, and a promoted person or a pilot changing owner on a tick both agree on. The town's plan
is one closed form, so neighbours can't disagree about it. This is `SCALE.md`'s D, done for the town first.

### 6. A client that draws a town (V)

`SCALE.md`'s E, for the town:

- **A floating origin**: `RenderOrigin` (the cylinder city's f64 origin, recentred every kilometre) for
  the ring and the town.
- **Build by distance and frame budget**: the town's blocks as chunks on a quadtree within the 3.3 km sight
  line, built a few a frame, as `city.rs` built the cylinder's (1.5 to 4 ms a frame, 1,400 cached).
- **The station language at three distances**: panels and their joints procedural in `station.wgsl` up
  close; a tower's faces as one quad each further off; past 1 km, blocks as boxes in the haze.
- **Instancing** for everything there are many of: people, cars, lamps, props.
- **Numbers on screen**: `perf.rs`' readout with the town's counts (chunks, people, cars, draws), and a
  showcase camera on a crowded street that the browser suite times.

### 7. Start on foot (O)

When the town has a street to wake in: a new pilot wakes in the town (a flat of the colony's, or a bunk at
the nearest Watch station's hostel), and the bay is up the spoke lift. The objectives start on the street.
The suit stays exactly as it is.

## Phases

| Phase | What | State |
|---|---|---|
| FR0 | This plan | written |
| F1 | The ring at 3 km, the hub on its own turn | done (protocol 42) |
| T1 | The town's plan as a closed form on the ring's deck, its goldens | to do |
| V1 | The town drawn in the station language: chunks, a floating origin, level of detail; a showcase camera | to do |
| P1 | Ambient people and traffic on the town's plan; the census | to do |
| W1 | The Watch's stations, patrols and dispatch, with incidents from a test hook | to do |
| P2 | Promotion and release: the taken set, the pools in the tick, the wire's tiers | to do |
| O1 | A new pilot wakes in the town | to do |
| D1 | The town split into district sectors | when one sector isn't enough |

Each phase follows `CLAUDE.md`'s rules: the tick allocation-free and lock-free, closed forms deterministic
with their goldens equal native and wasm, a protocol bump where the wire or the world's numbers move.

## What this changes in `RING.md`

- **The ring's numbers** (its table, "Sight", the spin, the rim's speed) are this document's.
- **The order of R4:** the town comes before the base's buildings. The Muster stays where it is, at 0.4 g,
  for when a pilot first goes up to their suit.
- **The first hour** starts on the street, not on Bay Row; `RING.md`'s 600 m route is the base's, for later.

## Verification

- **Every phase:** `scripts/ci.sh`; the goldens that move named in the commit.
- **F1:** the space tests round the ring (the hull, nav's routes, salvage's routes clear of it, the dock,
  launches from the bays at the hub's turn); `scripts/e2e.sh hangar`, `frames`, `chart`; `gfx --grep colony`.
- **T1 and on:** a walker and a car reaching every district by its roads; nothing of the plan overlapping;
  the goldens.
- **Performance:** the client's frame time on the crowded-street camera, and `cargo bench -p bc-sector`
  with the town's population promoted, before and after each of P2, W1 and D1.
