# Before Colony wire protocol (v6)

Everything is little-endian and bit-packed LSB-first (`bc_proto::bits`). Datagrams are one QUIC
datagram each, at most `min(1100, connection max)` bytes, and never fragmented. The first 4 bits
of every datagram give the packet kind: `1` = input, `2` = snapshot.

## Quantization

| Quantity | Encoding |
|---|---|
| Entity position | 3 × 21 bits over ±32 768 m (3.1 cm steps) |
| Entity velocity | 3 × 14 bits over ±2 048 m/s (0.25 m/s) |
| Entity rotation | smallest-three: 2-bit index + 3 × 10 bits |
| Own position, velocity, propellant | raw `f32` (lossless: the client re-simulates from them) |
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
| `count` × `InputCmd` body, newest first (ticks descend by 1) | 106 each |

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

Buttons, by bit: 0 FIRE_PRIMARY, 1 FIRE_SECONDARY, 2 MELEE, 3 BOOST, 4 BRAKE, 5 FLIGHT_ASSIST*,
6 ZERO*, 7 RCS_SHARP, 8 GRAB*, 9 STOW, 10 THROW, 11 JETTISON, 12 MODE* (the frame's mode: Neo-Bird,
Hyper Jammer), 13 SPECIAL (the frame's special attack: Full Open Attack, Cross Crusher), 14–15
reserved. Starred bits are states.

Four commands fit in 64 bytes (86 + 4 × 106 bits). A client sending several ticks at once sends
overlapping windows two ticks apart, so each command is in two packets. States persist while a
client is silent (the server repeats its last command, keeping only FLIGHT_ASSIST, ZERO, GRAB and
MODE); presses (STOW, THROW, JETTISON, MELEE, SPECIAL) act on the tick they first appear, and a
repeated command never fires.

Lag compensation reaches back at most 8 ticks. A view delta of 128 or more (8 ticks) resolves at
exactly `tick − 8`, so the 8-bit field's saturation at 15.9 ticks loses nothing.

## Server → client: snapshot

| Section | Content |
|---|---|
| header (116 bits) | kind=2, tick, ack_input_tick, input_health (i8), time_echo_ms, echo_hold_ms, tidi_pct, flags |
| own (1 + 641 bits) | slot, generation, frame, alive, pos, vel (f32), rot (16-bit), ang_vel, propellant (f32), g_strain, heat, energy, ammo ×2, weapon_ready (4), charge, parts ×6, zero_strain, zero_mode, flags, ambac/thrust factors, respawn_in, extra mass (kg, i18), cargo ×4 (kg, 14 bits each), credits (24), held chunk (10), lock target (10), lock progress (4), special timer (8, ticks), special cooldown (8, ticks ÷ 4) |
| ZERO (1 + ≤200 bits) | source_jev, advice_age, threat_count, per threat {slot, 7 × p}, rec_target + p, rec_maneuver + p, threat_level + confidence, flanked, has_solution, solution (oct 2×12), hit_p |
| events | repeated `[1][event]`, closed by `[0]` |
| rocks | repeated `[1][rock]` (18 bits each), closed by `[0]` |
| missiles | repeated `[1][missile]` (119 bits each), closed by `[0]` |
| entities | repeated `[1][entity]` (207 bits each), closed by `[0]` |
| objects | repeated `[1][object]` (12–232 bits each), closed by `[0]` |

The writer reserves room for every list terminator still owed before it writes a record, so a
snapshot is never cut off mid-list.

Header notes:
- `input_health` is how many ticks of this client's input the server has buffered beyond the
  current tick.
- `time_echo_ms` is the `client_time_ms` of the newest input packet the server received.
- `echo_hold_ms` is how long the server held that packet before this snapshot. `255` means "255 ms
  or more": clients must not take an RTT sample from it.

Own-state notes:
- A part with any armour left encodes as at least 1/255: 0 means it is gone.
- The server flies the suit with the ambac and thrust factors rounded to the same 8 bits, and with
  exactly `extra_mass_kg`, so prediction matches it.
- `weapon_ready` has a bit each for the primary, secondary, melee weapon and the frame's special.
- Flags: BOOSTING, BLACKOUT, OVERHEAT, CHARGING, SABER_ACTIVE, ZERO_CAPABLE, FLIGHT_ASSIST,
  LOCKED_ON, DOCKED (in the colony's dock), LUNGE (saber windup and swing: the flight model's
  lunge), SPECIAL_ACTIVE (the jammer is on, a melee move is out), TRANSFORMING (the special
  timer counts the change of form down),
  LOCK_ACQUIRED (your missile lock), MISSILE_LOCK (someone's missile lock is on you),
  MISSILE_INCOMING (a guided missile is tracking you), PARKABLE (you're resting against an
  asteroid, slowly enough to park: a signed-in pilot who leaves now stays parked there).
- The lock target is the designation the server accepted: alive, hostile and on your sensors.
  Lock progress counts 0–15 toward a missile lock on it; LOCK_ACQUIRED says it's there.
  LOCKED_ON ignores locks by suits you can't see (a jamming suit's lock goes unnoticed).
- The special timer counts ticks: of a change of form, of Full Open, or of the break until the
  Hyper Jammer hides the suit again.

Entity record: slot (10), generation (2), frame (4), faction (3), pilot kind (2), position (63),
rotation (32), velocity (42), aim (18), flags (13), and 6 part-armour buckets (3 bits each, 0–7).
FIRING_PRIMARY and FIRING_SECONDARY say the slot fired in the last 4 ticks: clients draw a
stream weapon's tracers from them (its shots send no BeamSpawn), and a flamethrower's flag is set
while it's lit. SABER says a melee strike is out. The last two flags are SPECIAL (the frame's
special is engaged; a jamming suit shows it only to its allies, and its enemies' sensors lose it
past 150 m) and MELEE_ALT (the melee strike
under way comes from a ranged slot, the Dragon Fang). ASLEEP says its pilot is offline, asleep in
the cockpit.

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
| 7 | extension | 3-bit sub-kind: 0 = RockBreak {id, rock, by}; 1 = MissileBurst {id, missile id, position, cause (2 bits: hit, proximity, expired, blocked)}; 2–7 reserved |

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

