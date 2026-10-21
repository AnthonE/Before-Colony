---
name: playtest
description: Play Before Colony in a real browser from the shell (scripts/pt) and look at it — walk the bay and the colony, fly, fight, open the terminals and the chart, review the showcase's art — to find what needs polish, and write up what's found. Use when asked to playtest, play, look around, check how something looks or feels in the game, review the UI or the copy, or confirm a change in the running game rather than in tests.
---

# Playtesting Before Colony

`scripts/pt` keeps one browser open on the game and drives it a command at a time: you start a
world, open the game, then look (`shot`, `around`, `ui`), act (`walk`, `key`, `turn`, `use`, `cmd`)
and note what you find (`note`). Every command prints a one-line status after it (where the pilot
is, the objective, what the page is prompting, new toasts, a count of new console errors). A
screenshot is a PNG path: **Read it to see it.** `scripts/pt help` lists everything.

```bash
scripts/pt up                      # the world (builds first if the code is newer than the build)
export PT_SESSION=scout            # your own browser and pilot (default: pt); keep it for the session
scripts/pt open                    # a guest pilot, straight into their bay on foot (survival rules)
scripts/pt shot bay                # -> e2e/artifacts/playtest/scout/001-bay.png: Read it
scripts/pt walk airlock && scripts/pt use      # the cap lift down to the colony
scripts/pt wait 's.place=="city"' 120 && scripts/pt cmd '{"cmd":"skip"}'
scripts/pt around                  # four views turning round, on one contact sheet
scripts/pt note "major/copy: the Hub Gate toast is cut off at 960 px"
scripts/pt close                   # your browser (scripts/pt down: the world, for everyone)
```

`PT_SESSION` names your browser, your pilot and your folder (`e2e/artifacts/playtest/<session>/`:
the numbered screenshots, `notes.md`, `console.log`). Several agents can share one world, each
with their own session; they see each other in it.

## What it's like in there

- **It draws in software** (SwiftShader: no GPU here). Expect 2–15 frames a second in the bay and
  in space at `quality=low` (the default `open` asks for), and seconds a frame in the city. `shot`
  waits for a fresh frame; give `wait` generous timeouts. Don't call a stutter a bug unless `fps`
  in the status is high and it still stutters.
- **The low tier has no HDR, bloom or post effects.** Judge the *look* (light, materials, colour)
  only on `high`: the showcase (`scripts/pt showcase city 3 480`, High by default) or
  `scripts/pt open 'quality=high'` (slower still). Judge layout, words, flow and feel on `low`.
- **The pointer is never locked**, so the mouse can't steer: `turn <yaw> [pitch]` turns the eyes
  (flying or on foot), `mouse right <ms>` fires the secondary weapon (the primary needs the lock).
  The page's "CLICK TO WALK" / "CLICK TO FLY" stays up for that reason: it isn't a finding.
  (The driver refuses the lock on purpose: headless Chromium grants it, and with it held on foot
  the game slows tenfold and its memory climbs by gigabytes until it aborts. `PT_LOCK=1` lets the
  page lock it, to look into that.) Keys work: `key KeyW 1500`, `key ShiftLeft+KeyW 2000`,
  `key Escape`. A hold moves the pilot only as far as the frames drawn in it, so travel with
  `walk` and `fly_to` (they keep up with slow frames) and keep keys for feel.
- **If the game dies** (a wasm trap or a panic) every status line says so in capitals from then
  on, and what you see is frozen: read `logs` (the first error is the one that matters; the rest
  are its fallout), note it with the steps that led there, and `open` again. `wasm_mb` in the
  status is the game's memory: it should hold steady, and a climb is a finding.
- **The view is 960×540** (the e2e suites' size); panels or cockpit monitors cut off at its edges
  may be fine at a player's size: check with `PT_VIEWPORT=1600x900` (a new session, as the size
  is set when the browser starts) before you report one.
- **Holds and waits are wall-clock**: the world doesn't pause between your commands. A drone can
  find you while you're writing a note.

## Getting places

The e2e suites (`e2e/tests/*.spec.ts`) are the tested routes; when one of these stops working,
the suite shows the current way.

