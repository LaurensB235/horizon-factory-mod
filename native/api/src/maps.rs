//! The full-screen map, client glue. When the scanner is used the server sends the known terrain tiles and fields near
//! the player and tells the client to open the mod's own screen; this file keeps that data and draws it over the whole
//! window: terrain colours, a dot per field in the resource colour, an arrow for the player's position and facing.
//! Drag with the mouse to pan, scroll to zoom, hover a dot to see how much ore is left in that field, Esc closes it.
//! The rules (view maths, picking, tooltip, messages, tiles) are in `factory_core::view`, `::wire` and `::tiles`.
use factory_core::drill::Resource;
use factory_core::map::Entry;
use factory_core::terrain::{self, Run};
use factory_core::tiles::{self, TILE_BLOCKS, TileId};
use factory_core::view::{self, View};
use factory_core::wire;
use horizon_client::mod_api::{
    Flow, Host, HudBatch, NetFromServerEvent, ScreenMouseButtonPressedPreEvent,
    ScreenMouseButtonReleasedPreEvent, ScreenMouseDraggedPreEvent, ScreenMouseScrolledPreEvent,
    ScreenRenderPostEvent,
};
use std::collections::HashMap;
use std::sync::Mutex;

/// The mod id the map's screen is opened with (`Host::mod_screen_open`).
const MOD_ID: &str = "factory";
/// Blocks per terrain pixel (the scan takes one column in two each way).
const PIXEL_BLOCKS: f32 = 2.0;
const BACKGROUND: u32 = 0xFF_10_12_14;
const EDGE: u32 = 0xFF_00_00_00;
const WHITE: u32 = 0xFF_FF_FF_FF;

/// One terrain tile ready to draw: its runs of equal pixels and its average colour for the zoomed-out view.
struct Tile {
    runs: Vec<Run>,
    average: Option<u32>,
}

struct State {
    /// The token of the view the data belongs to; a new token starts a fresh picture.
    token: u64,
    tiles: HashMap<TileId, Tile>,
    fields: Vec<Entry>,
    view: View,
    /// The size of the last frame in GUI pixels, which the mouse hooks need (they are not given the screen's size).
    size: (f32, f32),
    dragging: bool,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    let mut guard = STATE.lock().ok()?;
    let state = guard.get_or_insert_with(|| State {
        token: 0,
        tiles: HashMap::new(),
        fields: Vec::new(),
        view: View::new(0.0, 0.0, 1.0),
        size: (854.0, 480.0),
        dragging: false,
    });
    Some(f(state))
}

fn start_view(state: &mut State, token: u64) {
    if state.token != token {
        state.token = token;
        state.tiles.clear();
        state.fields.clear();
    }
}

// ------------------------------------------------------------------------------------------------- messages

/// Client: the tiles, the fields and the open message from the server. The bytes are the sender's own, so each part is
/// decoded defensively.
pub fn net_from_server(host: &mut dyn Host, event: &mut NetFromServerEvent<'_>) {
    let text = String::from_utf8_lossy(event.data);
    match event.channel {
        wire::TILES_CHANNEL => {
            let Some((token, tiles_in)) = wire::unpack_tiles(&text) else {
                return;
            };
            with_state(|s| {
                start_view(s, token);
                for (id, classes) in tiles_in {
                    let tile = Tile {
                        runs: tiles::tile_runs(&classes),
                        average: tiles::tile_average(&classes),
                    };
                    s.tiles.insert(id, tile);
                }
            });
        }
        wire::FIELDS_CHANNEL => {
            let Some((token, fields)) = wire::unpack_fields(&text) else {
                return;
            };
            with_state(|s| {
                start_view(s, token);
                s.fields = fields;
            });
        }
        wire::OPEN_CHANNEL => {
            let Some((token, x, z)) = wire::unpack_open(&text) else {
                return;
            };
            with_state(|s| {
                start_view(s, token);
                s.view = View::new(x, z, 1.0);
                s.dragging = false;
            });
            host.mod_screen_open(MOD_ID);
        }
        _ => {}
    }
}

// ------------------------------------------------------------------------------------------------- input

fn ours(host: &dyn Host) -> bool {
    host.mod_screen() == Some(MOD_ID)
}

/// Left button down on the map starts a drag.
pub fn mouse_pressed(host: &dyn Host, event: &mut ScreenMouseButtonPressedPreEvent<'_>) -> Flow {
    if ours(host) && event.button == 0 {
        with_state(|s| s.dragging = true);
    }
    Flow::Continue
}

pub fn mouse_released(host: &dyn Host, event: &mut ScreenMouseButtonReleasedPreEvent<'_>) -> Flow {
    if ours(host) && event.button == 0 {
        with_state(|s| s.dragging = false);
    }
    Flow::Continue
}

