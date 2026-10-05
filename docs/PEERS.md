# Peers: what the nearest games teach us

A survey (October 2026) of the games that each have a piece of Before Colony's fantasy: live in a
working colony city, walk to the hangar, and launch a mobile suit into space. It says what each has
and lacks, what we take from it and where that lands in our code, and ends with a list, most
important first. `docs/CONTROLS.md` covers what players of these and other games expect of the
controls; this covers the rest.

Claims marked *unverified* come from secondary sources only. The sources are listed at the end.

## Is anything like it?

No. Nobody ships the whole fantasy. The nearest games each have one piece of it:

| Game | What it has | What it lacks | What we take |
|---|---|---|---|
| Mobile Suit Gundam Battle Operation 2 (2018–) | Gundam suits with manual aim, in space and on Earth; a map inside a derelict colony [P1] | A world: it's 6v6 matches | Cost brackets; close quarters as the way into free aim; new suits as the cadence |
| VRChat's Space Colony "Island-4" (2021) | An O'Neill cylinder 8 km across and 32 km long, curve gravity, crowds [P2] | Mechs, combat, anything to do | The scale is the draw; hanging out needs a way to talk |
| Star Citizen (alpha) | Walk a city, ride a lift to your hangar, fly to orbit [P3] | Mechs; a 1.0, after $1 billion | Stage the transitions; shared spaces are griefed without weapons; jobs from the city |
| The Gundam Metaverse (2022–) | Promised: virtual space colonies for the fandom [P4] | Delivered: a shop and Gunpla scanning | Every place needs a verb; people want their own build |
| Gundam Rogue Orbit (March 2027) | Customised Gundams, colour schemes, hordes and bosses [P5] | A world; colonies | PvE that escalates; a colour scheme is expected |
| X-Wing (1993) and TIE Fighter (1994) | A hub of doors to walk between, training before the war, a cockpit to read, music that follows the fight [P7][P8] | A world: the hub is a picture | Training as a place (`TRAINING.md`); briefings, a debrief, a film room |

Before Colony already has the chain in outline: the city, the cap lift up to the bay (`DESIGN.md`,
"The First Colony, inside"), the catapult into space and the dock home ("Sorties"). None of them
ships that. What we lack is what makes a chain worth walking: **reasons to cross it again and
again, and people to cross it with**. Most of what follows is one of those two.

## Battle Operation 2

The nearest Gundam game: manual aim with no lock-on button (`CONTROLS.md`, "Where we break convention" 4), suits
from every series, and still adding them (Gundam 00's Double O Riser and Susanoo in the summer of
2026, *unverified*).

- **Cost brackets.** Every suit has a cost and a room sets the range it allows, so a low-cost fight
  has no flagships in it; a costlier suit waits longer to sortie again [P1]. We gate by price
  instead: a new Leo is about 41,000 credits of parts, a Gundam 175,000–223,000 (`DESIGN.md`,
  "Survival"). A price keeps Gundams rare, but doesn't keep a fight fair once one turns up. A suit's
  cost can be worked out from what it is (its stat sheet, `bc_sim::tuning`, or what its parts fetch
  on the Exchange) and used wherever we choose who fights whom: contract tiers, and the Dolls sent
  after a pilot.
