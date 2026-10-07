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
| `bc-sim` | `no_std` + `alloc` at construction only | The simulation: flight, weapons, damage (and the systems inside the parts), lag comp, sensors, Mobile Doll AI, ZERO, the debris field, salvage and mining; the bodies suits stand on (`bodies`, `content::landmarks`), surface contact (`ground::move_step`) and concealment. Each suit's stat sheet (`tuning`): its frame times what's broken and what's fitted, built the same way by the server and the owner's prediction. The First Colony as closed forms (`colony`): its frames (the sector's, the colony's turning one, a strip's city coordinates), its day on the tick's clock, its mirrors and its −X end, and the city's streets, blocks and buildings worked out from where you ask (`colony::city`, its tables in `content::city`). Shared by the server and the browser. |
| `bc-sector` | std, no tokio | The hot loop: a paced thread, lock-free queues, jitter buffers, interest, snapshot encoding, metrics. |
| `bc-zero` | std + tokio | Tactical oracles off the hot path: the `TacticalOracle` trait, `JevOracle`, the worker. |
| `bc-client-core` | std, no transport | Client state machine for the browser *and* bots: clock, inputs, prediction, interpolation, world model, the salvage view, `DollBrain`, `MinerBrain` and `LanderBrain`; the sector's bodies as the client knows them (`surface`), the camera's clamp to them, a walking suit's gait (`gait`) and the landmarks' meshes (`body_mesh`); the link state machine (dial, sign in, redial), the pointer, settings, first-flight hints, the objectives and their waypoints (`objectives`), the HUD's and page's palette; survival: the hangar as the server tells it (`hangar`), the bay's layout (`bay`) and the first-person walker and its guide (`walker`). The colony's city on foot: what the walker stands on (`city`), its routes along the streets (`city_nav`), the meshes of its blocks by level of detail (`city_mesh`) and the block atlas its shaders paint from (`city_atlas`). |
| `bc-econ` | std | The economy, off the hot path: items (ores, materials, each line's parts, weapons, equipment), what's broken inside a suit's parts and what overhauling it takes (`faults`), recipes and the colony's valuations (`catalogue`), stores, the suit in the bay and what it launches as (`suit`), what its life has done to its paint (`weathering`), the fabricator's and foundry's job queues on the wall clock (`fab`), the Colony Exchange's order books with the colony as a market maker (`exchange`), a pilot's hangar and every request it takes (`hangar`), the Proving Ground's board of the day's best times (`proving`), and the JSON messages (`wire`). |
| `bc-auth` | `no_std` | Wallet sign-in: the EIP-4361 message both sides build, EIP-55 addresses, and (features) the server's signature check and a local wallet for agents and tests. The browser builds only the message. |
| `bc-sound` | lib | The sound bank, generated in code (no audio files): cues, the mixer (culling, cooldowns, voices, panning), the cockpit's loops and alarms, the score (the title theme on the Super Famicom's sound chip, in software). Pure Rust; the browser plays it through Web Audio. |
| `bc-server` | bin + lib | WebTransport sessions (`net/session.rs`: a pilot's session from slot to goodbye, and survival's hangar, sorties and requests, and the Proving Ground's trainers), sign-in and the pilot registry (`pilots`: records behind a `PilotStore`, in memory or files, one session per wallet, resume tokens, suits left hidden that are put back at boot), the colony's exchange (`market`), the Proving Ground's board (`proving`), egress thread, roster, dev HTTP, `/status`. |
| `bc-bot` | lib + bins | Bot SDK (`BotClient`), `mobile_doll`, `miner`, `flaneur` (on foot in the colony) and `suit_inside` (a suit inside it) example agents, `bc-swarm` load tester. |
| `bc-client` | wasm32 bin | Bevy app: procedural jointed suits (every frame's kit, animated from its `MeleeSpec`s, walking and kneeling on a body), sky, colony, field and landmarks (custom shaders), particles and effects (missiles, stream tracers, flame, jammer shimmer; sunlit smoke, burning wrecks, beams that keep a minimum width on screen), the camera's look (a grade on every tier, a lens vignette, a flare that rocks and suits hide), camera (chasing, or from the cockpit: a wraparound cockpit hung on the camera, its monitors showing the instruments rendered into one texture by a second UI camera, and a world-aligned radar sphere; `cockpit`), input with lock assist, HUD in the mobile-suit monitor style (chamfered plates and hazard-striped cautions from a small UI material, `ui_panel`; amber target corners, off-screen chevrons, a damage silhouette; one set of instruments drawn in the screen's corners or onto the cockpit's monitors; the page's fonts and palette: `bc_client_core::palette`), ZERO overlay, offline showcase scenes; the hangar bay drawn (`hangar`), with its haze, a flood light's shadow and contact shadows on the deck (`shade`), on foot in it with the launch and homecoming sequences (`onfoot`), and its terminals' data for the page (`terminal`). The colony from outside (`colony`: its mirrors, its ends and its lights, `dots`) and inside (`city`: the city on a render layer of its own, streamed by level of detail round a floating origin, lit strip by strip, its haze and its windows), and the cap lift and the city on foot (`onfoot`). |
| `bc-model` | lib | The suits' procedural designs on a shared 24-bone rig in the anime's proportions (measured off Bandai's line art of the Leo and the Tallgeese: a small head, a short torso over hips 1.55 m above the origin, the knee three-fifths of the way down, long shins and tall feet; the sim's humanoid capsules follow it), and the sockets their kits are drawn from (muzzles, blades, the Dragon Fang, missile hatches, the cockpit's eye), checked against each frame's hit capsules; a suit put together from several frames' sections (`Parts`, `build_parts`: one frame's head, another's body, arms, legs, backpack or weapon, each with its sockets, a swapped head set where the body's own sat); running lights (`Paint::Light`: port red, starboard green, white strobes, a red beacon, lamps, which the hull shader blinks) and stencils (a unit number in seven-segment digits whose segments the shader lights by each suit's seed, so one mesh carries every suit's number); the legs' two-bone IK (`ik`); ambient occlusion baked into every vertex from the whole suit at rest (`ao`); the cockpit seen from the seat (`cockpit`: the shell, its monitors' faces and their regions of the shared screen texture, the radar's place). |
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
- Tick with 128 suits on the bodies (walking, hopping, crouching, digging, sleeping and waking),
  64 Heavyarms hunting them with guns and missiles, and 256 dolls: p50 1.2 ms, p99 3.3 ms.
- Live server under a 64-bot swarm flying all six frames (bots on the same 4-core machine):
  histogram p99 ≤ 4 ms, 0 hot-path allocations; worst tick 8–11 ms in most runs, and one run
  with a single 60 ms tick (1 overrun), when the machine stalled the sector thread. 0–53 of
  115,200 snapshots were dropped at a full egress ring across three runs.

## Tick pipeline (`Sector::tick`)

1. **Control:** Join (wake the suit a signed-in pilot left asleep, if it's still there; else claim a
   suit at the faction's spawn, clearing the longest-asleep sleeper if the sector is full), Leave
   (release it), Sleep (a signed-in pilot left: the suit stays, asleep; under survival, one parked
   in a landmark's hide spot is reported on the slot's ring, `Report::Parked`, for the pilot's
   record), Restore (at boot, put such a suit back, asleep, and answer on `SectorShared::restored`),
   Discard (take away one put back after the server stopped waiting for it), Respawn frame; inside
   the colony, Board (seat a pilot in one of the Charter Board's trainers on the Blast Hall's
   gantry, `Sim::launch_at`).
2. **Inputs:** drain each slot's ring into a 64-slot jitter buffer, and process acks.
3. **Oracle advice** in, with a 15-tick time-to-live.
4. **Apply inputs** for tick `T`: the client's command if it arrived; otherwise the last one with fire
   cleared (the same view delay). After 8 silent ticks the suit goes hands-off, keeping its states
   (and, for 30 ticks more, its lock-on: `LOCK.md`).
   Until a client is first heard from, "the last one" is the suit's own input (what a woken or
   restored suit was left with: the grip held), so a rider never lets go before its pilot speaks.
5. **`Sim::step`:**
   1. Mobile Doll AI (re-plans every 3rd tick, staggered). A ZERO seizure overrides the pilot
      (keeping the grip). Dolls hunt riders from above and never push into a landmark.
   2. Specials: their cooldowns run down; the Hyper Jammer follows MODE and drains energy; Full
      Open runs; a transformable frame changes form on MODE (`bc_sim::transform`, which the
      client's predictor runs too). Sleepers' specials don't run.
   3. Flight. First every suit's stat sheet is rebuilt (`bc_sim::tuning`) from its parts,
      systems and equipment as they stood at the end of the last tick, which is what its pilot's
      client was just told. Then `bc_sim::ground::move_step` for every live suit: a free suit flies
      (AMBAC/RCS, per-axis thrust (damaged thrusters cough on ticks the owner's client can work
      out), propellant and any leak under the sector's flight rules (`tuning::FlightRules`: under
      anime rules only boost burns and the tank refills, on the ground too; the Welcome's ANIME
      flag tells the owner's client), G-strain against the pilot's own tolerance each way), swept
      against the rocks, the landmarks and the colony, which are crashes its pilot feels; a suit on
      a body is caught, walks, hops or lets go
      in the body's frame, and its world pose is derived from that. Wrecks drift, and so do
      sleepers (`sim/sleep.rs`: no flight assist, no attitude hold), unless parked on a body
      (held there, moving with it).
   4. Chunks (loose ore, limbs, hulks): free ones drift on closed-form segments, bounce off the
      colony, rocks and landmarks (relative to a moving surface), and expire.
   5. Rebuild the spatial hash (counting sort, 128 m cells).
   6. Record lag-comp history, so `history[T]` is exactly snapshot `T`.
   7. Cover (`sim/conceal.rs`): since when each suit has lain still, the hide spot it's in, which
      riders stand still (for replication), what each pilot could park on, and the counts for the
      metrics. Then missile locks build or fall apart on each launcher-carrying suit's designation.
   8. Guns: charge, heat, energy, arm cone, magnetism, spawn, lag-comp catch-up (the first rock,
      landmark or colony in the way stops it, and a rock is worn down by it). A shooter on a body
      fires from where it was on the body as its pilot saw it. The flamethrower (`sim/flame.rs`)
      burns what's in its cone every few ticks while it's lit, without lag compensation. A weapon
      on an arm the Dragon Fang has taken along waits.
   9. Projectile sweeps against per-part capsules (skipping parts that are gone), rocks, landmarks
      and the colony, in order along the path; shots wear rocks down until they shatter into ore.
      Then missiles (`sim/missile.rs`): the seeker (every third tick, blind to a suit that's gone
      dark), proportional navigation on the motor's Δv budget, and a proximity fuse swept against
      enemy suits, rocks, landmarks and the colony, each end a `MissileBurst`. A missile pool of
      1 024 is allocated with the sim; a launch into a full pool fizzles.
   10. Melee (`sim/melee.rs`), driven by each blade's `MeleeSpec`: swings sweep an arc (sub-steps
       per tick), the Dragon Fang's thrust drives its head out along the aim, twin weapons strike
       with a blade in each hand. Blades that parry clash; a stroke chips ore off a rock (not the
       one underfoot, unless the stroke is aimed down into it) and cuts a part off a hulk.
   11. Damage resolves in order: limbs come off as chunks, overflow spills to the torso, a blow
       through thinned armour may reach a system inside the part (rolled with `hash01`; a struck
       reactor scrams, a cockpit concusses, actuators jam), suits die and leave hulks (spilling
       their holds).
   12. Salvage: grab, stow, throw, jettison, and sales at the dock. Presses are edges against the
       previous tick's buttons, so this runs before they're recorded.
   13. Heat, energy, statuses (a scram's and a concussion's timers; damage control mending one
       damaged system at a time), ZERO strain (seizure and lockout), respawns.
   14. ZERO rollouts (staggered every 3 ticks per pilot).
   15. Shattered rocks grow back once no suit awake is near, nor a sleeper in the way (checked
       every 30 ticks).
6. **The Proving Ground** (the colony's inside only, `Sector::watch_training`): each pilot's run of
   the course (`colony::course::Run`) stepped with where their suit stands at the end of the tick,
   and their drill in the Blast Hall (`colony::hall::Drill`) fed this tick's `TargetHit` events of
   theirs and the clock. A course flown or a drill cleared goes on the slot's report ring
   (`Report::Course`, `Report::Drill`) for the board. A few closed forms a pilot; nothing allocated.
7. **Sleepers' fates** (destroyed, or cleared for room) onto the notes queue, for the server to tell
   their pilots; under survival, the records of those in hide spots hit this tick onto
   `SectorShared::reparked`, for their pilots' records.
8. **Tactical pictures** for ZERO pilots (≈4 Hz), only when an external oracle is attached.
9. **Snapshots** for each client, straight into its ring. The egress thread is unparked.

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
- **Prediction.** The own suit is stepped with `bc_sim::ground::move_step`, the server's own step
  (flight, or surface contact on a body), and reconciled on every snapshot by replaying the
  unacknowledged commands. Measured error over a 100 ms-RTT, 5%-loss link: p99
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
  - G-strain arrives exactly (`f32`), so a blackout starts on the same tick on both sides. A crash's
    jolt is the flight step's own (`flight::crash`), so a knock-out is predicted with it.
  - While ZERO flies the suit (a seizure) the pilot's commands aren't what it flies, so it's drawn
    from the server's state carried on at its velocity and spin, and each snapshot's correction
    blends out.
- **Interpolation.** Everyone else is drawn `interp_delay` ticks (2–6, adaptive) in the past, with
  Hermite interpolation of position and velocity and normalised lerp of rotation. A suit on a body
  is interpolated in the body's frame and then composed with the body's pose at the view time
  (below, "Bodies and frames").
- **Lag compensation.** A shot carries the shooter's view time (`view_tick_q4`). The projectile is
  flown through the recorded history from that time to now, at most 8 ticks. During catch-up, flight
  step `k` is tested against `history[view + k]`; afterwards it continues against the present.
  Either way, it meets targets where the shooter saw them, and the rocks, landmarks and colony
  where they were then too.
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
    what that client's sensors can see (a rider standing still counts a tenth), then salvage
  chunks.
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
  - Under survival, a suit parked in a landmark's hide spot outlives the server. The sector puts a
    `Report::Parked` (a `bc_sim::sim::ParkRecord`: the landmark, the body-frame pose and stance,
    and the suit's kit) on the slot's report ring before it frees the slot, and the session saves
    it in the pilot's record (`PilotRecord.parked`). At boot, before sessions are accepted,
    `GameShared::restore_parked` reads every record (`PilotStore::all`) and sends the sector a
    `Control::Restore` for each that is still in a bay marked out and still names this build's
    landmarks (`LANDMARKS_VERSION`). The sector answers each it put back on
    `SectorShared::restored`, and the record then names that sleeper, so the pilot's next Join
    wakes it as any other. Any fate of the sleeper clears `parked` on disk, so a hidden suit
    destroyed while its pilot is away is never put back; every hit on it replaces the record with
    what's left of it (`Reparked`, newer by sector tick), so limbs shot off don't grow back with a
    restart. While the pilot's session is live (leaving, it saves the record it has), that news
    waits on the session and is applied once it ends (`Pilots::release`, `apply_park_news`); a fate
    that comes before a restored suit is bound to its pilot is kept for the binding
    (`Pilots::bind_restored`); and a restore answered after the 2 s wait, its record already let
    go, is discarded again (`Control::Discard`).

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

## Bodies and frames: suits on moving ground

A suit can land and walk on a rock of the field, on Hermit, or on MO-II, which rolls and drifts
(`DESIGN.md`, "Surfaces"). This is how that stays exact for every player at once, in the integer
tick, under the hot path's rules.

**Bodies are closed forms in the integer tick** (`bc_sim::bodies`).
- A landmark's pose at tick `t` plus a fraction is `landmark_pose(def, t, frac)`: its spin and orbit
  phases are `((t % period) as f32 + frac)` over a whole number of ticks, through libm. Rocks don't
  move.
- Every consumer evaluates the same code at the tick it needs: the server and the predictor at
  `(t, 0)`, lag compensation's rewind at `(spawn_tick + k, frac)`, the renderer at the view time.
  They agree to the bit, a phase never drifts however long the server runs, and **body poses never
  travel on the wire**.
- Surfaces are exact signed distance functions in the body's frame (`Shape::probe`: distance and
  outward normal): an ellipsoid (rocks, Hermit) or a union of rounded boxes, a rounded cylinder, a
  capsule (MO-II), minus sphere cuts (the hide spots' bowls). Sweeps are analytic for an ellipsoid
  and its cuts, and conservative sphere tracing for a union: at most 48 steps, never stepping over
  the 6 m of MO-II's thinnest feature. Fixed work, libm only, no allocation, and a bounding-sphere test
  turns away first every query nowhere near a landmark.
- Inside the colony (an interior sector) the one body is its city, `Body::City`, still at the
  sector's origin, its frame the colony's own. Its surface is `colony::interior::probe`, the
  signed distance to the hull from inside, the end caps and the city's boxes near the point (a box
  measured in city coordinates; nothing past 64 m looked for, and the distance capped there, so
  sphere tracing stays conservative), and `ground_under` is what's straight under a suit.

**Two frames.** A free suit lives in the sector's frame. A suit on a body (on its feet, aloft in its
grip, or parked) *is* an `Anchor` in the body's frame: its position, rotation, velocity and spin
relative to the body, and its stance. That is what's authoritative, what the state hash covers,
and what travels (the own state's exactly). Its world `FlightState` is derived from it every tick
(`ground::derive`: position `P + R·local`, rotation `R·rot`, velocity the surface's there plus
`R·vel`), before the spatial hash and the lag-comp history are written. So the hash, the history,
weapons, sensors and AI see a rider as they see any suit, and nothing drifts however long it rides:
the world pose is one composition off, recomputed every tick, never accumulated.

**One mover.** `ground::move_step(bodies, mover, cmd, ctx, dt)` is the only thing that moves a live
suit: the server runs it for every suit, and the owner's client runs it to predict its own.
- It decides the transitions at the start of the step from the step's own inputs (the body gone,
  the grip cleared, a hop, a catch), and touchdown, walking off and losing the grip from the step's
  own result. Both sides therefore catch, land and let go on the same tick.
- A free suit takes `flight::step_in`, numerically unchanged (`step` is split into `integrate` and
  `world::constrain`, proven bitwise). A suit that never arms the grip and never nears a landmark
  flies bit for bit as it did before bodies existed, which is why the older goldens didn't move.
  Inside the colony a free suit takes `colony::interior::step` (its pull, its air, its walls).
- The city has a down, the spin's, which other bodies don't: over it a suit in its grip falls
  that way under the colony's pull (not toward the nearest surface under the grip's), and stands
  only on ground facing within 30° of up, so walls stop it and keep it off them, and a roof's edge
  is stepped off rather than walked round.
- On the ground the step is kinematic. It places the origin over the surface with `place`, which
  never reads the suit's rotation, so the own state's 16-bit rotation can't perturb a replayed
  position. Aloft is `flight::integrate` in the body's frame, with grip gravity and the descent
  brake added.
- The rider's own body is the only geometry it ignores. Other rocks, other landmarks, the colony
  and the sector's box stay solid to it; if one moves it, the anchor is re-derived from the result.

**Invariants**, asserted for every suit on every tick of the surface tests
(`tests/common::check_invariants`):

| | Invariant |
|---|---|
| I1 | A rider's world pose (and a parked sleeper's) is bit for bit its anchor composed with its body's pose this tick. |
| I2 | A rider's body is known and alive (a rock that broke this tick lets its riders go on the next). |
| I3, I4 | An awake free suit, and a wreck, have no anchor. |
| I5 | A sleeper resting against a body without standing on it is at rest on it. |
| I6 | A grounded suit's feet are within 1 cm of the surface after its step, unless a wall stopped it, when it stands where it last stood. |
| I7 | The stance is on a sixteenth-metre grid, from 6 m (crouched) to 9.125 m (standing). |
| I8 | No landmark's surface moves faster than 5 m/s (a content test: MO-II's worst is 2.87 m/s). |
| I9 | A still body's surface velocity is exactly zero, positive-zero bits: never worked out as 0·r. |
| I10 | `flight::step` and `step_in` are numerically unchanged. |
| I11 | A rider is slower than 30 m/s over its body aloft, 28 m/s on the ground, and its local position fits the wire. |

**Step order.** Flight (`move_step`) → chunks → wrecks → the spatial hash → the history (world
poses, right for riders by I1) → `cover_step` → locks, guns, projectiles, missiles, melee, damage
and the rest (see "Tick pipeline"). `cover_step` caches what concealment needs once a tick (since
when each suit has lain still, its hide spot, still riders, what pilots could park on); concealment
itself is worked out on demand, so a hidden suit that fires shows in the same tick's snapshot.

**One clock for the bodies.**
```
server tick T:  body(T) ∘ L(T) → world → history[T] → snapshot T
client:  t_view = server_now − interp_delay (2–6)   every body, every other suit and rider
         t_own  = server_now + lead − 1             the own suit (free: its world pose; on a body: its local pose)
         G      = t_own − t_view                    typically 5–8 ticks; about 50 on a bursty link
```
Every body is drawn once, at `t_view`. Another rider is drawn at `body(t_view) ∘ L(t_view)`: exactly
the server's history, which is what lag compensation tests a shot against. The own suit on a body
is drawn at `body(t_view) ∘ L(t_own)`: its feet on the deck as drawn, but off its true world pose
by the surface's speed over G ticks.

| Error (worst case) | Rocks, Hermit (still) | MO-II (≤ 2.87 m/s) | The colony, were it walkable (177 m/s) |
|---|---|---|---|
| Another rider drawn vs the server's history at `t_view` | ≤ 7.8 mm (the wire's steps) | ≤ 7.8 mm | – |
| The own rider's feet vs the deck as drawn | 0 | 0 | – |
| The own rider drawn vs its true world pose, `v·G/30`, at G = 2, 5, 8, 50 | 0 | 0.19, 0.48, 0.77, 4.8 m | 12, 30, **47**, 295 m |
| The own shot's or blade's origin vs what its pilot saw | 0 | 0 for G ≤ 8; `v·(G − 8)/30` past that (4 m at 50) | – |
| A free own suit vs a landmark it flies beside (drawing only) | 0 | 0.77 m at G = 8 | – |
| Spin phase after any uptime | 0 | 0 | 0 |

