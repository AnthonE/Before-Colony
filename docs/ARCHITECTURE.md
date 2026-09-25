# Before Colony: architecture

```
 browser (Bevy 0.19 → wasm32)                          server (Rust, one process)
┌──────────────────────────────┐   WebTransport   ┌────────────────────────────────────────────────┐
│ bc-client  (render, HUD, FX) │  (HTTP/3, QUIC)  │ tokio (2 workers)                               │
│   └ bc-client-core           │ ── datagrams ──▶ │  session tasks ──rtrb SPSC per slot──┐          │
│       prediction (bc-sim)    │ ◀─ datagrams ─── │  control stream   ArrayQueue control │          │
│       interpolation, world   │ ── control ────▶ │  oracle worker ◀─rtrb─┐              ▼          │
└──────────────────────────────┘    stream        │                       │   ┌──────────────────┐ │
 agents: bc-bot (native, wtransport)              │  axum dev HTTP        └── │ sector-0 thread  │ │
   └ bc-client-core (same code path)              │  (/, /cert-hash,          │  bc-sector       │ │
                                                  │   /status)                │   └ bc-sim tick  │ │
                                                  │                           └──────┬───────────┘ │
                                                  │  egress-0 thread ◀─rtrb bytes────┘ unpark()    │
                                                  │   send_datagram                                │
                                                  └────────────────────────────────────────────────┘
```

## Crates

| Crate | Kind | Role |
|---|---|---|
| `bc-proto` | `no_std`, **no `alloc`** | Wire format: bit packing, quantization, input/snapshot/event/control codecs. It *cannot* allocate. |
| `bc-sim` | `no_std` + `alloc` at construction only | The simulation: flight, weapons, damage, lag comp, sensors, Mobile Doll AI, ZERO. Shared by the server and the browser. |
| `bc-sector` | std, no tokio | The hot loop: a paced thread, lock-free queues, jitter buffers, interest, snapshot encoding, metrics. |
| `bc-zero` | std + tokio | Tactical oracles off the hot path: the `TacticalOracle` trait, `JevOracle`, the worker. |
| `bc-client-core` | std, no transport | Client state machine for the browser *and* bots: clock, inputs, prediction, interpolation, world model, `DollBrain`. |
| `bc-server` | bin + lib | WebTransport sessions, egress thread, roster, dev HTTP, `/status`. |
| `bc-bot` | lib + bins | Bot SDK (`BotClient`), `mobile_doll` example agent, `bc-swarm` load tester. |
| `bc-client` | wasm32 bin | Bevy app: scene, suits, camera, input, HUD, FX, ZERO overlay. |
| `bc-alloc` | lib | Counting global allocator: proves the tick never allocates and counts violations in production. |

## The hot path: no locks, no allocations

"Hot path" means the sector tick: draining inputs, the simulation step, and encoding snapshots for
every client. The rules, and how they are enforced:

| Rule | Enforcement |
|---|---|
| No heap allocation after construction | `bc-sim` storage is `Box<[T]>` sized in `Sim::new`, and `storage.rs` is the only file allowed to allocate. The `no_alloc` tests (`bc-sim`, `bc-sector`) wrap 1,000+ ticks of 64 pilots and 256 dolls in `bc_alloc::count` and assert **0** heap operations. |
| No locks | `bc-proto` and `bc-sim` are `no_std`, where `Mutex` does not exist. `bc-sector`'s `clippy.toml` bans Mutex, RwLock, Condvar, mpsc, Rc, String, HashMap, VecDeque, `Vec::push`, `Box::new`, `vec!`, `format!` and `println!`, as a hard error under `-D warnings`. |
| In production too | The server binary installs `CountingAlloc` and marks every tick as a hot region; `/status` reports `hot_path_allocations`. It stayed at **0** in a 64-bot, 256-doll swarm. |
| Cross-thread traffic only through preallocated lock-free queues | An `rtrb` SPSC input ring per client slot, whose producer is carried by a `SlotLease`. A crossbeam `ArrayQueue` for control and free leases. An `rtrb` byte ring per slot for outbound packets. `rtrb` rings for oracle pictures and advice. |
| The sector never wakes tokio | tokio's remote wake takes a mutex. The sector instead `unpark()`s its egress OS thread (a futex) once per tick, and the egress thread calls `send_datagram`. |

quinn, rustls and tokio allocate and lock internally. That is fine, because they live only on the
network threads.

