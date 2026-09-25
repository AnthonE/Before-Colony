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
| `bc-sim` | `no_std` + `alloc` at construction only | The simulation: flight, weapons, damage, lag comp, sensors, Mobile Doll AI, ZERO, the debris field, salvage and mining. Shared by the server and the browser. |
| `bc-sector` | std, no tokio | The hot loop: a paced thread, lock-free queues, jitter buffers, interest, snapshot encoding, metrics. |
| `bc-zero` | std + tokio | Tactical oracles off the hot path: the `TacticalOracle` trait, `JevOracle`, the worker. |
| `bc-client-core` | std, no transport | Client state machine for the browser *and* bots: clock, inputs, prediction, interpolation, world model, the salvage view, `DollBrain` and `MinerBrain`. |
| `bc-server` | bin + lib | WebTransport sessions, egress thread, roster, dev HTTP, `/status`. |
| `bc-bot` | lib + bins | Bot SDK (`BotClient`), `mobile_doll` and `miner` example agents, `bc-swarm` load tester. |
| `bc-client` | wasm32 bin | Bevy app: procedural jointed suits (every frame's kit, animated from its `MeleeSpec`s), sky, colony and field (custom shaders), particles and effects (missiles, stream tracers, flame, jammer shimmer), camera, input with lock assist, HUD, ZERO overlay, offline showcase scenes. |
| `bc-model` | lib | The suits' procedural designs on a shared 24-bone rig, and the sockets their kits are drawn from (muzzles, blades, the Dragon Fang, missile hatches), checked against each frame's hit capsules. |
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
   2. Specials: their cooldowns run down; the Hyper Jammer follows MODE and drains energy; Full
      Open runs; a transformable frame changes form on MODE (`bc_sim::transform`, which the
      client's predictor runs too).
   3. Flight: AMBAC/RCS, thrust, propellant, G-strain, swept against the rocks. Wrecks drift.
   4. Chunks (loose ore, limbs, hulks): free ones drift on closed-form segments, bounce off the
      colony and rocks, and expire.
   5. Rebuild the spatial hash (counting sort, 128 m cells).
   6. Record lag-comp history, so `history[T]` is exactly snapshot `T`.
      Then missile locks build or fall apart on each launcher-carrying suit's designation.
   7. Guns: charge, heat, energy, arm cone, magnetism, spawn, lag-comp catch-up (rocks stop it,
      and are worn down by it). The flamethrower (`sim/flame.rs`) burns what's in its cone every
      few ticks while it's lit, without lag compensation. A weapon on an arm the Dragon Fang has
      taken along waits.
   8. Projectile sweeps against per-part capsules (skipping parts that are gone) and rocks, which
      shots wear down until they shatter into ore. Then missiles (`sim/missile.rs`): the seeker
      (every third tick), proportional navigation on the motor's Δv budget, and a proximity fuse
      swept against enemy suits, rocks and the colony, each end a `MissileBurst`. A missile pool
      of 1 024 is allocated with the sim; a launch into a full pool fizzles.
   9. Melee (`sim/melee.rs`), driven by each blade's `MeleeSpec`: swings sweep an arc (sub-steps
      per tick), the Dragon Fang's thrust drives its head out along the aim, twin weapons strike
      with a blade in each hand. Blades that parry clash; a stroke chips ore off a rock and cuts a
      part off a hulk.
   10. Damage resolves in order: limbs come off as chunks, overflow spills to the torso, suits die
      and leave hulks (spilling their holds).
   11. Salvage: grab, stow, throw, jettison, and sales at the dock. Presses are edges against the
       previous tick's buttons, so this runs before they're recorded.
   12. Heat, energy, ZERO strain (seizure and lockout), respawns.
   13. ZERO rollouts (staggered every 3 ticks per pilot).
   12. Shattered rocks grow back once no suit is near (checked every 30 ticks).
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
  - A change of form is predicted too. The predictor seeds the form from the snapshot (its frame,
    and the special timer counting a change down), then steps it with each replayed command before
    flying it, as the server does. The thrust cut is applied on top of the replicated thrust
    factor on both sides, so a Wing Zero changing form every 3 s still predicts to p99 0.2 mm.
  - Lag compensation resolves shots against the capsules of the form a suit has now, not the one
    it had at the shooter's view time (a change takes 24 ticks; the rewind is at most 8).
- **Interpolation.** Everyone else is drawn `interp_delay` ticks (2–6, adaptive) in the past, with
  Hermite interpolation of position and velocity and normalised lerp of rotation.
- **Lag compensation.** A shot carries the shooter's view time (`view_tick_q4`). The projectile is
  flown through the recorded history from that time to now, at most 8 ticks. During catch-up, flight
  step `k` is tested against `history[view + k]`; afterwards it continues against the present.
  Either way, it meets targets where the shooter saw them.
  - A melee strike samples the world over its stroke, each sample rewound by the pilot's latency
    when the strike began (not frozen at that moment, which would leave a fast target behind).
    Its broad phase is widened by how far a suit can move in the rewind, as a shot's is.
  - Brains (the Mobile Doll brain in bots and the browser autopilot) aim at targets as they will
    be at the tick the server resolves the shot against (`InputContext::resolve_tick`), which is
    later than the view when the view is more than 8 ticks old.
- **Snapshots.**
  - Every datagram fits in `min(1100 B, the connection's max datagram)`, never fragmented.
  - Contents: header, full-precision own state, ZERO info, events repeated until acked (beam spawns,
    hits, kills, clashes, seizures, parts coming off, rocks shattering, "left your sensors"),
    changed rocks, missiles in flight (at most 12: those tracking the client, then the nearest
    within 5 km), then as many entities as fit, chosen by a per-client priority accumulator over
    what that client's sensors can see, then salvage chunks.
  - About 33 KB/s per client at 30 Hz.
- **Chunks and rocks: dirty until acked.** Each snapshot's record lists the chunks (id, generation,
  version) and rocks (id, version) it carried; an ack promotes them to what the client holds. A
  chunk is sent whenever what the client holds differs from the server's (a new segment, a grab),
  nearest first, and a Gone record when it leaves the client's range (3 km, or 3.3 km for one it
  already has) or the world. A drifting chunk costs nothing after that: its segment is closed-form,
  and the server moves it on exactly the quantized segment it sent, so every client computes the
  same pose to the bit. Events and entities leave room for up to 16 rocks and 6 objects when some
  are waiting.
