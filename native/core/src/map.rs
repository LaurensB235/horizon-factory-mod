//! The field database behind the web map, game independent. The server scans around each online player now and then
//! and merges what it finds here, so the map slowly covers everything players have walked past. A scan REPLACES what
//! the store held inside the scanned square, which is how depleted fields disappear from the map.
//!
//! Stored as text, one field per line: `resource,x,z,richest,seen` (`seen` is the server tick of the scan). The web
//! page reads the same text, so the format must stay simple.
use crate::drill::Resource;
use crate::scan;

/// The most fields kept; when more arrive the least recently seen go first.
pub const MAX_ENTRIES: usize = 4000;
/// Two fields of one resource closer than this (blocks, per axis) are the same field seen twice.
pub const SAME_FIELD: i32 = 20;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub resource: Resource,
    pub x: i32,
    pub z: i32,
    pub richest: u8,
    pub seen: u64,
    /// Ore units left in the field (see `units_for_level`); 0 when not counted.
    pub units: u32,
}

/// Ore units a patch block of richness `level` still holds: the one rule lives in `drill` (the drill screen counts the
/// same way), so the map and the screen always agree.
pub use crate::drill::units_for_level;

/// The square (both corners included, in blocks) to read at full resolution to count a field: its centre plus a margin
/// that grows with its size in the coarse picture.
pub fn field_box(field: &scan::Field, origin_x: i32, origin_z: i32) -> Area {
    let (cx, cz) = scan::field_world(field, origin_x, origin_z);
    let half = ((field.pixels as f32).sqrt().ceil() as i32 + 3) * scan::BLOCKS_PER_PIXEL;
    Area {
        min_x: cx - half,
        min_z: cz - half,
        max_x: cx + half,
        max_z: cz + half,
    }
}

/// A square of the world, both corners included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub min_x: i32,
    pub min_z: i32,
    pub max_x: i32,
    pub max_z: i32,
}

impl Area {
    pub fn contains(&self, x: i32, z: i32) -> bool {
        (self.min_x..=self.max_x).contains(&x) && (self.min_z..=self.max_z).contains(&z)
    }
}

/// Replace what the store holds inside `area` with `found`, then drop duplicates across the area's edge and cap the size.
pub fn merge(store: &mut Vec<Entry>, area: Area, found: Vec<Entry>) {
    store.retain(|e| !area.contains(e.x, e.z));
    for new in found {
        // a field cut by the edge of an earlier scan shows up twice: keep the newer, richer sighting
        if let Some(old) = store.iter_mut().find(|e| {
            e.resource == new.resource
                && (e.x - new.x).abs() <= SAME_FIELD
                && (e.z - new.z).abs() <= SAME_FIELD
        }) {
            if new.seen >= old.seen {
                *old = new;
            }
        } else {
            store.push(new);
        }
    }
    if store.len() > MAX_ENTRIES {
        store.sort_by_key(|e| std::cmp::Reverse(e.seen));
        store.truncate(MAX_ENTRIES);
    }
}

/// Every field in a scanned picture as an entry, with `seen` as the scan's tick.
pub fn entries_from(
    fields: &[scan::Field],
    origin_x: i32,
    origin_z: i32,
    seen: u64,
    units: &[u32],
) -> Vec<Entry> {
    fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let (x, z) = scan::field_world(f, origin_x, origin_z);
            Entry {
                resource: f.resource,
                x,
                z,
                richest: f.richest,
                seen,
                units: units.get(i).copied().unwrap_or(0),
            }
        })
        .collect()
}

/// The square a picture with its north-west pixel at the origin covers, in world blocks.
pub fn picture_area(origin_x: i32, origin_z: i32) -> Area {
    let size = scan::MAP_SIZE as i32 * scan::BLOCKS_PER_PIXEL;
    Area {
        min_x: origin_x,
        min_z: origin_z,
        max_x: origin_x + size - 1,
        max_z: origin_z + size - 1,
    }
}

pub fn encode(entries: &[Entry]) -> String {
    let mut out = String::new();
    for e in entries {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            scan::resource_name(e.resource),
            e.x,
            e.z,
            e.richest,
            e.seen,
            e.units
        ));
    }
    out
}

