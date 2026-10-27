# The First Colony as a ring: the plan of record

> A wheel a kilometre across, turning once a minute. Its floor rises ahead of you into a lattice of light, and
> your bay opens straight onto the base.

The owner's ask (October 2026): "it's time to redo the space colony. The inside was a nice test, but I want the
Gundam Wing style. It's boring as hell to look at the one we've got now. And when we do it we need a military
area, a base: right now at the start of the game I'm running all over the place, it takes too long. Then let's
take our time with each building, so they have interesting interiors." Their two references are the 1990s TV
series' colonies:
- **Outside:** angular slab panels round a spindle, ruled with lines of small lights on black.
- **Inside:** a city on a floor that bends up into the distance under a lattice of frames, light falling through it
  in shafts, and the far end lost in bright blue haze.

These are influences for a look and a shape. Every design and name stays the game's own (`STORY.md`, "Voice and
naming").

This document replaces the cylinder `COLONY.md` built. `COLONY.md`, `COLONY_LOOK.md`, `TRAINING.md`, `OPEN_CITY.md`
and `LIFE.md` stay as the record of what was built in it and what carries over; where they disagree with this one,
this one wins.

## Status

| Phase | State |
|---|---|
| R1.0: this plan | written |
| R1.1: the cylinder's inside closed by default (its code kept behind the server's `--colony` until it's cleared) | done; no rent runs meanwhile (`Homes::hold`) |
| R1.2, R1.3: the ring in space: its hull, hub, bays, spokes and louvres, in the sector and on every screen | done (protocol 40): `world`'s ring, `colony::hub`'s drum, spine, spokes and bays at 0.4 g, `colony::louvres`, the hub landmark, the field kept, the dock's approach, nav and the salvage routes round it, up as +Y, and a first exterior of slabs and lights (`bc-client`'s `colony.rs`) and the chart's |
| L1: the look test, painted anime on one camera, signed off by the owner | first pass behind `?look=anime`, waiting on the owner |
| R1.4: the outside dressed: slab panels, light grids, beacons | first pass: the ceiling's glass a pale sky blue the lattice reads against, the slabs kept clean, rows of lights ruling them round the rim |
| R2: Gates' building pieces in `bc_sim::pieces`; the decks; a walkable test building | built: `pieces` (lattice, sock, shape, support, check, parts, prefab), `colony::deck` and `foot::DeckGround`, `content::base`'s test hall walked in at its door and up its stairs (`tests/pieces.rs`, `PIECES_GOLDEN`), drawn by `bc_client_core::piece_mesh` and `bc-client`'s `base.rs` in the drum's inside with its own light (`?showcase=base`); walking it in the game comes with R3 |
| R1.5, R1.6: the cylinder's inside cleared (homes refunded) | to do |
| R3: the base opens: the bay's airlock onto Bay Row, the desks in its rooms | to do |
| R4 on: the base's buildings, one at a time; then the ring's town | to do |

## The owner's decisions

Asked in planning (October 2026), and answered:

- **A ring, a bit smaller,** "and then we could have more colonies being built, and that's the game." Floor radius
  1 km, 600 m wide, 1 g. More rings being built is the arc of the eras (`STORY.md`).
- **Painted anime, like the OVA.** This reverses `COLONY_LOOK.md`'s "physically lit, no toon shading". A look test on
  one camera comes before anything adopts it.
- **The base is up in the docking hub.** You wake in your bay, the early game needs no lift ride, and the town is a
  place you go later.
- **The cylinder is retired,** and the inside is closed until the base opens.
- **Buildings are built from Gates' pieces** (`AnthonE/Gates`, MIT: foundations, walls, doorways, floors, stairs and
  windows on a 3 m lattice, with Rust's support rules), "but tweaked so it's not all raw survival". We build with
  them first, one building at a time; players get the hammer later, on new colonies' building sites.

## The ring

