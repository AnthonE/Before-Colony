# The First Colony, polished: the look, and the passes that get it there

`COLONY.md` built the city: every block, street and room from closed forms, walked, ridden and
shared. This plan is about how it looks. The goal, in the owner's words: very realistic and anime
at the same time, a GTA in space with mobile suits.

## The look: real light, an anime frame

Light, materials and air stay physical. That's the "realistic" half, and the engine already works
that way (lux, nits and EV100 throughout `city.rs`). The "anime" half doesn't come from cel shading
or ink lines. It comes from what the anime that defined colony life chose to put in the frame:

- **A colour script by the hour.** Each time of day has its palette and its mood. Noon is pale and
  hazy blue-white. The golden hour has long amber light and violet shadows. Dusk is magenta haze
  with the lamps coming on. Night is deep blue with warm windows. Shinkai's cities are lit as
  carefully as a photograph, then coloured like a poster.
- **The sky.** Here the sky is the colony: the other two strips hang overhead through kilometres of
  haze, the windows burn white, and there are clouds in the core. The Endless Waltz OVA and every
  Gundam colony interior lean on that shot. It should be the best thing on screen at every hour.
- **Light that glints and blooms.** Glass catches the window strips. Wet asphalt mirrors the signs.
  Lamps, signals and tail lights flare. Silhouettes stand against bright haze.
- **Density at eye level.** Patlabor 2's Tokyo and Ghost in the Shell's port city are believable
  because of their clutter: signs, wires, vending machines, shopfronts, people. GTA's city is
  believable because it moves: traffic, pedestrians, signals changing.

For the colony itself, the realistic reference is NASA Ames's 1975 space-settlement paintings
(Rick Guidice, Don Davis): land in many greens and browns, bright windows, clouds hanging in the
middle of the cylinder. That is the daytime look to aim for; the anime is how it's lit and framed.

**Not doing:** toon or cel shading on the city, and outlines on buildings. The city stays physically
lit. Outlines on characters and suits only, as an option, is an open question below.

## Where it stands

*(From the showcase's city cameras on High, WebGL2, at noon, the late afternoon, dusk and night: see
"Review set".)*

**What already works:** the scale and the curve. From Hub Gate the city runs 32 km into blue haze
and rises up both sides: the shot only a cylinder gives. The layout reads as a real city plan:
avenue, tram median, canal, parks, districts that rise and fall.

**What reads as a prototype:**
- **Greybox materials.** Walls are near-uniform dark slate and roofs flat light grey. The window
  grid shrinks to a fine moiré at distance. Plazas and pavements are large untextured pale greys.
  Nearly everything sits in one blue-grey band.
- **Flat light all day.** The key light's elevation is twice the mirrors' angle
  (`colony::time::day`), and daylight stays above 0.5 from dawn's end to dusk's start. So the light
  is above about 47° for all 32 minutes of day: roofs lit, walls dark, noon and "late afternoon"
  (`t=1350`) nearly identical. Low, raking light along the axis exists only in the four-minute dawn
  and dusk, and even then the light's colour barely warms (`light_city`'s `warm`).
- **No shadows past 150 m.** The city never sets the Sun's cascades: `sky::apply_light_tier`
  returns early inside, so the city keeps whatever it found. In a showcase that's Bevy's default
  (one cascade to 150 m on WebGL2, so the aerial cameras show no shadows at all); in game it's
  space's (500 m).
- **The sky doesn't read as a city by day.** The other two strips overhead are lost in uniform
  haze. What reads instead is the windows' frame grid, pale blue with fine lines. At noon the
  far strips' grey roofs, through about half a transmittance of haze, come out at almost the haze's
  own brightness, so there's nothing left to see. Colour and darker values on the ground (asphalt,
  trees, water, shadow), and thinner air in the core (pass 1.1), are what bring them back.
- **Cracks along the window edges.** A hairline of the clear colour shows where the far strips'
  ground meets the window glass (two meshes whose shared edges round differently).
- **Night is closest.** Lit windows and the far strips glittering overhead like a star field are
  already the anime shot. But every window is the same warm white and too many are lit, so towers
  read as LED panels. The lamps are hard square dots with no light around them, the streets are
  black, and nothing blooms. From the cap lift the street grid, which should be lines of light,
  is dark: each lamp's pool is a few metres wide and vanishes at distance.
