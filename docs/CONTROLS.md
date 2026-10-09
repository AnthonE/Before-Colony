# Controls: what players will expect

A survey (September 2026) of the games Before Colony's players will measure it against: Gundam and
mech games, 6DOF space sims, and PC shooters in the browser. It sets our scheme against theirs, says
where we break convention on purpose and what that costs, and lists what players will ask for, most
important first. `DESIGN.md` has the controls as they are.

Keys marked *unverified* come from secondary sources only. The sources are listed at the end.

## Is anything like it?

Nothing we found combines Newtonian 6DOF flight, mouse free aim, mobile suits and an MMO [A]. The
nearest games each share only part of it:

- **Mobile Suit Gundam Battle Operation 2** (GBO2) has Gundam suits and manual aim. Funnels lock
  after you hold the reticle on a target, which is our missile lock. But you stay upright, and space
  only adds "ascend and descend" to the ground controls [A1].
- **Star Citizen** is a Newtonian MMO, with coupled/decoupled flight, G protection and lead pips. It
  flies ships, not limbed suits [A2][B2].
- **Elite Dangerous** and **Space Engineers** have flight-assist and dampener toggles and
  zero-G movement.
- No Gundam game has flown Newtonian 6DOF, the two Gundam MMOs (UC Gundam Online, Gundam Online)
  included [A1].

So players arrive with three sets of habits that disagree with each other:

- **Gundam action players** expect lock-on and target switching, a dodge step, homing melee, and a
  controller.
- **Space-sim players** expect a flight-assist toggle, lead pips, a velocity vector, radar, and
  HOTAS.
- **Mech-sim players** expect a cockpit view and a second reticle for the arms, and bring the
  first-person against third-person argument with them.

We can't meet all three by default. The pillars choose for us: free aim with no gun lock, and
Newtonian flight. The cost is paid in reading aids and onboarding.

`PEERS.md` looks at the nearest games beyond their controls: what each has of the whole fantasy,
and what we take from it.

## The cockpit view

Tab, or the mouse wheel (in for the cockpit, out to chase), switches between the chase camera and
the cockpit. The choice is a setting, so the next sortie starts in the same view.

- **Why Tab.** V is the usual camera key, but V is our flight assist. MechWarrior Online and Star
  Citizen use F3 and F4 [A2][B2], but function keys need Fn on Mac laptops and 60% keyboards have
  none [C1]. Tab is free, reachable from WASD and in the same place on every layout, and there's no
  tab-targeting in this game for it to be mistaken for. Scrolling in to go first person follows
  games whose third-person camera zooms on the wheel (Skyrim and Starfield, *unverified*) [C4].
- **Where the eye sits.** The camera is the head's main camera, whose picture is what a mobile
  suit's cockpit monitors show. Gundam canon has it both ways: the cockpit is in the torso and the
  head carries the sensors [A3]. Neo-Bird looks out from over its canopy, along its nose.
- **Where it looks.** Along the aim, as the chase camera does, and the suit turns after it. This is
  War Thunder's mouse-aim model and Star Citizen's gimballed-guns-follow-the-cursor model. It is not
  MechWarrior Online's, where the view is locked to the torso and a second reticle floats over it
  [A2]. Our suits turn to the aim by themselves, so a torso-locked view would send the crosshair
  wandering across the screen while the body caught up.
- **Two reticles, the other way round.** MWO draws a fixed torso reticle and a floating arm
  reticle; Star Citizen added boresight symbols [B2]. Ours: the crosshair is the aim. While a weapon
  can't bear, because hands reach only 50° off the body's axis and Neo-Bird's nose 2°, the crosshair
  dims and `( )` marks where the primary weapon would fire.
- **The velocity vector.** `-o-` shows where the suit is drifting (`-x-` when moving backwards).
  Elite's flight-assist-off pilots keep asking for exactly this [B1].
- **Your own suit.** Everything but the head the eye is inside stays drawn, so blade strokes, the
  Dragon Fang and a shoulder coming round are all visible. MWO's cockpit shows arms and legs [A2].
  A test checks that no frame's own armour sits inside the camera's near plane or across the
  crosshair's central 15°.
- **Damage.** With the head shot off, the picture comes from the sub-camera: greyer, fringed, and
  marked `SUB-CAM` on the armour readout. Senjou no Kizuna II's cockpit glitches on hits [A1].
