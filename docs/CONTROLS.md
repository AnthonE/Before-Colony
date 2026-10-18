# Controls: what players will expect

A survey (September 2026) of the games Before Colony's players will measure it against: anime-mech and
mech games, 6DOF space sims, and PC shooters in the browser. It sets our scheme against theirs, says
where we break convention on purpose and what that costs, and lists what players will ask for, most
important first. `DESIGN.md` has the controls as they are.

Keys marked *unverified* come from secondary sources only. The sources are listed at the end.

## Is anything like it?

Nothing we found combines Newtonian 6DOF flight, mouse free aim, suits and an MMO [A]. The
nearest games each share only part of it:

- **The licensed 6v6 anime-mech shooter** (2018–) has anime suits and manual aim. Its remote
  weapons lock after you hold the reticle on a target, which is our missile lock. But you stay upright, and space
  only adds "ascend and descend" to the ground controls [A1].
- **Star Citizen** is a Newtonian MMO, with coupled/decoupled flight, G protection and lead pips. It
  flies ships, not limbed suits [A2][B2].
- **Elite Dangerous** and **Space Engineers** have flight-assist and dampener toggles and
  zero-G movement.
- No licensed anime-mech game has flown Newtonian 6DOF, the franchise's two MMOs included [A1].

So players arrive with three sets of habits that disagree with each other:

- **Anime-mech action players** expect lock-on and target switching, a dodge step, homing melee, and a
  controller.
- **Space-sim players** expect a flight-assist toggle, lead pips, a velocity vector, radar, and
  HOTAS.
- **Mech-sim players** expect a cockpit view and a second reticle for the arms, and bring the
  first-person against third-person argument with them.

We can't meet all three by default. The pillars choose for us: free aim with no gun lock, and
Newtonian flight. The cost is paid in reading aids and onboarding.

`PEERS.md` looks at the nearest games beyond their controls: what each has of the whole fantasy,
and what we take from it.

## Shooter controls: the default

The owner's call (October 2026): flying a suit from the cockpit should feel close to playing
Overwatch in first person, and the 6DOF handling was too much of a simulator to start with. So the
default is a shooter's (`DESIGN.md`, "Shooter controls"; `bc_client_core::upright`), and the
simulator's is a setting away (Settings, CONTROLS: flight controls). The simulation under them is
the same: only what the keys and the mouse ask of it changes, and it travels in the command, so it's
the same for the server, the prediction and agents.

| Shooter controls | Simulator controls | Why |
|---|---|---|
| The view stays upright, levelling out onto the ground's up (the colony's, the lock-on's ground) | The view rolls with the suit | Every first-person shooter; Overload's auto-level is the 6DOF game's most praised comfort option, and roll adds most to motion sickness [B3][C6] |
| The mouse turns about that up, never past 89° of the horizon | About the suit's up, which pitches with it: it loops | A shooter's yaw and pitch, stopping where Overwatch does (89°, *unverified*) |
| W along the crosshair, A/D square to it along the ground, Space/C (or Ctrl) along the up | Along the suit's own axes, which lag the aim | Spectator and noclip cameras, and Descent's and Overload's 6DOF, fly where you look [B3] |
| A diagonal no faster than straight | A diagonal is the axes' sum | A shooter's movement |
| The RCS fires whenever the aim is off the nose | R, held | Drones and agents always fly with it; without it a pilot turned at two fifths of their rate |
| V held half a second for flight assist | V pressed | V is Overwatch's melee key (*unverified*; ours is F): a tap mustn't throw a shooter's player into Newtonian flight |
| Q/E roll while held, and it levels out again | Q/E roll, and it stays | An upright view; a rolled one is a held key away |

What it costs: no loops or inverted flight on the mouse alone (Q/E, or the simulator's controls),
and the thrusters still reach a velocity axis by axis. The arms reach 50° off the nose, so after a
flick past that the crosshair dims while the suit comes round.

**The anime rules' handling** (the owner's second call, October 2026: "too slow to turn", and the
suit "floats" when the mouse moves fast; W must go where the camera points). The shooter controls
asked for the right thing, but the suit took a second and a half to come round a quarter turn and
pushed sideways at half its forward thrust, so after a flick it slid on the way it had been going
for seconds. Under the anime rules (the default) suits now turn three times as fast and push every
way as hard as ahead, half as hard again as their frame's own (`DESIGN.md`, "Flight rules"), and
the simulation stays one for both control schemes and for agents. The real rules keep the
simulator's handling.

