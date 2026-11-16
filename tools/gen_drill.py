"""Generate the 3x3x3 drill multiblock content into a mod folder.

Regenerate with:  python tools/gen_drill.py .     (from the repo root; running it twice gives no diff)


One description of the whole model (boxes in a 0..48 cube) is sliced into one model per cell.
The controller is the front-centre of the bottom layer (x1, y0, z0), so its menu can be clicked; the other 26 cells are drill_part.
The models are made for a drill whose front faces north. The blockstates turn them: every block has a `facing` (north, east,
south, west) and the variant for it rotates the model about the y axis by 90 degrees per clockwise quarter turn.
"""
import json, os, sys

root = sys.argv[1]
NS = "horizon_factory"
SIZE = 3                      # cells per axis (3x3x3)
CELL = 16                     # model units per cell
CONTROLLER = (1, 0, 0)
FACINGS = ["north", "east", "south", "west"]   # clockwise from north, so the index is the number of quarter turns

# Texture slots of the model. Placeholders: vanilla textures.
TEXTURES = {
    # the mod's own textures, made by tools/gen_textures.py
    "frame":   "horizon_factory:block/drill_frame",
    "plate":   "horizon_factory:block/drill_plate",
    "housing": "horizon_factory:block/drill_housing",
    "front":   "horizon_factory:block/drill_front",
    "front_on": "horizon_factory:block/drill_front_on",
    "top":     "horizon_factory:block/drill_top",
    "metal":   "horizon_factory:block/drill_metal",
    "dark":    "horizon_factory:block/drill_dark",
}

# The whole drill: (from, to, texture slot). Units 0..48 on each axis, y is up.
BOXES = [
    ((0, 0, 0), (48, 3, 48), "plate"),            # base plate
    ((0, 3, 0), (4, 42, 4), "frame"),             # four corner posts
    ((44, 3, 0), (48, 42, 4), "frame"),
    ((0, 3, 44), (4, 42, 48), "frame"),
    ((44, 3, 44), (48, 42, 48), "frame"),
    ((0, 42, 0), (48, 46, 48), "frame"),          # top frame
    ((12, 3, 12), (36, 36, 36), "housing"),       # central housing (engine)
    ((12, 36, 12), (36, 40, 36), "top"),          # housing cap
    ((20, 40, 20), (28, 48, 28), "metal"),        # exhaust stack on top
    ((18, 3, 4), (30, 14, 12), "dark"),           # fuel hopper on the north side
    ((20, 14, 2), (28, 18, 12), "metal"),         # hopper neck
    ((18, 0, 18), (30, 3, 30), "dark"),           # drill collar on the base
]
# The front face of the housing (north, -z) is a separate thin box so it can swap for the lit texture.
FRONT = ((14, 6, 11), (34, 26, 12), "front")


def clip(box, cx, cy, cz):
    (x0, y0, z0), (x1, y1, z1), tex = box
    o = (cx * CELL, cy * CELL, cz * CELL)
    lo = [max(v, o[i]) for i, v in enumerate((x0, y0, z0))]
    hi = [min(v, o[i] + CELL) for i, v in enumerate((x1, y1, z1))]
    if any(lo[i] >= hi[i] for i in range(3)):
        return None
    return {
        "from": [lo[i] - o[i] for i in range(3)],
        "to":   [hi[i] - o[i] for i in range(3)],
        "faces": {d: {"texture": "#" + tex} for d in ("north", "south", "east", "west", "up", "down")},
    }


def slice_model(cx, cy, cz, lit=False):
    els = []
    for b in BOXES:
        e = clip(b, cx, cy, cz)
        if e: els.append(e)
    (fx0, fy0, fz0), (fx1, fy1, fz1), _ = FRONT
    e = clip(((fx0, fy0, fz0), (fx1, fy1, fz1), "front_on" if lit else "front"), cx, cy, cz)
    if e: els.append(e)
    used = {f["texture"][1:] for el in els for f in el["faces"].values()}
    model = {
        "textures": {k: v for k, v in TEXTURES.items() if k in used},
        "elements": els,
    }
    # particle texture is required by block models
    model["textures"]["particle"] = TEXTURES["frame"]
    return model


def write(rel, obj):
    p = os.path.join(root, rel)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "w", encoding="utf-8", newline="\n") as f:
        json.dump(obj, f, indent=2)
        f.write("\n")


def cell_name(x, y, z):
    return f"x{x}_y{y}_z{z}"

cells = [(x, y, z) for y in range(SIZE) for x in range(SIZE) for z in range(SIZE)]
parts = [c for c in cells if c != CONTROLLER]
assert len(cells) == 27 and len(parts) == 26

# ---- models: one per cell, plus the lit controller
for c in cells:
    if c == CONTROLLER:
        write(f"assets/{NS}/models/block/drill.json", slice_model(*c, lit=False))
        write(f"assets/{NS}/models/block/drill_lit.json", slice_model(*c, lit=True))
    else:
        write(f"assets/{NS}/models/block/drill_part_{cell_name(*c)}.json", slice_model(*c))

