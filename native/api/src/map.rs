//! The map database and what is sent to a player's map screen, glue. Every minute, and at once when a player uses the
//! scanner, the server scans around the player and keeps two things, both saved with `Host::set_setting` (one plain file
//! per key in the mod's data folder, so no file code lives here):
//!  - `fields.txt`: every field known, with the ore units left in it (counted block by block);
//!  - `terrain_<tx>_<tz>.txt`: the terrain as tiles of 128 x 128 blocks, and `tiles.txt` listing them with a version.
//! The web page reads these files; the map screen gets the same data over mod channels (see `send_view`).
//! A scan replaces what the field store held inside its square, so a depleted field vanishes. The rules (merging, units,
//! formats, tiles, messages) are in `factory_core::map`, `::tiles`, `::terrain` and `::wire`.
use crate::sampling;
use factory_core::map::{self, Entry};
use factory_core::scan;
use factory_core::terrain::Picture;
use factory_core::tiles::{self, TILE_BLOCKS, TileId, Tiles};
use factory_core::wire;
use horizon_client::mod_api::{Host, NativeServerPlayer, ServerTickEvent, ServerTickPhase};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

const MOD_ID: &str = "factory";
/// The setting key of the field list, so the file is `<mod data folder>/fields.txt`.
const KEY: &str = "fields.txt";
const TILES_KEY: &str = "tiles.txt";
/// Server ticks between scans of the whole player list (20 a second: a minute).
const EVERY_TICKS: u64 = 1200;
const OVERWORLD: &str = "minecraft:overworld";
/// The map screen gets the tiles within this many tiles of the player's (so at most 5 x 5 tiles, 640 blocks across).
const VIEW_TILES: i32 = 2;

/// The field store, loaded from the saved file the first time it is needed.
static STORE: Mutex<Option<Vec<Entry>>> = Mutex::new(None);

/// The terrain tiles in memory and the tick each was last written (its version on the page).
struct TileState {
    tiles: Tiles,
    versions: HashMap<TileId, u64>,
}
static TILE_STATE: Mutex<Option<TileState>> = Mutex::new(None);

/// The last server tick seen, so a scan started by a player's action can stamp what it finds.
static TICKS: AtomicU64 = AtomicU64::new(0);
/// A new number for every view sent to a client, so the client can tell one view from the next.
static VIEWS: AtomicU64 = AtomicU64::new(1);

fn tile_key(id: TileId) -> String {
    format!("terrain_{}_{}.txt", id.0, id.1)
}

/// The index text: one line `tx,tz,version` per tile.
fn index_text(versions: &HashMap<TileId, u64>) -> String {
    let mut ids: Vec<&TileId> = versions.keys().collect();
    ids.sort();
    ids.iter()
        .map(|id| format!("{},{},{}\n", id.0, id.1, versions[*id]))
        .collect()
}

fn read_index(text: &str) -> HashMap<TileId, u64> {
    text.lines()
        .filter_map(|l| {
            let mut p = l.split(',');
            let x = p.next()?.trim().parse().ok()?;
            let z = p.next()?.trim().parse().ok()?;
            let v = p.next()?.trim().parse().ok()?;
            Some(((x, z), v))
        })
        .collect()
}

/// The tile state, created (and its index read back from the saved file) on first use.
fn tile_state<'a>(host: &dyn Host, guard: &'a mut Option<TileState>) -> &'a mut TileState {
    guard.get_or_insert_with(|| TileState {
        tiles: Tiles::new(),
        versions: read_index(&host.setting(MOD_ID, TILES_KEY).unwrap_or_default()),
    })
}

/// Bring the given tiles into memory (a tile written before a restart is read back from its file).
fn load_tiles(host: &dyn Host, state: &mut TileState, ids: &[TileId]) {
    for &id in ids {
        if state.tiles.get(id).is_none() {
            if let Some(classes) = host
                .setting(MOD_ID, &tile_key(id))
                .and_then(|t| tiles::decode_tile(&t))
            {
                state.tiles.insert(id, classes);
            }
        }
    }
}

