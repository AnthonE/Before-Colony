# Scale: what stops the world getting bigger

The goal is GTA in space: one world people live in, from the city's streets to the far side of the
Moon, with cars, crowds, crime and fleets, and many more players than a match holds. This is where
the code stands against that, what breaks first, and the order to fix it in. `ARCHITECTURE.md`'s
"Scaling path" is the short form; this is the long one, with numbers.

## What we have

One server process runs one world: `sector-0` (space, ±32.768 km) and, under survival, `sector-1`
(the colony's inside), which keeps `sector-0`'s clock. The tick is allocation-free and lock-free, and
it is fast. Nothing else is shared between sectors, so building more is mostly a matter of routing.

**Measured** (`cargo bench -p bc-sector --bench sector`, 4 cores, release). Every pilot joins
at the same spawn and flies firing, so the whole crowd is in one fight and on everyone's
sensors. This is the worst case for replication.

| Pilots | Drones | Tick p50 | Tick p99 | Max | Out, total |
|---:|---:|---:|---:|---:|---:|
| 64 | 256 | 1.2 ms | 2.5 ms | 4.5 ms | 2.1 MB/s |
| 128 | 256 | 2.6 ms | 5.0 ms | 10.8 ms | 4.2 MB/s |
| 256 | 256 | 6.4 ms | 12.3 ms | 13.7 ms | 8.4 MB/s |
| 448 | 64 | 13.2 ms | 25.2 ms | 29.2 ms | 14.7 MB/s |

The budget is 33 ms at 30 Hz. The tick grows faster than linearly: doubling the pilots costs
about 2.3× the time. Every snapshot is full: 1,093 B, 32.8 KB/s a pilot, whatever the crowd.

**So one sector holds about 300 pilots in one fight with room to spare, and 450 at the edge.**
That is already EVE-class for one grid. The ceilings below come before that one.

## The bottlenecks, worst first

### 1. One process is the whole world (structural)

- There is no gateway, no sector per process and no handoff (`ARCHITECTURE.md`, "Scaling path"
  items 2–3, unbuilt).
- Every session holds a `sector-0` lease for its whole life, in the bay and on foot too
  (`bc-server/src/net/session.rs`). So `--max-clients` (64) caps **connections per process**,
  not pilots flying.
- What survival keeps is process-local: the pilot registry (`pilots.rs`), the exchange
  (`market.rs`), the Charter Board, the Proving Ground and the homes are each a `Mutex` in this
  process. Each is saved whole to its own JSON file, and the last writer wins. Two processes
  can't share any of them.

Until this changes, the world is one server's worth. Nothing else on this list matters as much.

### 2. The tick's own costs that grow as n²

- **Replication is per client over every suit**, on the sector thread
  (`Sector::replicate` → `replicate::build_snapshot`):
  - Each client walks every suit slot, twice.
  - It also walks every rock in the field, every chunk (1,023) and every missile.
  - That is O(clients × entities) a tick, and the bench's curve is mostly this.
- **Perception never uses the spatial hash.**
  - `Sim::perceive_into` queries a sphere of `sensor_range × 1.3`, which is 8–13 km.
  - With 128 m cells, that box is far over the hash's 4,096-cell limit, so `SpatialHash::query_box`
    falls back to visiting every item.
  - Every drone's and every Orrery pilot's look at the world is a full scan, so they cost O(n²).
  - Only weapons, melee and missiles, whose queries are short, get the hash's benefit.

### 3. The wire: one datagram a tick, absolute states

- A snapshot is one datagram of at most 1,100 B (`MAX_DATAGRAM`), sent every tick.
- A free suit's record is 211 bits, so about 30 entities fit beside the header and own state.
  The priority accumulator shares those 30 places among everyone in view. With 200 suits in
  sight, each one is refreshed about four times a second.
- Every record is absolute: there is no delta against what the client acked. Rocks and chunks
  are the exception, since they are sent dirty-until-acked.
- IDs are 10 bits (`SLOT_BITS`, `CHUNK_BITS`, `ROCK_BITS`, `MISSILE_BITS`), so a sector holds at
  most 1,023 of each.
- Positions are 21 bits over ±32,768 m (3.1 cm), so a sector is a 65 km cube.

These limits are right for a sector. They are why the world must be **many sectors**, never one
big one, and why a crowd needs another kind of record than a duelling suit.

### 4. The crowd on foot is off the tick and quadratic

- The plaza (`bc-server/src/plaza.rs`) is one `Mutex<HashMap>` of everyone on foot.
- Each viewer, ten times a second, filters everyone on its strip into a fresh `Vec`, sorts the
  result and keeps 48 (`MAX_PEOPLE`, one datagram's worth). That is O(N² log N) on the two tokio
  workers, which also carry every session, the economy and HTTP.
- Past 48 nearby, the farther people are simply not shown. There is no accumulator here as
  there is for suits.
- `OPEN_CITY.md`'s S1 (people and cars as entities of `sector-1`'s tick) is what crime, cars and
  the Watch need. Done with today's replication, S1 inherits bottleneck 2.

### 5. Egress and the runtime

- Each sector has one egress thread, which walks every slot's ring and calls `send_datagram`
  serially.
- The process has one QUIC endpoint (one UDP socket) and two tokio workers (`main.rs`).
- The plaza's datagrams don't use the egress at all: each session sends its own from its task.
- At about 15 MB/s for 450 pilots this holds. Encryption and the syscall per datagram land on
  one or two cores, though, and that is the next wall after bottleneck 2.

### 6. The client: built at start-up, one f32 frame for space

- **Nothing is streamed but the city.**
  - All content is compiled into the wasm: shaders, fonts, and audio that is synthesised at boot.
  - Meshes are procedural: every suit model at every LOD, every landmark and every rock is built
    at start-up.
  - Each rock is three entities (`rocks.rs`).
  - Rocks and landmarks switch their LOD in a CPU loop over the whole set, every frame.
  - The city alone streams: a quadtree of chunks at four LODs, built in 1.5–4 ms a frame, with
    1,400 cached (`bc-client/src/city.rs`).
- **No floating origin in space.**
  - The city renders around an f64 `RenderOrigin` that re-centres every 1 km.
  - Space is plain f32 world coordinates. That is fine within one 65 km sector, but not for a
    world of sectors, or a camera in one sector drawing its neighbour.
- **No size budget for the wasm**, and no CI check on it.
- **Per-entity cost:**
  - A suit is 26 bone entities plus effects.
  - A person in the plaza is a whole figure tree, with no cap, and the map of who is wanted is
    rebuilt every frame (`people.rs`).

What's in our favour: almost everything is a closed form of a seed and the tick (the bodies, the
city, its traffic and its walkers). So **streaming the world means streaming seeds and versions,
not meshes**. The client already rebuilds the field from the server's seed.

