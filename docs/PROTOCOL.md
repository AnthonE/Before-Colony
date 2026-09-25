# Before Colony wire protocol (v2)

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
| `count` × `InputCmd` body, newest first (ticks descend by 1) | 102 each |

`InputCmd` body:

| Field | Bits |
|---|---|
| view delta (`tick·16 − view_tick_q4`, lag-comp view time in 1/16 ticks) | 12 |
| aim (octahedral) | 32 |
| thrust x, y, z (`i8`, local frame: right, up, forward) | 24 |
| roll (`i8`) | 8 |
| buttons: FIRE_PRIMARY, FIRE_SECONDARY, MELEE, BOOST, BRAKE, FLIGHT_ASSIST*, ZERO*, RCS_SHARP (*states) | 8 |
| lock target (entity slot, 1023 = none) | 10 |
| shot_seq | 8 |

Four commands fit in about 62 bytes. A client sending several ticks at once sends overlapping
windows two ticks apart, so each command is in two packets.

## Server → client: snapshot

| Section | Content |
|---|---|
| header (116 bits) | kind=2, tick, ack_input_tick, input_health (i8), time_echo_ms, echo_hold_ms, tidi_pct, flags |

- `input_health` is how many ticks of this client's input the server has buffered beyond the
  current tick.
- `time_echo_ms` is the `client_time_ms` of the newest input packet the server received.
- `echo_hold_ms` is how long the server held that packet before this snapshot. `255` means "255 ms
  or more": clients must not take an RTT sample from it.
| own (1 + 486 bits) | slot, generation, frame, alive, pos, vel (f32), rot (16-bit), ang_vel, propellant (f32), g_strain, heat, energy, ammo ×2, weapon_ready, charge, parts ×6, zero_strain, zero_mode, flags, ambac/thrust factors, respawn_in |
| ZERO (1 + ~203 bits) | source_jev, advice_age, threat_count, per threat {slot, 7 × p}, rec_target + p, rec_maneuver + p, threat_level + confidence, flanked, has_solution, solution (oct 2×12), hit_p |
| events | repeated `[1][event]`, closed by `[0]` |
| entities | repeated `[1][entity]` (204 bits each), closed by `[0]` |

Entity record: slot (10), generation (2), frame (4), faction (3), pilot kind (2), position (63),
rotation (32), velocity (42), aim (18), flags (10), and 6 part-armour buckets (3 bits each, 0–7).

Events carry a 3-bit kind and an 8-bit age (ticks before the snapshot):

| Event | Payload |
|---|---|
| BeamSpawn | id, shooter, weapon, shot_seq, origin, velocity (direction + speed) |
| Hit | id, target, part, shooter, weapon, damage fraction |
| Kill | id, victim, killer |
| Leave | slot: left your sensors (idempotent, no id) |
| Clash | id, a, b |
| Seizure | id, pilot, active |

Events repeat in every snapshot until the client acks one that carried them. `id` (the low 16 bits
of the event sequence) lets clients de-duplicate the repeats.

## Control stream

Frames are `[u16 LE payload length][u8 tag][payload]`, byte-aligned.

| Tag | Message | Direction |
|---|---|---|
| 1 | Hello {version, pilot kind, frame, faction, name ≤ 16 B} | client → server (first frame) |
| 2 | Welcome {version, client slot, tick, tick_hz, sector, zero_allowed, max_datagram, field_seed, field_rocks} | server → client |
| 3 | Reject {reason: version / full / bad hello} | server → client |
| 4 | Roster {entity slot, pilot kind, name (empty = left)} | server → client |
| 5 | Respawn {frame} | client → server |
| 6 | Bye {reason} | either |

Pilots claiming to be a server-side Mobile Doll are downgraded to `Agent`.

The Welcome's `field_seed` (u32) and `field_rocks` (u16) name the sector's debris field: clients
build it with `bc_sim::field::Field::generate(field_seed, field_rocks)`, identical to the
server's, and predict their suit against it.