The colony's column is why its hull isn't walkable yet. Measured over a 100 ms, 5%-loss link
(`bc-sector/tests/netcode.rs`): the own rider was drawn within `2.87·G/30 + 5 cm` of the server on
every frame (worst 0.48 m at G ≈ 7); other riders within 1.7 cm of the server's history (p99); and
prediction on MO-II's deck stayed within 0.1 mm in the body's frame (p99).

*Rejected: drawing the bodies at `t_own`.* The own suit would then be exact, but every other rider
would be off the history by `v·G`: every shot at a rider would need a rewind in the body's frame,
and free suits beside a body would be drawn off it. The own suit is one observer; every other rider
is a target.

**The view shift.** A shot or a blade from a suit on a moving body starts from where its pilot saw
it: on the body as it was at the shot's view time (`Sim::as_seen_on_its_body`, in `fire` and in
`melee_sweep`; the client's `Predictor::as_seen` for its own predicted beam). The pilot saw every
target at that time, since lag compensation rewinds them there, and saw its own suit on the deck as
the deck was then. So the shot starts exactly there, whoever it's aimed at: on the same body,
another, or flying free. On a still body the shift is nothing, and is skipped. Measured on MO-II:
the server's muzzle matched the client's drawn beam origin, p99 under 1 mm.

