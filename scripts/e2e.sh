#!/usr/bin/env bash
# Runs a Playwright suite against a freshly started server.
#   scripts/e2e.sh spike [project]   # transport smoke test (server in echo mode)
#   scripts/e2e.sh slice [project]   # the vertical slice (game mode, dolls + an agent bot)
set -euo pipefail
cd "$(dirname "$0")/.."
suite="${1:-slice}"
project="${2:-webgl2}"
port="${BC_HTTP_PORT:-8080}"
export BC_URL="http://127.0.0.1:${port}"
export PLAYWRIGHT_BROWSERS_PATH="${PLAYWRIGHT_BROWSERS_PATH:-/opt/pw-browsers}"

cargo build --release -p bc-server -p bc-bot --bins --examples
pids=()
cleanup() { for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT

case "$suite" in
  spike)
    ./target/release/bc-server --mode echo --http "127.0.0.1:${port}" --web-dir web/dist &
    pids+=($!)
    ;;
  slice)
    ./target/release/bc-server --mode game --http "127.0.0.1:${port}" --web-dir web/dist --mobile-dolls "${BC_DOLLS:-24}" &
    pids+=($!)
    sleep 1
    ./target/release/examples/mobile_doll --server "$BC_URL" --name "Agent-01" &
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
"${runner[@]}" pnpm exec playwright test "$suite" --project="$project"