- **Patterns alias.** Windows, paving and lamps are drawn by `step` in the shader with no filtering,
  so at distance they shimmer into moiré and speckle instead of settling to their average colour.
- **The canal is a hole past 350 m.** The ground leaves the channel open for the chunk's water, but
  only L0 chunks draw water. Beyond them the channel shows the clear colour: a black dashed line down
  the strip from the lift.
- **Empty at eye level.** No street lamps (their light is painted on the asphalt), no furniture or
  signs, low-poly cone trees only in parks and plazas, and nobody about but pilots and agents. The
  avenue's "tree-lined" pavements are flat green paint, so on the showcase's street camera the
  pilots stroll down what reads as a lawn 18 m wide. At night the street is black and they vanish
  into it: nothing lights the ground but the painted lamp spots.
- **Three strips, one look.** From the axis, Charter, Canal and Gardens are the same grey grid of
  boxes, though `STORY.md` gives each its own character (offices and money; depots and quays;
  orchards and terraces).
- **The canal reads as concrete.** The water is a flat grey-blue with nothing in it, and the quay's
  railing is a solid black slab 1.1 m high.
- **The window banks are a grid.** Up close the glass is a pale blue sheet ruled by its frame. It
  doesn't read as glass with space beyond it, and the strip rising past it reads as grey carpet.
- **The rooms are blown out.** The Exchange floor is a white box: ceiling panels clip to white, the
  street through the door is a white band across the floor, the boards are coloured blocks, and
  there's nothing in the room.

## What bounds it

- **WebGL2 is the default build.** It has one directional light, one shadow cascade (500 m), and no
  compute shaders. Bevy 0.19's SSAO, SSR, TAA, auto exposure, clustered decals and atmosphere need
  compute or bindless textures, so they're WebGPU-only or unavailable. Contact shadows and
  volumetric fog need a depth prepass and shadow maps; check each on WebGL2 before relying on it.
  So most of this plan is shader work in `city.wgsl` and its imports, and closed forms. The
  WebGPU tier gets extras on top: three cascades, light shafts, contact shadows.
- **Shaders never re-implement the layout** (`CLAUDE.md`). Anything painted from afar reads the
  block atlas (`bc::city`). New per-lot data (heights for long shadows, district materials) is
  baked by `bc_client_core::city_atlas` from the rules, the same way.
- **What's drawn is what's walked.** `every_vertex_lies_on_a_box_of_the_rules` holds every chunk
  vertex to a solid of the rules. Decoration that suits and walkers can't touch (small rooftop
  plant, signs flat on walls, lamp heads overhead) can come in as a new surface the test bounds
  separately. Anything a walker or a suit would collide with (lamp posts, kiosks, tower crowns,
  setbacks) is a layout change: it goes into `bc_sim::colony::city` and bumps `CITY_VERSION` with
  the protocol.
- **Anything everyone must agree on is a closed form of the tick**, like the trams: traffic,
  pedestrians, signals, weather. It is never sent and never stored, and it stays allocation-free
  and deterministic (`CITY_GOLDEN`, `no_alloc`).
- **Budgets** (`COLONY.md`, "Budgets"): triangles a chunk per level
  (`levels_get_lighter_and_stay_in_budget`), about 300 draw calls, and the mesh-building budget a
  frame. Instancing keeps street furniture and traffic inside the draw-call budget.

## The passes

Each pass is PR-sized and lands on its own, ordered by how much of the screen it improves for the
work. Every pass ends with the review set re-shot, before and after.

### Pass 0: what's broken (one small PR)

- **The canal past L0.** L1 to L3 chunks draw the channel's water too: one quad a block at the
  water's height, inside their triangle budgets.
- **The window cracks.** The glass runs a little past the strip's edge, below the ground, so the
  two meshes overlap instead of meeting edge to edge.
- **The city's shadows.** `light_city` sets the Sun's cascades for the city (pass 1.3 tunes them).
- **The showcase.** *Done* (with this plan): the shared visuals assumed a game client
  (`inside.rs`, `people::name_tags`), so every `?showcase=` scene panicked, the gfx e2e suite
  with them.

