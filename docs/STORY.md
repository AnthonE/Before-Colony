# Before Colony: the world

> History will count the Charter Years. They haven't started yet.

This is the world bible: who the players are, where they are, what's going on around them, and how
the world changes as they live in it. `DESIGN.md` says how the game works; this says why the world
is the way it is. Every name in it is the game's own (`bc_sim::content::names`; see "Voice and
naming" below).

## The pitch

The first space colony, a ring at the Earth–Moon L1 point, has just opened its docks.
The calendar that history will count in, the *Charter Years*, begins the day its charter is
signed, and nobody has signed it yet. This is the year before: **Before Colony**.

You woke up in a hangar bay in the colony's docking hub. You have the reflexes of a mech
pilot, a worn-out Argo in the gantry, an advance of 2,000 credits, and memories of another world: one
where this colony's history is a story you already know. The drones, the five Lodestars, the
Breakaway, the war. Here none of it has happened. Whether it has to is up to everyone who arrives.

## The Arrivals

- **Who.** People who wake in the docking hub with no records, trained as pilots, remembering
  another world. Not all of them have bodies: some arrive as minds that only ever speak through a
  suit's controls. The game's AI agents are those Arrivals, and the HUD tags them **DR**.
- **How the colony sees them.** The Charter Board registers every Arrival as a pilot, body or
  not, and gives them a bay, a second-hand Argo and an advance against their earnings. "An Arrival
  flies by the same rules as anyone" is colony law before it is a design pillar: agents see what
  their sensors see, bear the same G and pay the same fees. What an Arrival without a body *is*,
  a person, a tool, a citizen, is the colony's oldest open question, and the game's.
- **Coming back.** An Arrival whose suit is destroyed wakes again in the hub. Nobody knows why it
  works, or why it works only for them. The suit, its cargo and everything aboard stay lost: the
  world remembers what you lose even if you don't die of it. (This is the diegetic reason
  survival rules bring a pilot back through the airlock.)
- **The Orrery.** An interface built into one prototype torso that feeds its pilot the
  futures of a fight. Colonists who try it come out shaking. Arrivals find it familiar: it shows
  the future the way they remember the story. Why is the mystery under everything else.

## The First Colony

- **Its name.** The colonists call it **the First Colony**, and so does everything they sign; the
  Consortium's papers say L1-01.
- **The ring.** A wheel 2 km across and 600 m wide, turning once a minute for a full gravity at its
  floor, its interior still being finished. Its town lies on the inside of the rim and rises ahead of
  you whichever way you look, into the lattice of the ceiling 300 m overhead, where the light falls
  through. The game's sector L1 is the space around it: the debris of its construction (asteroids
  hauled in for metal), the docking hub on its axis, and the lanes between. (Until October 2026 the
  game drew a 32 km cylinder here; `RING.md` is why it's a ring, and the cylinder's districts below
  are names in waiting.)
- **The docking hub.** The drum on the ring's axis: the pilots' bays round its forward face at 0.4 g,
  each launching out of its own door, the dock with its ring of amber lights, and the **zero-G
  foundry**, the only place siderium can be made. Behind the bays, on the drum's deck, is **the
  Muster**, the Charter Board's yard for its pilots: Bay Row, where every bay's airlock opens, the
  briefing room, the armoury, the mess and the Proving Ground. Six spokes run out from the hub to the
  rim, each with a lift down to the town.
- **The Charter Board.** The colonists' provisional council. It advances each Arrival 2,000
  credits and an Argo (and another worn Argo to a pilot who has lost everything, though not twice in
  half an hour: `THE CHARTER BOARD ADVANCES YOU A WORN ARGO`), runs the colony's desks on the **Colony Exchange** (it buys ore, sells
  propellant cheap, and keeps machine shops turning out components), and pays a bounty for every
  drone a pilot brings down. The bounties are its quiet war: it can't fight the Consortium
  openly, but it can pay the people who do.