| Key | Now | Before |
|---|---|---|
| Mouse, flicked | an Argo comes round a quarter turn in under half a second | about a second and a half |
| W after a quarter turn at 100 m/s | an Argo flies where the crosshair is within two seconds (an Altair, one) | nearly five: it pushed sideways at 1.75 g |
| Space | always up: double-tapped inside the colony too, a burst step (the colony's fall and hover are L's) | double-tapped inside, it dropped the suit |
| Shift | overdrives every way's thrusters, so it speeds whichever way is held | the main thrusters only |
| Ctrl | down, as C is (crouches on the ground) | unbound: Ctrl+W closes the tab |
| X | brakes as hard as the suit can: at once, with boost's overdrive, no G guard (an Argo stops from its cruise in 2.6 s) | eased off as flight assist does, retro thrust only (10.5 s) |

## The cockpit view

The cockpit is the default view: the pilot sits in the suit. Tab, or the mouse wheel (in for the
cockpit, out to chase), switches between the chase camera and the cockpit. K switches the cockpit
between its framed monitors (the default) and the holo cockpit, which turns the frame see-through
for a view all round, and holding Alt looks round from the seat without moving the aim. The choices
are settings, so the next sortie starts in the same view.

- **Why Tab.** V is the usual camera key, but V is our flight assist. MechWarrior Online and Star
  Citizen use F3 and F4 [A2][B2], but function keys need Fn on Mac laptops and 60% keyboards have
  none [C1]. Tab is free, reachable from WASD and in the same place on every layout, and there's no
  tab-targeting in this game for it to be mistaken for. Scrolling in to go first person follows
  games whose third-person camera zooms on the wheel (Skyrim and Starfield, *unverified*) [C4].
- **Where the eye sits.** The camera is the head's main camera, whose picture is what an anime
  mech's cockpit monitors show. The anime have it both ways: the cockpit is in the torso and the
  head carries the sensors [A3]. Aquila looks out from over its canopy, along its nose.
- **Where it looks.** Along the aim, as the chase camera does, and the suit turns after it (under
  shooter controls, upright to the ground's up, the mouse turning it as on foot). This is
  War Thunder's mouse-aim model and Star Citizen's gimballed-guns-follow-the-cursor model. It is not
  MechWarrior Online's, where the view is locked to the torso and a second reticle floats over it
  [A2]. Our suits turn to the aim by themselves, so a torso-locked view would send the crosshair
  wandering across the screen while the body caught up.
- **Two reticles, the other way round.** MWO draws a fixed torso reticle and a floating arm
  reticle; Star Citizen added boresight symbols [B2]. Ours: the crosshair is the aim, and the guns
  converge on what's under it (`bc_sim::aim`): the shot goes where the crosshair is, however far
  the gun is from the eye. While a weapon can't bear, because hands reach only 50° off the body's
  axis and Aquila's nose 2°, the crosshair dims and `( )` marks where the primary weapon would fire.
- **The velocity vector.** `-o-` shows where the suit is drifting (`-x-` when moving backwards).
  Elite's flight-assist-off pilots keep asking for exactly this [B1].
- **Your own suit.** Everything but what's at the eye (the head it's inside, a high collar:
  `bc_model::near_eye_bones`) stays drawn, so blade strokes, the Talon and a shoulder coming round
  are all visible. MWO's cockpit shows arms and legs [A2]. A test checks that no frame's own armour
  sits inside the camera's near plane or across the crosshair's central 15°, and another that the
  arms, legs and gun are never left out.
- **Damage.** With the head shot off, the picture comes from the sub-camera: greyer, fringed, and
  marked `SUB-CAM` on the armour readout. The licensed cockpit-pod arcade game's cockpit glitches on hits [A1].
- **Chase camera for wrecks.** A wreck is always watched from the chase camera.
- **First person is the default.** The pilot sits in the suit: that's the fantasy, and the crosshair
  is exact from the eye. MWO added third person over fan protest because new players couldn't read
  how the mechs moved, and Hawken Reborn made it the default [A2], so the chase camera stays a key
  away (Tab), drawing the crosshair on the point the guns converge on. Xbox's accessibility
  guidelines ask that players be able to choose between first and third person [C5]. A settings
  file from before the change (version 1) starts in the cockpit once.

## Where we match convention

