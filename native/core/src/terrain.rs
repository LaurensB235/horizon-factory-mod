//! Terrain colours for the scanner maps, game independent. The server reads the top block of each sampled column and
//! turns its name into a small class number; a class has one map colour. The classes travel as one character each, and
//! the client merges equal neighbours in a row into runs so a 128 x 128 picture is a few thousand rectangles.
//!
//! Wire format (channel `factory:terrain`): the first line `origin_x,origin_z,size`, the second line `size * size`
//! characters, row by row from the north-west pixel. The client treats it as untrusted bytes: a wrong length or size
//! gives nothing.

/// Pixels along one side of a picture (one pixel is `scan::BLOCKS_PER_PIXEL` blocks).
pub const SIZE: usize = 128;
/// The classes: indices into `COLOURS`; 0 is "nothing known".
pub const CLASS_COUNT: u8 = 21;

const UNKNOWN: u8 = 0;
const GRASS: u8 = 1;
const DIRT: u8 = 2;
const STONE: u8 = 3;
const SAND: u8 = 4;
const RED_SAND: u8 = 5;
const GRAVEL: u8 = 6;
const WATER: u8 = 7;
const SNOW: u8 = 8;
const ICE: u8 = 9;
const LEAVES: u8 = 10;
const WOOD: u8 = 11;
const LAVA: u8 = 12;
const NETHER: u8 = 13;
const END: u8 = 14;
const CLAY: u8 = 15;
const MOSS: u8 = 16;
const DEEPSLATE: u8 = 17;
const MUD: u8 = 18;
const PLANT: u8 = 19;
const BUILT: u8 = 20;

/// Map colours (0xRRGGBB), by class: the look of a paper map, not the game's own map palette.
const COLOURS: [u32; CLASS_COUNT as usize] = [
    0x20_22_26, // unknown
    0x5C_9A_3C, // grass
    0x86_5F_3E, // dirt
    0x7D_7F_83, // stone
    0xE0_D3_9A, // sand
    0xC2_6A_3A, // red sand and terracotta
    0x8E_8A_86, // gravel
    0x3A_6F_D4, // water
    0xF2_F5_F8, // snow
    0xA5_D3_F0, // ice
    0x2F_6B_2A, // leaves
    0x6B_4B_2A, // wood
    0xE8_6A_16, // lava
    0x7A_2F_32, // nether
    0xDD_DA_A8, // end stone
    0x9D_A4_B4, // clay
    0x4C_7A_3A, // moss
    0x4A_4A_52, // deepslate
    0x4B_3B_32, // mud
    0x78_B0_4A, // plants (flowers, tall grass, crops)
    0xB4_8D_5E, // buildings (planks, bricks, glass and the like)
];