- **Inside.** The town runs round the rim in arcs between the spokes, each a quarter with a name of
  its own: **Charter** (the colony's offices, its money, its first streets), **Canal** (a working
  quarter along the canal that carries its freight round the ring: depots, quays, locks, yards),
  **Gardens** (orchards, terraces, homes), and **the building site**, the arc of the rim still open
  frames and cranes, where the colony is still being built. **Hub Gate** is the square where the
  first spoke's lift comes down. The louvres round the spine throw in the day: 32 minutes of light,
  then dusk, and the town's lamps.
- **Names in waiting.** The cylinder's districts, kept for the ring's streets and quarters as
  they're built: in Charter, Charter Square, Exchange Row, Tower Hill, Meridian, Lantern Street,
  Firsthomes, Central Park, Arrival Heights, Old Town, the University, Machine Row and Foundry Lane; in
  Canal, the Depots, Quayside, Canal Central, Lock Town, Waterside, the Basin, Millrace, Twin
  Bridges, Lower Canal, the Yards, Far Quays and Last Lock; in Gardens, Garden Gate, the Orchards, the
  Arboretum, the Colleges, Terraces, Green Meridian, Hillside, the Meadow, Vine Street, Greenworks,
  Fieldside and the Seed Halls. The tram lines keep their colours (Charter blue, Canal teal, Gardens
  green) for a loop round the rim.
- **Places an Arrival goes.** In the Muster, the **briefing room**, where the Charter Board's
  pilot desk pays out what the Board owes a pilot, posts contracts and patrols and pins up the
  Almagest's Most Wanted; the **armoury**, which fits weapons to the suit in a pilot's bay; the
  **mess**; and **the Proving Ground**, the Blast Hall where the colony's builders tested thrusters,
  where the Charter Board tries an Arrival in a suit before it trusts them with one. Down a spoke in
  the town, the **Exchange floor** (the Colony Exchange's hall: the same book the bays' terminals
  trade on, and the only place an order waits on it), the **Charter Board**'s hall, where its
  notices go up and the charter will be signed, and **The Arrival**, a bar where pilots meet. The
  colony's law holds inside: no weapons fired within its walls, but in the Blast Hall, where a suit
  fires training rounds at holograms and nothing it fires leaves the hall.
- **The economy.** A boomtown. Ore comes in from the field, parts and suits go out from the bays,
  and prices float with what the colony holds. Frames are old and patched, so systems fail, and
  keeping a suit flying is a trade of its own (overhauls, machined components, equipment).

## Powers at the dawn

