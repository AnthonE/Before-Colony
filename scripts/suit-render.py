"""Renders a suit model (from `cargo run -p bc-model --example suit_json`) in Blender, for looking
at a design from the views its reference art is drawn in.

    pip install bpy==4.2.0   # Blender as a Python module (Python 3.11)
    cargo run -q -p bc-model --example suit_json -- leo > leo.json
    python3 scripts/suit-render.py leo.json out/leo [front back side three rear34 head ...]

Each view is written to `<out>-<view>.png`; `sheet` (the default) lays front, side, back and the
three-quarter views out on one image. `--livery oz|colonies|alliance|white` picks the paint,
`--size N` the height in pixels, `--blend` also saves the scene (`<out>.blend`) to open in Blender,
and `--glb` exports the suit as glTF (`<out>.glb`). `--turn Head:0,40,0` turns a bone (and what
hangs off it) about its joint by x, y and z angles in degrees, in the suit's frame, to see a pose:
a head turned to aim, an arm raised; give it once per bone.

`--look flat|cel|kit|real|worn` draws it as flat paint (the default, for the design views), as the
anime's cel shading with ink lines, as a plastic model kit, as realistic painted armour with its
decals, or as armour that has seen a war (`suit_looks.py`); each has its lighting (`--env
studio|paper|kit|space` picks another). `hero` is a low three-quarter view for comparing them.

The suit's frame (x right, y up, z forward) becomes Blender's (x, -z, y): its front faces -Y, as
Blender's front view sees it.
"""

import argparse
import json
import math
import os
import sys

import bpy
from mathutils import Vector

import suit_looks

# The client's palette (crates/bc-client/src/materials.rs): sRGB and perceptual roughness.
PALETTE = [
    (0.9, 0.91, 0.93, 0.38),
    (0.1, 0.22, 0.66, 0.4),
    (0.72, 0.08, 0.08, 0.4),
    (0.95, 0.75, 0.1, 0.35),
    (0.11, 0.12, 0.13, 0.5),
    (0.4, 0.5, 0.32, 0.5),
    (0.44, 0.47, 0.5, 0.45),
    (0.83, 0.85, 0.88, 0.38),
    (0.2, 0.32, 0.6, 0.42),
    (0.38, 0.41, 0.27, 0.55),
    (0.6, 0.5, 0.34, 0.55),
    (0.62, 0.63, 0.64, 0.5),
    (0.3, 0.31, 0.33, 0.6),
    (0.66, 0.71, 0.8, 0.2),
    (0.26, 0.28, 0.31, 0.32),
    (0.02, 0.025, 0.03, 0.08),
]
# Body, trim, accent and eye glow (crates/bc-client/src/suits_vis.rs, hull.wgsl's EYES).
LIVERIES = {
    "oz": (5, 6, 4, (6.0, 0.4, 2.2)),
    "colonies": (8, 0, 2, (0.2, 6.0, 1.2)),
    "alliance": (10, 4, 2, (6.0, 0.4, 2.2)),
    "white": (0, 1, 2, (0.2, 6.0, 1.2)),
}


def load_palette(repo):
    """The palette as the client has it, read from materials.rs when it's there (its second bank
    grows with the designs)."""
    path = os.path.join(repo, "crates/bc-client/src/materials.rs")
    try:
        text = open(path).read()
    except OSError:
        return PALETTE
    start = text.index("const PALETTE")
    body = text[text.index("[", text.index("=", start)) + 1 : text.index("];", start)]
    out = []
    for line in body.splitlines():
        line = line.strip().rstrip(",")
        if line.startswith("("):
            out.append(tuple(float(x) for x in line.strip("()").split(",")))
    return out or PALETTE


def srgb_to_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def to_blender(p):
    return (p[0], -p[2], p[1])


def material(name, rgb, rough, metal=0.0, emit=None):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*[srgb_to_linear(c) for c in rgb], 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    if emit is not None:
        bsdf.inputs["Emission Color"].default_value = (*emit, 1.0)
        bsdf.inputs["Emission Strength"].default_value = 1.0
    # The baked occlusion (the vertex colour's alpha) darkens the paint a little, as the game's does.
    nodes, links = m.node_tree.nodes, m.node_tree.links
    attr = nodes.new("ShaderNodeAttribute")
    attr.attribute_name = "ao"
    mix = nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    mix.inputs["Factor"].default_value = 0.6
    mix.inputs["A"].default_value = bsdf.inputs["Base Color"].default_value
    links.new(attr.outputs["Color"], mix.inputs["B"])
    links.new(mix.outputs["Result"], bsdf.inputs["Base Color"])
    return m