/// The class of a block, by its name (`minecraft:grass_block[snowy=false]` or `grass_block`).
pub fn class_of(block: &str) -> u8 {
    let name = block.strip_prefix("minecraft:").unwrap_or(block);
    let name = name.split('[').next().unwrap_or(name);
    if name == "air" || name == "cave_air" || name == "void_air" {
        return UNKNOWN;
    }
    // our own patch blocks lie on the ground: they read as bare ground
    if name.starts_with("horizon_factory:") && name.ends_with("_patch") {
        return DIRT;
    }
    let has = |s: &str| name.contains(s);
    match name {
        "water" | "bubble_column" | "kelp" | "kelp_plant" | "seagrass" | "tall_seagrass" => WATER,
        "lava" => LAVA,
        "ice" | "packed_ice" | "blue_ice" | "frosted_ice" => ICE,
        "snow" | "snow_block" | "powder_snow" | "white_concrete" => SNOW,
        "grass_block" | "short_grass" | "fern" => GRASS,
        "moss_block" | "moss_carpet" | "azalea" | "flowering_azalea" => MOSS,
        "mud" | "muddy_mangrove_roots" => MUD,
        "clay" => CLAY,
        "gravel" => GRAVEL,
        "end_stone" | "end_stone_bricks" => END,
        _ if has("leaves") => LEAVES,
        _ if has("red_sand") || has("terracotta") => RED_SAND,
        _ if has("sand") => SAND,
        _ if has("deepslate") => DEEPSLATE,
        _ if has("netherrack")
            || has("nether_")
            || has("basalt")
            || has("blackstone")
            || has("soul_")
            || has("crimson")
            || has("warped") =>
        {
            NETHER
        }
        _ if has("dirt") || has("podzol") || has("mycelium") || has("farmland") || has("path") => {
            DIRT
        }
        _ if has("_log") || has("_wood") || has("stem") || has("hyphae") => WOOD,
        _ if has("planks")
            || has("brick")
            || has("glass")
            || has("slab")
            || has("stairs")
            || has("fence")
            || has("wool")
            || has("concrete")
            || has("bookshelf") =>
        {
            BUILT
        }
        _ if has("flower")
            || has("tulip")
            || has("poppy")
            || has("dandelion")
            || has("rose")
            || has("grass")
            || has("vine")
            || has("lily")
            || has("wheat")
            || has("sapling")
            || has("bush")
            || has("crop") =>
        {
            PLANT
        }
        _ if has("stone")
            || has("andesite")
            || has("diorite")
            || has("granite")
            || has("tuff")
            || has("cobble")
            || has("calcite")
            || has("dripstone")
            || has("_ore")
            || has("bedrock") =>
        {
            STONE
        }
        _ => STONE,
    }
}

/// The colour (0xRRGGBB) of a class; an unknown class number gives the "unknown" colour.
pub fn colour(class: u8) -> u32 {
    COLOURS.get(class as usize).copied().unwrap_or(COLOURS[0])
}

/// One character per class (`0`..`K`), so the picture is plain text.
pub(crate) fn class_char(class: u8) -> char {
    char::from_digit(u32::from(class), 36)
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('0')
}

pub(crate) fn char_class(c: char) -> Option<u8> {
    let v = c.to_digit(36)?;
    (v < u32::from(CLASS_COUNT)).then_some(v as u8)
}

/// The message for a picture whose north-west pixel is at (`origin_x`, `origin_z`). `classes` must hold `SIZE * SIZE`
/// entries; anything else gives an empty message.
pub fn encode(origin_x: i32, origin_z: i32, classes: &[u8]) -> String {
    if classes.len() != SIZE * SIZE {
        return String::new();
    }
    let mut out = format!("{origin_x},{origin_z},{SIZE}\n");
    out.extend(classes.iter().map(|&c| class_char(c)));
    out
}

/// A decoded picture: where its north-west pixel is and the class of each pixel.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub origin_x: i32,
    pub origin_z: i32,
    pub classes: Vec<u8>,
}

/// Read a message; `None` unless the header and the length are exactly right.
pub fn decode(text: &str) -> Option<Picture> {
    let (head, body) = text.split_once('\n')?;
    let mut p = head.split(',');
    let origin_x = p.next()?.trim().parse().ok()?;
    let origin_z = p.next()?.trim().parse().ok()?;
    let size: usize = p.next()?.trim().parse().ok()?;
    if size != SIZE || p.next().is_some() || body.chars().count() != SIZE * SIZE {
        return None;
    }
    let classes: Option<Vec<u8>> = body.chars().map(char_class).collect();
    Some(Picture {
        origin_x,
        origin_z,
        classes: classes?,
    })
}

/// A horizontal run of equal pixels: column, row, length and class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub x: u16,
    pub y: u16,
    pub len: u16,
    pub class: u8,
}