**Lag compensation** keeps its world-space history unchanged: it is exact because riders are
recorded as `body(T) ∘ L(T)` (I1) and drawn as `body(view) ∘ L(view)`. The rewind sweeps landmarks
at the rewound time, where the shooter saw them. Missiles and flame aren't lag-compensated, and meet
bodies at the present tick. The broad phase's 170 m pad stays: a body adds at most 0.77 m.

**Frame switches don't pop.**
- *The own suit.* Landing or letting go changes how it is drawn (from `P(t_own)` to
  `body(t_view) ∘ L(t_own)`, or back), a jump of at most `v·G`. It is handed to the drawn suit as a
  correction and blended out on the same spring as any other, never cut. While on a spinning body,
  a correction still being blended out turns with the deck.
- *Other suits.* A track whose samples straddle a switch converts the earlier sample into the later
  one's frame at the earlier one's own tick (both exact), and interpolates there: continuous in
  position and velocity.
- *Prediction.* `reconcile` seeds the anchor from the own state (body-local; position and velocity
  are `f32`, exact). When the kept sample and the server agree on the body, the error is measured
  in the body's frame. A `RockBreak` at tick B lets riders go at B + 1, and the predictor marks the
  rock dead from then only, so replays before the break still stand on it.
- *The aim.* While the own suit stands on a spinning body, the browser turns the aim with the deck,
  so a still mouse holds its bearing on it.

