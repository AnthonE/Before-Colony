# The open city: homes, the street, heat and the Watch

> A multiplayer GTA inside one O'Neill cylinder, on Rust's rules: your flat is a real door in a real
> building that everyone walks past, and what's behind it is yours for as long as you can keep it.

The owner's ask (October 2026): "people can have houses and apartments, and it's not instanced. Let's
make this like a real multiplayer GTA in space. Forget space and mecha for a minute: we can ace
everything else perfectly." Asked how far to go, the owner chose all of it, in phases: homes, cars you
own, crime and the police, collisions, and businesses. And they said to take whatever helps from
Gates, the survival game this one shares a timeline with (`AnthonE/Gates`).

`STORY.md` is the world this lives in, `LIFE.md` the colony's jobs, meals and great works (whose
GTA, Sims and WoW brief this continues), `COLONY.md` the city it's built into, `PEERS.md` what the
nearest games teach and `ROADMAP.md` how its items are built. This document is the design for the open
city, and its phases (below) are built the way `ROADMAP.md` builds its items: thin, joined end to end,
each tested.

## Status

| Phase | State |
|---|---|
| D: this design | written |
| H0: buildings you can walk into: lobbies, stairs, lifts, corridors, flats and houses as closed forms of the city | built |
| H1: a door of your own: leases and deeds, the code lock, the panel, rent as upkeep | built |
| H2: furnishing: furniture bought and placed, seen by everyone who comes in; storage | built |
| H3: your bed: asleep at home when logged out, waking there | built |
| S1: the street in the tick: people on foot and their vehicles in the interior sector | in progress: the shared models (`bc_sim::foot`, `car`, `markab`), the homes overlay and the v39 codecs built; the tick next |
| S2: collisions: cars meet cars and people | planned |
| C1: cars you own | planned |
| X1: heat and the Watch | planned |
| X2: a huge deal: incidents the whole strip sees | planned |
| I1: intrusion: the Astrolabe's skills | planned |
| R1: raids | planned |
| B1: businesses: shopfronts leased like flats | planned |

## The owner's decisions

Asked in planning (October 2026), and answered:

- **Homes are not instanced.** A flat is at its own address in a building of the city. Everyone sees
  the same door, the same lit window, the same people going in. (GTA Online's apartments are
  instanced: a door you walk through into a copy. Ours is the door.)
- **Flats are rented, houses bought.** Rent by the week, paid ahead into the home's panel; houses and
  the best flats bought outright and sold back.
- **Visitors and break-ins: like Rust, with raid hours.** A code lock and friends authorised at the
  home's panel (Rust's tool cupboard). Homes can be broken into, but only while the owner or one of
  their crew is online, or in the strip's raid window. Unpaid rent is Rust's upkeep: the home lapses.
- **Furniture in the first version:** bought, placed, and seen by everyone who comes in.
- **Crime anywhere.** No sanctuaries: but this is a colony in a pressure hull, and its police are
  suits and drones that "don't hesitate to be deadly". Anything crazy is a huge deal, for the city's
  people and for the players.
- **Death, Rust's way.** You drop what you carry, in a bag anyone can loot, and wake in your home's
  bed, or at Hub Gate if you have none.
- **Cars collide and can be stolen.** And the pilots are "jacked with tech skills", after Ghost in
  the Shell: they can get into machines.

This reverses three things the docs said before. `LIFE.md` 5 recommended that "the homes, the kitchens
and the bars stay exactly as safe as now"; `PEERS.md`'s Star Citizen lessons kept vehicles from
meeting each other or people; and `ROADMAP.md`'s "Where PvP lives" was open. All three are decided
here, and those documents point here.

## What it has to be

1. **Not instanced, and never seamless by brute force.** The city is a closed form of where you
   stand (`bc_sim::colony::city`), so a building's floors, flats and doors are too: nothing of the
   building is stored or sent, only who holds what. Where a change of place would cost us
   seamlessness, we stage it, as the cap lift does (`PEERS.md`, Star Citizen): a lift's ride is a
   curtain, not a moving room.
