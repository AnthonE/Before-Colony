#!/usr/bin/env bash
# Everything CI checks, in order. E2E runs only when BC_E2E=1 (needs Chromium; see e2e/).
set -euo pipefail
cd "$(dirname "$0")/.."
step() { echo; echo "==> $*"; }

step "format";               cargo fmt --all -- --check
step "clippy (native)";      cargo clippy --workspace --all-targets -- -D warnings
step "clippy (wasm client)"; cargo clippy -p bc-client --target wasm32-unknown-unknown -- -D warnings
step "clippy (wasm, webgpu)"; cargo clippy -p bc-client --target wasm32-unknown-unknown --features webgpu -- -D warnings
step "tests";                cargo test --workspace --release
step "determinism on wasm";  cargo test -p bc-sim --target wasm32-unknown-unknown --release --test determinism
step "tick benchmark";       cargo bench -p bc-sim --bench tick -- --warm-up-time 1 --measurement-time 2
if [ "${BC_E2E:-0}" = "1" ]; then
  step "web build";          ./scripts/build-web.sh webgl2
  step "e2e: transport";     ./scripts/e2e.sh spike webgl2
  step "e2e: vertical slice"; ./scripts/e2e.sh slice webgl2
  step "e2e: graphics";      ./scripts/e2e.sh gfx webgl2
  step "e2e: the Gundams";   ./scripts/e2e.sh frames webgl2
  step "e2e: lock-on";       ./scripts/e2e.sh lockon webgl2
  step "e2e: the page";      ./scripts/e2e.sh ui webgl2
  step "e2e: sign-in";       ./scripts/e2e.sh login webgl2
  step "e2e: survival";      ./scripts/e2e.sh hangar webgl2
  step "e2e: on the bodies";  ./scripts/e2e.sh surface webgl2
  step "e2e: the colony";     ./scripts/e2e.sh colony webgl2
fi
echo; echo "all green"