### Pass 1: air and light (every frame, every hour)

The largest gain for the least work: none of it adds geometry.

- **1.0 A day with a golden hour.** Reshape `colony::time::day` so the mirrors open and close
  over the whole day rather than snapping half open at dawn's end. Daylight and the mirrors' angle
  then follow one smooth curve, and the light rakes low along the axis for the first and last
  quarter of the day, not just for four minutes. The mirrors outside and the light inside share
  `mirror_beta`, so they stay in agreement. Only clients draw it, but it lives in `bc_sim` and the
  wasm determinism test hashes it, so that golden moves with it, along with its own tests
  (periodic, continuous).
- **1.1 One sky function** (`shaders/colony_sky.wgsl`, `bc::colony_sky`), shared by `city.wgsl`
  and `city_inside.wgsl`. It gives the colour and transmittance of the air along any ray in the
  colony, and the colour seen along a direction that leaves it:
  - **Haze by height.** The air itself thins only about 17% from the floor to the axis (the
    colony's 3.2 km against air's 8 km scale height). But haze isn't air: dust, moisture and the
    city's exhaust sit in a mixed layer near the ground, as aerosols do on Earth (scale height
    about a kilometre). So the haze's density falls off with height above the floor,
    `ρ(h) = ρ₀·exp(−h/H)` with `h = R − r` and `H` about 1 km, integrated along the ray in closed
    form or a few steps. Street-level distances then go blue and soft. A look straight up spends
    most of its 6 km in the clean core, so the city overhead stays legible instead of drowning in
    one uniform fog. That's both more correct than today's single `DistanceFog` density and the
    layered depth of an anime background.
  - **Scattering toward the light.** The haze brightens toward the window over the strip and
    warms there near dawn and dusk (Mie forward scattering).
  - **The colour script.** A small table in Rust (`colony::time` phase to haze colour, sun tint,
    sky light, exposure bias, grade), passed as a uniform, so the hours are tuned in one place.
    Small objects with Bevy's `StandardMaterial` (people, cars, trams) keep `DistanceFog`,
    matched to the sky function's near-ground density. Their distances are short enough that
    the two agree.
- **1.2 Reflections from the sky function.** Inside, `EnvironmentMapLight` is removed (`city.rs`),
  so the curtain walls are flat dark glass and the canal reflects nothing. Glass, water and (pass 3)
  wet pavement sample `colony_sky(reflect(v, n))` with Fresnel. The result: the white window bands,
  the far strips, the haze, the lamps at night. No cube map, no compute, and it works on WebGL2.
- **1.3 Shadows at the city's scale.** The city sets the Sun's `CascadeShadowConfig` itself on the
  way in (today it never does: see "Where it stands"). On WebGL2, one cascade sized to the camera,
  about 300 m on foot and more from a suit or a rooftop. On WebGPU, three cascades out to 3 km.
  Beyond the shadow map, **long shadows from a height atlas**: bake each lot's roof height
  (16 m texels, `city_atlas`) and march a few steps toward the key light in `city.wgsl`. At dawn and
  dusk the towers of the business district then lay shadows kilometres long across the strip.
  That's the most dramatic thing the colony's low sun can do, and a cheap texture march.
- **1.4 The grade and the bloom by the hour.** `ColorGrading` and `Bloom` follow the colour script
  (`light_city` already sets exposure by daylight). Night gets more bloom and a cooler grade with
  warm lights; golden hour gets lifted shadows tinted violet.
- **1.5 WebGPU extras.** `VolumetricFog` with the key light for shafts between towers at
  dawn and dusk, and contact shadows for feet, kerbs and benches.

### Pass 2: facades

65,000 buildings are boxes with a window grid painted on. Close up, that reads as a game from
2005.

- **2.0 Filtered patterns.** Every procedural pattern (windows, mullions, paving, lamp pools, lane
  lines) is antialiased by its footprint (`fwidth`), fading to its average where it's smaller than
  a pixel. Lamp pools also grow softer and wider with distance, so from a suit or the lift the
  streets read as lines of light. This comes first because everything after it adds more pattern.