| Ours | The same elsewhere |
|---|---|
| W/S forward/back, A/D strafe | Star Citizen, Everspace 2, Warframe Archwing [B2][B3] |
| Space / C up/down (and Ctrl down) | Space Engineers, key for key [B4]; Ctrl is a shooter's crouch |
| On a body: Space hops (held, lifts off), C crouches | Space Engineers' jump and crouch on foot, the same keys [B4] |
| L arms the grip to land, and clears it to let go | Elite's landing gear (*unverified*) [B1] |
| Q/E roll | Star Citizen, Everspace 2, Space Engineers [B2][B3][B4] |
| Shift boost (whichever way) · X brake (as hard as the suit can) | Star Citizen (X is its space brake) [B2] |
| R RCS | Kerbal Space Program (R toggles RCS) [B4] |
| LMB/RMB weapons | Mecha BREAK [A2] |
| Esc menu | The browser's own exit from pointer lock [C1] |
| Hold the reticle on a target to lock missiles | The 6v6 shooter's remote weapons [A1] |
| M the chart (a 3D map) · N its auto-nav | Elite's galaxy and system maps and nearly every PC game with one; dragging to turn a 3D map and the wheel to zoom it, as in Elite's galaxy map; `◆` waypoints as in Everspace 2; flying a picked destination for the pilot, as Elite's supercruise assist does (*unverified*) |

**On a surface.** Landing, walking and hiding (`DESIGN.md`, "Surfaces") reuse keys players already
have: Space and C are jump and crouch on foot in Space Engineers, and up and down in its flight,
so on a body they become a hop (held: lift off on the thrusters) and a crouch at no cost to anyone
[B4]. The grip needed one new key.
- **L, for Land or Latch.** It was free (I, K, L, M, N, O, P, U and Y were unbound; M is the chart
  now, N its auto-nav, Y the lock-on, U the eject, and O F10's second key), it is pressed
  once per landing rather than in a fight, and Elite puts landing gear on it (*unverified*).
- **Rejected:** P, Space Engineers' landing gear (*unverified*), is the showcase's key; N, Star
  Citizen's landing key (*unverified*), lost to L's mnemonic; T is throw and Enter docks, and
  they're the usual chat keys (item 3 below); Ctrl combinations are out (Ctrl+W closes the
  tab), and so are Alt and the function keys.
- **The grip is a state** on the wire, as flight assist is: a stalled client keeps its suit on the
  body, and a lost packet can't flip it.
- **Crouch is a toggle on the ground**, not a held key. Hiding means lying crouched and still for as
  long as it takes, and holding C for minutes would be a chore. The simulation keeps the stance
  until told otherwise, so a stalled tab stays crouched and hidden. In flight C is still held down.

**The Markab** (`DESIGN.md`, "The Markab"), the rocket car, drives up walls and flies, and the
game players will hold it against is Rocket League, whose cars drive its walls and ceiling, boost,
leap and roll in the air (`PEERS.md`, "Cars that drive on walls"). Its keys are Rocket League's
where ours already agree, and our own flight's where they don't:
- **W/S and A/D drive, and in the air pitch and yaw.** Rocket League's air steering, key for key
  [D1]: the nose goes down on W, as a stick pushed forward.
- **Space the lift jets, Shift the tail's, Q/E the roll.** Rocket League jumps on the right mouse
  button and boosts on the left, and rolls on Shift held with A/D (*unverified*) [D1]. Ours keep
  the flight's: Space rises and Shift boosts in a suit, and Q/E roll it, so a pilot's hands know
  them already; Q/E are Rocket League's own "air roll left/right" too. The car's handbrake moved
  to X, the flight's brake.
- **F the fan.** A mnemonic, free while driving, pressed once (a toggle, as the grip is), never in
  a hurry. On from the pool: a player's first wall should climb.
