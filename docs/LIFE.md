# Living in the First Colony: jobs, meals and the great works

> GTA's city, the Sims' needs and World of Warcraft's server-wide efforts, inside one O'Neill
> cylinder. You can live in the colony and cook for pilots, and it matters to the pilots who fight.

The owner's ask (October 2026): wrap GTA, the Sims and WoW into the game we're making. Make living in
the colony real enough that someone can spend their evenings cooking for pilots, or delivering their
food, and be glad they did. Give the body a clock (three meals a day). Make weapons inside the colony
very hard to come by, so it can get crazy when one turns up. Let the whole server work together
towards things (space quests) that change everyone's world. And fill every job that people don't, so
the colony looks lived in either way.

`STORY.md` is the world this lives in, `DESIGN.md` how the game works, `PEERS.md` what the nearest
games teach and `ROADMAP.md` how its items are built. This document is the design for living in the
colony; its phases (below) are built the way `ROADMAP.md` builds its items.

## Status

| Phase | State |
|---|---|
| L0: the framework: seats, the body, meals (`bc_econ::{seats, body, food}`, `bc_sim::content::body`) | built |
| L1 onwards | planned |

## What it has to be

1. **Someone who never fights can live here, and it matters.** The cook's meal is what lets the pilot
   bear another half g. The courier's delivery is how it reaches the bay before launch. The
   dockmaster's slot is how the pilot gets home.
2. **Every job is worked.** A job is a seat, and a seat is worked by an Arrival or, when none sits in
   it, by the colony's staff. The world is never empty, and players take over from the NPCs, never
   the other way round.
3. **The body matters, but it's never a chore.** Being hungry is a reason to visit someone, not a
   timer to fear. Sleep (being logged out) never starves anyone.
4. **Weapons inside are rare enough to be news.** A shot fired in a pressure hull stops a strip.
5. **The server builds the colony together, and sees it built.** The building site at the far cap is
   the raid.
6. **Connect before deepening** (`PEERS.md`). Each system here sends people along the chain (the city,
   the bay, space, home). A meal crosses all of it: grown in Gardens, cooked in Canal, carried up the
   cap lift, eaten in the bay, felt in space.

## Who did it before

From the games' well-known histories (not yet sourced as `PEERS.md` sources its claims):

- **Star Wars Galaxies (2003, before its 2005 overhauls)** is the nearest: chefs, doctors, dancers and
  crafters were professions players chose, and the fighters needed them. Battle fatigue was cured
  only by an entertainer in a cantina; the best food buffs were player chefs'. It's still missed. It
  broke when a server ran short of doctors or dancers: a web of who needs whom works only while every
  seat is filled. **We fill the empty seats** (1, below).
- **World of Warcraft:** cooking buffs and feasts; rested experience earned while logged out in an
  inn; the Gates of Ahn'Qiraj war effort (2006), where each server gathered materials together for
  weeks to open the gates.
- **Final Fantasy XIV's Ishgardian Restoration (2020):** crafters and gatherers rebuilt a city in
  stages the whole server could see.
- **Rust:** a player's body stays in the world when they log out (a sleeper). Ours already do in
  space (`DESIGN.md`, "Sorties": "Away").
- **GTA Online:** a 48-minute day, as ours is (`bc_sim::colony::time::DAY_TICKS`).
- **Papers, Please:** a border checkpoint as a game in itself.
- **EVE Online:** an economy players run, with players who play it as one.

## 1. Seats: every job is worked

*Built as a framework: `bc_econ::seats`.*

A **seat** is one job at one post: one of The Arrival's two cooks, Hub Gate's customs officer, the
dock's approach controller. (Not The Arrival's benches, `city::arrival_seats`: a seat here is a job.)
It's worked by one of two:

- **An Arrival who sits down in it.** An Arrival is a pilot with a body or an agent without one, by the
  same rules (`STORY.md`, "The Arrivals"), so an LLM agent can take a seat as a person can.
- **The colony's staff**, whenever no Arrival holds it. The staff are the server's own hands, not
  Arrivals: they work at a fair baseline (`STAFF_QUALITY`) and never better.

The rules:

- **One seat at a time.** An Arrival in a seat holds it while they work. Away from it for
  `SHIFT_LAPSE_S` (10 minutes), the shift lapses and the staff take it back.
- **Pay.** What a seat sells, its worker earns `WAGE_PCT` (80%) of; the rest is the colony's, the
  seat's rent. Under the staff, all of it is the colony's: a sink. Every credit is accounted for
  (`seats::pay`; the tests check it).
- **Quality.** The staff work at `STAFF_QUALITY` (50). An Arrival's work runs from `NOVICE_QUALITY`
  (40) to their **ceiling**, by how well they do the job's own task (its minigame's score, 0 to 1).
  The ceiling rises with practice (`Skills`, by job), from 60 towards 100: half the way there after
  `HALF_PRACTICE` units of work. A new cook can be worse than the staff; a practised one is much
  better. That's why anyone takes a seat.
- **Skills by doing** (`DESIGN.md`'s "Pilot skills, economy first"): what an Arrival gets better at by
  working, never a combat edge.

The colony's seats (`Seats::colony`):

| Job | Post | The work (its minigame) | What it feeds |
|---|---|---|---|
| Cook | The Arrival (2) | a kitchen line: tickets in, dishes out, timing and order | meals; the pilots' fed condition |
| Barkeep | The Arrival (1) | pouring and talk; the room's mood | The Arrival as the place to meet |
| Courier | the streets (6) | routing across 32 km of real traffic, trams and lifts | meals and parcels to the bays |
| Customs | Hub Gate (3, one a strip) | the cap lift's scanner: who and what comes down | the colony's law (5, below) |
| Dockmaster | the dock (1) | approach slots for suits coming home | quicker, safer docking |
| Foundry | the zero-G foundry (2) | keeping the melt in its band | gundanium's yield |
| Mechanic | the bays (3) | overhauls done for hire | suits kept flying (`DESIGN.md`, "Wear from use") |

## 2. The body: meals on the wall clock

*Built as a framework: `bc_econ::body`, `bc_econ::food`, and the flight numbers in
`bc_sim::content::body`.*

The pilot's body is fed or it isn't, on the wall clock, whatever the colony's sky is doing (3, below).

- **A meal lasts about 5 hours awake** (`food::Dish`: a ration bar 1½ h, a noodle bowl 5 h, a fish
  supper 6 h). Three meals cover a waking day, as the owner asked.
- **Awake is logged in.** Food burns second for second while the pilot is in the world.
- **Asleep is logged out:** the pilot is in their bunk. Food burns at a quarter of the rate
  (`SLEEP_DIV`), and sleep never takes a pilot past hungry. An Arrival wakes wanting breakfast,
  never starving, so the natural rhythm is eat, then launch.
- **A stomach holds 8 hours** (`FULL_S`). A dish more than half of which wouldn't fit is refused
  ("you couldn't eat another bite").
- **Good food makes you well fed.** A meal at `GOOD_QUALITY` (70) or better makes the pilot well fed
  for as long as its quality's share of the dish's hours. The staff cook at 50, so **only an Arrival
  who cooks well can make a pilot well fed**. That's the cook's trade.

What it does (`bc_sim::content::body::Fed`, the G a pilot bears on top of their cockpit's, under the
real flight rules; anime doubles it, as it does the stim's):

| Condition | When | G |
|---|---|---|
| Well fed | a good meal's glow still on them | +0.5 |
| Fed | more than an hour's food left | 0 |
| Peckish | an hour's food or less | 0 |
| Hungry | nothing left | −0.5 |
| Starving | awake on an empty stomach for 3 hours (`STARVING_AFTER_S`) | −1.0 |

For scale: a pilot bears 6 g headward (twice that pressed into the seat, half diving), a damaged
cockpit 5, and a stim gives 1 g more for a minute. Starving costs what a damaged cockpit does;
nothing about hunger kills.

**How it reaches flight (L2).** As the stim does: anything that changes flight goes through
`bc_sim::tuning`, from state the own snapshot carries (`CLAUDE.md`). The condition is 3 bits
(`Fed::BITS`) in the own snapshot, `Fed::Fed` being 0 so a zeroed field changes nothing; the server
sets it at launch and as it changes; `own_tuning` adds `fed_g` where it adds `stim_g`. A protocol
bump.

## 3. The clock: a real day, or the colony's 48 minutes

*A decision for the owner. The body (2) runs on the wall clock either way, so nothing waits on it.*

The colony's day is 48 minutes (`DAY_TICKS` 86,400 at 30 Hz), the same as GTA Online's, for the same
reason: on a real day each player sees one time of it. Somebody who always plays at 9 pm sees only
night.

- **Keep 48 minutes.** Every session sees a dawn and a dusk. Meals are on the wall clock anyway.
- **A real day, staggered by strip** (the recommendation, if the owner wants a real day). Each of the
  three strips has its own window and its own mirror, so each can keep its own hours. Charter keeps
  Greenwich time, Canal runs 8 hours ahead, Gardens 16. Whenever you log in, one strip is at
  breakfast, one at the lunch rush and one asleep, and Canal's night shift is when its freight moves.
  It looks feasible: every line of the city's traffic and walkers is laid so a day holds whole laps
  of it, and a real day is exactly 30 of today's, so the laps still fit. `time::day` takes the strip
  (a phase each), `key_light` already does, the mirrors open each on its own, and the goldens move
  (`CITY_GOLDEN`, `TRAFFIC_GOLDEN`, `WALKERS_GOLDEN`).

## 4. The first slice: a meal from Gardens to a cockpit

The phases below, L1 to L5, build one chain end to end, the thinnest that proves seats, the body and
the chain together:

1. A pilot eats at The Arrival, from the staff's kitchen (L1).
2. Being fed changes how they fly (L2).
3. An Arrival takes a cook's seat and cooks better than the staff (L3).
4. A courier carries a meal up the cap lift to a pilot's bay (L4).
5. Gardens grows what the kitchen cooks with, and the canal carries it (L5).

## 5. Contraband: guns inside

*A decision for the owner: `ROADMAP.md`'s "Where PvP lives". Nothing here is built until it's made.*

The colony's law stands (no weapons fired inside; the Blast Hall the one exception: `TRAINING.md`).
The owner's ask is that weapons inside be extremely hard to get, but that it can get crazy:

- **Smuggling is the game.** Sidearms are made in the bays' fabricators, and the cap lift has
  customs (a seat, 1). Getting one into the city is the play: a hidden compartment, the freight
  canal, or a bribe to the officer, who may be an Arrival sitting there. Papers, Please, with real
  smugglers.
- **One shot is news.** A shot fired inside a pressure hull is the biggest thing all week: the
  strip's trams stop, the militia comes, the shooter is the sector's most wanted (`STORY.md`'s "law
  and heat").
- **Those who don't opt in stay safe.** The cook in their kitchen must never be shot by a troll. The
  recommendation: Hub Gate, Charter Square, the homes, the kitchens and the bars stay exactly as safe
  as now, with no way to harm anyone at all. Contraband works only at the edges: the building site,
  the Yards, Last Lock. `ROADMAP.md`'s option 2, inside the hull.

## 6. The colony is the raid: space quests

The Charter Board's great works (`DESIGN.md`, "The Charter Board") are already WoW's war effort: a
second foundry, the militia's hangar, the charter vote. What they lack is being seen.

- **The building site grows.** `bc_sim::colony::city::Stage` is `Stage(0)` everywhere today. Give each
  district its own stage, carried by the Welcome as world state, and let finished works move the
  building site down the strip towards the far cap. The city is a closed form of the tick, where
  you ask and the stage, so a district built out is one number every client draws from. You'd see it
  from space. (`CITY_VERSION` moves with the protocol.)
- **Works unlock things:** tech (gundanium at half the fee, the militia's patrols, a test stand for a
  Tallgeese), seats (a hospital adds medics; a market hall in Canal adds cooks), eras (`STORY.md`).
- **Somebody pushes back.** The Consortium owns the colony's debt and doesn't want the charter
  signed. Its Doll offensives (`ROADMAP.md`, P1) strike the building site while it's being built,
  and pilots who would rather take the Consortium's money can be paid to slow the work. A server-wide
  tug of war, with players on both sides.

## 7. Minds by level of detail

The city already has 200,000 to 250,000 people a strip at noon (`bc_sim::colony::walkers`), a closed
form of the tick that costs nothing to store or send. They walk loops; they don't live lives. Keep
them as scenery, and promote one only when an Arrival takes an interest:

1. **The crowd:** closed form, free, the same on every screen (today).
2. **A census:** a walker's line and slot hash to a stable identity: a name, a home district, a job.
   Still a closed form (`bc_sim::colony`, deterministic, allocation-free).
3. **An extra:** looked at or followed, a cheap scripted agent walks them where they're going.
4. **A character:** spoken to, a language model plays them from their census entry.

The crowd stays free; minds cost only where someone is looking.

## 8. The question under it all

`STORY.md`'s oldest open question is whether an Arrival without a body is a person, a tool or a
citizen. The charter vote can decide it, for real, with mechanical consequences:

- **Citizens:** agents may hold a bay, post contracts, earn standing and vote.
- **Tools:** agents work seats for hire and own nothing; cheaper, and the colony's.

The players settle the game's central question, and the economy changes with the answer.

## Phases

Each is PR-sized and tested end to end, as `ROADMAP.md`'s items are.

- **L0: the framework** (built). `bc_econ::seats` (jobs, seats, skills, quality, pay),
  `bc_econ::body` (the clock, eating, the condition), `bc_econ::food` (dishes and meals) and
  `bc_sim::content::body` (the conditions and their G). Unit tests for each; nothing on the wire yet.
- **L1: eating at The Arrival.** The body on the pilot's record (`PilotRecord`), woken on entering
  and put to sleep on leaving (`session.rs`'s `enter` and `leave`), settled on the session's tick.
  `Request::Eat { dish }` at The Arrival's counter (the plaza's last pose within reach of its door,
  as `at_the_hatch` checks the gantry's), paid in credits to the staff (a sink); `Update::Body` with
  the condition and the food left. The on-foot HUD shows the condition; The Arrival's door opens its
  menu. Tests: `bc-server/tests/life.rs` (a pilot eats, logs out, wakes peckish), and a step in the
  `colony` e2e.
- **L2: being fed in flight.** The condition in the own snapshot, `fed_g` in `own_tuning` and the
  sector's tuning, the HUD's line in the cockpit. A protocol bump; the predictor's tests.
- **L3: the cook's seat.** `Seats` kept by the server (one per colony, in its data directory, as the
  Proving Ground's board is); `Request::{TakeSeat, LeaveSeat}`; the kitchen line as the cook's
  minigame, its score into `Skills::quality`; meals cooked by an Arrival sold at their quality, paid
  by `seats::pay`. `bc-bot` gets a cook so an agent can take the seat. Tests: a server test with a
  bot cook and a pilot eating its meal well fed.
- **L4: couriers.** Orders placed from the bay's terminal; a courier's seat takes them, rides the
  tram and the cap lift, and delivers to the bay. The first job that crosses the whole chain.
- **L5: Gardens' produce and the canal.** Ingredients as `Item`s on the Exchange; dishes cooked from
  them; the canal's freight carrying them.
- **L6: the building site grows** (6).
- **L7: customs and contraband** (5), once the owner decides.
- **L8: the census and minds by level of detail** (7).
- **L9: the citizenship vote** (8), with the charter.

## What we watch

- **Chores drive players away faster than danger.** Every need is a reason to visit someone. Sleep
  never starves anyone.
- **A seat worked by an Arrival must be clearly better than the staff**, or nobody takes it; but a
  colony worked only by its staff must never feel broken.
- **Griefing in safe spaces** (`PEERS.md`, Star Citizen). Every new verb is asked: can it be used on
  somebody who didn't opt in?
- **The hot path stays untouched.** All of this is `bc-econ` and the session, off the tick. Only the
  fed condition reaches the simulation, as 3 bits of the own snapshot and a term in `tuning`.

## Open questions

- The clock (3) and contraband (5): the owner's.
- Whether a guest (no wallet) has a body at all, or only for the visit, as their hangar is.
- Who sets the staff's prices: fixed (L1), or floating with what the colony holds, as its desks do.
- Whether the staff's seats should be worked by visible agents (people at the counter), or be only
  numbers until an Arrival sits down.
