//! Sampling the world around a player for patch blocks: shared by the scanner map and the compass.
//!
//! One world column per map pixel (2 blocks per pixel, 128 x 128 pixels, so 128 blocks each way from the player),
//! read in one `Host::read_columns` call. The palette of that call holds block NAMES only, so the richness of each
//! found patch is read separately with `block_property`.
use factory_core::drill::{self, Resource};
use factory_core::scan::{self, BLOCKS_PER_PIXEL, MAP_SIZE, Sample};
use factory_core::terrain;
use horizon_client::mod_api::{Host, NativeHeightmap, NativeServerPlayer};
use std::collections::HashSet;

/// Blocks read under and from each column's top (the heightmap ignores leaves, so patches under trees are found).
const BELOW: u32 = 3;
const ABOVE: u32 = 1;

/// The samples (row by row, north to south, west to east) and the world position of the north-west pixel.
pub struct Sampled {
    pub samples: Vec<Sample>,
    /// The terrain class of each sampled column (`factory_core::terrain`), same order as `samples`.
    pub terrain: Vec<u8>,
    pub origin_x: i32,
    pub origin_z: i32,
}

/// The richness of the patch block at (x, y, z). The run's start (`bases`) is the game's idea of where it begins, so
/// the block one below and one above are tried too; `None` when none of them is a patch with a readable richness.
fn richness_at(host: &dyn Host, x: i32, y: i32, z: i32) -> Option<u8> {
    for dy in [0, -1, 1] {
        let at = [x, y + dy, z];
        if scan::parse_patch_state(&host.block(at)).is_some() {
            return host
                .block_property(at, "richness")
                .and_then(|v| drill::parse_richness(&v));
        }
    }
    None
}

/// Read the area around `player`. `Err` carries the game's reason (for example another dimension).
pub fn sample_area(host: &mut dyn Host, player: &NativeServerPlayer) -> Result<Sampled, String> {
    let half = (MAP_SIZE as i32 / 2) * BLOCKS_PER_PIXEL;
    let origin_x = player.position[0].floor() as i32 - half;
    let origin_z = player.position[2].floor() as i32 - half;

    let mut columns = Vec::with_capacity(MAP_SIZE * MAP_SIZE);
    for row in 0..MAP_SIZE as i32 {
        for col in 0..MAP_SIZE as i32 {
            columns.push((
                origin_x + col * BLOCKS_PER_PIXEL,
                origin_z + row * BLOCKS_PER_PIXEL,
            ));
        }
    }
    let read = host.read_columns(
        host.dimension(),
        &columns,
        NativeHeightmap::MotionBlockingNoLeaves,
        (BELOW, ABOVE),
    )?;

    let patches: Vec<Option<(Resource, u8)>> = read
        .palette
        .iter()
        .map(|s| scan::parse_patch_state(s))
        .collect();
    let per_column = (BELOW + ABOVE) as usize;
    let mut samples: Vec<Sample> = vec![None; columns.len()];
    let mut classes = vec![0u8; columns.len()];
    for (i, &(x, z)) in columns.iter().enumerate() {
        let Some(run) = read.blocks.get(i * per_column..(i + 1) * per_column) else {
            continue;
        };
        // the top block that is not air gives the column its map colour
        classes[i] = run
            .iter()
            .rev()
            .map(|&idx| {
                read.palette
                    .get(idx as usize)
                    .map_or(0, |n| terrain::class_of(n))
            })
            .find(|&c| c != 0)
            .unwrap_or(0);
        // the topmost patch block in this column's run (the run is read from the bottom up)
        let Some((k, (resource, _))) = run.iter().enumerate().rev().find_map(|(k, &idx)| {
            patches
                .get(idx as usize)
                .copied()
                .flatten()
                .map(|patch| (k, patch))
        }) else {
            continue;
        };
        let level = read
            .bases
            .get(i)
            .copied()
            .flatten()
            .and_then(|base| richness_at(&*host, x, base + k as i32, z))
            .unwrap_or(1);
        samples[i] = Some((resource, level));
    }
    Ok(Sampled {
        samples,
        terrain: classes,
        origin_x,
        origin_z,
    })
}

/// The ore units left in each of `fields` (same order), counted block by block: every column of the field's box is read
/// at one block resolution (the coarse picture takes one column in four), and each patch block of the field's resource
/// adds `map::units_for_level` of its richness. A column belongs to the first field whose box holds it, so fields whose
/// boxes overlap are never counted twice. Fields whose columns are not loaded count what could be read.
pub fn count_units(
    host: &mut dyn Host,
    fields: &[scan::Field],
    origin_x: i32,
    origin_z: i32,
) -> Vec<u32> {
    let per_column = (BELOW + ABOVE) as usize;
    let mut counted: HashSet<(i32, i32)> = HashSet::new();
    let mut out = vec![0u32; fields.len()];
    for (fi, field) in fields.iter().enumerate() {
        let area = factory_core::map::field_box(field, origin_x, origin_z);
        let mut columns: Vec<(i32, i32)> = Vec::new();
        for x in area.min_x..=area.max_x {
            for z in area.min_z..=area.max_z {
                if counted.insert((x, z)) {
                    columns.push((x, z));
                }
            }
        }
        if columns.is_empty() || columns.len() > 65_536 {
            continue;
        }
        let Ok(read) = host.read_columns(
            host.dimension(),
            &columns,
            NativeHeightmap::MotionBlockingNoLeaves,
            (BELOW, ABOVE),
        ) else {
            continue;
        };
        let patches: Vec<Option<(Resource, u8)>> = read
            .palette
            .iter()
            .map(|n| scan::parse_patch_state(n))
            .collect();
        for (i, &(x, z)) in columns.iter().enumerate() {
            let Some(run) = read.blocks.get(i * per_column..(i + 1) * per_column) else {
                continue;
            };
            let Some(base) = read.bases.get(i).copied().flatten() else {
                continue;
            };
            for (k, &idx) in run.iter().enumerate() {
                let Some(Some((resource, _))) = patches.get(idx as usize) else {
                    continue;
                };
                if *resource != field.resource {
                    continue;
                }
                let level = richness_at(&*host, x, base + k as i32, z).unwrap_or(1);
                out[fi] += factory_core::map::units_for_level(level);
            }
        }
    }
    out
}