/// Dragging pans: the map follows the cursor.
pub fn mouse_dragged(host: &dyn Host, event: &mut ScreenMouseDraggedPreEvent<'_>) -> Flow {
    if ours(host) {
        with_state(|s| {
            if s.dragging {
                s.view.pan(event.delta[0], event.delta[1]);
            }
        });
    }
    Flow::Continue
}

/// The wheel zooms around the cursor.
pub fn mouse_scrolled(host: &dyn Host, event: &mut ScreenMouseScrolledPreEvent<'_>) -> Flow {
    if ours(host) {
        let cursor = host.cursor();
        with_state(|s| {
            let (w, h) = s.size;
            s.view.zoom_at(w, h, cursor[0], cursor[1], event.scroll[1]);
        });
    }
    Flow::Continue
}

// ------------------------------------------------------------------------------------------------- drawing

fn argb(rgb: u32) -> u32 {
    0xFF00_0000 | (rgb & 0x00FF_FFFF)
}

/// A rectangle clipped to the screen; `None` when nothing is left.
fn clip(rect: [f32; 4], w: f32, h: f32) -> Option<[f32; 4]> {
    let x0 = rect[0].max(0.0);
    let y0 = rect[1].max(0.0);
    let x1 = (rect[0] + rect[2]).min(w);
    let y1 = (rect[1] + rect[3]).min(h);
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
}

fn fill(host: &dyn Host, batch: &mut HudBatch, rect: [f32; 4], colour: u32, size: (f32, f32)) {
    if let Some(r) = clip(rect, size.0, size.1) {
        host.fill(batch, r, colour);
    }
}

fn draw_terrain(host: &dyn Host, batch: &mut HudBatch, s: &State) {
    let (w, h) = s.size;
    let scale = s.view.scale as f32;
    let tile_px = TILE_BLOCKS as f32 * scale;
    let pixel = PIXEL_BLOCKS * scale;
    for (id, tile) in &s.tiles {
        let (sx, sy) = s.view.to_screen(
            w,
            h,
            f64::from(id.0 * TILE_BLOCKS),
            f64::from(id.1 * TILE_BLOCKS),
        );
        if sx > w || sy > h || sx + tile_px < 0.0 || sy + tile_px < 0.0 {
            continue;
        }
        // zoomed far out a terrain pixel is under one screen pixel: the whole tile is one rectangle
        if pixel < 1.0 {
            if let Some(avg) = tile.average {
                fill(host, batch, [sx, sy, tile_px, tile_px], argb(avg), s.size);
            }
            continue;
        }
        for run in &tile.runs {
            let rect = [
                sx + f32::from(run.x) * pixel,
                sy + f32::from(run.y) * pixel,
                f32::from(run.len) * pixel + 0.5,
                pixel + 0.5,
            ];
            fill(host, batch, rect, argb(terrain::colour(run.class)), s.size);
        }
    }
}

fn draw_marker(
    host: &dyn Host,
    batch: &mut HudBatch,
    at: (f32, f32),
    half: f32,
    resource: Resource,
    size: (f32, f32),
) {
    fill(
        host,
        batch,
        [
            at.0 - half - 1.0,
            at.1 - half - 1.0,
            half * 2.0 + 2.0,
            half * 2.0 + 2.0,
        ],
        argb(view::outline_rgb(resource)),
        size,
    );
    fill(
        host,
        batch,
        [at.0 - half, at.1 - half, half * 2.0, half * 2.0],
        argb(view::resource_rgb(resource)),
        size,
    );
}

/// The player as an arrow at `at`, pointing where they look. `yaw` is the game's (0 faces south, 90 west).
fn draw_arrow(host: &dyn Host, batch: &mut HudBatch, at: (f32, f32), size: f32, yaw: f32) {
    let r = yaw.to_radians();
    let f = [-r.sin(), r.cos()]; // forward on the map (x east, z south)
    let side = [-f[1], f[0]];
    let pt = |a: f32, b: f32| [at.0 + f[0] * a + side[0] * b, at.1 + f[1] * a + side[1] * b];
    for (grow, colour) in [(1.5, EDGE), (0.0, WHITE)] {
        host.quad(
            batch,
            [
                pt(size + grow, 0.0),
                pt(-size * 0.7 - grow, -size * 0.7 - grow),
                pt(-size * 0.3, 0.0),
                pt(-size * 0.7 - grow, size * 0.7 + grow),
            ],
            colour,
        );
    }
}

