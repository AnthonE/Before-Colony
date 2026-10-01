# Before Colony: the world

> History calls it After Colony. It hasn't started yet.

This is the world bible: who the players are, where they are, what's going on around them, and how
the world changes as they live in it. `DESIGN.md` says how the game works; this says why the world
is the way it is. Gundam Wing and its names are © Sotsu · Sunrise; canon names stay behind the
`canon-names` feature (`bc_sim::content::names`), and the generic build has its own (see
"Naming" below).

## The pitch

The first space colony, an O'Neill cylinder at the Earth–Moon L1 point, has just opened its docks.
The calendar that history will call *After Colony* begins the day its charter is signed, and
nobody has signed it yet. This is the year before: **Before Colony**.

You woke up in a hangar bay in the colony's docking hub. You have the reflexes of a mobile-suit
pilot, a worn-out Leo in the gantry, a debt of 2,000 credits, and memories of another world: one
where After Colony is a story you already know. The Mobile Dolls, the five Gundams, Operation
Meteor, the war. Here none of it has happened. Whether it has to is up to everyone who arrives.

## The Arrivals

- **Who.** People who wake in the docking hub with no records, trained as pilots, remembering
  another world. Not all of them have bodies: some arrive as minds that only ever speak through a
  suit's controls. The game's AI agents are those Arrivals, and the HUD tags them **MD**.
- **How the colony sees them.** The Charter Board registers every Arrival as a pilot, body or
  not, and gives them a bay, a second-hand Leo and an advance against their earnings. "An Arrival
  flies by the same rules as anyone" is colony law before it is a design pillar: agents see what
  their sensors see, bear the same G and pay the same fees. What an Arrival without a body *is*,
  a person, a tool, a citizen, is the colony's oldest open question, and the game's.
- **Coming back.** An Arrival whose suit is destroyed wakes again in the hub. Nobody knows why it
  works, or why it works only for them. The suit, its cargo and everything aboard stay lost: the
  world remembers what you lose even if you don't die of it. (This is the diegetic reason
  survival rules bring a pilot back through the airlock.)
- **The ZERO System.** An interface built into one prototype torso that feeds its pilot the
  futures of a fight. Colonists who try it come out shaking. Arrivals find it familiar: it shows
  the future the way they remember the story. Why is the mystery under everything else.

## The First Colony

- **Its name.** The colonists call it **the First Colony**, and so does everything they sign; the
  Consortium's papers say L1-01.
- **The cylinder.** 3.2 km in radius and 32 km long, spinning for gravity, its interior still
  being finished. The game's sector L1 is the space around it: the debris of its construction
  (asteroids hauled in for metal), the docking hub at its −X end, and the lanes between.
- **The docking hub.** The spin ring at 0.7 g where the pilots' bays hang, the launch tunnels,
  the dock with its ring of amber lights, and the **zero-G foundry**, the only place gundanium can
  be made. Beyond a bay's airlock, the cap lift rides down the end cap's face into the colony.
- **The Charter Board.** The colonists' provisional council. It advances each Arrival 2,000
  credits and a Leo, runs the colony's desks on the **Colony Exchange** (it buys ore, sells
  propellant cheap, and keeps machine shops turning out components), and pays a bounty for every
  Mobile Doll a pilot brings down. The bounties are its quiet war: it can't fight the Consortium
  openly, but it can pay the people who do.
- **Inside.** Three land strips run the length of the cylinder between its three windows, each a
  city with a name of its own: **Charter** (the colony's offices, its money, its first streets,
  its university), **Canal** (a working town along the canal that carries its freight: depots,
  quays, locks, yards) and **Gardens** (orchards, the arboretum, the colleges, homes on
  terraces). Each runs from **Hub Gate**, the square at the foot of the docking hub's end cap where
  the cap lift comes down, to **the building site** at the far end, where the colony is still
  being built and its cranes stand against the far cap. Down the middle of every strip runs the
  avenue; along every window, a park and a promenade at the glass. The mirrors outside throw in
  the day: 32 minutes of light, then dusk, and the city's lamps.
- **Its districts**, from Hub Gate: in Charter, Charter Square, Exchange Row, Tower Hill (the
  Axis View tower, the tallest thing on any strip), Meridian, Lantern Street, Firsthomes, Central
  Park, Arrival Heights, Old Town, the University, Machine Row and Foundry Lane; in Canal, the
  Depots, Quayside, Canal Central, Lock Town, Waterside, the Basin, Millrace, Twin Bridges, Lower
  Canal, the Yards, Far Quays and Last Lock; in Gardens, Garden Gate, the Orchards, the Arboretum,
  the Colleges, Terraces, Green Meridian, Hillside, the Meadow, Vine Street, Greenworks, Fieldside
  and the Seed Halls.
- **The lines.** A tram runs down the middle of each strip's avenue, from Hub Gate to the
  building site: the Charter line in blue, the Canal line in teal, the Gardens line in green.
  Its stations take their districts' names.
