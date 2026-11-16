"""Generate the mod's own simple 16x16 pixel textures, and the patch block models and blockstates that use them.

    python tools/gen_textures.py .        (from the repo root; needs Pillow: pip install pillow)

Everything is drawn here from a seeded random source, so running it twice gives byte-identical files.
No texture is copied from the game.

Patch blocks: a rubble ground texture with ore speckles in the resource colour. The number of speckle clusters
grows with the richness level (level_1 sparse ... level_8 dense), so richness is visible.
Drill: steel frame, plate, housing, vent front (unlit and lit), top, copper metal and dark parts.
"""
import hashlib
import json
import os
import random
import sys

from PIL import Image

root = sys.argv[1] if len(sys.argv) > 1 else "."
NS = "horizon_factory"
TEX_DIR = os.path.join(root, "assets", NS, "textures", "block")
LEVELS = [f"level_{i}" for i in range(1, 9)]


def rng(*parts):
    seed = int(hashlib.sha256("|".join(map(str, parts)).encode()).hexdigest()[:12], 16)
    return random.Random(seed)


def save(img, name):
    os.makedirs(TEX_DIR, exist_ok=True)
    img.save(os.path.join(TEX_DIR, name + ".png"), optimize=False, compress_level=9)


def shade(c, d):
    return tuple(max(0, min(255, v + d)) for v in c)


def new(base):
    return Image.new("RGB", (16, 16), base)


def noise(img, r, amount):
    px = img.load()
    for y in range(16):
        for x in range(16):
            px[x, y] = shade(px[x, y], r.randint(-amount, amount))


# --------------------------------------------------------------------------- patch blocks
GROUND = (104, 98, 90)  # grey-brown rubble
RESOURCES = {
    "coal":   [(28, 28, 32), (52, 52, 58), (92, 92, 104)],
    "iron":   [(214, 172, 142), (186, 138, 108), (148, 98, 78)],
    "copper": [(226, 118, 78), (194, 96, 60), (74, 172, 150)],
}


def ground(r):
    img = new(GROUND)
    noise(img, r, 12)
    px = img.load()
    for _ in range(26):  # lighter and darker pebbles
        x, y = r.randrange(16), r.randrange(16)
        px[x, y] = shade(GROUND, r.choice([-30, -22, 18, 26]))
    return img


def patch_texture(resource, level):
    r = rng("patch", resource, level)
    img = ground(r)
    px = img.load()
    pal = RESOURCES[resource]
    clusters = 2 + 2 * level          # level 1: 4 clusters ... level 8: 18
    taken = set()
    for _ in range(clusters):
        for _try in range(20):
            x, y = r.randrange(15), r.randrange(15)
            w, h = r.choice([(1, 1), (2, 1), (1, 2), (2, 2)])
            cells = {(x + i, y + j) for i in range(w) for j in range(h)}
            if not cells & taken:
                taken |= cells
                main = r.choice(pal[:2])
                for (cx, cy) in cells:
                    px[cx, cy] = shade(main, r.randint(-8, 8))
                cx, cy = next(iter(cells))
                if r.random() < 0.6:
                    px[cx, cy] = pal[2]        # a bright glint
                break
    return img


# --------------------------------------------------------------------------- drill parts
def steel_frame():
    r = rng("drill_frame")
    img = new((176, 180, 188))
    noise(img, r, 6)
    px = img.load()
    for i in range(16):
        for (x, y) in ((i, 0), (i, 15), (0, i), (15, i)):
            px[x, y] = (214, 218, 226) if (x + y) % 2 == 0 else (196, 200, 208)
        for (x, y) in ((i, 1), (i, 14), (1, i), (14, i)):
            px[x, y] = (128, 132, 142)
    for (x, y) in ((2, 2), (13, 2), (2, 13), (13, 13)):
        px[x, y] = (84, 88, 98)  # rivets
    return img


def plate():
    r = rng("drill_plate")
    img = new((88, 92, 100))
    noise(img, r, 7)
    px = img.load()
    for i in range(16):
        px[i, 7] = (60, 64, 72); px[7, i] = (60, 64, 72)
        px[i, 8] = (112, 116, 126); px[8, i] = (112, 116, 126)
    for (x, y) in ((2, 2), (13, 2), (2, 13), (13, 13)):
        px[x, y] = (150, 154, 164)
    return img


