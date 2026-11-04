#!/bin/sh
# The local check (CLAUDE.md, MVP mode): format, then a warnings-clean
# compile of the workspace AND both of the browser client's builds
# (bc-client compiles only for wasm, so only these two clippies see it), then
# the tests of any crate named, in release (the simulation-heavy tests are
# slow unoptimised). Minutes cold, seconds warm. scripts/ci.sh is what CI runs.
#
#   scripts/quick.sh                   # fmt + the three clippies
#   scripts/quick.sh bc-sim bc-sector  # ...then those crates' tests
set -eu
cd "$(dirname "$0")/.."
cargo fmt --all
cargo clippy -q --workspace --all-targets -- -D warnings
cargo clippy -q -p bc-client --target wasm32-unknown-unknown -- -D warnings
cargo clippy -q -p bc-client --target wasm32-unknown-unknown --features webgpu -- -D warnings
for crate in "$@"; do
  cargo test -q --release -p "$crate"
done
echo "quick: OK"
