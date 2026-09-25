#!/usr/bin/env bash
# Build the browser client and run a local sector with Mobile Dolls, an AI agent and a miner.
#   scripts/dev.sh            then open http://127.0.0.1:8080 in Chrome or Edge
# Env: BC_DOLLS (default 24), BC_AGENTS (default 1), BC_MINERS (default 1),
#      BC_ORACLE (local|jev; jev needs TYPESAFE_API_KEY)
set -euo pipefail
cd "$(dirname "$0")/.."
./scripts/build-web.sh webgl2
cargo build --release -p bc-server -p bc-bot --bins --examples
pids=()
cleanup() { for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT INT TERM
./target/release/bc-server --mobile-dolls "${BC_DOLLS:-24}" --oracle "${BC_ORACLE:-local}" &
pids+=($!)
sleep 1
for i in $(seq 1 "${BC_AGENTS:-1}"); do
  ./target/release/examples/mobile_doll --name "Agent-$(printf %02d "$i")" &
  pids+=($!)
done
for i in $(seq 1 "${BC_MINERS:-1}"); do
  ./target/release/examples/miner --name "Miner-$(printf %02d "$i")" &
  pids+=($!)
done
echo
echo "  Before Colony is up: open http://127.0.0.1:8080   (add ?autopilot=1 to watch the Mobile Doll brain fly)"
echo
wait "${pids[0]}"
