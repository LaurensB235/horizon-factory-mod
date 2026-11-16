"""Generate the Resource Scanner item: its 16x16 icon, model, item definition, data declaration, name and recipe.

    python tools/gen_scanner_item.py .        (from the repo root; needs Pillow: pip install pillow)

Drawn here pixel by pixel, so running it twice gives byte-identical files. Nothing is copied from the game. The scanner is
a handheld device: a dark body with an antenna, a green radar screen with a sweep ring and two blips, a row of buttons and
a speaker grille. It is a plain item (template `minecraft:diamond`, stack size 1); everything it does is native code.
"""
import json
import os
import sys

from PIL import Image

root = sys.argv[1] if len(sys.argv) > 1 else "."
NS = "horizon_factory"

# palette
EDGE = (22, 24, 28)
BODY_DARK = (58, 64, 72)
BODY = (88, 96, 106)
BODY_LIGHT = (128, 138, 150)
SCREEN_BG = (10, 34, 24)
RING = (96, 232, 160)
RING_DIM = (44, 130, 92)
BLIP_IRON = (236, 200, 168)
BLIP_COPPER = (232, 128, 90)
ANT = (170, 176, 184)
ANT_TIP = (255, 84, 60)
BUTTON_A = (240, 170, 48)
BUTTON_B = (90, 200, 120)
GRILLE = (34, 38, 44)

img = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
px = img.load()


def put(x, y, colour):
    px[x, y] = colour + (255,)


# body: a rounded rectangle x 3..12, y 4..15
for y in range(4, 16):
    for x in range(3, 13):
        corner = (x in (3, 12)) and (y in (4, 15))
        if not corner:
            put(x, y, BODY)
for x in range(4, 12):
    put(x, 4, BODY_LIGHT)          # lit top edge
for y in range(5, 15):
    put(4, y, BODY_LIGHT)          # lit left edge
    put(11, y, BODY_DARK)          # shaded right edge
for x in range(4, 12):
    put(x, 14, BODY_DARK)          # shaded bottom edge
# outline
for x in range(4, 12):
    put(x, 3, EDGE)
    put(x, 15, EDGE)
for y in range(4, 15):
    put(2, y, EDGE)
    put(13, y, EDGE)
for (x, y) in ((3, 3), (12, 3), (3, 15), (12, 15)):
    put(x, y, EDGE)

# antenna on the top right: a thin rod and a red tip
for y in range(0, 3):
    put(11, y + 1, ANT)
put(11, 0, ANT_TIP)

# screen: x 4..11? keep a bezel: x 5..10, y 5..10
for y in range(5, 11):
    for x in range(5, 11):
        put(x, y, SCREEN_BG)
# sweep ring (a small circle) and its centre dot
for (x, y) in ((6, 6), (7, 6), (8, 6), (9, 6), (6, 9), (7, 9), (8, 9), (9, 9), (5, 7), (5, 8), (10, 7), (10, 8)):
    put(x, y, RING_DIM)
for (x, y) in ((7, 7), (8, 8)):
    put(x, y, RING)
put(8, 7, RING)                     # the sweep arm
put(9, 6, RING)
# two blips: an iron and a copper field
put(6, 9, BLIP_IRON)
put(9, 8, BLIP_COPPER)

# buttons and speaker grille
put(5, 12, BUTTON_A)
put(6, 12, BUTTON_A)
put(8, 12, BUTTON_B)
put(9, 12, BUTTON_B)
for x in (5, 7, 9):
    put(x, 13, GRILLE)

os.makedirs(os.path.join(root, "assets", NS, "textures", "item"), exist_ok=True)
img.save(os.path.join(root, "assets", NS, "textures", "item", "scanner.png"))


def write_json(rel, obj):
    path = os.path.join(root, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="") as f:
        json.dump(obj, f, indent=2)
        f.write("\n")


# model: a flat item sprite; the item definition points at it
write_json(f"assets/{NS}/models/item/scanner.json", {
    "parent": "minecraft:item/handheld",
    "textures": {"layer0": f"{NS}:item/scanner"},
})
write_json(f"assets/{NS}/items/scanner.json", {
    "model": {"type": "minecraft:model", "model": f"{NS}:item/scanner"},
})
# the item itself: a plain item that stacks to one
write_json(f"data/{NS}/horizon/item/scanner.json", {
    "template": "minecraft:diamond",
    "max_stack_size": 1,
})

# the name, merged into the language file in its existing order
lang_path = os.path.join(root, "assets", NS, "lang", "en_us.json")
with open(lang_path, encoding="utf-8") as f:
    lang = json.load(f)
lang[f"item.{NS}.scanner"] = "Resource Scanner"
write_json(f"assets/{NS}/lang/en_us.json", lang)

# the recipe: the same shape as before, but the result is the scanner item
write_json(f"data/{NS}/recipe/scanner.json", {
    "type": "minecraft:crafting_shaped",
    "category": "misc",
    "key": {
        "I": "minecraft:iron_ingot",
        "R": "minecraft:redstone",
        "C": "minecraft:compass",
    },
    "pattern": ["IRI", "RCR", "IRI"],
    "result": {"id": f"{NS}:scanner", "count": 1},
})

# an enlarged preview for checking the pixels by eye (not part of the mod)
preview = img.resize((160, 160), Image.NEAREST)
sheet = Image.new("RGBA", (160, 160), (120, 130, 140, 255))
sheet.paste(preview, (0, 0), preview)
sheet.save(os.path.join(root, "scanner_preview.png"))
print("wrote the scanner item")
