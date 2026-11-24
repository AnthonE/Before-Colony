# Decisions: spoken, and waiting to be spoken

Two lists, in Gates' shape (`AnthonE/Gates`, `docs/DECISIONS.md`). **Spoken** is append-only: the
date, the call, the owner's own words, where it lands. **Open** is every knob still waiting on the
owner, with the default that ships until they speak. An agent may rely on a default; only the owner
moves a knob from open to spoken.

The owner's decisions made before this page are in the area docs that asked for them, and stand:
`FREE_ROAM.md`, `RING.md` and `OPEN_CITY.md`, each under "The owner's decisions".

## Spoken

| Date | Decision | Pointer |
|---|---|---|
| 2026-11-22 | **Phases to 1.0, and take from the sibling games** ("let's really lay out some sort of phases of how we can build this game ... AI really has a chance at making a really AAA quality game ... if you need to check out one shot and Gates, they might have some things that could be helpful to rip on") | `PHASES.md` |
| 2026-11-23 | **The browser first** ("browser first"). Steam waits; the desktop build comes when the browser's first hour is done | `PHASES.md`, 0.5 |
| 2026-11-23 | **The business is Elo Pros'** ("business model is whatever ELO pros needs, it's our home; we got the scry-forge repo that has that site"). What a listed game owes the store, and what it sells there, is `AnthonE/scry-forge`'s to say; Before Colony builds its side | `PHASES.md`, "The business" |
| 2026-11-23 | **Suits fight each other away from the colony** ("pvp should probably happen mostly away from the space colony ... you can't shoot the space colony, but outside a certain range that's fine. THIS IS FOR MECH"). Pilot against pilot in suits, outside a range round the ring; inside it, weapons safe between pilots, and the colony itself never takes fire | `PHASES.md`, 0.3; `PEERS.md` 10 |
| 2026-11-23 | **On foot, open season anywhere** ("when it comes to humans that's pretty much open season anywhere"). Pilots on foot can hurt each other wherever they meet; the Watch answers it as a crime where it holds (`OPEN_CITY.md`, "Heat and the Watch") | `PHASES.md`, 0.3 |
| 2026-11-23 | **Open a paid board** ("open a paid board"), as Gates and oneshot run theirs | `AGENTS.md`, `CONTRIBUTING.md` |
| 2026-11-23 | **Proximity voice** ("voice would be good, proximity") | `PHASES.md`, 0.3 |
| 2026-11-23 | **Build all of it now** ("let's cook it all") | this page |

## Open (defaults ship until spoken)

| Knob | Default | Where |
|---|---|---|
| The reference machine ("runs well, idk") | A 2021 mid-range laptop: an Intel i5-1135G7 with Iris Xe graphics, 16 GB, 1080p, Chrome stable. The low one: a 2018 laptop with Intel UHD 620 and 8 GB, at Low. A browser game is judged on integrated graphics | `PHASES.md`, "What AAA means" |
| How far from the colony suits may fight pilots | 5 km from the ring's hull and the hub | `PHASES.md`, 0.3 |
| What a pilot who kills a pilot outside the range owes | Nothing; inside it they can't | `PHASES.md`, 0.3 |
| The board's price | Gates' and oneshot's: a flat price for a merged pull request, more for content that ships, set and funded on elopros.com | `CONTRIBUTING.md` |
| Voice in space | A suit's radio, out to the sensors' range; on foot, the voice carries 30 m | `PHASES.md`, 0.3 |
| The copy at listing | Free, one per wallet, as Gates' | `PHASES.md`, "The business" |
| Where the store's items live | Solana, one collection for the title, priced in the store's coin (the listing's `pass_solana`) | `PHASES.md`, "The business" |
| A coin of Before Colony's own | None: credits in the game, the store's coin for its items | `PHASES.md`, "The business" |
| What's sold and what's only earned | Paint, palettes and decals sold; a suit's frame, parts, credits and anything with stats only earned | `PHASES.md`, "The business" |
| Paying players from play | Reviewed season prizes from the boards (no wipes but the one at listing) | `PHASES.md`, "The business" |
| Ducking into the colony's reach mid-fight | Safe at once: no engaged timer yet (Gates and Rust hold an attacker hostile for 5 minutes) | `docs/ROADMAP.md`, "PvP in space" |
| PvP under arcade rules | Off: arcade spawns every side together for the combat suites (`--pvp` turns it on) | `crates/bc-server/src/config.rs` |
| A fight on foot | 100 health, a blow 20 (twice from behind), 2 m reach; down 25 s, up with 30; a lethal hit within 60 s of getting up puts you out, and out wakes in the bay; 10 s of protection on arrival, lost by striking first; hitting back within 15 s is free of heat | `crates/bc-sim/src/colony/brawl.rs`, `docs/OPEN_CITY.md` |
| Voice for agents and guests | Agents hear no one's voice (`--voice-agents` lets them); guests talk and hear like anyone | `crates/bc-server/src/voice.rs` |
| Push to talk | Hold \` or the mouse's back button; a toggle in the settings; no open mic yet | `docs/CONTROLS.md` |
| Reports from players | 5 a player's address each 10 minutes, kept under the server's data dir; the store's intake is the same shape | `crates/bc-server/src/bugs.rs` |