The colony's frame has its origin at the ring's middle, with +X towards the sun (the spin axis, as before).
`world::COLONY_CENTER` moves to (−16,250, −4,200, 0), where the cylinder's bay ring was. That keeps the dock, the
lanes and the field where they are, and every deck coordinate inside the pose codec's ±16 km.

| Part | Numbers |
|---|---|
| The ring | Floor and hull at r 1,000 m; ceiling at r 700 (300 m up); \|x\| ≤ 300. 6.28 km round, 3.8 km² of floor. |
| Spin | One turn in 1,903 ticks (63.4 s): 1.0004 g at the floor, rim speed 99 m/s. |
| The ceiling | The window lattice the light falls through: frames across and along, glass between. |
| Sight | From the floor you see about 1.4 km before the floor rising ahead meets the ceiling: the tunnel of the reference, and the bound on what's ever drawn. |
| The hub drum | r 440, x −650 to −350. The bays' doors are on its forward face at r 400 and face −X, so a launch goes out along −X as from the cylinder's bay ring, and the ring is behind it. The deck hatch is at r < 40. |
| The base deck | Floor at r 400 (0.4 g), x −602 to −350: 252 m wide and 2.5 km round. Bay Row runs behind the bays, the one way out of every pilot's airlock. |
| Spine and spokes | The spine r 100, from the drum to the ring's far face; six spokes at x 0, r 100 to 700, each with a lift down to the town (later). |
| Light | Louvres round the spine, between the spokes, opening with `colony::time::day().mirror_beta`: the 48-minute day and its colour script unchanged. |
| The dock | On the axis 800 m off the hub's mouth: (−17,700, −4,200, 0), r 300 (450 m further out than the cylinder's, so a suit coming in at speed swings round it clear of the drum). The client's dock approach is its ring and 100 m, so a launch from the face doesn't dock straight back. |

**Physics.** To the space sector the ring is still a solid: a hollow cylinder, |x| ≤ 300 and 700 ≤ r ≤ 1,000, plus
the hub and spokes as landmark shapes. Hits come four ways (the outer cylinder, the inner one from the hole, either
face). Inside, `colony::spin_frame` keeps the cylinder's pull, Coriolis and drag, with the ring's ω.

**Space's "up"** was "away from the colony" for the default controls, the lock-on's floor and the HUD's altitude. A
1 km ring 16 km off has no sides worth standing on, so up is +Y everywhere in space (the field gets a flat floor), and
`LOCKON_GOLDEN` moves with it.

**The field doesn't move.** `Field::keep_out` keeps the cylinder's old box as its own numbers, so the rocks, Fornax and
every hiding place stay where pilots know them. A volume of empty space is left where the cylinder was. Filling it is
a later choice, and it would move the field's goldens.

## The look: painted, like the OVA

The colony and everything in it are drawn like a 1990s anime's painted backgrounds:

- **Light in soft bands.** The key light falls in two or three value bands with painted, slightly soft edges, not a
  smooth Lambert falloff.
- **Shadows with colour.** Shade goes violet and blue, never grey or black.
- **Rim light.** It catches edges against the haze.
- **Haze in layers.** Distance and height wash things out in steps of a lighter, cooler colour, as an anime
  background's planes are painted.
- **The vanishing point glows.** Inside the ring, where the floor bends up into the ceiling, it burns white-blue.
- **Light shafts** fall through the ceiling's lattice.
- **Colour by the hour.** `city_hour.rs`'s colour script keeps its shape: pale noon, amber golden hour, magenta dusk,
  deep blue night with warm windows. It is pushed more saturated.
- **Outside, black and lights.** Slab panels in pale blue-grey with dark seams, lines of tiny white and green lights
  along every seam and edge, red beacons, the louvres' glow.
- **Grime kept to suggestion.** Wear is painted as a few strokes, not Rust's realism.

The suits keep their own look (`docs/UI.md`, `bc-model`). Painting them is a separate question for later.

