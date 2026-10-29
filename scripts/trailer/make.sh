#!/usr/bin/env bash
# The alpha trailer, cut in a browser from the game's own footage and its own sound, the way Gates
# cuts its (Gates' ci/trailer):
#
#   scripts/trailer/make.sh                 # → scripts/trailer/out/before-colony-alpha.mp4 (1080p), -720p.mp4, poster.jpg
#   scripts/trailer/make.sh stills          # the store's stills (shots/stills.json) → out/stills/*.jpg, 1920x1080
#   scripts/trailer/make.sh shoot [names]   # just film the trailer's footage (shots/trailer.json)
#   scripts/trailer/make.sh frames 16,40.5  # trailer frames as JPEGs in build/frames, to check a cut
#   scripts/trailer/make.sh preview         # every 4th frame at 720p, quick to check a cut
#   scripts/trailer/make.sh battle [names]  # the fight reel (shots/battle.json) → out/before-colony-battle.mp4
#
# overhaul.sh is a quicker cut for seeing new work move (shots/overhaul.json: the ring, the Ring Deck,
# the Muster), captioned and cross-faded over the title theme; shots/survey.json stills every camera.
#
# Footage: film.mjs plays each shot's showcase scene (`?showcase=`, offline, on a fixed clock) in
# headless Chromium and steps its clock a frame at a time through the client's film hook
# (`window.__bcFilm`), so SwiftShader's pace never shows. trailer.html draws every frame of the
# cut as a pure function of t; render.mjs screenshots it and pipes the frames to ffmpeg; mix.py
# lays the title theme and the bank's cues (bc-sound's `bank` example: every sound the game
# makes, synthesized) and sfx.py's trailer hits on the cue list the page exports.
#
# Needs the browser build in web/dist (scripts/build-web.sh; BC_PREBUILT=1 uses what is there),
# ffmpeg, node with Playwright, python3 with numpy.
set -euo pipefail
cd "$(dirname "$0")"
HERE="$PWD"
ROOT="$(cd ../.. && pwd)"
fail() { echo "trailer: $*" >&2; exit 1; }
command -v ffmpeg >/dev/null || fail "no ffmpeg"
command -v node >/dev/null || fail "no node"
if [ -z "${PLAYWRIGHT_BROWSERS_PATH:-}" ] && [ -d /opt/pw-browsers ]; then
  export PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers
fi

MODE=video; ARG=""
case "${1:-}" in
  "") ;;
  stills|preview) MODE="$1" ;;
  battle) MODE=battle; ARG="${2:-}" ;;
  shoot) MODE=shoot; ARG="${2:-}" ;;
  frames) MODE=frames; ARG="${2:?frames t1,t2,…}" ;;
  *) fail "unknown option $1" ;;
esac

B="$HERE/build"
mkdir -p "$B"/{fonts,clips,snd,sfx,node_modules} "$HERE/out"
[ -e "$B/node_modules/playwright" ] || ln -sfn "$(npm root -g)/playwright" "$B/node_modules/playwright"
cp film.mjs render.mjs trailer.html "$B/"

# The page the footage is filmed from: the browser build, served as it is (the showcase needs no
# game server).
[ "${BC_PREBUILT:-0}" = 1 ] || (cd "$ROOT" && scripts/build-web.sh webgl2)
[ -s "$ROOT/web/dist/webgl2/bc_bg.wasm" ] || fail "no browser build in web/dist: scripts/build-web.sh"
PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
python3 -m http.server "$PORT" --bind 127.0.0.1 -d "$ROOT/web/dist" >/dev/null 2>&1 &
GAME=$!
trap 'kill $GAME 2>/dev/null; kill ${SERVER:-} 2>/dev/null' EXIT
export BC_URL="http://127.0.0.1:$PORT"
sleep 0.5

if [ "$MODE" = stills ]; then
  node "$B/film.mjs" shots/stills.json "$HERE/out/stills"
  ls -la "$HERE/out/stills"; exit 0
fi
if [ "$MODE" = battle ]; then
  # The fight reel: the battle showcase's skirmish and more, cut end to end and scored (reel.py).
  node "$B/film.mjs" shots/battle.json "$B/battle" $ARG
  [ -s "$B/snd/music_combat.wav" ] || cargo run -q -p bc-sound --release --example bank -- "$B/snd"
  python3 reel.py shots/battle.json "$B/battle" "$B" "$HERE/out/before-colony-battle.mp4"
  exit 0
fi
node "$B/film.mjs" shots/trailer.json "$B/clips" $ARG
[ "$MODE" = shoot ] && exit 0
python3 - "$B/clips" <<'PY'
import glob, json, os, sys
d = sys.argv[1]
index = {os.path.basename(p): len(glob.glob(os.path.join(p, "*.jpg"))) for p in sorted(glob.glob(os.path.join(d, "*"))) if os.path.isdir(p)}
json.dump(index, open(os.path.join(d, "index.json"), "w"))
print("trailer: footage", index)
PY

