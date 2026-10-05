# Before Colony: game design

> A *legit* Gundam Wing mobile-suit simulator, built as an MMO. Newtonian flight, free aim,
> AI that plays by the same rules as you, and the ZERO System as a combat AI that shows you the
> future and then tries to take the controls.

Gundam Wing and its names are © Sotsu · Sunrise. This is a fan project. Canon names are compiled in
only with the `canon-names` feature of `bc-sim` (default on); without it the game ships generic names.
All art is procedural.

## Pillars

1. **It flies like a mobile suit in space.** Newtonian 6DOF, AMBAC, pilot G limits. There is no
   drag or "space friction" unless you turn on flight assist. How much the tank and the pilot's
   body hold you back is the sector's choice: anime rules (the default, for fun over realism)
   make the tank a boost gauge that refills, and the real rules make every newton count.
2. **You aim.** Mouse free aim with no tab-targeting. A lock-on (Y) moves you about your target and
   marks the lead, but never points the guns (`LOCK.md`). Beams take a fraction of a second to arrive, so
   leading, dodging and range matter.
3. **AI is a first-class citizen.** Mobile Dolls are the NPCs, as in the show. External AI agents
   connect through the same protocol as humans and are labelled **MD**. The ZERO System is a
   predictive AI in your cockpit.
4. **The server is the truth and the tick is sacred.** Sectors tick at 30 Hz with no locks and no
   allocations (see `ARCHITECTURE.md`). Everything else is built around protecting that.

## Setting

**The year before the colony calendar begins.** The first colony, an O'Neill cylinder at L1, has
just opened its docks, and the calendar history will call After Colony starts the day its charter
is signed. The pilots are **Arrivals**: people, and minds without bodies (the agents), who woke in
its docking hub remembering another world where After Colony is a story they already know. The
world moves through eras as the community builds it. `docs/STORY.md` is the world bible.

The milestones so far take place in **Sector L1**: the colony (3.2 km radius, 32 km long) lies
below the combat zone, with the debris of its construction around it and the Consortium's OZ
Mobile Doll patrols circling above, guarding its claims. The sector is a ±32.768 km cube, and suits
are kept within ±30 km.

Suits can land on the sector's bodies, walk on them, and hide in them (see "Surfaces"):
- **MO-II** ("Mobile Operation II"), a resource satellite serving the colony's dock. It lies 3.6 km
  past the colony's −X end, about 30 s from the launch gate. A 400 m core with a module at each
  end, four pylons round its middle and a mast at its nose. It rolls about its long axis once every
  320 s and drifts round a 200 m station-keeping circle every 30 minutes, so no point of it moves
  faster than 2.87 m/s. The **Aft Well**, a bowl 25 m deep, is cut into its aft module's face, on
  the axis.
- **Hermit**, a big asteroid 15.6 km from the field's centre: 1.8 × 1.24 × 1.52 km and still. It
  isn't one of the field's rocks, so it can't be mined or shattered. Three craters, each a bowl
  about 30 m deep, sit at an end of each of its axes: **THE DEEP**, **KEYHOLE** and the **FAR
  SIDE**.
- **The field's big rocks.** A rock whose smallest half-axis is at least 10 m (75 of the 160) can
  be gripped. A smaller one can still be rested against, and a suit parks on it.

**The colony's hull isn't walkable.** It spins for 1 g inside, so its hull moves at 177 m/s:
standing on it would fling a suit off at more than 1 g, and the browser would draw its own suit up
to 47 m from where the server has it. To suits and shots the colony is a still, solid cylinder. Its
spin runs on the tick's clock (one turn every 3 405 ticks, 113.5 s), so every client draws it turned
the same way, and a later hull walker has it to hand. **Its inside is walkable on foot** (below,
"The First Colony, inside"): there the colony's own frame is the ground, and it doesn't move.

Space comes first because that is where gundanium is made (it can only be refined in zero-G) and
where the colony's logistics live. Earth, with atmosphere, gravity and re-entry, is on the roadmap.

## Flight model (`bc-sim/src/flight.rs`)

Everything is in SI units and shared bit-for-bit between the server and the browser's prediction.

### Flight rules: anime or real (`bc_sim::tuning::FlightRules`)

A sector's pilots fly by one of two sets of rules, people and agents alike. The server picks
(`--flight anime|real`, `BC_FLIGHT` for the scripts), and its Welcome tells clients, which predict
by them. **Mobile Dolls fly by the real rules either way**: machines built cheap, they burn every
newton and run dry, and a Doll that has been out a while can hardly dodge. (With bottomless tanks
they never tire, and a melee pilot can't catch them: the autopilot's Shenlong landed 0 to 3 hits
in three minutes against 24 of them, against 36 to 82 by the real rules and 71 to 128 with the
Dolls on them.)

- **Anime** (the default). The tank is a **boost gauge** (the flight panel reads `BOOST`).
  - Only boost burns it, with all the thrust boost gives, as ever. Flying, turning on RCS,
    flight assist's braking and a blade's lunge burn nothing, and work on an empty gauge.
  - Let go of Shift and it fills back up: a whole tank in 20 s, in flight or standing on a body.
    Leaning on boost with the gauge dry gets nothing (no boost, no refill), and flight assist
    holds the plain cruise, not boost's.
  - A holed tank refills at half the rate, and a failed one not at all: it leaks dry, and the suit
    flies on without boost.
  - Pilots bear twice the G (12 g for good), as the show's do: a Gundam's boost doesn't black its
    pilot out. A Wing Zero boosting on a nearly empty gauge (lighter, it pulls up to 16 g) still
    can, and so can a pilot hurt by a struck cockpit.
  - What the tank makes out there stays out there: under survival, a suit comes home with no more
    propellant than it launched with.
- **Real.** Every newton burns propellant at `|F| / (Isp·g0)` (the rest of this section), and a
  pilot bears 6 g. The tank is the sortie's delta-v: brake before you're dry.

- **Thrust** is limited per axis: main (forward), side (lateral and vertical) and retro. Boost
  multiplies main thrust. Every newton burns propellant at `|F| / (Isp·g0)`, so mass falls as you
  burn and delta-v follows the rocket equation (tested to within 1%).