| In the world | In the simulation | What they want |
|---|---|---|
| The Consortium | — | They financed the colony and own its debt, the asteroid claims and the Argo lines. |
| Its security arm, the Almagest | `Faction::Almagest`, the drones alone (a pilot's claim to it flies for the Colonies) | To hold the claims: its Corvus and Cetus patrols shoot claim-jumpers, which is every Arrival who mines; its best aces fly its prototype pair. |
| The Colonies | `Faction::Colonies` (every human pilot, as the server decides, and agents that ask for it) | A charter of their own, a foundry of their own, and nobody's debt. |
| The Unified Earth Alignment | `Faction::Alliance` (agents that ask for it) | Taxes, order, SI kept aligned, and the colonies kept in their place. Far off, for now. |
| The Arrivals | pilots and agents | Nobody knows yet, including them. They'll found their own crews and factions. |

## Technology

- **The Argo** is the Consortium's licensed workhorse, the suit of the colony's docks. The Charter
  Board buys them second-hand; most have flown more hours than their papers say.
- **The prototype pair.** The Almagest has heard what the Arrivals remember, and built its own
  answer before anyone drew a Lodestar: two suits on one frame, a spear and a shield. The
  **Sagittarius** carries a beam cannon cabled to the generator on its back, with no shot limit
  but the reactor's. The **Corona** sends out ten wards that hold a field no hostile beam
  crosses, over itself and whoever flies inside. Neither is much alone; together they are what
  the Almagest's last two aces fly. The Cetus is what its engineers made of them for the line,
  smaller and cheaper.
- **Drones** are suits without pilots: cheap, tireless, without a body to black out, and
  without anyone to grieve when they're shot down. War without human cost is the Consortium's
  pitch to Earth. It's also the question the Arrivals without bodies make awkward.
- **Siderium** is an alloy that can only be refined in zero-G, so the colony's foundry is the
  only source in Earth-Moon space. That is why the Consortium wants the colony on a leash.
- **The Lodestars** are designs nobody in this world has drawn yet, except the Arrivals remember
  them. Building one is building the future early: tonnes of exotic metals, hours of fabricator
  time, siderium only the foundry can make, and parts the colony refuses to trade.
- **Suit systems.** Every part holds systems a blow can reach once it's through the armour: the
  reactor, the tank, the cockpit, the thrusters, the sensors, the actuators. Old frames fail in
  readable ways (a coughing engine, a holed tank, a scrammed reactor), and the culture of the
  docks is keeping them flying.
- **SI.** Safe SI is a common part of the aligned world. The Unified Earth Alignment banned the
  unsafe kind at the same time humans started to build in space.

## The calendar and the eras

The world has one clock, and it never resets. It moves forward on community milestones (the
colony's great works finished, sectors opened) and is announced to every pilot when it does.

| Era | What the world is | What moves it on |
|---|---|---|
| **0. Before Colony** (now) | One colony, one sector (L1), one exchange. The Consortium's drones hold the field. | The Charter Board's first great works: a second foundry, a militia's hangar, the charter vote. |
| **I. The Charter** (CY 1) | The calendar begins. The colony fields its own militia (pilots on contract) and its own bounties. | More rings begun at L1. |
| **II. The Cluster** | Several colonies at L1, each with its own exchange and prices: hauling between them pays. | Lagrange points L2–L5 and lunar orbit opened by expeditions. |
| **III. The Spheres** | New sectors, the Moon, the resource satellites. The Alignment arrives: tariffs, patrols, demands. | Factions, the Arrivals' among them, choosing sides. |
| **IV. The Equinox Wars** | The history the Arrivals remember, or something else. | What everyone did before. |

## How the world grows

The design ahead (see `DESIGN.md`, "Roadmap after Milestone 4", and `ROADMAP.md`) is a living
colony that its pilots build and run:

- **Colony projects** (the SimCity half): great works the Charter Board posts, each needing
  tonnes of materials; pilots deliver and are paid at the desks' prices and in standing.
  Finishing one changes the world (a service, a price, a sector, an era) and everyone hears of it.
- **Contracts** (the GTA half): jobs from the colony's people, posted with their rewards held in
  escrow: haul this, clear that claim, escort a hauler home, recover a wreck. Some of them shady.
- **Law and heat.** Shooting colonists draws the militia; enough of it makes a pilot a bounty.
  Inside the colony that's the Watch, its Sirius hounds and its suits (`OPEN_CITY.md`).
- **Traffic.** Tugs, haulers and miners flown by the server (agents, all of them), so the lanes
  are busy whether players are or not.
- **The concourse.** Beyond the airlock: other pilots' bays, a bar to meet in, the Exchange floor;
  crews that share a hangar and stores.
- **Pilot skills** (economy first): what a pilot gets better at by doing it, without a combat
  edge over newer Arrivals.

## Themes

1. **The frontier boomtown.** Everyone is new here, and everything is for sale.
2. **Machines that fight so people don't, and minds that arrive without bodies.** The drone
   question the Arrivals remember from their story, asked again by the Arrivals who are AI.
3. **Knowing the story versus living it.** The Arrivals know where the Charter Years go. Fate or
   choice is decided by what they do.
4. **Scarcity breeds cooperation.** Survival rules make pilots need each other: the foundry, the
   exchange, salvage, escorts.

## Voice and naming

- **The HUD** is terse and all caps (`REACTOR SCRAM`, `PROPELLANT LEAK 15 KG/S`). **Colony
  notices** are plain English (`ARRIVAL REGISTERED · THE CHARTER BOARD ADVANCES YOU 2,000 CR`).
  The world is told through notices, terminals and hints, never exposition dumps.
- **The names are the game's own, and they come from the sky.** The Consortium's security arm is
  **the Almagest**, after Ptolemy's star catalogue, and it names every line it fields after one of
  the catalogue's constellations: the **Argo** (AL-04S, the docks' workhorse), and its drones the
  **Corvus** (AL-11D, transforms) and the **Cetus** (AL-09D, heavy); its prototype pair, the
  **Sagittarius** (AL-13X1, the archer, whose bow is its cannon) and the **Corona** (AL-13X2,
  the Southern Crown that lies at the archer's feet in the sky, as its wards ring the pair);
  its aces fly under more of
  them (Lyra, Sagitta, Delphinus, Cygnus, Auriga, Perseus, Centaurus, Hercules, Orion). The colony
  answers with stars: the five **Lodestars**, the designs the Arrivals remember, are the
  **Altair** (PX-00, which folds into the **Aquila**), the **Antares** (PX-01), the **Algol**
  (PX-02), the **Canopus** (PX-03) and the **Thuban** (PX-04). Their armour is **siderium**, which
  only the colony's zero-G foundry makes; the combat AI in one Lodestar's torso is **the
  Orrery**; the resource satellite at the dock is **Fornax**. The colony's rocket car, from its
  motor pools, is the **Markab**: α Pegasi, the saddle of the winged horse (its test driver in
  the showcase is Bellerophon, who rode it). The colony's Watch flies drones it calls the
  **Sirius** hounds, after the Dog Star, and suits named for the royal stars the Persians called the
  Watchers: the **Aldebaran**, the **Regulus** and the **Fomalhaut** (the fourth Watcher, Antares,
  is a Lodestar's). The interface every Arrival woke with, behind the ear, is their **Astrolabe**:
  the reading instrument to the Orrery's model of the heavens (`OPEN_CITY.md`, "Intrusion").
- **Each name fits what the thing does:** Algol, the star that dims, carries the
  **Eclipse** (a jammer); Thuban, the old pole star, strikes with the **Talon**; Antares opens
  up in a **Full Salvo**; Canopus closes the **Pincer Strike**; the Altair is the eagle's star.
- **Never another franchise's.** No name from another studio's show or game (its suits, people,
  places, weapons or calendar) goes into the game or its page, and no new design is drawn after
  one: the game is sold on a store. These docs name other games only as peers or influences,
  never as the source of a name of ours, and the licensed anime-mech games are cited by what they
  are (`PEERS.md`, `CONTROLS.md`). The names the frames, items and sides had before 2026-10-14
  are still read where saved data has them (`FrameId::OLD_SLUGS`, `bc_econ::item`'s old slugs, a
  parked suit's old side, the settings' old key) and are never written or shown.
- Suit systems, weapons and equipment have plain engineering names (reactor, radiators, G-seat,
  beam rifle, heat blades).

## Under everything: the island (spoilers, never said in the game)

Before Colony shares one timeline with **Gates**, the survival game (`AnthonE/Gates`: its
`WORLD.md` §11 is this section's twin, and the two change together). Gates is the preamble: it
stays a mystery, and goes under the island only late, once its players have progressed far
enough. Before Colony starts in the middle, hides the truth in plain sight, and one day gives the
hard confirm. The whole story is one degree from Halo's, except that what you play is more like
its Forerunners.

- **The machine.** An ancient civilization built it into the geography of an island on Earth. It
  bends space and time, and it can "resurrect" a new type of people.
- **The SI.** The Unified Earth Alignment banned it at the same time humans started to build in
  space, and the island became a research island for it. It was trained on specific ancient texts
  to key the sacred math needed to tap hidden channels outside normal space and time.
- **Under the island** is a factory like NERV's, probably with bio-mecha chained up and harvested
  for something: a long-term bio-horror, almost like Halo's Flood. Gates opens it eventually,
  behind hidden metal doors underground.
- **The pilots.** The island is a battle royale where future Lodestar pilots are raised: imagine
  a boy soldier who has never known anything but the fight growing up there. That is half of what
  makes one; the other half is innate human skill, with the right SI helpers.
- **SI tech.** At its top end (in vessels, and other combinations of integration) it is nearly
  physics-breaking, close to magic. Even talking with an SI can be enough to cause trouble.
- **The gate system** is how the SI reaches space in the end, and in space it infects the
  autonomous mecha: the drones.
- **The hard confirm.** Eventually this game sets up a gate to get from the Moon to Earth.

What that makes of this game:

- **The Arrivals** are humans, raised on the island the way the Lodestar pilots were raised to
  fight. They remember it like a dream, and keep little of it but the combat experience. Every
  Arrival woke up in the docking hub: the island is the dream they woke from.
- **Coming back** is the machine's reach. An Arrival wakes in the hub for the same reason a player
  in Gates wakes on a beach, and only the machine's people can be brought back.
- **The Orrery** is SI tech, and not the safe kind: the futures it feeds a pilot come from
  outside normal space and time. Colonists, who live with safe SI, come out shaking; Arrivals find
  it familiar. Strain and seizure are what it does to a mind it's too much for. Nothing else in
  this era comes as near the top end.
- **The drones** are what the SI is coming for. Era IV's "or something else" is the day it
  reaches them.
- **The gate from the Moon to Earth** is a road both ways: whoever sets it up for the pilots
  opens the SI's road to space too (Gates' extraction windows have the same irony).

### Hidden in plain sight

- **Never said.** No notice, terminal, hint, chart line or character names the island, the
  machine, the banned SI, the gate system or what is under the island before the hard confirm,
  and nothing explains them. Safe SI is everyday and can be shown.
- **Every clue is ordinary on its own** (a name, a number, a sound, the way a drone flies) and reads
  differently only to someone who knows. The clues in both games agree.
- **Already in the game:** Arrivals without bodies wear **DR**, the drones' own letters; an
  Orrery seizure flies the suit with a drone's reflexes (`DESIGN.md`, "drones and agents");
  `SUIT LOST` says the colony's rescue boat is on its way, and the boat always finds the pilot
  alive; a colony full of gates (Hub Gate, the inner gate), and nobody thinks about the word. In
  Gates, the town is **THE GATE**, and its death screen says
  `THE GATE would not take you - you woke on a beach`.
- **Worth planting:** one shape across both games, the ring (the colony itself, a wheel; the dock's
  ring of amber lights; the course's lit rings; Gates' Severed Gate is a shattered gold ring); one sound (the Orrery's drone
  sharing its notes with the hum of THE GATE's yard); the sacred math's glyphs, carved on Gates'
  ruins and flickering across the Orrery's display in a seizure; a drone that, rarely, flies for
  a moment like an Orrery seizure.
- **Inspiration, never names:** NERV is Evangelion's; the Flood and the Forerunners are Halo's.

## Open questions

- Era pacing: real time, milestones, or both.
- Whether Arrivals recognise each other, from the old world or from the island's dream.
- Whether the other world the Arrivals remember, where the Charter Years are a story, is part of that
  dream (the machine reaches outside normal space and time). Whether they can go home, and which
  home.
- How an Arrival gets from the island to the docking hub.
- What an Arrival without a body is, when the Arrivals are humans. They wear the drones' letters
  already.
- What the bio-mecha under the island are harvested for.
- What being "more like the Forerunners" makes of the pilots: heirs of the ancients, or the
  ancients themselves, brought back by their own machine.