- **Close quarters teach free aim.** Derelict Colony (2020) is a wrecked colony: collapsed
  buildings, the old roads on the ceiling. Its developers built it for closer, more ground-like
  fighting with less room to climb, and said why: it suits players still learning to hit with
  bazookas and beam rifles in space [P1]. That is our "unhittable" risk (`CONTROLS.md`, "No lead
  pip without ZERO"). We have close quarters already, MO-II's Aft Well and Hermit's craters, where
  suits fight on their feet (`DESIGN.md`, "Surfaces"). The colony's inside stays weapons safe by
  law (`SUITS_INSIDE.md`) but for the Blast Hall's training rounds (`TRAINING.md`), so a hot
  interior would have to be somewhere else: the axis port's scaffolds, or the building site's
  frames.
- **Infantry beside the suits.** Pilots fight on foot among the mobile suits, planting charges in
  bases (*unverified*). Ours walk only in the bay and the city; on foot in the sector is on the
  roadmap (`DESIGN.md`, "Surfaces, next").
- **New suits are the cadence.** A match game lives on its roster. Our frames are code, not art
  (`bc-model`, all procedural), so a frame costs its row in `bc-sim/src/content/frames.rs`, its
  model and its kit; Tallgeese and Epyon are next on the roadmap.

What it can't teach us is a world: its suits live nowhere between matches.

## Island-4

A fan-made VRChat world: an O'Neill cylinder at O'Neill's own size, 8 km across and 32 km long,
with gravity that curves with the floor, so you can run all the way round and come back to where
you started. It was one of VRChat's most popular worlds, on its home screen [P2]. There are no
mechs and nothing to do.

- **The scale is the attraction.** People came to stand in it. Ours is 6.4 km across and 32 km
  long, with three cities in it (`STORY.md`, "The First Colony"), and the cap lift's ride down the
  end cap with the whole colony in view is the moment players will remember (`Seq::LiftDown`,
  `bc-client/src/onfoot.rs`). Keep it skippable, but give every new pilot their first ride (P0 3).
- **Hanging out needs a way to talk.** A VRChat world is a room full of voices. Ours is silent:
  there's no chat, emote, wave or seat anywhere, and The Arrival, our bar, says only `QUIET FOR
  NOW` (`onfoot.rs`). Pilots pass each other in flight suits of their own colours and can do
  nothing together.
- **Physics as a toy.** Its players run the circle because they can. Our gravity falls with height
  (`bc_sim::colony::frame::gravity`: 1 g on the ground, less up a tower), and nobody is given a
  reason to feel it. A place near the axis, where a jump carries a long way, would be one.

## Star Citizen

The game that does the fantasy's other half: walk through a city, ride a lift to your hangar, fly
out to orbit. It passed $1 billion crowdfunded in May 2026, after 14 years and with no 1.0 [P3]. It
flies ships, not suits.

- **Don't buy seamlessness; stage it.** One seamless world is what has cost it most. We hide each
  change of place in a sequence the fiction already has: the cap lift's ride, the bay venting, the
  catapult down the tunnel, the glide in from the dock (`DESIGN.md`, "Sorties"). The city is a
  closed form of where you stand (`bc_sim::colony::city`): nothing of it is stored or streamed, so
  nothing of it can fail to arrive. Keep doing this.
- **Time to fun.** Its players have waited minutes for a ship to be "delivered", and its instanced
  hangars and elevators have lost ships and dropped people through floors [P3]. Our boarding,
  venting and catapult take about 10 s, and Space skips them (`onfoot.rs`). But a survival pilot
  then flies about 15 km from the launch gate to the Dolls, the fifth of their objectives. Measure
  the time from the title screen to the first thing that happens, and keep it short.
- **Weapons-free zones are griefed without weapons.** Its armistice zones have been abused by
  holding cargo elevators, ramming landing pads and, until it was patched, "healing" players into a
  stupor and stripping them [P3]. Our shared spaces already refuse that: other pilots are ghosts to
  you, vehicles collide with neither each other nor people, and a vehicle is its driver's alone
  (`COLONY.md`, phase 5). Suits inside must keep to it (`SUITS_INSIDE.md`: "nothing touches a
  suit"), and its open question about thrusters near people should be answered the same way.
- **Jobs from the city.** Its missions are taken in a city and flown in space, which is what brings
  players back to the city. Ours would come from the Charter Board, which is one fixed line today
  (P1 6).

## The Gundam Metaverse

Bandai Namco announced it in 2022: Gundam's fandom in virtual space colonies, one each for games,
anime, music and Gunpla, joined by a hub. It opened in 2023 as the "Gunpla Colony" test: a shop
for real Gunpla, and your own Gunpla scanned with a phone to see in 3D [P4].

- **Every place needs a verb.** A colony that's a shop isn't a place to be. Ours has three places
  and one of them works: the Exchange floor, whose terminal is the bays' Exchange. The Charter
  Board's hall says `ARRIVALS REGISTER AT THE DESK · NOTICES BY THE DOOR` and nothing more, The
  Arrival is quiet, and the building site is `Stage(0)` everywhere it's asked
  (`bc_sim::colony::city::Stage`), so nothing a pilot does can build it out.
- **People want their own build.** Gunpla is a kit *you* built and painted, and scanning it in was
  the Metaverse's pitch. Every Colonies Leo in our sector is the same blue, white and red
  (`livery`, `bc-client/src/suits_vis.rs`), and nothing a pilot owns says it's theirs but the
  callsign over it.

## Gundam Rogue Orbit

Bandai Namco's next Gundam game, out March 5, 2027: a Gundam customised with weapons, frames,
thrusters and a colour scheme of your own, against hordes of enemy units and towering bosses [P5].

- **PvE that escalates.** Ours is a patrol: 24 Mobile Dolls in squads of four, a squad topped up
  every 4 s however many pilots are out (`target_dolls`, `bc_sim::config`). There are no
  offensives, no convoys and nothing big. A fight that builds is what brings in the player who
  doesn't want to fight people.
- **A colour scheme is expected.** Rogue Orbit paints the suit your way; so will every Gundam game
  players hold us up against.

## X-Wing and TIE Fighter

LucasArts' space combat sims, made by Totally Games (Lawrence Holland and Edward Kilham) for
MS-DOS in 1993 and 1994. The owner brought them in for their feel. They have no world and no city.
What they have is what a new pilot needs: somewhere to learn before it counts, and a cockpit that
tells you what's going on.

- **The hub is a place with doors.**
  - X-Wing's concourse is the spaceport of the Mon Calamari cruiser *Independence* [P7]:
    - the left hangar door leads to the Proving Ground, the middle one to Historical Combat;
    - the desk on the right is the Tour of Duty;
    - small doors lead to the Tech Room and the Film Room, and Registration is bottom right;
    - pointing at anything names it.
  - TIE Fighter's is a two-level atrium, with stormtroopers at its doors [P8].
  - Ours is real: Hub Gate's square, and its places behind their doors (`COLONY.md`). Their doors
    map onto ours:

    | X-Wing's door | Ours |
    |---|---|
    | Proving Ground | the Blast Hall (`TRAINING.md`) |
    | Tour of Duty | the Charter Board's contracts |
    | Tech Room | the maintenance console's stat sheet |
    | Registration | signing in with a wallet |
    | Film Room | nothing yet (below) |
- **Training before it counts, against a clock.**
  - **X-Wing's Pilot Proving Ground ("the Maze")** [P7][P9]:
    - gates on platforms floating in space, flown in order;
    - targets on the platforms, and on later levels turrets that fire back;
    - all of it holographic and "safe" to hit, but a hit stops you and costs time;
    - eight levels per craft, each with less time, then a flight badge.
  - **TIE Fighter's Training Simulator** [P8][P10]:
    - a twisting tunnel of fans and turning disks, each with a window to slip through;
    - each target hit adds two seconds;
    - "after Level 8, the course stays the same, but you have five seconds less".
  - **TIE Fighter's Combat Chamber:** four missions per craft, with lessons radioed in mid-flight,
    and a bronze, silver or gold medallion for them.
  - **What we take:** the Proving Ground (`TRAINING.md`), built: a ring course in the colony's air,
    timed, with a par and a certificate; the Blast Hall's drill, its targets lit one at a time
    against a clock that each one struck puts time back on (the Maze's added time); trainers
    boarded in the hall; and the day's best on its wall, X-Wing's high-score table.
- **It teaches the machine.**
  - The official guide's tips for the Maze are about energy [P9]:
    - to slow down, raise laser recharge rather than cut the throttle;
    - shunt laser energy into the shields;
    - balance the shields front and rear.
  - Our suits are machines too (reactor, heat, systems, propellant: `DESIGN.md`, "Suit systems and
    malfunctions").
  - The first course teaches what the colony does to a suit. A later one could teach heat and the
    tank.
- **A cockpit you read** [P7][P8].
  - The Combat Multiview Display: the target's name, shields, hull, systems, distance and cargo.
    In TIE Fighter it shows the target as a 3D model, turned as the target is turned to you.
  - A target box that's yellow out of range, red in range, and green with a firing solution.
  - TIE Fighter's threat lights: the missile light blinks yellow while a lock is being made, and
    turns solid red once it's made.
  - Front and rear sensor scopes, whose dots dim with range.
  - Ours has the lock-on's bracket, the lead `◆` and the lock tones (`LOCK.md`,
    `bc_sound::cockpit`). The missile warning that goes from blinking to solid is the piece to
    check ours against.
- **Music that follows the fight.** iMUSE switched cues on events without a seam [P8]: a friendly
  or hostile arrival, a capital ship dropping out of hyperspace, a goal done, a victory, a failure.
  (The 1998 re-releases replaced it with looped CD audio, which fans count a loss, *unverified*.)
  Ours crossfades between calm and combat (`bc_sound::music`). iMUSE's lesson: give the music cues
  for events, not just a level of danger.
- **Briefings, and a secret order** [P8][P11].
  - The briefing is an animated map that scrolls and zooms to each group of ships.
  - In TIE Fighter you can also put questions to the deck officer, and a cloaked figure gives
    secret objectives: the Emperor's Secret Order. It has six circles, reached by flying its goals,
    and a tattoo that grows with rank.
  - Our take:
    - the Charter Board's contracts (P1 6) should brief on the chart, which already flies its view
      to things and draws courses (`chart.rs`);
    - `STORY.md`'s shady contracts could carry a second order from someone else.
- **A debrief, ranks and medals** [P7][P8].
  - After every mission: accuracy, goals, kills and losses.
  - Ranks by score; medals presented by Mon Mothma; patches on the pilot's sash.
  - Ours says what came home (`DESIGN.md`, "Sorties"). A debrief, and a pilot record that shows
    what a pilot has done, are the take.
- **The Film Room** [P8].
  - A mission is recorded and replayed from any ship's camera.
  - "Enter Sim" lets you fly on from any moment.
  - Our simulation is deterministic to the bit (`ARCHITECTURE.md`, "Determinism"), so a sortie's
    inputs replay it exactly. A film room is cheap for us once the server keeps the inputs.
- **Simple to fly, and your own path.**
  - Holland: "I like to keep the controls as simple as possible, so someone can jump in and enjoy
    the game… I want them to be able to hop into the cockpit and fly" [P12].
  - And: "We wanted it to be a flexible game system that allowed people to do different parts, not
    be forced to go always along the path" [P9]. Our objectives come in any order (`DESIGN.md`,
    "Objectives and the chart"), and the Proving Ground is recommended, never required.
- **A small cog in a war.**
  - TIE Fighter's guide: "a starfighter is only a small part of the Imperial plan" [P10].
  - Holland: "the heroic scale of a single guy having an impact… but still you wanted the sense of
    the larger story going on around the hero" [P9].
  - The Charter Board's quiet war of bounties is ours (`STORY.md`).

## The lesson under all of them

Nobody has built the whole thing because it is a city sim, a flight sim and a mech combat game at
once, each at full scale. Our answer so far:

- **One simulation and closed forms.** The city, the day, the trams and the bodies' poses are
  functions of the tick and of where you ask (`CLAUDE.md`, hot-path rules): nothing to store,
  stream or keep in step.
- **Procedural art.** No asset pipeline to feed (`DESIGN.md`: "All art is procedural").
- **Agents as population.** A world that's empty when its players are few is every MMO's risk.
  Ours has pilots who are agents (`STORY.md`, "The Arrivals"), though only the scripts start any
  so far.
- **Thin slices, joined end to end.** Every part is thin, and every part reaches the next.

The rule to keep: **connect before deepening.** A new system should send pilots along the chain
(city, bay, space, home) rather than deepen one link of it.

## What we take, most important first

How each is built (where it lands, the wire, the tests) is `ROADMAP.md`.

**P0: small, and worth doing before more players arrive.**

1. **A floor under loss.** A signed-in pilot whose only suit is destroyed, with nothing to build
   with and nothing to sell, has no way back into a cockpit. The starter kit goes only to a pilot
   with no record (`Hangar::starter`, `bc-econ/src/hangar.rs`; `enter`,
   `bc-server/src/net/session.rs`), and there's no insurance, loan or reissue. (A guest gets a
   fresh one every visit.) EVE insures every ship at 40% for nothing, as new players' safety net
   [P6]. Ours: the Charter Board reissues a worn Leo to a pilot with an empty bay who couldn't
   build one. `STORY.md`'s "debt of 2,000 credits" isn't in the code; this is where to make it real,
   or drop it.
2. **Text chat.** The first thing anyone does in a shared place is say something. In the city to
   everyone near (the plaza's interest, `bc-server/src/plaza.rs`), in space to the sector; on the
   control stream, off the hot path. T and Enter are throw and dock (`CONTROLS.md`, "Where we break
   convention" 3): pick its key first.
3. **Objectives along the chain.** Every objective today is in space
   (`bc-client-core/src/objectives.rs`). Add the colony's: ride the cap lift down, find the
   Exchange floor, sell on the book. A new pilot should find out the colony is there.
4. **The Arrival does something.** Seats (`COLONY.md`, 2.6), and it's where chat is easiest to
   find.

**P1: what makes the chain worth walking.**

5. **The Proving Ground** (X-Wing and TIE Fighter; the owner's ask). A place to learn a suit before
   it counts: a hall off Hub Gate's square, walked on foot, a trainer boarded there, and a course
   of rings through the colony's air against a clock (`TRAINING.md`). Built: the course, the hall,
   its trainers, its drill and its board; next, the hall's own course and a level ladder.
6. **Contracts on the Charter Board.** Jobs posted in the city, flown in space, paid at the desk:
   haul this, clear that claim, escort a hauler home (`DESIGN.md`, roadmap, "Contracts"), with the
   reward held in escrow as the Exchange holds its orders' (`bc-econ`). Star Citizen's mission
   givers, as our Charter Board.
7. **Liveries.** Body, trim and accent colours chosen at the suit's maintenance console, carried on
   the suit (`bc_econ::suit::Suit`) and drawn for everyone (`livery` takes only the frame and the
   faction today). Paint costs credits: a sink that sells itself.
8. **Doll offensives.** Squads that scale with the pilots out, an offensive now and then against
   the dock or MO-II, and something big: a carrier that launches Dolls. The bounties already pay
   for it (`content::salvage::bounty`).
9. **Agents as population.** Flaneurs that ride the trams and go into the places (today's walks 60
   m back and forth, `bc-bot/examples/flaneur.rs`), and the server's own tugs, haulers and miners
   in the lanes (`DESIGN.md`, roadmap, "Traffic").
10. **Decide where PvP lives.** This is a decision that's open, not a recommendation. Every browser
   pilot is on the Colonies' side (`bc-client/src/net.rs`) and friendly fire is off
   (`bc_sim::config`), so people can't harm each other, and `DESIGN.md`'s hunted sleepers can only
   be hunted by bots of another faction. Star Citizen has armistice zones and lawless space; EVE
   grades its space by security. `STORY.md`'s "law and heat" is ours, and it needs a map of where
   the law holds.

**P2: depth, once the above is in.**

11. **Cost brackets** (Battle Operation 2), for contract tiers and the Dolls sent after a pilot.
12. **Close quarters outside the law:** the axis port's scaffolds, the building site's frames.
    Where free aim is learnt.
13. **Suits inside the colony,** once the building site gives them work (`SUITS_INSIDE.md`, "Why
    bring a suit inside?"): `Stage` moved on by deliveries.
14. **Low g near the axis,** as a place to play.

## Sources

The starting point was a survey the project's owner brought in October 2026. Its claims were
checked against:

- **[P1] Battle Operation 2.** Bandai Namco's developer newsletter on the late-May 2020 map update
  (bo2.ggame.jp/en/info/?p=37296); Steam discussions on cost ranges and re-sortie times
  (steamcommunity.com/app/1367080/discussions). The infantry and the 2026 suits come from the
  survey's own source (Techprincess), *unverified*.
- **[P2] Island-4.** Ryan Schultz on VRChat worlds (ryanschultz.com/tag/vrchat-worlds); MoguraVR
  on its opening (moguravr.com/?p=154015).
- **[P3] Star Citizen.**
  - Funding: Massively Overpowered (December 2025, past $900 million); gagadget (past $1 billion,
    May 2026).
  - Griefing: RSI Spectrum, "Cargo problem: rampant abuse of armistice privilege"; starcitizen.tools
    on Alpha 3.13.1 (med guns disabled in armistice zones).
  - Hangars and delivery: RSI Spectrum, "Alpha 4.0.1 current issues & updates"; a player's 2024
    early impressions (talisman.org/~erlkonig/writings/on-games/star-citizen/early-impression-2024).
- **[P4] The Gundam Metaverse.** Automaton (March 2022, the plan; March 2023, the Gunpla Colony
  test).
- **[P5] Gundam Rogue Orbit.** Gematsu and AniTrendz (September 2026, the date and gameplay);
  Sortir à Paris on customisation and colour schemes.
- **[P6] EVE Online.** EVE University's wiki, "Insurance".
- **[P7] X-Wing.** The Collector's CD-ROM pilot manual
  (archive.org/details/Star_Wars_X-WING_STARFIGHTER_PILOT_MANUAL); Wikipedia, "Star Wars: X-Wing
  (video game)"; Wookieepedia, "The Maze (pilot proving ground)".
- **[P8] TIE Fighter.** Its pilot manual (archive.org/details/tie.-fighter-manual); Wikipedia, "Star
  Wars: TIE Fighter"; Wookieepedia, "Training simulator", "Combat chamber" and "Secret Order of the
  Empire".
- **[P9]** *X-Wing: The Official Strategy Guide* (1993)
  (archive.org/details/star-wars-x-wing-the-official-strategy-guide-1993).
- **[P10]** The TIE Fighter strategy guide (Prima) (archive.org/details/tie_figher_prima_strategy_guide).
- **[P11]** PC Gamer, "The making of Star Wars: TIE Fighter"
  (pcgamer.com/the-making-of-star-wars-tie-fighter).
- **[P12]** The Digital Antiquarian, "The Second Coming of Star Wars"
  (filfre.net/2021/02/the-second-coming-of-star-wars).
