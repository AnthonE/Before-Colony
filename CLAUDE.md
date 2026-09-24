# Before Colony — notes for AI assistants

A Gundam Wing space MMO (free aim, Newtonian 6DOF). Rust server, Bevy 0.19 client compiled to wasm,
WebTransport (QUIC) between them. See `docs/ARCHITECTURE.md` and `docs/DESIGN.md`.

## Hot-path rules (non-negotiable)
- The sector tick (`bc-sim` step + `bc-sector` input drain/encode) must not allocate or lock.
  Storage is sized at construction (`Box<[T]>`); `bc-sim/src/storage.rs` is the only allocation site.
- `bc-proto` is `no_std` with no `alloc` — it cannot allocate by construction.
- `bc-sim` and `bc-sector` have `clippy.toml` bans (Mutex/RwLock/String/HashMap/`vec!`/`format!`/`Box::new`…).
- Threads talk only through preallocated lock-free queues (`rtrb`, crossbeam `ArrayQueue`).
  The sector thread never wakes tokio; it `unpark()`s the egress thread.
- Proof: the `no_alloc` tests count heap operations with `bc-alloc::CountingAlloc` and must stay at 0.
- Determinism: use `bc_sim::math` (libm) for trig; never enable glam `fast-math` or wasm `simd128`.

## Commands
- `scripts/ci.sh` — everything CI runs (`BC_E2E=1` adds the browser tests).
- `scripts/dev.sh` — build the web client, run a sector with Mobile Dolls and an AI agent.
- `cargo test --workspace --release` — all native tests (bc-client is a no-op natively; release
  because the simulation-heavy tests are slow unoptimised).
- `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo clippy -p bc-client --target wasm32-unknown-unknown -- -D warnings`
- `scripts/build-web.sh [webgl2] [webgpu]` — browser build into `web/dist/` (needs wasm-bindgen-cli 0.2.128).
- `cargo run -p bc-server --release` then open http://127.0.0.1:8080
- `scripts/e2e.sh spike|slice [webgl2|webgpu]` — Playwright against a real server.
- Never set `RUSTFLAGS` (it would drop the `web_sys_unstable_apis` cfg from `.cargo/config.toml`).