# What the page letters with: the game's own faces.
cp "$ROOT"/web/fonts/*.ttf "$B/fonts/"
# What the mix plays: the game's bank (the title theme among it) and the trailer's hits.
[ -s "$B/snd/music_title.wav" ] || cargo run -q -p bc-sound --release --example bank -- "$B/snd"
[ -s "$B/sfx/trailer_boom.wav" ] || python3 sfx.py "$B/sfx" >/dev/null

PORT2=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
python3 -m http.server "$PORT2" --bind 127.0.0.1 -d "$B" >/dev/null 2>&1 &
SERVER=$!
export TRAILER_URL="http://127.0.0.1:$PORT2/trailer.html" TRAILER_Q=""
sleep 0.5

if [ "$MODE" = frames ]; then
  node "$B/render.mjs" stills "$B/frames" "$ARG"
  echo "trailer: frames in $B/frames"; exit 0
fi

node "$B/render.mjs" cues "$B/cues.json"
python3 mix.py "$B" "$B/cues.json" "$B/mix.wav"
LN=$(ffmpeg -hide_banner -nostats -i "$B/mix.wav" -af loudnorm=I=-14:TP=-1.5:LRA=11:print_format=json -f null - 2>&1 \
  | python3 -c "import sys, json; t = sys.stdin.read(); j = json.loads(t[t.rindex('{'):]); print(':'.join(f'{k}={j[v]}' for k, v in (('measured_I', 'input_i'), ('measured_TP', 'input_tp'), ('measured_LRA', 'input_lra'), ('measured_thresh', 'input_thresh'), ('offset', 'target_offset'))))")
AF="loudnorm=I=-14:TP=-1.5:LRA=11:$LN:linear=true"
NAME=before-colony-alpha

if [ "$MODE" = preview ]; then
  TRAILER_STEP=4 node "$B/render.mjs" video "$B/preview.mkv"
  ffmpeg -v error -y -i "$B/preview.mkv" -i "$B/mix.wav" -map 0:v -map 1:a -af "$AF" -ar 48000 \
    -vf scale=1280:720 -r 30 -c:v libx264 -preset veryfast -crf 26 -pix_fmt yuv420p -c:a aac -b:a 160k -shortest \
    "$HERE/out/$NAME-preview.mp4"
  ls -la "$HERE"/out/; exit 0
fi

# In ten-second parts, each its own browser, kept until the cut changes.
STAMP=$(cat trailer.html "$B/clips/index.json" | sha256sum | cut -c1-16)
[ "$(cat "$B/parts/.stamp" 2>/dev/null)" = "$STAMP" ] || { rm -rf "$B/parts"; mkdir -p "$B/parts"; echo "$STAMP" > "$B/parts/.stamp"; }
TOTAL=$(python3 -c "import json; print(json.load(open('$B/cues.json'))['TOTAL'])")
N=$(python3 -c "import math; print(math.ceil($TOTAL / 10))")
: > "$B/parts/list.txt"
for i in $(seq 0 $((N - 1))); do
  part="$B/parts/$(printf %02d "$i").mkv"
  if [ ! -s "$part" ]; then
    node "$B/render.mjs" video "$B/parts/tmp.mkv" $((i * 10)) $(python3 -c "print(min($TOTAL, ($i + 1) * 10))")
    mv "$B/parts/tmp.mkv" "$part"
  fi
  echo "file '$part'" >> "$B/parts/list.txt"
done
ffmpeg -v error -y -f concat -safe 0 -i "$B/parts/list.txt" -c copy "$B/silent.mkv"
ffmpeg -v error -y -i "$B/silent.mkv" -i "$B/mix.wav" -map 0:v -map 1:a -af "$AF" -ar 48000 \
  -c:v libx264 -preset slow -crf 19 -maxrate 8M -bufsize 16M -pix_fmt yuv420p -c:a aac -b:a 192k -movflags +faststart -shortest \
  "$HERE/out/$NAME.mp4"
ffmpeg -v error -y -i "$HERE/out/$NAME.mp4" -vf scale=1280:720:flags=lanczos -c:v libx264 -preset slow -crf 23 -maxrate 3M -bufsize 6M \
  -pix_fmt yuv420p -c:a copy -movflags +faststart "$HERE/out/$NAME-720p.mp4"
# The poster: the title drop, where the page says it is.
POSTER=$(python3 -c "import json; print(json.load(open('$B/cues.json'))['POSTER'])")
node "$B/render.mjs" stills "$B/poster" "$POSTER"
cp "$(ls "$B"/poster/*.jpg | head -1)" "$HERE/out/poster.jpg"
ls -la "$HERE"/out/
