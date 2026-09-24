# Before Colony: game design

> A *legit* Gundam Wing mobile-suit simulator, built as an MMO. Newtonian flight, free aim,
> AI that plays by the same rules as you, and the ZERO System as a combat AI that shows you the
> future and then tries to take the controls.

Gundam Wing and its names are © Sotsu · Sunrise. This is a fan project. Canon names are compiled in
only with the `canon-names` feature of `bc-sim` (default on); without it the game ships generic names.
All art is procedural.

## Pillars

1. **It flies like a mobile suit in space.** Newtonian 6DOF, finite propellant, AMBAC, pilot G
   limits. There is no drag or "space friction" unless you turn on flight assist.
2. **You aim.** Mouse free aim with no tab-targeting. Beams take a fraction of a second to arrive, so
   leading, dodging and range matter.
3. **AI is a first-class citizen.** Mobile Dolls are the NPCs, as in the show. External AI agents
   connect through the same protocol as humans and are labelled **MD**. The ZERO System is a
   predictive AI in your cockpit.
4. **The server is the truth and the tick is sacred.** Sectors tick at 30 Hz with no locks and no
   allocations (see `ARCHITECTURE.md`). Everything else is built around protecting that.

## Setting

After Colony 195–196, Earth Sphere. Milestone 1 takes place in **Sector L1: the L1 Colony Cluster**.
An O'Neill cylinder (3.2 km radius, 32 km long) lies below the combat zone, with a debris field
around it and OZ Mobile Doll patrols circling above. The sector is a ±32.768 km cube, and suits are
kept within ±30 km.

Space comes first because that is where gundanium is made (it can only be refined in zero-G) and
where the war's logistics live. Earth, with atmosphere, gravity and re-entry, is on the roadmap.

## Flight model (`bc-sim/src/flight.rs`)

Everything is in SI units and shared bit-for-bit between the server and the browser's prediction.

- **Thrust** is limited per axis: main (forward), side (lateral and vertical) and retro. Boost
  multiplies main thrust. Every newton burns propellant at `|F| / (Isp·g0)`, so mass falls as you
  burn and delta-v follows the rocket equation (tested to within 1%).
- **Attitude.** The suit turns toward your aim.
  - **AMBAC** (Active Mass Balance Auto Control) swings the limbs to rotate the suit. It costs no
    propellant but has modest authority. Losing arms or legs reduces it, and so does firing or
    swinging a saber, because the limbs are busy.
  - **RCS** (hold R) adds strong attitude thrusters that burn propellant.
- **Flight assist** (V) turns the stick into a velocity command: it brakes to a stop when you let
  go. With it off you are fully Newtonian. **Brake** (X) always retro-burns.
- **Pilot G.** Sustained load above 6 g builds G-strain. At 100% the pilot blacks out and control
  authority collapses until strain falls below 50%. A Wing Zero on boost pulls about 12 g, so you
  *can* out-thrust your own body, as Zechs did in the Tallgeese. Mobile Dolls have no body, so no
  G limit. Agents are pilots, so they do have one.
- **Hull contact.** The colony is solid: suits slide along the hull and beams splash against it.

| Frame | Role | Dry mass | Accel (boost) | Δv | Armour | Loadout |
|---|---|---|---|---|---|---|
| Leo (OZ-06MS) | line suit | 7.1 t | 3.5 g (5.6 g) | ≈2.6 km/s | titanium | beam rifle · machine cannon · beam saber |
| Wing Gundam Zero (XXXG-00W0) | hero suit | 8.0 t | 8 g (12 g) | ≈3.7 km/s | gundanium (×0.55 damage) | Twin Buster Rifle · machine cannons · beam saber · **ZERO System** |
| Taurus (OZ-13MS) | Mobile Doll | 6.5 t | 5 g | | titanium | beam rifle |
| Virgo (OZ-02MD) | Mobile Doll | 9.5 t | 3 g | | heavy (×0.8) | beam cannon, Planet Defensors (visual) |