**Precision.** Each sector is one `f32` frame (±32.8 km), and each body one of its own; nothing is
ever expressed in a bigger frame.

| Where | Resolution |
|---|---|
| A rider's state (body-local `f32`) | 61 µm at Hermit's 900 m; 30 µm at MO-II's 385 m; 3.8 µm at 60 m |
| Its composition into the sector, `P + R·L` | one world ulp: 0.98 mm at 8–16 km, 1.95 mm at 16–32 km; recomputed every tick, never accumulated |
| On the wire | riders 1.5625 cm in the body's frame; free suits 3.125 cm in the sector's |
| Phases | exact integers modulo the period; the drawing's fraction adds at most 0.0039 tick (0.1 mm of MO-II's orbit) |

**Bandwidth.** The cap is unchanged: 1 100 B × 30 Hz, 33 KB/s a client. A rider's record is 191
bits, 17 fewer than a free suit's, and the own state grows by 4 bits flying free and 18 or 24 on a
body. A datagram still holds 38 free suits, or 41 riders (`PROTOCOL.md`). A rider standing still
counts a tenth in the priority, and its record says the same thing every time but for its flags
and parts, so it is drawn with no jitter at all. Measured with a 256-byte client and 24 suits on
Hermit, half of them still: the walkers refreshed at 13.3 Hz and the still ones at 1.5 Hz, where
all walking they get 7.4 Hz.

**CPU.** A tick poses each landmark once (two sincos pairs). A suit on a body costs about six probes
(each at most eight primitives and a cut on MO-II), a turn or a flight integration, a rock-box
query and a bounding test or two: by count some 1.5–2 µs, so under 1 ms were all 512 suits
attached. A free
suit with its grip armed adds the search for a surface to land on (pilots only, about 0.3 µs). A
shot or missile adds two bounding tests and the colony's analytic sweep. `cover_step` is O(alive
suits) plus at most four sphere tests per rider. Nothing allocates: the new per-suit arrays and
`cover_step`'s scratch are sized in `Sim::new`. Measured (`benches/tick.rs`): 128 riders on
MO-II, Hermit and the rocks (about 69 on their feet and 9 aloft on any tick, the rest asleep or
flying), 64 Heavyarms firing guns and missiles at them, and 256 dolls tick at p50 1.2 ms, p99
3.3 ms, in line with the duels without bodies.

**Sectors and handoff.** Bodies are sector content and riders are sector-local: a `BodyRef` names a
body of this sector. Every landmark's reach is at least 2 km inside the sector's limit, and the
grip lets go 40 m off a surface, so a rider can't reach an edge; handoff, when it comes, takes a
suit only when it is free. Persisted parks carry the landmark and `LANDMARKS_VERSION`, which moves
with the protocol version on any change to the landmarks.

**Planets and moons, later.** A moon is too big for one `f32` frame (0.125 m steps at its 1 737 km
radius), so a lunar sector *is* the moon-fixed frame: a tangent patch ±30 km across, where the
ground is still in sector coordinates and the moon's turn never enters the simulation. Its surface
becomes a heightfield tile with a bounded slope (heights worked out without cancellation:
`h = (x² + z² + y² + 2yR)/(|p − C| + R)`, never materializing the centre), and grip gravity becomes
the body's own (1.62 m/s²). `move_step`, the wire and the interpolation carry over unchanged, since
they already work as "a body's frame and a probe". A seamless map would key positions by
`(sector, f32 local)`, or by `i64` millimetres.

## The colony inside

The First Colony is 3.2 km across and 32 km long, with a city on each of its three land strips:
about 13,000 blocks. None of it is stored or sent. It's a closed form of where you ask, in
`bc_sim::colony`, `no_std`, allocation-free and deterministic like the rest of `bc-sim`
(`CITY_GOLDEN`, `TRAFFIC_GOLDEN` and `WALKERS_GOLDEN` in `tests/determinism.rs`, and the
`no_alloc` tests), so the browser and the server see the same walls, and every screen the same
traffic and crowds.

- **Frames.** Inside, everything is in the colony's own frame, which turns with it: there the city
  stands still. A strip's **city coordinates** (`frame::CityPos`: `x` along, `s` across from its
  edge, `h` up) unroll the cylinder with plumb walls, so a building is a box in them and
  collision is box against box (`city::solid`). The walker walks in `(x, h, −s)` with the spin's
  gravity, `ω²(R − h)` (`bc_client_core::city::CityGround`, through `walker::Solid`).
- **The day** is the tick's (`colony::time`): the mirrors' opening, the Sun's height over each
  strip, the lamps. Every client's colony shows the same hour.
- **The city's rules** (`colony::city`) turn a block's strip and grid cell into what stands
  there with an integer hash: its district (`content::city`), its kind (buildings, park, plaza,
  canal, site, a key place, a landmark tower) and up to nine buildings, each at most eight boxes
  (`Building::pieces`: a body, tiers set back, a crown, roof plant, a mast). A key place's hall
  has a room behind its door (`city::room`, a closed form of the place): the hall's solids are its
  walls round the room, the door's lintel, what's over the ceiling and the counter, so the walker,
  the server's checks and the meshes all have it. `texel` sums each block up
  in four bytes: the client bakes them into a 256 × 81 texture, and the shaders paint the streets
  and the city seen through the windows from it (`bc::city`, `shaders/city_lib.wgsl`, its constants
  checked against Rust by `bc_client_core::city_atlas`'s tests). Shaders never re-implement the
  layout. The street's furniture (`colony::furniture`: lamp posts under the paint's pools, trees,
  benches) is a closed form too, solid to people and their cars (`city::solid`) and not to suits,
  which step over it (`city::each_solid`, `solid_built`).
- **Drawing it** (`bc-client/src/city.rs`): its own render layer, switched to when the pilot is
  inside; the ground, windows and end caps built once; buildings in chunks from 2 km (one box a
  block) down to 256 m (every building, trees, railings), chosen by a quadtree round the camera
  and built within a share of the frame (a showcase builds them all first); a chunk's mesh is
  relative to its own anchor, and everything is drawn relative to a render origin that follows the
  camera by the kilometre, so `f32` stays fine far from the axis. The camera's strip is lit by the
  scene's one directional light (with shadows, cascades sized to the camera's height); the other
  two, kilometres off, by their own key, sky and ground bounce. One sky function
  (`shaders/colony_sky.wgsl`, `bc::colony_sky`) is the air for everything inside: haze thickest near
  the floor and clean in the core, lit by the windows' beams, and the colour seen along any ray, which
  glass and water reflect. A colour script by the hour (`city_hour.rs`) drives it, the Sun, the
  exposure, the grade and the bloom; Bevy's distance fog, matched to it, is left only for what the
  city's shader doesn't draw (people, cars, trams). The buildings' surfaces are `bc::facade`
  (`shaders/city_facade.wgsl`: materials by district and strip, wear, rooms behind the windows) and
  the ground's is `bc::city`'s paint; both take their derivatives at the top of `city.wgsl` (they
  must stay in uniform control flow) and are filtered by the pixel's footprint. The Low tier compiles
  their cheap variants (`FACADE_LOW`, `city_sketch`), since software rasterisers pay for every branch.
  `docs/COLONY_LOOK.md` is the look these serve.