/// Write the picture into the terrain tiles and save the ones that changed, plus the index when any did.
fn save_terrain(host: &mut dyn Host, picture: &Picture, tick: u64) {
    let Ok(mut guard) = TILE_STATE.lock() else {
        return;
    };
    let state = tile_state(&*host, &mut guard);
    load_tiles(&*host, state, &Tiles::touched(picture));
    let changed = state.tiles.apply(picture);
    for id in &changed {
        if let Some(classes) = state.tiles.get(*id) {
            host.set_setting(MOD_ID, &tile_key(*id), &tiles::encode_tile(classes));
            state.versions.insert(*id, tick);
        }
    }
    if !changed.is_empty() {
        host.set_setting(MOD_ID, TILES_KEY, &index_text(&state.versions));
    }
}

/// Scan around `player` now and update the stores; false when the area could not be read (another dimension, say).
pub fn scan_player(host: &mut dyn Host, player: &NativeServerPlayer) -> bool {
    if player.dimension != OVERWORLD {
        return false;
    }
    let tick = TICKS.load(Ordering::Relaxed);
    let Ok(area) = sampling::sample_area(host, player) else {
        return false;
    };
    let fields = scan::find_fields(&area.samples);
    // the exact count of what is left, block by block
    let units = sampling::count_units(host, &fields, area.origin_x, area.origin_z);
    let found = map::entries_from(&fields, area.origin_x, area.origin_z, tick, &units);
    let text = {
        let Ok(mut guard) = STORE.lock() else {
            return false;
        };
        let store = guard
            .get_or_insert_with(|| map::decode(&host.setting(MOD_ID, KEY).unwrap_or_default()));
        map::merge(
            store,
            map::picture_area(area.origin_x, area.origin_z),
            found,
        );
        map::encode(store)
    };
    host.set_setting(MOD_ID, KEY, &text);
    save_terrain(
        host,
        &Picture {
            origin_x: area.origin_x,
            origin_z: area.origin_z,
            classes: area.terrain.clone(),
        },
        tick,
    );
    true
}

/// Every minute: scan around every player in the overworld.
pub fn server_tick(host: &mut dyn Host, event: &mut ServerTickEvent<'_>) {
    if !matches!(
        event.phase,
        ServerTickPhase::ServerTickEnd {
            world_running: true
        }
    ) {
        return;
    }
    let tick = event.context.tick.get();
    TICKS.store(tick, Ordering::Relaxed);
    if tick == 0 || tick % EVERY_TICKS != 0 {
        return;
    }
    for player in host.server_players() {
        scan_player(host, &player);
    }
}

/// Send the player's client everything known around them (the nearest tiles and fields) and tell it to open the map.
pub fn send_view(host: &mut dyn Host, player: &NativeServerPlayer) {
    let token = VIEWS.fetch_add(1, Ordering::Relaxed);
    let (px, pz) = (
        player.position[0].floor() as i32,
        player.position[2].floor() as i32,
    );
    let (cx, cz) = (px.div_euclid(TILE_BLOCKS), pz.div_euclid(TILE_BLOCKS));
    let tile_list: Vec<(TileId, Vec<u8>)> = {
        let Ok(mut guard) = TILE_STATE.lock() else {
            return;
        };
        let state = tile_state(&*host, &mut guard);
        let wanted: Vec<TileId> = (-VIEW_TILES..=VIEW_TILES)
            .flat_map(|dx| (-VIEW_TILES..=VIEW_TILES).map(move |dz| (cx + dx, cz + dz)))
            .collect();
        load_tiles(&*host, state, &wanted);
        wanted
            .into_iter()
            .filter_map(|id| state.tiles.get(id).map(|t| (id, t.clone())))
            .collect()
    };
    let entries = STORE
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default();
    for message in wire::pack_tiles(token, &tile_list) {
        let _ = host.net_send_to_player(player.uuid, wire::TILES_CHANNEL, message.as_bytes());
    }
    let fields = wire::pack_fields(token, &entries, px, pz);
    let _ = host.net_send_to_player(player.uuid, wire::FIELDS_CHANNEL, fields.as_bytes());
    let open = wire::pack_open(token, player.position[0], player.position[2]);
    let _ = host.net_send_to_player(player.uuid, wire::OPEN_CHANNEL, open.as_bytes());
}
