# The UI's look

What the page around the game (`web/ui.js`, `web/style.css`) and the cockpit HUD (`bc-client`'s `hud.rs`,
`ui_panel.rs`) look like, why, and the rules a new screen keeps to. Read it before adding a panel or a
readout; check the result in the gallery (`scripts/build-web.sh page`, then `/?gallery`) and, for the HUD,
the showcase's `chase` and `duel` scenes (`scripts/e2e.sh gfx webgl2 --grep "chase|duel"`).

## What the nearest games teach

Looked at in October 2026 (store screenshots, reviews, players' complaints):

| Game | What works | What players hold against it |
|---|---|---|
| Armored Core VI (2023) | Near-monochrome off-white on dark; what a fight needs (the target's armour and stagger, your own) packed round the reticle, the rest at the edges. The garage: a selected row is a solid light bar with dark text, part names large, numbers right-aligned, better and worse in colour. | Every bar the same colour; magazines unlabelled; weapon status tiny in a corner; slow screens between the garage and the sortie; no HUD options. |
| MechWarrior 5: Clans (2024) | One teal for everything, red for damage; thin lines, small capitals; weapons as a table (name, ammo, range); the objective under a short teal bar; the Mech Lab's tabs as words across the top, part cards round the 3D mech. | A screen for every body part. |
| Daemon X Machina: Titanic Scion (2025) | White, thin: armour as a bar along the top, weapons as slanted cards with a large ammo number and a small name; the hangar's list beside the parameters it changes; one red accent. Adjustable HUD, colour-blind options. | Cumbersome menus at the start. |
| Mecha BREAK (2025) | Armour and boost as arcs either side of the reticle; the ammo count large beside it; a distance on each target. | Menus slow, cluttered and deep, like a phone game's; low contrast; icons hard to tell apart. |
| Titanfall 2 (2016) | Restraint: the screen is clean and only gets busy when the Titan's Core is ready; the reticle floats with the movement. | |
| Hawken (2012) | Precise and military, built into the cockpit, yellow as its one accent. | |
| Gundam Breaker 4 (2024) | The counter-example: gradients, hexagons and frames everywhere. Fun for an arcade toy, tiring for an MMO you live in. | |
| Zone of the Enders: The 2nd Runner | Holographic wire panels in the cockpit: the anime's glow. | |

Sources: [Steam store pages](https://store.steampowered.com) of each; [The Gemsbok's critique of Armored Core VI](https://thegemsbok.com/art-reviews-and-articles/armored-core-vi-6-fromsoftware-critique-mechanics-analysis/);
[TechNode on Mecha BREAK's launch](https://technode.com/2025/07/04/mecha-break-launches-globally-but-faces-widespread-player-criticism/);
[Kotaku on Titanfall 2's crosshairs](https://kotaku.com/im-loving-titanfall-2s-crosshairs-1788237294); [HUDS+GUIS on Hawken](https://www.hudsandguis.com/2013/11/27/hawken-mech-huds/);
[RPG Site on Titanic Scion's settings](https://www.rpgsite.net/feature/17774-branching-path-daemon-x-machina-titanic-scion-pc-settings-breakdown).

## The rules

1. **Glass, not boxes.** A panel is dark glass (the ink at `palette::GLASS_ALPHA`) with fine scanlines and
   four corner ticks (`palette::TICK_ALPHA`, `TICK_LEN`) where a box would have its border. The page and the
   HUD draw it alike (`--ticks` in `style.css`, `ui_panel.wgsl`), from the same numbers (a test holds them).
2. **Colour is meaning, and little of it.** Cyan is what is; amber is what to do (one amber thing on a screen:
   the button to press, the key, the money); red is what's wrong; green what was gained; pink the Orrery.
   Everything else is the label blue and white. Two gauges never differ by colour alone: each is labelled
   (Armored Core VI's fault).
3. **Chosen is inverted.** The choice the pointer is on, in a list of them (the menu, a filter), is a light
   plate with dark text (Armored Core VI's garage, Titanic Scion's hangar). The open tab is underlined.
4. **Small labels, large numbers.** A label is 10–11 px, spaced wide, in the label blue; the number it names
   is plain, larger and in the readout font.
5. **A heading starts with a short bar** (MechWarrior's objective) and runs out into a hairline.
6. **Quiet until it matters.** Out of a fight the HUD shows the flight and little else; a caution, a lock,
   a missile come up when they happen and go when they're over (Titanfall 2). The page shows nothing over the
   world it doesn't need to. Of the suits in view one is the **focus**, named in full with a bright bracket
   (your lock-on, else your missile lock, the Orrery's pick, the nearest hostile locking on to you, the nearest
   hostile); anyone within 600 m is named too, a few others and whoever is locking on to you carry their range
   (`LOCK 2.1 km`), the rest are dimmed brackets, and no tag lands on another (`bc_client_core::tags`).
   The **vitals** sit either side of the crosshair, as Armored Core VI and Mecha BREAK keep them: the torso's
   armour on the left (the suit is lost with it), the propellant on the right, quarter rings of ten segments
   named under their ends, dimmed while full, amber under half, red under a quarter (`hud::update_vitals`).
7. **Fast.** Nothing animates for longer than a quarter of a second or stands between a key and what it does
   (Mecha BREAK's and Armored Core VI's slow menus); motion stops when the browser asks for less.
8. **Keys as caps.** What to press is drawn as a key (`E`), then what it does.
9. **The cockpit is the same glass** (`bc_model::cockpit`, `shaders/monitor.wgsl`): dark gunmetal, and the
   monitors digital panes of see-through glass, nothing solid behind them, held at their rails and corner
   brackets on arms, the world and the frame showing through (the framed cockpit's panes hide 60% of it,
   the holo cockpit's 10%), what they show lit over it, a soft sheen and a lit rim. Each readout is as
   large as its lines fit its monitor (`hud::fit_instruments`). The suit's own light (its eye colour) runs
   in thin lines: one under each monitor, the panoramic window's four corners lit as the HUD's panels have
   their ticks, the radar's quarter marks, a band round each grip. Switching into the cockpit powers the
   monitors on, a line sweeping down each. A warm light overhead picks out the frame.

## Next

- The cockpit's readouts as instruments, not only text: the flight's speed and G as dials, the arms' rounds
  as bars, drawn into the monitors' texture (MechWarrior's weapon table, Titanic Scion's large numbers).
- HUD options: its scale and opacity as settings (`settings::KNOBS`), as Titanic Scion has them.
- The objective's marker in the view (`DRONE 1.5 km`) can sit under a focus tag: let `tags` keep clear of it.