- **Chase camera for wrecks.** A wreck is always watched from the chase camera.
- **Third person stays the default.** MWO added third person over fan protest because new players
  couldn't read how the mechs moved. Hawken Reborn made it the default [A2]. Xbox's accessibility
  guidelines ask that players be able to choose between first and third person [C5].

## Where we match convention

| Ours | The same elsewhere |
|---|---|
| W/S forward/back, A/D strafe | Star Citizen, Everspace 2, Warframe Archwing [B2][B3] |
| Space / C up/down | Space Engineers, key for key [B4] |
| On a body: Space hops (held, lifts off), C crouches | Space Engineers' jump and crouch on foot, the same keys [B4] |
| L arms the grip to land, and clears it to let go | Elite's landing gear (*unverified*) [B1] |
| Q/E roll | Star Citizen, Everspace 2, Space Engineers [B2][B3][B4] |
| Shift boost · X brake | Star Citizen (X is its space brake) [B2] |
| R RCS | Kerbal Space Program (R toggles RCS) [B4] |
| LMB/RMB weapons | Mecha BREAK [A2] |
| Esc menu | The browser's own exit from pointer lock [C1] |
| Hold the reticle on a target to lock missiles | GBO2's funnels [A1] |
| M the chart (a 3D map) · N its auto-nav | Elite's galaxy and system maps and nearly every PC game with one; dragging to turn a 3D map and the wheel to zoom it, as in Elite's galaxy map; `◆` waypoints as in Everspace 2; flying a picked destination for the pilot, as Elite's supercruise assist does (*unverified*) |

**On a surface.** Landing, walking and hiding (`DESIGN.md`, "Surfaces") reuse keys players already
have: Space and C are jump and crouch on foot in Space Engineers, and up and down in its flight,
so on a body they become a hop (held: lift off on the thrusters) and a crouch at no cost to anyone
[B4]. The grip needed one new key.
- **L, for Land or Latch.** It was free (I, K, L, M, N, O, P, U and Y were unbound; M is the chart
  now, N its auto-nav, Y the lock-on, and U the eject), it is pressed
  once per landing rather than in a fight, and Elite puts landing gear on it (*unverified*).
- **Rejected:** P, Space Engineers' landing gear (*unverified*), is the showcase's key; N, Star
  Citizen's landing key (*unverified*), lost to L's mnemonic; T and Enter are throw and dock, and
  the usual chat keys to plan around (item 3 below); Ctrl combinations are out (Ctrl+W closes the
  tab), and so are Alt and the function keys.
- **The grip is a state** on the wire, as flight assist is: a stalled client keeps its suit on the
  body, and a lost packet can't flip it.
- **Crouch is a toggle on the ground**, not a held key. Hiding means lying crouched and still for as
  long as it takes, and holding C for minutes would be a chore. The simulation keeps the stance
  until told otherwise, so a stalled tab stays crouched and hidden. In flight C is still held down.

**Ejecting: U** (`DESIGN.md`, "Doom and ejecting"). Titanfall ejects on its use key, which is our
roll (E). Of the keys still free (I, K, O, U), U is the one the left hand reaches from W without
leaving the keyboard's left half. Steel Battalion hid its eject button under a plastic flap so that
nobody pressed it by mistake (`PEERS.md`, [P20]); ours is a hold of a second, except in the 3 s of a doom, when a
tap ejects at once and a hold blows the suit up instead.

## Where we break convention, and what it costs

1. **V is flight assist.** V switches the camera in Space Engineers and Everspace 2, and (*unverified*)
   GTA V and PUBG. Flight assist or dampeners sit on Z in Elite and Space Engineers
   [B1][B3][B4][C4]. A player who presses V to change view turns flight assist off, which in
   Newtonian flight is serious.
   - **Done:** the camera is on Tab and the wheel, a first-flight hint teaches it early, and
     toggling flight assist announces itself mid-screen.
   - **Still open:** consider making flight assist hold-to-toggle (half a second on V).
2. **Z is the ZERO System.** Z toggles flight assist in Elite, dampeners in Space Engineers and
   look-around in Star Citizen. Our mnemonic is worth keeping, but it will surprise sim players.
3. **T is throw and Enter is dock.** T is "target ahead" in Elite and Star Wars: Squadrons [B5], and
   T and Enter are the usual chat keys. An MMO will want chat: plan the keys before chat lands.
