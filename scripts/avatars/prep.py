#!/usr/bin/env python3
"""The avatars people wear on the Exchange floor (docs/FLOOR.md), made small enough to send.

    python3 scripts/avatars/prep.py SOURCE_DIR [OUT_DIR]

SOURCE_DIR holds the originals, as their authors publish them (see web/assets/avatars/LICENSE.md
for where each comes from); OUT_DIR (default web/assets/avatars) gets one .glb each. A VRoid model
(.vrm, glTF underneath) is ~15 MB, mostly textures and 410 facial blend shapes nobody on the floor
sees; what comes out is ~1 MB:

  - the morph targets go (the face keeps its rest expression), and the VRM extension with them;
  - each material keeps its base colour only (the models are unlit: anime cel colour, no normal,
    shade, rim or emission maps), and the images nothing uses any more go;
  - each image is cut to at most MAX_PX a side and stored as a palette PNG (cel colour quantises
    cleanly); alpha is kept where the material uses it;
  - the arms come down out of the T-pose to a relaxed stance, baked into the bones' rest rotations,
    so a model stands like a person with no animation playing.

A .glb (the android) is copied as it is: it is already small, and its own animations play.
Needs Pillow. Deterministic: the same sources give the same bytes.
"""
from __future__ import annotations

import io
import json
import math
import shutil
import struct
import sys
from pathlib import Path

from PIL import Image

MAX_PX = 512
# The face's texture carries the eyes' surround and the mouth: a little more of it reads at a glance.
FACE_PX = 1024


def read_glb(path: Path) -> tuple[dict, bytes]:
    b = path.read_bytes()
    magic, _, _ = struct.unpack_from("<III", b, 0)
    if magic != 0x46546C67:
        raise ValueError(f"{path} is not a glb")
    clen, _ = struct.unpack_from("<II", b, 12)
    j = json.loads(b[20 : 20 + clen])
    off = 20 + clen
    blen, _ = struct.unpack_from("<II", b, off)
    return j, b[off + 8 : off + 8 + blen]


def write_glb(path: Path, j: dict, binary: bytes) -> None:
    js = json.dumps(j, separators=(",", ":"), sort_keys=True).encode()
    js += b" " * (-len(js) % 4)
    binary += b"\0" * (-len(binary) % 4)
    total = 12 + 8 + len(js) + 8 + len(binary)
    out = struct.pack("<III", 0x46546C67, 2, total)
    out += struct.pack("<II", len(js), 0x4E4F534A) + js
    out += struct.pack("<II", len(binary), 0x004E4942) + binary
    path.write_bytes(out)


def quat_axis(axis: tuple[float, float, float], deg: float) -> list[float]:
    s, c = math.sin(math.radians(deg) / 2), math.cos(math.radians(deg) / 2)
    return [axis[0] * s, axis[1] * s, axis[2] * s, c]


# The relaxed stance, for VRoid's skeleton (VRM 0: the model faces −Z, its left arm along −X, every
# bone's rest rotation the identity): upper arms down beside the body, elbows a little bent forward,
# hands turned in.
STANCE = {
    "J_Bip_L_UpperArm": quat_axis((0, 0, 1), 72.0),
    "J_Bip_R_UpperArm": quat_axis((0, 0, 1), -72.0),
    "J_Bip_L_LowerArm": quat_axis((0, 1, 0), -14.0),
    "J_Bip_R_LowerArm": quat_axis((0, 1, 0), 14.0),
    "J_Bip_L_Hand": quat_axis((1, 0, 0), 8.0),
    "J_Bip_R_Hand": quat_axis((1, 0, 0), 8.0),
}


