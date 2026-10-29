# The UI's look

What the page around the game (`web/ui.js`, `web/style.css`) and the cockpit HUD (`bc-client`'s `hud.rs`,
`instruments.rs`, `ui_panel.rs`) look like, why, and the rules a new screen keeps to. Read it before
adding a panel or a readout; check the result in the gallery (`scripts/build-web.sh page`, then
`/?gallery`) and, for the HUD, the showcase's `chase` and `duel` scenes
(`scripts/e2e.sh gfx webgl2 --grep "chase|duel"`).

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
   Everything else is the label blue and white. On the page the numbers read off a screen (a time, an
   amount held, a stat's bar, a count) are yellow (`--yellow`), apart from the amber of what to do. Two
   gauges never differ by colour alone: each is labelled (Armored Core VI's fault).
3. **Chosen is inverted.** The choice the pointer is on, in a list of them (the menu, a filter), is a light
   plate (on the page a bluish light grey, `--plate`, not a glare of white) with dark text (Armored Core
   VI's garage, Titanic Scion's hangar). The open tab is underlined.
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
   the holo cockpit's 10%), what they show lit over it, a soft sheen and a lit rim. The side monitors sit
   inside a 16:9 view. Each panel is drawn as large as it fits its monitor, measured
   (`instruments::fit_instruments`, in steps so a line coming and going doesn't make it jitter). The
   suit's own light (its eye colour) runs in thin lines: one under each monitor, the panoramic window's
   four corners lit as the HUD's panels have their ticks, the radar's quarter marks, a band round each
   grip. Switching into the cockpit powers the monitors on, a line sweeping down each. A warm light
   overhead picks out the frame.
10. **Instruments, not lines of text.** What has a measure is drawn as one, in lit segments
    (`bc_client_core::instruments::Gauge`, `ui_panel.wgsl`): the flight's speed and G as dials, the
    speed round to boost's cruise and marked at the cruise, G round to half again what the pilot
    bears, marked there and red past it; every store (armour, propellant, energy, rounds) and load
    (heat, the blows, strain) as a bar, a store amber under half and red under a quarter, a load amber
    over 60% and red over 85%. A panel is a table under its heading (MechWarrior's weapon table): a
    key, a label, a bar, its number, its state; what's broken and what to do about it in notes under
    it. What a panel shows is its readings (`instruments::Readings`), apart from how they're drawn
    (`instruments::update_instruments`): the game reads them off the suit, the showcase makes them
    up, and a new readout is a row, not a new layout.

## The front screen

The title is laid out like Gates' menu (itself after Rust's, the launcher players of both know): the
wordmark and who you fly as along the top, a column of choices down the left (PLAY, THE COLONY,
CONTROLS, SETTINGS; BACK TO ELO PROS on the store's copy), the pane picked beside it in glass, the
world to the right. It's up from the page's first moment: the game downloads behind it, LAUNCH
fills as it comes (boot.js's steps, `bc-boot`) and opens once the game has sent `init`; a boot that
fails says so under LAUNCH, with RELOAD, TRY WEBGL 2 and COPY DETAILS. The server is a row, as
Gates' list is: up or not, its rules, the pilots and agents on it (`/status`), and THE COLONY reads
the same document for the sector now and the Proving Ground's best (callsigns and times only, as on
the hall's wall). Until the world draws, a still of space at L1 stands behind it. With `?autoplay`
it's only the link's state. Its states are the gallery's `title*` scenes.

## The suit's console

The suit's maintenance console (the bay's SUIT tab) is laid out like a mech lab: Armored Core VI's
garage, MechWarrior 5's part cards round the mech, Titanic Scion's list beside the parameters it
changes. The suit stands in the middle, the real one in its gantry: while the tab is open the camera
leaves the pilot's eyes and stands off its front (`bay::Refit`, eased in a quarter of a second), the
page sends the drag and the wheel as `suit_view`, and the panel's middle is clear glass, each part
round it a pane of its own. Across the top, the suit, whether it would launch (and why not) and its
totals; under them the slots as numbered tabs (1–9: the six parts, the weapons, the equipment, the
service), a square where one needs work (red: missing or failed; amber: damaged, worn, a weapon not
fitted, the tank low). It opens on the first that does.

Down the left, the open slot's choices as cards: what's fitted, what the stores hold for it, the
slot empty. Each card is a few rows of the stat sheet, the ones it changes, as bars with a tick where
the suit stands now and the change in green or red (the console's `refit`, worked out by the hangar's
own fit and strip on a copy, so it can't disagree with what the server will do). Round the suit,
a callout per slot (the suit's right on the left, as it faces you). On the right, the chosen card in
full: what it is, its rows, how you fire it (a key against each mount and the special), the stat sheet
as it would launch with it, and the one thing to do, amber, at the pane's foot. ↑↓ choose, Enter
does it. Its states are the gallery's `suit*` scenes.

## Next

- The Orrery's panel as instruments too: its picks' confidence and the threat as bars.
- HUD options: its scale and opacity as settings (`settings::KNOBS`), as Titanic Scion has them.
- The suit's console: light the chosen slot's part on the suit itself, and paint (liveries) as a slot.
- The objective's marker in the view (`DRONE 1.5 km`) can sit under a focus tag: let `tags` keep clear of it.