**The look test (L1)** is behind `?look=anime`: `hull.wgsl`'s `paint_bands` repaints what the hull material lit
in three bands with soft edges (violet shadow, cool half-light, warm light), a highlight stroke and a cool rim, on
everything plated with it but the suits (the colony outside, its louvres, the bays, the stations, the base). Its
shots (`BC_GFX_EXTRA="look=anime" scripts/e2e.sh gfx webgl2`) go side by side with the references and the plain
look, and only once the owner has seen them does anything adopt it.

## The base: the Muster

The Charter Board's yard for its pilots, in the hub drum on the base deck (working name *the Muster*: where the
Arrivals muster). Every stop of a new pilot's first hour is within about 150 m of their bay:

| Stop | What it does | Built from |
|---|---|---|
| Bay Row | Every bay's airlock opens onto it: other pilots, the base's map | pieces |
| The briefing room | The Charter Board's pilot desk: the certificate's rifle paid out, contracts, patrols, the Most Wanted board | pieces |
| The armoury | Fits weapons straight to the suit in your bay | pieces |
| The mess | Meals (`LIFE.md`) | pieces |
| The Proving Ground | The Blast Hall: trainers, the drill, the board (`TRAINING.md`), at 0.4 g | its own suit-scale shell |
| The militia's hangar | A building site until its great work is done (`bc_econ::charter`) | pieces, as it's built |

The first hour becomes: wake in the bay, out onto Bay Row, the briefing room, the Proving Ground, back to the
briefing room for the rifle, the armoury, back to the bay, launch. A test sums the routes between those stops and
keeps them under 600 m. Today's trek is 3.8 km.

The Exchange floor and The Arrival go down the spoke lift to the ring's town, reached once a pilot has a reason to go.

## Buildings from pieces

`bc_sim::pieces` ports the lattice path of Gates' building system, credited in its modules:
- `lattice` and `sock`: exact points on a lattice of squares and triangles, and the footprints on it;
- the 21 shapes, at 3 m cells and storeys;
- `support`: what bears what;
- `check`: Gates' `check_site`, keeping only the spot, overlap and support rules;
- `stairs`: its circulation, plus stepped treads under our walker's 0.45 m step.

Gates' runtime store, claim, upkeep and decay are not taken until players build.

- **On a deck.** A building sits in the deck's unrolled coordinates (`colony::deck::DeckPos {deck, x, s, h}`, walls
  plumb to the axis, as the cylinder's city stood), bent onto the deck only when drawn. At r 400 a 30 m building's
  floor curves 0.28 m.
- **Boxes first.** Frames turn in quarter turns, so every solid is a box. `deck::solid(deck, min, max)` keeps the
  contract `foot::Walker` and the server's pose check already use. Triangles and 30° walls come later as exact
  prisms.
- **Authored as data.** A building is a `const` prefab of placed pieces and its rooms (`RoomDef`: the place, its
  cells, its counter), baked to solids at compile time, so nothing allocates. A test places each piece in order
  through `check`, holds it supported, and walks a walker to every counter and up and down every flight.
- **Drawn as itself.** `bc-client-core`'s `piece_mesh` builds each piece from the same parts table the solids come
  from, then dresses it inside its own volume, as Gates' `piece_dress` does. The dress is white and grey colony
  panels, military olive and hazard trim, grating, ribs and lit strips. Props (consoles, lockers, racks, bunks,
  gantries) come from a kit in `bc-model`.

## What carries over, and what goes

**Carries over:**
- `foot::Walker` and its `Solid`;
- `colony::time` (the day);
- the presence framing (its `strip` becomes a `deck`);
- the plaza's checks;
- the figures and their gait;
- cars and Markabs' meshes;
- `colony_sky.wgsl` (re-pointed at the ring);
- the interior sector's plumbing and its spin-frame physics;
- the Blast Hall's drill and trainers;
- `bc_econ::homes`' lock, crew and tenure (for the ring's homes, later).