- **The trams** (`colony::transit`) are a timetable in the tick: a line down each strip's
  avenue, eleven stations, twelve trains running out and back, speeding up and slowing down at
  1.5 m/s². Every client draws every train where the tick has it (`bc-client/src/trams.rs`),
  and nothing about them is sent. The stations' platforms are part of the city's walls. A rider
  walks inside their car, in the car's own frame (`bc_client_core::tram`): the timetable carries
  them, its acceleration pushes them, and they get on and off through doors that open only while
  the train stands at a platform.
- **The traffic** (`colony::traffic`) is a closed form of the tick like the trams: `each_car`
  works out the cars near an area (`each_ring_car` the moving ones, `each_bay_car` the parked
  for good). Each block row has **rings** of cars going round it with its blocks on their right
  (traffic keeps right): out along the street on its outer edge, back along the one on its inner
  edge (the avenue's carriageway for rows ±1), turning at the **wide** cross streets. Track 0 (the
  kerb lane) goes round one 512 m stretch, track 1 (the next lane) round the whole row, so no two
  rings share a lane: nothing turns left, nothing crosses a street along the strip or the tram's
  median, and no two paths ever cross. **Signals** stand at every junction of
  the wide cross streets on an 80 s cycle, 40 s for all the cars at once and 40 s for the people
  (2 s all-red, 38 s walk); 512 m at 12.8 m/s is half a cycle and the phases alternate in a
  checkerboard, so both ways of every street ride a **green wave**. Cars run in **platoons** of up
  to ten, one a cycle a ring, queueing nose to tail behind the stop line at each red; each car is
  its own closed form (its place further back, its start a moment later), so none closes on the one
  ahead. At night track 0's platoons park whole in the bays beside their queues and pull out at
  dawn, each at its own hour; parked cars line the paint's **bays** for good elsewhere. The narrow
  cross streets carry no cars. Cars stand on the road at `h = 0`.
- **The city's people** (`colony::walkers`, not the pilots below) are a closed form too:
  `each_walker` gives everybody near an area. They walk **lines**, loops laid where nothing
  stands (round a block's pavement, or a run of four blocks' over the narrow cross streets' zebras;
  the avenue's walks, the quays, a park's loop, a plaza's monument, the banks' promenades), with
  **slots** evenly round each that move on one every few seconds; a day is whole laps of every
  line, so the crowd comes round each day to the bit and nobody on a line ever closes on another.
  Who's out is a slot's own draw against how busy the place is (its district, the hour, a station
  near), asked at the line's corners (its **portals**), where people fade in and out. Hub Gate's
  square is a **lattice** of crossing lines on its paving's bands, timed so that people pass each
  other's paths half a slot apart. Sitters step off their line onto the avenue's **benches** for a
  couple of laps and back into their slot; the **platforms** queue at the doors, board and get off
  on the trams' timetable (`transit::stands`). Standers wait by the walls, at kerbs and railings.
- **The two together** keep a contract, each half a test on its own side and the whole in
  `tests/life.rs` (no car's footprint comes within `walkers::RADIUS + 0.2` m of anybody): no car
  ever drives a narrow cross street, and the bays stay 10 m clear of every crossing (C1); people
  are on a road only on a narrow cross street's zebra, in rows ±2 to ±12 but the canal's (C2), and
  never on the streets along the strip, the wide cross streets, the avenue's carriageways or its
  median (C3); queued cars stand behind their stop lines through the people's half (C4). People
  and cars never share ground, so neither needs the other's state. Nobody uses the people's half
  yet. Both are ghosts to pilots, their cars and suits.
- **Life drawn** (`bc-client/src/life.rs`): every screen draws the same cars and people from the
  closed forms at the trams' clock; nothing is sent. Each frame `bc_client_core::life::Choice`
  asks them for what's round the camera on its strip (the cars by distance from the eye, the people
  only near the ground), leaves out what stands within 9 m of a suit on the city, fades a civilian
  under a pilot on foot, and picks the nearest for each of five pools (people near and far, cars
  near and far, parked cars) up to the tier's caps (`TierSettings::life`); `life::Slots` keeps an
  entity on the car or person it showed while they stay picked. Pools are made once at Ultra's
  caps. One material paints every part by its vertex's slot and the `MeshTag` (`life_lib.wgsl`
  keeps the tag layout and slots that `life`'s tests read), and never changes after start-up (a
  change would re-specialise every entity using it): what changes, an indicator's flash included,
  rides in the tags. A car's look is keyed on its home (`traffic::Car::home`), a person's on their
  strip, so nobody's changes as they move. Figures are posed by swapping meshes from a bank of
  frames (`life_mesh`), with no skinning or vertex shader. The showcase's pilots are
  walkers it names (`showcase_city`), which life leaves out (`LifeBorrowed`).
- **Vehicles** are their drivers' own: a bicycle model in city coordinates against the city's
  walls (`bc_client_core::vehicle`), taken at a motor pool (`colony::pools`), sent as the driver's
  pose with what they drive, and drawn round them on everyone's screen.
