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
- **The station language everywhere, painted like the anime.** The clean panels of the Ring Deck and Bay
  Row (`RING.md`, "The station language") become the town's look: its towers, streets and halls. Each
  district takes its own paint and accent over the same panels, so it is not all white. Wear stays a hint.
  They are lit the painted way (`RING.md`, "The look": light in soft bands, shadows going violet and blue,
  rim light, haze in layers), which `?look=anime` already does to the panels (`shaders/anime.wgsl`). The
  owner chose that over plain panels, so the town is built under it, and it becomes the default look once
  the town's first showcase camera has been seen beside the plain one (V1).

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

*Built: `bc_sim::colony::town` (the rules) and `content::town` (the districts and the key places),
`TOWN_GOLDEN`, `TOWN_VERSION` 1, protocol 43.*

The cylinder's city taught the way: a town is a closed form of where you ask, allocation-free and
deterministic, its goldens checked native and wasm. The ring's town is one on the ring's deck
(`colony::deck::RING`). Nothing of it is stored or sent. Its coordinates are `x` across (the colony's own,
fore at the hub's side to aft) and `t` round the floor from Hub Gate (`town::town_t` of the deck's `s`).

**Round the ring:** twelve districts of 1,571 m (`content::town::DISTRICTS`), each twelve cells of 131 m.
A cross street (20 m) runs at every cell's edge and an avenue (40 m) between districts. A spoke's lift
comes down at the start of every even district, under its spoke, in the boulevard's middle. Its foot is
a portal the boulevard runs under: four piers, a lintel at 24 m and the shaft up to the ceiling. The four
blocks round it are its square, and the avenue between them is the square's too (the piers stand on it,
so cars cross the ring at the other avenues). Hub Gate is the first.

