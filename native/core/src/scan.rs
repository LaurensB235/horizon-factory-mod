//! Finding fields and drawing them: the game-independent half of the resource scanner (and of the compass later).
//!
//! The scanner samples one world column per map pixel (2 blocks per pixel, 128 x 128 pixels, so the picture is 256
//! blocks wide, 128 each way from the player). Each sample is "no patch" or "a patch of this resource at this
//! richness". From the samples this module draws the 16384 map colour bytes and finds and describes the fields.

use crate::drill::{MAX_LEVEL, Resource, parse_richness};

/// Map width and height in pixels (a vanilla map is 128 x 128).
pub const MAP_SIZE: usize = 128;
/// World blocks per map pixel, in both directions: a picture 256 blocks wide.
pub const BLOCKS_PER_PIXEL: i32 = 2;

/// What one sampled column holds: nothing, or a patch of a resource with its richness level (1 to 8).
pub type Sample = Option<(Resource, u8)>;

/// Read a block state as the world reading names it (`horizon_factory:iron_patch[richness=level_5]`) back to a
/// resource and a richness. `None` for any other block. A missing or unreadable richness counts as level 1.
pub fn parse_patch_state(state: &str) -> Option<(Resource, u8)> {
    let (name, props) = match state.split_once('[') {
        Some((name, rest)) => (name, rest.trim_end_matches(']')),
        None => (state, ""),
    };
    let resource = match name {
        "horizon_factory:coal_patch" => Resource::Coal,
        "horizon_factory:iron_patch" => Resource::Iron,
        "horizon_factory:copper_patch" => Resource::Copper,
        _ => return None,
    };
    let level = props
        .split(',')
        .find_map(|kv| kv.strip_prefix("richness="))
        .and_then(parse_richness)
        .unwrap_or(1);
    Some((resource, level))
}

// ----------------------------------------------------------------------------------------------- colours

/// Vanilla `MapColor` ids used for the picture.
const COLOR_GRAY: u8 = 21;
const COLOR_BLACK: u8 = 29;
const COLOR_ORANGE: u8 = 15;
const COLOR_CYAN: u8 = 23;
const SNOW: u8 = 8;

/// Colour byte = `MapColor` id times 4 plus the shade (0 darkest, 1 normal, 2 brightest).
const fn byte(id: u8, shade: u8) -> u8 {
    id * 4 + shade
}

/// The map background: where there is no patch.
pub const BACKGROUND: u8 = byte(COLOR_GRAY, 0);
/// The player marker (a small white cross in the middle).
pub const PLAYER: u8 = byte(SNOW, 2);

/// The colour byte for a patch: coal black, iron orange, copper cyan. Richer fields are brighter.
pub fn patch_colour(resource: Resource, level: u8) -> u8 {
    let id = match resource {
        Resource::Coal => COLOR_BLACK,
        Resource::Iron => COLOR_ORANGE,
        Resource::Copper => COLOR_CYAN,
    };
    let shade = if level >= 6 {
        2
    } else if level >= 3 {
        1
    } else {
        0
    };
    byte(id, shade)
}

/// Draw the picture: `samples` are `MAP_SIZE * MAP_SIZE` columns, row by row (`index = row * MAP_SIZE + column`, rows
/// run north to south, columns west to east). The result is the 16384 colour bytes a map wants, with a small cross
/// marking the player in the middle. Missing samples (a slice that is too short) draw as background.
pub fn render_map(samples: &[Sample]) -> Vec<u8> {
    let mut out = vec![BACKGROUND; MAP_SIZE * MAP_SIZE];
    for (i, px) in out.iter_mut().enumerate() {
        if let Some(Some((resource, level))) = samples.get(i) {
            *px = patch_colour(*resource, (*level).min(MAX_LEVEL));
        }
    }
    let mid = MAP_SIZE / 2;
    for (dx, dz) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
        let x = (mid as i32 + dx) as usize;
        let z = (mid as i32 + dz) as usize;
        out[z * MAP_SIZE + x] = PLAYER;
    }
    out
}

// -------------------------------------------------------------------------------------------------- fields

/// A connected patch of one resource in the picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub resource: Resource,
    /// How many sampled columns it covers.
    pub pixels: usize,
    /// Centre in pixel coordinates (column, row).
    pub centre: (f32, f32),
    /// The richest level seen in it.
    pub richest: u8,
}

