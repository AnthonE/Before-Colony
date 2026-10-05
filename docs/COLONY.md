# The First Colony, inside: the plan for Milestone 5

## Context

Rebuild the L1 colony to look like the Gundam Wing references, and let a pilot walk from the hangar bay into its
interior: a city at full scale. The two references:
- **Image 1, the interior:** a dense city on a land strip, a long central avenue, parks, window strips with a
  structural grid curving up both sides, and heavy blue haze.
- **Image 2, the end seen from space:** a broad disc, a spire on the axis, mirror panels radiating from that end, and
  dotted rows of lights.

The colony keeps the simulation's size: 3.2 km radius, 32 km long, 20 km around, 1 g at the floor. That gives about
13,000 city blocks and about 65,000 buildings.

**Decided (with the project's owner):**
- **Players:** shared from day one. Other pilots walking in the city ship in the first release.
- **Travel:** trams, drivable vehicles, and later suits inside the colony.
- **v1 content:** districts and landmarks to find, plus working places: the Exchange floor, the Charter Board and
  a bar.
- **Build-out:** the far (+X) end is a building site.
- **Name:** "the First Colony". The server's arrival news already says it, and it resolves STORY.md's open question.
- **Pilots on foot:** flight suits, coloured per pilot.
- **Suits inside:** weapons are safe inside, by colony law.

## Status

| Phase | State |
|---|---|
| 0: foundations (frames, day, city rules, routes) | done. The routes follow the street grid (avenue, then a cross street, then a lane) rather than an A* over a graph; that's enough while the grid has no closures. |
| 1.1, 1.2, 1.4: mirrors, the −X end and its lights, the windows showing the city | done |
| 1.3: solid end structures | to do (needs a protocol bump of its own) |
| 2: into the colony | done, behind `--colony`: the lift (a cut down the end cap's face, not a lift lobby beyond the airlock), the city drawn and streamed, walking it, the Exchange floor, the Charter Board, The Arrival, districts and sights named on the way, the map (M), the `colony` e2e suite. `/status`'s `"city"` counts, and `__bc`'s `city_chunks`/`city_tris`/`city_ms` (what the streamer shows, and the slowest frame's building lately). The sights' found-list (`bc_client_core::sights`, the `sights_found` setting): each sight goes on it the first time it's reached, and the map ticks them off and lists them (`__bc.sights_found`). Indoor rooms: the Exchange floor, the Charter Board and The Arrival each have a room behind their door (`colony::city::room`: its walls, ceiling and door are the hall's solids, so walked and checked like any wall; `CITY_VERSION` 2), used at its counter inside, drawn close up with its own surfaces (its back wall shows the place), lit by its lamps, and the eye adapts to EV 8 inside (`__bc.city_room`, `city_ev`). The Proving Ground off Hub Gate's square (the Blast Hall, `docs/TRAINING.md`) has a room at a mobile suit's scale behind 40 m blast doors (`CITY_VERSION` 4). |
| 3: shared presence | done: pose and plaza datagrams (kinds 3 and 4), the server's plaza with its checks, the plaza's tick keeping the clock on foot, other pilots drawn in flight suits with their names, `bc-bot`'s `enter_city`/`set_pose`/`people` and the `flaneur` agent, `bc-server/tests/plaza.rs`, and the browser seeing an agent at Hub Gate (`colony` e2e). Figures are rigid pieces turned at their joints (`bc_client_core::figure`), not the suits' 24-bone rig; no sitting yet. `bc-swarm --walkers N` (agents strolling by each strip's Hub Gate, `bc_bot::Stroll`, the flaneur's legs). Two browsers in one e2e (`colony`): two pilots meet at Hub Gate, each seeing the other where they stand; one rides home and is gone from the other's street, and a suit an agent flies in (`bc-bot`'s `suit_inside`) comes down onto the avenue by the other, each seeing the other. |
| 4: trams | done: the timetable (`colony::transit`: 11 stations 2.45 km apart, 12 trains, 81 s runs at 1.5 m/s² to 56.8 m/s, 19.8 s stops, out on the +s track and back on the −s), island platforms with steps in the city's walls, trains drawn from the clock, riding in the car's frame (`bc_client_core::tram`) with the push of its acceleration, line and station names on the way, the presence anchor for riders (4 bits) and the server's boarding checks, the `colony` e2e ride of one stop, and a platform view in the showcase. The hash of the colony covers it (`CITY_GOLDEN`, not a separate `TRANSIT_GOLDEN`). Known: at the ends of the line a train changes tracks during its stop (no crossover is drawn); trains pass through people standing on the tracks; no seats, no gangways between cars, no sounds yet. |
| 5: vehicles | done: motor pools (`colony::pools`: Hub Gate's and one by each station), a bicycle model for cars and scooters against the city's walls (`bc_client_core::vehicle`), the server's checks (a driver's first pose at a pool, no faster than 45.5 m/s), the DRIVING controls, the chase and seat cameras, vehicles drawn in their drivers' colours, and a `colony` e2e drive. Vehicles are their drivers' alone: nobody else can take or ride one, and a car left behind is gone. No collisions between vehicles, or with people. |
| 6: suits inside | begun (`SUITS_INSIDE.md`): the interior sector with its pull, air and city, weapons safe, in and out by the inner gate, the client flying it; the interior keeps space's tick (one colony clock); on-foot pilots see the suits near them (spectator slots in the interior sector), and suits see the city's people, their cars and the trams; with the grip armed, suits land on the city and walk it, under its own pull (`Body::City`, `colony::interior::probe`), in the `inside` e2e too. Not yet: the axis port's handoff, and work for suits at the building site. |

**Where it starts (at `a40504a`):**
- **Simulation.** `bc_sim::world` treats the colony as a still solid cylinder: `COLONY_CENTER` (0,−4200,0), axis +X,
  `colony_spin_angle` 3405 ticks per turn. The docking hub is drawn only. `DOCK_CENTER` is at x −17250 and
  `LAUNCH_GATE` at −16980.
- **Exterior.** `bc-client/src/colony.rs` draws strips at `FIRST_WINDOW` 0.4 + k·120°. `colony_window.wgsl` paints a
  generic interior using a day clock that differs per client. The mirrors are hinged at +X, which is physically
  backwards.
- **Inside.** Nothing draws inside the cylinder.
- **On foot.** On foot is client-only:
  - `bc-client-core/src/walker.rs` with `bay::GRAVITY` 6.9 along −Y, collision by a linear box scan in `bay::Layout`;
  - the airlock only opens the pause menu (`onfoot.rs`).
- **Server.** The server knows only `bc_econ::wire::Place {Hangar, Space}`. On foot, the client gets no datagrams,
  so it has no server clock.
- **Rendering.** There is no floating origin, no Bevy States and no asset files. WebGL2 limits apply: one directional
  light, one shadow cascade, and LOD switched on the CPU.

## Architecture

1. **The colony as closed forms in bc-sim.** This follows the `bc_sim::field` and `content::landmarks` precedent:
   allocation-free, libm, and deterministic on native and wasm, so the client, the server and a future interior
   sector all share it.
   - **Math** (`bc_sim::colony::{frame, time, mirrors, city, transit}`):
     - Frames: sector ↔ colony frame (spins with `colony_spin_angle`) ↔ city coordinates `CityPos {strip, x, s, h}`.
       City coordinates are the cylinder unrolled, with plumb (radial) walls, so a building is an axis-aligned box.
     - `gravity(h) = ω²(R−h)`.
     - The colony day, from the tick.
     - The city layout as a function of integer cell coordinates, with an integer hash and fixed-size per-block
       arrays.
     - Queries: `solid(strip, aabb, stage)`, `block_info`, `lots`, `ground`, `key_place`.
   - **Content** (`bc_sim::content::city`): district tables, places and names. `CITY_VERSION` moves with
     `PROTOCOL_VERSION`, like `LANDMARKS_VERSION`.
   - `FIRST_WINDOW` and the strip angles move here, so the outside and the inside always agree.
2. **Shaders never re-implement the layout.** At startup the client bakes a per-block RGBA8 texture from the Rust
   rules: 256 blocks along × 72 rows (3 strips × 24). Each texel holds kind, district, height class, seed and flags.
   WGSL (`bc::city`) draws only the pattern inside a block. The outside windows, the inside ground and the far LOD
   all read the same texture.
3. **Inside, everything is drawn in the colony frame, so the city never moves.**
   - Only what's seen through the windows turns: stars by the spin angle, the mirror-borne sun.
   - A floating render origin: `RenderOrigin(DVec3)` and `Placed(DVec3)`, re-based every km. Chunk meshes are built
     relative to their own anchors.
   - `Venue {Space, Bay, City}` hides the space scene in the city. `Indoors` keeps meaning "on foot".
   - The bay stays at `BAY_ORIGIN`. The switch to the city is a fade at the lift doors.
4. **Each strip has its own sun**, the window opposite it, stylised on the axis at twice the mirror angle.
   - The city shader uses Bevy PBR on the camera's strip, with the one `Sun` light re-aimed to that strip's key light
     (shadows on High/Ultra).
   - The other two strips get a simple key-plus-sky term in their own frame.
   - Haze has a single definition, the sky function (`bc::colony_sky`, `COLONY_LOOK.md` pass 1); Bevy's
     `DistanceFog`, matched to it, covers only what the city's shader doesn't draw.
5. **Walking.** `Walker` becomes generic over a `Solid` trait plus an `Env {gravity, pseudo}`. Its users:
   - `bay::Layout`, unchanged;
   - the city, in a walker frame of (x, h, −s); the handedness matters, or the city comes out mirrored;
   - moving interiors (lift car, tram car) in their own frame, re-expressed at the doorway.

   Routes come from an A* over the street graph, followed by the existing `Guide`.
6. **Presence, "the plaza", stays off the tick, like the hangar.**
   - New datagram kinds: `Pose`=3 (client to server, 15 Hz) and `Plaza`=4 (server to client, 10 Hz).
   - The session task handles both. It sends plaza datagrams on its existing 100 ms `every` timer. The hub is a
     `Mutex<Vec<Option<Person>>>` in `GameShared`, the `market.rs` pattern.
   - Plaza datagrams carry tick, sub-tick, echo and hold, so the client keeps a server clock on foot. A 2 Hz
     heartbeat in the bay keeps the colony day and timetable in sync everywhere.
   - People on a tram or in a car are sent in its frame (an anchor), so they stay exactly inside it on every screen.
   - Poses are checked for plausibility against the closed form; implausible ones are not relayed. No people go into
     the sector simulation: they need no prediction or lag compensation.
7. **Ship safely.** The city sits behind a server `--colony` flag (Welcome flag 16 `COLONY`; 8 is `ANIME`) until presence and
   trams work. The new exterior ships on its own first. The city is survival-only, because it hangs off the bay.

## Phases (PR-sized steps)

### Phase 0: foundations (nothing visible changes)
- **0.1 Frames.** `crates/bc-sim/src/colony/{mod,frame}.rs`:
  - constants `STRIPS`, `FIRST_WINDOW`, `STRIP_WIDTH`;
  - `CityPos`; `to_colony` and `from_colony` (`Land` or `Window`); `colony_to_sector`; `gravity`; `s_scale`;
    `local_frame`.
  - `colony.rs` switches to them.
  - Tests: round trips on every strip; plumb walls; 1 g at h = 0 and 0.7 g at r 2252; agreement with `world.rs`.
- **0.2 Colony day.** `colony/time.rs`: `day(t, frac) -> Day {phase, daylight, sun_elev, mirror_beta, lamps}` and
  `key_light(strip, &Day)`.
  - `colony.rs::spin` uses it instead of `VisTime`/`DAY_SECS`, so the outside day becomes shared. The showcase maps
    `?t=` onto ticks.
  - Tests: periodic, continuous.
- **0.3 City rules v1.** `colony/city.rs`, `content/city.rs`:
  - `BlockId`, `block_info`, `lots -> [Building; 9]`, `solid`, `ground`, `district_at`, `key_place`;
  - `Stage(u8)`, so the building site can grow with colony projects later.
  - Tests:
    - `CITY_GOLDEN` hash in `bc-sim/tests/determinism.rs`, native and wasm;
    - streets, avenue, promenades, tram corridor and platforms are never solid (dense sampling);
    - buildings stay inside their blocks and under the height cap;
    - `solid` matches a brute-force check (proptest);
    - every key-place door faces a street;
    - a `no_alloc.rs` case: 1e5 queries allocate nothing.
- **0.4 Street graph and routes.** `bc-client-core/src/city_nav.rs`: A* over intersections plus avenue, promenade,
  quay, bridge and door edges, returning waypoints for `Guide`.
  - Tests: every key place reachable from its Hub Gate; routes at most 1.6× the straight-line distance.

### Phase 1: the new colony, outside (the image-2 look; ships on its own)
- **1.1 Mirrors.** `colony/mirrors.rs` gives `quad(k, beta)` and `swept_bounds()`; `colony.rs` draws them.
  - Hinged at the −X end. The opening angle follows the day: about 1° at night, 45° at noon.
  - Red lights at the corners.
  - Content test in the style of `landmarks_are_clear_of_everything`: the swept volume is clear of the field's reach,
    `LANDMARKS` (by 2 km), `SPAWN_BASES`, the launch ring and the dock.
- **1.2 The −X end and the lights.**
  - The bay ring: a disc with bay notches and six spokes. The docking hub restyled as a spire of stacked modules.
    Hull ribs and panel lines. The +X axis port with scaffolds and cranes.
  - `shaders/light_dots.wgsl`: one mesh of about 20k dots along the window frames, ring edges, spire tiers and mirror
    edges. Each dot keeps at least 1.5 px on screen, glows in HDR and blinks with its own phase.
  - Showcase colony cams 6–8: image 2's composition, the night side, mirrors opening in the morning. Matching
    `gfx.spec.ts` rows.
- **1.3 Solid end structures** (separate PR, protocol bump).
  - `world.rs`: `hull_contact`, `colony_sweep`, `inside_colony` and `constrain` cover the spire and the ring. Both are
    shapes of revolution, so a still collider is exact, the same argument as for the hull.
  - Add them to the field keep-out. The default field doesn't change, so `FIELD_GOLDEN` stays.
  - Tests: extend `colony_sweep_matches_dense_sampling`; survival launch and dock tests; re-check every determinism
    golden.
- **1.4 Windows show the real city.**
  - `bc-client-core` `block_atlas()` bakes the texture (tested natively); it is bound to `WindowMaterial`.
  - `colony_window.wgsl` paints from it and from `bc::city`, keeping the clouds and haze. Night lamps come from
    `day().lamps`.
  - A native test `include_str!`s the WGSL and checks its constants against Rust.

### Phase 2: into the colony (single pilot, behind `--colony`)
- **2.1 Wire and server** (protocol bump).
  - `bc-econ/src/wire.rs`: `Place::City`; `Request::EnterCity {strip}` and `LeaveCity`; `Update::Place` gains
    `strip: Option<u8>` (serde default).
  - `session.rs`: enter the city only from the hangar; launch is refused while in the city.
  - `/status` shows place `"city"`.
  - `config.rs`: `--colony` and `--colony-stage`. `HangarState::in_city()`.
  - New `bc-server/tests/city.rs`. Update `PROTOCOL.md`.
- **2.2 Airlock to the lift lobby.**
  - `bay.rs`: doors that let you through when open (`hits` skips them); corridor and lobby blocks beyond x < −18;
    `Spot::Lift(k)` (`lift_1` to `lift_3`); `route` reaches them.
  - `hangar.rs` draws the corridor and lobby with a sign per strip.
  - The prompt becomes "AIRLOCK: TO THE LIFTS". "LEAVE THE BAY" stays in the pause menu.
  - Walker tests: to each lift and back. `hangar.spec.ts` stays green.
- **2.3 The venue and the interior shell.**
  - New `venue.rs`: `Venue`, `RenderOrigin`/`Placed` and re-basing, and switching each scene's visibility.
  - `sky.rs`:
    - a city branch in `apply_light_tier`: exposure by time of day, fog, cluster config, shadow bounds;
    - `eclipse` is skipped inside, and the `Sun` is aimed by `key_light`;
    - `sky.wgsl` becomes a `bc::sky` import with a rotation uniform.
  - New `city.rs`:
    - `CityMaterial = ExtendedMaterial<StandardMaterial, CityExt>` with `shaders/{city,city_lib}.wgsl`;
    - one ground mesh per strip, split across its width every 32 m. It is straight along the length, so there is no
      ground LOD: roads, parks and water are painted from the texture;
    - end caps; window ribs every 400 m; an inside-window shader with the turning sky, the mirror and the haze.
  - Near plane 0.15 m in the city.
  - Showcase `Scene::City`, views 1–6:
    1. the avenue at Hub Gate looking +X at noon (image 1)
    2. from the lift at r ≈ 2700
    3. night
    4. window bank
    5. canal
    6. Exchange floor

    Matching gfx rows.
- **2.4 Building meshes and streaming, without Bevy** (`bc-client-core/src/{city_mesh,city_lod}.rs`).
  - `ChunkMesh {anchor: DVec3, frame, MeshData}`. Pieces: base plus tower, setbacks, roof kit, shopfront band, site
    frames and cranes, and L0 street furniture. Vertex colours carry class, window style, seed and AO, as `hull.wgsl`
    uses them.
  - LOD:

    | Level | Distance | Chunk | Content | Triangles per chunk |
    |---|---|---|---|---|
    | L0 | to 350 m | 256 m | full buildings | 25k |
    | L1 | to 900 m | 512 m | | 12k |
    | L2 | to 2.2 km | 1 km | one box per lot | 6k |
    | L3 | beyond | 2 km | one box per block | 4k |

    L3 covers the whole interior (about 140k triangles), so the skyline never flattens into paint.
  - Hysteresis; an LRU cache; a per-frame budget that adapts to the frame time. In showcase mode it builds
    synchronously, so screenshots are reproducible.
  - Switching is on the CPU, as in `rocks.rs` and `landmarks.rs`.
  - Tests: every vertex within 1 mm of its curved surface; no NaNs; the densest CBD chunk within budget; exact
    coverage with no flicker along a scripted camera path; deterministic output.
- **2.5 The lift and walking.**
  - `walker.rs`: `Solid` and `Env`; `Layout` implements `Solid`, and the bay tests are unchanged.
  - `bc-client-core/src/city.rs`: `CityCollider` and `CityFoot`.
  - `onfoot.rs`: `Seq::LiftDown{strip}` and `LiftUp`, a 948 m ride you can skip after 2 s; walking the city; the
    Hub Gate lift sends `LeaveCity`.
  - `__bc` gets `venue`, `strip`, `district`, `city_feet`, `chunks`, `city_tris`, `city_ms`. Dev hooks: `walk_to`
    takes city slugs; new `lift`.
  - New `e2e/tests/colony.spec.ts`:
    - bay to lift 1 to Hub Gate;
    - walk to the Exchange floor and buy;
    - back up to the bay;
    - `/status` place changes, and `hot_path_allocations` stays 0.
  - New `scripts/e2e.sh colony` (survival, `--colony`, no dolls) and a step in `ci.sh`.
- **2.6 Key places v1** (strip 0, all within about 700 m of the lift).
  - Hub Gate.
  - The Exchange floor: its terminal opens `Panel::Terminal(Spot::Exchange)`, the bay's panel and requests.
  - The Charter Board hall: news, bounty rates, the site's stage.
  - The Arrival, a bar: you can sit (its seats out front, `arrival_seats`; inside is still to come).
  - About 10 landmarks, named on approach, with a found-list kept in settings.
  - An M map overlay on the page, from published JSON.
  - Indoor rooms use EV 8.
  - New `controls.rs` group "IN THE COLONY" (read CONTROLS.md first).

### Phase 3: shared presence
- **3.1 `bc-proto/src/presence.rs`** (protocol bump): `PacketKind::Pose`/`Plaza`, `Anchor`, `PersonPose`,
  `PlazaWriter`/`PlazaReader`. No allocation. Anchor kinds for trams and vehicles are reserved now.
  - Tests in `roundtrip.rs`: round trips within half a step; decoders never panic; how many people fit in 1100 B.
- **3.2 Server hub.** New `bc-server/src/plaza.rs`:
  - `accept()`, checked against `bc_sim::colony`;
  - `fill(viewer)`: same strip, within 1.5 km, with a rotating priority, at most 48 people;
  - hide a pilot after 5 s of silence and drop them at 15 s.

  In the session, pose datagrams are accepted only in the city. Plaza datagrams go out at 10 Hz in the city and as a
  2 Hz heartbeat in the bay. Names go as `Update::People` on tag 11. `/status` gets `"city": {people, by_strip}`,
  with no positions.
  - Tests in `tests/plaza.rs`: two bots see each other; a teleport isn't relayed; leaving removes a pilot; the clock
    is within 1 tick.
- **3.3 Client.** `bc-client-core/src/plaza.rs`:
  - `PlazaView` (Hermite interpolation in each anchor's frame; anchor switches without a pop);
  - `plaza_clock` (the existing `Clock`);
  - `colony_tick`, `set_pose` and `poll_pose`.

  `lib.rs` dispatches on `packet_kind`. `net.rs` sends poses on the 8 ms timer. bc-bot gets `enter_city`,
  `send_pose` and `people`, a `flaneur` example and `bc-swarm --walkers`. Bodiless agents appear as telepresence
  drones tagged MD (decide in STORY.md).
- **3.4 Pilots drawn.** `bc-model/src/pilot.rs`: a flight-suit pilot on the shared 24-bone rig at human scale,
  coloured per pilot, about 1.5k triangles near and 150 far, AO-baked.
  - `people_vis.rs`: walk, run, idle and sit, posed with the `gait`/`ik` machinery; name tags within 40 m; nothing
    beyond 1.5 km.
  - Tests: triangle budget, fits the walker's box. A showcase crowd.
- **3.5 Two-pilot e2e.** Two browser contexts each see the other: name, and position within 2 m. `/status` counts 2.

### Phase 4: trams (in v1; without them only about 2 km round Hub Gate is practical)
- **4.1 Timetable.** `colony/transit.rs` with station tables in content: `train(line, k, t, frac) -> TrainState`, the
  cars' solids, and platforms in the city rules.
  - Tests: `TRANSIT_GOLDEN`; exactly periodic; trains at least 300 m apart; |a| ≤ 1.5 m/s²; doors line up with
    platforms at each stop.
- **4.2 Riding.** `trams_vis.rs`.
  - The walker rides in the car frame with `Env.pseudo = −a`. Board and alight at open doors; seats; "NEXT TRAIN"
    boards; sound cues (`bc-sound`).
  - Presence carries the `Tram` anchor; the server checks boarding against the timetable within ±1 s.
  - E2e: ride one stop. Gfx rows.
- **v1 release:** drop the `--colony` gate.
- Docs:
  - `ARCHITECTURE.md`: crates, "The colony inside" (frames, closed forms, venue, presence, trams), verification rows;
  - `PROTOCOL.md`: datagrams 3 and 4, places, People;
  - `DESIGN.md`: "The colony isn't walkable" becomes the colony on foot; roadmap;
  - `STORY.md`: names, districts, places, the hub's geography;
  - `CONTROLS.md`;
  - `CLAUDE.md`: the `colony` e2e suite; `bc_sim::colony` stays deterministic and allocation-free.

### Phase 5: vehicles
- `bc-client-core/src/vehicle.rs`: a bicycle-model car and scooter in city coordinates, colliding through `solid`,
  with grip from `ground()`.
- Motor pools at Hub Gates and stations. E to take a vehicle or leave it. A "DRIVING" controls group, with GTA-like
  keys checked against CONTROLS.md.
- Procedural meshes; the chase camera on Tab.
- Presence `Vehicle` anchor plus steering and light bits. Server checks: speed at most 35 m/s, on roads and plazas,
  never inside solids.
- E2e drive and a gfx row.

### Phase 6: suits inside the colony (outline only; needs its own design doc first)
- **The interior sector.** A second sector on thread `sector-1`, in the colony frame. There the city is still (ground
  speed exactly 0), the case the ground code handles best.
- **bc-sim:**
  - `WorldKind {Space, Interior}`;
  - the interior constraint and sweep: inside the cylinder, the city's solids via `colony::city`, and the end caps;
  - centrifugal force, Coriolis and drag in `flight::integrate` for the interior only, so space flight stays bit for
    bit (I10);
  - `move_step` with real gravity instead of grip gravity, on a new `Body::City`.
- **Colony law: weapons safe.** Fire and strike inputs are ignored inside, so no shot sweeps through the city are
  needed. Mobile Dolls stay outside.
- **Getting in and out.** `Request::Launch {into: Colony}` from the bay to an inner launch gate near the axis; docks
  back the same way. Later, handoff at the axis port with frame conversion (scaling path, step 2).
- **On-foot pilots watch suits** through spectator slots chosen by position. People, trams and cars stay relayed
  ghosts.
- **Open questions:**
  - flight and propellant in air;
  - suits per sector;
  - the gameplay reason to bring suits inside (candidate: suit work on the building site for colony projects).

## Numbers (all tunable)

**Frame**
- ω = 0.055359 rad/s.
- Strip width 3351.03 m. The strip edge is at s = 0, at `window_centre(k)` + 30°.
- 0.7 g at r = 2252 m (the bay ring).

**Across a strip**
- Central avenue 80 m: tram median 16 m (30 m at stations), roads 2 × 14 m, tree-lined sidewalks 2 × 18 m.
- 12 rows of 128 m blocks each side of the avenue.
- One row on the +s side is the canal: 40 m of water, 20 m quays, a bridge at every block.
- Window-bank park and promenade, 99.5 m at each edge, railing at the glass.

**Along a strip**
- Block index `bx = floor((x+16384)/128)`.
- Streets 24 m, 40 m every 4th.

| Part | Blocks | x |
|---|---|---|
| Hub Gate (its square two rows either side of the avenue, civic blocks round it) | 3–7 | [−16000, −15360] |
| City | 8–198 | to +9088 |
| Site | 199–249 | to +15616 |

**Districts and buildings**
- 12 districts per strip, 2048 m each, from a table per strip. Strip names (placeholders): Charter, Canal, Gardens.

| District | Heights | Built as |
|---|---|---|
| Business district | towers 80–240 m | towers on podiums, stepping in up to three times, crowned (a hat, a stepped top, a lantern, a penthouse), masts on the tall ones |
| Midtown | 25–80 m | street walls setting back once or twice; towers on some lots (80–240 m) |
| Residential | 12–40 m | penthouses, courts round yards, water tanks on the Canal's roofs, terraces in the Gardens |
| Old Town | 9–18 m | chimneys on the party walls, mansard attics |
| University | 9–27 m, campaniles to about 58 m | quads, some with a campanile |
| Works, Port | 9–34 m, stacks to about 64 m | sawtooth, monitor and bayed sheds, stacks, offices on the corners |
| Civic | 16–41 m, to about 50 m with their lanterns | attics, and lanterns on the big ones; towers on some lots (80–240 m) |
| Site | frames 20–120 m, cranes to 160 m | |

- Floors 3.6 m, ground floor 5 m, cap 240 m.
- A building is at most eight boxes (its pieces, `city::Building::pieces`), each standing on the street or on one
  under it; crowns and masts never pass 240 m.

**Street furniture** (`colony::furniture`: like the city, a closed form of where you ask)
- Lamps where the ground's paint has their pools (`city_lib.wgsl`, the numbers `city_atlas.rs`'s tests check):
  down every block's kerbs about 30 m apart (`LAMP_GAP`), evenly from corner to corner along the lanes (the
  corner posts 0.25 m in from both kerbs, before the crossings) and between the corners along the cross streets
  (none in the canal's row); a post 0.8 m in from its kerb, its arm out to the lantern 1.5 m over the street; the
  bank road's far kerb the same. The avenue's lamps are lanterns on 4 m posts at 22.7 m from its middle line,
  under the trees. Path lamps beside a park's loop (its pavilions stand inside them, a metre clear) and along the
  quays, 2.2 m from the water; eight on a plaza's ring, 24 m out.
- Trees in the avenue's pits, every 8 m (24.3 m from its middle line, none within 5 m of a crossing), and a row
  down each quay; benches (0.42 m high) between the avenue's trees every 16 m. Nothing stands in a crossing, a key
  place's doorway (6 m clear) or a walk.
- The colony's own lamps (Hub Gate's square, the banks' promenades, the median) are paint only, for now.
- Drawn close up (L0) from the rules' boxes (`city_mesh`: posts, benches and trunks exactly their solids, the arms,
  heads and lanterns over anyone's reach); trees at L0 and L1, a species by strip and setting: the avenue's and
  the quays' keep their leaves at least 5.5 m up and clear of the lanterns (the parks' and plazas', walked under,
  2.9 m).