/// Merge equal neighbours in each row, so a picture is drawn as few rectangles as possible.
pub fn runs(picture: &Picture) -> Vec<Run> {
    let mut out = Vec::new();
    for y in 0..SIZE {
        let row = &picture.classes[y * SIZE..(y + 1) * SIZE];
        let mut x = 0;
        while x < SIZE {
            let class = row[x];
            let mut end = x + 1;
            while end < SIZE && row[end] == class {
                end += 1;
            }
            if class != UNKNOWN {
                out.push(Run {
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

/// Where a world position falls in a picture, in pixels (fractional), given the blocks per pixel.
pub fn to_pixel(picture: &Picture, x: f64, z: f64, blocks_per_pixel: f64) -> (f64, f64) {
    (
        (x - f64::from(picture.origin_x)) / blocks_per_pixel,
        (z - f64::from(picture.origin_z)) / blocks_per_pixel,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_get_sensible_classes() {
        assert_eq!(class_of("minecraft:grass_block[snowy=false]"), GRASS);
        assert_eq!(class_of("minecraft:oak_leaves[distance=1]"), LEAVES);
        assert_eq!(class_of("minecraft:water[level=0]"), WATER);
        assert_eq!(class_of("minecraft:red_sand"), RED_SAND);
        assert_eq!(class_of("minecraft:sandstone"), SAND);
        assert_eq!(class_of("minecraft:deepslate_iron_ore"), DEEPSLATE);
        assert_eq!(class_of("minecraft:oak_log[axis=y]"), WOOD);
        assert_eq!(
            class_of("horizon_factory:iron_patch[richness=level_3]"),
            DIRT
        );
        assert_eq!(class_of("minecraft:air"), UNKNOWN);
        assert_eq!(class_of("minecraft:diorite"), STONE);
        assert_eq!(class_of("something:unheard_of"), STONE);
    }

    #[test]
    fn every_class_has_a_colour_and_a_character() {
        for c in 0..CLASS_COUNT {
            assert_ne!(colour(c), 0, "class {c}");
            assert_eq!(char_class(class_char(c)), Some(c));
        }
        assert_eq!(colour(200), colour(0));
        assert_eq!(char_class('Z'), None);
    }

    fn picture() -> Picture {
        let mut classes = vec![GRASS; SIZE * SIZE];
        classes[5] = WATER;
        classes[SIZE + 7] = STONE;
        Picture {
            origin_x: -128,
            origin_z: 64,
            classes,
        }
    }

    #[test]
    fn a_picture_round_trips_through_the_message() {
        let p = picture();
        assert_eq!(decode(&encode(p.origin_x, p.origin_z, &p.classes)), Some(p));
    }

    #[test]
    fn decode_refuses_anything_that_is_not_exactly_a_picture() {
        let p = picture();
        let good = encode(p.origin_x, p.origin_z, &p.classes);
        assert!(decode("").is_none());
        assert!(decode("1,2,128\nabc").is_none());
        assert!(decode(&good.replace(",128\n", ",64\n")).is_none());
        assert!(decode(&good[..good.len() - 1]).is_none());
        assert!(decode(&format!("{good}0")).is_none());
        assert!(decode(&good.replacen('1', "Z", 1)).is_none());
        assert!(encode(0, 0, &[1, 2, 3]).is_empty());
    }

    #[test]
    fn runs_merge_equal_neighbours_and_skip_unknown() {
        let mut p = picture();
        p.classes[0] = UNKNOWN;
        p.classes[1] = UNKNOWN;
        let r = runs(&p);
        let first_row: Vec<_> = r.iter().filter(|r| r.y == 0).collect();
        assert_eq!(first_row.len(), 3); // grass 2..5, water 5, grass 6..
        assert_eq!(
            (first_row[0].x, first_row[0].len, first_row[0].class),
            (2, 3, GRASS)
        );
        assert_eq!(
            (first_row[1].x, first_row[1].len, first_row[1].class),
            (5, 1, WATER)
        );
        let covered: usize = r.iter().map(|r| r.len as usize).sum();
        assert_eq!(covered, SIZE * SIZE - 2);
    }

    #[test]
    fn world_positions_map_to_pixels() {
        let p = picture();
        assert_eq!(to_pixel(&p, -128.0, 64.0, 2.0), (0.0, 0.0));
        assert_eq!(to_pixel(&p, 0.0, 128.0, 2.0), (64.0, 32.0));
    }
}