| Where | How |
|---|---|
| The title screen | `open 'autoplay=0'`, then `ui` for its controls, `click '#launch-button'` |
| The bay, on foot | `open` (survival is the default: a guest wakes in their bay) |
| A bay terminal | `walk fabricator` (or `stores`, `exchange`, `suit`), `use`; `key Escape` closes |
| Space, launched | `walk cockpit`, `use` (the launch sequence; `cmd '{"cmd":"skip"}'` skips it) |
| Home from space | `cmd '{"cmd":"fly_to","spot":"dock"}'`, `wait '!s.flying_to' 400`, `key Enter` |
| A fight | `open 'autopilot=1&frame=altair'` (the drone brain flies and fights), or flying yourself: `cmd '{"cmd":"aim_hostile","on":true}'` and `mouse right 3000` |
| The colony's streets | `walk airlock`, `use`, `wait 's.place=="city"'`, `cmd '{"cmd":"skip"}'` (the lift ride) |
| A place in the city | `walk proving_ground` (`exchange_floor`, `charter_board`, `bar`, `hub_gate_1`, `tram`, `seat`, `pool`, `gantry`, `sight_0`…`sight_9`: on the strip you're on); `use` at its desk |
| A suit inside the colony | `walk cockpit`, `key KeyQ` (Q at the cockpit launches inside), then `cmd '{"cmd":"fly_to","spot":"hub_gate_1","up":22,"ahead":60}'`; `key KeyL` lands |
| The chart | flying: `key KeyM`; `ui` reads it |
| The controls sheet, settings | `key F1`; `key Escape` for the menu, `click '#pause [data-cmd="settings"]'`… (read `ui` for the real selectors) |
| One scene, exactly | `showcase <scene> <cam> <t> <frames>`: `e2e/tests/gfx.spec.ts` lists the scenes and their moments (`lineup`, `duel`, `colony`, `hangar`, `city`, `surface`, `chase`, `lodestars`, `pair`, `markab`…) |

`status` prints the client's whole `window.__bc`; the fields that say where you are: `place`
(hangar, city, space), `seq` (walking, launching, lift_down…), `focus` (what's in view to use),
`terminal`, `district`, `strip`, `interior` (a suit inside the colony), `objective`, `alive`.
`wait` takes any JS over them: `scripts/pt wait 's.terminal=="exchange"' 60`.

## What to look for

Read the doc for the area before judging it: what it says is the intent, and a gap between the
two is a finding.

- **Broken**: console errors (`logs`; the status line counts new ones), a panic, a black or frozen
  view, a prompt that does nothing, a route above that no longer works, a pilot stuck in geometry.
- **The words** (`ui` gives you the page's exact text): the HUD terse and in capitals, colony
  notices plain English, no exposition dumps (`docs/STORY.md`, "Voice and naming"). Every name is
  the game's own: a name from another franchise is a blocker. Typos, cut-off text, text over text,
  a hint naming the wrong key (check it against the controls sheet, F1).
- **The page**: panels that overflow at 960×540, buttons that don't say what they do, a toast that
  hides what it talks about, an objective that doesn't lead anywhere you can find.
- **Controls**: compare what a key does with `docs/CONTROLS.md` (what players expect, and where
  we break convention on purpose).
- **The look**: against `docs/COLONY_LOOK.md` (worn and real, a clean colony, an anime sky) and the
  showcase's review set; z-fighting, flicker, seams, things floating or sunk, pop-in, the scale
  of people against suits and buildings.
- **The new player's way in**: from the title, could someone who's never read the docs tell what to
  do next, every step? `docs/PEERS.md` says what the nearest games have taught players to expect.

## Writing it up

`note` as you go (it records where you were and your last screenshot). When you're done, write
`e2e/artifacts/playtest/<session>/report.md`, most important first, one entry per finding:

```markdown
### <what's wrong, in a line>
- **Severity**: blocker | major | minor | nit      **Kind**: bug | ux | copy | visual | controls | perf
- **Where**: the area, and the `scripts/pt` commands that get there from `open`
- **Seen**: what happens (the screenshot's file name)
- **Expected**: what should, and the doc that says so
- **Fix**: where in the code, if you looked (`crates/bc-client/src/onfoot.rs:1181`), and how
```

Say what you couldn't reach or check, and what's slow because the browser draws in software
rather than because the game is. Leave a finding out rather than guess at one: a claim someone has
to re-check costs more than it saves. Fix nothing while playtesting unless you were asked to;
your report is the deliverable.

## When you change the game

`scripts/pt up` rebuilds a stale build but doesn't restart a world that's running: after changing
the code, `scripts/pt down && scripts/pt up` (a web-only change: rebuild with
`scripts/build-web.sh webgl2` and `scripts/pt open` again). The dev hooks the driver uses are
`window.bcInbox` (`crates/bc-client/src/page.rs`, `UiCmd`) and `window.__bc`
(`crates/bc-client/src/dev_hooks.rs`); a playtest that needs another is worth adding there, as
`look` was for `turn`.