**Goes** (tied to the cylinder's 32 km grid and three strips):
- the city's rules, traffic, walkers, trams, pools, furniture, home plans and the course;
- `content::city`;
- the city's meshes, atlas and shaders;
- the `colony`, `inside` and `home` browser suites;
- the `CITY_`, `HOMES_`, `TRAFFIC_`, `WALKERS_` and `INTERIOR_` goldens.

**Homes held when the cylinder goes are refunded in full:**
- a lease's bank;
- a deed's price;
- what the furniture cost;
- the contents, back to the pilot's account, collected at their bay.

From R1.1 to then, no rent is charged and nothing lapses.

## Defaults taken in planning

The owner may overturn any of these:

| Question | Default |
|---|---|
| Space's up | +Y everywhere |
| The field | Kept as it is |
| The bays | 99 door positions on the drum's face; one shared Bay Row behind them |
| Jump at 0.4 g | Scaled by gravity, so a jump is as high as in the bay today (nobody vaults a 1.5 m half wall) |
| Homes | Refunded in full |

## Phases

1. **R1.1:** survival's inside closed by default (the server's `--colony` opens it; `--no-colony` kept and
   meaningless). `dev.sh`'s `BC_COLONY=1` opens it. The browser suites that need it pass `--colony`. Rent is held.
2. **R1.2:** the old interior fenced. `colony::frame` keeps the cylinder's own numbers, so nothing inside reads
   `world`. Every golden stays identical.
3. **R1.3a:** space's questions about the colony asked through functions (`world::colony_clearance`,
   `colony_normal`, `colony_reach`, `up_at`, `altitude`). No behaviour change.
4. **R1.3b:** the ring in space (a protocol bump):
   - `world`'s ring;
   - `hub` (drum, spine, spokes, bays at r 400);
   - `louvres`, replacing `mirrors`;
   - the hub landmark;
   - the field frozen;
   - the dock's approach;
   - nav;
   - a placeholder ring on every screen, the chart and the showcase's cameras.
5. **L1** (the look test) and **R1.4** (the outside dressed).
6. **R2:** the pieces, the decks, a test prefab, `piece_mesh`, and `?showcase=base` to walk it.
7. **R1.5, R1.6:** the generic parts of the old inside split out, homes refunded, the rest deleted.
8. **R3:** the base opens (a protocol bump):
   - presence on decks;
   - the plaza on the base deck;
   - the bay's airlock onto Bay Row;
   - the desks in the base's rooms;
   - the objectives reordered;
   - a `base` browser suite.
9. **R4 on:** one building at a time, each with its prefab, props, desk, signs, lamps, a showcase camera, an e2e step
   and screenshots before it merges. In order: Bay Row, the briefing room, the armoury, the mess, the Proving Ground,
   the militia's hangar. Then the ring's town, each step its own plan when it's reached:
   - the spoke lift;
   - the ring's shell (floor, lattice ceiling, light shafts, haze along the tunnel);
   - the first quarter (the Exchange floor, The Arrival, the Charter Board's hall);
   - people and a tram loop as closed forms;
   - homes;
   - suits inside the ring;
   - the second ring's building site, with the hammer for players.

## Verification

- **Every step:**
  - `scripts/ci.sh`;
  - each golden that moves named in its commit, native and wasm agreeing;
  - the hot-path rules (`CLAUDE.md`) kept: `bc_sim::pieces` and `colony::deck` allocate nothing and use `bc_sim::math`.
- **The ring in space:**
  - `scripts/e2e.sh hangar` (launch from the new doors and dock again);
  - `frames`, `lockon` and `chart`;
  - `gfx --grep colony`, shot against the outside reference.
- **The look:** the showcase's exterior and bay cameras at noon, the golden hour and night, beside the references.
- **The base:**
  - the `base` suite (two pilots meet on Bay Row and walk to the desks);
  - a playtester on the first ten minutes;
  - the first hour's route under 600 m.
