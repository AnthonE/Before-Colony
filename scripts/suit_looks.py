"""Material looks for `suit-render.py`: the same suit drawn as flat paint (the default), as the
anime's cel shading with ink lines, as a plastic model kit, as realistic painted armour, and as
armour that has seen a war. Everything is procedural: tileable textures generated here with numpy
(panel plates and their seams, grime, paint chips, scratches, streaks), decals drawn with the
game's own font and ray-cast onto the armour, and three lighting set-ups (paper, studio, space).

A look reads the per-vertex data `bc_model` bakes (the occlusion, and the bevel flag that marks a
worn edge), so wear lands where the game's hull shader puts it.
"""

import math
import os

import bpy
import numpy as np
from mathutils import Matrix, Vector
from PIL import Image, ImageDraw, ImageFont

LOOKS = ["flat", "cel", "kit", "real", "worn"]
# Each look's lighting, unless `--env` says otherwise.
ENV_OF = {"flat": "studio", "cel": "paper", "kit": "kit", "real": "space", "worn": "space"}
ENVS = ["studio", "paper", "kit", "space"]

# Tile sizes in metres: one texture repeat across the armour.
PANEL_TILE = 4.4
GRIME_TILE = 9.0
CHIP_TILE = 2.6
SCRATCH_TILE = 5.5
STREAK_TILE = 6.0


# --- Textures. ---


def _norm(a):
    a = a - a.min()
    return a / max(a.max(), 1e-9)


def fbm(n, seed, beta, lo=1):
    """Tileable fractal noise by spectral synthesis: random phases under a 1/f^beta falloff (and
    nothing below wavenumber `lo`), so it wraps seamlessly."""
    rng = np.random.default_rng(seed)
    k = np.fft.fftfreq(n) * n
    kk = np.sqrt(k[:, None] ** 2 + k[None, :] ** 2)
    amp = np.where(kk >= lo, np.maximum(kk, 1.0) ** (-beta / 2.0), 0.0)
    spec = amp * np.exp(2j * np.pi * rng.random((n, n)))
    return _norm(np.real(np.fft.ifft2(spec)))