- **2.1 Rooms behind the windows** (interior mapping): each window looks into a room with depth,
  back wall, floor and ceiling, from the wall's UVs and the view direction, no geometry. By night
  the rooms are lit at varied colour temperatures, with blinds half down and the odd silhouette.
  This is what sells a city at street level and at night, and it's a few dozen lines of WGSL.
- **2.2 Materials by district and strip.** The district is in the atlas already. Old Town gets
  brick, stone sills and cornice bands. Midtown gets tile and concrete with balconies painted in
  relief (normal tilt). The business district gets glass curtain walls with spandrel panels and
  pass 1's reflections. The Works get corrugated cladding and roller doors. Today a wall's tint is
  a hash across six greys and browns. Each strip gets a character of its own, as GTA's
  neighbourhoods have (`STORY.md`): Charter is stone and glass, offices and money. Canal is brick
  and steel, depots, cranes and locks. Gardens is render and timber, terraces, roof gardens and
  orchards. Looking up from any strip, the other two then read as different places, not as copies
  of the one you're standing in.
- **2.3 Ground floors.** Shopfronts with awnings, lit interiors (interior mapping again, deeper),
  roller shutters on some, and signs (pass 3).
- **2.4 Rooftops.** Plant rooms, water tanks, aerials with red obstruction lights (suits fly over
  the city: those lights are real airspace furniture), rooftop gardens, a helipad on some towers.
  As a decoration surface no taller than a few metres above the roof, bounded by the test.
- **2.5 Tower crowns and setbacks** (layout change, `CITY_VERSION`). Stepped tops, spires and
  crowns give the skyline the silhouettes an anime skyline is drawn with. Today every tower is a
  box on a podium.

### Pass 3: the street at eye level

- **3.1 Paint and wear on the ground** (`city_paint`): lane lines, crossings, stop lines, tram
  markings, manholes, patching and oil stains, tyre-darkened lanes, gutters.
- **3.2 Wet streets.** A closed form of the day for the colony's weather (rain from the
  sprinkler grid in the core, or just the street cleaners at night). While wet, roughness drops
  and puddles mirror the lamps and signs through pass 1.2. It's the most anime thing a night street
  can do.
- **3.3 Street furniture, instanced.** Lamp posts where the ground is painted with their pools
  today (the posts, the heads, and real light from the nearest few), traffic signals cycling on a
  closed form of the tick, tram shelters, benches, bollards, planters, kiosks, vending machines,
  fire hydrants, bins. Posts and kiosks are solid, so they're a layout change. A first cut can be
  overhead-only (lamp heads and wires, signals on gantries) to stay clear of the walker.
- **3.4 Trees.** The avenue's "tree-lined" pavements are green paint today. Plant two rows of
  trees down every avenue pavement and along the canal. Replace the octahedron crowns with
  clustered leaf cards (alpha-tested, swaying in the vertex shader), with impostors past L1.
- **3.5 The canal.** The water shows the sky function's reflections (pass 1.2) with ripples and
  the lamps' streaks at night, and the quays' railings are drawn as posts, rails and glass inside
  their solid box (no layout change: the test bounds vertices, not looks).
- **3.6 The rooms.** The Exchange floor, the Charter Board and The Arrival lit like rooms, not
  light boxes: panels that stop short of clipping, a door that shows the street without flooding
  the floor, desks, screens with legible rows, chairs and the bar's stools, and the odd figure
  at work.
- **3.7 Signs and neon.** Procedural shop signs (blades and fascias, glyph-like strokes from a
  hash, never a real brand) on Midtown and Old Town ground floors, lit at night. Holographic
  billboards on the business district's towers (`holo.wgsl` is the precedent). With pass 3.2 at
  night, this is the shot people screenshot.

### Pass 4: life (the GTA half)

A city with nobody in it reads as a model however good the light is. Only pilots and agents walk
it today.