/// Group neighbouring samples of the same resource (8-way connected) into fields. Fields come out in a fixed order:
/// by their first pixel, row by row.
pub fn find_fields(samples: &[Sample]) -> Vec<Field> {
    let n = MAP_SIZE;
    let mut seen = vec![false; n * n];
    let mut fields = Vec::new();
    for start in 0..n * n {
        let Some(Some((resource, _))) = samples.get(start) else {
            continue;
        };
        if seen[start] {
            continue;
        }
        let resource = *resource;
        let (mut count, mut sx, mut sz, mut richest) = (0usize, 0f64, 0f64, 0u8);
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            let (x, z) = ((i % n) as i32, (i / n) as i32);
            if let Some(Some((_, level))) = samples.get(i) {
                richest = richest.max(*level);
            }
            count += 1;
            sx += f64::from(x);
            sz += f64::from(z);
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let (nx, nz) = (x + dx, z + dz);
                    if nx < 0 || nz < 0 || nx >= n as i32 || nz >= n as i32 {
                        continue;
                    }
                    let j = nz as usize * n + nx as usize;
                    if seen[j] {
                        continue;
                    }
                    if matches!(samples.get(j), Some(Some((r, _))) if *r == resource) {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        fields.push(Field {
            resource,
            pixels: count,
            centre: ((sx / count as f64) as f32, (sz / count as f64) as f32),
            richest,
        });
    }
    fields
}

/// Distance in blocks and the compass direction from the picture middle (the player) to `field`.
pub fn locate(field: &Field) -> (f32, &'static str) {
    let mid = MAP_SIZE as f32 / 2.0;
    let dx = (field.centre.0 - mid) * BLOCKS_PER_PIXEL as f32;
    let dz = (field.centre.1 - mid) * BLOCKS_PER_PIXEL as f32;
    ((dx * dx + dz * dz).sqrt(), compass_name(dx, dz))
}

/// Eight-point compass name for an offset in world blocks (north is negative z, east is positive x).
pub fn compass_name(dx: f32, dz: f32) -> &'static str {
    if dx == 0.0 && dz == 0.0 {
        return "here";
    }
    // 0 degrees = north, clockwise
    let deg = dx.atan2(-dz).to_degrees().rem_euclid(360.0);
    const NAMES: [&str; 8] = [
        "north",
        "north-east",
        "east",
        "south-east",
        "south",
        "south-west",
        "west",
        "north-west",
    ];
    NAMES[(((deg + 22.5) / 45.0) as usize) % 8]
}

/// The nearest field of each resource, as one line each (`iron: 54 blocks north-west, richest level 8`), or a line
/// saying none was found. Always three lines, in the order coal, iron, copper.
pub fn describe_nearest(fields: &[Field]) -> Vec<String> {
    [Resource::Coal, Resource::Iron, Resource::Copper]
        .into_iter()
        .map(|resource| {
            let best = fields
                .iter()
                .filter(|f| f.resource == resource)
                .map(|f| (locate(f), f))
                .min_by(|a, b| a.0.0.total_cmp(&b.0.0));
            let name = resource_name(resource);
            match best {
                Some(((dist, dir), f)) => format!(
                    "{name}: {} blocks {dir}, richest level {}",
                    dist.round() as i32,
                    f.richest
                ),
                None => format!("{name}: none in range"),
            }
        })
        .collect()
}

pub fn resource_name(resource: Resource) -> &'static str {
    match resource {
        Resource::Coal => "coal",
        Resource::Iron => "iron",
        Resource::Copper => "copper",
    }
}

/// Read a resource name back (`iron`); `None` for anything else.
pub fn parse_resource(name: &str) -> Option<Resource> {
    match name {
        "coal" => Some(Resource::Coal),
        "iron" => Some(Resource::Iron),
        "copper" => Some(Resource::Copper),
        _ => None,
    }
}

/// The nearest field of `resource` to the picture middle (the player), by centre distance.
pub fn nearest_of(fields: &[Field], resource: Resource) -> Option<&Field> {
    fields
        .iter()
        .filter(|f| f.resource == resource)
        .min_by(|a, b| locate(a).0.total_cmp(&locate(b).0))
}

/// World block coordinates (x, z) of a field centre, for a picture whose north-west pixel is at
/// (`origin_x`, `origin_z`).
pub fn field_world(field: &Field, origin_x: i32, origin_z: i32) -> (i32, i32) {
    (
        origin_x + (field.centre.0 * BLOCKS_PER_PIXEL as f32).round() as i32,
        origin_z + (field.centre.1 * BLOCKS_PER_PIXEL as f32).round() as i32,
    )
}

