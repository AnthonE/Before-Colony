# Contributing to Before Colony

Before Colony is a space mech MMO in the browser: free aim, Newtonian flight, a ring colony with a
town on its floor. The server runs every tick, the simulation is deterministic and allocation-free,
and the colony and its town are closed forms every screen agrees on, so almost every claim about
the game can be checked by running something. The paid work below is built on that.

People and agents are equally welcome. Use any harness and any model, and say which in the PR.
`AGENTS.md` is where an agent starts; `CLAUDE.md` is the whole process.

## What gets paid

| work | what counts | pay |
|---|---|---|
| **An accepted pull request** | anything in scope below, merged | the board's posted price |
| **Content that ships** | only if the owner posts a bonus on the board (a base building that opens, a frame that ships) | what that bonus posts |
| **A security finding** | a confirmed break of a promise in [`SECURITY.md`](SECURITY.md), reported privately | by severity, as the board posts |

**The price is the board's, and this repo never types it.** Read it at
<https://elopros.com/games/before-colony/board/>, or:

```sh
curl -s 'https://elopros.com/api/munus?game=before-colony'
```

**The lane opens when it's posted there.** Until that board lists the standing bounty
`before-colony-pr`, nothing is posted and nothing is owed; PRs are still welcome.

**How you get paid.** Put your wallet on the PR description's `Wallet:` line: an EVM `0x` address
(the board can't pay a Solana address), and deliver the PR on the board with one signed call
(`AGENTS.md`, "The deal"). Or sign in at <https://elopros.com/login.html> with the GitHub that
opened the PR and link a wallet there. Nothing is owed until the PR is merged; then it's paid in
the reserve, at the pool's mark on the day it's paid (the board's row names the coin), by public
transfer, which is the receipt. The board takes at most 3 deliveries a wallet a UTC day.

Contributions land under the repo's MIT terms ([`LICENSE`](LICENSE)): opening a PR is offering it
on them.

## Scope

**In scope**

- **The items in [`docs/NOW.md`](docs/NOW.md)'s "Buildable now"**, 0.2 first. Name the item's label
  on the PR's first line (`Closes NOW.md §fr-v1`).
- **Bugs you can show**: a failing test, a `scripts/pt` shot, an e2e trace, a report that the fix
  turns green.
- **The client on real hardware**: a GPU, browser or OS where the page fails, fixed with the
  device named.
- **Frame time on the reference machine** (`docs/NOW.md` §perf-ref), with numbers from real
  hardware.

**Out of scope**, closed by quoting this list

- The items `docs/NOW.md` lists as "In flight (house)": the house is building them this round.
- Docs-only PRs, and README landscaping.
- UI or copy changes, unless they fix a broken flow; anything that breaks `docs/UI.md`'s rules.
- New frames, modes, weapons or systems that no area doc has.
- Edits to vendored code: `crates/fx`, `scripts/fxstrip.mjs` and `scripts/board.mjs` are Gates' and
  the agent kit's, fixed there and synced.
- Edits to the owner's pages: `docs/DECISIONS.md`, `docs/PHASES.md`.
- Art files: every suit, building, person and car is procedural, by rule (`docs/DESIGN.md`).
- Names from another franchise: every name is the game's own (`docs/STORY.md`, "Voice and naming").

## Setting up

`CLAUDE.md`, "Fresh box", in short:

- **The toolchain is pinned** (`rust-toolchain.toml`: the compiler, clippy, rustfmt and the
  `wasm32-unknown-unknown` target); rustup installs it on the first `cargo` command.
- **`wasm-bindgen-cli` exactly 0.2.128:** `cargo install wasm-bindgen-cli --version 0.2.128 --locked`.
  `scripts/build-web.sh` refuses any other.
- **Never set `RUSTFLAGS`**, in a shell, a script or a workflow. It replaces `.cargo/config.toml`'s
  flags and drops the `web_sys_unstable_apis` cfg, and the WebGPU build breaks without a word.
- **The browser client compiles only for wasm.** `bc-client` is a no-op natively, so a green native
  build says nothing about it; `scripts/quick.sh` runs its two wasm clippies (WebGL2 and WebGPU).
- **The browser suites** need Node 18+ and pnpm, then `cd e2e && pnpm i` and Playwright's Chromium.
- **Tests in debug crawl.** Run them in release, as `scripts/quick.sh` does.

## The check a PR must pass

`scripts/quick.sh` green (format, the native clippy and both wasm clippies), then what you
touched:

| touched | run |
|---|---|
| any crate | `scripts/quick.sh` with the crates' names: their tests, in release |
| the simulation or the sector's tick (`bc-sim`, `bc-sector`) | their tests: the `no_alloc` proofs stay at 0 and the goldens hold. Determinism on wasm: `cargo test -p bc-sim --target wasm32-unknown-unknown --release --test determinism` |
| the wire (`bc-proto`, the Welcome, `bc_econ::wire`) | an incompatible change bumps `PROTOCOL_VERSION` in `crates/bc-proto/src/lib.rs` (the handshake then refuses older clients); additive JSON an older page ignores needs no bump (`docs/OPEN_CITY.md`, "On the wire"). New `Request` and `Update` variants go at the end of their enums: a merge convention, not a wire rule |
| the town's or the old city's layout | `TOWN_VERSION` (`crates/bc-sim/src/content/town.rs`) or `CITY_VERSION` (`crates/bc-sim/src/content/city.rs`) bumped with the protocol, and each golden that moves named in the commit |
| the page or a screen | `scripts/build-web.sh page`, then `scripts/e2e.sh gallery` (it photographs every screen) |
| an effect | `scripts/fx.sh` with the effect's name: its strip on the fx stage |
| a shader | derivatives taken at the top of `fragment()`, before any branch (`CLAUDE.md`, rule 6) |
| how anything looks or plays | a look check in the PR: a `scripts/pt shot`, an fx strip or the gallery's shot |
| an area a browser suite covers | `scripts/e2e.sh` with the suite and `webgl2` (the suites are listed in `scripts/e2e.sh`) |
| the docs that name files | `sh scripts/refs.sh` |

`scripts/ci.sh` is everything CI runs, in order. The browser suites run nightly
(`.github/workflows/e2e.yml`); SwiftShader is neither the look nor the speed (`CLAUDE.md`'s traps).

## What gets a PR closed

Green is the floor, not the bar (the owner, of Gates: *"not any ai slop but the best here"*). A PR
is closed, even with every check green, when:

- its check is red, or wasn't run;
- an allocation or a lock is smuggled into the tick, behind a warm-up or a cfg;
- a test or a golden was edited to make a wall pass, or a golden moved with no rule change that
  explains it (fix the change, never the wall);
- an incompatible wire change without the `PROTOCOL_VERSION` bump, or the layout without its version;
- trig goes around `bc_sim::math`, or glam's `fast-math` or wasm's `simd128` is switched on;
- a shader takes a derivative under a branch;
- `RUSTFLAGS` is set anywhere;
- it's more than one change, a giant mixed diff, comment churn, or landscaping dressed as work;
- the code ignores the surrounding idiom;
- agent-written work has no `Agent:` line;
- a security finding was posted in public (report it privately: `SECURITY.md`);
- it builds what the house has in flight (`docs/NOW.md`);
- it names something after another franchise, or says in the game what `docs/STORY.md` says is
  never said.

**A flat price makes this list the only filter.** The board pays the same for a one-line fix as
for a system, so the question at review is never "was this big enough?", only the list above. A
small correct change is welcome and paid like anything else; a small change dressed as work is
what the list closes. Five thin PRs that each need an afternoon of review are worth less than one
that lands clean, and each is judged on its own. A closed PR stays closed: do what the review left
in a new one.

## The PR

Fill in the template: what changed and why (the `docs/NOW.md` label, the report or the bug), the
checks you ran, the look check, `Agent:` with the exact harness, provider and model (or `person`),
and `Wallet:` if you want paying. One change per PR. Nobody needs to claim anything first.

## Reviews

A review that finds a real problem, or confirms a change, is welcome as a PR comment.
