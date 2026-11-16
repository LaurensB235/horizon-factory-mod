//! The web map's terrain, game independent. Every scan gives a 128 x 128 picture (one pixel per 2 x 2 blocks); the map
//! keeps the world as tiles of 64 x 64 pixels (128 x 128 blocks) aligned to multiples of 128, so each scan updates at
//! most nine small files and an old area is never rewritten when a player walks elsewhere.
//!
//! A tile is stored as `TILE * TILE` characters (the terrain classes, row by row from the north-west corner). Pixels of
//! class 0 ("unknown") never overwrite what is already known.
use crate::scan::BLOCKS_PER_PIXEL;
use crate::terrain::{self, Picture};
use std::collections::HashMap;

/// Pixels along one side of a tile.
pub const TILE: usize = 64;
/// Blocks along one side of a tile.
pub const TILE_BLOCKS: i32 = TILE as i32 * BLOCKS_PER_PIXEL;

/// A tile's coordinates: its north-west corner in blocks divided by `TILE_BLOCKS`.
pub type TileId = (i32, i32);

#[derive(Default)]
pub struct Tiles {
    tiles: HashMap<TileId, Vec<u8>>,
}

impl Tiles {
    pub fn new() -> Self {
        Self::default()
    }

    /// Put a tile in (for example one read back from a file).
    pub fn insert(&mut self, id: TileId, classes: Vec<u8>) {
        if classes.len() == TILE * TILE {
            self.tiles.insert(id, classes);
        }
    }

    pub fn get(&self, id: TileId) -> Option<&Vec<u8>> {
        self.tiles.get(&id)
    }

    /// The tiles a picture touches, so the caller can load them before `apply`.
    pub fn touched(picture: &Picture) -> Vec<TileId> {
        let mut out: Vec<TileId> = Vec::new();
        let span = terrain::SIZE as i32 * BLOCKS_PER_PIXEL - 1;
        for wx in [picture.origin_x, picture.origin_x + span] {
            for wz in [picture.origin_z, picture.origin_z + span] {
                out.push((wx.div_euclid(TILE_BLOCKS), wz.div_euclid(TILE_BLOCKS)));
            }
        }
        let (x0, x1) = (
            out.iter().map(|t| t.0).min().unwrap_or(0),
            out.iter().map(|t| t.0).max().unwrap_or(0),
        );
        let (z0, z1) = (
            out.iter().map(|t| t.1).min().unwrap_or(0),
            out.iter().map(|t| t.1).max().unwrap_or(0),
        );
        (x0..=x1)
            .flat_map(|x| (z0..=z1).map(move |z| (x, z)))
            .collect()
    }

    /// Write a picture into the tiles (creating them) and return the tiles whose content changed.
    pub fn apply(&mut self, picture: &Picture) -> Vec<TileId> {
        let mut changed: Vec<TileId> = Vec::new();
        for py in 0..terrain::SIZE {
            for px in 0..terrain::SIZE {
                let class = picture.classes[py * terrain::SIZE + px];
                if class == 0 {
                    continue;
                }
                let wx = picture.origin_x + px as i32 * BLOCKS_PER_PIXEL;
                let wz = picture.origin_z + py as i32 * BLOCKS_PER_PIXEL;
                let id = (wx.div_euclid(TILE_BLOCKS), wz.div_euclid(TILE_BLOCKS));
                let lx = (wx.rem_euclid(TILE_BLOCKS) / BLOCKS_PER_PIXEL) as usize;
                let lz = (wz.rem_euclid(TILE_BLOCKS) / BLOCKS_PER_PIXEL) as usize;
                let tile = self.tiles.entry(id).or_insert_with(|| vec![0; TILE * TILE]);
                let cell = &mut tile[lz * TILE + lx];
                if *cell != class {
                    *cell = class;
                    if !changed.contains(&id) {
                        changed.push(id);
                    }
                }
            }
        }
        changed
    }
}

/// The file text of a tile: one character per pixel (see `terrain`).
pub fn encode_tile(classes: &[u8]) -> String {
    classes.iter().map(|&c| terrain::class_char(c)).collect()
}

/// Read a tile back; `None` unless it is exactly `TILE * TILE` valid characters.
pub fn decode_tile(text: &str) -> Option<Vec<u8>> {
    let classes: Option<Vec<u8>> = text.trim_end().chars().map(terrain::char_class).collect();
    classes.filter(|c| c.len() == TILE * TILE)
}

