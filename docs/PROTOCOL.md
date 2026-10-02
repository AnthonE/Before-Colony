# Before Colony wire protocol (v15)

Everything is little-endian and bit-packed LSB-first (`bc_proto::bits`). Datagrams are one QUIC
datagram each, at most `min(1100, connection max)` bytes, and never fragmented. The first 4 bits
of every datagram give the packet kind: `1` = input, `2` = snapshot.

## Quantization

| Quantity | Encoding |
|---|---|
| Entity position | 3 × 21 bits over ±32 768 m (3.1 cm steps) |
| Entity velocity | 3 × 14 bits over ±2 048 m/s (0.25 m/s) |
| Entity rotation | smallest-three: 2-bit index + 3 × 10 bits |
| Rider position (in its body's frame) | 3 × 15 bits over ±256 m on a rock, 3 × 17 bits over ±1 024 m on a landmark (1.5625 cm steps either way) |
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
| own (1 + 777..797 bits) | slot, generation, frame, alive, pos, vel (f32), rot (16-bit), ang_vel, propellant (f32), g_strain (f32), heat, energy, ammo ×2, weapon_ready (4), charge, parts ×6, zero_strain, zero_mode, flags, systems (24), modules (20), scram (7, ticks), concussed (7, ticks), repairing (4: a system, 15 = none), repair left (7, ticks ÷ 8), respawn_in, extra mass (kg, i18), cargo ×4 (kg, 14 bits each), credits (24), held chunk (10), lock target (10), lock progress (4), special timer (8, ticks), special cooldown (8, ticks ÷ 4), arms (46, below), burst step (17, below), footing (2), cover (2), and on a body its body (6 or 12) and stance (8) (below) |
| ZERO (1 + ≤200 bits) | source_jev, advice_age, threat_count, per threat {slot, 7 × p}, rec_target + p, rec_maneuver + p, threat_level + confidence, flanked, has_solution, solution (oct 2×12), hit_p |
| events | repeated `[1][event]`, closed by `[0]` |
| rocks | repeated `[1][rock]` (18 bits each), closed by `[0]` |
| missiles | repeated `[1][missile]` (119 bits each), closed by `[0]` |
| entities | repeated `[1][entity]` (194 or 211 bits each), closed by `[0]` |
| objects | repeated `[1][object]` (12–232 bits each), closed by `[0]` |

The writer reserves room for every list terminator still owed before it writes a record, so a
snapshot is never cut off mid-list.

What fits, in the 8 800 bits of a 1 100-byte datagram: the fixed part is the header, the own state,
ZERO's presence bit and the five lists' terminators.

| | Own flying free | Own on a rock (the largest) |
|---|---|---|
| Fixed | 883 bits | 903 bits |
| Free suits (1 + 211 bits each), nothing else | 37 | 37 |
| Suits on bodies (1 + 194 bits each), nothing else | 40 | 40 |
| Room kept for six of the largest objects (6 × 233 bits) | 30 free / 33 riders | 30 / 33 |
| With ZERO on (+200 bits) | 36 / 39 | 36 / 39 |
| A 256-byte connection (2 048 bits) | 5 / 5 | 5 / 5 |

### Bodies and riders

A suit standing on a body, in its grip in the air, or parked on it is a *rider*, and is sent in the
body's frame. A body is named by a `BodyRef`: a 2-bit kind, then an id.

| Kind | Body | Id |
|---|---|---|
| 0 | a rock of the debris field | 10 bits (the rock's index) |
| 1 | a landmark (MO-II, Hermit: `bc_sim::content::landmarks`) | 4 bits (its index) |
| 2, 3 | invalid: the record doesn't decode | |

Two rules keep this cheap and exact:
- **Body poses never travel.** Rocks don't move, and come from the Welcome's field; a landmark's
  pose is a closed form in the integer tick, from compiled content. Client and server work out
  the same pose for the same tick, to the bit.
- **A rider is never sent without its body known.** Its rock is in the field (a shattered rock
  keeps its pose: the riders on it are let go a tick later, and the next snapshot sends them
  free), and its landmark is one of the first `landmarks` of the Welcome. A client drops a record
  that names any other.

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
| 7 | extension | 3-bit sub-kind: 0 = RockBreak {id, rock, by}; 1 = MissileBurst {id, missile id, position, cause (2 bits: hit, proximity, expired, blocked)}; 2 = SystemHit {id, target (entity slot), system (4), level (2)}: a blow reached a system inside a suit; 3–7 reserved |

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
strip's line, 13 driving a car, 14 on a scooter), 86 bits in all. A rider's `x`, `s` and `h` are from their train's middle, from its
track's middle plus 2,048 m, and from its floor: everyone draws them inside the train wherever
their own screen has it (`transit::train` is a closed form of the tick).

| Datagram | Content | Size |
|---|---|---|
| kind 3, Pose (client → server, 15 Hz in the city) | kind (4), seq (u16), strip (2), the pose (86) | 14 B |
| kind 4, Plaza (server → client: 10 Hz in the city, 2 Hz in the bay) | kind (4), the sector's tick (u32), in the city (1), strip (2), then until the bits run out: per person their client slot (10), how long before the tick their pose was heard (6 bits, 10 ms steps) and the pose (86) | 5 B + 12.75 B a person, at most 48 (617 B) |

A pose is taken only if it could be: on the pilot's strip, inside the colony, out of the walls
(`bc_sim::colony::city::solid`), no further from the last one taken than 13.5 m/s and 2 m allow, the
first within 150 m of the strip's Hub Gate, and newer (`seq`) than the last. A rider must be inside
their train's cars; getting on or off, within 8 m of the train while it stood with its doors open
(within 3 s of the sector's tick). A driver's first pose must be at a motor pool
(`bc_sim::colony::pools`), and no driver goes faster than 45.5 m/s. Anything else isn't
passed on (`/status`'s `city.refused_poses`). Each pilot is sent the people on their strip within
1.5 km of them, heard from in the last 5 s, nearest first; their names come once each on the
control stream (`people`). The plaza's tick keeps a client's clock (and the colony's day) when no
snapshots come: in the city, and in the bay.

## Control stream

Frames are `[u16 LE payload length][u8 tag][payload]`, byte-aligned, at most 256 bytes with the
prefix, except the hangar's (tag 11), which may carry up to 64 KiB.

| Tag | Message | Direction |
|---|---|---|
| 1 | Hello {version, pilot kind, frame, faction, name ≤ 16 B, flags (1 SIGN_IN, 2 RESUME), resume token (32 B, only with RESUME)} | client → server (first frame) |
| 2 | Welcome {version, client slot, tick, tick_hz, sector, zero_allowed, max_datagram, field_seed, field_rocks, flags (1 SIGNED_IN, 2 WOKE, 4 SURVIVAL, 8 ANIME, 16 COLONY), landmarks (u8)} | server → client |
| 3 | Reject {reason: 1 version, 2 full, 3 bad hello, 4 frame not allowed, 5 sign-in failed, 6 sign-in required, 7 resume token expired, 8 no signature in time} | server → client |
| 4 | Roster {entity slot, pilot kind, name (empty = left), flags (1 VERIFIED, 2 ASLEEP)} | server → client |
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
| `dock` | | take the suit home (at rest inside the dock) |
| `enter_city` | `strip` (0–2) | ride the cap lift down from the bay to that strip's Hub Gate (the colony open, and the pilot in their bay) |
| `leave_city` | | ride the lift back up from Hub Gate to the bay |
| `say` | `text` | a line on the colony's radio, to everyone connected, under any rules: control characters stripped, whitespace made single spaces, cut to 160 characters; at most 5 lines in 10 s (more get a refusing `note`). Never logged; `/status` counts them (`radio_lines`) |

Items are slugs: `ore.nickel_iron`, `mat.steel`, `mat.components`, `part.leo.torso`,
`weapon.beam_rifle`, `module.g_seat`. Parts are
`head`, `torso`, `arm_l`, `arm_r`, `legs`, `backpack`. Prices are credits a tonne for ores and
materials (quantities in kg), credits a piece for everything else.

Server → client (`Update`): `place` {`place`: `hangar`, `space` or `city`, `bay`, and in the city
its `strip`}; `hangar` (credits,
stock, parts with their condition, the bay: `empty`, `docked` or `out` with the suit, the job
queues with their time left); `market` (every item's bid, ask, last and volume, the pilot's
orders, the fee); `book` {`depth`, `history`}; `note` {`text`, `ok`} answering a request (or
news: a job done, an order filled); `sortie` {`outcome`: `docked`, `lost`, `recovered`, `text`};
`news` {`text`} (a pilot's arrival; the colony's announcements); `people` {`people`: [{`id`,
`name`}]} (in the city: the names of people seen there for the first time, by the slot the plaza's
datagrams use); `said` {`from`, `text`} (a line on the colony's radio, the speaker's own included,
from the moment the pilot was welcomed, in the order the server heard them). A suit (in the bay, or out) carries
`faults`, a map from system slug to `damaged` or `failed` (absent when everything works), and
`modules`, its five equipment mounts' slugs (or `null`); a part on the shelf carries its own
`faults`.
The server sends the hangar and the market whenever they change, the market at most every 2 s.

A launch puts the suit in the sector at the docking hub's mouth (the pilot's slot and the Welcome
stay the same; snapshots start), and `place` says `space`. Docking answers with a `sortie` and
`place: hangar`, or a refusing `note`. A suit destroyed out there sends `sortie: lost` at once and
`place: hangar` once the wreck clears.

The colony (the Welcome sets COLONY: a survival server run with `--colony`): from the bay,
`enter_city` answers `place: city` with the strip, or a refusing `note`; in the city the hangar and
the market keep coming (the Exchange floor's terminal is the bay's), and `launch` is refused.
`leave_city` answers `place: hangar`. The city itself is compiled content (`bc_sim::colony::city`,
`content::city::CITY_VERSION`), the same on every client and the server, so any change to it bumps
the protocol version. Walking the city is the client's own, as in the bay; where the pilot stands goes
to the server in pose datagrams (above), for the others there to see.

### Setting up the sector

The Welcome's `field_seed` (u32) and `field_rocks` (u16) name the sector's debris field: clients
build it with `bc_sim::field::Field::generate(field_seed, field_rocks)`, identical to the
server's, and predict their suit against it. Its `landmarks` (u8) says how many of the compiled
landmarks the sector has: the first that many of `bc_sim::content::landmarks::LANDMARKS`, by
index (a client takes no more than it knows of). Their shapes and motion are compiled content,
so any change to them bumps the protocol version, as the field's generator does.