- **R at a pool.** Q and E were taken (the scooter, the car); R is beside them.
- **The camera turns with the face.** Up a wall the chase camera's up is the wall's, as Rocket
  League's car camera turns with its walls; in the air it stays upright to the colony, and the
  view never rolls with a barrel roll (rolling views are what make people sick: the shooter
  controls' reason, above).

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
     toggling flight assist announces itself mid-screen. Under shooter controls (the default) V is
     held half a second to toggle it, and a tap only says so: V is a shooter's melee key.
   - **Still open:** the same hold for the simulator's controls.
2. **Z is the Orrery.** Z toggles flight assist in Elite, dampeners in Space Engineers and
   look-around in Star Citizen. Our mnemonic is worth keeping, but it will surprise sim players.
3. **T is throw and Enter is dock.** T is "target ahead" in Elite and Star Wars: Squadrons [B5], and
   T and Enter are the usual chat keys.
   - **Done:** the colony's radio opens on `/`, and on Enter wherever Enter has nothing to dock:
     on foot, and in a suit away from where it docks (always, under arcade rules). Enter asks the
     dock only where the HUD's marker says ENTER: within twice the ring's radius of the dock (the
     hub's mouth and its deck hatch with it), of the inner gate, or of a trainer's gantry pad
     (`bc_client_core::dock`), so the dock's refusals come only where they teach something. One
     press goes one place, decided once a frame. On the chart Enter still sets a course.
   - **Still open:** T, the other usual chat key, stays throw.
4. **The lock moves you; it doesn't aim for you.** Most licensed anime-mech games lock on: the 2v2
   arena brawlers, the kit-building brawlers, even the cockpit-pod arcade game [A1]. Lock-on is
   also the most common complaint in mech games: too strong, unreliable, or grabbing the wrong target
   (AC6, Mecha BREAK, Daemon X Machina, the kit-building brawlers) [A2]. The 6v6 shooter has no
   lock-on button, which shows anime-mech players will accept free aim [A1].
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
   setting). The Orrery's solution is better than a linear pip, since it weighs the target's likely
   maneuvers, and replaces it when the Orrery has one.
6. **F1 and F10 are function keys.** On Mac laptops they need Fn, and 60% keyboards have none [C1].
   - **Done:** `?` is a second F1 and O a second F10, and the function keys still work. The
     controls sheet lists both, and so do the prompts that name F1.
   - **Why `?`.** It is the web's key for a page's list of shortcuts: GitHub and Gmail both show
     theirs on it [C7]. The page reads it by the character, so it is wherever the layout prints it,
     and the game never sees the key under it. On US and UK layouts that key is `/`, the radio:
     with Shift held to boost or run, `/` comes out as `?`, and the radio opens once Shift is let
     go. AZERTY puts `?` on the chart's key (M's place), with the same cost. Typed into the radio's
     line, a callsign or a terminal's field, it is only a question mark.
   - **Why O.** None of the games in the survey puts graphics quality on a key, so there was no
     habit to follow. Of the letters still free (I, K and O), O is for options, and it is out of
     reach of the left hand on W A S D, so a fight never presses it by mistake. While the radio's
     line is open it is a letter, not a key.
