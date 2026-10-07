# Before Colony wire protocol (v25)

Everything is little-endian and bit-packed LSB-first (`bc_proto::bits`). Datagrams are one QUIC
datagram each, at most `min(1100, connection max)` bytes, and never fragmented. The first 4 bits
of every datagram give the packet kind: `1` = input, `2` = snapshot.

v25 (from v24): specials charged by the fight (`docs/DESIGN.md`, "Specials charged by the
fight"): the own state's special cooldown (8 bits, ticks ÷ 4) is now the special's charge (8 bits,
in 255ths: 255 charged), since the fight takes time off it.

v24 (from v23): stagger (`docs/DESIGN.md`, "Stagger"): the events' second extension (the
extension's sub-kind 7 is now a 4-bit sub-kind of its own), whose sub-kind 0 is `Staggered`; in
the own state, after the doom, the suit's impact (3 bits, in sixths of what it stands), the
stagger's ticks left (5) and the designated target's impact (3 bits, 7 while it's staggered); and
the doom in steps of 3 ticks (5 bits) rather than ticks (7). The own state grows by 9 bits, and a
datagram still carries 37 free suits or 40 riders.

v23 (from v22): doom and ejecting (`docs/DESIGN.md`, "Doom and ejecting"): the extension's
sub-kinds 4 (`Doomed`), 5 (`Eject`) and 6 (`Blast`); the doom's ticks in the own state (7 bits,
after the cover); `WeaponKind` 20, the reactor's blast; and the hangar frame `eject`, which any
rules take.

v22 (from v21): a suit's weathering on the roster: how worn its paint is, 0 to 7, from the life it
has had (`bc_econ::weathering`), in the Roster flags' bits 2-4, for every client to draw.