/// The chat line for the compass: where the nearest field of `resource` is, or that there is none.
pub fn compass_line(resource: Resource, field: Option<&Field>) -> String {
    let name = resource_name(resource);
    match field {
        Some(f) => {
            let (dist, dir) = locate(f);
            format!(
                "Compass: {name} field {} blocks {dir}, richest level {}",
                dist.round() as i32,
                f.richest
            )
        }
        None => format!("Compass: no {name} field in range"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> Vec<Sample> {
        vec![None; MAP_SIZE * MAP_SIZE]
    }
    fn put(samples: &mut [Sample], x: usize, z: usize, r: Resource, level: u8) {
        samples[z * MAP_SIZE + x] = Some((r, level));
    }

    #[test]
    fn patch_states_parse() {
        assert_eq!(
            parse_patch_state("horizon_factory:iron_patch[richness=level_5]"),
            Some((Resource::Iron, 5))
        );
        assert_eq!(
            parse_patch_state("horizon_factory:coal_patch"),
            Some((Resource::Coal, 1))
        );
        assert_eq!(
            parse_patch_state("horizon_factory:copper_patch[richness=level_8]"),
            Some((Resource::Copper, 8))
        );
        assert_eq!(
            parse_patch_state("horizon_factory:iron_patch[richness=level_x]"),
            Some((Resource::Iron, 1)),
            "an unreadable richness counts as 1"
        );
        for other in [
            "minecraft:stone",
            "horizon_factory:drill[lit=true]",
            "",
            "iron_patch",
        ] {
            assert_eq!(parse_patch_state(other), None, "{other:?}");
        }
    }

    #[test]
    fn the_map_is_16384_bytes_with_a_player_cross() {
        let map = render_map(&blank());
        assert_eq!(map.len(), 16384);
        let mid = MAP_SIZE / 2;
        for (x, z) in [
            (mid, mid),
            (mid + 1, mid),
            (mid - 1, mid),
            (mid, mid + 1),
            (mid, mid - 1),
        ] {
            assert_eq!(map[z * MAP_SIZE + x], PLAYER);
        }
        assert_eq!(map[0], BACKGROUND);
        assert_eq!(map.iter().filter(|&&b| b == PLAYER).count(), 5);
    }

    #[test]
    fn resources_have_their_own_colours_and_richness_is_brightness() {
        let mut s = blank();
        put(&mut s, 3, 3, Resource::Coal, 8);
        put(&mut s, 5, 3, Resource::Iron, 8);
        put(&mut s, 7, 3, Resource::Copper, 8);
        put(&mut s, 9, 3, Resource::Iron, 4);
        put(&mut s, 11, 3, Resource::Iron, 1);
        let map = render_map(&s);
        let at = |x: usize| map[3 * MAP_SIZE + x];
        assert_ne!(at(3), at(5));
        assert_ne!(at(5), at(7));
        assert_ne!(at(3), at(7));
        // the same resource: brighter shade for richer, and all different from the background
        assert!(at(5) % 4 > at(9) % 4 && at(9) % 4 > at(11) % 4);
        assert_eq!(at(5) / 4, at(9) / 4, "same MapColor id for iron");
        assert!(
            [at(3), at(5), at(7), at(9), at(11)]
                .iter()
                .all(|&b| b != BACKGROUND)
        );
    }

    #[test]
    fn a_short_sample_slice_draws_background() {
        let map = render_map(&[Some((Resource::Iron, 8))]);
        assert_eq!(map.len(), 16384);
        assert_ne!(map[0], BACKGROUND);
        assert_eq!(map[1], BACKGROUND);
    }

    #[test]
    fn fields_are_found_and_separated() {
        let mut s = blank();
        // iron blob of 2x2 at the top left, a diagonal pair of coal, a far copper pixel
        for (x, z) in [(2, 2), (3, 2), (2, 3), (3, 3)] {
            put(&mut s, x, z, Resource::Iron, 5);
        }
        put(&mut s, 3, 3, Resource::Iron, 8);
        put(&mut s, 20, 20, Resource::Coal, 2);
        put(&mut s, 21, 21, Resource::Coal, 3); // diagonal: connected
        put(&mut s, 100, 10, Resource::Copper, 1);
        // two iron pixels next to each other but a different resource between must not join
        put(&mut s, 50, 50, Resource::Iron, 1);
        put(&mut s, 51, 50, Resource::Coal, 1);
        put(&mut s, 52, 50, Resource::Iron, 1);
        let f = find_fields(&s);
        assert_eq!(f.len(), 6);
        let iron = &f[0];
        assert_eq!(
            (iron.resource, iron.pixels, iron.richest),
            (Resource::Iron, 4, 8)
        );
        assert_eq!(iron.centre, (2.5, 2.5));
        assert!(
            f.iter()
                .any(|x| x.resource == Resource::Coal && x.pixels == 2)
        );
        assert_eq!(f.iter().filter(|x| x.resource == Resource::Iron).count(), 3);
        assert_eq!(find_fields(&s), f, "deterministic");
    }

    #[test]
    fn compass_names_cover_the_eight_directions() {
        assert_eq!(compass_name(0.0, -10.0), "north");
        assert_eq!(compass_name(10.0, -10.0), "north-east");
        assert_eq!(compass_name(10.0, 0.0), "east");
        assert_eq!(compass_name(10.0, 10.0), "south-east");
        assert_eq!(compass_name(0.0, 10.0), "south");
        assert_eq!(compass_name(-10.0, 10.0), "south-west");
        assert_eq!(compass_name(-10.0, 0.0), "west");
        assert_eq!(compass_name(-10.0, -10.0), "north-west");
        assert_eq!(compass_name(0.0, 0.0), "here");
    }

    #[test]
    fn nearest_fields_are_described_per_resource() {
        let mut s = blank();
        let mid = MAP_SIZE / 2;
        // iron 10 pixels north of the player (20 blocks), another iron far east; coal to the south-east; no copper
        put(&mut s, mid, mid - 10, Resource::Iron, 7);
        put(&mut s, mid + 40, mid, Resource::Iron, 8);
        put(&mut s, mid + 5, mid + 5, Resource::Coal, 3);
        let lines = describe_nearest(&find_fields(&s));
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "coal: 14 blocks south-east, richest level 3");
        assert_eq!(lines[1], "iron: 20 blocks north, richest level 7");
        assert_eq!(lines[2], "copper: none in range");
    }

    #[test]
    fn locate_scales_pixels_to_blocks() {
        let f = Field {
            resource: Resource::Coal,
            pixels: 1,
            centre: (64.0, 64.0 - 32.0),
            richest: 1,
        };
        let (dist, dir) = locate(&f);
        assert_eq!(dist, 64.0);
        assert_eq!(dir, "north");
    }

    #[test]
    fn resource_names_round_trip() {
        for r in [Resource::Coal, Resource::Iron, Resource::Copper] {
            assert_eq!(parse_resource(resource_name(r)), Some(r));
        }
        assert_eq!(parse_resource("gold"), None);
        assert_eq!(parse_resource(""), None);
    }

    #[test]
    fn the_nearest_field_of_a_resource_is_chosen() {
        let mut s = blank();
        let mid = MAP_SIZE / 2;
        put(&mut s, mid + 30, mid, Resource::Iron, 4); // 60 blocks east
        put(&mut s, mid - 5, mid, Resource::Iron, 2); // 10 blocks west
        put(&mut s, mid, mid + 2, Resource::Coal, 8);
        let fields = find_fields(&s);
        let near = nearest_of(&fields, Resource::Iron).unwrap();
        assert_eq!(near.richest, 2);
        assert_eq!(locate(near).1, "west");
        assert!(nearest_of(&fields, Resource::Copper).is_none());
    }

    #[test]
    fn a_field_centre_becomes_world_coordinates() {
        let f = Field {
            resource: Resource::Iron,
            pixels: 1,
            centre: (64.0, 70.0),
            richest: 1,
        };
        // the picture starts at world (-100, 200); pixel 64 is 128 blocks across, pixel 70 is 140 blocks down
        assert_eq!(field_world(&f, -100, 200), (28, 340));
    }

    #[test]
    fn the_compass_line_names_the_resource_and_the_direction() {
        let f = Field {
            resource: Resource::Copper,
            pixels: 3,
            centre: (64.0 + 25.0, 64.0),
            richest: 6,
        };
        assert_eq!(
            compass_line(Resource::Copper, Some(&f)),
            "Compass: copper field 50 blocks east, richest level 6"
        );
        assert_eq!(
            compass_line(Resource::Iron, None),
            "Compass: no iron field in range"
        );
    }
}