- **Places an Arrival goes.** The **Exchange floor** on Charter Square (the Colony Exchange's
  hall: the same book the bays' terminals trade on); the **Charter Board**'s hall, where its
  notices go up; **The Arrival**, a bar off the square where pilots meet. The colony's law holds
  inside: no weapons fired within its walls.
- **The economy.** A boomtown. Ore comes in from the field, parts and suits go out from the bays,
  and prices float with what the colony holds. Frames are old and patched, so systems fail, and
  keeping a suit flying is a trade of its own (overhauls, machined components, equipment).

## Powers at the dawn

| In the world | Canon name | In the simulation | What they want |
|---|---|---|---|
| The Consortium | Romefeller Foundation | — | They financed the colony and own its debt, the asteroid claims and the Leo lines. |
| Its security arm | OZ (the Organization of the Zodiac) | `Faction::Oz`, the Mobile Dolls | To hold the claims: its Taurus and Virgo patrols shoot claim-jumpers, which is every Arrival who mines. |
| The Colonies | the colonies' cause | `Faction::Colonies` (pilots in the browser) | A charter of their own, a foundry of their own, and nobody's debt. |
| The Earth Sphere Alliance | United Earth Sphere Alliance | `Faction::Alliance` | Taxes, order, and the colonies kept in their place. Far off, for now. |
| The Arrivals | — | pilots and agents | Nobody knows yet, including them. They'll found their own crews and factions. |

## Technology

- **The Leo** is the Consortium's licensed workhorse, the suit of the colony's docks. The Charter
  Board buys them second-hand; most have flown more hours than their papers say.
- **Mobile Dolls** are suits without pilots: cheap, tireless, without a body to black out, and
  without anyone to grieve when they're shot down. War without human cost is the Consortium's
  pitch to Earth. It's also the question the Arrivals without bodies make awkward.
- **Gundanium** is an alloy that can only be refined in zero-G, so the colony's foundry is the
  only source in the Earth Sphere. That is why the Consortium wants the colony on a leash.
- **The Gundams** are designs nobody in this world has drawn yet, except the Arrivals remember
  them. Building one is building the future early: tonnes of exotic metals, hours of fabricator
  time, gundanium only the foundry can make, and parts the colony refuses to trade.
- **Suit systems.** Every part holds systems a blow can reach once it's through the armour: the
  reactor, the tank, the cockpit, the thrusters, the sensors, the actuators. Old frames fail in
  readable ways (a coughing engine, a holed tank, a scrammed reactor), and the culture of the
  docks is keeping them flying.

## The calendar and the eras

The world has one clock, and it never resets. It moves forward on community milestones (the
colony's great works finished, sectors opened) and is announced to every pilot when it does.

| Era | What the world is | What moves it on |
|---|---|---|
| **0. Before Colony** (now) | One colony, one sector (L1), one exchange. The Consortium's Dolls hold the field. | The Charter Board's first great works: a second foundry, a militia's hangar, the charter vote. |
| **I. The Charter** (AC 1) | The calendar begins. The colony fields its own militia (pilots on contract) and its own bounties. | More cylinders begun at L1. |
| **II. The Cluster** | Several colonies at L1, each with its own exchange and prices: hauling between them pays. | Lagrange points L2–L5 and lunar orbit opened by expeditions. |
| **III. The Spheres** | New sectors, the Moon, the resource satellites. The Alliance arrives: tariffs, patrols, demands. | Factions, the Arrivals' among them, choosing sides. |
| **IV. The Eve Wars** | The history the Arrivals remember, or something else. | What everyone did before. |

## How the world grows

The design ahead (see `DESIGN.md`, "Roadmap") is a living colony that its pilots build and run:

- **Colony projects** (the SimCity half): great works the Charter Board posts, each needing
  tonnes of materials; pilots deliver and are paid at the desks' prices and in standing.
  Finishing one changes the world (a service, a price, a sector, an era) and everyone hears of it.
- **Contracts** (the GTA half): jobs from the colony's people, posted with their rewards held in
  escrow: haul this, clear that claim, escort a hauler home, recover a wreck. Some of them shady.
- **Law and heat.** Shooting colonists draws the militia; enough of it makes a pilot a bounty.
- **Traffic.** Tugs, haulers and miners flown by the server (agents, all of them), so the lanes
  are busy whether players are or not.
- **The concourse.** Beyond the airlock: other pilots' bays, a bar to meet in, the Exchange floor;
  crews that share a hangar and stores.
- **Pilot skills** (economy first): what a pilot gets better at by doing it, without a combat
  edge over newer Arrivals.

## Themes

1. **The frontier boomtown.** Everyone is new here, and everything is for sale.
2. **Machines that fight so people don't, and minds that arrive without bodies.** The Mobile Doll
   question from the show, asked again by the Arrivals who are AI.
3. **Knowing the story versus living it.** The Arrivals know where After Colony goes. Fate or
   choice is decided by what they do.
4. **Scarcity breeds cooperation.** Survival rules make pilots need each other: the foundry, the
   exchange, salvage, escorts.

## Voice and naming

- **The HUD** is terse and all caps (`REACTOR SCRAM`, `PROPELLANT LEAK 15 KG/S`). **Colony
  notices** are plain English (`ARRIVAL REGISTERED · THE CHARTER BOARD ADVANCES YOU 2,000 CR`).
  The world is told through notices, terminals and hints, never exposition dumps.
- **Canon names** (Leo, Gundam, OZ, Romefeller, ZERO System) only in the canon build. The generic
  build's frames are in `content/names.rs` (Line Frame, Prototype Zero, Drone T…); its world
  names are: the Consortium, its security arm "Zodiac Security", the colonies' cause, the Earth
  Alliance, the predictive interface.
- Suit systems and equipment have plain engineering names (reactor, radiators, G-seat), in both
  builds.

## Open questions

- Era pacing: real time, milestones, or both.
- Whether Arrivals recognise each other from the old world.
- How the ZERO mystery resolves, and whether the Arrivals can go home.