- **Being there** is the hangar's business (`bc_econ::wire`: `enter_city`, `leave_city`, `place:
  city`), behind the server's `--colony` flag (the Welcome's COLONY). Walking is the client's, as
  in the bay.
- **The people** ("the plaza") stay off the tick too. A pilot in the city sends their pose 15
  times a second (`bc_proto::presence`, datagram kind 3); their session task checks it against
  the closed form (in the walls? too far from the last? first seen away from Hub Gate?) and keeps
  it in `bc-server`'s `plaza` (a `Mutex`ed map: the session tasks', not the sector's). Every
  100 ms each session sends its pilot the people near them (kind 4) with the sector's tick, which
  keeps the client's clock on foot; in the bay it sends just the tick, twice a second. Clients
  draw people 200 ms behind, between the poses heard (`bc_client_core::plaza`), as figures in
  flight suits of their own colours (`figure`, `bc-client/src/people.rs`). People never enter the
  sector's simulation: nothing to predict or rewind, and nothing on the hot path.
- **Suits inside** fly the server's second sector, in the colony's own frame
  (`bc_sim::colony::interior`, `SUITS_INSIDE.md`), on the colony's one clock (it keeps the first
  sector's tick). Those on foot watch them: their session holds a spectator's slot in that sector
  (`Control::Watch`), whose snapshots (marked SPECTATOR) carry the suits near the pilot and no own
  suit, and the client draws them on the city's layer. A suit's pilot is sent the plaza's people
  round the suit, from where the sector last had it (each slot's `Metrics::pilots[slot].pos`, an
  atomic the sector writes in its tick). There the city is a body, `Body::City` ("Bodies and
  frames"), and suits land and walk on it.
- **The Blast Hall's live fire** (`bc_sim::colony::hall`, `TRAINING.md`): weapons are free only in
  the Proving Ground's room. Each interior tick clears the weapons' buttons of any suit elsewhere,
  from where it is as the tick starts, and the owner's prediction clears them alike. Shots there
  meet no suit, and `first_blocker`'s interior branch stops them at the room's bounds or at one of
  its targets (closed forms of the tick, drawn alike on every screen), which raises a `TargetHit`
  event. Allocation-free and deterministic (`HALL_GOLDEN`, `no_alloc`), and outside the hall the
  interior runs exactly as before.
- **The Proving Ground's trainers, drill and board** (`TRAINING.md`, phases 3 and 4).
  - **Trainers.** On foot at the hall's gantry's hatch, a pilot boards one of the Charter Board's
    trainers (`Request::BoardTrainer`, `Control::Board`, `LaunchAt::Gantry`): a Leo standing on the
    gantry's pad, flown through the inside's sector as a suit from the bays is. A trainer
    (`Suits::trainer`) docks only on its gantry, which puts its pilot back on foot at the hatch
    (`Plaza::enter_at`). Nothing of their hangar is in it.
  - **Times.** The inside's sector times every pilot's course and drill in its tick. The run is
    stepped with the suit's place at each tick's end, the place the pilot's prediction has for that
    tick, so the client (stepping its own run on the predictor's samples) reads the same time. The
    session puts the times on the day's board (`bc_econ::proving`, behind a lock as the exchange is:
    `proving.rs`, `proving.json`) and a signed-in pilot's bests on their record.
  - **The board** goes to everyone in the colony as `Update::Proving`, and is drawn on the hall's
    back wall (`bc-client/src/board.rs`, laid out by the UI into a texture by a camera of its own)
    and at its desk.

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
`simd128`. The bodies' poses are closed forms in the integer tick through the same libm, so a
spinning station is where it is to the bit on both (`SURFACE_GOLDEN`: suits landing, walking,
hopping, hiding, sleeping, digging their rock out from under them and taking off as a bird, under
fire, beside 4 dolls).

## Verification

| Test | Proves |
|---|---|
| `bc-proto/tests/roundtrip.rs` | Codecs round-trip within ½ LSB; decoders never panic on arbitrary bytes. |
| `bc-sim/tests/no_alloc.rs`, `bc-sector/tests/no_alloc_sector.rs` | 0 heap operations per tick with 64 clients + 256 dolls, with the Gundams duelling, with 32 missile boats keeping 500 missiles in the air, and with riders on every body walking, hopping, crouching, hiding, digging, sleeping and waking under fire (a rock shattered under them), and under survival parking in the Aft Well and being restored. |
| `bc-sim/tests/determinism.rs` | Identical state hash on native and wasm32, for the reference scenario, for suits flying into rocks and firing through them, for a salvage run, for the Gundams duelling with every blade, for sleepers, and for suits on the bodies; the generated debris field is identical too. |
| `bc-sim/tests/kit_ai.rs` | The kit-aware pilot, each Gundam against three Taurus and a Virgo: Heavyarms locks on and opens fire, Deathscythe closes jammed and reaps, Sandrock fires missiles, Shenlong lands its fang and flame, Wing Zero flies out as Neo-Bird and fights unfolded. |
| `bc-sim/tests/{missiles,full_open}.rs` | A lock builds in half a second in its cone and falls apart twice as fast; a guided salvo runs down a crossing target; a target faster than the motor's Δv outruns it; a jammer breaks the seeker's hold where a plain break doesn't; missiles pass friends and burst at the end of their life; a full pool swallows launches. Full Open fires everything for 3 s, then locks the suit out and cools down. |
| `bc-sim/tests/jammer.rs` | A jamming Deathscythe leaves its enemies' sensors (past 150 m for eyes), Mobile Dolls and ZERO lose it, allies see it shimmer, locks on it drop and its own go unnoticed; firing or striking breaks it for 2 s; it drains energy and needs a fifth of it to engage. |
| `bc-sim/tests/ranged.rs` | The flamethrower burns within its cone and reach only, a round a burn, and overheats its target; the Dragon Fang takes the flamethrower's arm along; stream weapons fire without spawn events; the buster shield flies at its speed. |
| `bc-sim/tests/{content,melee}.rs` | Every table row sits at its id and the Gundams fly as designed; every blade reaches as far as its row says and mines, twin blades strike once each, the Dragon Fang thrusts where it's aimed, the Cross Crusher is Sandrock's special, only blades that parry clash, and a blade meets a target it chases at speed as its pilot sees it. |
| `bc-sim/tests/{flight,combat,fire_control,lagcomp,mobile_dolls,zero,field,salvage}.rs` | Rocket equation, FA, blackout (the G envelope each way, the brain's oxygen, crashes and what isn't one), no tunnelling, arm loss, charge, sabers and clashes, lag comp (and its clamp), dolls fight to a kill, ZERO accuracy, calibration, seizure, magnetism; suits stop at rocks at 2 km/s and rocks stop shots; limbs come off as chunks and shots pass where they were, hulks, bounces, expiry, lighter suits. |
| `bc-sector/tests/salvage_net.rs` | Over the same link: chunks reach the client exactly as the server moves them, across bounces; chunks that go leave the client; a kill hands its wreck to its hulk; changed rocks arrive; a miner under survival rules docks and brings its haul home. |
| `bc-sim/tests/survival.rs`, `bc-sector/tests/survival_net.rs` | A suit launches as it was built (parts missing, worn, weapons not fitted that don't fire, what's in the tank); it docks only at rest in the dock, awake, and goes home with its hold and what it holds; the colony pays bounties on Mobile Dolls; a pilot shot down stays down. Through the sector: no loadout, no suit; launch, dock and home with the hold on the slot's report ring; a suit lost is reported, then its pilot goes home. The `no_alloc` tests cover survival ticks too. |
| `bc-sim/tests/{systems,modules}.rs`, `bc-sim/src/tuning.rs` | Blows through thin armour reach systems about twice per part's life whatever the weapon (Mobile Dolls less), the same way every run; each system's levels do what their table says (sensors, locks, scram, leak, coughing thrusters, actuators that jam and let go); each module changes its stat, weighs what it weighs and goes with its part; damage control mends one damaged system at a time; faults and equipment launch and come home. |
| `bc-sim/tests/flight.rs` (anime rules), `bc-sim/tests/kit_ai.rs` (anime rules), `bc-client-core/tests/anime_predict.rs` | Mobile Dolls fly by the real rules whatever the sector's. Under anime rules flying, turning on RCS and braking burn nothing, boost burns what it always has, an empty gauge still flies (at its plain cruise) and fills only once boost is let go, a holed tank refills slower and a failed one not at all, and pilots bear twice the G; the kit pilot lets go of a dry gauge until it's half full, and every frame still shows its kit; the owner's prediction keeps the gauge, and so the suit's mass and where it goes, with the server's from any snapshot, and by the real rules it wouldn't. |
| `bc-sector/tests/netcode.rs` (failing systems) | A suit whose thrusters cough, whose tank leaks and whose pilot is hurt, carrying a thruster kit, a G-seat and a cargo rack, is predicted over the bad link to p99 under 0.1 mm. |
| `bc-econ/tests/wear.rs` | Faults come home with their parts and travel with them to the shelf (a faulted part isn't new); overhauls restore them from the stores; equipment fits one of a kind on its own part and comes off with it; the stat sheet follows what's fitted and broken; records from before load as they were; the colony deals in components and equipment, and opens their desks on an old exchange. |
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
| `e2e/tests/gfx.spec.ts` | Every showcase scene renders cleanly, `gundams` included (Full Open's salvo, the jammer, the shotels and Cross Crusher, the fang at full reach, the flamethrower, Neo-Bird), `surface` (a Leo walking on MO-II as it rolls, one kneeling asleep in the Aft Well, one landing on Hermit), and the hangar bay. |
| `e2e/tests/hangar.spec.ts` | Survival in the browser: the pilot comes in through the airlock, walks to each terminal and uses it, fabricates and trades through the panels, boards at the hatch, launches through the bay doors into space, and docks home again. |
| `bc-sim` `colony::transit`, `bc-client-core` `tram` | The timetable comes round to the bit and runs without jumps, never past 1.5 m/s² or its top speed; trains on a track stay 300 m apart; every door of a standing train is on its platform, a hand's breadth from its edge, open only there; the platform is climbed by its steps; a car's doors let a walker through only when open. A walker on a platform walks in through an open door, rides out of the station held by the car's walls against its push, and walks out onto the next platform. |
| `bc-sim` `colony::traffic` (`tests/traffic.rs`) | No two cars ever touch; cars keep to the roads (off everything built, the furniture, the median and the pavements), keep right in a painted lane's middle and never cross a street along the strip; no car touches a tram; no car is on a signalised junction or its crossings in the people's half, nor ever on a narrow street's crossings; cars clear the junctions well before the amber; the signals go round every way at once; cars move smoothly and never appear or vanish in view, parking and pulling out included; the traffic comes round exactly; fewer are out at night, parked in the bays; the rows run on as the site is built out; `each_car` is `each_ring_car` and `each_bay_car`. |
| `bc-sim` `colony::walkers` (`tests/walkers.rs`), `tests/life.rs` | Nobody stands in anything solid or within 0.6 m of anybody; every line is clear of everything solid, walked every 0.25 m; the square's people cross half a slot apart; people move smoothly and come and go only faded; everybody comes round each day (the platforms each week); tiles find everybody once; busy downtown and by day, quiet at night, nobody on the site; people cross only the narrow streets, on their zebras; sitters sit on the furniture's benches and never bump anybody getting up; people board and leave trains through open doors and platforms never crowd. With the traffic: no car ever comes within reach of anybody, over 7 million people sampled at four hours. |
| `bc-client-core` `life`, `life_mesh`, `showcase_city`, `figure` | The shader keeps the tag layout and slots; tags round-trip; lights follow the traffic (a queue brakes, the parked show nothing); a frame keeps to its tier's caps and reaches, clear of suits, borrowed walkers and pilots, the same every time; the nearest are picked, ties by id; pooled entities keep their cars and people. Vehicles fit their kinds' footprints with their wheels on the ground and their lamps at their ends; figures fit the walkers' radius in every baked frame and a sitter the space `walkers::sitting` keeps; meshes are light and face out. The planted foot slides a third of the body's travel (it slid on ahead before the knee bent on the forward swing). The showcase's pilots are the city's walkers, its vehicles keep 0.3 m off every car and person, its street cameras stand where nobody walks or drives, and the third sees people and cars at its hours. |
| `bc-sim` `colony::pools`, `bc-client-core` `vehicle` | Every motor pool stands on open road, room for a car. A car gets up to speed, brakes to a stop and then backs up; it turns the way its wheel says; it stops at a wall and is never inside one; its driver gets out beside it. The server takes a car only from a pool, at a car's speed. |
| `bc-proto` `presence`, `bc-server` `plaza`, `bc-client-core` `plaza` and `figure` tests, `bc-server/tests/plaza.rs` | A pose goes round within half a step (14 B); 48 people fit one datagram; the bay's heartbeat carries only the tick; decoders never panic. The plaza takes walks, drops reordered poses, and doesn't pass on teleports, walls, another strip or a first pose away from Hub Gate; it shows only the strip and the near, and hides the silent. Clients draw people between the poses heard and hold them at the last; headings turn the short way. A figure is under 1,500 triangles and fits the walker's box; walking swings its legs. Over real WebTransport, two agents at Hub Gate see each other by name within 5 cm, a teleport isn't relayed, going back up removes a pilot, and the plaza keeps the clock within two ticks; riders are taken from beside a standing train's open doors to the next platform, and nobody boards a running train or stands outside its cars. |
| `bc-server/tests/city.rs` | Over real WebTransport: with `--colony` the Welcome says so, a pilot rides down to a strip's city and back, trades on the exchange from there, can't launch from it, and `/status` follows them; without it, the lifts are closed. |
| `bc-sector/tests/training_net.rs`, `bc-server/tests/proving.rs` | The Proving Ground: through the sector's queues, a trainer boarded on the Blast Hall's gantry clears the drill and docks back (none outside the colony), and the course flown from the inner gate is timed by the sector to the millisecond its pilot's run reads, with no heap operations. Over real WebTransport, a pilot walks from Hub Gate to the gantry's hatch, boards (their bay untouched), clears the drill, and the server's time is on the board, in `/status` and on their record; docked back, they're on foot at the hatch, their first step taken there; the board and their best outlive a restart; a pilot gone while flying a trainer wakes in their bay, their own suit there. |
| `bc-server/tests/inside.rs`, `bc-sector/tests/watch_net.rs` | A suit launched into the colony by the inner gate flies there with its weapons safe and docks back out, and a pilot who leaves inside finds it towed home; the inside keeps the outside's tick; a suit flown down over Hub Gate sees a pilot walking there by name, where they are, and they see it (as a spectator), until they ride back up; armed, a suit lands on the avenue and walks up it, predicted as the server has it. A spectator's snapshots carry no own suit and the suits near it, tell it of one that leaves its view, and cost the tick nothing on the heap. |
| `bc-sim` `ground::tests::city` | An armed suit over the avenue is caught by the city, comes down no faster than the brake, stands a stance over the floor, walks up the avenue at a walk and lifts off without a jump; walking into the blocks, it climbs the kerb and is stopped short of whatever stands there, never nearer anything than its stance; dropped on a roof it stands there, and walked over the edge it falls, kept off the walls it passes, and stands on what's below. The city's probe agrees with `colony::city::solid_built` (the city without its street furniture, which suits step over) everywhere in it and finds the ground straight under a suit. |
| `e2e/tests/colony.spec.ts` | The colony in the browser (`--colony`, an agent strolling outside Hub Gate): out through the bay's airlock, down the cap lift (skipped) to Hub Gate, the agent seen there, the map, a walk through the streets to the Exchange floor to buy there through its panel, back to Hub Gate and up to the bay; the server's `/status` follows the pilot (`city`, then `hangar`), and the hot path never allocates. Then a tram from Hub Gate's platform one stop up the line, and a car from its motor pool up the avenue, with no pose refused. |
| `e2e/tests/surface.spec.ts` | On the bodies in the browser, signed in: the lander autopilot flies to MO-II, lands in the Aft Well and hides; leaving parks the suit there, and coming back wakes in it, grounded and hidden; then, flown by hand, it wakes there again and lifts off, free past 40 m (`FLYING`). Throughout, no hot-path allocation, every snapshot fits, and every rider names a body the client knows. |
| `bc-client-core` tests (`nav`, `chart`, `sphere`) | The Lagrange points balance (the restricted three-body problem's pull vanishes at L1, L2, L3) and L4 and L5 make equilateral triangles in the Moon's plane, which holds the Sun; Earth and the Moon sit on one line through L1, within 2.3° of the sky's first survey. Courses run straight when the way is clear, and otherwise round the colony and the landmarks without coming near them (over, under and along the colony, round its end to the dock, through Hermit and MO-II, and 120 random pairs across the sector), never far longer than the crow's flight. The auto-nav, flown tick by tick against a real `Sim` as its client would, takes every playable frame on five trips (out of the dock round MO-II to its Aft Well, MO-II across the field to Hermit's KEYHOLE, from under the colony to a rock, Hermit home to the dock, a point high over the field), by both flight rules and through the full field, and arrives at rest every time without touching the hull, a landmark or a rock. Every place is named, found and arrived at in the open; the chart's camera zooms out to frame the Earth Sphere and back in to the sector, tracks a suit at speed to the metre, picks what's under the cursor and keeps labels apart. |
| `e2e/tests/chart.spec.ts` | The chart in the browser: open over the sector, out to the Earth Sphere and back, `]` to MO-II's Aft Well, a course set there (round the station), the auto-nav engaged and the chart closed; the suit flies itself and comes to rest over the well, and the hot path never allocates. Screenshots of each step in `e2e/artifacts/`. |
| `bc-sim/tests/surface.rs` | Suits on bodies: the grip catches a slow suit and lands it feet first on a rock, Hermit and MO-II, and nothing attaches without it (flight then is bit for bit the old `step_in`); fast, boosting or climbing suits aren't caught, and catch and release have hysteresis; roll-level turns the feet down and leaves the nose; walking keeps the stance round a rock, inner corners block, rounded edges don't, walls slide; running off a ledge lands running; a suit on MO-II rides it round a full spin-and-orbit period bit for bit; hops, lift-off, the sticky crouch, letting go with the surface's velocity, legless and Neo-Bird suits; other bodies and the colony stay solid to a rider; aiming overhead; a shattered rock, sleeping and waking on a body, specials asleep, seized riders; riders go on the wire in their body's frame. Combat: free suits, chunks, shots and flame stop at landmarks; the colony no longer eats shots before suits; shots hit a rider and the hull shields one on the far side; a crater's rim stops a level shot; a rider's lag-compensated shot and blade start where it was seen; seekers drop parked and hidden suits; a blade digs the rock underfoot only aimed down, and can free the suit; spills land outside the body; dolls hunt riders from above; ZERO never predicts a grounded suit into its ground. Every test checks the invariants (I1–I7, I11) on every tick. |
| `bc-sim/tests/concealment.rs` | Parked suits power down after 8 s, or 60 s after a fight (by firing or by being hit), and never having fought isn't having fought; allies see parked and hidden suits; crouched still in a hide spot a suit goes dark in 3 s and is seen within 225 m; firing shows it for 5 s from the same tick; cold anywhere halves the signature; regrowth ignores distant sleepers but not one in the way; hidden sleepers are cleared for room last; a suit that slept hidden wakes hidden; what a pilot could park on is worked out once a tick. |
| `bc-sim` unit tests (`colony`), `bc-sim/tests/{determinism,no_alloc}.rs` | The colony's frames round-trip on every strip, walls are plumb, gravity is 1 g on the floor and 0.7 g at the bays; its day comes round to the bit, opens in daylight on a fresh server and flows without jumps; the mirrors' sweep is clear of everything in the sector; the city's streets, avenue, Hub Gate's square and lanes are clear to walk, buildings keep inside their blocks and under the cap, `solid` is what the boxes say, every key place's door faces a street; its hash is the same on native and wasm (`CITY_GOLDEN`); 100,000 queries allocate nothing. |
| `bc-client-core` tests (`city`, `city_nav`, `city_mesh`, `city_atlas`, `controls`) | A walker on the avenue at 1 g jumps 0.66 m, stops at kerbs, walls and the glass's railing and crosses the canal by its bridges; the guide walks from Hub Gate to every place, keeping to the streets; every vertex of a chunk lies on a box of the rules, its faces face out, levels get lighter and stay in their triangle budgets, chunks tile the strip and come out the same every time; the atlas holds every block where the shader looks, and its grid is the rules' grid. |
| `bc-sim` unit tests (`bodies`, `content::landmarks`, `ground`, `world`, `math`) | Probes are exact on the surface and Lipschitz; bounds are lower bounds; traces match dense sampling (none misses anything 6 m thick); poses are periodic bit for bit and their point velocities match their motion, exactly zero on a still body; the landmarks are clear of everything else in the sector, within the surface-speed budget, their hide spots on bowl floors behind walls; 75 rocks of the default field are grippable; placement converges to within 1 cm on every body; the stance stays on its grid; the colony's sweep matches dense sampling and its spin is 1 g. |
| `bc-client-core/tests/{surface_predict,landmarks}.rs` | Seeded from every snapshot of a real Sim's suit catching, landing, walking, running, hopping, crouching, letting go, lifting off and digging itself free on a rock, Hermit and MO-II (and the Aft Well), the prediction keeps footing, body and stance exact and flies within 1 cm in the body's frame for 10 ticks ahead; it stops at a landmark where the server does. Unit tests: rock deaths are tick-stamped, riders interpolate in their body's frame and across a frame switch without a pop, a still rider stays glued and lives 300 ticks, an unknown body is dropped and counted, a frame switch is a correction and not a cut, the camera stays out of the body, the gait's planted feet don't slide on MO-II, the landmarks' meshes lie on their surfaces, and the lander lands, walks and hides. |
| `bc-sector/tests/netcode.rs`, on the bodies | Over the same link, with real clients drawing at 60 and 144 Hz: prediction holds up walking and hopping on MO-II; the own rider is drawn within the clock's bound; landing and letting go blend without a cut; other riders are drawn where the server had them, and still ones stay glued through loss; a pilot hits a walking rider as often as a free one, and a rider shooting from MO-II hits what it saw; still riders yield bandwidth. |
| `bc-sector/tests/sleep_net.rs`, `bc-server/tests/hide.rs` | A rider that wakes keeps its grip until its pilot is heard from; under survival a suit parked in a hide spot is reported, and restored into a fresh sector exactly where it was, dark after 8 s. Through a real server: a hidden suit is put back at boot (in `/status` and the roster before anyone connects) and its pilot wakes in it, grounded and crouched within 1 cm of where it was, with its hold; a record saved for other landmarks (an older `LANDMARKS_VERSION`) is towed home; a restored sleeper destroyed isn't restored again, and one hunted comes back as its hunters left it; a restore answered too late is discarded; `/status` counts hidden sleepers and gives no positions. |
| `bc-model` tests | Every design stays within 2.6 m of its hit capsules (Neo-Bird's own), no two frames share a mesh, every kit has the sockets it's drawn from, a suit of mixed parts takes each section's bones and sockets from its own frame, the cockpit's eye sees out past the suit it rides (nothing drawn inside the near plane or across the crosshair's 15°), and the triangle budgets; the cockpit keeps a clear box round the crosshair and its screens share one texture without stretching; the occlusion bake (open where nothing's near, shaded where pieces meet, and quick enough for startup). |

## Under abuse

Titanfall's servers were DDoSed out of the game for years (`PEERS.md`, "Staying up"). A flood of
packets is for the network in front of the server to stop: a host's scrubbing, anycast, a firewall's
rate limits on the UDP port. The server keeps a flood of connections or requests from crowding the
pilots out, and its limits are counted on `/status`.

- **Connections** (`bc-server` `net::admit`, in the accept loop):
  - With more than 32 handshakes under way, an address that hasn't proved it hears back is sent a
    QUIC Retry first. Spoofed sources never get a handshake, and until an address is validated
    QUIC sends it at most three times what it sent.
  - One address holds at most 8 connections, handshaking or open (`--per-address`): an IPv4
    address (mapped into IPv6 or not), or an IPv6 address's /64, which is what one subscriber is
    given. Loopback is excepted: the server's own agents connect from it.
  - At most 512 connections in all (`--max-connections`). Past either cap the attempt is refused.
  - QUIC's handshake and then WebTransport's must each finish within 5 s.
- **A session** (`net::game`, `net::session`):
  - Its control stream must open, and its Hello arrive, within 5 s each.
  - At most 128 sign-ins wait on a wallet at once, each for at most a minute (`Config::sign_wait`).
  - Control frames are at most 256 bytes and hangar frames 64 KiB, so a stream's buffer stays small.
  - Inputs and on-foot poses: 120 a second, in bursts of 240; past that they're dropped.
  - Hangar requests and respawns: 10 a second, in bursts of 40. Past that they're refused, and a
    session refused 200 times in a row is ended.
  - The radio: 5 lines in 10 s, 160 characters each.
  - A session in the sector that sends no input for a minute is ended (`Config::idle_timeout`).
- **The sector** is out of reach: its slots are leased (a session without one is refused at its
  Hello), and its queues are bounded, so a session can't make the tick allocate or wait.
- **Not covered here:** the HTTP server is the development one. Deployed, it goes behind a reverse
  proxy or a CDN with timeouts and its own limits.

## Scaling path

1. **One sector per server** (now), and a second for the colony's inside. The server builds one
   sector and runs it on the thread `sector-0`, with its own rings and egress thread; the Welcome's
   sector is 1. With `--colony` it builds the inside too (`sector-1`, `egress-1`, the Welcome's 2),
   which keeps `sector-0`'s tick (`bc_sector::spawn_follower`: woken by it after each of its
   ticks, never ahead), so the colony has one clock. Nothing else in a sector is shared with
   another, so many per process (a thread each) is a matter of building more.
2. **Sector per process**, with a gateway that routes sessions by sector. Handoff moves the pilot
   record through the gateway when a transfer orbit completes. A suit hands off only flying free:
   bodies and their riders are sector-local ("Bodies and frames").
3. **TiDi.** When a sector's tick overruns, stretch wall-clock time per tick and publish `tidi_pct`
   (the header field already exists).
4. **Encoding off the sim thread.** Snapshot encoding is embarrassingly parallel per client: move it
   to worker threads that read a double-buffered world, if client counts demand it.
