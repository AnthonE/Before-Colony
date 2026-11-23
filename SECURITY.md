# Security and cheating

Before Colony's server runs the world and a client only asks it things. These are its promises,
each with the code that keeps it and the test that proves it. Breaking one, privately reported, is
a paid finding (below). Two of them are **landing**: they are being built this round (`docs/NOW.md`,
"In flight") and hold once their branch merges, with its test.

## The promises

1. **The server decides.** A client sends inputs (`bc_proto::InputCmd`) and, on foot, poses; flight,
   combat, damage, the hangar, the fabricator, the Exchange and the Charter Board are the server's.
   - A client is told only what its suit's sensors detect (`Sim::detects`; `docs/NETCODE.md`,
     "Trust"), so a hacked client has nothing hidden to draw. The Eclipse and the hide spots hold:
     `crates/bc-sim/tests/concealment.rs`.
   - A shot looks back at most 8 ticks, 267 ms (`MAX_REWIND_TICKS` in
     `crates/bc-sim/src/config.rs`): `crates/bc-sim/tests/lagcomp.rs`, `rewind_is_clamped`.
   - On foot the server passes on only a pose that could happen (`crates/bc-server/src/plaza.rs`,
     and `street.rs` for the ring's town), and in the old city's homes nobody gets past a door its
     register keeps them out of: `crates/bc-server/tests/plaza.rs`,
     `crates/bc-server/tests/foot_plausible.rs`.
   - Signing in proves the wallet: the server rebuilds the message the wallet signed and checks
     the signature, or the Elo Pros pass (`crates/bc-auth`): `crates/bc-server/tests/login.rs`.
   - The side a pilot flies for is the server's: `crates/bc-server/tests/sides.rs`.
   - Connections, sign-ins and every client's rates are capped (`docs/ARCHITECTURE.md`, "Under
     abuse"): `crates/bc-server/tests/admit.rs`.
2. **No client can make the tick allocate, lock or wait.** The sector's storage is sized when it's
   built and its queues are bounded: `crates/bc-sim/tests/no_alloc.rs` and
   `crates/bc-sector/tests/no_alloc_sector.rs` count heap operations and hold them at 0, and a
   live server reports its own on `/status` (`hot_path_allocations`).
3. **A suit's weapons are safe inside the colony, except in the Blast Hall.** In the sector of
   the colony's inside (the old city's today, behind the server's `--colony`; suits come inside the
   ring later, `docs/RING.md`), a suit's fire, locks, specials and blades are cleared before the
   tick, and what's fired in the hall touches no suit and never leaves it
   (`colony::hall::weapons_free`, and `Sim::colony_law` in `crates/bc-sim/src/sim/mod.rs`):
   `crates/bc-sim/tests/interior.rs`, `weapons_are_safe_inside`,
   `training_rounds_touch_no_suit_and_never_leave_the_hall` and
   `out_through_the_blast_doors_weapons_are_safe_again`. This is about suits. **On foot it is open
   season anywhere** (the owner's decision, `docs/DECISIONS.md`, 2026-11-23): pilots on foot
   hurting each other is the game, and the Watch answers it.
   - **Landing:** pilots in suits can't harm each other within the colony's reach (5 km from the
     ring by default, `docs/DECISIONS.md`), and the colony itself never takes fire
     (`docs/NOW.md` §pvp-range). Until it lands, every human pilot flies for the Colonies and
     friendly fire is off (`docs/PEERS.md` 10), so pilots in suits can't harm each other at all.
4. **Signatures and resume tokens are never logged.** A signature's `Debug` names its length and
   never its bytes, and so does any message carrying one (`crates/bc-proto/src/auth.rs`:
   `crates/bc-proto/tests/secrets.rs`); a wallet in a log is shortened (`pilots::short`); an
   agent's key file is readable by its owner only (`bc_bot::key`). Resume tokens are kept out of
   logs by review today: `docs/NOW.md` §sec-token gives them a signature's `Debug`.
5. **Landing: voice is never stored or logged.** Proximity voice is being built (`docs/NOW.md`
   §voice). Once it lands, the server passes a pilot's voice to those in range and keeps none of
   it: not on disk, not in a log, not in a report.

Aimbots and aim assistance are not here: the lock-on already moves a suit about its target by
design (`docs/LOCK.md`), and an input a person could have sent is legal.

## Paid findings

One award per underlying finding, to its first valid report.

| severity | what |
|---|---|
| **Critical** | playing as a wallet you don't hold, or a forged pass taken; credits, items or standing minted, or taken from another pilot's hangar; a client learning what its suit's sensors don't detect, through the real client path |
| **High** | breaking the server's authority (speed, rate of fire, damage, a craft or trade the rules refuse); a hit from further back than the rewind's bound; an on-foot pose or a home's door the plaza should refuse; a suit's weapon firing inside the colony outside the Blast Hall, or the hall's rounds reaching a suit or leaving it; a signature or resume token written to a log. Once each lands: a suit harming a pilot within the colony's reach, the colony taking fire, voice stored or logged |
| **Medium** | one client's crafted traffic (not volume) crashing or stalling a sector or the server for everyone on it, or making the tick allocate |

**The award for each severity is posted on Before Colony's board**
(<https://elopros.com/games/before-colony/board/>) by the owner; this file never types a figure.
Until one is posted there is no security bounty: a finding is still welcome and credited, and
nothing is owed.

## How to report

Privately, through GitHub's private vulnerability reporting: the repository's **Security** tab,
**Report a vulnerability**. Never in a public issue, pull request, comment or test name.

The owner has to turn that reporting on for the repository (Settings, Code security). Until the
Security tab offers it, open a public issue titled "security contact" that says nothing about the
finding, and the owner will open a private channel.

A good report names the commit, the steps and the evidence: best of all, a failing test in the
shape of the promise's own (above), or a `bc-bot` agent that does it (`crates/bc-bot`). If an agent
helped, name the exact harness, provider and model.

## Rules for testing

- Test against your own server: `cargo run -p bc-server --release`, with your own clients and
  agents (`crates/bc-bot`).
- Never against a server with other people on it, and never volume.
- No one else's wallet, account or data.
- **Out of scope:** volumetric denial of service (floods are for the network in front of the
  server: `docs/ARCHITECTURE.md`, "Under abuse"); the development HTTP server's routes (`/status`,
  `/floor.json` and the rest: deployed, they sit behind a proxy); elopros.com and the services we
  call; social engineering; aimbots; and what the design allows (pilots on foot fighting anywhere,
  the Watch's answer to it).

We fix first and disclose after, with credit if you want it.
