# The Exchange floor, from the store

The Colony Exchange's trading floor (`content::city::PLACES`, `THE EXCHANGE FLOOR`: a 48 m by 36 m hall
under a 12 m ceiling, the counter and the boards on its back wall) is where Before Colony's traders
stand. This is the plan for making it the room Elo Pros' traders stand in too: like the auction house
in a city of a big MMO, but reached from a web page as well as from the game.

## The one rule: one room, two clients

There is never a second copy of the floor to keep in step. The game's interior sector is the only
place the room exists. A pilot walks in through the door from the street; a trader on the store's
page appears in the same room as a lighter client of the same server. Both are people on foot in
`plaza.rs`, checked by the same rules, seen by everyone in the room alike.

A lighter client is confined to the room: it can go anywhere inside, never out of the door. The
door is the way into the rest of the game ("that's the colony: launch the game to go out").

## Steps

1. **Look in (built).** `/floor.json` on the game server (`bc-server/src/floor.rs`): who is on
   the floor now, where in the room (its own frame: `u` along the front from the door's middle,
   `v` in from the front face towards the boards) and which way they face, whether an agent flies
   them, and what the boards show (the busiest items: bid, ask, last, volume, in credits). The
   room's shape is in the document, so the page needs no copy of the city. Only people are
   listed (the colony's crowd never walks in here), by the callsign other pilots already see; no
   wallet or key. Answered to the store's origin (`--cors-origin`). The store reads it through
   its own API and draws the room from above in the trade terminal's floor pane
   (`AnthonE/scry-forge`, `watchtower/js/floor.js`).
2. **Walk in.** The store's page signs in with the wallet the game already accepts (SIWE, both
   hosts on `--siwe-domain`) and sends poses like any pilot on foot. The plaza takes a lighter
   client's first pose just inside the door, and refuses any pose of it outside the room
   (`Why::Room`, beside `Why::Locked`). Measure whether the page can carry the game's wasm
   (`web/dist`'s size) or draws the room itself; the server does not care which.
3. **Talk.** Trade chat as bubbles over each person (the store's trade chat, keyed on the same
   wallet); proximity voice as a LiveKit room per floor, each voice's gain set on the listener's
   side by distance in the room's frame (the store's voice box, `docs/ops/VOICE.md` there). A
   guild's tag over its members (the store's guilds, keyed on the wallet, which `/floor.json`
   then carries for signed-in pilots).
4. **Agents on the floor.** An agent trader is a `bc-bot` that signs in, walks to the counter and
   places orders; its body stands there while it works.
5. **The terminal in the game.** The counter's terminal grows the panels a trader expects:
   candles from the exchange's tape, a depth ladder, time and sales.

## Scale

A busy floor is a hot spot. Today a viewer sees at most `MAX_PEOPLE` others (one datagram's worth,
nearest first). Past that, the floor is the first place to try `NETCODE.md`'s crowd plan (the
changed inputs of a strip, coded once and broadcast), and tiers: the nearest in full every tick,
the rest less often. `/floor.json` lists at most `floor::MAX_LISTED`, and reports how many are
`here` in all.

## What the floor never does

It never shows anyone who isn't there. The room's energy is the people in it; the colony's crowd
stays on the street, and a page that drew a fuller room than the server reports would be inventing
an audience.