2. **Yours means yours, until someone works for it.** A locked door holds against anyone without the
   code, enforced by the server on every pose, not by a wall the client draws. Breaking in takes
   charges, skill and the raid hours, and draws the Watch.
3. **Every new verb is asked: can it be used on someone who didn't opt in?** (`LIFE.md`, "What we
   watch".) Living in the colony is opting in to the colony's dangers, as living on a Rust server
   is. What we promise instead: you're never shut in your own home (the door that opens for you
   opens out too), never locked out of your own lock, never raided while you sleep outside the
   raid hours, and never lose your bay or your suit to the street.
4. **Crime is rare and loud.** A gun in a pressure hull is news (`LIFE.md` 5). The Watch answers
   fast and kills; the crowd runs; the trams stop; the radio says who. It should feel like the
   biggest thing that happened all week, because it usually is.
5. **Connect before deepening** (`PEERS.md`). A home sends pilots along the chain: it's where you
   wake, eat, keep what the bay can't, and come back to; the smuggled gun and the breaching charge
   come up the cap lift from the bay's fabricator; the Watch's suits are suits.
6. **Take what's built.** Gates already runs most of Rust's rules on the same stack (a deterministic
   simulation with no allocation in its tick, a Bevy client, WebTransport). Its numbers are tested
   in play; we take them before inventing ours ("From Gates", below).

## Who did it before

- **GTA Online (2013):** a 48-minute day, as ours is; apartments and garages bought on a phone and
  entered through a door that loads an instanced copy; a wanted level of stars that climbs with what
  the police see and falls when they lose you. Its lesson: the wanted level is the fun, and the
  instancing is the seam everyone notices.
- **Rust (2018):** bases with code locks, the tool cupboard's list of who may build, upkeep paid
  from the cupboard or the base decays, raids with explosives, sleepers you can kill, a backpack
  dropped at death, a sleeping bag to wake in, the Outpost's turrets that shoot anyone who draws a
  weapon there. Softcore servers protect bases outside a raid window. Rust's own fixed homes
  (its apartment buildings: bought or rented, a master key, break-ins) are the nearest thing to
  ours.
- **Gates (ours):** Rust's rules rebuilt on our stack and tested (below).
- **Ghost in the Shell:** bodies with machines in them, and minds that get into machines: cars,
  cameras, other people's eyes. Ours are the Arrivals, who already include minds without bodies
  (`STORY.md`), and the Astrolabe (I1).
- **The Sims:** a home you furnish and live in, and the body's clock (`LIFE.md` 2).

## Homes

### The buildings (H0)

*A closed form of the city: `bc_sim::colony::homes`, allocation-free and deterministic like
`city`.*