**Measured:**
- Tick with 64 pilots (32 running ZERO) and 256 Mobile Dolls in a dense fight: p50 1.15 ms, p99
  2.9 ms, against a 33 ms budget at 30 Hz.
- Live server under a 64-bot swarm (bots on the same 4-core machine): histogram p99 ≤ 4 ms, worst
  tick 18 ms, 0 overruns.

## Tick pipeline (`Sector::tick`)

1. **Control:** Join (claim a suit at the faction's spawn), Leave (release it), Respawn frame.
2. **Inputs:** drain each slot's ring into a 64-slot jitter buffer, and process acks.
3. **Oracle advice** in, with a 15-tick time-to-live.
4. **Apply inputs** for tick `T`: the client's command if it arrived; otherwise the last one with fire
   cleared (the same view delay). After 8 silent ticks the suit goes hands-off.
5. **`Sim::step`:**
   1. Mobile Doll AI (re-plans every 3rd tick, staggered). A ZERO seizure overrides the pilot.
   2. Flight: AMBAC/RCS, thrust, propellant, G-strain. Wrecks drift.
   3. Rebuild the spatial hash (counting sort, 128 m cells).
   4. Record lag-comp history, so `history[T]` is exactly snapshot `T`.
   5. Weapons: charge, heat, energy, arm cone, magnetism, spawn, lag-comp catch-up.
   6. Projectile sweeps against per-part capsules. Saber arcs, 3 sub-steps per tick, with clashes.
   7. Damage resolves in order; limbs are lost, overflow spills to the torso, suits die.
   8. Heat, energy, ZERO strain (seizure and lockout), respawns.
   9. ZERO rollouts (staggered every 3 ticks per pilot).
6. **Tactical pictures** for ZERO pilots (≈4 Hz), only when an external oracle is attached.
7. **Snapshots** for each client, straight into its ring. The egress thread is unparked.

## Netcode

- **Transport.** WebTransport (HTTP/3 over QUIC). Unreliable datagrams carry inputs and snapshots;
  one reliable bidirectional stream carries the handshake, roster and respawns.
  - Dev servers use a self-signed ECDSA P-256 certificate, valid 14 days. Browsers pin it with
    `serverCertificateHashes`, served at `/cert-hash`. Production should use a real certificate.
- **Inputs.** One `InputCmd` per tick. Each packet repeats the last 4 commands, so a single loss
  costs nothing. A client that sends several ticks at once sends them as overlapping windows, so
  every command still travels twice. Toggles (flight assist, ZERO) are sent as states, never as
  presses. The client quantizes its own commands before predicting with them, so it simulates
  exactly what the server decodes.
- **Clock.**
  - The client estimates server time from snapshots (asymmetric filter, RTT/2 compensation).
  - RTT comes from the echo of its latest input packet minus how long the server held it. A hold
    of 255 ms is saturated and is not used as a sample.
  - It runs its input clock `lead` ticks ahead of the server. It steers `lead` so that the *lowest*
    input buffer the server reports over the last second (`input_health`) stays at about 2 ticks:
    it lengthens `lead` at once when the buffer runs low, and shortens it slowly. A steady sender
    ends up with a short lead. A bursty one (a slow agent, a tab rendering at 2 fps) gets a long
    enough lead that its commands still arrive in time.
- **Browser network loop.** The browser runs the network loop on an 8 ms timer as well as once per
  rendered frame. Receiving, clock sync, the autopilot and sending inputs therefore keep their
  30 Hz cadence even when rendering is slow. Datagrams are stamped with their arrival time by a
  receive task, so timing never depends on the frame rate.
- **Prediction.** The own suit is stepped with `bc_sim::flight` and reconciled on every snapshot by
  replaying the unacknowledged commands. Corrections are blended out visually; respawns snap.
  Measured error over a 100 ms-RTT, 5%-loss link: p99 **0.2 mm**.
- **Interpolation.** Everyone else is drawn `interp_delay` ticks (2–6, adaptive) in the past, with
  Hermite interpolation of position and velocity and normalised lerp of rotation.
- **Lag compensation.** A shot carries the shooter's view time (`view_tick_q4`). The projectile is
  flown through the recorded history from that time to now, at most 8 ticks. During catch-up, flight
  step `k` is tested against `history[view + k]`; afterwards it continues against the present.
  Either way, it meets targets where the shooter saw them.
  - Brains (the Mobile Doll brain in bots and the browser autopilot) aim at targets as they will
    be at the tick the server resolves the shot against (`InputContext::resolve_tick`), which is
    later than the view when the view is more than 8 ticks old.
- **Snapshots.**
  - Every datagram fits in `min(1100 B, the connection's max datagram)`, never fragmented.
  - Contents: header, full-precision own state, ZERO info, events repeated until acked (beam spawns,
    hits, kills, clashes, seizures, "left your sensors"), then as many entities as fit, chosen by a
    per-client priority accumulator over what that client's sensors can see.
  - About 33 KB/s per client at 30 Hz.
- **Beams** are one spawn event each: they fly straight at constant velocity, so every client draws
  the whole flight from it. The shooter draws its own shot immediately and matches the server's
  event by `shot_seq`.
- **Interest management** is the sensor model: clients only learn about what their suit can
  detect.

## AI layers

| Layer | Rate | Where | What |
|---|---|---|---|
| Reflex | every tick | `bc-sim` (deterministic, allocation-free) | Mobile Doll utility AI, fire control, ZERO rollouts and the local oracle |
| Tactical | ≈4 Hz per ZERO pilot | `bc-zero` worker (async) | `TacticalOracle`: typed advice with probabilities. `JevOracle` runs a batched Jev call |
| Strategic | seconds | external agents | Bot SDK today; MCP and a Python gym on the roadmap |

**Jev integration.**
- The request is `POST /v1/systemone` with one `state` document and every question at once:
  - target (choice)
  - each threat's next maneuver (choice over the 7 hypothesis labels)
  - evasive action (choice)
  - threat level (score: low / moderate / high / lethal)
  - flanked (noul)
- The state is named buckets ("close (under 1 km)", "closing fast", "left arm destroyed") because
  Jev is weak at numeric precision. All arithmetic stays in the simulation.
- The answer is parsed tolerantly: score probabilities are keyed by level index, and noul answers
  carry no confidence.
- It is blended with the local oracle as `p ∝ p_local^½ · p_jev^½`.
- Failure handling: 400 ms timeout; a circuit breaker opens after 3 failures in a row, for 10 s.
- The API key stays on the server.

## Determinism

The server (native, glam `scalar-math`) and the browser (wasm32, no SIMD) run the same `bc-sim`
code. Every transcendental function goes through the pure-Rust `libm` (`bc_sim::math`). The
`determinism` test hashes 600 ticks of a busy scenario and gets the same golden value on x86_64 and
on wasm32 (under Node, via `wasm-bindgen-test-runner`). Never enable glam's `fast-math` or wasm
`simd128`.

## Verification

| Test | Proves |
|---|---|
| `bc-proto/tests/roundtrip.rs` | Codecs round-trip within ½ LSB; decoders never panic on arbitrary bytes. |
| `bc-sim/tests/no_alloc.rs`, `bc-sector/tests/no_alloc_sector.rs` | 0 heap operations per tick with 64 clients + 256 dolls. |
| `bc-sim/tests/determinism.rs` | Identical state hash on native and wasm32. |
| `bc-sim/tests/{flight,combat,fire_control,lagcomp,mobile_dolls,zero}.rs` | Rocket equation, FA, blackout, no tunnelling, arm loss, charge, sabers and clashes, lag comp (and its clamp), dolls fight to a kill, ZERO accuracy, calibration, seizure, magnetism. |
| `bc-sector/tests/netcode.rs` | Over a simulated 100 ms / 5%-loss link: prediction error and clock sync, and a client that sends inputs only twice a second still has an accurate RTT and commands that arrive in time. |
| `bc-server/tests/{echo,duel,oracle}.rs` | A real server over real WebTransport: echo; two agents find and fight each other; Jev advice reaches a ZERO pilot. |
| `bc-zero/tests/jev_mock.rs` | Jev request contract, parsing, timeout, 429/529 breaker, garbage. |
| `bc-sim/benches/tick.rs` | Tick percentiles. |
| `e2e/tests/{spike,slice}.spec.ts` | The Bevy wasm client in Chromium: transport, then the full slice (autopilot flies, fights, sees agents and ZERO futures; the server confirms hits and 0 hot-path allocations). |

## Scaling path

1. **Sector per thread** (now): many sectors per process, each with its own sim, rings and egress
   thread.
2. **Sector per process**, with a gateway that routes sessions by sector. Handoff moves the pilot
   record through the gateway when a transfer orbit completes.
3. **TiDi.** When a sector's tick overruns, stretch wall-clock time per tick and publish `tidi_pct`
   (the header field already exists).
4. **Encoding off the sim thread.** Snapshot encoding is embarrassingly parallel per client: move it
   to worker threads that read a double-buffered world, if client counts demand it.
