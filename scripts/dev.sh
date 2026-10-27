#!/usr/bin/env bash
# Build the browser client and run a local sector with drones, an AI agent and a miner.
#   scripts/dev.sh            then open http://127.0.0.1:8080 in Chrome or Edge
# Env: BC_DRONES (default 24), BC_AGENTS (default 1), BC_MINERS (default 1),
#      BC_ORACLE (local|jev; jev needs TYPESAFE_API_KEY),
#      BC_RULES (survival|arcade; default survival), BC_FLIGHT (anime|real; default anime: the
#      tank is a boost gauge that refills), BC_DATA (a directory: keep pilots, hangars and the
#      exchange across restarts), BC_COLONY=1 (open the retired cylinder's inside, closed by
#      default while the ring is built, docs/RING.md: the bay's airlock leads down into its city,
#      where an agent strolls outside each Hub Gate), BC_AGENT_KEYS (a directory: each agent signs in with its own wallet,
#      <name>.key there, made on the first run; without it they fly as guests)
set -euo pipefail
cd "$(dirname "$0")/.."
./scripts/build-web.sh webgl2
cargo build --release -p bc-server -p bc-bot --bins --examples
pids=()
cleanup() { for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT INT TERM
data=()
[ -n "${BC_DATA:-}" ] && data=(--data-dir "$BC_DATA")
# The colony's inside is closed while the ring is built; BC_COLONY=1 opens the old one (survival only).
colony=0
[ "${BC_RULES:-survival}" = "survival" ] && [ "${BC_COLONY:-0}" = "1" ] && colony=1
[ "$colony" = "1" ] && data+=(--colony)
./target/release/bc-server --drones "${BC_DRONES:-24}" --oracle "${BC_ORACLE:-local}" --rules "${BC_RULES:-survival}" --flight "${BC_FLIGHT:-anime}" "${data[@]}" &
pids+=($!)
sleep 1
# An agent's wallet, if BC_AGENT_KEYS names where they're kept (a path without spaces).
[ -n "${BC_AGENT_KEYS:-}" ] && mkdir -p "$BC_AGENT_KEYS" && chmod 700 "$BC_AGENT_KEYS"
key() { if [ -n "${BC_AGENT_KEYS:-}" ]; then echo "--key=$BC_AGENT_KEYS/$1.key"; fi; }
for i in $(seq 1 "${BC_AGENTS:-1}"); do
  ./target/release/examples/drone --name "Agent-$(printf %02d "$i")" $(key "Agent-$(printf %02d "$i")") &
  pids+=($!)
done
for i in $(seq 1 "${BC_MINERS:-1}"); do
  ./target/release/examples/miner --name "Miner-$(printf %02d "$i")" $(key "Miner-$(printf %02d "$i")") &
  pids+=($!)
done
# The colony open: someone strolling outside each strip's Hub Gate, and two more come to sit
# outside The Arrival.
if [ "$colony" = "1" ]; then
  for strip in 0 1 2; do
    ./target/release/examples/flaneur --name "Flaneur-0$((strip + 1))" --strip "$strip" $(key "Flaneur-0$((strip + 1))") &
    pids+=($!)
  done
  for k in 4 5; do
    ./target/release/examples/flaneur --name "Flaneur-0$k" --sit --seat "$k" $(key "Flaneur-0$k") &
    pids+=($!)
  done
fi
echo
echo "  Before Colony is up: open http://127.0.0.1:8080   (add ?autopilot=1 to watch the drone brain fly)"
echo
wait "${pids[0]}"