4. **The lock moves you; it doesn't aim for you.** Most Gundam games lock on: Extreme Vs., Gundam
   Breaker, SD Gundam Battle Alliance, even the cockpit arcade game Senjou no Kizuna [A1]. Lock-on is
   also the most common complaint in mech games: too strong, unreliable, or grabbing the wrong target
   (AC6, Mecha BREAK, Daemon X Machina, Gundam Breaker 4, SD Gundam Battle Alliance) [A2]. GBO2 has no
   lock-on button, which shows Gundam players will accept free aim [A1].
   - **Done** (`LOCK.md`): Y, or a click of the middle button, locks on, and what it locks is the
     fight's footing, not the guns. Flight assist keeps pace with the target, W closes in and stops
     short, A/D circle it, and the suit settles onto its level as if on the ground; the mouse still
     aims. Against the complaints: the target is the hostile nearest the crosshair, the next is a
     tap away, it's let go by holding the key, and its bracket is always marked.
   - **Why Y.** It was free and sits by T/G/H; Bevy's keys are physical positions, so QWERTZ keeps
     it there. The middle button is a second key for it, held as well as clicked (holding it lets
     go, as holding Y does), so it isn't free for looking around (P1 #7). Y is chat in
     Counter-Strike: chat (`PEERS.md`) takes `/`.
5. **A lead pip.** Elite, Star Citizen and Everspace 2 all give one [B1][B2][B3]. With 4 km/s beams
   at kilometre ranges, expect "unhittable" complaints. **Done:** locked on, a ◆ marks where the
   primary's shot meets the target if it flies on as it is, with the time the shot takes (a
   setting). ZERO's solution is better than a linear pip, since it weighs the target's likely
   maneuvers, and replaces it when ZERO has one.
6. **F1 and F10 are function keys.** On Mac laptops they need Fn, and 60% keyboards have none [C1].
   They need second keys.
7. **Left Ctrl was a hidden "down"**, and holding it with W is Ctrl+W, which closes the browser tab.
   Browsers reserve that shortcut outside fullscreen, and itch.io players report tabs closing on
   them exactly this way [C1]. **Done:** down is C alone.
8. **C means two things.** Held in flight it is down; on a body it toggles a crouch. The flight
   panel always says which (`GROUNDED` or `CROUCHED`), and a first-landing hint names the keys.

## What players will ask for, most important first

**P0: small, and worth doing before more players arrive.**

1. **Raw mouse input.** Today the browser applies the operating system's mouse acceleration:
   winit 0.30.13 calls `requestPointerLock()` with no options. Raw input is what shooter players set
   their aim by; Google's case for it cites professional players' accuracy [C1].
   - The fix is `requestPointerLock({ unadjustedMovement: true })` through a small `#[wasm_bindgen]`
     extern, because web-sys 0.3.105 has no binding for it. Fall back to a plain lock on
     `NotSupportedError`, and keep winit from re-requesting a plain lock.
   - Support: Chrome/Edge on Windows, macOS and ChromeOS (not Linux), Firefox 152+ and Safari 18.4+,
     about 78% of users [C1].
   - Chrome reports `movementX` in device pixels and Firefox in CSS pixels, so the same sensitivity
     feels different between them.
2. **Flashing.** A ZERO seizure dims the whole screen by 30% at random on a 17 Hz clock
   (`zero_vision.wgsl`). Over a bright scene that can exceed the three flashes a second that
   photosensitivity guidelines allow [C5]. **Done:** a "Flashing effects" setting turns it off,
   and starts off when the browser asks for reduced motion.
3. **Off-screen markers** for hostiles, whoever is locking you, and missiles. Examples are Everspace
   2's edge arrows and Elite's compass. Nearly every game in the survey has them, and they matter
   more from the cockpit [B3][B5]. **Done:** chevrons at the edge of the view point at missiles
   tracking you, whoever is locking on to you, and hostiles within 2 km.

**P1: expected of any PC game in this genre.**

4. **Key rebinding.** This is a *basic* accessibility guideline and "one of the best value"
   features; AZERTY players need it [C5].
   - Xbox's guideline also asks to remap Esc, which a browser can't give up, so the menu needs a
     second key [C5].
   - Bevy's `KeyCode` is already a physical key position. `navigator.keyboard.getLayoutMap()`
     (Chromium only) can print the right labels.
   - Keep `BINDINGS`, the hints and this document drawn from one table.