- **4.1 Traffic as a closed form.** Cars on loops along the avenue's roads and the cross streets,
  each a function of the tick and its loop (spacing, speed, stops at signals in step with pass
  3.3). Like the trams, never sent, and the same on every screen. Drawn instanced, from a handful of
  car meshes and liveries. At night, from a rooftop or a suit, the avenue becomes rivers of white
  and red lights, the classic anime night-city shot. Pilots' cars pass through them at first;
  avoiding them is a later problem, and a solvable one, since the server can evaluate the same
  closed form.
- **4.2 Pedestrians as a closed form.** The same on the pavements: civilians (the figure, with
  clothes instead of flight suits and varied builds and colours), denser downtown and by day,
  thinning at night, gathering at stations when a train is due. Distant crowds become instanced
  impostors.
- **4.3 Small motion.** Birds over the parks, flags on the civic blocks, steam from vents, the
  canal's boats, fountains on the plazas, shop signs flickering.

### Pass 5: the colony as the sky

- **5.1 Clouds in the core.** A soft cloud layer in a band of radius (about 1.6 to 2.2 km), lit by
  the windows, drifting and changing with the hour. Real colonies would have them and Gundam's do.
  Seen from the street, they sit between you and the city overhead.
- **5.2 The city overhead by night.** The other strips' streets, windows and traffic glittering
  through the haze. The far strips' shading (`city.wgsl`, "elsewhere") already carries their
  lit windows. Pass 1's clean core lets them through, and pass 4's traffic lights add the
  moving part.
- **5.3 The windows as glass.** Up close the glass reflects the city faintly (the sky function
  again) and lets space show through: the mirror's glare by day, the Earth and the Moon wheeling
  past with the spin (`city_inside.wgsl` already turns the stars). The frame thins with distance
  instead of ruling the whole sky.
- **5.4 Three strips, three places.** The strips' identities (pass 2.2) carried to their ground
  from afar: the Gardens' orchards and terraces green and patterned, the Canal's water and yards,
  Charter's towers. Seen overhead through the haze, each strip is a different place.

### Pass 6: the lens

- **6.1 Light that flares.** Streaks on bright points at night (lamps, signals, tail lights), a
  glint where the window bands meet glass. Bloom carries most of it on WebGL2.
- **6.2 Photo mode.** A free camera with the hour as a slider and the showcase's cameras as
  presets; depth of field on WebGPU. Players will make the colony's marketing for us.

### Later: people and suits up close

Skinned civilians and pilots in place of the rigid 12-piece figure (`bc_client_core::figure`).
Suit-scale props at the building site (gantries, cranes worked by suits). The suits are already
lit by the strip's sun through Bevy's lighting, so they sit in the city's light as they are.

## Review set

The showcase's city cameras (`?showcase=city&cam=N`) are the review set, shot on High at noon
(`t=480`), the late afternoon (`t=1350`, the golden hour once pass 1.0 makes one), dusk (`t=1560`),
night (`t=1900`) and dawn (`t=2280`). New cameras land with
the passes that need them: the avenue at the golden hour toward the business district, a rooftop at
night, the canal at dusk in the wet, and a suit among the towers. Each pass's PR shows its before
and after from the same cameras. `scripts/e2e.sh gfx webgl2 --grep city` keeps them rendering
cleanly on every tier.

## Where to start

Pass 0, then pass 1. Beyond pass 0's canal water and glass edges they add no geometry, they stay
in a handful of files (`city.rs`, `city_mesh.rs`, `city.wgsl`, a new `colony_sky.wgsl`,
`colony::time`, `city_atlas.rs`), and they lift every frame at every hour. Pass 2.0 (filtering)
goes with them, since everything after adds pattern. After that, the order can follow
what the game needs next. Pass 4 (life) is the one that makes it feel like GTA rather than a
model, and it's independent of passes 2 and 3.

## Open questions for the owner

1. **How anime?** (a) Physically lit with an anime colour script, as above (recommended). (b) The
   same, plus outlines and a cel ramp on characters and suits only. (c) Toon shading throughout.
2. **Weather.** Is rain (or something like it) part of the colony? Pass 3.2 needs a reason
   for wet streets.
3. **Signs.** `STORY.md` has colony notices in plain English. Do shop signs follow that, or mix in
   other scripts?
4. **Ambient people.** Are client-side ghosts (everyone sees the same ones, nobody can touch them)
   acceptable as a first step?