def materials_for(livery, palette):
    body, trim, accent, eye = LIVERIES[livery]
    cache = {}

    def get(code):
        if code in cache:
            return cache[code]
        if code < 16:
            slot = [body, trim, accent][code] if code < 3 else None
            if slot is None:
                m = material(f"eye{code}", (0.0, 0.0, 0.0), 0.2, emit=tuple(e / 6.0 for e in eye))
                m.node_tree.nodes["Principled BSDF"].inputs["Emission Strength"].default_value = 6.0
            else:
                r, g, b, rough = palette[slot]
                m = material(f"paint{code}", (r, g, b), rough)
        elif code < 32:
            r, g, b, rough = palette[code - 16]
            m = material(f"fixed{code - 16}", (r, g, b), rough)
        elif code < 48:
            r, g, b, rough = palette[code - 32]
            m = material(f"metal{code - 32}", (r, g, b), max(0.2, rough * 0.6), metal=1.0)
        elif code < 64:
            r, g, b, _ = palette[code - 48]
            m = material(f"glow{code - 48}", (r, g, b), 0.3, emit=(r * 4.0, g * 4.0, b * 4.0))
        else:
            r, g, b, rough = palette[16 + (code - 64)]
            m = material(f"fixed{16 + code - 64}", (r, g, b), rough)
        cache[code] = m
        return m

    return get


def build(model, get_material):
    """Each bone an object at its joint (its mesh in its own space), parented as the rig is, so
    turning one carries what hangs off it."""
    root = bpy.data.objects.new(model["frame"], None)
    bpy.context.scene.collection.objects.link(root)
    objects = {}
    for bone in model["bones"]:
        j = bone["joint"]
        parent = bone["parent"]
        pj = next((b["joint"] for b in model["bones"] if b["name"] == parent), (0.0, 0.0, 0.0))
        pos = bone["positions"]
        if not pos:
            obj = bpy.data.objects.new(bone["name"], None)
            obj.parent = objects.get(parent, root)
            obj.location = to_blender([j[k] - pj[k] for k in range(3)])
            bpy.context.scene.collection.objects.link(obj)
            objects[bone["name"]] = obj
            continue
        verts = [to_blender((pos[i], pos[i + 1], pos[i + 2])) for i in range(0, len(pos), 3)]
        idx = bone["indices"]
        faces = [tuple(idx[i : i + 3]) for i in range(0, len(idx), 3)]
        mesh = bpy.data.meshes.new(bone["name"])
        mesh.from_pydata(verts, [], faces)
        cols = bone["colors"]
        codes = [round(cols[k * 4] * 255) for k in range(len(verts))]
        mats = sorted({codes[f[0]] for f in faces})
        for c in mats:
            mesh.materials.append(get_material(c))
        slot = {c: i for i, c in enumerate(mats)}
        for poly, f in zip(mesh.polygons, faces):
            poly.material_index = slot[codes[f[0]]]
        # The baked occlusion and the bevel flag (a worn edge), for the looks.
        ao = mesh.color_attributes.new("ao", "FLOAT_COLOR", "POINT")
        bevel = mesh.color_attributes.new("bevel", "FLOAT_COLOR", "POINT")
        for k in range(len(verts)):
            a, b = cols[k * 4 + 3], cols[k * 4 + 1]
            ao.data[k].color = (a, a, a, 1.0)
            bevel.data[k].color = (b, b, b, 1.0)
        mesh["codes"] = mats
        # The game's own normals: flat on faces and bevels, smooth over turned shapes' curves.
        nrm = bone["normals"]
        mesh.polygons.foreach_set("use_smooth", [True] * len(mesh.polygons))
        mesh.normals_split_custom_set_from_vertices(
            [to_blender((nrm[i], nrm[i + 1], nrm[i + 2])) for i in range(0, len(nrm), 3)]
        )
        mesh.update()
        obj = bpy.data.objects.new(bone["name"], mesh)
        obj.parent = objects.get(parent, root)
        obj.location = to_blender([j[k] - pj[k] for k in range(3)])
        bpy.context.scene.collection.objects.link(obj)
        objects[bone["name"]] = obj
    bpy.context.view_layer.update()
    return objects