7. **Ctrl is down, and Ctrl+W closes the tab.** Holding Ctrl with W is Ctrl+W, which closes the
   browser tab. Browsers reserve that shortcut outside fullscreen, and itch.io players report tabs
   closing on them exactly this way [C1]. Down was C alone for that; the owner put Ctrl back
   (October 2026), since a shooter's players reach for it.
   - **Done:** in a sortie the page asks the browser to confirm leaving (`beforeunload`), so Ctrl+W
     (and Ctrl+Shift+W, the window) asks "Leave site?" instead of closing; the cost is a click and
     the pointer's lock. Most of Ctrl's combinations with the game's keys (Ctrl+A, D, S, F, R, H)
     are the page's to keep, and the game keeps them (Bevy's `prevent_default_event_handling`).
   - **Still open:** the browser keeps the rest outside fullscreen: Ctrl+Tab (the camera key)
     and Ctrl+1 to 9 (the rack's keys) switch tabs, and Ctrl+T (throw) and Ctrl+N (the auto-nav)
     open one; sinking while pressing them leaves the game's tab. Fullscreen with keyboard lock
     (P2 #13) would give the game all of them. On Windows, Ctrl+Shift switches keyboard layouts
     where more than one is installed: sinking on boost can do that, and nothing in a page can
     stop it.
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
2. **Flashing.** An Orrery seizure dims the whole screen by 30% at random on a 17 Hz clock
   (`orrery_vision.wgsl`). Over a bright scene that can exceed the three flashes a second that
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
   - Colour the reticle when a blade can reach, like the cockpit-pod game's orange [A1].
   - Keep the lock warnings, which we already have.
6. **Gamepad.** Bevy's gilrs backend is already in the build (the `3d` feature brings it) and reads
   standard-mapped pads in the browser [C2].
   - It needs a default layout, deadzones, per-stick sensitivity and a "press any button" prompt.
   - Decide the aim-assist policy up front. The licensed hero shooter (2022–2023) gave aim assist to console pads only,
     so a pad on PC got none [A1].
   - HOTAS needs a direct `getGamepads()` path with press-to-bind, because non-standard devices
     arrive raw.
7. **Hold to look around** without moving the aim (the camera looks, the suit doesn't turn).
   - Every space game has it: Alt in Everspace 2 and Space Engineers, Z in Star Citizen, the middle
     mouse button in Elite [B5].
   - Alt is risky in a browser (on its own it can reach the browser's menu), so this said to use
     the middle button. That has gone to the lock-on since (`LOCK.md`: a click locks on, a hold
     lets go), and Z is the Orrery. Still open: Alt, as Everspace 2 and Space Engineers
     have it, if it can be kept from the browser's menu, or a key of the player's own once
     rebinding (item 4) is in.
8. **A basic lead pip** as an optional training aid. **Done** (item 5 above).

**P2: depth, once the above is in.**

9. **A burst step.** A double-tapped direction that spends propellant and G, like the 2v2 brawlers' step,
   the 6v6 shooter's double-tap evade, AC6's Quick Boost and Mecha BREAK's Shift evade [A1][A2]. Every
   reference game has one. **Done:** double-tap W/A/S/D/Space/C (a setting turns it off) for 36
   m/s that way in 0.3 s, once every 1.2 s; locked on, along the fight's axes (`DESIGN.md`, "The
   burst step"). A key of its own (for pads, and pilots who hate double taps) is still open.
10. **Mild homing on blade lunges, within a cone** (the 2v2 brawlers, Zone of the Enders). Landing a lunge on a
    target that moves in 6DOF is hard. **Done:** a lunge drives along the aim within 15° of the
    nose (`DESIGN.md`, "Blades lunge").
11. **Comfort.**
    - Optional horizon lines, and optional auto-level while flight assist is on. Overload's
      auto-level is praised; roll adds most to motion sickness (pitch alone 1.95 on a sickness scale,
      pitch and roll 4.33) [B3][C6].
      - **Done:** with the grip armed and a surface within 150 m, the suit rolls its feet
        toward it (Q/E override, and the nose stays on the aim). That is auto-level referenced to the
        surface, on the axis that matters most. In open flight, shooter controls (the default)
        level the view and the suit onto the colony's up whenever Q/E are let go.
    - A separate cockpit field of view (Starfield added one). A lower minimum of about 50° vertical.
    - Boost's field-of-view widening on its own slider rather than tied to shake.
    - Separate sensitivity for the cockpit, the chase camera and on foot (Hawken Reborn, Battlefield)
      [A2][C3].
12. **Cut to third person for blade kills and the Aquila change** when flying from the cockpit.
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

- **[A1] Licensed anime-mech games.**
  - The 6v6 shooter: the official manual (bo2.ggame.jp/en/images/manual/ps5/03.jpg, 05.jpg) and
    Siliconera on its space battles.
  - The 2v2 arena brawler: Dustloop's controls page.
  - The hero shooter: Shacknews on its closure; Game*Spark on its aim assist.
  - The cockpit-pod arcade game: Game Watch (game.watch.impress.co.jp/docs/20061201/pod.htm);
    HUDS+GUIS on its sequel.
- **[A2] Mech games.**
  - MechWarrior Online: Gaming Nexus, GDC 2012; its Steam discussions (F3 third person, formerly
    F4); Engadget (2013) on adding third person.
  - Armored Core VI: Gfinity; its Steam discussions.
  - Mecha BREAK: Prima's controls guide; its Steam discussions.
  - Hawken Reborn: 505 Games' patch notes.
  - Titanfall 2: titanfall.wiki.gg on terminations.
- **[A3]** Wikipedia, on the licensed franchise's suits.
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
- **[D1] Rocket League.** Mejoress's keyboard guide (April 2024: right mouse jump, left mouse boost,
  Left Shift powerslide and air roll, W/A/S/D throttle, steering and air pitch and yaw), *unverified*
  against the game's own Controls menu.
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
- **[C7] Help keys** (checked October 2026). GitHub Docs, "Keyboard shortcuts" ("Typing ? on GitHub
  brings up a dialog box that lists the keyboard shortcuts"); Gmail Help, "Keyboard shortcuts for
  Gmail" (support.google.com/mail/answer/6594: "press the question key (Shift + slash)").