/// Merge equal neighbours in each row of a tile into runs (column, row, length, class), skipping unknown pixels.
pub fn tile_runs(classes: &[u8]) -> Vec<terrain::Run> {
    let mut out = Vec::new();
    for y in 0..TILE {
        let row = &classes[y * TILE..(y + 1) * TILE];
        let mut x = 0;
        while x < TILE {
            let class = row[x];
            let mut end = x + 1;
            while end < TILE && row[end] == class {
                end += 1;
            }
            if class != 0 {
                out.push(terrain::Run {
                    x: x as u16,
                    y: y as u16,
                    len: (end - x) as u16,
                    class,
                });
            }
            x = end;
        }
    }
    out
}

/// The average colour (0xRRGGBB) of the known pixels of a tile, for drawing a zoomed-out tile as one rectangle;
/// `None` when nothing in it is known.
pub fn tile_average(classes: &[u8]) -> Option<u32> {
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for &c in classes.iter().filter(|&&c| c != 0) {
        let rgb = terrain::colour(c);
        r += u64::from((rgb >> 16) & 255);
        g += u64::from((rgb >> 8) & 255);
        b += u64::from(rgb & 255);
        n += 1;
    }
    (n > 0).then(|| (((r / n) as u32) << 16) | (((g / n) as u32) << 8) | (b / n) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(origin_x: i32, origin_z: i32, class: u8) -> Picture {
        Picture {
            origin_x,
            origin_z,
            classes: vec![class; terrain::SIZE * terrain::SIZE],
        }
    }

    #[test]
    fn a_picture_aligned_to_the_grid_fills_four_tiles() {
        let mut t = Tiles::new();
        let changed = t.apply(&picture(0, 0, 1));
        assert_eq!(changed.len(), 4);
        for id in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert!(t.get(id).unwrap().iter().all(|&c| c == 1), "{id:?}");
        }
        assert!(t.get((2, 0)).is_none());
    }

    #[test]
    fn an_unaligned_picture_touches_nine_tiles_and_negative_coordinates_work() {
        let p = picture(-100, -60, 3);
        assert_eq!(Tiles::touched(&p).len(), 9);
        let mut t = Tiles::new();
        let changed = t.apply(&p);
        assert_eq!(changed.len(), 9);
        // block (-100, -60) is in tile (-1, -1), local pixel ((-100 mod 128)/2, (-60 mod 128)/2) = (14, 34)
        let tile = t.get((-1, -1)).unwrap();
        assert_eq!(tile[34 * TILE + 14], 3);
        assert_eq!(tile[0], 0, "pixels the picture did not reach stay unknown");
    }

    #[test]
    fn unknown_never_overwrites_and_an_unchanged_scan_reports_nothing() {
        let mut t = Tiles::new();
        t.apply(&picture(0, 0, 2));
        let mut p = picture(0, 0, 0);
        p.classes[0] = 5;
        assert_eq!(t.apply(&p), vec![(0, 0)]);
        assert_eq!(t.get((0, 0)).unwrap()[0], 5);
        assert_eq!(
            t.get((0, 0)).unwrap()[1],
            2,
            "an unknown pixel keeps what was known"
        );
        assert!(t.apply(&p).is_empty());
    }

    #[test]
    fn runs_and_the_average_describe_a_tile() {
        let mut classes = vec![0u8; TILE * TILE];
        classes[0] = 1;
        classes[1] = 1;
        classes[2] = 7;
        let runs = tile_runs(&classes);
        assert_eq!(runs.len(), 2);
        assert_eq!((runs[0].x, runs[0].len, runs[0].class), (0, 2, 1));
        assert!(tile_average(&vec![0; TILE * TILE]).is_none());
        let avg = tile_average(&classes).unwrap();
        assert_eq!(
            avg & 0xFF,
            ((terrain::colour(1) & 255) * 2 + (terrain::colour(7) & 255)) / 3
        );
    }

    #[test]
    fn tiles_round_trip_as_text_and_bad_text_is_refused() {
        let mut classes = vec![1u8; TILE * TILE];
        classes[7] = 7;
        let text = encode_tile(&classes);
        assert_eq!(text.len(), TILE * TILE);
        assert_eq!(decode_tile(&text), Some(classes));
        assert!(decode_tile("111").is_none());
        assert!(decode_tile(&"Z".repeat(TILE * TILE)).is_none());
    }
}