v21 (from v20): the Blast Hall, the Proving Ground off Hub Gate's square: a key place with a room
at a mobile suit's scale behind 40 m blast doors (new solids for everyone)
(`content::city::CITY_VERSION` 4); its live fire, the colony's law's one exception; and the event
for a training round scoring on one of its targets (the extension's sub-kind 3, `TargetHit`).

v20 (from v19): the city's massing (crowns, setbacks, masts, roof plant: new solids for everyone)
and its street furniture and trees (lamp posts, trunks and benches, solid to people and cars, not
to suits) (`content::city::CITY_VERSION` 3). Nothing changes on the wire.

v19 (from v18): suits standing on the colony's city, and walking it: the body reference's kind 2
(`City`, no id), its riders placed over ±16 384 m (below).

v18 (from v17): spectator snapshots, for pilots on foot in the colony's city watching the suits
inside it (the header's SPECTATOR flag); the colony's inside keeping the outside's tick; plaza
datagrams for pilots flying inside; and the key places' rooms in the city
(`content::city::CITY_VERSION` 2).

## Quantization

| Quantity | Encoding |
|---|---|
| Entity position | 3 × 21 bits over ±32 768 m (3.1 cm steps) |
| Entity velocity | 3 × 14 bits over ±2 048 m/s (0.25 m/s) |
| Entity rotation | smallest-three: 2-bit index + 3 × 10 bits |
| Rider position (in its body's frame) | 3 × 15 bits over ±256 m on a rock, 3 × 17 bits over ±1 024 m on a landmark, 3 × 21 bits over ±16 384 m on the colony's city (1.5625 cm steps on each) |
| Rider velocity (over its body) | 3 × 10 bits, two's complement, in steps of 32/511 m/s (6.26 cm/s; zero is exact) |
| Own position, velocity, propellant, G-strain | raw `f32` (lossless: the client re-simulates from them) |
| Own rotation | smallest-three at 16 bits per component |
| Aim (input) | octahedral 2 × 16 bits (≈0.005°) |
| Probabilities | 7 bits |

## Client → server: `InputPacket`

| Field | Bits |
|---|---|
| kind (=1) | 4 |
| ack_snapshot (newest snapshot tick received) | 32 |
| client_time_ms (echoed for RTT) | 16 |
| count − 1 | 2 |
| newest command's tick | 32 |
| `count` × `InputCmd` body, newest first (ticks descend by 1) | 107 each, 169 locked on |

`InputCmd` body:

| Field | Bits |
|---|---|
| view delta (`tick·16 − view_tick_q4`, lag-comp view time in 1/16 ticks, saturating at 255) | 8 |
| aim (octahedral) | 32 |
| thrust x, y, z (`i8`, local frame: right, up, forward) | 24 |
| roll (`i8`) | 8 |
| buttons (below) | 16 |
| lock target (entity slot, 1023 = none) | 10 |
| shot_seq | 8 |
| locked on (a `LockOn` follows) | 1 |
| `LockOn`, when locked on: ref_vel (3 × 14, centred grid over ±2 048 m/s: 0.25 m/s steps, zero exact) | 42 |
| `LockOn`: up (octahedral, 2 × 10) | 20 |

**Locked on** (`docs/LOCK.md`): flight assist holds the suit's velocity relative to `ref_vel` (the
target's, as the pilot sees it), reading the stick in axes levelled to `up` (the stick's up; its
forward the aim laid flat), and the suit rolls level with `up`. The server checks neither against
the target: the simulation caps `ref_vel` at the frame's boosted cruise, and that's all a lock-on
can ask for. A free suit only; on a body the grip's rules fly it.

Buttons, by bit: 0 FIRE_PRIMARY, 1 FIRE_SECONDARY, 2 MELEE, 3 BOOST, 4 BRAKE, 5 FLIGHT_ASSIST*,
6 ZERO*, 7 RCS_SHARP, 8 GRAB*, 9 STOW, 10 THROW, 11 JETTISON, 12 MODE* (the frame's mode: Neo-Bird,
Hyper Jammer), 13 SPECIAL (the frame's special attack: Full Open Attack, Cross Crusher), 14 GRIP*
(land on a body near enough and slow enough, and keep hold of it; clear: let go), 15 BURST (a
burst step along the stick: it starts on the press, so a repeated command never steps again).
Starred bits are states.

`aim` is in the sector's frame, on a body or not. On its feet on a body, a suit reads `thrust` as
legs: x and z walk (Shift runs), and `thrust[1]` sets the stance and stays set: −64 or less
crouches, 32 or more stands, 100 or more (standing) hops, and anything between keeps the stance it
has, so a silent client stays crouched.

Four commands fit in 65 bytes (86 + 4 × 107 bits), or 96 locked on (86 + 4 × 169). A client sending several ticks at once sends
overlapping windows two ticks apart, so each command is in two packets. States persist while a
client is silent (the server repeats its last command, keeping only FLIGHT_ASSIST, ZERO, GRAB,
MODE and GRIP, and the lock-on for 30 ticks more); presses (STOW, THROW, JETTISON, MELEE, SPECIAL, BURST) act on the tick they first appear, and a
repeated command never fires.

Lag compensation reaches back at most 8 ticks. A view delta of 128 or more (8 ticks) resolves at
exactly `tick − 8`, so the 8-bit field's saturation at 15.9 ticks loses nothing.

## Server → client: snapshot

| Section | Content |
|---|---|
| header (116 bits) | kind=2, tick, ack_input_tick, input_health (i8), time_echo_ms, echo_hold_ms, tidi_pct, flags |
| own (1 + 813..833 bits) | slot, generation, frame, alive, pos, vel (f32), rot (16-bit), ang_vel, propellant (f32), g_strain (f32), heat, energy, ammo ×2, weapon_ready (4), charge, parts ×6, zero_strain, zero_mode, flags, systems (24), modules (20), scram (7, ticks), concussed (7, ticks), repairing (4: a system, 15 = none), repair left (7, ticks ÷ 8), respawn_in, the rack (8: 2 bits a consumable), a stim's clock (12, ticks), extra mass (kg, i18), cargo ×4 (kg, 14 bits each), credits (24), held chunk (10), lock target (10), lock progress (4), special timer (8, ticks), special charge (8, 255ths: 255 charged, or nothing to charge; ticks ÷ 4 of its cooldown before v25), arms (46, below), burst step (17, below), footing (2), cover (2), doom (5, steps of 3 ticks until a doomed suit's reactor goes, rounded up; 0: not doomed, v23; in steps since v24), impact (3, sixths of what the suit stands, v24), stagger (5, ticks left; 0: steady, v24), the designated target's impact (3, sixths; 7: staggered, v24), and on a body its body (6 or 12) and stance (8) (below) |
| ZERO (1 + ≤200 bits) | source_jev, advice_age, threat_count, per threat {slot, 7 × p}, rec_target + p, rec_maneuver + p, threat_level + confidence, flanked, has_solution, solution (oct 2×12), hit_p |
| events | repeated `[1][event]`, closed by `[0]` |
| rocks | repeated `[1][rock]` (18 bits each), closed by `[0]` |
| missiles | repeated `[1][missile]` (119 bits each), closed by `[0]` |
| entities | repeated `[1][entity]` (194 or 211 bits each), closed by `[0]` |
| objects | repeated `[1][object]` (12–232 bits each), closed by `[0]` |

The writer reserves room for every list terminator still owed before it writes a record, so a
snapshot is never cut off mid-list.

Header flags, by bit: 0 SPECTATOR (v18): a spectator's snapshot, sent to a pilot on foot in the
colony's city by its inside sector (below, "Suits inside the colony"). It has no own state and no
ZERO, no events but Leave notices, no rocks, missiles or objects; its `ack_input_tick`,
`input_health` and echo say nothing (the pilot sends no input).

What fits, in the 8 800 bits of a 1 100-byte datagram: the fixed part is the header, the own state,
ZERO's presence bit and the five lists' terminators.

| | Own flying free | Own on a rock (the largest) |
|---|---|---|
| Fixed | 920 bits | 940 bits |
| Free suits (1 + 211 bits each), nothing else | 37 | 37 |
| Suits on bodies (1 + 194 bits each), nothing else | 40 | 40 |
| Room kept for six of the largest objects (6 × 233 bits) | 30 free / 33 riders | 30 / 33 |
| With ZERO on (+200 bits) | 36 / 39 | 36 / 39 |
| A 256-byte connection (2 048 bits) | 5 / 5 | 5 / 5 |

A suit on the colony's city takes 202 bits (its place needs 21 bits an axis): 38 of them fit, the
own suit flying free, and nothing else.

### Bodies and riders

A suit standing on a body, in its grip in the air, or parked on it is a *rider*, and is sent in the
body's frame. A body is named by a `BodyRef`: a 2-bit kind, then an id.

| Kind | Body | Id |
|---|---|---|
| 0 | a rock of the debris field | 10 bits (the rock's index) |
| 1 | a landmark (MO-II, Hermit: `bc_sim::content::landmarks`) | 4 bits (its index) |
| 2 | the colony's city, in an interior sector (v19): its floor, its buildings and its end caps | none |
| 3 | invalid: the record doesn't decode | |

Two rules keep this cheap and exact:
- **Body poses never travel.** Rocks don't move, and come from the Welcome's field; a landmark's
  pose is a closed form in the integer tick, from compiled content; the city stands still at an
  interior sector's origin (its frame is the colony's own). Client and server work out the same
  pose for the same tick, to the bit.
- **A rider is never sent without its body known.** Its rock is in the field (a shattered rock
  keeps its pose: the riders on it are let go a tick later, and the next snapshot sends them
  free), its landmark is one of the first `landmarks` of the Welcome, and the city is named only
  in a sector the Welcome says is the colony's inside (INTERIOR). A client drops a record that
  names any other.

A rider's sector pose is its body's at the snapshot's tick composed with its body-frame pose: the
position `P + R·local`, the rotation `R·rot`, the velocity the body's surface velocity there plus
`R·vel`.

Header notes:
- `input_health` is how many ticks of this client's input the server has buffered beyond the
  current tick.
- `time_echo_ms` is the `client_time_ms` of the newest input packet the server received.
- `echo_hold_ms` is how long the server held that packet before this snapshot. `255` means "255 ms
  or more": clients must not take an RTT sample from it.

Own-state notes:
- A part with any armour left encodes as at least 1/255: 0 means it is gone.
- The client flies its suit with the stat sheet it builds from the snapshot (`bc_sim::tuning`):
  the parts left, `systems` (2 bits a system, in `bc_sim::content::System` order: 0 working,
  1 damaged, 2 failed) and `modules` (4 bits a mount, in `bc_sim::content::modules::MOUNTS`
  order: 0 empty, else the module's code). The server builds the same one for the next tick from
  the same state, so prediction matches it, coughing main thrusters (their windows come from the
  tick and the slot) and a leaking tank included; and exactly `extra_mass_kg` (cargo, what's in
  hand, modules, less the parts shot off). AMBAC's authority is the one with the arms idle; busy
  arms take 0.6 of it, which the client works out tick by tick from the arms.
- `scram` and `concussed` count the ticks a scrammed reactor gives nothing and a concussed
  pilot's shots wander (the shooter's client draws its own shots wandering the same way).
- The arms record lets the client roll its suit's arms on from the snapshot as the server does
  (`bc_sim::arms`), so its prediction lunges, and turns with busy arms, on the same ticks:
  the strike under way (phase 2: none, windup, stroke, recovery; timer 5; mount 2, 3 being the
  special's melee move), ticks since a weapon fired or a strike began (3, saturating at 7), per
  mount (the loadout's three, then the special's) the ticks until it could fire or strike but
  for heat (6 bits each; 63 = not until something the client can't foresee changes: an arm shot
  off, energy or rounds run out), and each gun slot's missile salvo under way (rounds left, 3;
  ticks to the next, 2). Heat is the OVERHEAT flag, and the lockout after Full Open follows it.
- The burst step (`bc_sim::flight::Burst`), which the client rolls on from the snapshot as the
  server does: ticks of the step still to drive (4), ticks until another can start (6), the
  stick's direction at its press (3 × 2 bits: 0 back, 1 none, 2 forward on each axis; 3 is
  invalid), and whether BURST was held last tick (1), so a press is told from a held button.
- `weapon_ready` has a bit each for the primary, secondary, melee weapon and the frame's special.
- Footing (2 bits): 0 flying free, 1 on its feet (or knees) on a body, 2 in a body's grip in the
  air; 3 is invalid. Unless it is 0, the body's `BodyRef` and the stance follow: how high the
  suit's origin rides over the surface, in sixteenths of a metre (96 crouched to 146 standing).
  Then the position, velocity, rotation and angular velocity above are in the body's frame (the
  velocity over the body), so the client re-runs exactly what the server moves; it composes them
  with the body's pose at the snapshot's tick for the sector's frame.