/// Read the stored text; damaged lines are skipped, so a hand-edited or half-written file never stops the mod.
pub fn decode(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| {
            let mut p = line.split(',');
            let resource = scan::parse_resource(p.next()?.trim())?;
            let x = p.next()?.trim().parse().ok()?;
            let z = p.next()?.trim().parse().ok()?;
            let richest = p.next()?.trim().parse::<u8>().ok()?;
            let seen = p.next()?.trim().parse().ok()?;
            // the sixth column (units left) is missing in files written before it existed
            let units = match p.next() {
                Some(u) => u.trim().parse().ok()?,
                None => 0,
            };
            if p.next().is_some() || !(1..=8).contains(&richest) {
                return None;
            }
            Some(Entry {
                resource,
                x,
                z,
                richest,
                seen,
                units,
            })
        })
        .take(MAX_ENTRIES)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(resource: Resource, x: i32, z: i32, richest: u8, seen: u64) -> Entry {
        Entry {
            resource,
            x,
            z,
            richest,
            seen,
            units: 0,
        }
    }

    #[test]
    fn a_scan_replaces_what_the_store_held_inside_its_square() {
        let area = Area {
            min_x: 0,
            min_z: 0,
            max_x: 255,
            max_z: 255,
        };
        let mut store = vec![
            e(Resource::Iron, 100, 100, 8, 1), // inside, gone now (depleted)
            e(Resource::Coal, 500, 500, 3, 1), // outside, kept
        ];
        merge(&mut store, area, vec![e(Resource::Copper, 10, 10, 2, 5)]);
        assert_eq!(
            store,
            vec![
                e(Resource::Coal, 500, 500, 3, 1),
                e(Resource::Copper, 10, 10, 2, 5)
            ]
        );
    }

    #[test]
    fn a_field_seen_on_both_sides_of_a_scan_edge_is_kept_once() {
        let first = Area {
            min_x: 0,
            min_z: 0,
            max_x: 255,
            max_z: 255,
        };
        let second = Area {
            min_x: 256,
            min_z: 0,
            max_x: 511,
            max_z: 255,
        };
        let mut store = Vec::new();
        merge(&mut store, first, vec![e(Resource::Iron, 250, 50, 4, 1)]);
        merge(&mut store, second, vec![e(Resource::Iron, 262, 55, 8, 2)]);
        assert_eq!(store, vec![e(Resource::Iron, 262, 55, 8, 2)]);
        // another resource at the same place is a different field
        merge(
            &mut store,
            second,
            vec![
                e(Resource::Iron, 262, 55, 8, 3),
                e(Resource::Coal, 262, 55, 2, 3),
            ],
        );
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn the_store_is_capped_and_keeps_the_most_recent() {
        let area = Area {
            min_x: 0,
            min_z: 0,
            max_x: 10,
            max_z: 10,
        };
        let mut store: Vec<Entry> = (0..MAX_ENTRIES as i32)
            .map(|i| e(Resource::Coal, 1000 + i * 100, 0, 1, 1))
            .collect();
        merge(&mut store, area, vec![e(Resource::Iron, 5, 5, 8, 99)]);
        assert_eq!(store.len(), MAX_ENTRIES);
        assert!(store.iter().any(|x| x.seen == 99));
    }

    #[test]
    fn the_text_format_round_trips_and_skips_damage() {
        let entries = vec![
            e(Resource::Iron, -42, -70, 8, 1200),
            e(Resource::Coal, 5, 300, 1, 7),
        ];
        assert_eq!(decode(&encode(&entries)), entries);
        assert_eq!(
            decode(
                "iron,1,2,3,4\njunk\ncoal,1,2,9,4\ncoal,1,2,3\ncopper,5,6,7,8,9,10\n\ncopper,-1,-2,2,3"
            ),
            vec![
                e(Resource::Iron, 1, 2, 3, 4),
                e(Resource::Copper, -1, -2, 2, 3)
            ]
        );
        assert!(decode("").is_empty());
    }

    #[test]
    fn a_picture_covers_256_blocks() {
        let a = picture_area(-128, -128);
        assert!(a.contains(-128, -128) && a.contains(127, 127));
        assert!(!a.contains(128, 0) && !a.contains(0, -129));
    }
}
