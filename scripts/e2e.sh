#!/usr/bin/env bash
# Runs a Playwright suite against a freshly started server.
#   scripts/e2e.sh spike [project]   # transport smoke test (server in echo mode)
#   scripts/e2e.sh slice [project]   # the vertical slice (game mode, dolls + an agent bot)
#   scripts/e2e.sh gfx [project]     # every showcase scene renders cleanly (BC_GFX_QUALITY=high)
#   scripts/e2e.sh frames [project]  # the autopilot flies each Gundam's kit against the dolls
#   scripts/e2e.sh lockon [project]  # Y locks on to a Doll, W carries the suit in, holding Y lets go
#   scripts/e2e.sh ui [project]      # the title screen, the menu, reconnecting, disconnecting
#   scripts/e2e.sh login [project]   # wallet sign-in (a stub wallet with a test key), resume, take-over
#   scripts/e2e.sh hangar [project]  # survival: on foot in the bay, its terminals, launching and docking
#   scripts/e2e.sh surface [project] # the lander lands in MO-II's Aft Well, hides, parks and wakes there
#   scripts/e2e.sh colony [project]  # survival, the colony open: the cap lift, another pilot at Hub Gate, the streets, the Exchange floor, a sight
#   scripts/e2e.sh inside [project]  # survival, the colony open: Q at the cockpit launches the suit into the colony, it flies there, docks back at the inner gate
#   scripts/e2e.sh chart [project]   # the chart: the Earth Sphere and back, a course to MO-II's Aft Well, the auto-nav flying it
# Anything after the project goes to Playwright, e.g. `scripts/e2e.sh gfx webgl2 --grep "duel|hangar"`.
# The suits' suites run the arcade rules (any frame, free respawns) unless BC_RULES says otherwise,
# and every game-mode suite the anime flight rules unless BC_FLIGHT (anime|real) does.
set -euo pipefail
cd "$(dirname "$0")/.."
suite="${1:-slice}"
project="${2:-webgl2}"
port="${BC_HTTP_PORT:-8080}"
export BC_URL="http://127.0.0.1:${port}"
export PLAYWRIGHT_BROWSERS_PATH="${PLAYWRIGHT_BROWSERS_PATH:-/opt/pw-browsers}"

cargo build --release -p bc-server -p bc-bot -p bc-auth --bins --examples
pids=()
cleanup() { for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT

case "$suite" in
  spike|gfx)
    # The showcase runs offline; the echo server just serves the page.
    ./target/release/bc-server --mode echo --http "127.0.0.1:${port}" --web-dir web/dist &
    pids+=($!)
    ;;
  slice)
    ./target/release/bc-server --mode game --rules "${BC_RULES:-arcade}" --flight "${BC_FLIGHT:-anime}" --http "127.0.0.1:${port}" --web-dir web/dist --mobile-dolls "${BC_DOLLS:-24}" &
    pids+=($!)
    sleep 1
    ./target/release/examples/mobile_doll --server "$BC_URL" --name "Agent-01" &
    pids+=($!)
    ;;
  frames)
    ./target/release/bc-server --mode game --rules "${BC_RULES:-arcade}" --flight "${BC_FLIGHT:-anime}" --http "127.0.0.1:${port}" --web-dir web/dist --mobile-dolls "${BC_DOLLS:-24}" &
    pids+=($!)
    ;;
  lockon)
    # A few Dolls to lock on to.
    ./target/release/bc-server --mode game --rules "${BC_RULES:-arcade}" --flight "${BC_FLIGHT:-anime}" --http "127.0.0.1:${port}" --web-dir web/dist --mobile-dolls "${BC_DOLLS:-8}" &
    pids+=($!)
    ;;
  ui)
    ./target/release/bc-server --mode game --rules "${BC_RULES:-arcade}" --flight "${BC_FLIGHT:-anime}" --http "127.0.0.1:${port}" --web-dir web/dist --mobile-dolls "${BC_DOLLS:-4}" &
    pids+=($!)
    ;;
  login)
    # No dolls: an idle pilot shot down can't sleep (a wreck is simply gone).
    ./target/release/bc-server --mode game --rules "${BC_RULES:-arcade}" --flight "${BC_FLIGHT:-anime}" --http "127.0.0.1:${port}" --web-dir web/dist --mobile-dolls 0 &
    pids+=($!)
    ;;
  chart)
    # No dolls: nothing interrupts the auto-nav's flight.
    ./target/release/bc-server --mode game --rules "${BC_RULES:-arcade}" --flight "${BC_FLIGHT:-anime}" --http "127.0.0.1:${port}" --web-dir web/dist --mobile-dolls 0 &
    pids+=($!)
    ;;
  surface)
    # No dolls: nothing hunts the hider (and a wreck can't park).
    ./target/release/bc-server --mode game --rules "${BC_RULES:-arcade}" --flight "${BC_FLIGHT:-anime}" --http "127.0.0.1:${port}" --web-dir web/dist --mobile-dolls 0 &
    pids+=($!)
    ;;
  hangar)
    # Survival rules; the fabricator works fast so a job finishes inside the test.
    ./target/release/bc-server --mode game --rules survival --flight "${BC_FLIGHT:-anime}" --craft-speed 60 --http "127.0.0.1:${port}" --web-dir web/dist --mobile-dolls 0 &
    pids+=($!)
    ;;
  colony|inside)
    # Survival rules with the colony open; no dolls. An agent strolls outside Hub Gate.
    ./target/release/bc-server --mode game --rules survival --flight "${BC_FLIGHT:-anime}" --colony --http "127.0.0.1:${port}" --web-dir web/dist --mobile-dolls 0 &
    pids+=($!)
    sleep 1
    ./target/release/examples/flaneur --server "$BC_URL" --name "Flaneur-01" &
    pids+=($!)
    ;;
  *) echo "unknown suite $suite" >&2; exit 1 ;;
esac

for _ in $(seq 1 50); do curl -sf "$BC_URL/status" >/dev/null && break; sleep 0.2; done
cd e2e
[ -d node_modules ] || pnpm install --frozen-lockfile=false
mkdir -p artifacts
runner=()
[ "$project" = "webgpu" ] && runner=(xvfb-run -a)
"${runner[@]}" pnpm exec playwright test "${suite}.spec" --project="$project" "${@:3}"