| District | Quarter | Kind |
|---|---|---|
| Charter Square (Hub Gate's lift at its start) | Charter | Civic halls |
| Exchange Row | Charter | Towers on podiums |
| Tower Hill (a lift) | Charter | Towers on podiums |
| Old Town | Charter | Low, narrow lots |
| Quayside (a lift) | Canal | Depots and sheds |
| Lock Town | Canal | Homes |
| The Basin (a lift) | Canal | Midtown |
| The Yards | Canal | Depots and sheds |
| Garden Gate (a lift) | Gardens | Parkland |
| Terraces | Gardens | Homes |
| Green Meridian (a lift) | Gardens | Midtown |
| The Building Site | Site | Frames and cranes |

**Across the ring** (`town::band_at`), fore to aft:

| Band | x, m |
|---|---|
| The rim walk, along the ring's face | −1,000 to −940 |
| The rim road | −940 to −910 |
| Seven columns of blocks | −910 to −32 |
| The boulevard, its tram on a 16 m median | −32 to 32 |
| Four columns | 32 to 560 |
| The canal, round the whole ring, railed, bridged at every street | 560 to 600 |
| Two columns of depots on its far quay | 600 to 910 |
| The rim road, and the rim walk | 910 to 1,000 |

**A block** (`town::BlockId`: its cell and its column, 144 × 13 of them) is its cell inset by its streets'
halves, a 6 m pavement along the boulevard and the rim roads, a 14 m quay along the canal. What stands on
it (`town::block_use`) is one of:

- **a lift's square;**
- **a key place's site,** left empty for its pieces: the Exchange floor and The Arrival facing each other
  across the boulevard just past Hub Gate's square, the Charter Board's hall just before it, and a station
  of the Watch on the boulevard halfway round every district (`town::watch_station`);
- **a park:** all of Garden Gate, and a pocket park on about one block in sixteen where people live;
- **the building site's frames:** corner columns, and a crane's mast on one block in three;
- **the buildings its district builds** (`town::each_block_box`):
  - towers on podiums for the money;
  - halls for Civic;
  - sheds for the depots;
  - two or four lots of homes, or of Old Town's low houses;
  - two lots of midtown blocks.

  Each is up to 8 boxes, 5 m for the ground floor and 4 m a storey over it. They're taller nearer a lift,
  up to 2.5 times for the towers, and none over 200 m (the ceiling is at 500).

**Walked and asked:**

- `deck::solid` asks `town::solid` for the ring's deck, so the walker and the server's pose check meet the
  town as they meet the Muster's buildings.
- `town::each_box` gives every box in a span round the ring, wrapping either side of Hub Gate, for drawing.
- `town::ground_at` says what the floor is: a block, a road, the median, pavement, water or the rim walk.
  That's what P1's traffic and W1's dispatch will route on.

The tests hold:

- the lifts under their spokes;
- the columns and cells tiling the ring;
- every building on its block and under the ceiling;
- the towers rising round the lifts;
- the squares and places on their blocks;
- the lifts clear of the road and the blocks;
- `solid` equal to every box it touches, wrapping round the ring;
- the town allocation-free (`no_alloc`);
- the golden, native and wasm;
- a walker reaching every block's kerb and a car reaching every Watch station's door from Hub Gate,
  flooded over the floor 4 m at a time.

`cargo run -q -p bc-sim --example town_map -- [m_per_px] [t0] [t1] > town.ppm` draws the plan from above.

**Still to come with the town's first walkers (P1, O1):**

- The deck's seam is still a bulkhead, at the avenue between Quayside and Lock Town. It opens when the
  walker wraps round the ring.
- The plaza's pose codec spans 4,096 m of `s` (`bc_proto::presence`): enough for the base's deck, not for
  the ring's 18.85 km. The first pose on the town's floor widens it, a wire change.

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

Built so far (protocol 47): the census (`colony::census`, `CENSUS_GOLDEN`), the parked cars at their kerbs
(`street::each_parked`, `PARKED_GOLDEN`) and routes on the town's roads (`colony::roads`, `ROUTE_GOLDEN`),
all closed forms; the taken set on the server (`bc-server`'s `street`): per district, fixed sets of the
people down or away as fares, the kerbs emptied, the traffic's cars taken and the cars left about, each
take checked against the pilot's pose and sent to the districts round each pilot when their version
moves.

Since (protocol 48), promotion and release: a car driven fast where people walk has those ahead of it
run, someone run down or a car taken from its driver has those round it run, and someone run down gets up
where they lay, stands a moment and walks on. Each is the server's from the closed form's own place on
that tick (`colony::crowd::Loose`: the closed form's ground, so nobody out of the crowd steps into a road
either), in a fixed pool of 512 (`bc-server`'s `crowd`), stepped ten times a second with the Watch's units,
and let go once settled with both them and their place in the crowd out of everyone's sight; full, the
settled one furthest from every pilot gives way. On the wire in tiers (`Update::Crowd`): within 130 m
whole, out to 320 m in four numbers each, and a count past that; a screen draws the crowd but these
where these are (`bc_client_core::crowd`), and its cars can run them down where they're drawn. `/status`
has the counts. Still to do: the pools in the interior sector's tick (with S1), and a fare set down
coming out of the car where it stopped.

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

Built so far (protocol 47), on the server outside the tick: heat is the server's (from what it sees and
what the client reports, which can only add), an incident queue and its news, and each station's cars
and hounds sent by road (`colony::watch::ANSWERS`, the server's `dispatch`): they hold a cordon for the
level's time and go home, chase a wanted pilot, and stop and fine them with no word from the client.
Clients draw the units the server sends (`units`). `/dev/incident` (with `--dev-hooks`) raises one.
An offence adds heat only if it's seen (the Watch's cars, or anyone within 30 m), and every one is told
(`Update::Watched`): who saw, or that nobody did. Patrols while nothing happens, and the units promoted
into the tick, are still to do.

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
- **The station language at three distances**, painted in bands (`bc::anime`): panels and their joints
  procedural in `station.wgsl` up close; a tower's faces as one quad each further off; past 1 km, blocks as
  boxes in the haze's painted layers.
- **Instancing** for everything there are many of: people, cars, lamps, props.
- **Numbers on screen**: `perf.rs`' readout with the town's counts (chunks, people, cars, draws), and a
  showcase camera on a crowded street that the browser suite times.

Built so far: cells built a part at a time under the frame's budget and cached once out of sight; three
tiers (near cells whole, a quad a face to about 1 km, boxes in the haze past it, the far ones merged by
four cells and by district: 171k triangles to 94k on the crowded camera); `town.wgsl`'s distant path;
the town's numbers in `?perf=1`; showcase camera 9, timed nightly (`perf`, its triangles and meshes
capped); and the painted look as the default (`?look=plain` for the old one).

Instancing turned out to be there already: the people and cars are pooled entities sharing one material,
each one's look in its `MeshTag`, so Bevy batches them by mesh (a frame of a stride, a car's body), and
lamps, trees and railings are in the cells' own meshes. `?perf=1` now says what they cost: their triangles
(not in the town's) and their batches. What grows with the crowd is the CPU's work per entity, not the
draws. The floating origin is still to do.

### 7. Start on foot (O)

When the town has a street to wake in: a new pilot wakes in the town (a flat of the colony's, or a bunk at
the nearest Watch station's hostel), and the bay is up the spoke lift. The objectives start on the street.
The suit stays exactly as it is.

Built (protocols 47 and on): a new pilot wakes in the town, a returning one where they left it; the first
objectives are the street's (TAKE A CAR, CARRY A FARE, GO UP TO YOUR SUIT) before the bay's; E takes what
the pilot looks at, else the nearest; and the spoke express (Q at a lift's door, R to pick) rides across
the hub to another square, the quick way round an 18.85 km ring without a car.

## Phases

| Phase | What | State |
|---|---|---|
| FR0 | This plan | written |
| F1 | The ring at 3 km, the hub on its own turn | done (protocol 42) |
| T1 | The town's plan as a closed form on the ring's deck, its goldens | done (protocol 43) |
| V1 | The town drawn in the station language, painted like the anime: chunks, a floating origin, level of detail; a showcase camera; the anime look made the default | done but the floating origin (protocol 47); people and cars were batched all along (below) |
| P1 | Ambient people and traffic on the town's plan; the census | done (protocol 47) |
| W1 | The Watch's stations, patrols and dispatch, with incidents from a test hook | done but patrols (protocol 47) |
| P2 | Promotion and release: the taken set, the pools in the tick, the wire's tiers | the taken set (protocol 47); promotion, release and the crowd's tiers on the wire, on the server off the tick (protocol 48) |
| O1 | A new pilot wakes in the town | done (protocol 47): and a returning one where they left it; the street's objectives first (TAKE A CAR, CARRY A FARE, GO UP TO YOUR SUIT) |
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
