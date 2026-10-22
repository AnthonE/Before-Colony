# Netcode: the netcode of record

This file says what we build the multiplayer from, what we took from whom, and what we turned
down, so the choices aren't made again each time. `ARCHITECTURE.md`'s "Netcode" is how it works
today, `PROTOCOL.md` the bits, and `SCALE.md` what stops it growing, with numbers. Read this before
changing the wire, replication or how sectors meet.

The world we are building is one universe of many sectors (EVE's and New World's shape, not
Rust's or GTA Online's), with crowded streets inside the colony and fleets outside it. Everything
here serves that.

## What we keep, and why

The model is already the one every large shooter-like world lands on (Gaffer On Games, Valve,
Overwatch, Más Bandwidth's "choosing the right network model"):

- **The server is the man.** One authoritative tick, 30 Hz. Clients send inputs, never positions.
- **The owner predicts** its own suit with the server's own step and replays unacked commands on
  every snapshot (p99 0.1 mm over 100 ms and 5% loss).
- **Everyone else is interpolated** a few ticks in the past (Hermite, 2–6 ticks, adaptive).
- **Lag compensation** flies a shot through the recorded history from the shooter's view, at most
  8 ticks back.
- **Datagrams are never fragmented.** At 1% loss a 10-fragment packet is lost 9.5% of the time
  (Gaffer). Every datagram fits 1,100 B.
- **A priority accumulator** fills each datagram, over what the client's sensors can see.
- **The sim is deterministic** (libm, no fast-math, no `simd128`). Bodies, the city, its traffic
  and its people are closed forms of the tick and the seed, so they are never sent.

## The law: three classes

Gates' rule, and ours: **every thing belongs to one class, chosen by how its state changes, and
nothing rides the snapshot unless it moves.**

| Class | What | Pipeline | Cost at rest |
|---|---|---|---|
| **D, dynamic** | suits awake, missiles, salvage in flight | unreliable datagrams: per-entity deltas against acked baselines, priority-filled, under a byte budget | per tick, bounded |
| **S, structural** | homes, doors, furniture, parked suits, settled salvage, rocks' wear | versioned records, sent until acked (rocks and chunks today) or on the control stream (homes) | nothing between changes |
| **G, global** | the clock, the colony's day, notices, `tidi_pct` | header bits and control frames | nearly nothing |

Things change class: a suit parked in a hide spot is S until it wakes, a drifting chunk is a
closed-form segment sent once. What is a closed form of the tick (the bodies, the city, the
traffic, the walkers) is in no class at all: it costs nothing.

## The wire's road

In order. Each step is measured with `cargo bench -p bc-sector --bench sector` before and after.

1. **Acks for every datagram.** Each snapshot datagram carries a sequence number and a stream (the
   sector and the seat); each input packet acks the newest and the 32 before it as bits (Gaffer's
   reliable-ordered header, Gates' `ack_bits`). A datagram is applied and acked whole or not at
   all.
2. **Per-entity deltas against acked baselines** (Gaffer's snapshot compression and his networked
   VR physics; Gates' `is_delta`). The accumulator sends subsets, so a baseline is per entity:
   the last state of that entity the client acked. A record is coded against it in quanta (the
   position against where the baseline's velocity carried it), and falls back to an absolute
   record from the same encoder when there is no safe baseline: no keyframes, no second path.
   Titanfall 2 more than halved its servers' bandwidth and CPU by rewriting this.
3. **Parts under a budget.** More than one datagram a tick for a client with more to see (Gates'
   `SNAPSHOT_PARTS`), under a per-client byte bucket. Part one is today's snapshot; the rest carry
   entities. Egress is free on GameLift since June 2026, so a fleet fight can buy bytes instead of
   dropping suits.
4. **Interest from the grid, shared work once a tick** (Unreal's Replication Graph and Iris, EVE's
   finding that a big fight's cost is per observer, not the simulation). Quantize each entity once
   a tick for everyone; a client's candidates come from the cells its sensors reach. Perception
   (the drones', the Orrery's) uses the same grid (`SCALE.md`, A1–A2).
5. **Network LOD tiers** inside the accumulator: by distance and sensor range (Unreal's
   per-distance frequency, Dual Universe's 100 m and 700 m, Star Citizen's 5 px rule) and by threat
   and interaction (Halo Reach). A suit 20 km off is a blip a second, not a record a tick.
6. **The crowd.** The colony's people and traffic stay closed forms; a person becomes an entity of
   the interior sector only when something happens to them (FiveM culls the same way; Gaffer's
   1,000-player space game simulates its cosmetic props on the client). The pilots on foot, who
   are public on a street with no sensor model to hide them, may go as Quake Golf's lockstep
   words: the changed inputs of a strip, coded once, broadcast to it, stepped by every client.
   Measure it against deltas before choosing.

## One world

- **A gateway** in front of the sectors (Star Citizen's Gateway, New World's four entry points
  before seven hubs). The browser keeps one WebTransport session; the gateway routes its
  datagrams to the process running its sector, so a handoff is a re-route, not a handshake.
- **Sectors are processes**, static, placed in Earth-Moon space. A hot one is reinforced on a
  bigger machine (EVE's reinforced nodes, announced before the fight), never split while it runs.
- **One writer per entity.** Handoff carries an authority sequence (Gaffer's VR authority and
  ownership sequences): the receiving sector takes the suit on a tick both agree on, with its state
  in body-local terms. In deep space a transfer orbit hides it; where a seam must not show,
  neighbours replicate a band either side to each other, read-only.
- **The colony** is split along its 32 km into districts that keep one clock (`spawn_follower`
  already does it for the inside). The city is one closed form, so neighbours can't disagree.
- **TiDi per sector** (EVE). Under overload the tick stretches and the header's `tidi_pct` slows the
  clients' clocks. Wall-clock timers (rent, a raid's window) never dilate.
- **Shared state leaves the process.** Pilot records, homes and the Charter Board go to a database
  with row writes (the `PilotStore` trait is the seam; New World writes 800k rows every 30 s); the
  exchange is one service with one book; the radio is a pub/sub.
- **The record.** A sector logs each tick's quantized inputs, everything non-deterministic (joins,
  loadouts, the Orrery's advice, restores) and periodic hashes (Quake Golf's referee). That is a
  replay, a desync's repro, a killcam, and a standby that takes the sector over on the same
  timeline (Quake Golf's N4: every player kept their slot). It stays on the server, so it leaks
  nothing.

## Trust

- **The sensor model is the anti-maphack.** A client learns only what its suit detects
  (`Sim::detects`), so nothing it isn't told can be read out of it. Any field we replicate is
  public (Gates): what must stay hidden is never sent.
- **The server decides how far back a shot looks.** Today the shot carries its view time, clamped
  to 8 ticks. Gates' `favour_for` mints the depth from the client's ack evidence and caps it by
  QUIC's measured RTT, so a client that acks late gets less help, never more. Overwatch stops
  rewinding past about 220 ms.
- **Closed forms checked in the field.** The Hello carries a build hash, and clients can report a
  sliced hash of the bodies' poses and the chunks' segments at a tick (Quake Golf's referee), so a
  client that drifted is caught and sent the seed again, not left wrong.
- **Floods** are for the network in front of the server; the server caps connections, handshakes
  and rates (`ARCHITECTURE.md`, "Under abuse").

## Turned down

- **Lockstep for suits.** Quake Golf puts 256 players in a room by sending only inputs, and every
  client then holds the whole world: the sensor model and the Eclipse become a maphack, every
  browser simulates every suit, the weakest machine sets the pace, and lag compensation is gone.
  Lockstep is for public things only (the crowd, maybe).
- **Peer to peer** (GTA Online's 30, Elite's instances): the worst peer sets the game, and there is
  no authority to trust.
- **Generic meshing middleware** (SpatialOS: Worlds Adrift and Mavericks closed, Fractured queued
  at a few hundred players). We own the handoff code.
- **Sectors that split themselves while running.** Star Citizen's took eight years and still
  doesn't divide a crowded city; Ashes of Creation closed before it shipped. Static sectors,
  reinforced, and TiDi.
- **Fragmented datagrams**, and state too big for one: big things go on a reliable stream (a
  sector's join state), in slices.
- **Absolute records forever.** Fine for 64 pilots; the reason 200 suits in view refresh four times
  a second.

## What we watch

- `cargo bench -p bc-sector --bench sector`: the tick (the simulation and replication apart), bytes
  a datagram, records and bits a record, the share sent as deltas, for 64 to 448 pilots in one
  fight.
- `/status`: `hot_path_allocations` at 0, the tick's histogram under budget, and the split.
- The swarm (`bc-bot`) at 256 and 512 pilots.

## Sources

- Glenn Fiedler, Gaffer On Games: [Snapshot compression](https://gafferongames.com/post/snapshot_compression/),
  [State synchronization](https://gafferongames.com/post/state_synchronization/),
  [Snapshot interpolation](https://gafferongames.com/post/snapshot_interpolation/),
  [Networked physics in virtual reality](https://gafferongames.com/post/networked_physics_in_virtual_reality/),
  [Reliable ordered messages](https://gafferongames.com/post/reliable_ordered_messages/),
  [Packet fragmentation](https://gafferongames.com/post/packet_fragmentation_and_reassembly/),
  [Deterministic lockstep](https://gafferongames.com/post/deterministic_lockstep/).
- Más Bandwidth: [Choosing the right network model](https://mas-bandwidth.com/choosing-the-right-network-model-for-your-multiplayer-game/),
  [So I'm making a space game](https://mas-bandwidth.com/so-im-making-a-space-game/),
  [Titanfall and Apex history](https://mas-bandwidth.com/some-history-around-titanfall-and-apex-legends-multiplayer/).
- EVE Online: [Time dilation](https://eveonline.com/article/introducing-time-dilation-tidi),
  [Fixing lag: drakes of Destiny](https://www.eveonline.com/article/fixing-lag-drakes-of-destiny-part-1-1).
- Star Citizen: [Server meshing](https://starcitizen.tools/Server_meshing),
  [Object container streaming](https://starcitizen.tools/Object_Container_Streaming).
- New World: [the architecture behind it](https://aws.amazon.com/blogs/gametech/the-unique-architecture-behind-amazon-games-seamless-mmo-new-world).
- Unreal: [Iris](https://dev.epicgames.com/documentation/en-us/unreal-engine/introduction-to-iris-in-unreal-engine).
- FiveM: [OneSync](https://docs.fivem.net/docs/scripting-reference/onesync).
- Valve: [Source multiplayer networking](https://developer.valvesoftware.com/wiki/Source_Multiplayer_Networking).
- Gates (`AnthonE/Gates`): `NETCODE.md` (the classes, deltas, parts, `favour_for`).
- Quake Golf (`AnthonE/Quake-Golf`): `README.md` and `docs/wire.md` (lockstep words, the referee's
  hashes, relay trees, failover).