def turn(objects, spec):
    """`Bone:x,y,z`: turns the bone by those angles (degrees, about the suit's axes) at its joint."""
    from mathutils import Euler

    name, angles = spec.split(":")
    x, y, z = (math.radians(float(a)) for a in angles.split(","))
    # The suit's x, y, z are Blender's x, z and -y.
    objects[name].rotation_euler = Euler((x, -z, y), "XYZ")
    bpy.context.view_layer.update()


def bounds():
    lo = Vector((1e9, 1e9, 1e9))
    hi = -lo
    for o in bpy.context.scene.objects:
        if o.type != "MESH":
            continue
        for v in o.data.vertices:
            p = o.matrix_world @ v.co
            lo = Vector(map(min, lo, p))
            hi = Vector(map(max, hi, p))
    return lo, hi


# Views: (azimuth from the front, degrees, positive toward the suit's +x; elevation; ortho; what to
# frame; and a lens, mm, for the perspective ones). `art` and `figure` look as the line art (low and
# close, from the front) and a figure's photo from behind do.
VIEWS = {
    "hero": (30, 3, False, "all", 55),
    "art": (-8, -16, False, "all", 30),
    "figure": (195, 6, False, "all", 60),
    "front": (0, 0, True, "all"),
    "back": (180, 0, True, "all"),
    "side": (90, 0, True, "all"),
    "left": (-90, 0, True, "all"),
    "three": (35, 12, False, "all"),
    "low": (20, -12, False, "all"),
    "rear34": (145, 15, False, "all"),
    "head": (25, 8, False, "head"),
    "headback": (160, 15, False, "head"),
    "headside": (90, 0, True, "head"),
    "headfront": (0, 0, True, "head"),
    "torso": (20, 10, False, "torso"),
    "legs": (30, 5, False, "legs"),
    "feet": (35, 14, False, "feet"),
    "feetside": (90, 0, True, "feet"),
    "feetfront": (0, 4, True, "feet"),
    "feetback": (160, 12, False, "feet"),
}


def frame_box(what, lo, hi):
    if what == "head":
        # The suit's frame: the head round y 5.5..8 (Blender z), x -1.5..1.5.
        return Vector((-1.6, -1.6, 5.2)), Vector((1.6, 1.6, 8.2))
    if what == "torso":
        return Vector((-5.0, -3.5, 0.0)), Vector((5.0, 3.5, 8.2))
    if what == "legs":
        return Vector((-3.5, -2.5, lo.z)), Vector((3.5, 2.5, 3.0))
    if what == "feet":
        return Vector((-3.2, -2.8, lo.z)), Vector((3.2, 2.0, -5.8))
    return lo, hi


def setup_scene(size, aspect, env="studio"):
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 48
    scene.cycles.use_denoising = True
    scene.render.resolution_y = size
    scene.render.resolution_x = int(size * aspect)
    scene.render.film_transparent = False
    scene.view_settings.view_transform = "Standard"
    if env != "studio":
        suit_looks.environment(env, scene)
        return
    world = bpy.data.worlds.new("bg")
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (0.32, 0.34, 0.38, 1.0)
    bg.inputs["Strength"].default_value = 0.8
    scene.world = world
    # A key light high and to the front-right, a fill from the left, a rim from behind.
    for name, rot, energy in [
        ("key", (math.radians(50), 0, math.radians(30)), 3.2),
        ("fill", (math.radians(70), 0, math.radians(-60)), 1.0),
        ("rim", (math.radians(60), 0, math.radians(170)), 1.8),
    ]:
        light = bpy.data.lights.new(name, "SUN")
        light.energy = energy
        light.angle = math.radians(3)
        o = bpy.data.objects.new(name, light)
        o.rotation_euler = rot
        scene.collection.objects.link(o)