/// The hover tooltip for field `index`, next to the cursor and kept on the screen.
fn draw_tooltip(
    host: &dyn Host,
    batch: &mut HudBatch,
    entry: &Entry,
    cursor: [f32; 2],
    size: (f32, f32),
) {
    let (title, left) = view::tooltip(entry);
    let width = host.text_width(&title).max(host.text_width(&left)) + 26.0;
    let (bw, bh) = (width, 30.0);
    let x = (cursor[0] + 14.0).min(size.0 - bw - 4.0).max(4.0);
    let y = (cursor[1] + 14.0).min(size.1 - bh - 4.0).max(4.0);
    host.fill(batch, [x - 1.0, y - 1.0, bw + 2.0, bh + 2.0], 0xFF_00_00_00);
    host.fill(batch, [x, y, bw, bh], 0xF0_1A_1C_20);
    host.fill(
        batch,
        [x + 4.0, y + 5.0, 10.0, 10.0],
        argb(view::outline_rgb(entry.resource)),
    );
    host.fill(
        batch,
        [x + 5.0, y + 6.0, 8.0, 8.0],
        argb(view::resource_rgb(entry.resource)),
    );
    host.text(batch, &title, [x + 18.0, y + 5.0], 0xFF_FF_FF, true);
    host.text(batch, &left, [x + 18.0, y + 17.0], 0xD0_D0_D0, true);
}

/// Client: the full map, drawn over the whole window while the mod's own screen is open.
pub fn render_screen(host: &dyn Host, event: &mut ScreenRenderPostEvent<'_>) {
    if !ours(host) {
        return;
    }
    let size = (event.batch.width, event.batch.height);
    let cursor = host.cursor();
    let camera = host.camera();
    // remember the size for the mouse hooks, and take a copy of what the frame needs
    let Some(()) = with_state(|s| s.size = size) else {
        return;
    };
    let Ok(guard) = STATE.lock() else { return };
    let Some(s) = guard.as_ref() else { return };
    host.fill(event.batch, [0.0, 0.0, size.0, size.1], BACKGROUND);
    if s.tiles.is_empty() && s.fields.is_empty() {
        host.text(
            event.batch,
            "Nothing known here yet.",
            [size.0 / 2.0 - 60.0, size.1 / 2.0],
            WHITE & 0xFF_FF_FF,
            true,
        );
    }
    draw_terrain(host, event.batch, s);
    for entry in &s.fields {
        let at = s
            .view
            .to_screen(size.0, size.1, f64::from(entry.x), f64::from(entry.z));
        if at.0 > -8.0 && at.1 > -8.0 && at.0 < size.0 + 8.0 && at.1 < size.1 + 8.0 {
            // dots shrink when zoomed out so they do not pile into one clump
            let half = (s.view.scale as f32 * 3.0).clamp(1.5, 3.0);
            draw_marker(host, event.batch, at, half, entry.resource, size);
        }
    }
    if let Some(c) = &camera {
        let at = s
            .view
            .to_screen(size.0, size.1, c.position[0], c.position[2]);
        if at.0 > -10.0 && at.1 > -10.0 && at.0 < size.0 + 10.0 && at.1 < size.1 + 10.0 {
            draw_arrow(host, event.batch, at, 7.0, c.yaw);
        }
    }
    // the legend and the controls, then the hover tooltip on top
    // a dark backing keeps the text readable over the map
    host.fill(event.batch, [0.0, 0.0, 250.0, 32.0], 0xC0_10_12_14);
    host.fill(event.batch, [0.0, size.1 - 50.0, 70.0, 50.0], 0xC0_10_12_14);
    host.text(event.batch, "Resource map", [10.0, 8.0], 0xFF_FF_FF, true);
    host.text(
        event.batch,
        "Drag to move, scroll to zoom, Esc to close",
        [10.0, 20.0],
        0xB0_B0_B0,
        true,
    );
    for (i, resource) in [Resource::Coal, Resource::Iron, Resource::Copper]
        .into_iter()
        .enumerate()
    {
        let y = size.1 - 14.0 - (2 - i) as f32 * 12.0;
        host.fill(
            event.batch,
            [9.0, y, 9.0, 9.0],
            argb(view::outline_rgb(resource)),
        );
        host.fill(
            event.batch,
            [10.0, y + 1.0, 7.0, 7.0],
            argb(view::resource_rgb(resource)),
        );
        let name = match resource {
            Resource::Coal => "Coal",
            Resource::Iron => "Iron",
            Resource::Copper => "Copper",
        };
        host.text(
            event.batch,
            name,
            [22.0, y],
            view::label_rgb(resource),
            true,
        );
    }
    if let Some(i) = view::pick(&s.fields, &s.view, size.0, size.1, cursor[0], cursor[1]) {
        draw_tooltip(host, event.batch, &s.fields[i], cursor, size);
    }
}