## The plan

`NETCODE.md` is the netcode of record this plan builds towards. Each step pays on its own, in this order. The bench is the yardstick: rerun it before and after.

### A. Make one sector cheap (days)

1. **Hash perception properly.** Give `SpatialHash` a coarse level, such as 2 km cells, for
   sensor-range queries, or bucket by sector region. Then perception and the drones' looks cost
   what is near, not what exists. Keep the `no_alloc` tests at 0.
2. **Replicate from the hash, not the slot list.** Candidates for a client come from the cells
   within its sensor range. Rocks and chunks come from per-cell dirty lists, not full scans.
3. **Encode off the sim thread** (scaling path item 4). Snapshot building reads a
   double-buffered world from N workers, each fed by preallocated rings. It is embarrassingly
   parallel per client.
4. **Measure the split.** Time the sim step and replication apart in the bench and on
   `/status`, so we know which half to cut.

The target is 512 pilots in one fight at p99 ≤ 16 ms.

### B. Sessions that don't hold a flight slot (days)

A pilot in the bay, on foot or in a menu holds a session, not a sector lease. They lease one at
launch, as `launch_inside` already does for `sector-1`.

- Connections per process go from 64 to a few thousand. `net::admit` already caps them at 512.
- Raise `--max-connections` once the plaza (C) is fixed.
- More tokio workers, or one per core.

### C. The crowd as a sector, with interest tiers (weeks)

- Put the plaza on a spatial grid, off the global mutex: per-strip cells, each its own small
  lock-free list. Add a priority accumulator like the suits have.
- Build S1 (`OPEN_CITY.md`) on step A's hashed replication.
- **Tiered records:**
  - Near: full state at 30 Hz, as now.
  - Mid: position and heading only, at a lower rate, in a few bits relative to a cell's corner.
  - Far: counts and blips (the "crowd at Hub Gate"), not people.
- **Delta against the last acked state** for the near tier.
- **More than one datagram a tick** for a client in a crowd, under a per-client budget (say
  64 KB/s). Today it is one datagram whatever happens.

GTA Online's 30 in a session is the bar to beat for a street. What it takes is 150 people on a
block, each drawn and each able to be hit.

### D. The world as many sectors (months)

- **A gateway** (the scaling path's item 2). It terminates QUIC and routes each session's
  datagrams to the process running its sector. Sectors run one to a thread, and one process
  per sector once they are big.
- **Handoff:**
  - In deep space, a timed transfer orbit (`DESIGN.md`, "The world") hides it: the pilot's
    record moves while they fly the transfer.
  - Where a seam must not show (the colony and the space around it, or a fleet fight on a line),
    neighbours replicate a band either side of the border to each other read-only. A suit that
    crosses changes owner on a tick both agree on, with its state in body-local terms.
  - Bodies are already sector-local, and a suit only hands off flying free, which keeps this
    tractable.
- **The colony's inside split along its 32 km.** Each district becomes a sector with the same
  clock (`spawn_follower` already does one). Where the neighbours meet, the city is one closed
  form, so it can't disagree with itself.
- **Shared state out of the process:**
  - Pilot records, homes and the Charter Board go to a database with row-level writes, such as
    Postgres. The `PilotStore` trait is already the seam.
  - The exchange is one service with its own book, since it is one market.
  - The radio becomes a pub/sub.
- **Dynamic sectors:**
  - When a sector overruns, TiDi first (`tidi_pct` is in the header and unused).
  - A sector that runs hot for long splits in two along its busiest plane, and an empty one
    merges.

### E. A client that streams the world (weeks, alongside D)

- **A floating origin everywhere**, generalising the city's `RenderOrigin` to space. Positions
  are f64 on the client, rendered relative to the camera, and each sector is a frame placed in
  the next one up (Earth-Moon space).
- **Build by distance.** Rocks, landmarks and a sector's bodies are built when they come within
  range and freed when they leave, as the city's chunks already are. Each needs a frame-time
  budget per frame. Instance the rocks.
- **Content by manifest.** A sector's Welcome names its seed and content versions. Anything
  hand-made (textures, voice, music, authored interiors) is fetched over HTTP from a CDN, not
  compiled in. Keep the wasm to code and a small boot set, with a size budget checked in CI.
- **A cap on the crowd's cost.** Pool the plaza's figures as `life` pools its people. Draw
  far people as `life` does, a few draws for many.

## What we watch

- `cargo bench -p bc-sector --bench sector`: the table above, rerun with every step of A.
- `/status`: `hot_path_allocations` stays at 0, and the tick histogram stays under budget.
- The swarm (`bc-bot`) at 256 and 512 pilots before and after B and C.
