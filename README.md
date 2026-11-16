# Factory (Horizon mod)

Factorio-style resource fields and a coal-fuelled drill for [Horizon](https://playhorizon.dev), the Rust Minecraft client and server (Minecraft 26.3).

- **Resource fields.** Coal, iron and copper patches generate in the world as fields of ore blocks. A block's richness (1 to 8) says how full it is: a full block holds about 128 units of ore, each level about 16.
- **The drill.** A 3x3x3 machine you place with one item. Its front (the side with the fuel hopper and the menu) faces you, the body stands behind it. It needs solid ground under all of it and mines the 5x5 area under it, nearest block first.
  - Burns coal (one fuel slot). Mines one unit every 4 seconds while it burns and slowly lowers the richness of the blocks it mines; a worked-out block turns to stone.
  - Eight output slots. Hoppers and pipes take from the outputs and put coal into the fuel slot.
  - Its screen shows which ore it is on (the real raw iron, raw copper or coal item) and how many units are left under it.
  - Breaking any block of it removes the whole drill and drops one Drill.
- **The Resource Scanner.** Right click opens a full-screen map: terrain, one marker per known field, your position and the way you look. Drag to move, scroll to zoom, hover a field to see how much ore is left in it. The server keeps the map up to date around every player in the overworld.
- **Web map.** `web/index.html` draws the same data in a browser (see below).

Recipes: **Drill**: iron ingots on the top row and sides, a furnace in the middle, an iron pickaxe at the bottom (`III / IFI / IPI`). **Scanner**: iron ingots in the corners and the middle of the sides, redstone and a compass (`IRI / RCR / IRI`).

## What is in this repository

| Part | Where | What it is |
|---|---|---|
| Content mod | `data/`, `assets/`, `horizon-mod.toml` | Blocks, items, models, textures, worldgen, loot, recipes. Packed to `dist/horizon_factory.hmod` |
| Native mod | `native/` | Rust code: the drill's logic and screen, the scanner and its map, the map database. `native/core` is plain Rust with unit tests; `native/api` is the glue to the game |
| Generators | `tools/gen_*.py` | The textures, models and block definitions are drawn by these scripts (needs `pip install pillow`); run them from the repository root, for example `python tools/gen_drill.py .` |
| Web map | `web/index.html` | A single page that reads the map data files |

## Install

You need **both** parts. Without the content mod the blocks do not exist; without the native mod you cannot open the drill's screen or the scanner's map.

**Built for Horizon Nightly 2026-11-13 (commit `9f42064`).** The native mod pins that commit.

1. **Content mod.** Download [`dist/horizon_factory.hmod`](dist/horizon_factory.hmod) (SHA-256 `8b084d0ebf19832a0fe4d4b279dcd758839add32f5ca213762f266eb08b19f91`), put it in the `horizon-mods` folder of your game folder and switch it on in Options > Mods. Or build it yourself: `cargo horizon-mod build` (needs `cargo-horizon-mod` from the Horizon repository).
2. **Native mod.** In the Horizon launcher open **Mods > Add from GitHub** and paste this repository's link, or run `horizon-launcher mods add-github https://github.com/LaurensB235/horizon-factory-mod --trust-native-code`. The launcher builds a game with the mod in it.

> **Native mods run with full access to your computer and are not reviewed by anyone.** Read `native/` before you build it. The mod itself uses no file, network or process code (it keeps its map files through the game's own settings call).

### Playing on a server

The server needs the content mod in `horizon-mods/` and an engine built with the native mod; players need both parts too.

```
horizon-launcher native-build native --server-output <folder> --trust-native-code
```

builds a standalone server engine (on the operating system the launcher runs on; a Linux server needs it built on Linux). Point `[engine] path` in `server.toml` at `<folder>/horizon-engine`.

Ore fields are only created in chunks generated after the mod is on, so use a new world, or explore new land.

### The web map

The server writes the map data as plain files in `horizon-mods/.horizon-mod-data/factory/` (`fields.txt`, `tiles.txt`, `terrain_*.txt`). Serve `web/index.html` with those files next to it under `data/` (any static web server); it reads `data/fields.txt` and the terrain tiles. The server scans around every player in the overworld once a minute, and at once when someone uses the scanner; the page shows what has been scanned so far.

## Development

```
cargo horizon-mod build && cargo horizon-mod check target/horizon-mod/horizon_factory.hmod
cd native/core && cargo test
```

CI runs the same. The textures and drill models are generated: change the scripts in `tools/`, regenerate (`python tools/gen_textures.py .`, `python tools/gen_drill.py .`, `python tools/gen_scanner_item.py .`) and commit the result. The packed `dist/horizon_factory.hmod` is rebuilt from the sources and is byte-identical on every machine (line endings are pinned by `.gitattributes`).

Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.
