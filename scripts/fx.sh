#!/usr/bin/env bash
# The fx stage in the browser: one effect at a time in front of a dark card and a light card,
# a frame per step of the showcase's fixed clock (held by the film hook), each composed into a
# strip with its numbers and a diff against the last run (crates/bc-client/src/fx_stage.rs,
# scripts/fxstrip.mjs, the fx skill).
#
#   scripts/fx.sh list                              # every effect it can stage
#   scripts/fx.sh kill hit-beam-rifle               # → e2e/artifacts/fx/<name>/strip.png, numbers.json, diff.png
#   scripts/fx.sh kill --quality low                # the low tier (no HDR, no bloom); default high
#   scripts/fx.sh kill --frames 15 --span 2         # more frames over a shorter span
#   scripts/fx.sh --no-build …                      # reuse web/dist
#
# Builds web/dist/webgl2 (scripts/build-web.sh) and serves it itself: no game server. Needs
# wasm-bindgen-cli 0.2.128 and Playwright's Chromium (cd e2e && pnpm i, or a global one).
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=${FX_OUT:-e2e/artifacts/fx}
BUILD=1
QUALITY=high
NAMES=()
MORE=()
while [ $# -gt 0 ]; do
  case "$1" in
    --no-build) BUILD=0 ;;
    --quality) QUALITY=$2; shift ;;
    --frames|--span|--size|--steps) MORE+=("$1" "$2"); shift ;;
    -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
    *) NAMES+=("$1") ;;
  esac
  shift
done
[ ${#NAMES[@]} -gt 0 ] || { sed -n '2,15p' "$0"; exit 2; }
if [ "${NAMES[0]}" = list ]; then
  # fx_stage::list(): hit-/muzzle- for every WeaponKind, then the fixed ones.
  kinds=$(awk '/pub const ALL: \[WeaponKind/{f=1;next} f&&/\];/{exit} f' crates/bc-proto/src/types.rs \
    | sed -n 's/.*WeaponKind::\([A-Za-z]*\).*/\1/p' | sed 's/\([a-z]\)\([A-Z]\)/\1-\2/g' | tr 'A-Z' 'a-z')
  for k in $kinds; do echo "hit-$k"; done
  for k in $kinds; do echo "muzzle-$k"; done
  grep -o 's("[a-z-]*"\.into()' crates/bc-client/src/fx_stage.rs | sed 's/s("\([a-z-]*\)".*/\1/'
  exit 0
fi
if [ "$BUILD" = 1 ]; then
  scripts/build-web.sh webgl2
fi
[ -f web/dist/webgl2/bc_bg.wasm ] || { echo "fx: no web/dist/webgl2: run without --no-build" >&2; exit 1; }
for n in "${NAMES[@]}"; do
  node scripts/fxstrip.mjs page "$OUT/$n" --serve web/dist --hook bc \
    --query "showcase=fx&fx=$n&quality=$QUALITY&clean=1&gfx=webgl2" "${MORE[@]}"
done