**Which buildings are homes.** In a Residential district, every building whose body is one box
(`Form::Block`, or a `Form::Setback` with its tier left solid) and is 2 to 10 storeys tall is a
block of flats. In Old Town, every building of 2 to 4 storeys with a street side is a row of
terraced houses. Courts, towers, the Gardens' terraces and the penthouse tiers stay solid for now
(a penthouse is a later step: its tier's one flat, round the core). That's a few thousand
buildings a strip and some 100,000 homes in all, none of them stored.

**A block of flats**, worked out from its foot and its storeys (`FLOOR` 3.6 m, `GROUND_FLOOR` 5 m):

- **Its lobby door** in the middle of a street side (the short one, so its corridor runs the long
  way), or of an alley side if it has no street. A building's walls are a shell 0.3 m thick.
- **The core** across the front, inside the lobby door: the landing in the middle, the lift on one
  side and the stairs on the other, and a service room either side of those (bins and bikes on the
  ground floor, plant above).
- **The stairs:** two flights to a storey with a landing between, 0.2 m risers (under the walker's
  0.45 m step) on 0.28 m treads.
- **The lift:** at its doors on any storey, E rides a floor up, Q one down and a digit to that floor
  (0 the ground); the curtain falls and lifts, and you're out on that floor (a staged ride, as the
  cap lift's: the plaza lets a pose go from one of a building's lift doors to another's).
- **The corridor** from the core to the back wall, 2.4 m wide, and **the flats** either side of it,
  each from the corridor to the outer wall, about 8 m along it: from four flats a storey on a narrow
  building to eight on a wide one. Each flat's door is in the corridor's wall.
- **Windows** on the shell's outer walls at a bay of the rules (`homes::BAY`), which the facade's
  shader keeps (`city_atlas.rs`'s tests check its number), so a flat's windows are where the street
  sees them.

**A row of terraced houses:** the building cut along its street side into houses about 7 m wide,
each from the street back to a depth of 14 m (what's behind them stays solid), each with its own
front door and a straight stair against its party wall, 2 to 4 storeys. Old Town's middle lots have
no street and stay solid.

**Addresses.** A home is a `HomeId`: its strip, block, lot, storey and unit, in 32 bits. Its address
reads like one ("Flat 6B, 14 Firsthomes Row"), and its size, its view (the avenue, the canal, a park,
or a yard) and its rent or price are closed forms of where it is.

**Walked like any wall.** The plaza and the walker meet a home's walls through the same `city::solid`
they meet the street through. A building is at most eight boxes (`MAX_SOLIDS`), and a hollow one is
hundreds, so a query has a grain: people and cars (`Grain::Fine`) get only the boxes near them in a
home (worked out from the storey and the cell they're in), while suits and the shaders' map
(`Grain::Coarse`) keep the building's envelope as before. A suit can't get into a flat.

**Drawn as it is.** Up close the client builds the inside of the building you're in or at the door
of, the storeys either side of yours, from the same boxes (`city_mesh::home_inside`); the city's
chunks cut its doors through its front close up. A building's outer faces point outwards and aren't
drawn from inside, so its windows show the real street. Inside, it's lit as a key place's room is:
by its lamps, the eye adapting a couple of stops under the street's (`room_ev`), and the street
through the windows lit by its own light. Name tags of people on another storey or in another home
aren't shown (`homes::same_space`).

### A door of your own (H1)

*`bc_econ::homes` (the colony's register of homes, with Gates' lock ported into it, saved as
`homes.json` beside `charter.json`) and `bc-server/src/homes.rs` (the shared register, as
`proving.rs` holds the board). When people move into the tick (S1), the tick takes a fixed-size copy
of what it needs: the shut doors.*

**Renting and buying,** at the home's door. A free flat's door says what it is and what it costs
(E: rent it, a week or more paid ahead). Houses, and flats on the top storey of their building, are
bought (E: buy), and sold back to the colony for 80% of their price. Prices are numbers in
`content::homes`, provisionally:

| | |
|---|---|
| A flat's rent | 300 cr a week for 60 m² on a low storey, more for size, height and a view: a third of a patrol's pay or so (`charter::PATROL_REWARD` is 1,500) |
| A house | 60,000 cr and up |
| A top-storey flat | 40,000 cr and up |
| Most homes held at once | 2 |

**The panel is the cupboard.** Inside every home's door is its panel (Rust's tool cupboard, Gates'
hearth). It holds:

- **The rent, paid ahead.** Anyone may feed it credits; only the home's crew may take them out. It's
  spent by the hour. With a crew of more than four, each extra member adds 2% to the rent (4% past
  ten, capped at three times the rent: Gates' `group_pct`), so a flat stays a home, not a clan hall.
- **The crew:** up to 10 pilots who may open the door and use what's inside. A pilot joins by
  entering the code; anyone on it may leave; the crew may clear it to themselves. The tenant or owner
  is always on it. No pilot's name or key ever goes on the wire to build it: you can only add
  yourself, Gates' way.
- **The lock's codes.**

**The code lock** is Gates' (`sim-core/src/lock.rs`), as it is:

- A main code and a guest code, four digits each. A new home has the tenant's key alone.
- The right main code puts you on the crew; the guest code lets you open and close the door, nothing
  more. Setting the main code clears both lists, keeping only whoever set it.
- Wrong codes shock you (5 hp a miss, more for each one in a row; never lethal, reset after 10 s),
  and eight in a row lock the keypad for 15 minutes. (People on foot have no health till X1: until
  then the shock is a jolt and a word.)
- A knock (its own button on the keypad) is heard by the home's crew, wherever they are in the city.

**Doors** are open or closed. Within 3 m of a door, the same check opens and closes it, so nobody can
shut you in (Gates' `use_door`). The server is the lock: a pose that crosses into a home past a closed
door it doesn't pass is refused (`Why::Locked`), however thin the door is. Leaving is never refused.
Your own door opens at once on your screen and is confirmed by the server (Gates' `predict_door`).

**When the rent runs out:** a day's grace, and the lease lapses back to the colony. What was inside
goes to the tenant's bay stores, never lost. A bought home has no rent, but it must be lived in: one
left unvisited for 30 days is sold back to the colony at its price.

### Furnishing (H2)

Furniture is bought from the panel's catalogue (delivered by the colony's staff for now: a courier's
seat later, `LIFE.md` 1) and placed in build mode: a ghost of the piece, R to turn it, E to put it
down. The ghost (green where it fits, red where it doesn't) runs the same rule the server checks
(`content::homes::fits`, Gates' way), so what fits on your screen fits:

- inside the home, on its floor, clear of its walls, stairs and the other pieces;
- never in front of a door (a metre clear), so nobody is walled in or out;
- at most 64 pieces a home.

The catalogue (`content::homes`): a bed (H3), a sofa and an armchair, a table and chairs, a kitchen
line (`LIFE.md`'s cooks, later), a wardrobe, a chest and a safe (storage), lamps that switch, a screen
showing the colony's news, plants, rugs, and a model of your own suit in its livery on a stand
(`PEERS.md`: people want their own build). All procedural (`DESIGN.md`: no asset files), sold for
credits, a sink; sold back for half.

**Storage.** A chest holds 12 kinds of things from the bay's stores, a safe 6 and is harder to break
(R1). Each move in or out is all or nothing. Only the crew may open them, and what's inside is sent
only to the crew: everyone else sees the chest, not what's in it. A piece that holds something can't
be sold; a home given up or lapsed sends its furniture's half back and what its chests held to its
holder's bay.

Everyone who comes in sees the same pieces in the same places: they're part of the register, sent to
anyone in the building.

### Your bed (H3)

`LIFE.md` 2 says an Arrival asleep is logged out, "in their bunk". With a home, they are: log out in a
home whose crew you're on, with a bed in it, and you lie asleep in its free bed nearest you (a
sleeper, Rust's and Gates' word). The sleeper is the register's (`Homes::sleep`), not the plaza's: it
goes with the home's line to everyone near (`HomeLine::asleep`) and is drawn under the covers, and a
bed with someone asleep in it can't be sold. Log in and you wake beside it (`content::homes::wake_spot`,
the first side of the bed with room to stand), on foot in the city and not in the bay: the server
puts you there (`Plaza::enter_at`) and tells your client where (`Update::Place`'s `wake`). Taken off
the crew while you slept, or the bed gone, you wake in the bay. A bed is also where you'll wake after
dying (X1).

## The street in the tick (S1, S2)

Today a pilot on foot is a pose in the plaza: off the tick, checked for plausibility, and harmless.
Nothing can hit them because nothing can be decided about them fast or fairly enough. Crime, cars that
collide and police that shoot all need the same thing suits already have: **people on foot and their
vehicles as entities of the interior sector** (`sector-1`), simulated by its tick, predicted by their
owner and lag-compensated for everyone else.

- **The walker moves into the simulation** (`bc_sim::foot`, built), shared by the server and the
  owner's predictor as `ground::move_step` is, against `city::solid` and the homes' doors and
  furniture. Its trig is libm's, so it steps to the same bits on both (`FOOT_GOLDEN`, native and
  wasm); `Walker::step` keeps the browser's frame-rate walk and `Walker::tick` is the tick's,
  exactly four substeps of 1/120 s. The browser walks it today exactly as it walked before (the
  bay, the city, a flat's stairs), and walkers stepped by the tick to every key place and up a
  block's stairs are poses the plaza takes (`bc-server/tests/foot_plausible.rs`).
- **The homes' overlay** (`colony::homes::Overlay`, built): the held homes' shut doors and solid
  furniture in one fixed-size table, sized once (a home's `OverlayHome` holds its door, shut or not,
  and up to `content::homes::MAX_PIECES` solid pieces' boxes; it's `Copy`, to ride a preallocated
  queue). A home with no room in it is counted (`Overlay::overflowed`) and its door is open to the
  overlay, so the tick sizes it from the register with headroom. Every box in it is public: what one
  pilot meets, everyone meets. A shut door is shut for everyone, its crew too (they open it, with the
  `door` request, before they walk through), as every client already drew it. The server builds a
  home's from the register (`HomeRegister::overlay_home`) and each client the same from what it was
  told (`bc_client_core::homes::overlay_from`), with its own door just worked on top until the
  register agrees; its index is kept sorted along the strip as homes are set, so a walker's query
  looks only at the homes that could reach it.
- **The cars and the Markab move into the simulation too** (built): `bc_sim::car` (the bicycle
  model, its whole state its seven fields, `Vehicle::tick` two fixed substeps; `CARS_GOLDEN`) and
  `bc_sim::markab` (the rocket car, its fan, its jets and the faces it drives; the city round it
  gathered each step into a fixed `Near` of `MAX_NEAR` solids, twice the densest a step at its top
  speed meets; its whole state, the private parts too, a `MarkabState` it's made again from to the
  bit; `Markab::tick` four substeps; `MARKAB_GOLDEN`). The browser drives both as before.
- **No allocation, no locks:** the interior sector's storage is sized at start-up (pilots on foot,
  their vehicles, the Watch), under the `no_alloc` proofs, like every other tick.
- **Gates' netcode where ours has none on foot:** the last inputs repeated in every input packet,
  and the server alone deciding how far back a shot looks (`rewind.rs`, `stats::favour_for`),
  beside our `lagcomp.rs` for suits. But **the own walker goes exact**, not rounded and simulated as
  rounded (Gates' way): the walker rests a hair above a floor (its settling and falling stop at the
  last clear point), so its feet rounded down to the wire's 7.8 mm grid put its box in the floor,
  every stride then fails and steps it up a stair's height, and it hops; rounded into a wall, it
  sticks. A suit's own state already travels at full precision for the same reason, so the own
  walker does too (`FootOwn`, raw `f32` bits: about 150 bits a tick more than a rounded record, to
  one pilot) and the simulation keeps its state. Everyone else goes on the plaza's grid,
  delta-coded.
- **The plaza stays** for the people further off than the sector replicates (the far crowd at Hub
  Gate), and for the colony's clock on foot.
- **Collisions (S2):** cars meet cars and people. A person hit is knocked down and hurt by how fast
  (X1's health); a driver who hits someone draws heat. The city's own traffic stays a closed form and
  keeps out of everyone's way; the cars players drive are the ones that hit.

## Cars you own (C1)

- **Bought at the motor pools,** which keep lending today's cars as before. An owned car is the owner's
  colours and plate (`bc_client_core::life::car_tag`'s paints).
- **Kept in the world:** left at the kerb or in a house's garage, a car stays where it was parked, seen
  by everyone, and is saved as a parked suit is (`PilotRecord.parked`, restored at boot).
- **Its crew** is a roster, as a home's is (Rust's `VehiclePrivilege`, Gates' `roster.rs`): who may
  drive it.
- **Stolen:** anyone can take a car by hotwiring it (I1). The car's alarm draws heat; its owner is told
  where it went and when (a tracker on the map), and can take it back.

## Heat and the Watch (X1)

**Guns come in from outside.** Sidearms are made in the bays' fabricators and carried down the cap
lift, past customs (`LIFE.md` 5: a seat, Papers, Please), or brought in by the freight canal. What a
pilot carries on foot is their pockets: a small inventory of their own, dropped when they die.

**People can be hurt.** Health, and Gates' downed state: a mortal blow knocks you down for 40 to 50 s,
crawling, and a friend can lift you up in 6 s within 3 m (`wound.rs`). A second blow, a head shot or a
blast kills.

**Death, Rust's way.** You drop a bag with what you carried; it lasts 5 minutes, or 20, 40 or 60 by the
rarest thing in it (Gates' `backpack.rs`). The death screen has no timer: wake in your bed (not again
within 5 minutes) or at Hub Gate. Your bay, your suit and your home are untouched.

**Heat** is how much the Watch wants you, 0 to 5:

- **Seen, not done.** A crime adds heat only when it's seen: by the crowd (how many of the city's
  people are where it happened is a closed form, `colony::walkers`), by a camera, by another pilot. A
  ledger of what was done to whose things (Gates' `trust.rs`: doors, locks, boxes, bags) is the record
  it's drawn from.
- **What raises it:** a weapon drawn in the street; a blow; a shot; a death; a car taken; a door
  breached.
- **What lowers it:** time unseen; leaving the strip; the colony's fines (paid at Hub Gate's customs).

**The Watch** answers, and doesn't hesitate:

| Heat | Who comes | What they do |
|---|---|---|
| 1 | The **Sirius** hounds, the Watch's drones (the Dog Star: the colony's watchdogs) | Follow you and call it in |
| 2 | More hounds | Warn once, then shoot to drop you |
| 3 | The hounds, and a Watch suit | The suit lands in the street with the grip and hunts you |
| 4 | Watch suits | Lethal on sight |
| 5 | Everything the Watch has | The strip locks down (X2) |

The **Watch's suits** are named for the royal stars the Persians called the Watchers: **Aldebaran**,
**Regulus** and **Fomalhaut** (the fourth, Antares, is a Lodestar already). They're suits, flown in
the interior sector by the server's AI, landing on the street as a pilot's do (`Body::City`), with
anti-personnel rounds that touch no hull (the Blast Hall's rule, `colony::hall`). Hounds and suits
warn before they fire, Gates' sentry's way (a tone, then 1.5 s, then the burst), and answer fastest
where the colony keeps its guns: at Hub Gate and the desks, a drawn weapon is answered in seconds.
There are no sanctuaries; there are places it's very stupid to start something.

How many suits the Watch has is the colony's business: the militia's hangar, a great work already on
the Charter Board's list (`STORY.md`, era 0), gives it more.

## A huge deal (X2)

Something crazy happening in the colony is the biggest thing all week, and everyone sees it the same
way:

- **Incidents are world state:** a short fixed list (where, when, how bad), carried to every client as
  the building site's `Stage` will be. The crowd's and the traffic's closed forms take it as an input,
  so the same people run from the same corner and the same cars stop on every screen.
- **The strip reacts:** at heat 5 the strip's trams stop where they are, the cap lift locks, and the
  Watch closes the strip's bridges.
- **Everyone hears:** the radio, the news, the Most Wanted at The Arrival, and a bounty on the one who
  did it, payable by the Charter Board to whoever brings them down (the aces' bounties' way).

## Intrusion (I1)

The owner's "jacked with tech skills". Every Arrival woke in the docking hub with an **Astrolabe**: an
interface behind the ear, the reading instrument to the Orrery's model of the heavens. It's how a
pilot's reflexes reach a suit's controls, and it reaches other machines too. An Arrival without a body
is all Astrolabe.

Its skills grow by doing (`LIFE.md`'s `Skills`, never a combat edge in a suit):

- **Locks:** crack a code lock instead of breaching it: quiet, slow, and only in the raid hours.
- **Cars:** hotwire one.
- **Cameras:** blind one for a while, so the Watch doesn't see what you do.
- **Hounds:** turn a Sirius hound for a few seconds, or make it lose you.
- **Hardening:** fit your own lock with countermeasures that make the above slower.

Each is a minigame the server seeds and checks: the client plays it, the server knows the answer
(Gates' dial puzzles, `mech.rs`, and keycard doors, `monument.rs`). An agent plays the same
minigames through the same requests.

## Raids (R1)

- **Hit points:** doors, walls and windows by tier, and Gates' rule that a door is always weaker than
  its wall. A safe is harder than a chest.
- **Breaching charges** are made in the bay's fabricator and smuggled in. Planted, fused, sometimes a
  dud, and a limit on what one tick can bring down (Gates' `charge.rs`).
- **Raid hours.** A home can be broken into while its owner or one of its crew is online. Otherwise,
  only in its strip's raid window: four hours on the wall clock, staggered by strip so one is always
  open (Charter 18:00 to 22:00 UTC, Canal 02:00 to 06:00, Gardens 10:00 to 14:00; tunable). A new
  lease is safe for its first hour (Rust's Softcore servers).
- **What's at stake:** the chests and safes, and the sleepers in their beds (killed and looted inside
  the raid hours only). Never the bay, the suit or the home itself.
- **The Watch comes** to a breach (X1), so a raid is a race.

## Businesses (B1)

The ground floors the facade paints as shops (`G_SHOPS`, `city_facade.wgsl`) become shopfronts,
leased like flats: Rust's rentable shops (rent by the hour, a takeover that costs more each time, 24
hours to clear out). Inside, `LIFE.md`'s seats: a bar or a kitchen of your own, a garage, a clinic, a
furniture shop, worked by its owner, by Arrivals they hire, or by the colony's staff.

## From Gates

Gates (`AnthonE/Gates`, MIT) runs Rust's survival rules on the same stack and has tested them in play.

**What we take as it is:**

- **The code lock:** main and guest codes, a right code that authorises rather than opens, the lists'
  caps, the shock and the lockout, the knock.
- **The cupboard's crew,** self-service, its cap, and no pilot's key on the wire.
- **The rent's group tax,** and a day's grace.
- **The bag dropped at death,** its time by rarity, the death screen with no timer, the bed's cooldown.
- **The sentry's warning** before it fires.
- **A door is always weaker than its wall.**
- **Raid windows,** from its survey of Rust's Softcore.

**Code we port,** reshaped to our idiom and our crates' rules (`bc-sim`'s bans; fixed storage), each
module crediting Gates:

| Gates | Ours | Phase |
|---|---|---|
| `lock.rs`, `roster.rs` | `bc_econ::homes::Lock` and its capped lists | H1 |
| `deploy.rs`'s `use_door`, `crew_op`; `client-core`'s `predict_door`, `rollback_door` | the home's door and panel | H1 |
| `footprint::Pose`, `body_site`'s checks | furniture placement | H2 |
| `inventory.rs`'s `plan_move`, `resolve` | storage | H2 |
| `rewind.rs`, `stats::favour_for` | shots at people | S1, X1 |
| `wound.rs`, `backpack.rs`, `World::die`, `World::wake` | being hurt, dying, waking | X1 |
| `trust.rs` | heat's ledger | X1 |
| `sentry.rs` | the Watch's warning and burst | X1 |
| `charge.rs` | breaching charges | R1 |

**What we leave:** claims by flood fill, building pieces, support and collapse, and the wipe. Our
homes are fixed and our world doesn't wipe.

## On the wire

H0 to H3, on the control stream as the hangar's messages (tag 11, `bc_econ::wire`); `Request::desk()`
says where each is allowed, and the server checks it against the pilot's last pose:

| Request | Where |
|---|---|
| `rent { home, weeks }`, `buy { home }` | at the home's door |
| `enter_code { home, code }`, `door { home, open }`, `knock { home }` | at the home's door |
| `give_up`, `set_code { home, guest, code }`, `lock`, `leave_crew`, `clear_crew`, `feed`, `take_rent` | at the home's panel, inside |
| `furnish`, `move_piece`, `sell_piece` | inside, on the crew |
| `stash`, `unstash` (between a chest or safe and the bay's stores) | inside, on the crew |

There is no request to sleep: logging out at home is sleeping there.

`Update::Homes` carries the held homes of the buildings within 30 m of you: each home's door (open,
locked), the name on it, what you may do there, its furniture, and who's asleep in its beds; its
panel (the rent on it, its crew's size, whether its codes are set) and what its chests hold go to its
crew alone. A home not in it is free. It's sent again when the register moves (its version) or you
come near other buildings.

S1, on the datagrams (v39, `bc_proto::foot`; `docs/PROTOCOL.md`). Built and tested; nothing sends
them until a server walks people in the tick (behind a later `--street`):

| What | Where | Bits |
|---|---|---|
| `hello_flags::STREET` | the Hello: this client walks in the tick | 1 << 2 |
| `welcome_flags::STREET` | the handshake's Welcome: this server will walk you there | 1 << 6 |
| `InputCmd::act` (`FootAct`: sit, stand, ride a lift, take a vehicle, get out) | every command; cleared by `neutral` and by `stand_in`, kept by `quantized` | 1, + 24 with one |
| `header_flags::ON_FOOT` | a walker's snapshot, answering for their input (not SPECTATOR) | bit 2 |
| `foot_own` (`FootOwn`) | an ON_FOOT snapshot, after the objects: the own walker exact, and its mode (on foot, a train, a seat, a lift) | 1 + 262 on foot |
| `people` (`PersonRecord` as `QPerson` codes) | after it: the people near, whole or as deltas against acked records, as suits' are | 1 + 112 whole, about 47 at a walk |

H0 moved `CITY_VERSION` to 5 and `PROTOCOL_VERSION` to 36, and the city's hash (`CITY_GOLDEN`)
with them; the homes have their own (`HOMES_GOLDEN`). H1 to H3 add only JSON (`Update::Place`'s
`wake`, the homes' requests and lines), which an older page ignores.

## Tests

- **H0:** every flat and house reached from its door by the walker, up the stairs and by the lift;
  flats never overlapping or leaving their building; `unit_at` finding each one; the goldens; no
  allocation in the fine queries; the plaza passing stairs and lifts and refusing walls.
- **H1:** the register's rules (rent, the group tax, lapsing, every credit accounted for, Gates' lock
  cases: `bc_econ::homes`); the plaza refusing a stranger's pose into a locked home and letting the
  code's holder in (`plaza.rs`); `bc-server/tests/homes.rs`: every home request refused away from the
  home's door.
- **H2:** placement's rules (`content::homes`); everyone near seeing the same pieces and only the crew
  what a chest holds (`bc_econ::homes`); a shut door and furniture stopping the walker
  (`bc_client_core::homes`).
- **H3:** `bc-server/tests/homes.rs`: a pilot asleep in their bed wakes beside it in the city, logs
  out there and is asleep in it again, through a restart.
- **S1 so far:** `bc-sim/tests/foot.rs` (the walker stands without drifting, stops at a wall, steps
  a kerb, a bench and stairs, jumps as the colony's gravity says, lands from a fall; a tick is four
  substeps to the bit), `FOOT_GOLDEN`, `CARS_GOLDEN` and `MARKAB_GOLDEN` native and wasm, the
  `no_alloc` tests for all three and the overlay; `bc-server/tests/foot_plausible.rs`; the overlay
  against a look at every box (10,000 random queries), full, and set then removed, and
  `bc-server/tests/homes.rs`'s `the_registers_overlay_is_what_the_crew_and_strangers_see`; the car's
  copy and a Markab made again from its state driving on to the bit; and `bc_proto::foot`'s codecs
  (the own walker to the bit in every mode, a person within half a step, deltas rebuilt to the bit
  and refused without their baseline, the decoders never panicking).
- **H3 and the browser:** the `home` e2e suite (`scripts/e2e.sh home`): a flat rented, the lift
  ridden, a code set; a second browser knocks, is refused, enters the code and comes in; a sofa
  placed and seen by both; logged out in bed and woken there.

## What we watch

- **The street must stay worth walking.** If crime makes the city a place nobody goes, the Watch is
  too slow or too weak. If nobody ever tries anything, it's too fast.
- **Raids must cost the raider.** Charges are dear, the Watch comes, and the hours are limited.
- **The hot path stays untouched** until S1 moves the street into the tick, and then it's held to the
  tick's rules like everything else in it.
- **The city's goldens move once per phase, on purpose.**

## Open questions

- Whether a guest (no wallet) can rent a home. The recommendation: no; a home needs a record.
- How many homes the colony lets one pilot hold, and whether an agent may hold one (`LIFE.md` 8's
  citizenship vote can decide it).
- Penthouses: the setback tier's one flat round the core.
- Seeing into other people's homes through their windows (today the facade paints its own rooms).
