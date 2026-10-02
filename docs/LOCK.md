# Lock-on: fighting on the ground in space

The owner's call: a lock-on that puts the fight on a plane, as if on the ground, so that a fight in
six degrees of freedom reads like one on foot and aiming gets easier, without taking the aim away
from the pilot (`PEERS.md`, Battle Operation 2's Derelict Colony: close, ground-like fighting is
where players learn to hit). This is the design, and what's built of it.

## What it has to be

- **The lock moves you; it doesn't aim for you.** Shots go where the pilot aims, as ever (pillar 2,
  `DESIGN.md`). The lock gives the suit a floor to fight on, keeps station on the target, closes in
  and circles it, and marks where to lead.
- **Prediction stays exact.** Everything the lock decides is in the command: the server and the
  owner's prediction fly the same numbers (`ARCHITECTURE.md`).
- **Nothing new on the hot path.** The tick stays allocation-free and lock-free, and every
  determinism golden that doesn't lock on stays where it was.
- **The same for everyone.** Agents can lock on through the same command; Mobile Dolls never do (a
  perfect lead with perfect pace would be unbeatable).

## The feel

| Key | Locked on, flight assist on |
|---|---|
| **Y**, or a middle-mouse click | lock the hostile nearest the crosshair (within 10°, else the nearest on screen, else the nearest within 4 km); again: the next one; hold for a third of a second: let go |
| W | close in, braking in time to stop just outside your blade's reach (15 m without one) |
| S | back off |
| A / D | circle it, at the range you're at |
| neither W nor S | hold the range |
| Space / C | rise off the fight's floor, or drop below it; let go and you settle back onto it |
| X | brake to the target's velocity (not the sector's rest) |
| Mouse | aim, as ever; the ◆ marks where to lead |

- **The floor.** With Space and C let go, the suit settles onto the target's level: the vertical
  axis is locked, so the target sits at the height of your crosshair and aiming is mostly side to
  side. Its level is its altitude over the colony (`bc_sim::world::colony_altitude`): the colony is
  curved, so two suits at the same height kilometres apart round it are on the same floor.
- **Holding station.** Flight assist holds your velocity relative to the target's, so "stopped"
  means keeping pace with it, whatever it's doing (up to your frame's boosted cruise).
- **Level with the ground.** The suit rolls its feet to the fight's ground, so the mouse turns as it
  would on foot. Q/E still roll you off it.
- **The ground is away from the colony** (`bc_sim::world::colony_up`): over the hull, away from the
  axis; past an end cap, away from the cap. Both pilots in a fight share it. A target standing on a
  body is fought with that body's ground instead, and there's no floor to settle onto (its ground is
  in the way).
- **When it lets go.** The target downed, out of sight (off sensors, jamming, hidden) for a second,
  past 5 km, or your own suit gone.
- **When it stands aside** (the target stays locked and marked, the keys fly as they always do):
  flight assist off, on a body or coming down onto one with the grip armed, ZERO flying the suit, and
  Neo-Bird (it can't circle).

## How it works

**On the wire** (`bc_proto::LockOn`, protocol v13). A command locked on carries the velocity flight
assist holds the suit's relative to (the target's, as the pilot sees it: 3 × 14 bits on the centred
grid, so a target at rest is exactly at rest) and the fight's up (10-bit octahedral), behind a
1-bit flag: 62 bits more a command, and four commands still fit 96 bytes. A silent client's suit
stays locked on for 30 ticks after it goes hands-off, so a stall keeps station rather than braking
hard to the sector's rest.

**In the simulation** (`bc_sim::ground::lockon_assist`, `bc_sim::flight::LockOnAssist`). A free
suit's flight assist holds `ref_vel + (right·x + up·y + fwd·z)·cruise` instead of `stick·cruise`,
where `up` is the fight's, `fwd` the aim laid flat on its ground and `right` square to both; and the
suit rolls level with `up` (unless the pilot rolls, or the grip is levelling it to a surface). The
reference is capped at the frame's boosted cruise. That's all: the command's own numbers and the
suit's own state, so prediction is exact, and nothing is checked against the target (all a lock-on
asks for is a velocity flight assist could hold anyway).

**In the client** (`bc_client_core::lockon`, shared with agents). Each tick the keys are reshaped:
- the target is sampled where it's drawn and carried on at its velocity to where the own suit is
  predicted;
- **W** closes at `min(glide(d − stop, a), cruise)`, where `glide(e, a) = min(|e|/2τ, √(2a|e|) − aτ)`
  is the stopping curve through flight assist's lag (τ = 0.2 s) and `a` is what the suit's retro
  thrusters can brake with (at most 3 g): a Leo plans on 16.5 m/s², so it never flies past;
- idle, it holds the range with the same curve at 6 m/s²;
- **A/D** circle no faster than the side thrusters can turn the circle and the suit can turn to
  keep facing it, leaning in by `w²τ/d` for flight assist's lag so the circle doesn't widen;
- with **Space/C** let go it settles onto the floor at `glide(h, 6)`, at most half the cruise,
  where `h` is the target's altitude over the colony less the suit's;
- the wanted velocity is then put in the simulation's own axes (worked out from the quantized
  command, so it's read back exactly as meant) and scaled onto the stick.

**On the HUD.** The locked target's bracket, name, range and closing speed; the ◆ lead marker for the
primary weapon (`bc_sim::zero::fire_control::intercept`, the target's velocity as drawn, the shot's
speed on top of the suit's; a beam rifle's charged shot's once the charge is full), hidden while ZERO has a solution (which weighs the target's maneuvers);
and SPD and the velocity marker relative to the target.

## Balance

- A locked suit keeps pace with anything up to its boosted cruise, so running away is by
  acceleration, by range (past 5 km), by the jammer, or by getting out of sight in a hide spot.
- Closing to blade range is easy now; the target is warned (LOCK WARNING, as for any designation),
  and the burst step is the answer to a lunge: double-tapped, it goes along the fight's axes, so
  A and D sidestep round the attacker and S jumps back (`DESIGN.md`, "The burst step").
- Missile locks still need the target within 20° of the aim: the lock doesn't point the launchers.

## Numbers (all tunable)

| | |
|---|---|
| Pick | 10° of the crosshair, else 60°, else the nearest within 4 km |
| Breaks | past 5 km, or 30 ticks unheard |
| Release | key held 0.35 s |
| Stop | 0.7 × blade reach + 5 m (Leo 11.3 m), 15 m without a blade |
| Braking planned | 80% of retro thrust, at most 30 m/s² |
| Range hold, floor settle | 6 m/s², settling at most half the cruise |
| Reference cap | the frame's cruise × 1.8 |
| Kept by a silent client | 30 ticks past hands-off |

## Verification

- `bc-proto`: round trips, sizes, a still target stays still, a silent client's lock-on.
- `bc-sim`: plain flight assist when the lock-on is the suit's own axes with nothing to hold to;
  keeping pace with a target faster than cruise; braking to it; roll-level; ignored on a body;
  `colony_up`'s geometry; `LOCKON_GOLDEN` native and wasm; `locked_on_pilots_never_allocate`.
- `bc-client-core/tests/lockon_predict.rs`: against the real simulation, a pilot locks a target
  coasting away at 260 m/s (faster than a Leo's cruise) 250 m above it, closes to the stop on its
  level keeping pace with it, rolled level, circles it within 5% of the range, and the prediction
  stays within a centimetre of the server throughout.
- A target crossing the nose takes longer to pace than one ahead: matching its speed across falls
  to the side thrusters (a Leo's take about half a minute to match 260 m/s). That's the flight
  model, not the lock.