| Thing | People on foot, cars, the server's pose check (`city::solid`) | Suits (`city::each_solid`, `solid_built`) |
|---|---|---|
| Buildings and all their pieces | solid | solid |
| Lamp posts, trees' trunks (3 m) | solid | stepped over |
| Benches | stepped up onto (under `walker::STEP`, 0.45 m); a wall to a car | stepped over |
| Crowns, arms, lanterns; the parks' and plazas' trees | not solid | not solid |

**Structures**
- Bay ring: r 1950–2650, x [−16450, −16050].
- Spire: r ≤ 340, x [−16900, −16000], kept out of the dock and gate at x −16980 to −17550.
- Mirrors: 7000 × 3200 m, hinged at x −16000, r R+60. They stay within x ≤ −9000 and 8.21 km of the axis (the field
  reaches only |x| ≤ 7.3 km).
- Windows: glass at R, cross ribs every 400 m.
- −X cap: three glass lift shafts at the strip centres.

**Day and light**
- `DAY_TICKS` 86,400 (48 min): night 8, dawn 4, day 32, dusk 4. Daylight rises and falls as sin² over the light
  and the mirrors open with it, so the sun stays under 25° for about the first and last sixth of it (`COLONY_LOOK.md`
  pass 1.0).
- Exposure EV 14.5 at noon to 8.5 at night, by the colour script (`city_hour.rs`); EV 8 indoors.
- Haze by height (`bc::colony_sky`): about 1.5e-4 /m at the floor by day, scale height about a kilometre, so the
  street goes blue within a few kilometres while the opposite strip, across the clean core, shows at about 60%.

