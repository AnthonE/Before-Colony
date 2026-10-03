# Suits inside the First Colony (Milestone 5, phase 6): the design

The owner's call (`COLONY.md`): mobile suits can go inside the colony, and by the colony's law their
weapons are safe there. This is the design it was built from.

**Built:** the interior sector (`WorldKind::Interior`, `sector-1`), its pull, Coriolis and air,
the hull, the caps and the city's boxes (`bc_sim::colony::interior`), weapons safe, the inner
gate (in, and docking back out), the Welcome moving a pilot between sectors, the client flying and
drawing suits among the city's buildings with the gate's ring and marker; one clock for the
colony (the interior keeps space's tick: `bc_sector::spawn_follower`); pilots on foot watching
the suits near them (spectator slots, below); people, their cars and the trams drawn for pilots
in suits; suits landing on the city with the grip armed and walking it (`Body::City`, below);
and the tests (unit, `INTERIOR_GOLDEN` native and wasm, `no_alloc`,
`bc-sector/tests/watch_net.rs`, `bc-server/tests/inside.rs`, the `inside` e2e). **Not yet:** the
axis port's handoff, and the building site's work for suits.

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
  `Interior` only: a pure function of state, deterministic (libm), and allocation-free.
- **Standing and walking** (built). The city is the interior sector's one body, `Body::City`,
  still at its origin; `ground::move_step` moves every suit there as it does in space (a free
  suit taking `colony::interior::step`). With its grip armed (L) a suit coming in slow over the
  city is caught, lands, and walks, runs, crouches and hops on it as on a rock, and lets go as
  ever. Its surface is `colony::interior::probe`: the signed distance to the hull from inside (the
  floor and the glass), the end caps and the city's boxes (all but the walkers' walls, their edges
  rounded half a metre so a kerb is stepped up), exact over the floor and the roofs; what's
  straight under a suit is `ground_under`, which the catch and the landing ring read. The city has a down, the spin's, which no other body has: in its grip a suit
  falls that way under the colony's pull (`colony::frame::gravity`), not toward the nearest
  surface under the grip's, braked to 8 m/s as anywhere; it stands only on ground facing within
  30° of up (`ground::CITY_FOOTING_COS`), so a wall stops a suit walking into it and keeps one in
  the air off it, and a roof's edge is stepped off (the suit comes down on what's below), not
  walked round. A suit's origin keeps its stance from the floor and the roofs when free as on its
  feet (`colony::interior::FLOOR_CLEAR`), so letting go of the ground doesn't jump; lanes narrower
  than a suit (stance from each wall) can't be stood in.
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
  the docking hub's end (x ≈ −15,800, r ≈ 300 m), slow, nose down the colony, on flight assist:
  until its pilot is first heard from, that holds it by the gate (a page still catching up after
  the launch would otherwise find it fallen to the floor).
- **Back**: at rest inside the inner gate's ring of lights, Enter docks, as the outer dock does
  today (`Request::Dock`), back to the bay.
- **Later, through the axis port**: a suit flying out of the interior along the axis hands off to
  `sector-0` at the port, converting its state between frames (`colony_to_sector` and the spin's
  angular velocity). This is the scaling path's handoff, and waits on it.

## Who sees whom

- **One clock.** The interior sector keeps space's tick: it ticks each time `sector-0` has, right
  after it, woken by it (`bc_sector::spawn_follower`, `spawn_waking`), and never runs ahead. Its
  snapshots, the plaza's datagrams and the trams' timetable are then the same moment, for a pilot
  on foot and one in a suit, and a client can keep its clock from either.
- **Suits** are the interior sector's entities, replicated by its own snapshots to the pilots in
  it, exactly as in space.
- **Pilots on foot** (the plaza) aren't in either sector. They see suits through **spectator
  slots** (built): a session in the city takes a slot in the interior sector
  (`Control::Watch`, twice the pilots' slots there) whose snapshots, marked SPECTATOR in their
  header, carry no own suit and the suits within 2.5 km of where the pilot is (the plaza's last
  pose, in the colony's frame, moved twice a second), by distance-weighted priority as anyone's.
  The client draws them interpolated on the city's layer, relative to its render origin; it sends
  no input, and acks nothing, so a suit leaving the view is told for half a second.
- **Suits see people, trams and cars** (built) as the plaza relays them, ghosts as on foot:
  nothing touches a suit. The session sends a suit's pilot the people of the strip under the suit
  (over a window, the nearer strip's) within 1.5 km of it, from where its sector last had it
  (`Metrics::pilots[slot].pos`). A suit landing on a car or a crowd simply stands among them.

## Wire

- Welcome sector 2 for the interior; `Request::Launch` gains `into` (serde default `Space`, so
  today's clients are unchanged); a spectator flag on the snapshot header (one of its free bits);
  `BodyRef::City` (kind 2, no id) for a suit on the city, placed to 1.5625 cm over ±16,384 m (v19).
- A protocol bump, and new determinism goldens for an `Interior` scenario (native and wasm).

## Verification

- `bc-sim` unit tests: a suit launched in holds by the inner gate till its pilot is heard from;
  `interior_constrain` keeps capsules inside the hull and out of every box
  (dense sampling, as `colony_sweep_matches_dense_sampling`); a dropped suit falls at 1 g at the
  floor and less higher up; Coriolis deflects a dropped suit to −spin; drag caps speed;
  `ground::tests::city`: an armed suit lands on the avenue, walks it and lifts off without a jump;
  a wall stops it, never nearer anything than its stance; dropped on a roof it stands there, and
  walked over the edge it falls, kept off the walls, and stands below; the probe agrees with
  `colony::city::solid` and finds the ground straight under a suit.
- `no_alloc`: 64 suits flying the interior for 1,000 ticks (16 of them on the city, walking), 0
  heap operations.
- Determinism: an interior scenario hashes the same native and wasm; every existing golden is
  unchanged (space untouched).
- `bc-server/tests/inside.rs`: launch into the colony, fly, dock back; fire inputs do nothing;
  the two sectors keep one tick; a suit flown down over Hub Gate sees a pilot walking there, who
  sees it, and stops seeing it up the lift; armed, a suit lands on the avenue and walks up it,
  its pilot's prediction agreeing.
- `bc-sector/tests/watch_net.rs`: a spectator's snapshots carry no own suit and the suits near
  it, a suit leaving its view is told, and watching allocates nothing.
- e2e: a pilot launches into the colony from the bay, flies down over the avenue, lands there
  with the grip armed, sees the agent strolling outside Hub Gate, walks up the avenue, lets go and
  docks back (`inside`); a pilot on foot by Hub Gate's door watches a suit come in and stand on
  the avenue, and its pilot sees them (`colony`'s two-browser test; an agent, `bc-bot`'s
  `suit_inside`, flies the suit, as a page drawing in software beside another can't).

## Open questions

- **Why bring a suit inside?** The candidate: the building site. Colony projects (the Charter
  Board's great works, `DESIGN.md`'s roadmap) are built by suits carrying girders and machinery in
  the interior's gravity, paid from the project's funds. Without that, a suit inside is a
  sightseer.
- **Flight in air**: propellant use when hovering, and whether thrusters are allowed near people
  (a downwash rule?).
- **Suits among people**: a suit walking the avenue passes through the people and cars there
  (they're the plaza's, off the tick). Should it keep off them, or they off it?
- **How many suits a sector**: the interior shares nothing with space, so its budget is its own;
  64 is the guess.
- **Sleeping inside**: a pilot who leaves with their suit inside; parked on a roof, or sent back to
  the bay.
