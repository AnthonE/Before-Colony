---
name: fx
description: Make or tune a visual effect in Before Colony (beam hits, muzzle flashes, saber clashes, a suit's death, missile bursts, rocks shattering, thrusters) on the fx stage: stage it alone in the browser build in front of a dark and a light card, read its filmstrip and numbers, change it, and compare against the last run. Use when asked to add, fix, tune, compare or review an effect or a particle look.
---

# Effects in Before Colony

An effect is judged as it moves, never from a single still. The loop is:
1. stage the effect alone;
2. read its **strip** (every frame, its envelope and its numbers);
3. change one thing;
4. read the strip again and its **diff** against the last run.

What is shared and what isn't:
- **The stage's contract** (`fx::stage`) and **the strip tool** (`scripts/fxstrip.mjs`) are the
  shared `fx` crate's, **vendored from Gates** (`scripts/fx-sync.sh`).
- **Our particles are still our own** (`crates/bc-client/src/particles.rs`,
  `shaders/particles.wgsl`): Gates' and oneshot's pool and knobs aren't ours yet. So a change
  here is an edit and a rebuild, not a knob.

```bash
scripts/fx.sh list                              # every effect the stage can fire
scripts/fx.sh kill                              # → e2e/artifacts/fx/kill/{strip.png,numbers.json,diff.png}
scripts/fx.sh kill hit-beam-rifle clash         # several, one build
scripts/fx.sh kill --quality low                # the low tier: no HDR, no bloom (the default is high)
scripts/fx.sh kill --frames 15 --span 2         # more frames over a shorter span
scripts/fx.sh --no-build …                      # reuse web/dist
node scripts/board.mjs look --views fx-kill     # the strip on the board, beside its refs and marks
```

**Read `strip.png` first.** Its parts:
- **The hero frame:** the brightest frame, drawn large.
- **The envelope:** how much each card changed over time; the last run is dashed.
- **Every frame:** captioned with its step, time and per-card numbers.

Then read `numbers.json` for anything the picture leaves open, and `diff.png` after a change.

Setup:
- `scripts/fx.sh` builds `web/dist/webgl2` (`scripts/build-web.sh`, minutes), serves it itself and drives
  `?showcase=fx&fx=<name>` in headless Chromium on SwiftShader. It needs:
  - `wasm-bindgen-cli` 0.2.128;
  - Playwright;
  - no game server.

## The stage

- **The scene:** the showcase's `fx` scene (`crates/bc-client/src/fx_stage.rs`, `showcase.rs`'s `Scene::Fx`).
- **One effect, fired twice at once,** out in empty space:
  - in front of a **dark card** (the left half; light reads here);
  - in front of a **light card** (the right half; smoke, dust and debris read here).
- **The camera:** fixed (`fx::stage::Framing`), sized to the effect's radius, at mech scale (a hit is ~8 m, a kill ~40 m).
- **The clock:** the showcase's fixed 60 Hz clock, held by the film hook (`window.__bcFilm.step`, `__bcWake`):
  - step 0 is the **plate**, before the effect fires;
  - the effect fires on step 1.
- **What it fires:** a real `FxEvent` (`view.rs`), drained by `fx.rs` as a match's would be:
  - `Hit` and `Muzzle` for every `WeaponKind`;
  - `Kill`, `Blast`, `Clash`, `RockBreak`, `MissileBurst`, `Touchdown`, `Eject`, `Stagger`.
- **Names:** `hit-<weapon>`, `muzzle-<weapon>` (kebab case: `hit-heavy-beam-rifle`), `kill`, `blast`, `clash`, `rock-break`, `missile-burst`, `touchdown`, `eject`, `stagger`.
- **Not on the stage yet:** effects driven by a suit's state rather than an event (thrusters, charge, the flamethrower, the saber's cut). Those still go through the showcase's scenes (`lodestars`, `lineup`, `duel`: `scripts/pt showcase …`).

## The numbers

Every number is measured per card against the plate, on sRGB 0–255 Rec.601 luma.

