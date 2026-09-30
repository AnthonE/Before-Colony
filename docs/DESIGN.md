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

After Colony 195–196, Earth Sphere. The milestones so far take place in **Sector L1: the L1 Colony Cluster**.
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
- **Pilot G.** Sustained load above 6 g builds G-strain. At 100% the pilot blacks out and control
  authority collapses until strain falls below 50%. A Wing Zero on boost pulls about 12 g, so you
  *can* out-thrust your own body, as Zechs did in the Tallgeese. Mobile Dolls have no body, so no
  G limit. Agents are pilots, so they do have one.
- **Hull contact.** The colony is solid: suits slide along the hull and beams splash against it.
- **Rocks are solid too.** A suit that flies into one stops at its surface, losing its speed into
  it, and slides along it; nothing tunnels, even at 2 km/s. Shots stop at rocks, so a rock is
  cover. The field comes from a seed, so your browser predicts against the same rocks.

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

## Combat

| Weapon | Speed | Damage | Rate | Notes |
|---|---|---|---|---|
| Beam rifle | 4 km/s | 45 | 1.5/s | energy and heat; dodgeable at range |
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
- **Streams.** Rapid-fire weapons (gatlings, machine guns, vulcans) aren't sent shot by shot: every
  client draws their tracers from the firing flags. Only single shots (rifles, cannons, the buster
  shield) are events, and only those are predicted by the shooter's own client.
- **Lag compensation.** A shot resolves against the world as its shooter saw it, up to 8 ticks
  (267 ms) back. Details are in `ARCHITECTURE.md`.

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
  so a jamming Deathscythe slips them. A missile bursts within 4 m of an enemy suit (friends are
  safe), against a rock or the colony, or at the end of its 8 s life.
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
- **Blades lunge**: through the windup and the stroke the suit drives forward at 1.5× main thrust
  (never boosted: holding Shift through a swing doesn't black the pilot out), which adds several
  metres to the reach. The Dragon Fang is Shenlong's arm, so it doesn't.
- **Clashes.** A stroke that meets a suit whose own blade is out and facing it is parried: neither
  does damage, and each recovers for its blade's clash time. The Dragon Fang can't be parried.

## Sensors and visibility

Each frame has a sensor range, and each suit a signature. Boosting multiplies the signature by 1.5
and firing by 1.8 (for 1 s). Anything within 1.5 km is always visible. Losing the head cuts sensor
range to 40%. **The server only replicates what your sensors see**, so fog of war is also the
anti-wallhack.

**Deathscythe's Hyper Jammer** (held on MODE) defeats this model. To its enemies a jamming suit
shows a fiftieth of its signature, and their eyes see it only within 150 m. So it leaves their
screens, and Mobile Dolls, the ZERO System, locks and missile seekers lose it too, until it's all
but within reach of its scythe. Allies still see it, as a
shimmer. The jammer engages with a fifth of the energy pool and drains 30 energy/s against a
recharge of 18, so it runs about 12 s from full. Firing, striking or using a special shows
through it for 2 s.

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
    unassisted at 5 g, which a pilot bears for good.
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
- **Dying** spills the hold and drops what you were holding; someone else can pick it up.

## Mining

The rocks hold ore: most are nickel-iron, some titanium or volatiles, a few exotics. Their veins
show which, and thin as the ore is taken. A rock of radius r m has 60 + 25r of structure and
200r kg of ore, so a 10 m rock takes two saber strokes and holds 2 t.

- **Blades mine best.** A stroke into a rock does double damage and chips off up to 200 kg of ore,
  which drifts free, ready to grab. Machine cannon rounds wear a rock down at their usual damage.
  Beams do 0.3× and boil off 4 kg of ore for each point of damage, so shooting a rock apart wastes
  most of it.
- **A rock with no structure left shatters**: whatever ore is left flies off as 2–8 chunks. Nothing
  meets it until it grows back, 10 minutes later and only once no suit is within 1 km. Rocks crack
  as they're worked.
- **Hulks come apart.** A blade's stroke through a hulk cuts off the part nearest the blade, which
  drifts free as a limb small enough to stow.

## Survival: you build your suit (Milestone 3)

The default rules (`--rules survival`; `--rules arcade` keeps the old game, any frame and free
respawns). Nobody is handed a Gundam. A pilot starts on foot in their own **hangar bay** in the
colony's docking hub, with a worn-out Leo in the gantry (its beam rifle missing, the tank half
full), a little steel, propellant and munitions, and 2,000 credits. Everything better is built
from ore, salvaged from wrecks, or bought from other pilots, and what's flown out can be lost.

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
- **The suit's maintenance console** (by its feet): fit and strip parts and weapons, repair,
  dismantle, and whether the suit would launch.
- **The cockpit hatch** (on the catwalk): board and launch.
- **The airlock** (left wall): the way out of the bay (leave the game).

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
- **Repairs** cost 60% of a part's materials, pro rata; **scrapping** a part or a weapon gives back
  half of what went into it, as worn as it was.
