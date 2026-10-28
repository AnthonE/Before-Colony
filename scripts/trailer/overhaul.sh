#!/usr/bin/env bash
# A quick cut of a shot list's clips (shots/overhaul.json by default): each clip's frames as filmed by
# film.mjs, its "caption" lettered over it in the game's faces, cross-faded, over the title theme
# (bc-sound's bank). For seeing new work move, not the store's trailer (make.sh cuts that).
#
#   scripts/trailer/overhaul.sh                       # film what's missing, then cut → out/overhaul.mp4
#   BC_PREBUILT=1 scripts/trailer/overhaul.sh         # with the browser build already in web/dist
#   BC_DIST=dir scripts/trailer/overhaul.sh           # film from another build of the page
#   scripts/trailer/overhaul.sh cut                   # only cut what's filmed
set -euo pipefail
cd "$(dirname "$0")"
HERE="$PWD"
ROOT="$(cd ../.. && pwd)"
LIST="${BC_SHOTS:-shots/overhaul.json}"
B="$HERE/build"
CLIPS="$B/$(basename "$LIST" .json)"
mkdir -p "$B/node_modules" "$HERE/out"
if [ -z "${PLAYWRIGHT_BROWSERS_PATH:-}" ] && [ -d /opt/pw-browsers ]; then export PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers; fi
[ -e "$B/node_modules/playwright" ] || ln -sfn "$(npm root -g)/playwright" "$B/node_modules/playwright"
cp film.mjs "$B/"

if [ "${1:-}" != cut ]; then
  DIST="${BC_DIST:-$ROOT/web/dist}"
  [ -n "${BC_DIST:-}" ] || [ "${BC_PREBUILT:-0}" = 1 ] || (cd "$ROOT" && scripts/build-web.sh webgl2)
  PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
  python3 -m http.server "$PORT" --bind 127.0.0.1 -d "$DIST" >/dev/null 2>&1 &
  SERVER=$!
  trap 'kill $SERVER 2>/dev/null' EXIT
  sleep 0.5
  BC_URL="http://127.0.0.1:$PORT" node "$B/film.mjs" "$LIST" "$CLIPS"
fi
[ -s "$B/snd/music_title.wav" ] || (cd "$ROOT" && cargo run -q -p bc-sound --release --example bank -- "$B/snd")
python3 - "$LIST" "$CLIPS" "$ROOT/web/fonts" "$B/snd/music_title.wav" "$HERE/out/$(basename "$LIST" .json).mp4" <<'PY'
import json, os, subprocess, sys
listp, clips, fonts, music, out = sys.argv[1:6]
L = json.load(open(listp))
fps, (W, H) = L.get("fps", 30), L.get("size", [1280, 720])
X = 0.4  # each cross-fade, s
title, sub = f"{fonts}/ChakraPetch-SemiBold.ttf", f"{fonts}/ShareTechMono-Regular.ttf"
esc = lambda t: t.replace("\\", "\\\\").replace(":", "\\:").replace("'", "’").replace("%", "\\%")
ins, fil, lens = [], [], []
for i, s in enumerate(L["shots"]):
    ins += ["-framerate", str(fps), "-i", f"{clips}/{s['name']}/%03d.jpg"]
    d = len(os.listdir(f"{clips}/{s['name']}")) / fps
    lens.append(d)
    last = i == len(L["shots"]) - 1
    chain = [f"scale={W}:{H}", "setsar=1", "format=yuv420p"]
    for k, text in enumerate(s.get("caption") or []):
        big = k == 0
        if last:
            # The title over the last shot, coming up as it settles.
            size = H // (9 if big else 28)
            y = f"h/2-{size}" if big else f"h/2+{H // 18}"
            t0 = 0.9 + 0.3 * k
            alpha = f"min(1,max(0,(t-{t0})/0.6))"
        else:
            # A title low in the frame and a line under it, in after the cut and out before the next.
            size = H // (16 if big else 30)
            y = f"h-{H // 5}+{k * (H // 13)}"
            t0 = 0.35 + 0.25 * k
            alpha = f"min(1,max(0,(t-{t0})/0.4))*min(1,max(0,({d - 0.15:.3f}-t)/0.4))"
        chain.append(f"drawtext=fontfile={title if big else sub}:text='{esc(text)}':fontsize={size}:fontcolor=white:"
                     f"x=(w-text_w)/2:y={y}:alpha='{alpha}':shadowcolor=black@0.7:shadowx=2:shadowy=2")
    if last:
        chain.append(f"fade=t=out:st={d - 0.6:.3f}:d=0.6")
    fil.append(f"[{i}:v]{','.join(chain)}[s{i}]")
# Cross-faded one into the next.
acc, t = "[s0]", lens[0]
for i in range(1, len(lens)):
    fil.append(f"{acc}[s{i}]xfade=transition=fade:duration={X}:offset={t - X:.3f}[x{i}]")
    acc, t = f"[x{i}]", t + lens[i] - X
total = t
fil.append(f"[{len(lens)}:a]atrim=0:{total:.3f},afade=t=in:d=0.3,afade=t=out:st={total - 2.0:.3f}:d=2.0,loudnorm=I=-16:TP=-1.5[a]")
graph = ";".join(fil)
cmd = ["ffmpeg", "-v", "error", "-y", *ins, "-i", music, "-filter_complex", graph, "-map", acc, "-map", "[a]",
       "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "160k",
       "-ar", "48000", "-movflags", "+faststart", "-t", f"{total:.3f}", out]
subprocess.run(cmd, check=True)
print(f"overhaul: {len(lens)} clips, {total:.1f} s -> {out}")
PY