## Control stream

Frames are `[u16 LE payload length][u8 tag][payload]`, byte-aligned, at most 256 bytes with the
prefix.

| Tag | Message | Direction |
|---|---|---|
| 1 | Hello {version, pilot kind, frame, faction, name ≤ 16 B, flags (1 SIGN_IN, 2 RESUME), resume token (32 B, only with RESUME)} | client → server (first frame) |
| 2 | Welcome {version, client slot, tick, tick_hz, sector, zero_allowed, max_datagram, field_seed, field_rocks, flags (1 SIGNED_IN, 2 WOKE)} | server → client |
| 3 | Reject {reason: 1 version, 2 full, 3 bad hello, 4 frame not allowed, 5 sign-in failed, 6 sign-in required, 7 resume token expired, 8 no signature in time} | server → client |
| 4 | Roster {entity slot, pilot kind, name (empty = left), flags (1 VERIFIED, 2 ASLEEP)} | server → client |
| 5 | Respawn {frame} | client → server |
| 6 | Bye {reason: 0 leaving, 1 taken over, 2 idle, 3 shutdown} | either |
| 7 | Challenge {nonce (32 B), issued_at (u64, Unix seconds), domain (u8 length + ASCII, ≤ 64 B)} | server → client |
| 8 | Auth {address (20 B), signature (65 B: r, s, v)} | client → server |
| 9 | Token {resume token (32 B)} | server → client |
| 10 | Notice {code: 1 your sleeping suit was destroyed, 2 your sleeping suit is gone; name ≤ 16 B (who, for 1)} | server → client |

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

### Setting up the sector

The Welcome's `field_seed` (u32) and `field_rocks` (u16) name the sector's debris field: clients
build it with `bc_sim::field::Field::generate(field_seed, field_rocks)`, identical to the
server's, and predict their suit against it.