**Lift**
- 948 m at 40 m/s and 2.5 m/s², about 40 s.

**Trams**
- One maglev line per strip on the median.
- 11 stations about 2.45 km apart, from x −15,500 to +9,000.
- 60 m/s, 1.5 m/s², 20 s dwell: about 17 min end to end.
- 12 trains per line, one every 3 min. Each train is 3 cars × 24 m.

**Traffic** (`colony::traffic`: a closed form of the tick, like the trams; `docs/ARCHITECTURE.md`, "The colony inside")
- Which streets (`|c|` is the distance from the avenue's middle line):

| Street | Moving cars | Parked |
|---|---|---|
| The avenue's carriageways (−x on the +s side, +x on the −s side) | outer lane (\|c\| 17.9): rows ±1's track 0; middle lane (14.3): their track 1; the inner lane (10.7) empty | none |
| Streets along the strip (k 1 to 11, 24 m; 40 m at k 4 and 8) | each half its row's ring: track 0 by the kerb, track 1 next to it; a 40 m street's third lane empty | both kerbs' bays |
| The bank road (row 12's outer street, a lane a way) | the near lane: row 12's track 0; the far lane by the glass empty | both kerbs' bays |
| Wide cross streets (every 4th, 40 m) | one-block cross legs between neighbouring streets along the strip, queueing at the reds | track 0's night bays only |
| Narrow cross streets (24 m) | none (cars cross them only going straight along the strip) | both kerbs' bays, 10 m or more from a crossing |
| Hub Gate's stretch (blocks 3 to 7), the building site, the tram's median | none | none |

- Rings keep right, round each row: track 0 round one 512 m stretch (wide street to wide street), track 1 round the
  whole row, from bx 8 to bx 200 (sixteen blocks more for each district built out, at most 248).
- Signals at every junction of the wide cross streets: an 80 s cycle, 37 s green and 3 s amber for the cars, then
  2 s all-red and 38 s walk for the people. Phases `(800·strip + 1200·((bx/4 + |k|) mod 2)) mod 2400` ticks: a
  checkerboard, so at 12.8 m/s (512 m in half a cycle) both ways of every street ride a green wave. Cars are on a
  signalised junction only 1.4 to 27.3 s into its green.
- 12.8 m/s down the streets, 10 m/s on the cross legs, turns of 9.4 and 13.0 m radius at 4.85 and 5.7 m/s;
  2 m/s² up, 2.5 m/s² braking.
- Platoons of up to 10, one a cycle a ring: 7 m apart in the queue, each moving off 1.2 s after the one ahead.
  Track 0's lap is 160 s (two platoons), track 1's 3,920 s (49). A platoon waits 16 to 21 s at each red.
- Both ways' platoons pass a point of the avenue together, so a signals' 512 m stretch has a busy half and a quiet
  one: looking 400 m up the avenue (+x) from its middle two blocks (4n+1, 4n+2), no car is moving in view 23 to
  28% of the time; from the blocks either side of a wide street (4n+3, 4n+4), 0 to 2%. The showcase's third
  camera stands in the busy half (x −13,716, block 20).
- Out by the hour: none of the platoons stays in at the rushes; through the night track 0's platoons park whole,
  each at its own hour, 3.075 m to the kerb side and 9 m back from their places, and pull out at dawn. Track 1
  runs 70% full all day.
- Track 0's places manned and bays parked, by district: Business 0.95 and 0.25, Midtown 0.9 and 0.45, Civic 0.8
  and 0.3, Port 0.75 and 0.4, Works 0.7 and 0.4, Residential 0.6 and 0.55, University 0.6 and 0.45, Old Town 0.5
  and 0.6, Park 0.35 and 0.2, the site 0.2 and 0.1.
- Kinds: 68% cars (4.4 × 1.8 m), 10% taxis (4.6 × 1.8), 14% vans (5.2 × 2.0), 8% scooters (1.9 × 0.7); the bays
  hold cars and 20% vans.
- A strip has about 22,700 cars moving and 82,000 parked by day, 10,400 and 94,000 at night. A 300 m square (a
  frame's question) costs 0.03 ms natively: 12 to 30 moving cars and 56 to 128 parked; a 2 km square at night
  about 0.7 ms and 4,000 cars.

**People** (`colony::walkers`: the city's own, a closed form of the tick; pilots are the plaza's)
- Half a person's width (`RADIUS`) 0.3 m: nobody comes within 0.6 m of anybody.
- Lines are loops with slots about 4 m apart (never under 2 m), moved on one every 45 to 108 ticks: 1.0 to
  1.8 m/s walking, 2.8 m/s jogging. A day is whole laps of every line.
- Where: a block's pavement on three lines 1.6 (with the kerb on the right), 2.6 and 3.6 m in from the kerb, round
  a run of four blocks over the narrow cross streets' zebras (rows ±2 to ±12, not the canal's) or round each block
  (rows ±1); the avenue's walks on capsules 26.7 to 31.1, 27.8 to 30.0, 33.9 to 37.6 and 35.0 to 36.5 m out; a
  park's loop and its middle; two pairs of rings round a plaza's monument (11 to 12.4, 28 to 29.4 m); the quays;
  the banks' promenades (joggers round 8 blocks, strollers round 2); Hub Gate's square, 2 m either side of its
  paving's bands every 32 m, crossing half a slot apart (1.41 m at the nearest); the platforms (two files at each
  door, a way on and a way off).
- Standing: every 7 m along the pavements (by the walls 4.5 m in, at the kerb 0.95 m in), twos to fours in the
  square's cells, round the monuments, at the quays' railings every 9 m; asked every 30 s. Sitting: the avenue's
  benches, a slot off its line for two laps at a time.
- Who's out: a slot's draw against its line's share (0.1 to 0.3) times how busy the place is: by district, day
  and night (Business 1.0 and 0.05, Midtown 0.85 and 0.2, Old Town 0.7 and 0.25, the Works 0.35 and 0.03, …),
  an evening's bump as the lamps come on; near a station (the avenue and two rows either side) ×1.5 at its
  platform, falling to ×1 at 300 m; Hub Gate 1.0 and 0.3; the banks 0.5 and 0.04; nobody on the building site.
  People come and go at a line's corners, fading over 1.5 m.
- A strip has about 200,000 to 250,000 people out at noon, 105,000 to 125,000 at the end of dusk and 40,000 to
  50,000 at night. A 300 m square holds 200 to 840 at noon and 10 to 330 at night, and costs 0.12 to 0.25 ms
  natively.
- People and cars never share ground: the only road people cross is a narrow cross street, on its zebra, and no
  car drives one. `tests/life.rs` holds every car's footprint 0.5 m or more from anybody's middle; over 7.4
  million people sampled, the nearest is 3.17 m (somebody at a kerb by a parked van). Nobody crosses at the
  signals yet.

**Presence**
- Pose: about 128 bits at 15 Hz.
- Plaza: 74-bit header plus about 118 bits a person, at most 48 people (about 720 B, about 7 KB/s).
- Interest: same strip within 1.5 km.
- Others are drawn about 200 ms behind.
- Plausibility: on foot at most 9 m/s × 1.5 + 2 m, at most 0.3 m into a solid; strip changes only by lift.

**Budgets**
- City triangles on screen: Low 150k, Medium 500k, High 1.2M, Ultra 2.5M.
- About 300 draw calls.
- Mesh building per frame: clamp(15% of the frame, 1.5–4 ms).
- About 25 MB of vertex data on High.

## Verification
- **Every PR:** `scripts/ci.sh`, which covers:
  - fmt;
  - clippy, native and wasm, for both webgl2 and webgpu;
  - `cargo test --workspace --release`;
  - wasm determinism, including the new `CITY_GOLDEN` and `TRANSIT_GOLDEN`, and the city's life (`TRAFFIC_GOLDEN`,
    `WALKERS_GOLDEN`);
  - the `no_alloc` tests.