- **Wrecks become hulks.** A destroyed suit's wreck moves as its hulk does. Clients draw the wreck
  (with its death blasts) while it's replicated, and the hulk after it leaves; the Kill event names
  the hulk, so it isn't drawn twice.
- **Beams** are one spawn event each: they fly straight at constant velocity, so every client draws
  the whole flight from it. The shooter draws its own shot immediately and matches the server's
  event by `shot_seq`.
- **Stream weapons** (gatlings, machine guns, vulcans) and the flamethrower send nothing per round:
  clients draw tracers and flame from the firing flags, at each weapon's speed and colour. What
  they hit arrives as Hit events.
- **Missiles** are listed in each snapshot (those tracking the viewer first) and interpolated;
  their bursts are events. The client counts what it sees of each kit (`World::kit`) as snapshots
  arrive, so tests don't depend on how fast the page renders.
- **Interest management** is the sensor model: clients only learn about what their suit can
  detect. One question, `Sim::detects(viewer, j)`, answers it for replication, for Mobile Doll and
  ZERO perception and for locks, so the Hyper Jammer hides a suit from all of them at once.
  Designations (lock targets) are raw client input, so they're read through `Sim::designation`,
  which keeps only a live hostile on the designator's sensors.

## AI layers

| Layer | Rate | Where | What |
|---|---|---|---|
| Reflex | every tick | `bc-sim` (deterministic, allocation-free) | Mobile Doll utility AI, fire control, ZERO rollouts and the local oracle; the kit-aware pilot (`ai::kit`, profile `PILOT`) that agents and the autopilot fly with |
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
| `bc-sim/tests/no_alloc.rs`, `bc-sector/tests/no_alloc_sector.rs` | 0 heap operations per tick with 64 clients + 256 dolls, with the Gundams duelling, and with 32 missile boats keeping 500 missiles in the air. |
| `bc-sim/tests/determinism.rs` | Identical state hash on native and wasm32, for the reference scenario, for suits flying into rocks and firing through them, for a salvage run, and for the Gundams duelling with every blade; the generated debris field is identical too. |
| `bc-sim/tests/kit_ai.rs` | The kit-aware pilot, each Gundam against three Taurus and a Virgo: Heavyarms locks on and opens fire, Deathscythe closes jammed and reaps, Sandrock fires missiles, Shenlong lands its fang and flame, Wing Zero flies out as Neo-Bird and fights unfolded. |
| `bc-sim/tests/{missiles,full_open}.rs` | A lock builds in half a second in its cone and falls apart twice as fast; a guided salvo runs down a crossing target; a target faster than the motor's Δv outruns it; a jammer breaks the seeker's hold where a plain break doesn't; missiles pass friends and burst at the end of their life; a full pool swallows launches. Full Open fires everything for 3 s, then locks the suit out and cools down. |
| `bc-sim/tests/jammer.rs` | A jamming Deathscythe leaves its enemies' sensors (past 150 m for eyes), Mobile Dolls and ZERO lose it, allies see it shimmer, locks on it drop and its own go unnoticed; firing or striking breaks it for 2 s; it drains energy and needs a fifth of it to engage. |
| `bc-sim/tests/ranged.rs` | The flamethrower burns within its cone and reach only, a round a burn, and overheats its target; the Dragon Fang takes the flamethrower's arm along; stream weapons fire without spawn events; the buster shield flies at its speed. |
| `bc-sim/tests/{content,melee}.rs` | Every table row sits at its id and the Gundams fly as designed; every blade reaches as far as its row says and mines, twin blades strike once each, the Dragon Fang thrusts where it's aimed, the Cross Crusher is Sandrock's special, only blades that parry clash, and a blade meets a target it chases at speed as its pilot sees it. |
| `bc-sim/tests/{flight,combat,fire_control,lagcomp,mobile_dolls,zero,field,salvage}.rs` | Rocket equation, FA, blackout, no tunnelling, arm loss, charge, sabers and clashes, lag comp (and its clamp), dolls fight to a kill, ZERO accuracy, calibration, seizure, magnetism; suits stop at rocks at 2 km/s and rocks stop shots; limbs come off as chunks and shots pass where they were, hulks, bounces, expiry, lighter suits. |
| `bc-sector/tests/salvage_net.rs` | Over the same link: chunks reach the client exactly as the server moves them, across bounces; chunks that go leave the client; a kill hands its wreck to its hulk; changed rocks arrive. |
| `bc-sector/tests/netcode.rs` | Over a simulated 100 ms / 5%-loss link: prediction error and clock sync (in open flight, ramming and sliding round a rock, damaged, and changing into Neo-Bird and back every 3 s), and a client that sends inputs only twice a second still has an accurate RTT and commands that arrive in time. |
| `bc-sim/tests/transform.rs` | MODE folds Wing Zero into Neo-Bird and back over 24 ticks, weapons down (a charge is lost) and thrust cut; the bird cruises faster; ZERO stays engaged; a bird that dies respawns as Wing Zero. |
| `bc-server/tests/{echo,duel,oracle}.rs` | A real server over real WebTransport: echo; two agents find and fight each other; Jev advice reaches a ZERO pilot. |
| `bc-zero/tests/jev_mock.rs` | Jev request contract, parsing, timeout, 429/529 breaker, garbage. |
| `bc-sim/benches/tick.rs` | Tick percentiles. |
| `e2e/tests/{spike,slice}.spec.ts` | The Bevy wasm client in Chromium: transport, then the full slice (autopilot flies, fights, sees agents and ZERO futures; the server confirms hits and 0 hot-path allocations). |
| `e2e/tests/frames.spec.ts` | Each Gundam in the browser against the server's dolls: the autopilot flies its kit until the server's per-pilot counters (`/status`) and the client's (`window.__bc`) show it: Heavyarms' Full Open and missiles, Deathscythe jamming and reaping, Sandrock's missiles and shotels, Shenlong's fang or flame, Wing Zero out as Neo-Bird and back. |
| `e2e/tests/gfx.spec.ts` | Every showcase scene renders cleanly, `gundams` included (Full Open's salvo, the jammer, the shotels and Cross Crusher, the fang at full reach, the flamethrower, Neo-Bird). |
| `bc-model` tests | Every design stays within 2.6 m of its hit capsules (Neo-Bird's own), no two frames share a mesh, every kit has the sockets it's drawn from, and the triangle budgets. |

## Scaling path

1. **Sector per thread** (now): many sectors per process, each with its own sim, rings and egress
   thread.
2. **Sector per process**, with a gateway that routes sessions by sector. Handoff moves the pilot
   record through the gateway when a transfer orbit completes.
3. **TiDi.** When a sector's tick overruns, stretch wall-clock time per tick and publish `tidi_pct`
   (the header field already exists).
4. **Encoding off the sim thread.** Snapshot encoding is embarrassingly parallel per client: move it
   to worker threads that read a double-buffered world, if client counts demand it.