def housing():
    r = rng("drill_housing")
    img = new((72, 76, 84))
    noise(img, r, 6)
    px = img.load()
    for i in range(16):
        px[i, 0] = (44, 46, 52); px[i, 15] = (44, 46, 52)
        px[0, i] = (44, 46, 52); px[15, i] = (44, 46, 52)
    for y in (5, 10):
        for x in range(1, 15):
            px[x, y] = (54, 58, 66); px[x, y + 1] = (96, 100, 110)
    for (x, y) in ((3, 3), (12, 3), (3, 12), (12, 12)):
        px[x, y] = (168, 172, 182)
    return img


def vent(lit):
    r = rng("drill_front")
    img = new((38, 40, 46))
    noise(img, r, 4)
    px = img.load()
    for i in range(16):
        px[i, 0] = (92, 96, 106); px[i, 15] = (92, 96, 106)
        px[0, i] = (92, 96, 106); px[15, i] = (92, 96, 106)
    for y in range(3, 13, 3):             # four horizontal slits
        for x in range(3, 13):
            if lit:
                px[x, y] = (255, 168, 40); px[x, y + 1] = (212, 92, 20)
            else:
                px[x, y] = (22, 22, 26); px[x, y + 1] = (60, 62, 70)
    return img


def top():
    r = rng("drill_top")
    img = new((96, 100, 108))
    noise(img, r, 6)
    px = img.load()
    for x in range(0, 16, 4):
        for y in range(16):
            px[x, y] = (70, 74, 82); px[x + 1, y] = (124, 128, 138)
    return img


def copper_metal():
    r = rng("drill_metal")
    img = new((196, 112, 74))
    noise(img, r, 10)
    px = img.load()
    for i in range(16):
        px[i, 0] = (232, 144, 98); px[0, i] = (232, 144, 98)
        px[i, 15] = (140, 74, 46); px[15, i] = (140, 74, 46)
    for (x, y) in ((4, 4), (11, 4), (4, 11), (11, 11)):
        px[x, y] = (96, 176, 152)          # verdigris spots
    return img


def dark_rock():
    r = rng("drill_dark")
    img = new((30, 30, 36))
    noise(img, r, 8)
    px = img.load()
    for _ in range(14):
        px[r.randrange(16), r.randrange(16)] = (58, 58, 68)
    return img


DRILL_TEXTURES = {
    "drill_frame": steel_frame, "drill_plate": plate, "drill_housing": housing,
    "drill_front": lambda: vent(False), "drill_front_on": lambda: vent(True),
    "drill_top": top, "drill_metal": copper_metal, "drill_dark": dark_rock,
}


def write_json(rel, obj):
    p = os.path.join(root, rel)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "w", encoding="utf-8", newline="\n") as f:
        json.dump(obj, f, indent=2)
        f.write("\n")


def main():
    for res in RESOURCES:
        for i, lv in enumerate(LEVELS, start=1):
            save(patch_texture(res, i), f"{res}_patch_{i}")
        name = f"{res}_patch"
        for i, lv in enumerate(LEVELS, start=1):
            write_json(f"assets/{NS}/models/block/{name}_{i}.json", {
                "parent": "minecraft:block/cube_all",
                "textures": {"all": f"{NS}:block/{name}_{i}"},
            })
        write_json(f"assets/{NS}/blockstates/{name}.json", {
            "variants": {f"richness={lv}": {"model": f"{NS}:block/{name}_{i}"}
                         for i, lv in enumerate(LEVELS, start=1)}
        })
        # the item shows the richest block
        write_json(f"assets/{NS}/items/{name}.json",
                   {"model": {"type": "minecraft:model", "model": f"{NS}:block/{name}_8"}})
        # the old single model is replaced by the per-level models
        old = os.path.join(root, "assets", NS, "models", "block", f"{name}.json")
        if os.path.exists(old):
            os.remove(old)
    for name, fn in DRILL_TEXTURES.items():
        save(fn(), name)
    print("generated", 3 * 8, "patch textures and", len(DRILL_TEXTURES), "drill textures")


main()
