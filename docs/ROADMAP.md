# Roadmap: how PEERS.md's list gets built

`PEERS.md` says what the nearest games teach and what comes first. This is how each item gets built:
where it lands in the code, what goes on the wire, and how it's tested. It follows the house rule
from `PEERS.md`: **connect before deepening**, so every item sends pilots along the chain (the city,
the bay, space, home).

## Status

| Item | State |
|---|---|
| Lock-on: fighting on the ground in space (`LOCK.md`) | built |
| The weapons pass: the hit-rate harness (built), charged beams, true cones, lunges that home, a burst step | building |
| P0: a floor under loss, text chat, objectives along the chain, The Arrival's seats | next |
| P1, P2 below | planned |

## P0: before more players arrive

**A floor under loss.** A signed-in pilot whose only suit is gone, with less than a torso's worth of
credits and stores (at the colony's bid prices), finds a worn Leo in the gantry from the Charter
Board, at most once every 30 minutes (`bc-econ` `Hangar`, with a `reissued_at`; `bc-server`
`session.rs::enter` and the return from a loss). The news says so. `STORY.md`'s "debt" becomes "an
advance". Tests: `bc-server/tests/hangar.rs` (lost, broke, reissued; not again within the half hour;
not while there's a torso to fit). Known, and not made worse: a new wallet is a new starter kit.

**Text chat.** `Request::Say { text }` and `Update::Said { from, text }` on control-stream frames
tagged 11 (`bc_econ::wire`, `PROTOCOL.md`). One channel, the colony's radio, for everyone connected:
a ring in the server's shared state, read by each session on its 100 ms tick (off the hot path).
At most 5 lines in 10 s and 160 characters a line, control characters stripped, never logged (only
counted on `/status`). `/` opens it (Enter too, on foot, where Enter docks nothing); keys don't reach
the suit while typing. Agents: `bc-bot` `say` and `heard`. Tests: a two-bot server test; the page's
`ui` e2e types a line.

**Objectives along the chain** (`bc_client_core::objectives`, appended so their bits stay): ride the
cap lift down, find the Exchange floor, sell on the Exchange (with the colony open, the Welcome's
COLONY flag), each with a waypoint to the place's door in the city view.

**The Arrival does something.** Seats by its door (`content::city`), E to sit, others see you sit
(the presence's `ride` 15 is seated: no new bits, a protocol bump for the meaning), and chat over
the heads of those near you. Flaneurs come and sit.

## P1: what makes the chain worth walking

**Contracts on the Charter Board.** `bc-econ` `contracts.rs`: jobs the colony posts (deliver this
much ore or these parts to the dock; down Dolls over the field; bring a wreck home), each with a
reward held in escrow as the Exchange holds orders. Taken only at the Charter Board's hall in the
city (its door opens `Panel::Terminal(Spot::Charter)`), done in space, paid at the dock on the way
home (`Homecoming`, bounties). Wire: `Request::{Contracts, Take, Abandon}`, `Update::Contracts`.
Colony projects are contracts too: deliveries to the building site move its `Stage`
(`bc_sim::colony::city::Stage`, today `Stage(0)` everywhere), which becomes world state the Welcome
carries and every client draws from. Tests: econ unit tests (escrow balances, as the Exchange's
property test does), a server test that takes, flies and is paid; the colony e2e opens the board.

**Liveries.** Body, trim, accent and eye colours chosen at the suit's maintenance console from a
palette, carried on the suit (`bc_econ::suit::Suit.paint`), paid for in credits (a sink), and drawn
for everyone: sent with the roster on the control stream (it changes rarely), so the snapshot pays
nothing; `suits_vis::livery` takes it when there is one. Tests: econ (paint costs, survives strip and
refit), the hangar e2e paints a suit.

**Doll offensives.** The patrol scales with the pilots out (`SimConfig::target_dolls` as a base plus
a share per awake pilot, within the sector's suits), and now and then an offensive: a wing of Dolls
from one direction against the dock or MO-II, announced by the Charter Board, with a bounty on top.
Then something big: a carrier that launches Dolls (a new frame: content, model, a kit). All in the
sector tick, so no allocation and determinism goldens for the new scenarios.

**Agents as population.** Flaneurs that walk routes (`city_nav`) between the places, ride the trams
and sit at The Arrival; the server's own tugs, haulers and miners flying the lanes (agents on the
same protocol, started with the server). Tests: the plaza test with a walking flaneur; a sector
test with traffic.

**Where PvP lives: a decision for the owner.** Today every browser pilot is on the Colonies' side
and friendly fire is off, so people can't harm each other, and `DESIGN.md`'s hunted sleepers can be
hunted only by agents of another faction. The options:
1. As now: pilots against the Consortium's Dolls, never each other.
2. Lawless space past the colony's reach (say, beyond 8 km of the hull): PvP there, the colony's law
   inside it (Star Citizen's armistice zones; EVE's security levels).
3. Opt-in: a challenge sent with the lock-on and accepted.
4. Factions for pilots (`STORY.md`'s eras): sides chosen, and fought over.

## P2: depth, once the above is in

- **Cost brackets** (Battle Operation 2): a suit's cost from its stat sheet or its parts' prices,
  used to tier contracts and to size the Dolls sent after a pilot.
- **Close quarters outside the law:** solid structures at the axis port and the building site's
  frames (`COLONY.md` 1.3's colliders first), where free aim is learnt.
- **Suits inside the colony** (`SUITS_INSIDE.md`), once the building site gives them work.
- **Low gravity near the axis** as a place to play.