# ---- blockstates
def turned(model, facing):
    """A blockstate variant: the model, turned about y to the facing (no rotation key for north)."""
    v = {"model": model}
    if FACINGS.index(facing):
        v["y"] = 90 * FACINGS.index(facing)
    return v

write(f"assets/{NS}/blockstates/drill.json", {"variants": {
    f"facing={f},lit={lit}": turned(f"{NS}:block/drill" + ("_lit" if lit == "true" else ""), f)
    for f in FACINGS for lit in ("false", "true")
}})
write(f"assets/{NS}/blockstates/drill_part.json", {"variants": {
    f"cell={cell_name(*c)},facing={f}": turned(f"{NS}:block/drill_part_{cell_name(*c)}", f)
    for c in parts for f in FACINGS
}})

# ---- item: the controller's own model is a slice, so the inventory item shows the whole drill from a compact model
write(f"assets/{NS}/items/drill.json", {"model": {"type": "minecraft:model", "model": f"{NS}:block/drill_item"}})
item_model = {"textures": {k: v for k, v in TEXTURES.items() if k != "front_on"}, "elements": []}
item_model["textures"]["particle"] = TEXTURES["frame"]
for b in BOXES + [FRONT]:
    (x0, y0, z0), (x1, y1, z1), tex = b
    s = CELL / 48 * 1.0   # scale the 48-unit drill into one block
    item_model["elements"].append({
        "from": [round(v * s, 3) for v in (x0, y0, z0)],
        "to":   [round(v * s, 3) for v in (x1, y1, z1)],
        "faces": {d: {"texture": "#" + tex} for d in ("north", "south", "east", "west", "up", "down")},
    })
write(f"assets/{NS}/models/block/drill_item.json", item_model)

# ---- menu type: slot 0 is fuel, 1..8 are output (players only take from them); 4 data slots: burn left, progress, ore code, richness
write(f"data/{NS}/horizon/menu/drill.json", {"slots": 9, "data_slots": 4, "results": list(range(1, 9))})

# ---- block definitions
write(f"data/{NS}/horizon/block/drill.json", {
    # a dropper template: its menu (3x3, nine slots) opens on use, which the drill container needs
    "template": "minecraft:dropper",
    "properties": {"hardness": 3.5, "occludes": False},
    "state_properties": {"lit": {"type": "boolean"}},
    "default_state": {"lit": "false", "facing": "north", "triggered": "false"},
    "state_light": {"property": "lit", "values": {"false": 0, "true": 15}},
    "item": True,
})
write(f"data/{NS}/horizon/block/drill_part.json", {
    "template": "minecraft:iron_block",
    "properties": {"hardness": 3.5, "occludes": False},
    "state_properties": {
        "cell": {"type": "enum", "values": [cell_name(*c) for c in parts]},
        "facing": {"type": "enum", "values": FACINGS},
    },
    "default_state": {"cell": cell_name(*parts[0]), "facing": "north"},
    "item": False,
})

# ---- loot: the controller drops the drill; parts drop nothing (the native ticket drops one item for the whole structure)
write(f"data/{NS}/loot_table/blocks/drill.json", {
    "type": "minecraft:block",
    "pools": [{
        "condition": {"type": "minecraft:survives_explosion"},
        "entries": [{"type": "minecraft:item", "name": f"{NS}:drill"}],
        "rolls": 1,
    }],
    "random_sequence": f"{NS}:blocks/drill",
})
write(f"data/{NS}/loot_table/blocks/drill_part.json", {
    "type": "minecraft:block", "pools": [], "random_sequence": f"{NS}:blocks/drill_part",
})

# ---- recipe (placeholder: propose in the PR)
write(f"data/{NS}/recipe/drill.json", {
    "type": "minecraft:crafting_shaped",
    "category": "misc",
    "key": {"I": "minecraft:iron_ingot", "F": "minecraft:furnace", "P": "minecraft:iron_pickaxe"},
    "pattern": ["III", "IFI", "IPI"],
    "result": {"id": f"{NS}:drill"},
})

# ---- tags and lang (merge into existing files when present)
def merge_json(rel, update):
    p = os.path.join(root, rel)
    cur = json.load(open(p, encoding="utf-8")) if os.path.exists(p) else {}
    update(cur)
    write(rel, cur)

def add_tag(cur):
    vals = cur.setdefault("values", [])
    for v in (f"{NS}:drill", f"{NS}:drill_part"):
        if v not in vals: vals.append(v)
merge_json("data/minecraft/tags/block/mineable/pickaxe.json", add_tag)
merge_json(f"assets/{NS}/lang/en_us.json", lambda cur: cur.update({f"block.{NS}.drill": "Drill", f"block.{NS}.drill_part": "Drill Part"}))
print("generated", len(cells), "cells,", len(parts), "parts")