- **Salvage** docked in the hold comes home as ore, or as parts: a limb at 15% condition, each
  part still on a hulk at 40%. Parts of lines nobody can build (a Mobile Doll's) are scrap metal.

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

### Sorties

- **Launching:** board at the hatch. The bay vents, beacons turning red, the doors part, and the
  catapult throws the suit down the 220 m launch tunnel into space at the hub's mouth, inside the
  dock. (Space skips the sequence.)
- **Docking:** come to rest (under 25 m/s) inside the dock's ring of amber lights, off the mouth
  of the docking hub at the colony's −X end, and press Enter. The suit glides in down the tunnel,
  the doors shut behind it, and the pilot climbs out onto the catwalk. What came home goes to the
  stores: the suit as it is, the hold's ore, whatever was in hand, and the bounties earned.
- **Losing it:** a suit destroyed out there is gone, along with its hold. The bounties it earned are
  still paid, and the pilot is brought back to the bay through the airlock once the wreck clears.
- **Away:** a signed-in pilot who leaves keeps everything: their hangar, its jobs, their orders.
  Left out in the sector, their suit sleeps where it is (below), and they wake in it; one the
  sector lost track of is towed in. A guest's hangar lasts the visit.

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
| LMB · RMB · F | primary · secondary · melee |
| H | the frame's special: a toggle for Neo-Bird and the Hyper Jammer, a press for Full Open Attack and the Cross Crusher |
| V · Z | flight assist · ZERO System |
| Tab · mouse wheel | the camera: the cockpit (first person) or the chase camera (wheel in: the cockpit; out: chasing) |
| G · B · T · J | grab (toggle) · stow · throw · jettison |
| Enter | dock (survival): at rest inside the dock's ring of lights |
| 1–6 | arcade rules: respawn as Leo, Wing Zero, Heavyarms, Deathscythe, Sandrock or Shenlong |
| Esc · F1 · F10 | menu · the controls sheet · graphics quality |

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

**Sleeping in the cockpit.** A signed-in pilot who leaves (SLEEP & DISCONNECT in the menu, closing
the tab, a link that stays down, a minute without input) doesn't take the suit along: it stays in
the sector with its pilot asleep in the cockpit, and they wake in it when they're back.
- *Drifting.* Nobody flies a sleeping suit: no flight assist, no attitude hold. It carries on at the
  velocity and spin it had, fully Newtonian, and fetches up against rocks and the colony as a wreck
  would. Its eyes go dark, and brackets read ASLEEP.
- *Parking.* A suit resting against an asteroid (within 1.5 m of its surface, under 3 m/s) when its
  pilot leaves is parked instead: held where it sat, cold and still. Sensors don't find it; eyes
  do, inside 400 m. The HUD reads PARKED while you're somewhere you could park. Shatter the rock
  and the suit floats free. Moon craters are next (one more kind of `Body` to rest on).
- *Hunted.* Mobile Dolls leave sleepers alone; other pilots can shoot them down and salvage what's
  left. A sleeper destroyed stays gone, and its pilot is told by whom when they're back. Parked is
  safer than drifting.
- *Room.* At most 256 sleepers a sector; past that, or when the sector's suits run out, the longest
  asleep is cleared, and its pilot is told. A server restart clears them all.
- A suit that's already a wreck when its pilot leaves is gone, as a guest's suit always is.

**Settings** (from the title or the menu) are kept in the browser: mouse sensitivity, invert Y,
the flight camera, field of view (vertical; the panel gives the horizontal too), camera shake,
first-flight hints and graphics quality, along with the last callsign and frame launched. `bc_client_core::settings` defines them, their ranges and the stored text (a
key this build doesn't know is kept, for the build that wrote it). A new pilot gets one hint at a
time (on foot in the bay: walking, using a terminal, boarding; flying: thrust, boost, fire, the
cockpit view, flight assist, salvage, docking, the menu), each gone once it's been done.

**Sound.** Every sound is generated at boot (`bc-sound`, no audio files): weapons, impacts,
explosions, the engines worked by the throttle, RCS puffs, the lock tone quickening as a lock
builds, missile and low-propellant alarms, the pilot's heartbeat under G, the ZERO System's drone,
the dock and the sale, and a score that crossfades from calm to combat with the fight. There's no
air in space, so it's the cockpit's sound: the suit's own machinery, and the world as the sensors
render it, quieter with distance and gone beyond each cue's range. A blackout muffles everything.
The browser plays it through Web Audio; volumes are settings (master, weapons, cockpit, music).
`cargo run -p bc-sound --release --example reel -- reel.wav` writes a reel to listen to.

**Lock assist.** A frame with missiles designates the hostile nearest the reticle (within 10°) and
keeps it while it stays within 15°. Its bracket fills as the lock builds and reads LOCKED when it's
acquired. The HUD shows the special's state (READY, JAMMING, FIRING, the cooldown), the lock, and
MISSILE LOCK and MISSILE warnings, with a marker on each missile tracking you.

## Roadmap after Milestone 3

Milestone 1 was the playable slice; Milestone 2 the five Gundams (Heavyarms, Deathscythe, Sandrock,
Shenlong, and Wing Zero's Neo-Bird), each flown by pilots and agents; Milestone 3 survival: the
hangar bay on foot, building suits, and the Colony Exchange.

- **The colony, on foot:** the concourse beyond the airlock, other pilots' bays, a bar to meet in;
  crews (a friend's hangar, shared stores).
- **Economy:** contracts (haul this, clear that), insurance, market data for agents, the other
  colonies' exchanges with prices of their own (and hauling between them).

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
- **Salvage:** chunks that collide with each other, miners and pirates flown by the server.