def render_view(view, out, size):
    az, el, ortho, what = VIEWS[view][:4]
    lens = VIEWS[view][4] if len(VIEWS[view]) > 4 else 70
    lo, hi = frame_box(what, *bounds())
    centre = (lo + hi) / 2
    ext = hi - lo
    scene = bpy.context.scene
    cam_data = bpy.data.cameras.new(view)
    cam = bpy.data.objects.new(view, cam_data)
    scene.collection.objects.link(cam)
    a, e = math.radians(az), math.radians(el)
    # From the front (-Y), turned toward +x by the azimuth, raised by the elevation.
    d = Vector((math.sin(a) * math.cos(e), -math.cos(a) * math.cos(e), math.sin(e)))
    height = max(ext.z, max(ext.x, ext.y) * 1.0)
    aspect = scene.render.resolution_x / scene.render.resolution_y
    if ortho:
        cam_data.type = "ORTHO"
        cam_data.ortho_scale = max(ext.z * 1.06, max(ext.x, ext.y) * 1.06 / aspect) * max(1.0, aspect)
        dist = 60.0
    else:
        cam_data.lens = lens
        cam_data.sensor_fit = "VERTICAL"
        # Far enough back that the height fits, with a margin.
        dist = height * 0.54 / math.tan(cam_data.angle_y / 2) + max(ext.x, ext.y) * 0.3
    cam_data.clip_end = 500
    cam.location = centre + d * dist
    cam.rotation_euler = (-d).to_track_quat("-Z", "Y").to_euler()
    scene.camera = cam
    scene.render.filepath = f"{out}-{view}.png"
    bpy.ops.render.render(write_still=True)
    return scene.render.filepath


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else sys.argv[1:]
    ap = argparse.ArgumentParser()
    ap.add_argument("model")
    ap.add_argument("out")
    ap.add_argument("views", nargs="*")
    ap.add_argument("--livery", default="oz", choices=sorted(LIVERIES))
    ap.add_argument("--size", type=int, default=900)
    ap.add_argument("--samples", type=int, default=48)
    ap.add_argument("--blend", action="store_true")
    ap.add_argument("--glb", action="store_true")
    ap.add_argument("--turn", action="append", default=[])
    ap.add_argument("--look", default="flat", choices=suit_looks.LOOKS)
    ap.add_argument("--env", choices=suit_looks.ENVS)
    args = ap.parse_args(argv)
    views = args.views or ["sheet"]

    bpy.ops.wm.read_factory_settings(use_empty=True)
    repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    model = json.load(open(args.model))
    palette = load_palette(repo)
    objects = build(model, materials_for(args.livery, palette))
    if args.look != "flat":
        # Decals are ray-cast onto the armour at rest, then the look's materials replace the flat
        # paint slot for slot.
        bpy.context.view_layer.update()
        work = os.path.join(os.path.dirname(os.path.abspath(args.out)), ".suit-looks")
        decals = suit_looks.place_decals(repo, work) if model["frame"] == "Leo" else []
        body, trim, accent, eye = LIVERIES[args.livery]
        get = suit_looks.materials(args.look, (body, trim, accent), palette, eye, suit_looks.textures(work), decals)
        for o in objects.values():
            if o.type == "MESH":
                for i, code in enumerate(o.data["codes"]):
                    o.data.materials[i] = get(code)
    env = args.env or suit_looks.ENV_OF[args.look]
    for spec in args.turn:
        turn(objects, spec)
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    if args.blend:
        bpy.ops.wm.save_as_mainfile(filepath=os.path.abspath(args.out + ".blend"))
    if args.glb:
        bpy.ops.export_scene.gltf(filepath=os.path.abspath(args.out + ".glb"), export_format="GLB")

    sheet = "sheet" in views
    singles = [v for v in views if v != "sheet"]
    if sheet:
        singles += [v for v in ["front", "side", "back", "three", "rear34"] if v not in singles]
    written = {}
    for view in singles:
        aspect = {"head": 1.0, "feet": 1.6}.get(VIEWS[view][3], 0.62)
        setup_scene(args.size, aspect, env)
        bpy.context.scene.cycles.samples = args.samples
        written[view] = render_view(view, args.out, args.size)
        for o in [o for o in bpy.context.scene.objects if o.type in ("CAMERA", "LIGHT")]:
            bpy.data.objects.remove(o)
    if sheet:
        from PIL import Image

        tiles = [Image.open(written[v]) for v in ["front", "side", "back", "three", "rear34"]]
        w = sum(t.width for t in tiles)
        im = Image.new("RGB", (w, max(t.height for t in tiles)))
        x = 0
        for t in tiles:
            im.paste(t, (x, 0))
            x += t.width
        im.save(f"{args.out}-sheet.png")
        print(f"{args.out}-sheet.png")
    for v, p in written.items():
        print(p)


if __name__ == "__main__":
    main()