- **Visual:** `scripts/e2e.sh gfx webgl2 --grep "colony|city"`. Review the `e2e/artifacts/` shots against the two
  references (colony cam 6 is image 2; city view 1 is image 1).
- **Flows:**
  - `BC_E2E=1 scripts/ci.sh`, which adds the `colony` suite: lift, walk, Exchange, two pilots, a tram stop;
  - `hangar` must stay green.
- **Manually:**
  1. Run `BC_COLONY=1 BC_DATA=… scripts/dev.sh` (`BC_COLONY` is new: `dev.sh` passes it on as `--colony`).
  2. Open http://127.0.0.1:8080 in two windows.
  3. Walk out of the airlock, ride the lift, and see each other at Hub Gate.
  4. Ride a tram, and check `?perf=1` in the city on High and Low.
- **`/status`:** city counts but no positions, and `hot_path_allocations` 0.

## Risks
- **wasm CPU and memory.** Mitigations:
  - the adaptive build budget;
  - pre-building around Hub Gate during the lift ride;
  - LRU caps;
  - `RENDER_WORLD`-only meshes;
  - measuring with `perf.rs`.
- **WebGL2 depth precision.** Bevy's reverse-Z is remapped on GL: about 0.2 m at 1 km and 6 m at 5.5 km. Mitigations:
  - near plane 0.15 m;
  - no coplanar layers (markings are painted);
  - check `EXT_clip_control` first;
  - a near/far camera split only if banding shows.
- **Bevy 0.19 APIs.** `EnvironmentMapLight::rotation` for per-strip reflections; check it early in 2.3, and fall back
  to ambient only.
- **SwiftShader e2e speed.** Low tier, synchronous builds in the showcase, walks driven by dev hooks, and a CI cap
  on the detail level.
- **Looking like image 1.** Facade shader (window grid, lamps by the day), districts that give the skyline a shape,
  heavy haze, ribs, and review shots composed like the references.
- **Scope.** Each phase merges behind `--colony`. The exterior (Phase 1) ships first. Vehicles and suits come after
  v1.
