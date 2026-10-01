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
| `bc-client-core` | std, no transport | Client state machine for the browser *and* bots: clock, inputs, prediction, interpolation, world model, the salvage view, `DollBrain` and `MinerBrain`; the link state machine (dial, sign in, redial), the pointer, settings, first-flight hints, the HUD's and page's palette; survival: the hangar as the server tells it (`hangar`), the bay's layout (`bay`) and the first-person walker and its guide (`walker`). |
| `bc-econ` | std | The economy, off the hot path: items (ores, materials, each line's parts, weapons), recipes and the colony's valuations (`catalogue`), stores, the suit in the bay and what it launches as (`suit`), the fabricator's and foundry's job queues on the wall clock (`fab`), the Colony Exchange's order books with the colony as a market maker (`exchange`), a pilot's hangar and every request it takes (`hangar`), and the JSON messages (`wire`). |
| `bc-auth` | `no_std` | Wallet sign-in: the EIP-4361 message both sides build, EIP-55 addresses, and (features) the server's signature check and a local wallet for agents and tests. The browser builds only the message. |
| `bc-sound` | lib | The sound bank, generated in code (no audio files): cues, the mixer (culling, cooldowns, voices, panning), the cockpit's loops and alarms, the score (the title theme on the Super Famicom's sound chip, in software). Pure Rust; the browser plays it through Web Audio. |
| `bc-server` | bin + lib | WebTransport sessions (`net/session.rs`: a pilot's session from slot to goodbye, and survival's hangar, sorties and requests), sign-in and the pilot registry (`pilots`: records behind a `PilotStore`, in memory or files, one session per wallet, resume tokens), the colony's exchange (`market`), egress thread, roster, dev HTTP, `/status`. |
| `bc-bot` | lib + bins | Bot SDK (`BotClient`), `mobile_doll` and `miner` example agents, `bc-swarm` load tester. |
| `bc-client` | wasm32 bin | Bevy app: procedural jointed suits (every frame's kit, animated from its `MeleeSpec`s), sky, colony and field (custom shaders), particles and effects (missiles, stream tracers, flame, jammer shimmer; sunlit smoke, burning wrecks, beams that keep a minimum width on screen), the camera's look (a grade on every tier, a lens vignette, a flare that rocks and suits hide), camera (chasing, or from the cockpit: a wraparound cockpit hung on the camera, its monitors showing the instruments rendered into one texture by a second UI camera, and a world-aligned radar sphere; `cockpit`), input with lock assist, HUD in the mobile-suit monitor style (chamfered plates and hazard-striped cautions from a small UI material, `ui_panel`; amber target corners, off-screen chevrons, a damage silhouette; one set of instruments drawn in the screen's corners or onto the cockpit's monitors; the page's fonts and palette: `bc_client_core::palette`), ZERO overlay, offline showcase scenes; the hangar bay drawn (`hangar`), with its haze, a flood light's shadow and contact shadows on the deck (`shade`), on foot in it with the launch and homecoming sequences (`onfoot`), and its terminals' data for the page (`terminal`). |
| `bc-model` | lib | The suits' procedural designs on a shared 24-bone rig, and the sockets their kits are drawn from (muzzles, blades, the Dragon Fang, missile hatches, the cockpit's eye), checked against each frame's hit capsules; ambient occlusion baked into every vertex from the whole suit at rest (`ao`); the cockpit seen from the seat (`cockpit`: the shell, its monitors' faces and their regions of the shared screen texture, the radar's place). |
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
- Tick with 64 pilots (32 running ZERO) and 256 Mobile Dolls in a dense fight: p50 1.1 ms, p99
  2.1–3.0 ms across runs, against a 33 ms budget at 30 Hz.
- Tick with 64 Gundam pilots duelling in every playable frame (jammers, Neo-Birds, Full Opens,
  about 200 missiles in the air) and 256 dolls: p50 0.84 ms, p99 2.5 ms.
- Live server under a 64-bot swarm flying all six frames (bots on the same 4-core machine):
  histogram p99 ≤ 4 ms, 0 hot-path allocations; worst tick 8–11 ms in most runs, and one run
  with a single 60 ms tick (1 overrun), when the machine stalled the sector thread. 0–53 of
  115,200 snapshots were dropped at a full egress ring across three runs.

## Tick pipeline (`Sector::tick`)

1. **Control:** Join (wake the suit a signed-in pilot left asleep, if it's still there; else claim a
   suit at the faction's spawn, clearing the longest-asleep sleeper if the sector is full), Leave
   (release it), Sleep (a signed-in pilot left: the suit stays, asleep), Respawn frame.
2. **Inputs:** drain each slot's ring into a 64-slot jitter buffer, and process acks.
3. **Oracle advice** in, with a 15-tick time-to-live.
4. **Apply inputs** for tick `T`: the client's command if it arrived; otherwise the last one with fire
   cleared (the same view delay). After 8 silent ticks the suit goes hands-off.
5. **`Sim::step`:**
   1. Mobile Doll AI (re-plans every 3rd tick, staggered). A ZERO seizure overrides the pilot.
   2. Specials: their cooldowns run down; the Hyper Jammer follows MODE and drains energy; Full
      Open runs; a transformable frame changes form on MODE (`bc_sim::transform`, which the
      client's predictor runs too).
   3. Flight: AMBAC/RCS, thrust, propellant, G-strain, swept against the rocks. Wrecks drift, and
      so do sleepers (`sim/sleep.rs`: no flight assist, no attitude hold), unless parked on a rock.
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
6. **Sleepers' fates** (destroyed, or cleared for room) onto the notes queue, for the server to tell
   their pilots.
7. **Tactical pictures** for ZERO pilots (≈4 Hz), only when an external oracle is attached.
8. **Snapshots** for each client, straight into its ring. The egress thread is unparked.

## Netcode

- **Transport.** WebTransport (HTTP/3 over QUIC). Unreliable datagrams carry inputs and snapshots;
  one reliable bidirectional stream carries the handshake (and wallet sign-in), roster and respawns.
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
  - Snapshots stamped late by a page that stalled arrive in a burst, so a late one moves the
    estimate back at most 0.05 tick; only 15 far-late ones in a row (the server's clock really
    moved) move it back all the way.
  - What is drawn runs on eased clocks. The time the own suit is drawn at (the input clock) and the
    time everyone else is (the view) follow their estimates on a critically damped spring,
    snapping only when more than 4 ticks off, so a correction never makes anything on screen
    lurch. Commands are scheduled on the raw estimate; the view time they carry for lag
    compensation is the eased one that was drawn.
- **Browser network loop.** The browser runs the network loop on an 8 ms timer as well as once per
  rendered frame. Receiving, clock sync, the autopilot and sending inputs therefore keep their
  30 Hz cadence even when rendering is slow. Datagrams are stamped with their arrival time by a
  receive task, so timing never depends on the frame rate.
- **Prediction.** The own suit is stepped with `bc_sim::flight` and reconciled on every snapshot by
  replaying the unacknowledged commands. Measured error over a 100 ms-RTT, 5%-loss link: p99
  **0.1 mm**.
  - A tick the client sent nothing for (a stall) is flown on the server's stand-in for it
    (`InputCmd::stand_in`: the last command again without firing, then hands-off), in the replay
    and as it goes.
  - Every tick flown is kept, and the own suit is drawn between the last two, a tick behind the
    input clock: position and velocity lerped (exact for the integrator), rotation nlerped. Drawn
    tick by tick instead, a suit at 540 m/s would jump 18 m at a time against the smoothly moving
    camera.
  - A correction is measured where the suit was drawn, and blended out on a critically damped
    spring in position and rotation, so the drawn suit keeps its place and its pace at the moment
    of the news and bends onto the new path. Only a new life, or a relocation of more than 150 m,
    cuts (and cuts the camera).
  - A change of form is predicted too. The predictor seeds the form from the snapshot (its frame,
    and the special timer counting a change down), then steps it with each replayed command before
    flying it, as the server does. The thrust cut is applied on top of the replicated thrust
    factor on both sides, so a Wing Zero changing form every 3 s still predicts to p99 0.1 mm.
  - Lag compensation resolves shots against the capsules of the form a suit has now, not the one
    it had at the shooter's view time (a change takes 24 ticks; the rewind is at most 8).
  - The arms are predicted too (`bc_sim::arms`). A blade's lunge drives the suit, and busy arms (a
    strike under way, or a weapon fired in the last 6 ticks) cut AMBAC's turning to 0.6 of what
    damage leaves it. The snapshot carries the arms: the strike's phase and timer, how lately a
    weapon fired, each mount's wait and a missile salvo under way. The predictor rolls them on
    with each command as the server's specials, weapons and melee steps do (choosing and advancing
    a strike are the same functions on both sides). So the ticks it flies ahead of any news lunge,
    and turn, as the server's suit will: over the bad link, a pilot swinging every 2 s and firing
    bursts between is first predicted to p99 0.5 mm, where carrying the snapshot's arms over the
    ticks flown ahead missed by 2–4 m on every swing.
  - G-strain arrives exactly (`f32`), so a blackout starts on the same tick on both sides.
  - While ZERO flies the suit (a seizure) the pilot's commands aren't what it flies, so it's drawn
    from the server's state carried on at its velocity and spin, and each snapshot's correction
    blends out.
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
  - About 31 KB/s per client at 30 Hz (measured under the swarm).
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
- **Sign-in and pilots.** A Hello may ask to sign in with a wallet (SIWE; see `PROTOCOL.md`). The
  session task does the challenge, the signature check and the pilot registry's bookkeeping
  (`bc-server/src/pilots.rs`) before the sector hears of the pilot, so none of it touches the tick.
  Records go through the `PilotStore` trait: in memory today, plain serde data so a Redis or Mongo
  store can keep them as they are. One session per wallet: a new one takes the pilot over.
- **Sleepers.** A signed-in pilot's session ends with `Control::Sleep` instead of `Leave`, and the
  record keeps which suit (slot and generation, for this server run). The next session's Join
  carries it back (`Comeback`), and the sector wakes it if it's still there. The sector reports
  sleepers destroyed or cleared on a lock-free queue; a server task turns them into news for their
  pilots (`pilots.rs`), read out as a Notice when they're back.

- **Survival: the hangar, off the tick.** A pilot's hangar (credits, stores, the suit in the bay,
  job queues) lives in their record and in their session task, never in the sector. Its messages
  ride the control stream as JSON frames of their own (`bc_econ::wire`, frame tag 11): requests
  up, and the hangar, the exchange and the watched book down whenever they change (the market at
  most every 2 s). A launch hands the sector a `Loadout` on the Join (`Control::Join { launch }`):
  the suit enters the sector as it was built, at the docking hub's mouth. Docking is
  `Control::Dock`; the sector answers on the slot's report ring (`SlotLease::reports`, an
  `ArrayQueue` of 8, preallocated) with a `Homecoming` (what's left of the suit, its hold, what it
  held, its bounties) or a refusal, and reports a suit lost the same way. The session applies them
  to the hangar and saves the record. The exchange is one `bc_econ::Exchange` behind a mutex in the
  server (`market.rs`): the tick never sees it. With `--data-dir`, records and the exchange are
  files (written atomically; the exchange every minute and on shutdown).

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
| `bc-sector/tests/salvage_net.rs` | Over the same link: chunks reach the client exactly as the server moves them, across bounces; chunks that go leave the client; a kill hands its wreck to its hulk; changed rocks arrive; a miner under survival rules docks and brings its haul home. |
| `bc-sim/tests/survival.rs`, `bc-sector/tests/survival_net.rs` | A suit launches as it was built (parts missing, worn, weapons not fitted that don't fire, what's in the tank); it docks only at rest in the dock, awake, and goes home with its hold and what it holds; the colony pays bounties on Mobile Dolls; a pilot shot down stays down. Through the sector: no loadout, no suit; launch, dock and home with the hold on the slot's report ring; a suit lost is reported, then its pilot goes home. The `no_alloc` tests cover survival ticks too. |
| `bc-econ` tests (`tests/ledger.rs`) | Recipes, fitting and stripping, repairs and scrap, jobs on the clock, the exchange's matching, escrow and the colony's desk; a property test that no sequence of trades, cancels and colony drift makes or loses a credit or a kilogram. |
| `bc-server/tests/hangar.rs` | A real server under survival rules: a pilot starts in their bay, fabricates, fits, trades, launches, docks and comes home; a signed-in pilot's hangar outlives the server (`--data-dir`); a suit left out there is woken in, or towed home. |
| `bc-client-core` walker tests | The first-person body stands, walks, runs into walls, jumps and falls from the catwalk; the guide walks from the airlock to every place in the bay and back. |
| `bc-sector/tests/netcode.rs` | Over a simulated 100 ms / 5%-loss link: prediction error and clock sync (in open flight, ramming and sliding round a rock, damaged, changing into Neo-Bird and back every 3 s, striking and firing, and after a 1.5 s stall), and for the strikes the ticks flown ahead of any news too; a client that sends inputs only twice a second still has an accurate RTT and commands that arrive in time; and drawn like the browser at 60 and 144 Hz, a Wing Zero sprinting and stopping never steps back along its flight, changes pace only as its acceleration does, and doesn't surge against the chase camera. |
| `bc-client-core/tests/arms.rs` | Seeded from any snapshot of a Gundam striking, firing, launching salvos, opening fire in Full Open or changing form, the client's prediction keeps its arms in step with the server's (the strike, busy arms, the lunge) tick for tick, and flies to within millimetres of it. |
| `bc-sim/tests/transform.rs` | MODE folds Wing Zero into Neo-Bird and back over 24 ticks, weapons down (a charge is lost) and thrust cut; the bird cruises faster; ZERO stays engaged; a bird that dies respawns as Wing Zero. |
| `bc-server/tests/{echo,duel,oracle}.rs` | A real server over real WebTransport: echo; two agents find and fight each other; Jev advice reaches a ZERO pilot. |
| `bc-zero/tests/jev_mock.rs` | Jev request contract, parsing, timeout, 429/529 breaker, garbage. |
| `bc-sim/benches/tick.rs` | Tick percentiles. |
| `e2e/tests/{spike,slice}.spec.ts` | The Bevy wasm client in Chromium: transport, then the full slice (autopilot flies, fights, sees agents and ZERO futures; the server confirms hits and 0 hot-path allocations). |
| `e2e/tests/frames.spec.ts` | Each Gundam in the browser against the server's dolls: the autopilot flies its kit until the server's per-pilot counters (`/status`) and the client's (`window.__bc`) show it: Heavyarms' Full Open and missiles, Deathscythe jamming and reaping, Sandrock's missiles and shotels, Shenlong's fang or flame, Wing Zero out as Neo-Bird and back. |
| `e2e/tests/gfx.spec.ts` | Every showcase scene renders cleanly, `gundams` included (Full Open's salvo, the jammer, the shotels and Cross Crusher, the fang at full reach, the flamethrower, Neo-Bird), and the hangar bay. |
| `e2e/tests/hangar.spec.ts` | Survival in the browser: the pilot comes in through the airlock, walks to each terminal and uses it, fabricates and trades through the panels, boards at the hatch, launches through the bay doors into space, and docks home again. |
| `bc-model` tests | Every design stays within 2.6 m of its hit capsules (Neo-Bird's own), no two frames share a mesh, every kit has the sockets it's drawn from, the cockpit's eye sees out past the suit it rides (nothing drawn inside the near plane or across the crosshair's 15°), and the triangle budgets; the cockpit keeps a clear box round the crosshair and its screens share one texture without stretching; the occlusion bake (open where nothing's near, shaded where pieces meet, and quick enough for startup). |

## Scaling path

1. **Sector per thread** (now): many sectors per process, each with its own sim, rings and egress
   thread.
2. **Sector per process**, with a gateway that routes sessions by sector. Handoff moves the pilot
   record through the gateway when a transfer orbit completes.
3. **TiDi.** When a sector's tick overruns, stretch wall-clock time per tick and publish `tidi_pct`
   (the header field already exists).
4. **Encoding off the sim thread.** Snapshot encoding is embarrassingly parallel per client: move it
   to worker threads that read a double-buffered world, if client counts demand it.