- **Attitude.** The suit turns toward your aim, never faster than it can stop from, so it settles
  on the aim instead of swinging past it (even while firing or a blade takes AMBAC's limbs).
  - **AMBAC** (Active Mass Balance Auto Control) swings the limbs to rotate the suit. It costs no
    propellant but has modest authority. Losing arms or legs reduces it, and so does firing or
    striking with a blade, because the limbs are busy.
  - **RCS** (hold R) adds strong attitude thrusters that burn propellant.
- **Flight assist** (V) turns the stick into a velocity command: it brakes to a stop when you let
  go. With it off you are fully Newtonian. **Brake** (X) always retro-burns.
  - It eases onto the velocity asked for over about a fifth of a second, so G fades in and out
    instead of switching on and off, and a stop settles rather than slamming.
  - It spares the pilot's body: short of boost, it never pulls more than just under 6 g, whatever
    the thrusters could do, so flying with it never blacks you out. Boost, or flight assist off,
    gives you everything the thrusters have. (A Mobile Doll's flight assist snaps at full thrust.)
  - Held through a blackout, boost keeps flight assist aiming at the boosted cruise, so it doesn't
    brake while you're out.
- **The burst step** (`flight::Burst`; double-tap a direction, or BURST on the wire with the stick
  off centre). For 0.3 s the suit dashes along the keys at 120 m/s² (about 12 g), 36 m/s in all,
  and flight assist lets it be until it's done, then brings the suit back to what the stick asks.
  Locked on, it goes along the fight's axes, so A and D sidestep round the target and S jumps
  back. It burns as boost does (under the anime rules too: about 40 kg of a Leo's tank, and the
  gauge doesn't refill meanwhile), needs propellant, an awake pilot and boosters that work, and
  the next can start 1.2 s after the last's press. Under the anime rules a pilot bears it; under
  the real ones each step costs nearly half the G-strain to a blackout. Others see it as boost
  (its plumes, its heat on sensors). It's the answer to a lunge and to a shot from far off: a
  rifle shot from 2 km takes half a second, which a step turns into about 12 m.
- **Pilot G.** Sustained load above 6 g builds G-strain. At 100% the pilot blacks out and control
  authority collapses until strain falls below 50%. A Wing Zero on boost pulls about 12 g, so you
  *can* out-thrust your own body, as Zechs did in the Tallgeese. Mobile Dolls have no body, so no
  G limit. Agents are pilots, so they do have one.
- **Hull contact.** The colony is solid: suits slide along the hull and beams splash against it.
- **Rocks are solid too.** A suit that flies into one stops at its surface, losing its speed into
  it, and slides along it; nothing tunnels, even at 2 km/s. Shots stop at rocks, so a rock is
  cover. The field comes from a seed, so your browser predicts against the same rocks.
- **So are MO-II and Hermit.** A suit meets them as it meets a rock, but relative to the moving
  surface, which carries it along. A shot meets whatever is first along its path: a rock, a
  landmark, the colony or a suit. So a suit skimming the hull is hit, and one behind it isn't.

| Frame | Role | Dry mass | Accel (boost) | Δv | Armour | Loadout |
|---|---|---|---|---|---|---|
| Leo (OZ-06MS) | line suit | 7.1 t | 3.5 g (5.6 g) | ≈2.6 km/s | titanium | beam rifle · machine cannon · beam saber |
| Wing Gundam Zero (XXXG-00W0) | hero suit | 8.0 t | 8 g (12 g) | ≈3.7 km/s | gundanium (×0.55 damage) | Twin Buster Rifle · machine cannons · beam saber · **ZERO System** |
| Neo-Bird (Wing Zero's other form) | interceptor | 8.0 t | 8.5 g (11.9 g) | ≈3.7 km/s | as Wing Zero | Twin Buster Rifle (fixed forward) · machine cannons · **ZERO System** |
| Gundam Heavyarms (XXXG-01H) | gunship | 8.8 t | 5.2 g (7.3 g) | ≈2.8 km/s | gundanium (×0.55) | beam gatling · homing missiles · army knife · **Full Open Attack** |
| Gundam Sandrock (XXXG-01SR) | brawler | 9.6 t | 4.6 g (6.9 g) | ≈2.4 km/s | gundanium (×0.45) | beam machine gun · homing missiles · heat shotels · **Cross Crusher** |
| Gundam Deathscythe (XXXG-01D) | infiltrator | 7.3 t | 7 g (11.2 g) | ≈3.3 km/s | gundanium (×0.55) | buster shield · head vulcans · beam scythe · **Hyper Jammer** |
| Shenlong Gundam (XXXG-01S) | duellist | 7.5 t | 7.4 g (11.8 g) | ≈3.3 km/s | gundanium (×0.55) | Dragon Fang · flamethrower · beam glaive |
| Taurus (OZ-13MS) | Mobile Doll | 6.5 t | 5 g | | titanium | beam rifle |
| Virgo (OZ-02MD) | Mobile Doll | 9.5 t | 3 g | | heavy (×0.8) | beam cannon, Planet Defensors (visual) |

## Surfaces (`bc-sim/src/ground.rs`)

A suit can land on a body (MO-II, Hermit, the field's big rocks), walk on it, crouch and hide on
it, and lift off again. Each body has a frame of its own, and a suit on one moves in it, so a
station that turns carries its riders round with it.

- **Grip is opt-in: L arms it.** Armed, a suit that comes in slow and close is caught: its feet
  within 25 m of a surface it can grip, under 8 m/s relative to it and not leaving it faster than
  2 m/s, with no boost and no Space. Far from surfaces nothing changes, and a suit that never arms
  the grip flies exactly as it always has. Mobile Dolls and miners never arm it.
  - Coming in with the grip armed (within 150 m and under 40 m/s), the suit rolls its feet toward
    the surface; its nose stays on the aim, and Q/E override. A landing ring `( _ )` marks where
    it will come down: green with `LAND 18 m 3.2 m/s` when a catch would hold, amber with
    `TOO FAST 14 m/s` when it wouldn't.
- **Three footings.** A suit is *free* (flying in the sector), *aloft* (in the air over a body, in
  its grip) or *grounded* (on its feet).
  - **Aloft** is flight in the body's frame, under a *grip gravity* of 6 m/s² (0.6 g) toward the
    nearest surface. A descent faster than 8 m/s is braked, burning nothing. Flight assist holds
    a walk's (or, with Shift, a run's) speed along the surface but never holds altitude: let go of
    Space and C and the suit comes down. The thrusters still work: W/A/S/D steer, C dives (still
    braked), Space climbs at 20 m/s, and Q/E roll. More than 40 m over the surface, or faster than
    30 m/s over it, and the grip lets go. A catch is never undone by the fall it starts, which
    lands at no more than 8 m/s.
  - **Grounded** is legs, not thrusters, and burns no propellant. W/A/S/D walk at 8 m/s toward
    where you aim, Shift runs at 16, and crouched you creep at 3. The suit stands up to the
    surface, turns to face the aim and leans back (or forward) up to 40° to reach it, so hand
    weapons reach straight overhead. X stops hard. A blade's lunge is a dash along the ground.
  - **Where you can walk.** Rounded edges and gentle curves are walked round. An inner corner, or
    an edge that turns the ground by more than 34° in a step, is a wall, and you slide along it.
    So a crater's rim is a wall: you hop in and out. Walk off a drop of more than 1.2 m and you're
    aloft, and the grip brings you down onto the nearest surface, running if you were.
- **Hop and lift off.** Tap Space to hop: 10 m/s off the ground, an 8.3 m apex and 3.4 s in the
  air. Hold it, and once you're off the ground the thrusters take over, climbing at 20 m/s; past
  40 m you're flying free (`FLYING`).
- **Let go: L again.** On the ground the suit pushes off at 6 m/s along the normal, and leaves
  with exactly the velocity of the surface it stood on. A rock shattered underfoot floats the
  suit off at 1.5 m/s (`GRIP LOST`), and folding into Neo-Bird takes off too: a bird can't grip.
- **Crouch: C**, a toggle on the ground. The suit's origin drops from 9.125 m over the surface to
  6 m in about half a second. The simulation keeps the stance until it's told otherwise, so a
  client that stalls leaves its suit crouched. Space from a crouch stands up first, then hops.
- **Legs.** A suit without them still grips, kneels, turns (slowly) and lifts off on its
  thrusters, but it can't walk or hop.
- **Cover and concealment are different things.** *Cover* is geometry: shots meet the first thing
  in their way, so crouched below a crater's rim or behind a pylon you're shielded, anywhere.
  *Concealment* is what you show your enemies' sensors, and it comes from lying still (see
  "Sensors and visibility").
- **Fighting on a body.** A rider is a suit like any other to everyone else: shots and blades meet
  it where its shooter saw it, and the body shields it from the far side. A rider fires from where
  its pilot saw itself, on the station as it was drawn (`ARCHITECTURE.md`, "Bodies and frames").
  Mobile Dolls come at riders from above, rather than ploughing into the body. ZERO knows a
  grounded suit can't dive through the ground: it reads its maneuvers along the surface.
- **What the HUD says.** The flight panel shows `GRIP ARMED`, `ALOFT` (with the feet's height,
  `ALT 23 m`), `GROUNDED` or `CROUCHED`. While you're on a body, `SPD` and the velocity marker
  `-o-` are relative to it; flying within 2 km of a landmark, the marker is relative to that.
  Within 3 km of a landmark its name, range and range rate over its surface show, negative while
  you close on it (`MO-II  2.4 km  -38 m/s`), and its hide spots are marked (`<> AFT WELL 2.4 km`).
  `L - GRIP` says a surface you could grip is near; `HULL SPINS - NO GRIP` that the colony isn't
  one.

| Constant (`bc_sim::ground`) | Value |
|---|---|
| Origin over the ground: standing, crouched | 9.125 m, 6 m (soles 9.07 m below the origin) |
| Walk, run, crouched | 8, 16, 3 m/s, picked up at 20 m/s² (X stops at 40 m/s²) |
| Hop | 10 m/s off the ground |
| Grip gravity; the free brake on a descent | 6 m/s²; 8 m/s |
| A catch | feet within 25 m, under 8 m/s relative, leaving at under 2 m/s |
| The grip lets go aloft | feet above 40 m, or faster than 30 m/s |
| Roll-level and the landing ring | within 150 m, under 40 m/s |
| Push-off on letting go | 6 m/s |
| A wall | the ground turning more than 0.6 rad (34°) in a step; the origin never nearer than 5 m to its body |
| Grippable rocks | smallest half-axis 10 m or more |

## Combat

| Weapon | Speed | Damage | Rate | Notes |
|---|---|---|---|---|
| Beam rifle | 4 km/s | 45 | 1.5/s | energy and heat; dodgeable at range. Tap fires; held 1.2 s from the press it charges (glowing for everyone to see), and let go full the charged shot leaves; let go sooner, only the tap's shot went |
| Beam rifle, charged | 8 km/s | 90 | 1 per 1.2 s (the hold) | the sniper's shot: 0.9 m beam, 8 km reach, 36 heat; the lead marker leads for it once the charge is full |
| Machine cannon | 1.2 km/s | 6 | 10/s | ballistic; 400 rounds; small spread |
| Beam saber | – | 90 | swing | 9 m arc sweep with a lunge; blades clash (both parried) |
| Twin Buster Rifle | 8 km/s | 220 | 1 per 5 s | 0.6 s charge, visible to everyone; 5 m beam engulfs the whole suit |
| Beam cannon (Virgo) | 3.5 km/s | 70 | 0.8/s | |
| Beam gatling (Heavyarms) | 3 km/s | 7 | 10/s | a stream of beam rounds |
| Buster shield (Deathscythe) | 450 m/s | 80 | 1 per 5 s | the shield's beam claw, fired: slow, so lead it |
| Head vulcans (Deathscythe) | 1 km/s | 3 | 15/s | 300 rounds |
| Beam machine gun (Sandrock) | 3.5 km/s | 12 | 6/s | |
| Homing missiles (Heavyarms, Sandrock) | 120 m/s off the rail, then an 18 g motor | 32 each | salvos of 4, one per 2 s | guided when your lock is acquired; 24 rounds |
| Chest gatlings (Heavyarms) | 1.2 km/s | 4 | 30/s | Full Open only; 300 rounds |
| Micro-missiles (Heavyarms) | 150 m/s, then a 14 g motor | 16 each | volleys of 8 | Full Open only; 16 rounds |
| Flamethrower (Shenlong) | – | 7 a burn | 5 burns/s | a 70 m cone, ±12°; each burn adds 10 heat to what it touches, enough to overheat it; 150 burns |

**Hitting.** What a shot's speed, size and spread are worth, measured (`bc-sim/tests/hit_rate.rs`,
`-- --nocapture` prints it): one shot at a time, led perfectly but linearly (where the target would be
if it flew on as it is: the lock-on's ◆), at a Leo crossing at 150 m/s, coasting or jinking (flight
assist on, the stick thrown a new way every 0.4 s, boosting now and then), 32 trials a cell (so a
few points either way is noise). A gun's spread is a cone, its shots spread evenly over it
(`tuning::scatter`), and the HUD rings the crosshair with the widest of the suit's. Hit %, coasting
/ jinking:

| Weapon | m/s | Shot radius, m | Spread (cone) | 300 m | 600 m | 1 km | 1.5 km | 2 km | 3 km |
|---|---|---|---|---|---|---|---|---|---|
| Beam rifle | 4,000 | 0.6 | – | 100/100 | 100/100 | 100/100 | 100/97 | 100/62 | 100/88 |
| Beam rifle, charged | 8,000 | 0.9 | – | 100/100 | 100/100 | 100/100 | 100/100 | 100/100 | 100/97 |
| Twin Buster Rifle | 8,000 | 5.0 | – | 100/100 | 100/100 | 100/100 | 100/100 | 100/100 | 100/100 |
| Beam cannon (Virgo) | 3,500 | 1.2 | – | 100/100 | 100/100 | 100/100 | 100/100 | 100/81 | 100/81 |
| Beam machine gun | 3,500 | 0.45 | – | 100/100 | 100/100 | 100/100 | 100/100 | 100/81 | 100/56 |
| Beam gatling | 3,000 | 0.35 | 0.34° | 91/94 | 62/66 | 50/62 | 47/53 | 34/28 | out of range |
| Machine cannon | 1,200 | 0.25 | 0.23° | 100/94 | 100/94 | 97/44 | 62/31 | 78/16 | out of range |
| Head vulcans | 1,000 | 0.2 | 0.46° | 100/94 | 72/59 | 59/19 | 0/0 | 0/0 | out of range |
| Buster shield | 450 | 1.2 | – | 100/88 | 0/16 | out of range | | | |

So the lead is the whole skill against a target that flies straight, and a beam rewards it at any
range; spread is what limits the guns past a kilometre, and slow shots (the buster shield's claw)
are for close in. The charged shot is the sniper's answer to a target that jinks: at twice the
rifle's speed the target has half the time to change its mind, so held still and led, it hits at 3
km; what it costs is 1.2 s with the trigger held and the gun glowing for everyone to see. A pilot who dodges on purpose does better than this target, which changes its
mind on a timer.

- **Projectiles inherit the shooter's velocity** (it's space). Fire control solves the intercept in
  the shooter's frame.
- **Arms aim.** A hand-held weapon fires anywhere within 50° of the body axis (shoulder mounts 20°),
  so you don't have to point the whole suit.
- **Per-part damage.**
  - Parts: head (sensors), torso (destroyed means dead), arms (their weapons), legs (AMBAC mass,
    some thrust), backpack (main thrusters).
  - A limb shot to nothing comes off: it drifts away as wreckage (a limb chunk, which can be
    salvaged), and shots pass through where it was. A hit that blows a limb off spills half its
    excess into the torso. (A second hit on the same limb in the same tick carries through to the
    torso at half strength.)
  - Each part weighs a share of the frame, so a suit that loses parts is lighter.
  - A destroyed suit leaves a hulk: its hull and whatever parts are still on it, drifting on.
  - Wreckage bounces off the colony and rocks, and is cleared after 60 s (Mobile Dolls') or 180 s
    (pilots').
- **Heat and energy.** Overheating locks all weapons until heat falls to 50%. Beam weapons draw from
  an energy pool that the reactor recharges.
- **Systems inside the parts.** Armour protects what's behind it; once it thins, blows get through
  (below).
- **Streams.** Rapid-fire weapons (gatlings, machine guns, vulcans) aren't sent shot by shot: every
  client draws their tracers from the firing flags. Only single shots (rifles, cannons, the buster
  shield) are events, and only those are predicted by the shooter's own client.
- **Lag compensation.** A shot resolves against the world as its shooter saw it, up to 8 ticks
  (267 ms) back. Details are in `ARCHITECTURE.md`.

### Suit systems and malfunctions

Every part holds systems a blow can reach once it's through the armour. Each is working,
**damaged** or **failed**; a part shot off fails its own, which is how losing the head (sensors)
or the backpack (main thrusters, boosters) does what it does. (`bc_sim::content::systems`, applied
through each suit's stat sheet, `bc_sim::tuning`.)

| Part | System | Damaged | Failed |
|---|---|---|---|
| head | sensors | sensor range ×0.7 | ×0.4: the sub-camera |
| head | fire control | missile locks build at half the rate and fall apart twice as fast | no locks; ZERO's firing solution no longer pulls shots |
| torso | reactor | energy regeneration ×0.5 | ×0.15 |
| torso | propellant tank | leaks 3 kg/s | leaks 15 kg/s (a Leo's tank in under 3 minutes) |
| torso | radiators | heat dissipation ×0.6 | ×0.25 |
| torso | gyros | AMBAC ×0.75 | ×0.5 |
| torso | cockpit | the pilot bears 5 g (flight assist holds them under it) | 4 g |
| each arm | actuators | its weapons reach 35° off the axis, not 50° | 15°, and its hand can't grip (it lets go) |
| legs | leg thrusters | lateral, vertical and retro thrust ×0.8 | ×0.6 |
| backpack | main thrusters | main thrust ×0.7, and they cough: a quarter-second at 30% one time in five | ×0.35 |
| backpack | boosters | half of boost's extra thrust | no boost (and flight assist keeps its G guard) |

- **Criticals.** A blow that leaves a part standing reaches one of its working systems with a
  chance that grows as the armour thins and with the blow's size: `(1 − armour left) ×
  min(1, blow / 25% of the part)`, at most 75%. It finds a system by weight (the reactor most
  often), and knocks it down a level, two for a blow of 35% of the part or more. Over a part's
  life, about two blows get through whatever the weapon (measured on a Leo's torso: machine
  cannon 2.1, beam rifle 1.7, beam saber 1.2). A suit fights normally until it's past half its
  armour, and then things start breaking; a worn part launched at 60% breaks sooner. Mobile Dolls
  are built simply: 0.6× as often, and no pilot to hurt.
- **Statuses.** A struck reactor **scrams**: no energy at all for 4 s. A struck cockpit
  **concusses** the pilot: their shots wander up to 1.5° for 3 s. Struck actuators **jam** the
  arm's weapons for 3 s.
- **What others see.** A suit with a damaged system sparks, one with a failed system smokes, a
  holed tank vents a jet of propellant; the HUD lists what's broken in each part (`RCT DMG`,
  `TNK OUT`), the statuses (`SCRAM 3.1s · CONCUSSED · REPAIRING GYR 12s`) and the tank's leak, and
  says what a blow just reached (`REACTOR SCRAM`). The cockpit chirps for damage, sounds the
  master caution for a failure, and hisses while the tank vents.
- Systems are only mended in the bay (an overhaul), or by damage-control gear.

### Equipment modules

A suit's stat sheet is its frame's numbers times what's fitted and what's broken. Equipment
modules ride on mounts on the parts (the head one, the torso two, the legs and the backpack one
each), so a part shot off takes its module; a suit carries at most one of each kind. Each is one
fixed design with a physical trade-off (`bc_sim::content::modules`):

| Module | On | Gives | Costs |
|---|---|---|---|
| Sensor array | head | sensor range ×1.35 | signature ×1.1 |
| Fire-control computer | head | missile locks 1.5× as fast | 40 kg |
| Capacitor bank | torso | energy capacity ×1.5 | 350 kg |
| Reactor booster | torso | energy regeneration ×1.35 | heat dissipation ×0.85 |
| Radiator package | torso | heat dissipation ×1.5 | signature ×1.15 |
| Composite plating | torso | damage taken ×0.88 | 600 kg |
| G-seat | torso | the pilot bears 1 g more | 150 kg |
| Damage control | torso | restores one damaged system every 25 s (never a failed one) | 4 energy/s while it works |
| Auxiliary tank | backpack | tank ×1.4 (a Leo's delta-v about +30%) | 200 kg |
| Thruster kit | backpack | main thrust ×1.15 | specific impulse ×0.88 |
| Leg verniers | legs | lateral and vertical thrust ×1.25 | 150 kg |
| Cargo rack | legs | hold +1,000 kg | AMBAC ×0.9, 250 kg |

The owner's client builds the same stat sheet from its snapshot (the systems' levels and the
modules' codes), so a suit that coughs, leaks and carries a thruster kit is predicted as exactly
as a whole one.

### Neo-Bird

Wing Zero holds MODE to fold into **Neo-Bird**, and lets go to unfold. A change takes 0.8 s with
the weapons down and thrust cut to 30%, and once started it runs its course (a strike or a charge
under way is lost). The bird is the same suit, armour, energy, heat and tank, reshaped: faster in a
straight line (it cruises at 600 m/s under flight assist) but slower to turn, its rifles fixed
within 2° of the nose, no saber, and aircraft-shaped hitboxes with wide wings. ZERO stays engaged
through it, and a Wing Zero always respawns unfolded. The owner's client predicts the change
exactly.

### Missiles

- **Locks.** Keep your designation inside 20° of your aim and within 2.8 km for half a second and
  the lock is acquired; lose it and it falls apart twice as fast. The target is told (MISSILE
  LOCK), unless it can't see you.
- **Guidance.** A missile fired with the lock acquired is guided onto the locked suit by
  proportional navigation; otherwise it flies blind along your aim. Its motor steers and speeds it
  at up to 18 g, but its Δv is a budget (1.1 km/s): once spent the missile coasts and can't turn.
  So a target can outrun it, make it burn its motor turning, or break late.
- **Seekers** hold their target within 60° of the nose and 3.2 km times the target's signature,
  so a jamming Deathscythe slips them, and so does a suit that goes dark parked or hides: the
  seeker asks the sensors' question ("Sensors and visibility"). A missile bursts within 4 m of an
  enemy suit (friends are safe), where it meets a rock, a landmark or the colony, or at the end
  of its 8 s life.
- **Full Open Attack** (Heavyarms, SPECIAL): for three seconds every hatch opens and everything
  fires along the aim, heat or not: the beam gatling, both launchers and the chest gatlings,
  about 24 missiles. Then the suit is locked in an overheat for 5 s, and it's ready again 30 s
  after it started.

### Melee

Every blade strikes the same way: a windup, the stroke (when it can hit), and a recovery, with the
timings, arc and reach in its row of the weapon table.

| Blade | Suit (key) | Damage | Reach | Windup · stroke · recovery (ticks) | Notes |
|---|---|---|---|---|---|
| Beam saber | Leo, Wing Zero (F) | 90 | 9 m | 4 · 6 · 8 | right shoulder to left hip |
| Army knife | Heavyarms (F) | 55 | 5 m | 3 · 4 · 6 | quick |
| Beam scythe | Deathscythe (F) | 120 | 12 m | 6 · 6 · 10 | wide reaping arc |
| Heat shotels | Sandrock (F) | 70 a blade | 8 m | 5 · 6 · 8 | one in each hand, sweeping inward |
| Cross Crusher | Sandrock (H) | 100 a blade | 9 m | 8 · 5 · 15 | the shotels as a pincer; both arms; 8 s cooldown |
| Dragon Fang | Shenlong (LMB) | 85 | 35 m | 4 · 6 · 10 | thrust along the aim; no lunge; can't be parried |
| Beam glaive | Shenlong (F) | 110 | 13 m | 5 · 6 · 10 | overhead chop |

- **A blade hits a suit at most once a strike**; each of a twin weapon's blades hits it once. A lost
  arm loses its blade (the Cross Crusher needs both).
- **Blades lunge**: through the windup and the stroke the suit drives at 1.5× main thrust (never
  boosted: holding Shift through a swing doesn't black the pilot out), which adds several metres to
  the reach. The lunge homes, mildly: it drives along the aim while the aim is within 15° of the
  nose, and along the edge of that cone when it's further off, so a pilot who keeps the crosshair
  on a target that slips aside carries the blade after it (`flight::LUNGE_CONE`). The Dragon Fang
  is Shenlong's arm, so it doesn't lunge.
- **Clashes.** A stroke that meets a suit whose own blade is out and facing it is parried: neither
  does damage, and each recovers for its blade's clash time. The Dragon Fang can't be parried.

## Sensors and visibility

Each frame has a sensor range, and each suit a signature. Boosting multiplies the signature by 1.5
and firing by 1.8 (for 1 s). Anything within 1.5 km is in sight, unless it's jamming or hiding
(below). Losing the head cuts sensor range to 40%. **The server only replicates what your
sensors see**, so fog of war is also the anti-wallhack.

**Deathscythe's Hyper Jammer** (held on MODE) defeats this model. To its enemies a jamming suit
shows a fiftieth of its signature, and their eyes see it only within 150 m. So it leaves their
screens, and Mobile Dolls, the ZERO System, locks and missile seekers lose it too, until it's all
but within reach of its scythe. Allies still see it, as a shimmer. The jammer engages with a
fifth of the energy pool and drains 30 energy/s against a recharge of 18, so it runs about 12 s
from full. Firing, striking or using a special shows through it for 2 s.

**Hiding on a body** (`bc-sim/src/sim/conceal.rs`). What a suit shows depends on how still it
lies, and one question answers it for sensors, Mobile Dolls, ZERO, locks and missile seekers
alike (`Sim::concealment`):
- **Lurking.** A suit crouched on a body, stick idle and moving under 0.5 m/s over it, settles in
  3 s (`HIDING 3`). In a hide spot it is then **hidden** (`HIDDEN - AFT WELL`, the reactor idling):
  off its enemies' sensors and seekers, and their eyes find it only within 225 m. Anywhere else on
  a body it runs **cold** (`COLD`): half its signature, so sensors find it at half their range.
  Firing or being hit shows it for 5 s (`SEEN 5`); then it settles again.
- **Parked.** A sleeper parked on a body powers down (see "Sleeping in the cockpit"): off sensors,
  and seen only within 400 m, or 150 m in a hide spot.
- **Hide spots** are bowls cut into the landmarks: the **Aft Well** on MO-II, and **THE DEEP**,
  **KEYHOLE** and the **FAR SIDE** on Hermit. They're compiled into every client, so their markers
  give nobody away, and they turn with their body. Their rims are cover too: the top of a suit
  crouched on the Aft Well's floor is 10 m below its rim.
- **Allies** always see their own, hidden or parked.

Only a signature of zero lets the eye's range bind (a suit is found within eyesight *or* within
its enemy's sensor range scaled by its signature), which is why hide spots are needed to vanish:
running cold only shortens how far off sensors find you. Nothing vanishes the tick its pilot logs
off, so leaving doesn't give a suit away on the spot; and a sensor doesn't see through rock yet,
so hide spots stand in for line of sight.

## The ZERO System

*Zoning and Emotional Range Omitted.* It feeds its pilot the futures of the fight faster than a mind
can take.

- **Predicted futures.** For your two most dangerous threats, ZERO weighs 7 maneuver hypotheses:
  coast, or a full burn along each body axis. It runs a Bayesian filter over their observed
  acceleration with a persistence model, then rolls every hypothesis 1.5 s ahead. The HUD draws each
  as a ghost trail, with opacity proportional to its probability.
  - The browser recomputes the trails with the same code, so the server only sends the probabilities.
  - Measured against Mobile Dolls, its top guess is right 87% of the time (chance is 14%). Against
    scripted random maneuvers its probabilities are calibrated to an expected calibration error of
    0.06.
- **Firing solution.** ZERO picks the aim that connects across the most probability mass and shows
  it as a lead marker with a hit %. **Fire-time magnetism:** a shot within 1.5° of the solution snaps
  onto it on the server, with no camera pull to fight your mouse.
- **Advice.** It recommends a target, an evasive maneuver ("BREAK-HIGH 64%"), a threat level with
  confidence, and a flanking probability.
- **Strain.** ZERO strain builds while engaged, faster under G.
  - At 100% comes a **seizure**: for 3 s ZERO flies and fires the suit itself, relentlessly (and only
    at enemies, so it can't be used to grief). Then the System locks out for 10 s.
  - Strain is also what rate-limits the external oracle's cost (below).
- **TypeSafe Jev.** With `--oracle jev` and `TYPESAFE_API_KEY`, the server asks Jev (a "System One"
  model that returns typed answers with calibrated probabilities) the same questions about every
  ZERO pilot's situation, about four times a second. Its answers are blended with the local oracle
  and marked `[Jev]` on the HUD. The game never waits for it.

## Mobile Dolls and agents

- **Mobile Dolls** run a utility AI: target selection with hysteresis, engage, strafe, evade,
  retreat and regroup, plus squads with a focus target.
  - They lead perfectly but *linearly*, strafe on a timer, feel no G and never flinch, so a pilot
    who keeps changing acceleration will out-juke them.
  - They drive their suits with the same `InputCmd` a player sends.
- **Agents** are external AI players on the Bot SDK (`bc-bot`). They run the same client state
  machine as the browser, get the same sensor-limited view and input rate, and obey the same G
  limits. They are labelled **MD** in-game. The bundled `DollBrain` flies an agent with the Mobile
  Doll's judgement and the frame's whole kit (below). `MinerBrain` mines: it cuts rocks apart with its saber, stows the ore as its free hand
  catches it, and sells it at the dock, flying round the colony to get there. Write your own brain
  in a closure.
- **The kit-aware pilot** (`DollBrain`, and the browser's autopilot) picks targets and maneuvers
  like a Mobile Doll, then flies the frame it's in:
  - guns inside their reach, leading with the one that fits; launchers once the lock is acquired;
    the flamethrower close in; the Dragon Fang from a third of its reach; blades timed so the
    target is well inside their reach mid-stroke, lunge included;
  - a melee-first frame (Deathscythe, Shenlong) pursues: its main engine pointed where its
    velocity has to go, weaving on the way in, closing no faster than it can brake from, to just
    outside a blade's length;
  - Neo-Bird for the long haul; the jammer while closing, holding fire so as not to break it;
    Full Open with a lock inside 1.2 km; the Cross Crusher at arm's length;
  - it breaks sideways from a missile tracking it, and minds its pilot's G: strained, it flies
    unassisted at 5 g, which a pilot bears for good;
  - it lets go of boost once its tank is down to 5%, until it's half full again (under anime
    rules a boost gauge fills only once boost is let go).
  The server's own Mobile Dolls, and a ZERO seizure, keep the plain doll's reflexes.

## Salvage

Battles leave wreckage, and wreckage is worth money. Under survival rules what's brought home
goes to the pilot's stores (below); under arcade rules the dock buys it, and credits carry across
respawns (a signed-in pilot keeps them from one session to the next; a guest's go with them).

- **Grab** (G toggles it): the free hand (the left, unless it's gone) closes on the nearest free
  chunk within 8 m of reach that is moving at no more than 12 m/s relative to you, and holds it.
  Weapons in that hand can't be used meanwhile: a Leo holding something can't use its machine
  cannon or saber, and neither can a Wing Zero use its saber. Hauling makes you vulnerable.
- **Stow** (B) puts loose ore, or a limb, of up to 2.5 t into the hold, if there's room: a Leo
  carries 3 t, a Wing Zero 1.5 t, Mobile Dolls nothing. Anything heavier (a hulk) is towed in hand.
- **Throw** (T) flings what's in hand along your aim: 60 kN·s of push, at most 40 m/s. You are
  pushed back just as hard. **Jettison** (J) dumps the hold behind you.
- **Mass matters.** Cargo and what's in hand add to the suit's mass: a fully fuelled Leo (9.5 t)
  towing a 6 t hulk has about 60% of its usual acceleration, and turns slower too. Parts shot off
  make it lighter.
- **The dock** is just off the mouth of the docking hub at the colony's −X end, inside a ring of
  amber lights. Under survival rules, Enter at rest inside it takes the suit into its bay, and
  everything aboard with it. Under arcade rules it tops up your propellant, and arriving slower
  than 25 m/s sells the hold and whatever is in hand: nickel-iron 1 credit/kg, titanium 4,
  volatiles 3, exotics 15. Suit parts sell as titanium, except a Gundam's (gundanium, sold with the
  exotics).
- **Dying** spills the hold and drops what you were holding; someone else can pick it up. What a
  suit on a body spills (dying, or jettisoning) flies up off the body, never into it.

## Mining

The rocks hold ore: most are nickel-iron, some titanium or volatiles, a few exotics. Their veins
show which, and thin as the ore is taken. A rock of radius r m has 60 + 25r of structure and
200r kg of ore, so a 10 m rock takes two saber strokes and holds 2 t.

- **Blades mine best.** A stroke into a rock does double damage and chips off up to 200 kg of ore,
  which drifts free, ready to grab. Machine cannon rounds wear a rock down at their usual damage.
  Beams do 0.3× and boil off 4 kg of ore for each point of damage, so shooting a rock apart wastes
  most of it.
- **A rock with no structure left shatters**: whatever ore is left flies off as 2–8 chunks. Nothing
  meets it until it grows back, 10 minutes later and only once no suit awake is within 1 km (a
  sleeper holds it back only if the rock would grow over it). Rocks crack as they're worked.
- **Digging your own rock.** On a rock's surface, a blade doesn't cut into the rock underfoot
  unless the stroke starts aimed down, 30° or more below the horizon: then it digs, and can
  shatter the rock, which floats the suit off.
- **Hulks come apart.** A blade's stroke through a hulk cuts off the part nearest the blade, which
  drifts free as a limb small enough to stow.

## Objectives and the chart

A new pilot needs somewhere to go and something to do. The Charter Board's first jobs for an
Arrival are the **objectives** (`bc_client_core::objectives`), and the HUD shows one at a time,
top left (`OBJECTIVE 2/7`), with its waypoint in the world: a yellow `◆` with its name and range,
held at the edge of the view while it's off it.

| Objective | Done when | Waypoint |
|---|---|---|
| LAND ON MO-II | standing on it | MO-II |
| HIDE IN THE AFT WELL | hidden in it (the server's word) | the Aft Well |
| MINE 200 KG OF ORE | 200 kg in the hold and in hand | the nearest big rock |
| BRING THE ORE HOME (survival) · SELL ORE AT THE DOCK (arcade) | in the dock's ring with something aboard | the dock |
| RIDE THE CAP LIFT DOWN (the colony open) | in the city | flying, the dock; in the bay, the airlock's prompt |
| FIND THE EXCHANGE FLOOR (the colony open) | at its door | on the city's map (M), a `◆` on its door, and its range on the panel |
| SELL ON THE EXCHANGE (the colony open) | a sale filled on the Exchange, from anywhere | as above |
| REPORT TO THE PROVING GROUND (the colony open) | at the Blast Hall's desk | on the city's map, a `◆` on its blast doors |
| DOWN A MOBILE DOLL | a Doll downed | the nearest Doll in sight, else their patrols over the field |
| LAND ON HERMIT | standing on it | Hermit |
| DOWN 5 MOBILE DOLLS | five downed, over any number of visits | as above |

- **Any order.** Each is checked every frame, so doing one early counts; the HUD shows the first
  in the rules' order not yet done. Survival starts at the dock beside MO-II, in a worn Leo with
  no rifle, so it lands and hides first and fights last; arcade starts among the Dolls, so it
  fights first. The ore brought home goes down the chain, into the colony and onto its Exchange
  (`PEERS.md`'s "connect before deepening"). On foot (in the bay, or the city) the panel shows the
  first of those not yet done, since the flight's wait for the next sortie.
- **Kept with the settings**, as the hints are: what's done and the Dolls downed carry over from
  one visit to the next in the same browser. An objective done says so (`OBJECTIVE DONE - …`).
  The settings panel can hide the objective and its waypoint.
- **Not rewards.** They point the way to what pays (bounties, the dock's prices, the stores), and
  nothing about them is the server's: it pays what it always has.

**The chart (M)** is a holographic map of everything a pilot can find, in 3D, from a single rock
out to Earth and the Moon (`bc-client`'s `chart.rs`, on `bc_client_core::{chart, nav, sphere}`).
- *Drawn in light.* The sector on its own render layer: the colony turning with its three windows
  lit, the docking hub and the dock's amber ring, MO-II rolling on its station-keeping circle,
  Hermit and its craters, the hide spots ringed in amber, the field's rocks (each a light that
  keeps a least size on screen, coloured by its ore), every suit in sight with where it's heading
  (red hostile, green friendly, grey wrecks), missiles, the objective's `◆`, and the pilot's suit,
  an arrowhead with its velocity drawn 30 s ahead. The sector's limit is a dashed box.
- *Above and below.* A plane through the pilot's suit (P: through the sector's middle) is ruled in a
  grid that fades from the view's focus, with range rings round the pilot (1, 2, 5, 10, 20 km) and
  its ends named (`+X SUNWARD`, `-X DOCK END`); a stalk drops from everything to it.
- *Out to the Earth Sphere, without a cut.* Zooming out past the sector pulls the view toward the
  Earth Sphere's middle a little each step, until Earth, the Moon, the Moon's orbit and the five
  Lagrange points are in view at their true distances (`sphere`: L1 326,381 km from Earth and
  58,019 km from the Moon); the sector is a light at L1. The sky draws Earth and the Moon in the
  same directions, so the Earth in the window is where the chart puts it. Earth is a hologram of
  continents ruled in contour lines, lit on the Sun's side and pricked with cities on the night
  side. L2 to L5 are marked UNOPENED, with dashed lanes from L1: they open with the Cluster's
  expeditions (`STORY.md`), and a course can't be set to them yet.
- *Picking.* Hovering names anything with its range; a click selects it, and a card says what it is
  (in the world's voice), how far, how far above or below, how fast it's closing, how far and how
  long by the auto-nav and, by the real flight rules, about how much propellant the trip burns. A
  double-click flies the view to it; a double-click on empty space marks a nav point on the plane.
  The places are also listed as chips to click (the colony, the dock, the field, each landmark and
  hide spot, Earth, the Moon, the Sun, L1 to L5).
- *Courses.* Enter, a right click or SET COURSE sets a course: the shortest way there that keeps
  300 m off the colony and 60 m off the landmarks (`nav::plot`: straight if the way is clear,
  else through a ring of points round the colony's hull and round each landmark, searched
  shortest first). It ends just off what it's for: 150 m off a landmark's or a rock's surface on
  the near side, 120 m over a hide spot's bowl, in the dock's ring, 300 m off a suit and keeping
  pace, 800 m off the colony's hull. The chart draws it with chevrons flowing along it; with the
  chart closed it's laid out in space ahead of the suit as chevrons that fade with distance, with a
  `◇` on the HUD (held at the edge of the view) giving the range and, flown by hand, the time to
  it and `BRAKE` when the suit is closing too fast to stop in time.
- *The auto-nav (N, on the chart or in flight).* Flies the course with flight assist, no faster than 300 m/s, slowing for its
  turns and where it runs close to something, sidestepping rocks, never closing on anything it
  couldn't stop short of, and comes to rest at the end (`ARRIVED: MO-II · L arms the grip to
  land`). It's the pilot's own stick, the same command any pilot sends, worked out from the
  prediction tick by tick: every playable frame flies it to every kind of place, by both flight
  rules, without touching the colony, a landmark or a rock (`nav::tests`). Any flight key (thrust,
  boost, brake) hands the stick back, and so does locking on (Y): that's choosing to fight, and
  the lock-on's stick takes over. While it flies, the command carries no lock-on (its stick is in
  the suit's own axes) and no burst step. Moving the mouse takes back only the aim, and it keeps
  flying the course whichever way the suit looks. Landing is left to the pilot.
- *Hands off.* While the chart is open the cursor is free (the pointer's lock comes back when it
  closes) and the keys are the chart's, so the suit's stick is let go: flight assist holds it still
  unless the auto-nav is flying. The sector doesn't pause, and the chart says when hostiles are
  within 3 km. Esc or M closes it, and it closes by itself when the pilot leaves the sector.

## Survival: you build your suit (Milestone 3)

The default rules (`--rules survival`; `--rules arcade` keeps the old game, any frame and free
respawns). Nobody is handed a Gundam. A pilot starts on foot in their own **hangar bay** in the
colony's docking hub, with a worn-out Leo in the gantry (its beam rifle missing, the tank half
full), a little steel, propellant and munitions, and 2,000 credits. Everything better is built
from ore, salvaged from wrecks, or bought from other pilots, and what's flown out can be lost. The
starter Leo is second-hand inside too: its radiators are damaged, and the suit's console says
what overhauling them takes.

**How hard a good suit is.** In the colony's own prices, a new Leo is about 41,000 credits of parts
and weapons: some 10 t of ore (mostly nickel-iron and titanium) and an hour of fabricator time. A
Gundam is 175,000–223,000: 10–13 t of titanium ore, 2.4–3.4 t of exotic metals (a Leo's hold
carries 3 t of anything), 3½–4½ hours of fabricator time, and 15,000–20,000 credits of foundry
fees for its gundanium, which only the colony's zero-G foundry can make. The colony won't trade
Gundam technology at all (OZ is hunting for it): gundanium, and the Gundams' parts and weapons,
change hands only between pilots.

### The hangar bay (first person)

The bay (34 × 48 m, 30 m high) hangs in the hub's spin ring at 0.7 g. The suit stands in the
middle in a yellow gantry, facing the bay doors it launches through; a catwalk crosses in front of
its chest at the cockpit hatch, reached by the stairs along the left wall. The pilot walks it in
first person (a 0.6 × 1.8 m body that steps up stairs, jumps and falls: `bc_client_core::walker`,
the same code the agents walk with), and uses things by looking at them within reach (E):

- **The fabricator** (right wall): ore into materials, materials into parts and weapons. Jobs queue
  and run on the wall clock, so they finish while the pilot is out flying, or away.
- **The stores' racks** (right wall, forward): bulk goods by the kilogram, weapons, and suit parts
  one by one with their condition.
- **The Colony Exchange terminal** (left, by the doors).
- **The suit's maintenance console** (by its feet): fit and strip parts, weapons and equipment,
  repair armour, overhaul systems, dismantle, the suit's stat sheet as it would launch (delta-v,
  acceleration, sensors, energy, heat, hold, the G its pilot bears, damage taken), and whether it
  would launch.
- **The cockpit hatch** (on the catwalk): board and launch.
- **The airlock** (left wall): the way out of the bay (leave the game), or, when the colony's
  open, to the cap lift down into it.

The terminals are panels on the page over the live bay. Everything they show is the server's word,
and everything they do is a request the server checks (`bc_econ::wire`).

### Building a suit

A suit is its **torso** (the cockpit and the reactor) plus whatever else is fitted: head, arms,
legs (without them it can't walk, but in space it flies), backpack (the main thrusters), and a
weapon on each of its line's mounts, which hang on their arm. A torso fitted into an empty bay
starts a new suit; parts fit only their own line. Each part keeps its **condition** (1–100%),
which is its armour when it launches; a part shot off in the sector comes home missing, its weapon
with it. A suit launches with what's fitted, as worn as it is, its tank and magazines topped up
from the stores.

- **Materials** (fabricator): steel (nickel-iron), titanium alloy (titanium, a little volatiles),
  propellant (volatiles), electronics (exotic metals and steel), munitions (steel and volatiles).
  **Gundanium** (titanium alloy and exotic metals) only at the zero-G foundry, 400 credits a batch.
- **Parts** take structure (steel), armour (titanium alloy, or gundanium for a Gundam) and wiring
  (electronics) in proportion to their mass, plus their systems: the ZERO System in Wing Zero's
  torso, its wings, the Hyper Jammer, Full Open's gatlings and pods, the Cross Crusher's arms.
- **Repairs** restore armour for 60% of a part's materials, pro rata; **overhauls** restore a
  damaged system for 20 kg of machined components and 5 of electronics (2½ times that for a
  failed one; a Gundam's take exotic metals too). **Scrapping** a part, a weapon or a module gives
  back half of what went into it, as worn as it was.
- **Machined components** (steel, a little electronics and titanium alloy, at the fabricator) are
  what overhauls and equipment are made of; the colony deals in them, and in ordinary equipment.
- **Faults travel with parts.** A part stripped off carries its systems' faults to the shelf, and
  back when it's fitted; a part with faults isn't new, so it doesn't trade. Equipment comes off
  with its part, into the stores.
- **Salvage** docked in the hold comes home as ore, or as parts: a limb at 15% condition with its
  systems failed, each part still on a hulk at 40% with its systems damaged. Parts of lines nobody
  can build (a Mobile Doll's) are scrap metal.

### The Colony Exchange

An order book per item. Pilots place limit orders; an order that crosses trades at once at the
resting price (best first, oldest first), and the rest waits on the book or is handed back.
Resting orders hold their goods or credits in escrow, and fills wait in the trader's account until
they're next in their bay, so orders fill while their owners are away. Prices are credits a tonne
for bulk goods, a piece for everything else.

- **The colony** trades too, from a desk per item it deals in: it buys all the raw ore it can get,
  sells propellant cheap, and deals in materials, ordinary parts and weapons. Its middle price is
  the item's value times (the stock it wants ÷ the stock it has)^0.6, within ⅕× and 5×; it bids 10%
  under and asks 10% over. Pilots selling to it drive its prices down and buying drives them up,
  and its stock settles back towards what it wants over hours (what it uses up, what it imports),
  so prices drift back.
- **Sinks and sources.** A seller pays 2% of every sale; the foundry charges its fees; what the
  colony consumes is gone. Credits come in from what the colony buys and from bounties (Mobile
  Dolls shot down). The ledger balances: credits are neither made nor lost in a trade (a property
  test checks it).
- Each item's last trade price is sampled once a minute: the terminal draws the last hour.

### Wear from use

Systems wear down between fights as well as in them (`bc_econ::wear`). The sector counts what
each suit goes through out there (`bc_sim::sim::Usage`: ticks of main burn and of boost, rounds or
shots from each mount, overheats), and the hangar adds each sortie's to the suit's wear:

| What's counted | Wears | Service life |
|---|---|---|
| the main thrusters burning | the main thrusters | 30 minutes |
| boosting | the boosters | 5 minutes |
| rounds or shots from a mount | its arm's actuators (the fire control, for the head's, shoulders' and chest's) | 1,500 |
| overheating | the reactor | 6 times |

A system that's had its service life comes home a level worse (working to damaged, damaged to
failed), and the dock's note says so; overhauling it starts its life again. From a quarter of
the way through, an overhaul services it instead (half of what overhauling it damaged takes), so
keeping a suit flying is a steady trade in machined components. A part stripped off leaves its
systems' wear behind: the next one fitted starts afresh. The suit's console shows each worn
system's service life.

### Consumables: the rack and the hotbar

A suit carries a rack of consumables (`bc_sim::content::kits`), used in flight from the hotbar
(1–4; the HUD's `RACK` line counts them). They're made a few at a time at the fabricator and the
colony sells them; at launch the rack takes up to three of each from the stores, and what's left
comes home with the suit (nothing, if it's lost). Used up in fights, they're always in demand.

| Key | Consumable | What it does |
|---|---|---|
| 1 | Patch kit | A field repair: seals a leaking tank, or brings the worst-off system it can reach back a level (failed to damaged, damaged to working). A part shot off is beyond it. |
| 2 | Coolant flush | Dumps the suit's heat at once (Full Open's lockout still holds). |
| 3 | Chaff | Breaks every lock on the suit, and missiles tracking it lose it; for 3 s no new lock builds. |
| 4 | Stim | The pilot bears 1 g more (2 under anime rules) for a minute, then crashes, bearing 1 g less, for half a minute. One at a time. |

A kit with nothing to do (a cold suit's coolant, a patch kit with nothing broken) stays in the
rack. The use goes to the server on the control stream, the sector applies it at the next tick, and
the owner's snapshot carries the rack and the stim's clock (the stim is part of the stat sheet the
client predicts with, as the server flies with it).

### The Charter Board: contracts and the great works

The colony's notices (`bc_econ::charter`, one board per colony, kept with the exchange): a tab on
every terminal in the bay, and the Charter Board's own desk in Charter Square.

- **Contracts.** Jobs with their pay posted beside them. *Supply*: deliver so much of an item.
  Anyone but its issuer delivers part of it from their stores and is paid pro rata on the spot,
  and what's delivered goes to the issuer: to the colony's desk (its prices fall as if it had
  bought it), or to the pilot who posted it, waiting in their bay. A pilot's contract holds its
  reward in escrow from the moment it's posted (up to 8 at once, standing 1–72 hours), so a job
  is always good for its pay; what it hasn't paid when it expires or is withdrawn goes back. The
  colony keeps four of its own up for what its desks are shortest of (ore, steel, alloy,
  electronics, munitions, components), at 135% of their value, for two hours each. Agents deliver
  too: the miner hands its ore to the colony's contracts before it sells the rest.
- **Patrols** (once the militia has its hangar): take one, and down Mobile Dolls for 1,000 CR of
  bounties within the hour; the militia pays 1,500 CR on top. One pilot holds a patrol at a time,
  and a pilot holds one at a time; the bounties count when the suit comes home (or is lost).
- **The great works.** The era's projects, each needing tonnes of materials, delivered from the
  stores at 120% of the colony's value:
  - *A second foundry* (12 t of steel, 6 t of titanium alloy, 800 kg of electronics, 1.5 t of
    machined components): gundanium at half the fee, made twice as fast.
  - *The militia's hangar* (15 t of steel, 4 t of alloy, 2 t of components, 3 t of munitions,
    4 t of propellant): the militia posts patrols.
  - *The charter vote*, open once both are finished: three pilots of standing sign it, and the
    calendar begins (`AC 1 · THE CHARTER`, `STORY.md`).

  Every pilot hears when one is finished, wherever they are; the board lists each work's most
  generous contributors.
- **Standing** is the credits a pilot has earned from the colony's contracts, patrols and works:
  it's what signing the charter takes. A pilot's contract with another pilot earns none.
- **The ledger** still balances: credits enter only from the colony (what it pays on contracts
  and works joins its purchases), and escrow and deliveries never make or lose any (the ledger's
  property test covers the board too).

### Sorties

- **Launching:** board at the hatch. The bay vents, beacons turning red, the doors part, and the
  catapult throws the suit down the 220 m launch tunnel into space at the hub's mouth, inside the
  dock. (Space skips the sequence.)
- **Docking:** come to rest (under 25 m/s) inside the dock's ring of amber lights, off the mouth
  of the docking hub at the colony's −X end, and press Enter. The suit glides in down the tunnel,
  the doors shut behind it, and the pilot climbs out onto the catwalk. What came home goes to the
  stores: the suit as it is (its systems as broken as they came, its equipment if its parts came
  too), the hold's ore, whatever was in hand, and the bounties earned.
- **Arriving:** a pilot's first time in their bay, the news says they've arrived, and what the
  Charter Board advanced them.
- **Losing it:** a suit destroyed out there is gone, along with its hold. The bounties it earned are
  still paid, and the pilot is brought back to the bay through the airlock once the wreck clears.
- **A floor under it** (`Hangar::reissue`): a pilot back in an empty bay with no torso in the stores
  to build on, and less than a Leo torso's worth in credits, stores and parts (at the colony's
  values), finds a worn Leo in the gantry, the Charter Board's advance, as on the day they
  arrived; the news says so. At most once every 30 minutes, so it's a floor, not a free suit.
  (An Arrival's 2,000 cr is less than a torso: lose the first suit and the Board stands you
  another.) A new wallet is still a new starter kit.
- **Away:** a signed-in pilot who leaves keeps everything: their hangar, its jobs, their orders.
  Left out in the sector, their suit sleeps where it is (below), and they wake in it; one the
  sector lost track of is towed in. A guest's hangar lasts the visit.

## The First Colony, inside (Milestone 5)

The plan, its numbers and what's still to come are `COLONY.md`. What's in so far:

- **The colony, from outside**, as the Gundam Wing pictures have it: its mirrors hinged at the −X
  end, opening with the colony's day (about 1° at night, 45° at noon), the bay ring with its six
  spokes, the docking hub as a spire of stacked modules, scaffolds and cranes at the +X axis
  port, and rows of lights along the windows, the ring and the spire. Its windows show the city
  inside, the same streets the walkers walk, lit at night.
- **The colony's day** runs on the tick's clock: 48 minutes (32 of them daylight), the same on
  every client.
- **The city.** Three land strips, each 3.35 km across and 32 km long: an 80 m avenue down the
  middle, twelve rows of 128 m blocks either side, a canal in the fourth row on one side, a park
  and promenade along each window, Hub Gate's square at the docking hub's end and a building site
  at the far end. Each strip has twelve districts of its own (`content::city`), from the towers of
  Exchange Row to Old Town's low streets, all worked out as a closed form of where you are
  (`bc_sim::colony::city`): nothing is stored, and every client and the server see the same walls.
  Gravity is the spin's: 1 g on the ground, less up a tower.
- **Going in** (a survival server run with `--colony`): the bay's airlock leads to the cap lift,
  which rides down the end cap's face with the whole colony in view (Space skips the ride) to Hub
  Gate's terminal. From there the pilot walks the city with the bay's controls, and M shows the
  map of their strip. Districts and sights are named on the way in; a sight reached for the first
  time goes on the pilot's found-list (kept in their settings: "SIGHT FOUND · THE CLOCK TOWER · 2
  OF 10"), and the map ticks off the sights found by name, rings those still to find, and lists
  them.
- **Places:** the Exchange floor (its terminal is the bay's exchange), the Charter Board (its
  contracts and great works, above), The Arrival (a bar), the Proving Ground (the Blast Hall,
  `TRAINING.md`), and Hub Gate, whose lift goes back up to the bay.
  Each of the first four has a room behind its door: walk in, and use the place at its counter
  (E). The trading floor's boards run along its back wall, the Charter Board's notices are pinned
  on its, and the bar's shelves are behind its counter. The Blast Hall's room is a suit's: 86 m
  deep, 84 m wide and 60 m high, behind blast doors 40 m wide and 45 m high that a suit flies in
  through. Its back wall shows the course in light, and its desk the pilot's best time round it. Rooms are lit by their own lamps; the eye
  adapts to them going in (and the street through the door blazes), and back to the day going out.
- **The Arrival's seats:** two benches either side of its door, facing the avenue
  (`colony::city::arrival_seats`). E by one sits you on it (the view drops to a seated eye), and E
  again or a step stands you up; everyone else sees you sitting (the presence's ride 15), and the
  plaza takes a seated pose only on a seat. Agents come and sit too (`flaneur --sit`).
  A suit can't launch from the city: its pilot rides back up first.
- **Other pilots** are there too, on their own feet in flight suits of their own colours (from
  their names, the same on every screen), striding as fast as they go, their names over them
  within 40 m. Everyone on a strip within 1.5 km is shown, nearest first, up to 48. Agents can
  walk the city as well (`bc-bot`'s `flaneur`).

- **Trams** run down the middle of each strip's avenue: eleven stations from Hub Gate to the
  building site, a train every 2.8 minutes each way, 81 s between stations at up to 57 m/s. Walk
  in through a standing train's open doors from its island platform; it carries you (jump as it
  pulls away and it moves on under you); walk out at any station. Every train is where the colony's clock says, on
  every screen, and so is everyone riding one.

- **Cars and scooters** come from the motor pools: one beside each Hub Gate's door, one on the
  avenue by each tram station (E a car, Q a scooter). W/S the throttle and brake (held at a stop,
  reverse), A/D the wheel, Space the handbrake, Tab the camera (behind, or the driver's seat), E
  to get out once slowed to a walk. They ride up kerbs, stop at walls, and don't drive into the
  canal; a car tops out at 30 m/s, a scooter at 22. Others see the car (or the scooter, and its
  rider) in its driver's colour.

- **Suits inside** (`SUITS_INSIDE.md`): at the cockpit, Q launches the suit into the colony by
  the inner gate near the axis instead of out to space. In there it flies the colony's own frame:
  the spin pulls it to the floor (1 g there, less towards the axis) and Coriolis turns it aside,
  the air slows it, the hull, the end caps and the city's buildings stop it, and flight assist
  holds it where it is. Weapons are safe by the colony's law: nothing fires. The HUD marks the
  inner gate; at rest in its ring of lights, Enter docks back into the bay. It's the server's
  second sector (`sector-1`), keeping the first's tick: the colony has one clock. Pilots on foot
  see the suits flying within 2.5 km of them, and a suit's pilot sees the people below (those on
  the strip under it within 1.5 km, their cars too) and the trams, where everyone on foot sees
  them. With the grip armed (L) a suit lands on the city, the avenue or a roof, and walks it as it
  would a rock, but under the colony's own pull: its walls stop it, and walked off a roof's edge
  it comes down on whatever's below.

- **The Proving Ground's course** (`TRAINING.md`): 13 rings of light in the colony's air, from
  just off the inner gate down over the first window to the Charter strip. It runs along the avenue
  between the towers, slaloms over its carriageways, climbs and turns over the top, comes home, and
  ends on a pad on Hub Gate's square. The clock starts at the start ring and stops when the suit
  stands on the pad (set down with the grip armed, L).
  - The HUD's panel shows the ring, the stretch, the clock and the range, and the `◆` marks the
    next ring.
  - A finish is the Charter Board's flight certificate: first class within the par of 2:00, second
    within half as long again, third for flying it at all.
  - The best time is kept in the browser, with the settings.
  - Pilots on foot in the city see the rings over Hub Gate.

## The world (EVE-lite, roadmap)

- The Earth Sphere is split into **sectors**: L1–L5 colony clusters, lunar orbit, Earth orbit, and
  resource satellites (Barge; MO-II already keeps station in L1). Each sector is one simulation
  thread (then one process). Travel between them is a timed transfer orbit, so chokepoints and
  interdiction emerge naturally. A sector's bodies, and the suits on them, are its own: a suit
  leaves a sector flying free.
- **Time dilation** instead of crashes. When a sector's tick exceeds budget in a huge battle,
  simulation time slows and the snapshot header's `tidi_pct` tells clients to slow their clocks.
- **Factions and territory:** OZ, the Alliance, the Colonies (Operation Meteor), Romefeller, White
  Fang, the Preventers. Mobile Doll production lines are a faction investment. Gundanium is refined
  in zero-G.

## Controls

| Input | Action |
|---|---|
| Mouse (click to lock) | aim |
| Y · middle click | lock on (again: the next target; held: let go) |
| W/S · A/D · Space/C | thrust forward/back · left/right · up/down; double-tapped, a burst step that way |
| Q/E | roll |
| L | grip: armed, coming in slow and close lands you on a rock or a landmark; again, let go |
| Shift · X · R | boost · brake · RCS (fast turns) |
| LMB · RMB · F | primary · secondary · melee (a beam rifle: tap fires, hold to charge, let go full for the charged shot) |
| H | the frame's special: a toggle for Neo-Bird and the Hyper Jammer, a press for Full Open Attack and the Cross Crusher |
| V · Z | flight assist · ZERO System |
| Tab · mouse wheel | the camera: the cockpit (first person) or the chase camera (wheel in: the cockpit; out: chasing) |
| M · N | the chart: the sector in 3D out to the Earth Sphere, the objectives and courses · the auto-nav on the course set (on or off) |
| G · B · T · J | grab (toggle) · stow · throw · jettison |
| 1 · 2 · 3 · 4 | survival: the rack's patch kit · coolant flush · chaff · stim |
| Enter | dock (survival): at rest inside the dock's ring of lights |
| 1–6 | arcade rules: respawn as Leo, Wing Zero, Heavyarms, Deathscythe, Sandrock or Shenlong |
| / (Enter on foot) | talk on the colony's radio: Enter says it, Esc closes |
| Esc · F1 · F10 | menu · the controls sheet · graphics quality |

On a body (see "Surfaces"): W/A/S/D walk, Shift runs, Space hops (held, it lifts off on the
thrusters), C crouches (a toggle), X stops, a blade's lunge dashes along the ground, and Q/E do
nothing; L lets go. Aloft in a body's grip the keys fly as ever, but flight assist holds a walk's
speed over the body, or a run's with Shift, and never holds altitude.

On foot in the hangar bay: the mouse looks, W/A/S/D walk, Shift runs, Space jumps, E uses what's
in view (E again, or Esc, steps away from a terminal).

The list players see (the title screen's controls sheet and F1) is
`bc_client_core::controls::BINDINGS`; keep it in step with this table. Down is C alone: Left Ctrl
held with W is Ctrl+W, which closes the browser's tab. Turning flight assist on or off says so in
the middle of the screen, because V is the camera key in other games. `docs/CONTROLS.md` compares
this scheme with what players of other games expect, and lists what they'll ask for.

**The cockpit.** Tab (or the mouse wheel) switches between the chase camera and the cockpit: the
view from the head's main camera, which is what a mobile suit's cockpit monitors show. It looks
along the aim, as the chase camera does, so the crosshair is the aim either way and the suit turns
after it. The head isn't drawn from inside it; the rest of the suit is, so a blade stroke, the
Dragon Fang or a shoulder coming round shows. Neo-Bird's view is from over its canopy, along its
nose. With the head shot off the picture comes from the sub-camera, greyer and fringed. A wreck is
watched from the chase camera. The view is a setting, so the next sortie starts in it; the chase
camera is the default.

From the cockpit the pilot sits inside the suit, as in the show: the middle of the view is the
panoramic monitor (the world, with nothing of the cockpit across the crosshair), and round it a
fan of monitors on a dark frame, a wide one across the top, two down each side, with the control
grips and the console below. The instruments move onto the monitors: ZERO across the top, the
suit and its damage silhouette upper left, arms upper right, flight lower left, the hold lower
right. What marks the world stays over the view: the crosshair, the target corners, the edge
chevrons and the cautions. In the console, a radar sphere holds its bearings in space as the suit
turns: hostiles red, friends green, missiles amber, the pilot's suit at its heart (out to 3 km, on
a log scale). The cockpit is trimmed in the suit's livery, sways a little under G, and its monitors
flicker when the suit is hit (with flashing effects on) and fill with static on the sub-camera.

**The HUD** is drawn as a mobile suit's monitor: translucent plates with two corners cut, labels in
Chakra Petch over readouts in Share Tech Mono, white for the suit's own values, amber for what
wants attention, red for danger, pink for ZERO. Cautions (LOCK WARNING, MISSILE) come up in a
hazard-striped banner. Targets get amber corners with a name and range tag (red for hostiles, green
for friends, pink for ZERO's pick); what's off the view gets a chevron at its edge. The suit's
damage silhouette colours each part by its armour: white, amber, red, and an outline once it's
gone. Chasing, the panels sit in the screen's corners.

**Where the guns point.** A hand weapon fires only within 50° of the body's axis (Neo-Bird's rifles
within 2° of the nose), so until the suit has turned onto the aim the crosshair dims and `( )` marks
where the primary weapon would fire. `-o-` is the velocity vector, the way the suit is drifting
(`-x-`: the way it's drifting from, moving backwards).

**The page around the game.** The title screen takes a callsign (and, under arcade rules, a
mobile suit) and launches,
as a guest or signed in with a wallet (Sign-In with Ethereum: the wallet proves the address, and
nothing is authorized or spent). A signed-in pilot is someone the sector can remember; a guest's
suit goes when they do.
The link says what went wrong in words (a server that's down or unreachable, a full sector, a page
older than the server, a wallet that declined), offers Retry, and redials by itself, with backoff,
when a link that was in the world drops. A signed-in redial doesn't ask the wallet again, and
signing in from a second window takes the pilot over (the first lets go and doesn't fight back). Esc (or the browser taking the pointer back) opens the menu: Resume, Controls,
Disconnect. The sector doesn't pause. Menus are HTML over the live scene; the cockpit HUD is Bevy's.

**Sleeping in the cockpit.** A signed-in pilot who leaves (the menu's disconnect button, closing
the tab, a link that stays down, a minute without input) doesn't take the suit along: it stays in
the sector with its pilot asleep in the cockpit, and they wake in it when they're back. The menu's
button says what will become of it: LEAVE SUIT HIDDEN on the ground in a hide spot (survival),
PARK & DISCONNECT where it would park, SLEEP & DISCONNECT anywhere else.
- *Drifting.* Nobody flies a sleeping suit: no flight assist, no attitude hold. It carries on at the
  velocity and spin it had, fully Newtonian, and fetches up against rocks, landmarks and the colony
  as a wreck would. Its eyes go dark, and brackets read ASLEEP. Nobody works its special either: a
  Neo-Bird stays a bird, and a jammer goes off.
- *Parking.* A suit on its feet on a body, or resting against a rock or a landmark (within 1.5 m of
  its surface, under 3 m/s over it), when its pilot leaves is parked instead: held where it was,
  kneeling if it was crouched, and moving with the body. One aloft in a body's grip settles onto it
  first, and parks where it lands. The HUD reads PARKED while you're somewhere you could park (HIDE
  SPOT in one). Shatter the rock and the suit floats free.
- *Powering down.* A parked suit stays in sight for 8 s after its pilot leaves, or 60 s after it
  last fired or was hit, whichever is later, so nobody logs off out of a fight. (A suit that has
  never fought is dark after the 8 s.) Then its reactor idles: its enemies' sensors and seekers lose
  it, and their eyes find it only within 400 m, or 150 m in a hide spot. Its allies see it all
  along.
- *Hunted.* Mobile Dolls leave sleepers alone; other pilots can shoot them down and salvage what's
  left. A sleeper destroyed stays gone, and its pilot is told by whom when they're back. Parked is
  safer than drifting, and hidden safer still.
- *Room.* At most 256 sleepers a sector; past that, or when the sector's suits run out, the longest
  asleep is cleared (those in hide spots last), and its pilot is told.
- *A restart.* Under survival rules, a suit left on its feet in a landmark's hide spot is saved with
  its pilot's record, and put back where it was when the server starts, before anyone connects, with
  its real damage, tank, ammunition and hold: what it had when its pilot left, less what it has
  lost to hunters since (its record follows every hit, so limbs shot off aren't there to take
  again). It is asleep, and dark 8 s later, so it can be hunted before its pilot is back.
  Destroyed while they're away, it isn't put back again, even if that was as they were leaving. A restart clears
  every other sleeper: under survival the tugs bring it home to its bay as it launched.
- *Waking.* The pilot wakes where the suit is: on its feet (or knees) as they left it, still
  gripping, and crouched if it was. A suit that lay hidden stays hidden until it moves or fires:
  `WOKE IN AFT WELL - hidden. Move or fire and you're seen.`
- A suit that's already a wreck when its pilot leaves is gone, as a guest's suit always is.

**Settings** (from the title or the menu) are kept in the browser: mouse sensitivity, invert Y,
the flight camera, field of view (vertical; the panel gives the horizontal too), camera shake,
flashing effects (a ZERO seizure's flicker; off to start with when the browser asks for reduced
motion), first-flight hints, the objectives and graphics quality, along with the last callsign and frame launched. `bc_client_core::settings` defines them, their ranges and the stored text (a
key this build doesn't know is kept, for the build that wrote it). A new pilot gets one hint at a
time (on foot in the bay: walking, using a terminal, boarding; flying: thrust, the chart, boost,
fire, locking on and then the burst step, the cockpit view, flight assist, salvage, docking, the
menu; near a body: the grip, walking on it, hiding in a hide spot), each gone once it's been done.

**Sound.** Every sound is generated at boot (`bc-sound`, no audio files): weapons, impacts,
explosions, the engines worked by the throttle, RCS puffs, the lock tone quickening as a lock
builds, missile and low-propellant alarms, the pilot's heartbeat under G, the ZERO System's drone,
the dock and the sale, and a score that crossfades from calm to combat with the fight. On a body:
the grip's clamps closing and opening, the thud of a landing (as hard as it was), footsteps, the
reactor going dark and powering up, and, through the rock, the footsteps of anyone else on the
same body within 500 m, so hunter and hider can hear each other. There's no air in space, so it's
the cockpit's sound: the suit's own machinery, and the world as the sensors render it, quieter with
distance and gone beyond each cue's range. A blackout muffles everything.
The browser plays it through Web Audio; volumes are settings (master, weapons, cockpit, music).
`cargo run -p bc-sound --release --example reel -- reel.wav` writes a reel to listen to.

**The title theme** is a mid-90s anime opening as a Super Famicom game would have played it: an
original piece at 143 BPM in E minor (orchestra hits on a 3-3-2, a hook, verse, a pre-chorus that
stops dead before a "royal road" chorus, and a last chorus a whole step up; 60 s that loop). It's
written in MML, one string a voice as SNES composers wrote, and plays on the console's sound chip
in software (`bc_sound::spc`): eight voices, instruments of the kinds those games carried (an
overdriven guitar lead, power chords, slap bass, orchestra hit, brass, strings, choir) generated
in code and stored as 4-bit BRR in the chip's 64 KB (bank and echo buffer fit, with room for a
driver), the chip's Gaussian interpolation, its ADSR rates and its echo with the 8-tap FIR, stereo
at 32 kHz. `cargo run -p bc-sound --release --example title -- title.wav` writes it out.

**The colony's radio.** One channel for everyone connected, under any rules: `/` opens a line on
the page (Enter too, on foot), Enter says it, Esc closes it, and no keys reach the suit meanwhile.
The latest lines show on the left for a while after one comes in. Lines are cleaned (no control
characters, one line, 160 characters at most), a pilot says at most 5 in 10 s, and the server
keeps no log of them (`/status` counts them). Agents talk on it too (`bc-bot` `say` and `heard`).
In the city, what someone near you said shows over their head, under their name, for 8 s.

**Lock-on** (`LOCK.md`). Y, or a click of the middle button, locks the hostile nearest the
crosshair (again: the next one; held: let go), and the fight goes onto the ground: flight assist
holds your velocity relative to the target's, W closes in and stops just outside your blade's reach,
A/D circle it, and with Space and C let go the suit settles onto its level, the fight's floor, and
rolls level with the ground (away from the colony). The mouse still aims; the ◆ marks where the
primary's shot meets the target if it flies on as it is (ZERO's solution replaces it), SPD and the
velocity marker are relative to the target, and its bracket says how fast it closes. A burst step
goes along the fight's axes: double-tap A or D to sidestep round it, S to jump back. The lock goes
when the target's downed, out of sight or past 5 km. With flight assist off, on a body, under ZERO's
seizure or in Neo-Bird form the keys fly as they always do.

**Lock assist.** A frame with missiles designates the hostile nearest the reticle (within 10°) and
keeps it while it stays within 15°. Its bracket fills as the lock builds and reads LOCKED when it's
acquired. The HUD shows the special's state (READY, JAMMING, FIRING, the cooldown), the lock, and
MISSILE LOCK and MISSILE warnings, with a marker on each missile tracking you.

## Roadmap after Milestone 4

Milestone 1 was the playable slice; Milestone 2 the five Gundams (Heavyarms, Deathscythe, Sandrock,
Shenlong, and Wing Zero's Neo-Bird), each flown by pilots and agents; Milestone 3 survival: the
hangar bay on foot, building suits, and the Colony Exchange; Milestone 4 wear and tear: the
systems inside the parts, statuses, equipment, overhauls, and the world bible (`STORY.md`).

The direction is a living colony its pilots build and run: SimCity's colony projects and GTA's
jobs, law and traffic, on an economy whose sinks keep demand turning over. `PEERS.md` says what
the nearest games teach, and which of the items below come first and why.

- **Consumables and a survival hotbar** (done: the rack, above). Next: chaff drawn as it
  blooms, and decoys a missile chases.
- **Wear from use** (done: above).
- **Contracts** (begun: the Charter Board, above, with supply contracts and the militia's
  patrols): clear that claim, escort a hauler home, recover a wreck; shady ones.
- **Colony projects** (begun: the era's great works and the charter vote, above): the next eras'
  (new cylinders, the Cluster's expeditions), and the city showing them built.
- **Facilities:** workshops and refineries in the hub, leased by pilots and crews: production
  chains, and rent as a sink.
- **The colony, on foot** (begun: Milestone 5, above): other pilots in its streets, trams and
  vehicles, other pilots' bays; crews (a friend's hangar, shared stores).
- **Economy:** insurance, market data for agents, the other colonies' exchanges with prices of
  their own (and hauling between them).
- **Law and heat:** shooting colonists draws the militia; enough of it makes a pilot a bounty.
- **Traffic:** tugs, haulers and miners flown by the server, so the lanes are busy.
- **Pilot skills**, economy first: better at what they do, without a combat edge over newer
  Arrivals.

- **Sectors:** multiple sectors with handoff, transfer orbits, TiDi, persistence (a Redis or Mongo
  `PilotStore`, off the hot path).
- **Suits:**
  - Tallgeese, Epyon (its own ZERO).
  - Shooting missiles down; deployable Planet Defensors.
  - Wing Zero's fold drawn as it happens (today the model swaps, with a flash); Heavyarms'
    hatches opening for Full Open.
- **Agents:** an MCP server so LLM agents can fly as squad commanders, and a Python gym on the
  headless simulation for RL.
- **Earth:** atmosphere, gravity, re-entry heating (Wing's shield).
- **Surfaces, next** (each left out of the first round on purpose):
  - Walking the colony's hull. It needs a surface moving at 177 m/s, more than 1 g outward and a
    camera that rides with it. The clock and the hull's ordered sweep are in place; the axis
    port's end-cap module, which moves at most 5 m/s within 90 m of the axis, is the natural
    first step.
  - On foot in the sector, or a suit left hidden while its pilot flies another (survival's
    one-suit rule, and a pilot's body on the server).
  - Parked suits outliving a restart outside the landmarks' hide spots (on rocks: a rock grows
    back where it was, so its index and the field's seed would do).
  - Chunks coming to rest on bodies; damage and G from a hard landing; sensors that rock blocks
    (line of sight: hide spots stand in for it today).
  - More landmarks, and landmarks that can be mined or wrecked; bodies that move by more than a
    closed form (pushed, thrusting, streamed in), which would need their state on the wire.
  - A charged leap (the buttons are all taken: it would take a stance, or a bit of the burst
    step's); leaning and peeking round cover on Q/E; hold to look.
  - Coriolis and centrifugal pulls aloft (at most 0.043 m/s² on MO-II); a chase camera that turns
    with a spinning body (it lags MO-II by 4 mrad); gripping rocks under 10 m.
  - Keeping hidden sleepers' names off the roster and `/status` (they give away who, never where).
  - **Moons and planets.** A moon is too big for one `f32` frame (0.125 m steps at its radius), so a
    lunar sector *is* the moon's own frame, a patch of its surface ±30 km across: the ground is
    still in sector coordinates, and the moon's turn never enters the simulation. Its surface is a
    heightfield with a bounded slope, gravity is real (1.62 m/s²), and the stepping, the wire and
    the drawing carry over unchanged, since they already work in a body's frame against a probe.
- **Salvage:** chunks that collide with each other, miners and pirates flown by the server.
