# Suits inside the First Colony (Milestone 5, phase 6): the design

The owner's call (`COLONY.md`): mobile suits can go inside the colony, and by the colony's law their
weapons are safe there. This is the design it was built from.

**Built:** the interior sector (`WorldKind::Interior`, `sector-1`), its pull, Coriolis and air,
the hull, the caps and the city's boxes (`bc_sim::colony::interior`), weapons safe, the inner
gate (in, and docking back out), the Welcome moving a pilot between sectors, the client flying and
drawing suits among the city's buildings with the gate's ring and marker, and the tests (unit,
`INTERIOR_GOLDEN` native and wasm, `no_alloc`, `bc-server/tests/inside.rs`). **Not yet:**
`Body::City` (suits stand by resting on what's under them, flight assist holding them, rather
than walking), pilots on foot seeing suits (spectator slots), people, trams and cars drawn for
pilots in suits, the axis port's handoff, and the building site's work for suits.

## What it has to be

- A pilot flies their own suit from their bay into the colony, flies or walks it there among the
  city's towers, and flies it back to the bay. Other pilots, on foot or in suits, see it where it is.
- **Weapons safe.** Inside, nothing fires and nothing strikes: no shot, beam, missile, blade or
  special leaves a suit. That's the colony's law, and the simulation's too, so there's nothing to
  check against the city.
- **The hot path stays as it is.** The sector tick allocates nothing and locks nothing
  (`CLAUDE.md`). Space flight stays bit for bit what it is today (the determinism goldens).
- **The city stays still.** Inside, the colony's own frame is the ground, and the city doesn't move
  in it. Suits must live in that frame: in the sector's frame the city turns at up to 177 m/s, the
  case `ARCHITECTURE.md`'s "Bodies and frames" table rules out for walking.

## The interior is its own sector

A second sector, `sector-1` on its own thread, its rings and egress its own: the scaling path's
first step (`ARCHITECTURE.md`, "Scaling path": nothing in a sector is shared with another). Its
coordinates are the colony's own frame (`bc_sim::colony::frame`: `x` the axis, the hull at
`|(y, z)|` = 3,200 m, turning with the colony), so the city stands still in it and every closed
form of `bc_sim::colony` works there unchanged.

- **`Sim` gets a `WorldKind`** (`Space`, `Interior`), fixed at construction. Everything that differs
  is a `match` on it outside the per-entity loops, so `Space` runs the code it runs today.
- **What's solid inside** is the colony's inside, not its outside: the hull (from within), the end
  caps, the city's buildings and platforms (`colony::city::solid`, `each_solid`), Hub Gate's
  terminal and the spire's inner end. A new `world::interior_constrain` keeps a suit's capsule
  inside the hull and out of the city's boxes; `interior_sweep` replaces `colony_sweep` for the
  (non-existent) shots and for the suits' own motion. The city's boxes are queried by a suit's
  swept AABB, which is O(boxes near it), not O(city).
- **Gravity and the spin's pull.** Inside the turning frame a free suit feels the centrifugal pull
  `ω² r` (1 g at the floor), and Coriolis `−2 ω × v`. Both go into `flight::integrate` for
  `Interior` only: a pure function of state, deterministic (libm), and allocation-free. A suit
  standing on the ground uses `ground::move_step` with a new `Body::City` whose surface is
  `colony::city`'s (its probe is the box under the feet), and gravity from `colony::frame::gravity`
  in place of grip.
- **Air.** The interior has air: drag on a suit (`½ ρ C A v²`, with `ρ` 1.2 kg/m³) goes into
  `integrate` for `Interior`, which caps speeds near the ground at about 120 m/s and makes hovering
  cost propellant (`tuning`'s thrust against its weight). Whether propellant burns faster in air
  is an open question (below).

## Weapons safe

In an `Interior` sim the weapon steps don't run: `fire_control`, `melee`, `missiles` and the
special are skipped, and the buttons that start them (FIRE_PRIMARY, FIRE_SECONDARY, MELEE,
SPECIAL) are cleared from every input on its way into the tick (`InputCmd::neutral`'s mask, kept
for the interior). The HUD shows WEAPONS SAFE in place of the weapons' readouts. Mobile Dolls
never enter: the interior's spawn list has none. ZERO stays (it's a predictive interface, not a
weapon); its rollouts run against the interior's world.

## Getting in and out

- **From the bay**: a new `Request::Launch { into: Colony }`. The bay's catapult throws the suit up
  an inner tunnel instead of the outer one: the session hands the pilot's slot to `sector-1`
  (the Welcome's sector becomes 2), and the suit appears at an inner launch gate near the axis at
  the docking hub's end (x ≈ −15,800, r ≈ 300 m), slow, nose down the colony.
- **Back**: at rest inside the inner gate's ring of lights, Enter docks, as the outer dock does
  today (`Request::Dock`), back to the bay.
- **Later, through the axis port**: a suit flying out of the interior along the axis hands off to
  `sector-0` at the port, converting its state between frames (`colony_to_sector` and the spin's
  angular velocity). This is the scaling path's handoff, and waits on it.

## Who sees whom

- **Suits** are the interior sector's entities, replicated by its own snapshots to the pilots in
  it, exactly as in space.
- **Pilots on foot** (the plaza) aren't in either sector. They see suits through **spectator
  slots**: a session in the city subscribes to `sector-1`'s snapshots for the suits near it
  (interest by position, as today), drawing them interpolated; it sends no input. The snapshot's
  own-suit section is absent for a spectator.
- **Suits see people, trams and cars** as the plaza relays them, ghosts as on foot: nothing
  touches a suit. A suit landing on a car or a crowd simply stands among them.

## Wire

- Welcome sector 2 for the interior; `Request::Launch` gains `into` (serde default `Space`, so
  today's clients are unchanged); a spectator flag on the snapshot header (one of its free bits).
- A protocol bump, and new determinism goldens for an `Interior` scenario (native and wasm).

## Verification

- `bc-sim` unit tests: `interior_constrain` keeps capsules inside the hull and out of every box
  (dense sampling, as `colony_sweep_matches_dense_sampling`); a dropped suit falls at 1 g at the
  floor and less higher up; Coriolis deflects a dropped suit to −spin; drag caps speed; standing on
  a roof with `Body::City`.
- `no_alloc`: 64 suits flying the interior for 1,000 ticks, 0 heap operations.
- Determinism: an interior scenario hashes the same native and wasm; every existing golden is
  unchanged (space untouched).
- `bc-server/tests/interior.rs`: launch into the colony, fly, dock back; fire inputs do nothing.
- e2e: a pilot launches into the colony from the bay, flies over the avenue, docks back; a second
  pilot on foot at Hub Gate sees the suit.

## Open questions

- **Why bring a suit inside?** The candidate: the building site. Colony projects (the Charter
  Board's great works, `DESIGN.md`'s roadmap) are built by suits carrying girders and machinery in
  the interior's gravity, paid from the project's funds. Without that, a suit inside is a
  sightseer.
- **Flight in air**: propellant use when hovering, and whether thrusters are allowed near people
  (a downwash rule?).
- **How many suits a sector**: the interior shares nothing with space, so its budget is its own;
  64 is the guess.
- **Sleeping inside**: a pilot who leaves with their suit inside; parked on a roof, or sent back to
  the bay.