def panels(n, seed, plate=1.1, tile=PANEL_TILE, seam=0.03):
    """Plates in rows, like the hull shader's: rows a plate high, plates a plate or half a plate
    wide, each row shifted; 1 on a plate, 0 in a seam, a few rivets along the seams. Tileable."""
    rng = np.random.default_rng(seed)
    img = Image.new("L", (n, n), 255)
    d = ImageDraw.Draw(img)
    px = n / tile
    w = max(2, int(seam * px))
    rows = int(round(tile / plate))
    for r in range(rows):
        y = int(r * n / rows)
        for dx in (-n, 0, n):
            d.rectangle([0 + dx, y - w // 2, n + dx, y + w // 2], fill=0)
        x = rng.uniform(0, n)
        while True:
            width = (plate if rng.random() < 0.65 else plate * 0.5) * px
            for ox in (-n, 0, n):
                for oy in (-n, 0, n):
                    xx = int(x) % n + ox
                    d.rectangle([xx - w // 2, y + oy, xx + w // 2, y + int(n / rows) + oy], fill=0)
                    # Rivets either side of the seam, now and then.
                    if rng.random() < 0.35:
                        for k in range(4):
                            ry = y + oy + int((k + 0.5) * n / rows / 4)
                            for side in (-1, 1):
                                cx = xx + side * 3 * w
                                d.ellipse([cx - w, ry - w, cx + w, ry + w], fill=150)
            x += width
            if x >= rng.uniform(0, n) + n - 1 or x > 3 * n:
                break
    return np.asarray(img, dtype=np.float32) / 255.0


def scratches(n, seed, count=900):
    """Thin scratches in loose clusters, mostly running one way. 1 in a scratch. Tileable."""
    rng = np.random.default_rng(seed)
    img = Image.new("L", (n, n), 0)
    d = ImageDraw.Draw(img)
    centres = rng.random((24, 2)) * n
    for _ in range(count):
        cx, cy = centres[rng.integers(len(centres))] + rng.normal(0, n * 0.06, 2)
        ang = rng.normal(0.4, 0.5)
        ln = rng.uniform(0.01, 0.07) * n
        x2, y2 = cx + math.cos(ang) * ln, cy + math.sin(ang) * ln
        v = int(rng.uniform(120, 255))
        for ox in (-n, 0, n):
            for oy in (-n, 0, n):
                d.line([cx + ox, cy + oy, x2 + ox, y2 + oy], fill=v, width=1)
    return np.asarray(img, dtype=np.float32) / 255.0


def streaks(n, seed):
    """Grime running down: noise stretched along the image's v (up on a side face)."""
    rng = np.random.default_rng(seed)
    cols = fbm(n, seed, 1.2)[0]  # one row: a 1D profile across
    a = np.tile(cols[None, :], (n, 1))
    fade = fbm(n, seed + 1, 3.0)
    return _norm(a * (0.4 + 0.6 * fade) + rng.normal(0, 0.02, (n, n)))


def textures(dirpath, n=1024):
    """Writes the textures once into `dirpath`, returns their paths."""
    os.makedirs(dirpath, exist_ok=True)
    maps = {
        "panels": lambda: panels(n, 7),
        "grime": lambda: fbm(n, 11, 2.6),
        "chips": lambda: fbm(n, 13, 1.7, lo=3),
        "scratches": lambda: scratches(n, 17),
        "streaks": lambda: streaks(n, 19),
        "speckle": lambda: fbm(n, 23, 0.8, lo=8),
    }
    out = {}
    for name, make in maps.items():
        path = os.path.join(dirpath, f"{name}.png")
        if not os.path.exists(path):
            Image.fromarray((np.clip(make(), 0, 1) * 255).astype(np.uint8)).save(path)
        out[name] = path
    return out


# --- Decals. ---


def _font(repo, size, bold=True):
    name = "ChakraPetch-SemiBold.ttf" if bold else "ChakraPetch-Regular.ttf"
    try:
        return ImageFont.truetype(os.path.join(repo, "web/fonts", name), size)
    except OSError:
        return ImageFont.load_default()


def decal_images(repo, dirpath):
    """The stencils: a unit number, the Oz badge, the type's name, hazard stripes, the rescue
    mark and a propellant warning. RGBA, white (or coloured) on clear."""
    os.makedirs(dirpath, exist_ok=True)
    out = {}

    def save(name, img):
        p = os.path.join(dirpath, f"decal-{name}.png")
        img.save(p)
        out[name] = p

    img = Image.new("RGBA", (512, 512), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.text((256, 270), "07", font=_font(repo, 330), fill=(236, 234, 222, 255), anchor="mm")
    save("number", img)

    img = Image.new("RGBA", (512, 512), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    # A badge: a hexagon's outline, a bar across, and OZ.
    hexagon = [(256 + 230 * math.cos(math.radians(60 * k + 30)), 256 + 230 * math.sin(math.radians(60 * k + 30))) for k in range(6)]
    d.polygon(hexagon, outline=(236, 234, 222, 255), width=22)
    d.text((256, 262), "OZ", font=_font(repo, 210), fill=(236, 234, 222, 255), anchor="mm")
    d.rectangle([90, 382, 422, 398], fill=(236, 234, 222, 255))
    save("badge", img)

    img = Image.new("RGBA", (1024, 256), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.text((512, 100), "OZ-06MS  LEO", font=_font(repo, 120), fill=(236, 234, 222, 255), anchor="mm")
    d.text((512, 205), "SPECIAL FORCES  ORBITAL", font=_font(repo, 56, bold=False), fill=(236, 234, 222, 220), anchor="mm")
    save("type", img)

    img = Image.new("RGBA", (1024, 128), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    for k in range(-2, 22):
        x = k * 64
        d.polygon([(x, 128), (x + 32, 128), (x + 96, 0), (x + 64, 0)], fill=(230, 180, 30, 255))
        d.polygon([(x + 32, 128), (x + 64, 128), (x + 128, 0), (x + 96, 0)], fill=(25, 24, 22, 255))
    save("stripes", img)

    img = Image.new("RGBA", (512, 512), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.polygon([(256, 40), (480, 440), (32, 440)], fill=(232, 120, 30, 255))
    d.text((256, 330), "RESCUE", font=_font(repo, 70), fill=(20, 20, 20, 255), anchor="mm")
    d.polygon([(256, 140), (310, 230), (202, 230)], fill=(20, 20, 20, 255))
    save("rescue", img)

    img = Image.new("RGBA", (1024, 256), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rectangle([8, 8, 1016, 248], outline=(230, 180, 30, 255), width=10)
    d.text((512, 96), "WARNING  PROPELLANT", font=_font(repo, 92), fill=(230, 180, 30, 255), anchor="mm")
    d.text((512, 190), "N2O4 / UDMH   DO NOT PUNCTURE", font=_font(repo, 54, bold=False), fill=(230, 180, 30, 255), anchor="mm")
    save("warning", img)
    return out


# Where each decal goes on the Leo, in the suit's frame: (image, a point near the surface, the way
# its face looks, its width and height, m). Each is ray-cast onto the armour.
LEO_DECALS = [
    ("number", (4.4, 5.6, 0.0), (1.0, -0.22, 0.0), 1.15, 1.15),
    ("badge", (-4.4, 5.6, 0.0), (-1.0, -0.22, 0.0), 1.3, 1.3),
    ("badge", (1.0, 4.85, 1.6), (0.24, 0.33, 0.91), 0.7, 0.7),
    ("type", (-1.05, 4.75, 1.6), (-0.24, 0.33, 0.91), 1.15, 0.29),
    ("stripes", (0.0, 3.2, 1.5), (0.0, 0.0, 1.0), 1.7, 0.2),
    ("rescue", (-0.85, 3.95, 1.75), (-0.2, 0.42, 0.88), 0.42, 0.42),
    ("warning", (-1.7, 3.25, -3.9), (0.0, 0.0, -1.0), 1.7, 0.42),
]


def to_blender(p):
    return Vector((p[0], -p[2], p[1]))


def place_decals(repo, dirpath, specs=LEO_DECALS):
    """Empties standing on the armour where each decal goes, oriented to the surface: their local
    x and y span the decal, z its depth."""
    images = decal_images(repo, dirpath)
    scene = bpy.context.scene
    depsgraph = bpy.context.evaluated_depsgraph_get()
    placed = []
    for name, at, look, w, h in specs:
        n = to_blender(look).normalized()
        origin = to_blender(at) + n * 3.0
        hit, loc, normal, _, on, _ = scene.ray_cast(depsgraph, origin, -n)
        if not hit:
            continue
        z = normal.normalized()
        up = Vector((0, 0, 1))
        x = up.cross(z)
        if x.length < 1e-3:
            x = Vector((1, 0, 0))
        x.normalize()
        y = z.cross(x)
        empty = bpy.data.objects.new(f"decal-{name}", None)
        rot = Matrix((x, y, z)).transposed().to_4x4()
        scene.collection.objects.link(empty)
        # It rides the bone it landed on, so a turned bone carries it.
        empty.parent = on
        empty.matrix_world = Matrix.Translation(loc) @ rot @ Matrix.Diagonal((w, h, 0.25, 1.0))
        placed.append((empty, images[name]))
    return placed


# --- Node helpers. ---


class Nodes:
    def __init__(self, mat):
        self.t = mat.node_tree
        self.n = self.t.nodes
        self.l = self.t.links

    def new(self, kind, **props):
        node = self.n.new(kind)
        for k, v in props.items():
            setattr(node, k, v)
        return node

    def link(self, a, b):
        self.l.new(a, b)

    def math(self, op, a, b=None, clamp=False):
        m = self.new("ShaderNodeMath", operation=op, use_clamp=clamp)
        for i, v in enumerate([a, b]):
            if v is None:
                continue
            if isinstance(v, (int, float)):
                m.inputs[i].default_value = v
            else:
                self.link(v, m.inputs[i])
        return m.outputs[0]

    def mix(self, a, b, fac, blend="MIX"):
        """Colour mix of `a` and `b` (sockets or RGB tuples) by `fac`."""
        m = self.new("ShaderNodeMix", data_type="RGBA", blend_type=blend)
        for sock, v in [(m.inputs["Factor"], fac), (m.inputs[6], a), (m.inputs[7], b)]:
            if isinstance(v, (int, float)):
                sock.default_value = v
            elif isinstance(v, tuple):
                sock.default_value = (*v, 1.0) if len(v) == 3 else v
            else:
                self.link(v, sock)
        return m.outputs[2]

    def lerp(self, a, b, fac):
        """Float mix of `a` and `b` (sockets or numbers) by `fac`."""
        m = self.new("ShaderNodeMix", data_type="FLOAT")
        for sock, v in [(m.inputs["Factor"], fac), (m.inputs[2], a), (m.inputs[3], b)]:
            if isinstance(v, (int, float)):
                sock.default_value = v
            else:
                self.link(v, sock)
        return m.outputs[0]

    def ramp(self, value, lo, hi):
        """0 below `lo`, 1 above `hi`, smooth between."""
        m = self.new("ShaderNodeMapRange", interpolation_type="SMOOTHSTEP")
        m.inputs["From Min"].default_value = lo
        m.inputs["From Max"].default_value = hi
        self.link(value, m.inputs["Value"])
        return m.outputs["Result"]

    def attr(self, name, out="Fac"):
        a = self.new("ShaderNodeAttribute", attribute_name=name)
        return a.outputs[out]

    def tex(self, path, tile, coords, box=True, extension="REPEAT"):
        """An image across the armour at `tile` metres a repeat, projected along the dominant axis
        (Blender's box mapping: as the hull shader projects its plates)."""
        mapping = self.new("ShaderNodeMapping")
        mapping.inputs["Scale"].default_value = (1 / tile, 1 / tile, 1 / tile)
        self.link(coords, mapping.inputs["Vector"])
        img = bpy.data.images.load(path, check_existing=True)
        img.colorspace_settings.name = "Non-Color"
        t = self.new("ShaderNodeTexImage", image=img, extension=extension)
        if box:
            t.projection = "BOX"
            t.projection_blend = 0.25
        self.link(mapping.outputs["Vector"], t.inputs["Vector"])
        return t.outputs["Color"]


def lin(c):
    return tuple(x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c)


# --- Looks. ---


def paint_material(name, rgb, rough, look, tex, decals, painted=True, metal=False):
    """A painted (or bare metal) armour material in `look`."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    g = Nodes(mat)
    bsdf = g.n["Principled BSDF"]
    out = g.n["Material Output"]
    base = lin(rgb)
    ao = g.attr("ao")
    bevel = g.attr("bevel")
    coords = g.new("ShaderNodeTexCoord").outputs["Object"]

    if look == "cel":
        # Two tones and a deep shadow, lit from one fixed direction, flat as the line art's; the
        # ink comes from Freestyle.
        geo = g.new("ShaderNodeNewGeometry")
        light = g.new("ShaderNodeCombineXYZ")
        for k, v in zip("XYZ", Vector((0.55, -0.45, 0.7)).normalized()):
            light.inputs[k].default_value = v
        dot = g.new("ShaderNodeVectorMath", operation="DOT_PRODUCT")
        g.link(geo.outputs["Normal"], dot.inputs[0])
        g.link(light.outputs["Vector"], dot.inputs[1])
        lit = g.math("GREATER_THAN", dot.outputs["Value"], 0.05)
        crease = g.math("GREATER_THAN", ao, 0.5)
        tone = g.math("MULTIPLY", lit, crease)
        shade = tuple(c * 0.55 for c in base)
        colour = g.mix(shade, base, tone)
        hi = g.math("GREATER_THAN", dot.outputs["Value"], 0.88)
        colour = g.mix(colour, tuple(min(1.0, c * 1.25 + 0.03) for c in base), g.math("MULTIPLY", hi, 0.6))
        emit = g.new("ShaderNodeEmission")
        g.link(colour, emit.inputs["Color"])
        g.link(emit.outputs[0], out.inputs["Surface"])
        g.n.remove(bsdf)
        return mat

    real = look in ("real", "worn")
    worn = look == "worn"
    # Bare metal has no plates.
    plates = g.tex(tex["panels"], PANEL_TILE, coords) if painted else g.math("ADD", 1.0, 0.0)
    grime = g.tex(tex["grime"], GRIME_TILE, coords)
    chips = g.tex(tex["chips"], CHIP_TILE, coords)
    scratch = g.tex(tex["scratches"], SCRATCH_TILE, coords)
    streak = g.tex(tex["streaks"], STREAK_TILE, coords)
    speckle = g.tex(tex["speckle"], 1.3, coords)
    seam = g.math("SUBTRACT", 1.0, plates)

    # The paint, a shade uneven from plate to plate and speckled.
    colour = g.mix(base, tuple(c * 0.86 for c in base), g.math("MULTIPLY", speckle, 0.5 if real else 0.25))
    # Panel lines: drawn in dark on a kit, sunken seams on armour.
    colour = g.mix(colour, tuple(c * (0.22 if look == "kit" else 0.45) for c in base), seam)
    rough_s = g.math("ADD", rough if painted else 0.32, g.math("MULTIPLY", g.math("SUBTRACT", speckle, 0.5), 0.12))
    if look == "kit":
        rough_s = g.math("MULTIPLY", rough_s, 0.8)
    metallic = 1.0 if metal else 0.0
    height = g.math("MULTIPLY", plates, 1.0)
    # The baked occlusion darkens the crevices.
    colour = g.mix(colour, g.mix((0, 0, 0), colour, ao), 0.85 if real else 0.6)

    if worn and painted:
        # Paint faded by years of hard sunlight: paler and greyer.
        grey = g.new("ShaderNodeRGBToBW")
        g.link(colour, grey.inputs[0])
        colour = g.mix(colour, g.mix(grey.outputs[0], (0.5, 0.5, 0.5), 0.15), 0.3)
    if real and painted:
        # Grime in the crevices and in broad patches, and dirt running down the sides.
        dirt = (0.055, 0.05, 0.035)
        crevice = g.math("MULTIPLY", g.math("SUBTRACT", 1.0, ao), 1.4, clamp=True)
        patch = g.ramp(grime, 0.45, 0.85)
        amount = g.math("ADD", g.math("MULTIPLY", crevice, 0.8), g.math("MULTIPLY", patch, 0.35 if worn else 0.18), clamp=True)
        colour = g.mix(colour, dirt, amount)
        run = g.math("MULTIPLY", g.ramp(streak, 0.55 if worn else 0.7, 1.0), 0.5 if worn else 0.25)
        colour = g.mix(colour, dirt, run)
        rough_s = g.math("ADD", rough_s, g.math("MULTIPLY", amount, 0.25), clamp=True)

        # Paint worn off: on the bevelled edges first (the bake marks them), and, on a veteran,
        # flaking off the faces too; red primer under the paint, bare metal under that.
        edge = g.math("MULTIPLY", bevel, g.ramp(chips, 0.55 if worn else 0.74, 0.72 if worn else 0.84))
        flake = g.math("MULTIPLY", g.ramp(chips, 0.76, 0.8), 1.0 if worn else 0.0)
        wear = g.math("MAXIMUM", edge, flake)
        primer = g.math("MULTIPLY", g.ramp(chips, 0.6, 0.66), wear)
        metal_bit = g.math("MULTIPLY", g.ramp(chips, 0.78 if worn else 0.8, 0.82 if worn else 0.84), wear)
        bare = (0.42, 0.42, 0.4)
        colour = g.mix(colour, lin((0.42, 0.16, 0.1)), g.math("MULTIPLY", primer, 1.0 if worn else 0.0))
        colour = g.mix(colour, bare, metal_bit)
        metallic = g.math("MAXIMUM", metallic, metal_bit)
        rough_s = g.lerp(rough_s, 0.3, metal_bit)
        height = g.math("SUBTRACT", height, g.math("MULTIPLY", wear, 0.4))
        # Scratches: bright where they cut to metal.
        cut = g.math("MULTIPLY", g.ramp(scratch, 0.45, 0.8), 0.75 if worn else 0.3)
        colour = g.mix(colour, (0.55, 0.55, 0.52), cut)
        metallic = g.math("MAXIMUM", metallic, g.math("MULTIPLY", cut, 0.8))
        height = g.math("SUBTRACT", height, g.math("MULTIPLY", cut, 0.3))
        if worn:
            # Scorching where shots have grazed it, and dust kicked up over the feet.
            burn = g.math("MULTIPLY", g.ramp(g.tex(tex["grime"], 4.3, coords), 0.66, 0.86), 0.95)
            colour = g.mix(colour, (0.012, 0.011, 0.01), burn)
            rough_s = g.lerp(rough_s, 0.85, burn)
            # How far down the suit (the world's up is the suit's), for the dust on the feet.
            sep = g.new("ShaderNodeSeparateXYZ")
            g.link(g.new("ShaderNodeNewGeometry").outputs["Position"], sep.inputs[0])
            dust = g.math("MULTIPLY", g.ramp(g.math("MULTIPLY", sep.outputs["Z"], -1.0), 7.2, 9.0), 0.45)
            dust = g.math("MULTIPLY", dust, g.ramp(grime, 0.2, 0.6))
            colour = g.mix(colour, lin((0.52, 0.47, 0.38)), dust)

    # Decals, on paint only; a veteran's are worn with the paint.
    if painted and decals:
        for empty, path in decals:
            tc = g.new("ShaderNodeTexCoord", object=empty)
            m = g.new("ShaderNodeMapping")
            m.inputs["Location"].default_value = (0.5, 0.5, 0.0)
            g.link(tc.outputs["Object"], m.inputs["Vector"])
            img = bpy.data.images.load(path, check_existing=True)
            t = g.new("ShaderNodeTexImage", image=img, extension="CLIP", interpolation="Cubic")
            g.link(m.outputs["Vector"], t.inputs["Vector"])
            sep = g.new("ShaderNodeSeparateXYZ")
            g.link(tc.outputs["Object"], sep.inputs[0])
            depth = g.math("LESS_THAN", g.math("ABSOLUTE", sep.outputs["Z"]), 1.0)
            alpha = g.math("MULTIPLY", t.outputs["Alpha"], depth)
            if real:
                keep = g.math("SUBTRACT", 1.0, g.math("MULTIPLY", g.ramp(chips, 0.55, 0.75), 0.9 if worn else 0.3))
                alpha = g.math("MULTIPLY", g.math("MULTIPLY", alpha, keep), 0.92)
            colour = g.mix(colour, t.outputs["Color"], alpha)

    g.link(colour, bsdf.inputs["Base Color"])
    for sock, v in [(bsdf.inputs["Roughness"], rough_s), (bsdf.inputs["Metallic"], metallic)]:
        if isinstance(v, (int, float)):
            sock.default_value = v
        else:
            g.link(v, sock)
    if look == "kit":
        bsdf.inputs["Coat Weight"].default_value = 0.15
        bsdf.inputs["Coat Roughness"].default_value = 0.35
    bump = g.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.35 if real else 0.2
    bump.inputs["Distance"].default_value = 0.02
    g.link(height, bump.inputs["Height"])
    g.link(bump.outputs["Normal"], bsdf.inputs["Normal"])
    return mat


def glass_material(name, rgb, look, glow=None):
    """Visors, lenses and the mono-eye: glossy glass, or, glowing, an emitter."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    g = Nodes(mat)
    bsdf = g.n["Principled BSDF"]
    if look == "cel":
        emit = g.new("ShaderNodeEmission")
        emit.inputs["Color"].default_value = (*lin(rgb), 1.0) if glow is None else (*glow, 1.0)
        emit.inputs["Strength"].default_value = 1.0 if glow is None else 1.6
        g.link(emit.outputs[0], g.n["Material Output"].inputs["Surface"])
        return mat
    bsdf.inputs["Base Color"].default_value = (*lin(rgb), 1.0)
    bsdf.inputs["Roughness"].default_value = 0.06
    bsdf.inputs["Coat Weight"].default_value = 1.0
    if glow is not None:
        bsdf.inputs["Emission Color"].default_value = (*glow, 1.0)
        bsdf.inputs["Emission Strength"].default_value = 8.0
    return mat


def materials(look, livery, palette, eye, tex, decals):
    """A function from a vertex's paint code to its material, in `look` (see `suit-render.py`'s
    flat materials for the codes)."""
    body, trim, accent = livery
    cache = {}

    def get(code):
        if code in cache:
            return cache[code]
        if code < 3:
            r, g, b, rough = palette[[body, trim, accent][code]]
            m = paint_material(f"{look}-paint{code}", (r, g, b), rough, look, tex, decals)
        elif code == 3:
            m = glass_material(f"{look}-eye", (0.05, 0.02, 0.04), look, glow=tuple(e / 6.0 for e in eye))
        elif code < 32 or code >= 64:
            k = code - 16 if code < 32 else 16 + code - 64
            r, g, b, rough = palette[k]
            # Glass and lenses (the palette's GLASS, and the second bank's visor and lens).
            if k in (15, 18, 19):
                m = glass_material(f"{look}-glass{k}", (r, g, b), look)
            else:
                m = paint_material(f"{look}-fixed{k}", (r, g, b), rough, look, tex, decals if k == 16 else [])
        elif code < 48:
            r, g, b, rough = palette[code - 32]
            m = paint_material(f"{look}-metal{code}", (r, g, b), 0.3, look, tex, [], painted=False, metal=True)
        else:
            r, g, b, _ = palette[code - 48]
            m = glass_material(f"{look}-glow{code}", (r, g, b), look, glow=(r * 4.0, g * 4.0, b * 4.0))
        cache[code] = m
        return m

    return get


# --- Environments. ---


def environment(env, scene):
    """Lights, background and colour management for `env`."""
    world = bpy.data.worlds.new(env)
    world.use_nodes = True
    g = Nodes(world)
    bg = g.n["Background"]
    # Suns: (kind, rotation, strength, angular size in degrees, colour).
    lights = []
    if env == "paper":
        bg.inputs["Color"].default_value = (1.0, 1.0, 1.0, 1.0)
        bg.inputs["Strength"].default_value = 1.0
        scene.view_settings.view_transform = "Standard"
        scene.render.use_freestyle = True
        scene.render.line_thickness_mode = "RELATIVE"
        fs = bpy.context.view_layer.freestyle_settings
        # Ink on the outline, where paints meet, and on corners sharper than a chamfer's.
        fs.crease_angle = math.radians(118)
        ink = bpy.data.linestyles.new("ink")
        ink.color = (0.04, 0.03, 0.025)
        ink.thickness = 1.5
        for ls in list(fs.linesets):
            ls.linestyle = ink
        if not fs.linesets:
            fs.linesets.new("ink").linestyle = ink
        for ls in fs.linesets:
            ls.select_silhouette = ls.select_border = ls.select_crease = True
            ls.select_material_boundary = True
    elif env == "kit":
        # A dark blue-grey sweep and big soft lights, as a figure's product photo.
        tc = g.new("ShaderNodeTexCoord")
        sep = g.new("ShaderNodeSeparateXYZ")
        g.link(tc.outputs["Window"], sep.inputs[0])
        col = g.mix(lin((0.12, 0.14, 0.19)), lin((0.36, 0.39, 0.45)), sep.outputs["Y"])
        g.link(col, bg.inputs["Color"])
        bg.inputs["Strength"].default_value = 1.0
        scene.view_settings.view_transform = "AgX"
        lights = [
            ("SUN", (math.radians(52), 0, math.radians(35)), 3.0, 18.0, (1.0, 0.97, 0.93)),
            ("SUN", (math.radians(70), 0, math.radians(-70)), 1.1, 30.0, (0.9, 0.95, 1.0)),
            ("SUN", (math.radians(62), 0, math.radians(175)), 2.2, 12.0, (1.0, 1.0, 1.0)),
        ]
    elif env == "space":
        # Black, a scatter of stars, the Sun hard and warm from high to one side, the Earth's
        # blue light from below the other.
        tc = g.new("ShaderNodeTexCoord")
        vor = g.new("ShaderNodeTexVoronoi")
        vor.inputs["Scale"].default_value = 260.0
        g.link(tc.outputs["Generated"], vor.inputs["Vector"])
        star = g.ramp(g.math("SUBTRACT", 1.0, vor.outputs["Distance"]), 0.965, 1.0)
        col = g.mix((0.0015, 0.0018, 0.003), (1.0, 1.0, 1.0), g.math("MULTIPLY", star, 0.7))
        g.link(col, bg.inputs["Color"])
        bg.inputs["Strength"].default_value = 1.0
        scene.view_settings.view_transform = "AgX"
        scene.view_settings.look = "AgX - Medium High Contrast"
        lights = [
            ("SUN", (math.radians(48), 0, math.radians(38)), 6.5, 0.6, (1.0, 0.96, 0.9)),
            ("SUN", (math.radians(150), 0, math.radians(-50)), 1.8, 25.0, (0.45, 0.65, 1.0)),
            ("SUN", (math.radians(75), 0, math.radians(185)), 2.4, 2.0, (0.85, 0.9, 1.0)),
        ]
    else:  # studio: the plain grey of the design views
        bg.inputs["Color"].default_value = (0.32, 0.34, 0.38, 1.0)
        bg.inputs["Strength"].default_value = 0.8
        scene.view_settings.view_transform = "Standard"
        lights = [
            ("SUN", (math.radians(50), 0, math.radians(30)), 3.2, 3.0, (1, 1, 1)),
            ("SUN", (math.radians(70), 0, math.radians(-60)), 1.0, 3.0, (1, 1, 1)),
            ("SUN", (math.radians(60), 0, math.radians(170)), 1.8, 3.0, (1, 1, 1)),
        ]
    scene.world = world
    for k, (kind, rot, energy, size, colour) in enumerate(lights):
        light = bpy.data.lights.new(f"{env}{k}", kind)
        light.energy = energy
        light.color = colour
        light.angle = math.radians(size)
        o = bpy.data.objects.new(f"{env}{k}", light)
        o.rotation_euler = rot
        scene.collection.objects.link(o)
