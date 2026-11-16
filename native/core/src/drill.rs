//! Drill behaviour: fuel, mining rate, which patch is mined, depletion and output. Pure logic: the game glue
//! passes a snapshot of the patch blocks under the drill and the 9-slot inventory, and applies the returned change.
//!
//! Slots: 0 is fuel (coal only), 1 to 8 are output.

pub type Pos = (i32, i32, i32);

pub const SLOTS: usize = 9;
pub const FUEL_SLOT: usize = 0;
pub const FIRST_OUTPUT: usize = 1;
pub const MAX_STACK: u8 = 64;
/// One coal burns for 30 seconds.
pub const BURN_TICKS: u32 = 600;
/// One resource unit every 80 ticks (0.25 per second).
pub const MINE_INTERVAL: u32 = 80;
/// Each mined unit lowers the patch block one richness level with probability 1 in this number.
pub const DEPLETE_ONE_IN: u64 = 16;
pub const MAX_LEVEL: u8 = 8;

/// Ore units a patch block of richness `level` (1 to 8) still holds: a unit is mined every `MINE_INTERVAL` ticks and
/// lowers the block one level with probability 1 in `DEPLETE_ONE_IN`, so each level holds `DEPLETE_ONE_IN` units on
/// average and a full block about 128.
pub fn units_for_level(level: u8) -> u32 {
    DEPLETE_ONE_IN as u32 * u32::from(level.min(MAX_LEVEL))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Coal,
    Iron,
    Copper,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Coal,
    RawIron,
    RawCopper,
    Other(u32),
}

impl Resource {
    pub fn item(self) -> Item {
        match self {
            Resource::Coal => Item::Coal,
            Resource::Iron => Item::RawIron,
            Resource::Copper => Item::RawCopper,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    pub item: Item,
    pub count: u8,
}

pub type Inventory = [Option<Stack>; SLOTS];

/// A patch block under the drill with richness above zero (`level` 1 to 8). The glue lists them in a fixed scan order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatchCell {
    pub pos: Pos,
    pub resource: Resource,
    pub level: u8,
}

/// The world change a mining step asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    SetLevel {
        pos: Pos,
        level: u8,
    },
    /// Richness ran out: the block becomes stone.
    ToStone {
        pos: Pos,
    },
}

/// The saved state of one drill (the glue keeps it in the block entity data).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrillState {
    /// Ticks of fuel left from the coal being burnt.
    pub burn_left: u32,
    /// Ticks worked towards the next unit.
    pub progress: u32,
    /// Pseudo-random state for depletion, seeded from the block position so the logic is deterministic.
    pub rng: u64,
}

impl DrillState {
    pub fn new(seed: u64) -> Self {
        DrillState {
            burn_left: 0,
            progress: 0,
            rng: seed,
        }
    }