def shrink_image(data: bytes, px: int, alpha: bool) -> bytes:
    im = Image.open(io.BytesIO(data))
    im.load()
    im = im.convert("RGBA" if alpha else "RGB")
    if max(im.size) > px:
        k = px / max(im.size)
        im = im.resize((max(1, round(im.width * k)), max(1, round(im.height * k))), Image.LANCZOS)
    q = im.quantize(colors=256, method=Image.Quantize.FASTOCTREE if alpha else Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
    out = io.BytesIO()
    q.save(out, format="PNG", optimize=True)
    return out.getvalue()


def prep_vrm(src: Path, dst: Path) -> None:
    j, binary = read_glb(src)
    # No faces to pull, no VRM.
    for mesh in j["meshes"]:
        mesh.pop("weights", None)
        mesh.pop("extras", None)
        for p in mesh["primitives"]:
            p.pop("targets", None)
            p.pop("extras", None)
    j.pop("extensions", None)
    j["extensionsUsed"] = [e for e in j.get("extensionsUsed", []) if e != "VRM"]
    # Base colour only.
    keep_images: dict[int, tuple[bool, int]] = {}
    for m in j["materials"]:
        for k in ("normalTexture", "emissiveTexture", "occlusionTexture"):
            m.pop(k, None)
        m.pop("extras", None)
        pbr = m.setdefault("pbrMetallicRoughness", {})
        pbr.pop("metallicRoughnessTexture", None)
        pbr["metallicFactor"] = 0.0
        pbr["roughnessFactor"] = 1.0
        t = pbr.get("baseColorTexture")
        if t is not None:
            img = j["textures"][t["index"]]["source"]
            alpha = m.get("alphaMode", "OPAQUE") != "OPAQUE"
            face = "Face_00_SKIN" in (m.get("name") or "")
            prev = keep_images.get(img, (False, 0))
            keep_images[img] = (prev[0] or alpha, max(prev[1], FACE_PX if face else MAX_PX))
    # The stance.
    for n in j["nodes"]:
        if n.get("name") in STANCE:
            n["rotation"] = STANCE[n["name"]]
    # Rebuild the buffer from what's still used: the accessors' views, and the kept images, shrunk.
    used_acc: set[int] = set()
    for mesh in j["meshes"]:
        for p in mesh["primitives"]:
            used_acc.update(p["attributes"].values())
            if "indices" in p:
                used_acc.add(p["indices"])
    for s in j.get("skins", []):
        if "inverseBindMatrices" in s:
            used_acc.add(s["inverseBindMatrices"])
    for a in j.get("animations", []):
        for smp in a["samplers"]:
            used_acc.update((smp["input"], smp["output"]))
    old_views = j["bufferViews"]
    new_views: list[dict] = []
    out = bytearray()

    def add_view(data: bytes, extra: dict) -> int:
        out.extend(b"\0" * (-len(out) % 4))
        v = {"buffer": 0, "byteOffset": len(out), "byteLength": len(data), **extra}
        out.extend(data)
        new_views.append(v)
        return len(new_views) - 1

    acc_map: dict[int, int] = {}
    new_accs: list[dict] = []
    view_map: dict[int, int] = {}
    for i in sorted(used_acc):
        a = dict(j["accessors"][i])
        v = old_views[a["bufferView"]]
        if a["bufferView"] not in view_map:
            data = binary[v.get("byteOffset", 0) : v.get("byteOffset", 0) + v["byteLength"]]
            extra = {k: v[k] for k in ("byteStride", "target") if k in v}
            view_map[a["bufferView"]] = add_view(data, extra)
        a["bufferView"] = view_map[a["bufferView"]]
        acc_map[i] = len(new_accs)
        new_accs.append(a)
    img_map: dict[int, int] = {}
    new_imgs: list[dict] = []
    for i in sorted(keep_images):
        im = j["images"][i]
        v = old_views[im["bufferView"]]
        data = binary[v.get("byteOffset", 0) : v.get("byteOffset", 0) + v["byteLength"]]
        alpha, px = keep_images[i]
        img_map[i] = len(new_imgs)
        new_imgs.append({"name": im.get("name"), "mimeType": "image/png",
                         "bufferView": add_view(shrink_image(data, px, alpha), {})})
    # Point everything at the new indices.
    for mesh in j["meshes"]:
        for p in mesh["primitives"]:
            p["attributes"] = {k: acc_map[v] for k, v in p["attributes"].items()}
            if "indices" in p:
                p["indices"] = acc_map[p["indices"]]
    for s in j.get("skins", []):
        if "inverseBindMatrices" in s:
            s["inverseBindMatrices"] = acc_map[s["inverseBindMatrices"]]
    for a in j.get("animations", []):
        for smp in a["samplers"]:
            smp["input"], smp["output"] = acc_map[smp["input"]], acc_map[smp["output"]]
    tex_map: dict[int, int] = {}
    new_tex: list[dict] = []
    for m in j["materials"]:
        t = m.get("pbrMetallicRoughness", {}).get("baseColorTexture")
        if t is None:
            continue
        old = t["index"]
        if old not in tex_map:
            tx = dict(j["textures"][old])
            tx["source"] = img_map[tx["source"]]
            tex_map[old] = len(new_tex)
            new_tex.append(tx)
        t["index"] = tex_map[old]
    j["accessors"], j["bufferViews"], j["images"], j["textures"] = new_accs, new_views, new_imgs, new_tex
    j["buffers"] = [{"byteLength": len(out)}]
    write_glb(dst, j, bytes(out))


def main() -> None:
    src = Path(sys.argv[1])
    dst = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(__file__).resolve().parents[2] / "web/assets/avatars"
    dst.mkdir(parents=True, exist_ok=True)
    for f in sorted(src.iterdir()):
        name = f.stem.lower().replace(" ", "_")
        if f.suffix == ".vrm":
            prep_vrm(f, dst / f"{name}.glb")
        elif f.suffix == ".glb":
            shutil.copyfile(f, dst / f"{name}.glb")
        else:
            continue
        print(f"{f.name} -> {name}.glb {(dst / f'{name}.glb').stat().st_size / 1e6:.2f} MB")


if __name__ == "__main__":
    main()
