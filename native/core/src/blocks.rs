//! Which blocks a drill may be built into and on, decided by block name. The game names a block like
//! `minecraft:grass_block`, sometimes with a `[state]` suffix; unloaded space reads as air.

/// Block name without any `[state]` suffix.
pub fn base_name(name: &str) -> &str {
    name.split('[').next().unwrap_or(name)
}

/// Blocks the drill replaces when it is built (air and the small plants vanilla treats as replaceable).
const REPLACEABLE: &[&str] = &[
    "minecraft:air",
    "minecraft:cave_air",
    "minecraft:void_air",
    "minecraft:short_grass",
    "minecraft:tall_grass",
    "minecraft:fern",
    "minecraft:large_fern",
    "minecraft:dead_bush",
    "minecraft:snow",
];

/// Fluids are never ground.
const FLUIDS: &[&str] = &["minecraft:water", "minecraft:lava"];

pub fn is_replaceable(name: &str) -> bool {
    REPLACEABLE.contains(&base_name(name))
}

/// Solid enough to stand the drill on: not replaceable, not a fluid.
pub fn is_ground(name: &str) -> bool {
    let n = base_name(name);
    !REPLACEABLE.contains(&n) && !FLUIDS.contains(&n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_name_drops_the_state() {
        assert_eq!(base_name("minecraft:stone"), "minecraft:stone");
        assert_eq!(
            base_name("horizon_factory:drill[lit=true]"),
            "horizon_factory:drill"
        );
        assert_eq!(base_name(""), "");
    }

    #[test]
    fn air_and_small_plants_are_replaceable() {
        for n in [
            "minecraft:air",
            "minecraft:cave_air",
            "minecraft:short_grass",
            "minecraft:snow[layers=1]",
        ] {
            assert!(is_replaceable(n), "{n}");
        }
        for n in ["minecraft:stone", "minecraft:water", "minecraft:oak_log"] {
            assert!(!is_replaceable(n), "{n}");
        }
    }

    #[test]
    fn ground_is_solid_blocks_but_not_air_plants_or_fluids() {
        for n in [
            "minecraft:stone",
            "minecraft:grass_block",
            "horizon_factory:iron_patch[richness=level_8]",
            "minecraft:dirt",
        ] {
            assert!(is_ground(n), "{n}");
        }
        for n in [
            "minecraft:air",
            "minecraft:short_grass",
            "minecraft:water",
            "minecraft:lava[level=0]",
        ] {
            assert!(!is_ground(n), "{n}");
        }
    }

    #[test]
    fn unloaded_space_reads_as_air_and_is_not_ground() {
        assert!(is_replaceable("minecraft:air"));
        assert!(!is_ground("minecraft:air"));
    }
}
