"""Cut a reel from a shot list filmed by film.mjs, end to end, and score it (make.sh battle).

    python3 reel.py SHOTS.json CLIPS_DIR BUILD_DIR OUT.mp4

Each shot's frames (CLIPS_DIR/<name>/001.jpg…) follow the last; the list's `music` plays under the
whole cut, and each shot plays the sounds of its scene's `events` that fall inside it (a scene's
events repeat every `loop` s, as its script does). The mix is mix.py's; BUILD_DIR holds the bank
(snd/*.wav, bc-sound's `bank` example).
"""
import json
import math
import os
import subprocess
import sys
from pathlib import Path


def main(shots_path, clips, build, out):
    doc = json.load(open(shots_path))
    fps = doc.get("fps", 30)
    clips, build = Path(clips), Path(build)
    seq = build / "reel"
    seq.mkdir(parents=True, exist_ok=True)
    for f in seq.glob("*.jpg"):
        f.unlink()
    k, at, sfx = 0, 0.0, []
    for shot in doc["shots"]:
        n = round(shot["len"] * fps)
        for i in range(1, n + 1):
            k += 1
            os.symlink((clips / shot["name"] / f"{i:03d}.jpg").resolve(), seq / f"{k:05d}.jpg")
        ev = doc.get("events", {}).get(shot["scene"])
        if ev:
            t0, t1, loop = shot["t"], shot["t"] + n / fps, ev["loop"]
            for c, cue, gain in ev["cues"]:
                for j in range(math.floor((t0 - c) / loop), math.floor((t1 - c) / loop) + 1):
                    when = c + j * loop
                    if t0 <= when < t1:
                        sfx.append({"src": f"snd/{cue}.wav", "at": at + when - t0, "gain": gain})
        at += n / fps
    m = doc["music"]
    music = [{"src": m["src"], "at": 0, "in": m.get("in", 0), "len": at, "gain": m["gain"],
              "fade_in": m.get("fade_in", 0.3), "fade_out": m.get("fade_out", 2.0)}]
    cues = build / "reel_cues.json"
    json.dump({"CUES": {"total": at, "music": music, "sfx": sfx}}, open(cues, "w"))
    here = Path(__file__).resolve().parent
    subprocess.run([sys.executable, str(here / "mix.py"), str(build), str(cues), str(build / "reel.wav")], check=True)
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-framerate", str(fps), "-i", str(seq / "%05d.jpg"),
                    "-i", str(build / "reel.wav"), "-map", "0:v", "-map", "1:a",
                    "-af", "loudnorm=I=-14:TP=-1.5:LRA=11", "-ar", "48000",
                    "-c:v", "libx264", "-preset", "slow", "-crf", "19", "-pix_fmt", "yuv420p",
                    "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart", "-shortest", out], check=True)
    print(f"reel: {k} frames, {at:.1f} s, {len(sfx)} sounds -> {out}")


if __name__ == "__main__":
    main(*sys.argv[1:5])
