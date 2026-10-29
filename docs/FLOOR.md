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
   the floor now, where in the room (its own frame, `Room::local`: `u` along the front from the
   door's middle, `v` in from the front face towards the boards) and which way they face, whether
   an agent flies them, whether they're one of the store's traders (`visitor`), and what the boards
   show (the busiest items: bid, ask, last, volume, in credits). Only people are listed (the
   colony's crowd never walks in here), by the name other players already see; no wallet or key.
   Answered to the store's origin (`--cors-origin`).
2. **The store's traders stand in it (built).** Everyone signed in on the store's trade page
   stands on the floor, by the account's public name (`bc-server/src/visitors.rs`). The server
   reads the store's list (`--floor-visitors`; by default the store's own when `--siwe-domain`
   names elopros.com) every 10 s, and the plaza stands each trader on a spot of their own between
   the door and the counter, facing the boards (`Plaza::set_visitors`, slots from `VISITOR_BASE`
   up, above every session's). They don't move yet, and they never go stale between reads; a
   trader the list drops goes, and a list not read for a minute empties the room. Pilots walking
   in see them like anyone on foot; they aren't pilots, so `/status` doesn't count them.
3. **The game's own view of it, on the store (built).** `?showcase=floor` is the room drawn by the
   game (`showcase::Scene::Floor`): the city's Exchange floor, lit and furnished as in play, the
   street's life through the door, and everyone `/floor.json` lists standing where they are
   (`bc_client_core::floor`; `web/floor.js` reads the document every 2 s into
   `window.__bcFloor`). The store's trade terminal frames it (the store's `js/floor.js`).
   Cameras: in from the door (1), from the counter's end back to the door (2), high over the
   door (3).
4. **Walk in.** The trade page sends poses as a pilot on foot, confined to the room by the
   plaza (`Why::Room`, beside `Why::Locked`). The door is the way into the rest of the game.
5. **Talk.** Trade chat as bubbles over each person; proximity voice as a LiveKit room per floor,
   each voice's gain set on the listener's side by distance in the room's frame (the store's voice
   box); a guild's tag over its members.
6. **Avatars.** Each trader in their own look (anime, the game's), set on the store and carried
   with their name.
7. **Agents on the floor.** An agent trader is a `bc-bot` that signs in, walks to the counter and
   places orders; its body stands there while it works.
8. **The terminal in the game.** The counter's terminal grows the panels a trader expects:
   candles from the exchange's tape, a depth ladder, time and sales.

## Scale

A busy floor is a hot spot. Today a viewer sees at most `MAX_PEOPLE` others (one datagram's worth,
nearest first). Past that, the floor is the first place to try `NETCODE.md`'s crowd plan (the
changed inputs of a strip, coded once and broadcast), and tiers: the nearest in full every tick,
the rest less often. `/floor.json` lists at most `floor::MAX_LISTED`, and reports how many are
`here` in all.

## What the floor never does

It never shows anyone who isn't there. The room's energy is the people in it: pilots, and the
store's traders, each a real person signed in on its trade page now. The colony's crowd stays on
the street, and a view that drew a fuller room than the server reports would be inventing an
audience.