5. **Reading aids instead of a gun lock.** **Done** with the lock-on (`LOCK.md`): its bracket, range
   and closing speed, and the ◆. Designate a target (the one ahead, the nearest hostile,
   whoever is shooting you) for information only: a box, range and closing speed.
   - Colour the reticle when a blade can reach, like Senjou's orange [A1].
   - Keep the lock warnings, which we already have.
6. **Gamepad.** Bevy's gilrs backend is already in the build (the `3d` feature brings it) and reads
   standard-mapped pads in the browser [C2].
   - It needs a default layout, deadzones, per-stick sensitivity and a "press any button" prompt.
   - Decide the aim-assist policy up front. Gundam Evolution gave aim assist to console pads only,
     so a pad on PC got none [A1].
   - HOTAS needs a direct `getGamepads()` path with press-to-bind, because non-standard devices
     arrive raw.
7. **Hold to look around** without moving the aim (the camera looks, the suit doesn't turn).
   - Every space game has it: Alt in Everspace 2 and Space Engineers, Z in Star Citizen, the middle
     mouse button in Elite [B5].
   - Alt is risky in a browser (on its own it can reach the browser's menu), so this said to use
     the middle button. That has gone to the lock-on since (`LOCK.md`: a click locks on, a hold
     lets go), and Z is the ZERO System. Still open: Alt, as Everspace 2 and Space Engineers
     have it, if it can be kept from the browser's menu, or a key of the player's own once
     rebinding (item 4) is in.
8. **A basic lead pip** as an optional training aid. **Done** (item 5 above).

**P2: depth, once the above is in.**

9. **A burst step.** A double-tapped direction that spends propellant and G, like EXVS's step,
   GBO2's double-tap evade, AC6's Quick Boost and Mecha BREAK's Shift evade [A1][A2]. Every
   reference game has one. **Done:** double-tap W/A/S/D/Space/C (a setting turns it off) for 36
   m/s that way in 0.3 s, once every 1.2 s; locked on, along the fight's axes (`DESIGN.md`, "The
   burst step"). A key of its own (for pads, and pilots who hate double taps) is still open.
10. **Mild homing on blade lunges, within a cone** (EXVS, Zone of the Enders). Landing a lunge on a
    target that moves in 6DOF is hard. **Done:** a lunge drives along the aim within 15° of the
    nose (`DESIGN.md`, "Blades lunge").
11. **Comfort.**
    - Optional horizon lines, and optional auto-level while flight assist is on. Overload's
      auto-level is praised; roll adds most to motion sickness (pitch alone 1.95 on a sickness scale,
      pitch and roll 4.33) [B3][C6].
      - **Partly done:** with the grip armed and a surface within 150 m, the suit rolls its feet
        toward it (Q/E override, and the nose stays on the aim). That is auto-level referenced to the
        surface, on the axis that matters most. Auto-level in open flight is still open.
    - A separate cockpit field of view (Starfield added one). A lower minimum of about 50° vertical.
    - Boost's field-of-view widening on its own slider rather than tied to shake.
    - Separate sensitivity for the cockpit, the chase camera and on foot (Hawken Reborn, Battlefield)
      [A2][C3].
12. **Cut to third person for blade kills and the Neo-Bird change** when flying from the cockpit.
    Titanfall 2 does this for Titan executions [A2].
13. **Fullscreen with keyboard lock**, so the game owns Ctrl and Esc. Chromium passes browser
    shortcuts to a fullscreen page, and Firefox 151 added `keyboardLock: "browser"` [C1].
14. **Zoom in the cockpit** for long beam shots, on the wheel past the cockpit. MWO only zooms in
    first person [A2].

## Browser constraints, for whoever builds the above

- **Keys a tab can't own:**
  - Outside fullscreen: Ctrl+W/T/N, Ctrl+Shift+W/N/T and Ctrl+Tab in Chromium, and the same core
    set in Firefox.
  - On a Mac: Cmd+W/Q/T/N.
  - Never bind F5, F11 or F12 [C1].
- **Esc always ends pointer lock.** Chromium keeps that Esc to itself and refuses a new lock for
  1.25 s, and then only after a click, unless the page unlocked itself [C1].
- **Gamepads:**
  - Standard mapping is 17 buttons and 4 axes, with the triggers as analog buttons 6 and 7.
  - A pad only appears after a button press.
  - Pads map differently in Chrome and Firefox.
  - Joysticks and HOTAS arrive non-standard, capped at 16 axes and 32 buttons in Chromium [C2].
- **Field of view.** Bevy's is vertical; most games quote horizontal. Our 60–100° slider is about
  92–130° horizontal at 16:9, which is already as wide as players expect (Overwatch allows 80–103°
  horizontal). The settings panel now gives both figures [C3].

## Sources

The three surveys behind this document were run in September 2026. The main sources for each claim:

- **[A1] Gundam games.**
  - GBO2: the official manual (bo2.ggame.jp/en/images/manual/ps5/03.jpg, 05.jpg) and Siliconera on
    its space battles.
  - EXVS MBON: Dustloop's controls page.
  - Gundam Evolution: Shacknews on its closure; Game*Spark on its aim assist.
  - Senjou no Kizuna: Game Watch (game.watch.impress.co.jp/docs/20061201/pod.htm); HUDS+GUIS on
    Senjou II.
- **[A2] Mech games.**
  - MechWarrior Online: Gaming Nexus, GDC 2012; its Steam discussions (F3 third person, formerly
    F4); Engadget (2013) on adding third person.
  - Armored Core VI: Gfinity; its Steam discussions.
  - Mecha BREAK: Prima's controls guide; its Steam discussions.
  - Hawken Reborn: 505 Games' patch notes.
  - Titanfall 2: titanfall.wiki.gg on terminations.
- **[A3]** Wikipedia, "Mobile suit".
- **[B1] Elite Dangerous.**
  - Default keyboard binds: the shipped `KeyboardMouseOnly.binds`, mirrored at
    github.com/brammmers/edrefcard2.
  - elite-dangerous.fandom.com on flight assist, the flight model, the HUD and weapons.
  - Frontier forum threads asking for a velocity vector.
- **[B2] Star Citizen.** starcitizen.tools (Guide:Controls, Keybinds, Game_Options: F4 camera,
  Left Alt+C decoupled, X space brake); the 3.20 Master Modes notes.
- **[B3] Everspace 2 and 6DOF shooters.** everspace.fandom.com (controls, HUD); Overload's Steam
  discussions on auto-level; Wikipedia on Descent.
- **[B4] Space Engineers and Kerbal Space Program.** spaceengineers.wiki.gg (Controls, Key Bindings:
  V camera, Z dampeners, Space/C, Q/E); the KSP wiki (SAS, RCS).
- **[B5] Target and look keys.** EA's Star Wars: Squadrons manual (T, F, G, H); the Elite binds
  above.
- **[C1] Browsers.**
  - MDN on `requestPointerLock` and the Pointer Lock API; web.dev "Disable mouse acceleration";
    caniuse on `unadjustedMovement`.
  - Firefox 151 and 152 release notes; Safari 18.4 release notes.
  - Chromium's `pointer_lock_controller.cc` and `browser_command_controller.cc`.
  - The WICG keyboard-lock spec; itch.io reports of Ctrl+W.
  - winit 0.30.13's `platform_impl/web/web_sys/canvas.rs`.
- **[C2] Gamepads.** The W3C Gamepad spec; MDN "Using the Gamepad API"; Chromium's `gamepad.h`;
  the sources of `gilrs-core` 0.6.8 and `bevy_gilrs` 0.19.1.
- **[C3] Field of view and sensitivity.** Bevy's `PerspectiveProjection` (vertical); minecraft.wiki
  Options; esportstales on Overwatch's FOV; PureXbox on Starfield 1.7.36.
- **[C4] Camera keys.** Space Engineers V and Star Citizen F4 were verified against the wikis above.
  GTA V V, War Thunder V, Battlefield V's C, and Skyrim's and Starfield's wheel are *unverified*.
- **[C5] Accessibility.** gameaccessibilityguidelines.com (remapping, field of view, the full list);
  Xbox Accessibility Guidelines 107 (remapping) and 117 (camera and sensitivity).
- **[C6] Motion sickness.** PubMed 22097636 (rotation axes) and 18516842 (incidence); Purdue's
  "virtual nose" study; Meta's locomotion comfort guidance.