| number | what it says | read it as |
|---|---|---|
| `add` | mean light added over the card | glow: beams' flashes, sparks, fire. Peaks in the first frames of an impact |
| `sub` | mean light taken away | smoke, dust, debris. Mostly on the light card |
| `cover` | % of the card that changed at all | how much of the frame the effect owns |
| `peak` | 99.9th-percentile luma of what changed | 255 = something hit white |
| `clip` | % of the changed pixels at 250+ | a flash may clip for a frame; a cloud never should |
| `bbox`, `c` | where it changed, 0–1 of the card; its centre | spread and drift; `travel` is how far `c` moved |
| `envelope` | `peak`/`peak_t`, `fall_t` (half), `life_t` (last seen), `area_max`, `travel` | the effect's shape in time |
| `vs_prev` | % of each frame that moved since the last run | a change should move only the frames it means to |
| `plate.ok` | the cards read ~25 and ~185 | if not, the light changed and nothing else is comparable |

The numbers keep a floor under the eye; they never replace it. Look at the strip.

## Make or tune one

1. **Write the brief before touching anything** (3–5 lines). It covers:
   - the **envelope**: an instant or sustained, and how long;
   - the **size**, at mech scale;
   - the **colour and temperature** over life;
   - what must read on **each card**;
   - the **reference**: a board ref or the operator's words. Every look is the game's own, never another franchise's art (`docs/STORY.md`, "Voice and naming").
2. **Find the code.**
   - `fx.rs::update_fx` turns each `FxEvent` into calls on `Particles` (`particles.rs`):
     - its presets (`impact`, `muzzle`, `explosion`, `clash`, `smoke`, …), with the numbers inline;
     - the `Ramp`s (`Spark`, `Ember`, `Fire`, `Flash`, `Vapour`, `Dust`, `Smoke`, `Glow`) for colour and opacity over life.
   - Beams and plumes are ribbons (`beams.rs`); shock shells and chips are `blast.rs`.
   - The shaders are `shaders/particles.wgsl`, `beam.wgsl` and `shock.wgsl`.
3. **Stage it.** New: add it to `fx_stage.rs`'s `list()` (a name, a radius, a span, how it fires); something `Fire` doesn't cover gets a variant.
4. **Shoot, read, judge** against the brief, or send the `fx-judge` agent (`.claude/agents/fx-judge.md`) for a read that is not anchored on yours.
5. **Change one thing, rebuild, shoot again.** `diff.png` must show only the frames you meant to change.
6. **Check:**
   - `--quality low` (no HDR, no bloom): it must still read there;
   - the effect in its scene (`scripts/pt showcase duel 2 1.85`; the gfx suite's moments);
   - WebGPU once, for any shader you touched (`scripts/build-web.sh webgpu`, `?gfx=webgpu`).
7. **Board.** `node scripts/board.mjs look --views fx-<name>`.
   - You may `mark fx-<name> note "…" --by <you>`.
   - `right` and `wrong` are the operator's.
   - A new effect gets an `fx-<name>` view in `board/board.json`, sharing the fx views' `shoot` list.
8. **Then:** `scripts/quick.sh`, then commit.

## Judging (after Gates' threejs-procedural-vfx and threejs-visual-validation skills)

- **Every layer has a role:** silhouette, motion, light or residue.
- **One event drives every layer.** An impact peaks in the first two or three frames; its residue comes after.
- **Space is vacuum.** Debris and smoke keep their speed (no drag, no gravity in our particles). A cloud that slows like air is wrong unless a body's field explains it.
- **It reads on the low tier** (no bloom). A hot core on the dark card; nothing vanishes at range.
- **Alpha layers read on the light card:** smoke and debris have a body (`sub` well above 0).
- **Clip is for an instant.** `clip` over ~5% past the first frames is a blown-out blob.
- **Life matches the brief.** `life_t`, `gone`: nothing hangs or pops.
- **Motion from the strip, never one frame.**
- **Budget.** The tier's particle budget (`gfx.rs`: 300/1500/4000/10000) and light budget (0/4/12/24): a battle is many of these at once.
- **WebGPU rejects a derivative under a branch.** Take them at the top of `fragment()` (CLAUDE.md).

## Rules

- **Never edit `crates/fx`, `scripts/fxstrip.mjs` or `scripts/board.mjs` here.** `crates/fx/tests/vendored.rs` fails on an edit. Change them in Gates (or the agent kit), then `scripts/fx-sync.sh`.
- Effects draw; they never decide. Numbers live in code.