## Combat

| Weapon | Speed | Damage | Rate | Notes |
|---|---|---|---|---|
| Beam rifle | 4 km/s | 45 | 1.5/s | energy and heat; dodgeable at range |
| Machine cannon | 1.2 km/s | 6 | 10/s | ballistic; 400 rounds; small spread |
| Beam saber | – | 90 | swing | 9 m arc sweep with a lunge; blades clash (both parried) |
| Twin Buster Rifle | 8 km/s | 220 | 1 per 5 s | 0.6 s charge, visible to everyone; 5 m beam engulfs the whole suit |
| Beam cannon (Virgo) | 3.5 km/s | 70 | 0.8/s | |

- **Projectiles inherit the shooter's velocity** (it's space). Fire control solves the intercept in
  the shooter's frame.
- **Arms aim.** A hand-held weapon fires anywhere within 50° of the body axis (shoulder mounts 20°),
  so you don't have to point the whole suit.
- **Per-part damage.**
  - Parts: head (sensors), torso (destroyed means dead), arms (their weapons), legs (AMBAC mass,
    some thrust), backpack (main thrusters).
  - Hits on a destroyed limb carry through to the torso at half strength. A hit that blows a limb
    off spills half its excess into the torso.
- **Heat and energy.** Overheating locks all weapons until heat falls to 50%. Beam weapons draw from
  an energy pool that the reactor recharges.
- **Lag compensation.** A shot resolves against the world as its shooter saw it, up to 8 ticks
  (267 ms) back. Details are in `ARCHITECTURE.md`.

## Sensors and visibility

Each frame has a sensor range, and each suit a signature. Boosting multiplies the signature by 1.5
and firing by 1.8 (for 1 s). Anything within 1.5 km is always visible. Losing the head cuts sensor
range to 40%. **The server only replicates what your sensors see**, so fog of war is also the
anti-wallhack. (Deathscythe's Hyper Jammer will plug straight into this model.)

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
  Doll AI; write your own brain in a closure.

## The world (EVE-lite, roadmap)

- The Earth Sphere is split into **sectors**: L1–L5 colony clusters, lunar orbit, Earth orbit, and
  resource satellites (MO-II, Barge). Each sector is one simulation thread (then one process).
  Travel between them is a timed transfer orbit, so chokepoints and interdiction emerge naturally.
- **Time dilation** instead of crashes. When a sector's tick exceeds budget in a huge battle,
  simulation time slows and the snapshot header's `tidi_pct` tells clients to slow their clocks.
- **Factions and territory:** OZ, the Alliance, the Colonies (Operation Meteor), Romefeller, White
  Fang, the Preventers. Mobile Doll production lines are a faction investment. Gundanium is refined
  in zero-G.

## Controls

| Input | Action |
|---|---|
| Mouse (click to lock) | aim |
| W/S · A/D · Space/C | thrust forward/back · left/right · up/down |
| Q/E | roll |
| Shift · X · R | boost · brake · RCS (fast turns) |
| LMB · RMB · F | primary · secondary · beam saber |
| V · Z | flight assist · ZERO System |
| 1 / 2 | respawn as Leo / Wing Gundam Zero |

## Roadmap after Milestone 1

- **Sectors:** multiple sectors with handoff, transfer orbits, TiDi, persistence (Postgres, off
  the hot path), accounts.
- **Suits:**
  - Wing's bird-mode transformation.
  - Heavyarms (missile spam), Deathscythe (Hyper Jammer against the sensor model), Sandrock,
    Shenlong, Tallgeese, Epyon (its own ZERO).
  - Guided missiles, deployable Planet Defensors.
- **Agents:** an MCP server so LLM agents can fly as squad commanders, and a Python gym on the
  headless simulation for RL.
- **Earth:** atmosphere, gravity, re-entry heating (Wing's shield).