    fn next_random(&mut self) -> u64 {
        // splitmix64
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TickOutput {
    /// Whether the controller `lit` state should be true.
    pub lit: bool,
    pub change: Option<Change>,
}

fn has_room(inv: &Inventory, item: Item) -> bool {
    inv[FIRST_OUTPUT..].iter().any(|s| match s {
        None => true,
        Some(st) => st.item == item && st.count < MAX_STACK,
    })
}

fn insert(inv: &mut Inventory, item: Item) -> bool {
    for st in inv[FIRST_OUTPUT..].iter_mut().flatten() {
        if st.item == item && st.count < MAX_STACK {
            st.count += 1;
            return true;
        }
    }
    for s in inv[FIRST_OUTPUT..].iter_mut() {
        if s.is_none() {
            *s = Some(Stack { item, count: 1 });
            return true;
        }
    }
    false
}

fn take_one_coal(inv: &mut Inventory) -> bool {
    match inv[FUEL_SLOT] {
        Some(Stack {
            item: Item::Coal,
            count,
        }) if count > 0 => {
            inv[FUEL_SLOT] = if count == 1 {
                None
            } else {
                Some(Stack {
                    item: Item::Coal,
                    count: count - 1,
                })
            };
            true
        }
        _ => false,
    }
}

/// One server tick. `patches` are the minable patch blocks under the drill, in scan order; the first is mined.
/// The drill only burns fuel while it can work: with no patch, or no room for the item, it pauses and keeps its
/// fuel and progress.
pub fn tick(state: &mut DrillState, inv: &mut Inventory, patches: &[PatchCell]) -> TickOutput {
    let Some(target) = patches.first().copied() else {
        return TickOutput {
            lit: false,
            change: None,
        };
    };
    let item = target.resource.item();
    if !has_room(inv, item) {
        return TickOutput {
            lit: false,
            change: None,
        };
    }
    if state.burn_left == 0 {
        if !take_one_coal(inv) {
            return TickOutput {
                lit: false,
                change: None,
            };
        }
        state.burn_left = BURN_TICKS;
    }
    state.burn_left -= 1;
    state.progress += 1;
    let mut change = None;
    if state.progress >= MINE_INTERVAL {
        state.progress = 0;
        let inserted = insert(inv, item);
        debug_assert!(inserted, "room was checked above");
        if state.next_random().is_multiple_of(DEPLETE_ONE_IN) {
            change = Some(if target.level <= 1 {
                Change::ToStone { pos: target.pos }
            } else {
                Change::SetLevel {
                    pos: target.pos,
                    level: target.level - 1,
                }
            });
        }
    }
    TickOutput { lit: true, change }
}

/// How far the mining area reaches from the controller: 2 gives a 5x5 area (a 3x3 body mining a 5x5 area).
pub const AREA_RADIUS: i32 = 2;

/// The 5x5 area under the drill as (dx, dz) offsets from the controller, nearest to the centre first (ties by x, then
/// z), so the drill works from the middle outwards and always in the same order.
pub fn area_offsets() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    for dx in -AREA_RADIUS..=AREA_RADIUS {
        for dz in -AREA_RADIUS..=AREA_RADIUS {
            v.push((dx, dz));
        }
    }
    v.sort_by_key(|&(x, z)| (x * x + z * z, x, z));
    v
}

/// The block state value of richness `level` (1 to 8): `level_5`.
pub fn richness_name(level: u8) -> String {
    format!("level_{level}")
}

/// Read a `richness` state value (`level_5`) back to its level. `None` for anything else or a level outside 1 to 8.
pub fn parse_richness(value: &str) -> Option<u8> {
    let n: u8 = value.strip_prefix("level_")?.parse().ok()?;
    (1..=MAX_LEVEL).contains(&n).then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> Inventory {
        [None; SLOTS]
    }
    fn with_coal(n: u8) -> Inventory {
        let mut inv = empty();
        inv[FUEL_SLOT] = Some(Stack {
            item: Item::Coal,
            count: n,
        });
        inv
    }
    fn patch(x: i32, resource: Resource, level: u8) -> PatchCell {
        PatchCell {
            pos: (x, 0, 0),
            resource,
            level,
        }
    }
    fn count(inv: &Inventory, item: Item) -> u32 {
        inv[FIRST_OUTPUT..]
            .iter()
            .flatten()
            .filter(|s| s.item == item)
            .map(|s| s.count as u32)
            .sum()
    }
    /// Apply a change to a field list like the glue would.
    fn apply(field: &mut Vec<PatchCell>, c: Change) {
        match c {
            Change::SetLevel { pos, level } => field
                .iter_mut()
                .filter(|p| p.pos == pos)
                .for_each(|p| p.level = level),
            Change::ToStone { pos } => field.retain(|p| p.pos != pos),
        }
    }

    #[test]
    fn without_fuel_nothing_happens() {
        let mut st = DrillState::new(1);
        let mut inv = empty();
        let field = vec![patch(0, Resource::Iron, 8)];
        for _ in 0..1000 {
            assert_eq!(
                tick(&mut st, &mut inv, &field),
                TickOutput {
                    lit: false,
                    change: None
                }
            );
        }
        assert_eq!(count(&inv, Item::RawIron), 0);
    }

    #[test]
    fn one_coal_gives_seven_units_and_is_used_up() {
        let mut st = DrillState::new(1);
        let mut inv = with_coal(1);
        let mut field = vec![patch(0, Resource::Iron, 8)];
        let mut lit_ticks = 0;
        for _ in 0..700 {
            let out = tick(&mut st, &mut inv, &field);
            if out.lit {
                lit_ticks += 1;
            }
            if let Some(c) = out.change {
                apply(&mut field, c);
            }
        }
        assert_eq!(lit_ticks, BURN_TICKS);
        assert_eq!(count(&inv, Item::RawIron), 7); // 600 / 80 = 7.5, the 8th unit needs the next coal
        assert_eq!(inv[FUEL_SLOT], None);
        assert_eq!(st.burn_left, 0);
    }

    #[test]
    fn the_rate_is_one_unit_per_80_ticks() {
        let mut st = DrillState::new(7);
        let mut inv = with_coal(5);
        let field = vec![patch(0, Resource::Copper, 8)];
        for _ in 0..(MINE_INTERVAL - 1) {
            tick(&mut st, &mut inv, &field);
        }
        assert_eq!(count(&inv, Item::RawCopper), 0);
        tick(&mut st, &mut inv, &field);
        assert_eq!(count(&inv, Item::RawCopper), 1);
    }

    #[test]
    fn each_resource_gives_its_own_item() {
        for (r, item) in [
            (Resource::Coal, Item::Coal),
            (Resource::Iron, Item::RawIron),
            (Resource::Copper, Item::RawCopper),
        ] {
            let mut st = DrillState::new(3);
            let mut inv = with_coal(1);
            let field = vec![patch(0, r, 8)];
            for _ in 0..MINE_INTERVAL {
                tick(&mut st, &mut inv, &field);
            }
            assert_eq!(count(&inv, item), 1);
        }
    }

    #[test]
    fn full_output_pauses_without_burning_fuel() {
        let mut st = DrillState::new(1);
        let mut inv = with_coal(3);
        for s in inv[FIRST_OUTPUT..].iter_mut() {
            *s = Some(Stack {
                item: Item::RawIron,
                count: MAX_STACK,
            });
        }
        let field = vec![patch(0, Resource::Iron, 8)];
        for _ in 0..500 {
            assert!(!tick(&mut st, &mut inv, &field).lit);
        }
        assert_eq!(
            inv[FUEL_SLOT],
            Some(Stack {
                item: Item::Coal,
                count: 3
            })
        );
        assert_eq!(st.burn_left, 0);
        // making room lets it resume
        inv[FIRST_OUTPUT] = None;
        assert!(tick(&mut st, &mut inv, &field).lit);
    }

    #[test]
    fn output_full_of_other_items_also_pauses() {
        let mut st = DrillState::new(1);
        let mut inv = with_coal(1);
        for s in inv[FIRST_OUTPUT..].iter_mut() {
            *s = Some(Stack {
                item: Item::Other(1),
                count: 1,
            });
        }
        let field = vec![patch(0, Resource::Iron, 8)];
        assert!(!tick(&mut st, &mut inv, &field).lit);
    }

    #[test]
    fn stacks_fill_in_order_up_to_64() {
        let mut inv = empty();
        for _ in 0..70 {
            assert!(insert(&mut inv, Item::Coal));
        }
        assert_eq!(
            inv[1],
            Some(Stack {
                item: Item::Coal,
                count: 64
            })
        );
        assert_eq!(
            inv[2],
            Some(Stack {
                item: Item::Coal,
                count: 6
            })
        );
        assert_eq!(inv[FUEL_SLOT], None); // never the fuel slot
    }

    #[test]
    fn no_patch_means_no_work_and_no_fuel_use() {
        let mut st = DrillState::new(1);
        let mut inv = with_coal(2);
        for _ in 0..200 {
            assert!(!tick(&mut st, &mut inv, &[]).lit);
        }
        assert_eq!(
            inv[FUEL_SLOT],
            Some(Stack {
                item: Item::Coal,
                count: 2
            })
        );
    }

    #[test]
    fn the_first_patch_in_scan_order_is_mined_first() {
        let mut st = DrillState::new(11);
        let mut inv = with_coal(1);
        let field = vec![patch(0, Resource::Copper, 3), patch(1, Resource::Iron, 8)];
        for _ in 0..MINE_INTERVAL {
            tick(&mut st, &mut inv, &field);
        }
        assert_eq!(count(&inv, Item::RawCopper), 1);
        assert_eq!(count(&inv, Item::RawIron), 0);
    }

    #[test]
    fn same_seed_gives_the_same_run() {
        let run = |seed| {
            let mut st = DrillState::new(seed);
            let mut inv = with_coal(MAX_STACK);
            let mut field = vec![patch(0, Resource::Iron, 8)];
            let mut changes = vec![];
            for t in 0..20_000u32 {
                if inv[FUEL_SLOT].is_none() {
                    inv[FUEL_SLOT] = Some(Stack {
                        item: Item::Coal,
                        count: MAX_STACK,
                    });
                }
                let out = tick(&mut st, &mut inv, &field);
                if let Some(c) = out.change {
                    changes.push((t, c));
                    apply(&mut field, c);
                }
            }
            changes
        };
        assert_eq!(run(5), run(5));
        assert_ne!(run(5), run(6));
    }

    /// Mine one block of the given starting level to nothing with unlimited fuel and empty output; return units mined.
    fn units_until_stone(seed: u64, level: u8) -> u32 {
        let mut st = DrillState::new(seed);
        let mut inv = empty();
        let mut field = vec![patch(0, Resource::Iron, level)];
        let mut units = 0;
        for _ in 0..2_000_000u32 {
            inv[FUEL_SLOT] = Some(Stack {
                item: Item::Coal,
                count: MAX_STACK,
            });
            let out = tick(&mut st, &mut inv, &field);
            units += count(&inv, Item::RawIron);
            for s in inv[FIRST_OUTPUT..].iter_mut() {
                *s = None;
            }
            if let Some(c) = out.change {
                apply(&mut field, c);
                if field.is_empty() {
                    return units;
                }
            }
        }
        panic!("never depleted");
    }

    #[test]
    fn a_full_block_yields_about_128_units() {
        let n = 300u64;
        let total: u32 = (1..=n).map(|s| units_until_stone(s, MAX_LEVEL)).sum();
        let mean = total as f64 / n as f64;
        assert!(
            (115.0..=141.0).contains(&mean),
            "mean units per block was {mean}"
        );
    }

    #[test]
    fn a_block_at_level_one_goes_straight_to_stone() {
        for seed in 1..=50 {
            assert!(units_until_stone(seed, 1) >= 1);
        }
        let mut st = DrillState::new(2);
        let mut inv = with_coal(MAX_STACK);
        let field = vec![patch(0, Resource::Iron, 1)];
        let mut saw_stone = false;
        for _ in 0..100_000 {
            let out = tick(&mut st, &mut inv, &field);
            if let Some(c) = out.change {
                assert!(
                    matches!(c, Change::ToStone { .. }),
                    "level 1 must go to stone, got {c:?}"
                );
                saw_stone = true;
                break;
            }
            for s in inv[FIRST_OUTPUT..].iter_mut() {
                *s = None;
            }
            inv[FUEL_SLOT] = Some(Stack {
                item: Item::Coal,
                count: MAX_STACK,
            });
        }
        assert!(saw_stone);
    }

    #[test]
    fn a_coal_drill_can_feed_itself() {
        // One coal burns 600 ticks and the drill mines 7 coal in that time, so a hopper moving mined coal
        // back to the fuel slot keeps it running.
        let mut st = DrillState::new(9);
        let mut inv = with_coal(1);
        let field = vec![patch(0, Resource::Coal, 8)];
        let mut moved = 0;
        for _ in 0..5_000 {
            tick(&mut st, &mut inv, &field);
            // the glue (a hopper) moves one mined coal back into the fuel slot when it is empty
            if inv[FUEL_SLOT].is_none() && count(&inv, Item::Coal) > 0 {
                let idx = inv[FIRST_OUTPUT..]
                    .iter()
                    .position(|s| {
                        matches!(
                            s,
                            Some(Stack {
                                item: Item::Coal,
                                ..
                            })
                        )
                    })
                    .map(|i| i + FIRST_OUTPUT);
                if let Some(i) = idx {
                    let c = inv[i].unwrap().count;
                    inv[FUEL_SLOT] = Some(Stack {
                        item: Item::Coal,
                        count: 1,
                    });
                    inv[i] = if c == 1 {
                        None
                    } else {
                        Some(Stack {
                            item: Item::Coal,
                            count: c - 1,
                        })
                    };
                    moved += 1;
                }
            }
        }
        assert!(moved >= 5, "the drill kept itself running {moved} times");
        assert!(count(&inv, Item::Coal) > 0, "and still produced coal");
    }

    #[test]
    fn the_mining_area_is_5x5_centre_first() {
        let a = area_offsets();
        assert_eq!(a.len(), 25);
        assert_eq!(a.iter().collect::<std::collections::HashSet<_>>().len(), 25);
        assert_eq!(a[0], (0, 0));
        let d: Vec<i32> = a.iter().map(|&(x, z)| x * x + z * z).collect();
        assert!(d.windows(2).all(|w| w[0] <= w[1]), "nearest first");
        assert!(a.iter().all(|&(x, z)| x.abs() <= 2 && z.abs() <= 2));
        assert_eq!(a, area_offsets(), "always the same order");
    }

    #[test]
    fn a_level_holds_sixteen_units_and_a_full_block_128() {
        assert_eq!(units_for_level(1), 16);
        assert_eq!(units_for_level(8), 128);
        assert_eq!(
            units_for_level(200),
            128,
            "a level past the maximum counts as the maximum"
        );
    }

    #[test]
    fn richness_names_round_trip() {
        for level in 1..=MAX_LEVEL {
            assert_eq!(parse_richness(&richness_name(level)), Some(level));
        }
        assert_eq!(richness_name(5), "level_5");
        for bad in [
            "", "level_0", "level_9", "level_", "level_x", "5", "Level_5",
        ] {
            assert_eq!(parse_richness(bad), None, "{bad:?}");
        }
    }
}