- Cover (2 bits): 0 exposed, 1 settling (crouched still for under 3 s, or shown by firing or a
  hit), 2 cold (settled out of a hide spot: half the signature), 3 hidden (off enemies' sensors).
- Flags: BOOSTING, BLACKOUT, OVERHEAT, CHARGING, SABER_ACTIVE, ZERO_CAPABLE, FLIGHT_ASSIST,
  LOCKED_ON, DOCKED (in the colony's dock), LUNGE (saber windup and swing: the flight model's
  lunge), SPECIAL_ACTIVE (the jammer is on, a melee move is out), TRANSFORMING (the special
  timer counts the change of form down),
  LOCK_ACQUIRED (your missile lock), MISSILE_LOCK (someone's missile lock is on you),
  MISSILE_INCOMING (a guided missile is tracking you), PARKABLE (you're on your feet on a body, or
  resting against a rock or a landmark, slowly enough to park: a signed-in pilot who leaves now
  stays parked there).
- Charge is the primary's, as a fraction: the Twin Buster's charge, or on a weapon with a charged
  shot (the beam rifle) how long its trigger has been held, from the press to a full charge (1:
  let go and the charged shot, weapon kind 19, leaves). CHARGING, here and on the entity, is the
  Twin Buster charging or a charged shot held past its tap.
- The lock target is the designation the server accepted: alive, hostile and on your sensors.
  Lock progress counts 0–15 toward a missile lock on it; LOCK_ACQUIRED says it's there.
  LOCKED_ON ignores locks by suits you can't see (a jamming suit's lock goes unnoticed).
- The special timer counts ticks: of a change of form, of Full Open, or of the break until the
  Hyper Jammer hides the suit again.

Entity record, by kind:

| Field | Flying free | On a rock | On a landmark |
|---|---|---|---|
| slot 10, generation 2, frame 4, faction 3, pilot kind 2 | 21 | 21 | 21 |
| attached | 1 (= 0) | 1 | 1 |
| `BodyRef` | | 2 + 10 | 2 + 4 |
| aloft (in the body's grip, not on its feet) | | 1 | 1 |
| position | 63 (sector) | 45 (body frame) | 51 (body frame) |
| rotation | 32 (sector) | 32 (body frame) | 32 (body frame) |
| velocity | 42 (sector) | 30 (over the body) | 30 (over the body) |
| aim (sector) | 18 | 18 | 18 |
| flags | 16 | 16 | 16 |
| 6 part-armour buckets (3 bits each, 0–7) | 18 | 18 | 18 |
| **total** | **211** | **194** | **194** |

How high a rider stands isn't sent: the surface under it says (`Shape::probe`). A parked suit is
sent at rest on its body (velocity 0), ASLEEP. A rider standing still (stick idle, not turning,
not fighting) or parked is sent a tenth as often as a moving one at the same range: its record
says the same thing every time but for its flags and parts.
FIRING_PRIMARY and FIRING_SECONDARY say the slot fired in the last 4 ticks: clients draw a
stream weapon's tracers from them (its shots send no BeamSpawn), and a flamethrower's flag is set
while it's lit. SABER says a melee strike is out. The last two flags are SPECIAL (the frame's
special is engaged; a jamming suit shows it only to its allies, and its enemies' sensors lose it
past 150 m) and MELEE_ALT (the melee strike
under way comes from a ranged slot, the Dragon Fang). ASLEEP says its pilot is offline, asleep in
the cockpit. SPARKING, SMOKING and VENTING say a system inside it is damaged, one has failed, and
its tank is holed.

Missile record: pool id (10), generation (2), weapon kind (5), guided, targets you, friendly (1
each), position (63, the entity grid), velocity (3 × 12 bits over ±4 096 m/s). A snapshot lists at
most 12, those tracking the receiving pilot first, then the nearest within 5 km; clients
extrapolate between snapshots.

Events carry a 3-bit kind and an 8-bit age (ticks before the snapshot):

| Kind | Event | Payload |
|---|---|---|
| 0 | BeamSpawn | id, shooter, weapon, shot_seq, origin, velocity (direction + speed) |
| 1 | Hit | id, target, part, shooter, weapon, damage fraction |
| 2 | Kill | id, victim, killer, hulk (the chunk its wreck became, 1023 = none) |
| 3 | Leave | slot: left your sensors (idempotent, no id) |
| 4 | Clash | id, a, b |
| 5 | Seizure | id, pilot, active |
| 6 | Detach | id, from_hulk, source (suit slot, or hulk chunk), part, chunk (the limb) |
| 7 | extension | 3-bit sub-kind: 0 = RockBreak {id, rock, by}; 1 = MissileBurst {id, missile id, position, cause (2 bits: hit, proximity, expired, blocked)}; 2 = SystemHit {id, target (entity slot), system (4), level (2)}: a blow reached a system inside a suit; 3 = TargetHit {id, target (4 bits: one of the Blast Hall's), shooter (entity slot)}: a training round scored (v21); 4 = Doomed {id, suit}: its torso breached, its reactor going (v23); 5 = Eject {id, suit, position, velocity (3 × 14 bits over ±2 048 m/s)}: its pilot's capsule thrown clear (v23); 6 = Blast {id, suit, position}: a doomed suit blown up by its pilot, the damage arriving as Hits by weapon 20, the reactor (v23); 7 = the second extension (v24), a 4-bit sub-kind: 0 = Staggered {id, suit}: its attitude control overwhelmed, for `STAGGER_TICKS` (30) from the event's tick; 1–15 reserved |

Events repeat in every snapshot until the client acks one that carried them. `id` (the low 16 bits
of the event sequence) lets clients de-duplicate the repeats.

Rock record: id (10), destroyed (1), structure left in eighths (3), ore left in sixteenths (4). The
field itself comes from the Welcome's seed; only rocks whose state changed are sent, each until
acked.

Object record: a 2-bit kind, the chunk id (10), then:

| Kind | Payload |
|---|---|
| 0 Gone | nothing: forget the chunk (it's gone, or out of range) |
| 1 Free | generation (2), chunk, segment: age (16 bits of ticks), position (63), velocity (42), rotation (32), spin (3 × 10 bits over ±4 rad/s) |
| 2 Held | generation (2), chunk, holder slot (10), right hand (1), rotation relative to the holder (29), when it was grabbed (16 bits of age) |

Chunk: class (2 bits: ore, limb, hulk), then ore kind (2), or frame (4), faction (3, for the
livery) and part (3), or frame (4), faction (3) and a mask of parts still on it (6); a seed (8);
mass in 10 kg steps (12). A free chunk moves on
its segment: `pos(t) = pos + vel · (t − t0)·DT`, spinning at `spin`. The server moves chunks on
exactly the quantized segment it sends, so clients evaluating it at the same tick get the same
answer to the bit.

## The colony's people: pose and plaza datagrams

Pilots on foot in the colony's city (survival, `--colony`) are relayed by their session tasks,
off the sector's tick (`bc_proto::presence`; the server's `plaza`). Positions are a strip's city
coordinates, where the city stands still: `x` along (22 bits over ±16,384 m), `s` across from the
strip's edge (19 bits over 0–4,096 m), `h` up (15 bits over −8–248 m), all in 7.8 mm steps; the
walker's yaw (10 bits), pitch (8 bits over ±90°), speed over the ground (6 bits, 0.2 m/s steps to
12.6), GROUNDED and RUNNING, and what they ride (4 bits: 0 on foot, `k` + 1 on train `k` of the
strip's line, 13 driving a car, 14 on a scooter, 15 sitting on a seat: v16 gave 15 that meaning,
with no new bits), 86 bits in all. A rider's `x`, `s` and `h` are from their train's middle, from its
track's middle plus 2,048 m, and from its floor: everyone draws them inside the train wherever
their own screen has it (`transit::train` is a closed form of the tick).

| Datagram | Content | Size |
|---|---|---|
| kind 3, Pose (client → server, 15 Hz in the city) | kind (4), seq (u16), strip (2), the pose (86) | 14 B |
| kind 4, Plaza (server → client: 10 Hz in the city and flying inside the colony, 2 Hz in the bay) | kind (4), the sector's tick (u32), in the city (1), strip (2), then until the bits run out: per person their client slot (10), how long before the tick their pose was heard (6 bits, 10 ms steps) and the pose (86) | 5 B + 12.75 B a person, at most 48 (617 B) |

A pose is taken only if it could be: on the pilot's strip, inside the colony, out of the walls
(`bc_sim::colony::city::solid`), no further from the last one taken than 13.5 m/s over the reach
banked and 2 m allow (the time since the last pose earns reach, a move spends it, and up to 2 s of
it is carried to the next: a slow page's walk, sent late and then at once, passes; a teleport
doesn't), the first within 150 m of the strip's Hub Gate, and newer (`seq`) than the last. A rider must be inside
their train's cars; getting on or off, within 8 m of the train while it stood with its doors open
(within 3 s of the sector's tick). A driver's first pose must be at a motor pool
(`bc_sim::colony::pools`), and no driver goes faster than 45.5 m/s. A seated pose must be on a
seat (`bc_sim::colony::city::arrival_seats`, within 0.3 m), and stays put on it until its pilot
stands. Anything else isn't
passed on (`/status`'s `city.refused_poses`). Each pilot is sent the people on their strip within
1.5 km of them, heard from in the last 5 s, nearest first; their names come once each on the
control stream (`people`). A pilot flying a suit inside the colony is sent the people of the strip
under the suit (over a window, the nearer strip's), within 1.5 km of it in the air, nearest first.
The plaza's tick keeps a client's clock (and the colony's day) when none of the pilot's own
snapshots come: in the bay, and in the city (a spectator's snapshots, below, carry no round trip).

## Control stream

Frames are `[u16 LE payload length][u8 tag][payload]`, byte-aligned, at most 256 bytes with the
prefix, except the hangar's (tag 11), which may carry up to 64 KiB.

| Tag | Message | Direction |
|---|---|---|
| 1 | Hello {version, pilot kind, frame, faction, name ≤ 16 B, flags (1 SIGN_IN, 2 RESUME), resume token (32 B, only with RESUME)} | client → server (first frame) |
| 2 | Welcome {version, client slot, tick, tick_hz, sector, zero_allowed, max_datagram, field_seed, field_rocks, flags (1 SIGNED_IN, 2 WOKE, 4 SURVIVAL, 8 ANIME, 16 COLONY, 32 INTERIOR), landmarks (u8)} | server → client (again on moving between sectors) |
| 3 | Reject {reason: 1 version, 2 full, 3 bad hello, 4 frame not allowed, 5 sign-in failed, 6 sign-in required, 7 resume token expired, 8 no signature in time} | server → client |
| 4 | Roster {entity slot, pilot kind, name (empty = left), flags (1 VERIFIED, 2 ASLEEP, bits 2-4 the suit's weathering 0-7)} | server → client |
| 5 | Respawn {frame} | client → server |
| 6 | Bye {reason: 0 leaving, 1 taken over, 2 idle, 3 shutdown} | either |
| 7 | Challenge {nonce (32 B), issued_at (u64, Unix seconds), domain (u8 length + ASCII, ≤ 64 B)} | server → client |
| 8 | Auth {address (20 B), signature (65 B: r, s, v)} | client → server |
| 9 | Token {resume token (32 B)} | server → client |
| 10 | Notice {code: 1 your sleeping suit was destroyed, 2 your sleeping suit is gone; name ≤ 16 B (who, for 1)} | server → client |
| 11 | Hangar {JSON, `bc_econ::wire`: a Request up, an Update down} | either (survival rules; the radio's `say` and `said` under any) |

Pilots claiming to be a server-side Mobile Doll are downgraded to `Agent`.

A Hello is read version first: one from another protocol version answers `Reject {version}` even
if the rest of it (a frame this build doesn't know) would not parse.

### Signing in

Guests (and agents) send a Hello with no flags and get their Welcome straight away. A pilot with a
wallet sets SIGN_IN:

1. The server answers with a Challenge: a fresh random nonce, the time, and its domain (the host
   the page was served from, `--siwe-domain`).
2. Both sides build the same EIP-4361 ("Sign-In with Ethereum") text from it with
   `bc_auth::siwe_message`. It says it authorizes nothing and moves no funds. The wallet signs it
   with `personal_sign`, and the client sends the address and signature in an Auth.
3. The server recovers the signer from the signature. If it isn't the address, or the Auth takes
   longer than a minute, the session is rejected (5 or 8).
4. Welcome (with SIGNED_IN), then a Token.

A Token lets the pilot reconnect without signing again: a later Hello sets RESUME and carries it
instead of SIGN_IN. Each token works once (the session that redeems it gets a new one), and lasts
15 minutes after its session ends. An unknown or expired token is rejected with 7; the client then
signs in afresh.

One session per wallet: signing in (or resuming) while another session flies the same pilot sends
that one `Bye {taken over}` and waits for it to go. A client told it was taken over doesn't
redial, so two windows don't fight over one pilot.

The server logs addresses shortened (`0x1234…abcd`), and never signatures or tokens.

`--require-auth` turns guests (human ones) away with Reject 6.

### Leaving and coming back

A signed-in pilot's suit outlives the session: when it ends (a Bye, the link dropping, or a
minute without input, which the server ends with `Bye {idle}`), the suit stays in the sector,
asleep. Everyone's roster shows it with ASLEEP (and snapshots with the entity flag). The pilot's
next session wakes in it: the Welcome sets WOKE. If it's gone, the pilot starts in a new suit and a
Notice says why: destroyed while they slept (and by whom), or lost (cleared to make room, or the
server restarted). A suit that was already a wreck is simply gone. Guests' suits go when they do.

Under survival rules, a suit left on its feet in a landmark's hide spot outlives a restart: the
server puts it back, asleep, before anyone connects, and its pilot wakes in it as above (WOKE).
Its roster entry, ASLEEP, is there from the start.

A suit woken on a body (or put down on one) holds its grip until its client's first command
arrives: until then the server flies the input it left the suit with, GRIP set. A client should
send GRIP from its first command on, for a suit whose own state is on a body, or the suit lets go.

### Survival: the hangar's messages

Under survival rules (the Welcome sets SURVIVAL) a pilot starts in their hangar bay, not in the
sector: they get no snapshots until they launch. The hangar talks in JSON on the control stream
(tag 11), readable by people and agents alike; `bc_econ::wire` defines it. Every message is an
object tagged by `"t"`.

Client → server (`Request`):

| `t` | Fields | Does |
|---|---|---|
| `craft` | `item`, `batches` | queue batches of what makes `item` at its station |
| `cancel_job` | `station` (`fabricator`, `foundry`), `index` | cancel a queued job (what's not started comes back) |
| `fit` | `item` | fit a part or weapon from the stores (a torso into an empty bay starts a suit) |
| `strip` | `slot`: `{"kind": "part", "part": …}`, `{"kind": "mount", "mount": 0–2}` or `{"kind": "module", "module": 0–4}` | take it off into the stores (a part takes its equipment with it, and carries its faults) |
| `dismantle` | | strip the suit bare |
| `repair` | `part` (optional: all) | repair armour as far as the stores allow |
| `overhaul` | `part` (optional: all) | restore damaged and failed systems as far as the stores allow |
| `scrap` | `item` | melt one down for half its materials |
| `order` | `item`, `side` (`buy`, `sell`), `price`, `qty`, `rest` | a limit order on the exchange |
| `cancel_order` | `id` | |
| `watch` | `item` (or `null`) | send that item's book and history as they change |
| `launch` | | board and launch the suit in the bay |
| `dock` | | take the suit home (at rest inside the dock, or inside the colony the inner gate's ring; a trainer, on the Blast Hall's gantry) |
| `launch_inside` | | board and launch the suit into the colony through the inner gate (the colony open) |
| `board_trainer` | | on foot at the Blast Hall's gantry's hatch in the city: board one of the Charter Board's trainers there (the pilot's hangar untouched) |
| `enter_city` | `strip` (0–2) | ride the cap lift down from the bay to that strip's Hub Gate (the colony open, and the pilot in their bay) |
| `leave_city` | | ride the lift back up from Hub Gate to the bay |
| `watch_board` | `on` | send the Charter Board (`charter`) as it changes, or stop |
| `post` | `item`, `qty`, `reward`, `hours` (1–72) | post a supply contract; the reward goes into escrow |
| `withdraw` | `id` | take one's own contract down (what it hasn't paid comes back) |
| `deliver` | `id`, `qty` | deliver to a supply contract from the stores, paid pro rata on the spot |
| `take_patrol` · `drop_patrol` | `id` | take a militia patrol (one at a time), or give it up |
| `contribute` | `work` (`second_foundry`, `militia_hangar`), `item`, `qty` | deliver to one of the colony's great works |
| `sign` | | sign the charter (the vote open, and the pilot of standing) |
| `use_kit` | `kit` (`patch_kit`, `coolant`, `chaff`, `stim`) | in flight: use one from the suit's rack (the hotbar; nothing answers, the own state shows it) |
| `eject` | `destruct` (default false) | in flight, under any rules: eject from the suit, or (`destruct`, doomed) blow it up aboard. Nothing answers but the loss (`sortie`, and under survival the tugs' `news` on the wreck 45 s on) |
| `say` | `text` | a line on the colony's radio, to everyone connected, under any rules: control characters stripped, whitespace made single spaces, cut to 160 characters; at most 5 lines in 10 s (more get a refusing `note`). Never logged; `/status` counts them (`radio_lines`) |

Items are slugs: `ore.nickel_iron`, `mat.steel`, `mat.components`, `part.leo.torso`,
`weapon.beam_rifle`, `module.g_seat`. Parts are
`head`, `torso`, `arm_l`, `arm_r`, `legs`, `backpack`. Prices are credits a tonne for ores and
materials (quantities in kg), credits a piece for everything else.

Server → client (`Update`): `place` {`place`: `hangar`, `space` or `city`, `bay`, in the city
its `strip`, and `trainer: true` flying one of the Board's trainers (absent otherwise)}; `hangar` (credits,
stock, parts with their condition, the bay: `empty`, `docked` or `out` with the suit, the job
queues with their time left); `market` (every item's bid, ask, last and volume, the pilot's
orders, the fee); `book` {`depth`, `history`}; `note` {`text`, `ok`} answering a request (or
news: a job done, an order filled); `sortie` {`outcome`: `docked`, `lost`, `recovered`, `text`};
`news` {`text`} (a pilot's arrival; the colony's announcements); `people` {`people`: [{`id`,
`name`}]} (in the city: the names of people seen there for the first time, by the slot the plaza's
datagrams use); `charter` (the Charter Board, while watched: the era, the contracts with their
`task` (`{"kind": "supply", "item", "qty", "delivered"}` or `{"kind": "patrol", "bounty",
"earned"}`), reward, paid and seconds left, the great works with what each needs and has, their
top contributors, the pilot's standing and the charter's signatures); `said` {`from`, `text`} (a line on the colony's radio, the speaker's own included,
from the moment the pilot was welcomed, in the order the server heard them); `proving` (the
Proving Ground's board, in the colony: `course` and `drill`, the day's best as [{`name`, `ms`,
`you`}] fastest first, `course_record` and `drill_record` the best ever, `course_par_ms` and
`drill_par_ms`, and `mine` {`course_ms`, `drill_ms`}, the pilot's own bests; no keys of anyone's). A suit (in the bay, or out) carries
`faults`, a map from system slug to `damaged` or `failed` (absent when everything works), and
`modules`, its five equipment mounts' slugs (or `null`); a part on the shelf carries its own
`faults`.
The server sends the hangar and the market whenever they change, the market and the board at most
every 2 s. The board's notices (a great work finished, the vote open, an era begun) come to every
pilot as `news`, wherever they are.

A launch puts the suit in the sector at the docking hub's mouth (the pilot's slot and the Welcome
stay the same; snapshots start), and `place` says `space`. Docking answers with a `sortie` and
`place: hangar`, or a refusing `note`. A suit destroyed out there sends `sortie: lost` at once and
`place: hangar` once the wreck clears.

Suits inside the colony (the Welcome sets COLONY): `launch_inside` seats the suit in the
server's second sector, the colony's inside (`sector-1`, in the colony's own frame:
`bc_sim::colony::interior`), and the server sends a new Welcome: sector 2, INTERIOR set, no field
and no landmarks, and the client slot the inside sector knows the pilot by. Inputs go to that
sector, its snapshots come instead, and nothing fires there but in the Blast Hall (training
rounds, which touch no suit). `dock` at rest in the inner gate's
ring brings the suit home, with a `sortie` and a Welcome back to sector 1 (the client slot it
had). A client welcomed mid-session forgets what it flew in the last sector. Leaving while
inside, the colony's tugs bring the suit back to the bay.

The Proving Ground's trainers (`docs/TRAINING.md`): `board_trainer`, on foot at the Blast Hall's
gantry's hatch (the plaza's last pose within 12 m), seats the pilot in a trainer standing on the
gantry, with a Welcome to sector 2 as `launch_inside` gives and `place: space, trainer: true`; the
pilot leaves the plaza. `dock` at rest on the gantry puts them back on foot at its hatch: a `note`,
a Welcome back to sector 1 and `place: city` (their first pose is taken at the hatch, not Hub
Gate). Nothing comes home from a trainer, and leaving in one loses nothing. The inside's sector
times every pilot's course and drill; each one flown or cleared comes as a `note` (`THE BOARD ·
…`) and a new `proving`, which everyone in the colony is sent as it changes (at most every 2 s).

The inside keeps the outside's tick (v18): it ticks each time sector 1 has, right after it, so the
colony has one clock. Its snapshots, the plaza's datagrams and the trams' timetable are the same
moment, for a pilot on foot and one in a suit.

A pilot on foot in the city watches the inside's suits (v18): while they're there, the inside
sector gives them a spectator's slot of its own (no suit; it isn't in the Welcome) and sends them
its snapshots marked SPECTATOR, with the suits within 2.5 km of where they are (the plaza's last
pose of theirs, in the colony's frame, moved twice a second), interpolated as any. A suit that
leaves their view gets a Leave notice for half a second (a spectator acks nothing; its client also
forgets a suit it stops hearing of). Up the lift, it ends, and the client forgets what it watched.
A client takes spectator snapshots only in the city, and knows the colony's city as a body for
them (v19: the suits standing on it ride it).

The colony (the Welcome sets COLONY: a survival server run with `--colony`): from the bay,
`enter_city` answers `place: city` with the strip, or a refusing `note`; in the city the hangar and
the market keep coming (the Exchange floor's terminal is the bay's), and `launch` is refused.
`leave_city` answers `place: hangar`. The city itself is compiled content (`bc_sim::colony::city`,
`content::city::CITY_VERSION`), the same on every client and the server, so any change to it bumps
the protocol version (v18: the rooms behind the key places' doors, walked into and checked as any
of the city's walls). Walking the city is the client's own, as in the bay; where the pilot stands goes
to the server in pose datagrams (above), for the others there to see.

### Setting up the sector

The Welcome's `field_seed` (u32) and `field_rocks` (u16) name the sector's debris field: clients
build it with `bc_sim::field::Field::generate(field_seed, field_rocks)`, identical to the
server's, and predict their suit against it. Its `landmarks` (u8) says how many of the compiled
landmarks the sector has: the first that many of `bc_sim::content::landmarks::LANDMARKS`, by
index (a client takes no more than it knows of). Their shapes and motion are compiled content,
so any change to them bumps the protocol version, as the field's generator does.
