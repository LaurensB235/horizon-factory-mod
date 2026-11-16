//! The drill block entity: glue between the game (item slots, saved data, blocks) and the tested logic in
//! `factory_core::drill`. Everything with a rule in it lives in `factory-core`; this file only converts and applies.
//!
//! Slots (9, the content menu type `horizon_factory:drill`, laid out by `screen.rs`): 0 is fuel, 1 to 8 are output. State kept in the entity saved data:
//! `burn` (ticks of fuel left), `progress` (ticks towards the next unit) and `rng` (depletion random state).
use factory_core::blocks::base_name;
use factory_core::drill::{
    self, Change, DrillState, FIRST_OUTPUT, FUEL_SLOT, Inventory, Item, PatchCell, Resource, SLOTS,
    Stack,
};
use factory_core::layout::{self, Pos};
use horizon_client::mod_api::{Compound, Host, ItemStack, ModBlockEntity, ModContainer, Tag};

/// The block the entity belongs to, and the entity type id.
pub const DRILL_BLOCK: &str = "horizon_factory:drill";
pub const ENTITY_ID: &str = "horizon_factory:drill";
const COAL_PATCH: &str = "horizon_factory:coal_patch";
const IRON_PATCH: &str = "horizon_factory:iron_patch";
const COPPER_PATCH: &str = "horizon_factory:copper_patch";
const STONE: &str = "minecraft:stone";
/// The content mod menu type of the container (`data/horizon_factory/horizon/menu/drill.json`); a Horizon client needs
/// the native mod to show it (`screen.rs`).
pub const MENU: &str = "horizon_factory:drill";
/// `Level.setBlock` flags UPDATE_NEIGHBORS | UPDATE_CLIENTS (vanilla UPDATE_ALL).
const UPDATE_ALL: i32 = 3;

/// The entity a new drill block gets.
pub fn new_entity() -> (String, Compound) {
    (ENTITY_ID.to_string(), Compound::new())
}

/// The drill item slots: 9, hoppers put coal into slot 0 (from the top and the sides) and take from 1 to 8
/// (from below and the sides). Face order is the game `Direction`: down, up, north, south, west, east.
pub fn container() -> ModContainer {
    let output: Vec<usize> = (FIRST_OUTPUT..SLOTS).collect();
    let sides: Vec<usize> = std::iter::once(FUEL_SLOT)
        .chain(output.iter().copied())
        .collect();
    ModContainer {
        slots: SLOTS,
        // pipes and hoppers can take ore from every face but the top and put only coal in (`inputs`): the sides reach
        // the fuel slot and the outputs
        faces: [
            output.clone(),
            vec![FUEL_SLOT],
            sides.clone(),
            sides.clone(),
            sides.clone(),
            sides,
        ],
        inputs: vec![FUEL_SLOT],
        outputs: output,
        menu: Some(MENU.to_string()),
    }
}

// ----------------------------------------------------------------------------------------------- saved data

fn read_number(nbt: &Compound, key: &str) -> Option<i64> {
    // keys are Java strings; compare by text so this file needs no extra type
    nbt.iter()
        .find(|(k, _)| k.to_string_lossy() == key)
        .and_then(|(_, tag)| match tag {
            Tag::Int(v) => Some(i64::from(*v)),
            Tag::Long(v) => Some(*v),
            _ => None,
        })
}

/// A deterministic start for the depletion random state, from the block position (splitmix64 of the coordinates).
fn seed_for(pos: Pos) -> u64 {
    let mut z = (pos.0 as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((pos.1 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add((pos.2 as u64).wrapping_mul(0x94D0_49BB_1331_11EB));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn read_state(nbt: &Compound, pos: Pos) -> DrillState {
    DrillState {
        burn_left: read_number(nbt, "burn")
            .unwrap_or(0)
            .clamp(0, i64::from(u32::MAX)) as u32,
        progress: read_number(nbt, "progress")
            .unwrap_or(0)
            .clamp(0, i64::from(u32::MAX)) as u32,
        rng: read_number(nbt, "rng")
            .map(|v| v as u64)
            .unwrap_or_else(|| seed_for(pos)),
    }
}

fn write_state(nbt: &mut Compound, state: &DrillState) {
    nbt.insert("burn".into(), Tag::Int(state.burn_left as i32));
    nbt.insert("progress".into(), Tag::Int(state.progress as i32));
    nbt.insert("rng".into(), Tag::Long(state.rng as i64));
}

// -------------------------------------------------------------------------------------------------- items

fn plain_stack(item: &str, count: u8) -> ItemStack {
    ItemStack {
        item: item.into(),
        count: u32::from(count),
        damage: 0,
        extra: None,
    }
}

/// Game slots to the logic inventory. Items the logic does not know (something a player put in) are kept whole in
/// `others` and travel through as `Item::Other(index)`, so they come back unchanged.
fn to_core(slots: &[Option<ItemStack>], others: &mut Vec<ItemStack>) -> Inventory {
    let mut inv: Inventory = [None; SLOTS];
    for (i, slot) in slots.iter().take(SLOTS).enumerate() {
        let Some(stack) = slot else { continue };
        let item = match &*stack.item {
            "minecraft:coal" => Item::Coal,
            "minecraft:raw_iron" => Item::RawIron,
            "minecraft:raw_copper" => Item::RawCopper,
            _ => {
                others.push(stack.clone());
                Item::Other((others.len() - 1) as u32)
            }
        };
        inv[i] = Some(Stack {
            item,
            count: stack.count.min(255) as u8,
        });
    }
    inv
}

fn from_core(inv: &Inventory, others: &[ItemStack]) -> Vec<Option<ItemStack>> {
    inv.iter()
        .map(|slot| {
            slot.map(|s| match s.item {
                Item::Coal => plain_stack("minecraft:coal", s.count),
                Item::RawIron => plain_stack("minecraft:raw_iron", s.count),
                Item::RawCopper => plain_stack("minecraft:raw_copper", s.count),
                Item::Other(k) => {
                    let mut original = others[k as usize].clone();
                    original.count = u32::from(s.count);
                    original
                }
            })
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------ world

/// The mined patch blocks in the 5x5 area one block under the structure (the controller is in the bottom layer),
/// nearest to the centre first, with their richness.
fn scan_patches(host: &dyn Host, controller: Pos) -> Vec<PatchCell> {
    let mut patches = Vec::new();
    // the area is centred under the structure's middle column, not under the controller (which is at its front)
    let centre = layout::cell_pos(controller, (1, 0, 1), crate::facing_at(host, controller));
    for (dx, dz) in drill::area_offsets() {
        let pos = (centre.0 + dx, centre.1 - 1, centre.2 + dz);
        let at = [pos.0, pos.1, pos.2];
        let name = host.block(at);
        let resource = match base_name(&name) {
            COAL_PATCH => Resource::Coal,
            IRON_PATCH => Resource::Iron,
            COPPER_PATCH => Resource::Copper,
            _ => continue,
        };
        let level = host
            .block_property(at, "richness")
            .and_then(|v| drill::parse_richness(&v))
            .unwrap_or(1);
        patches.push(PatchCell {
            pos,
            resource,
            level,
        });
    }
    patches
}

fn apply_change(host: &mut dyn Host, change: Change) {
    match change {
        Change::SetLevel { pos, level } => {
            host.set_block_property(
                [pos.0, pos.1, pos.2],
                "richness",
                &drill::richness_name(level),
            );
        }
        Change::ToStone { pos } => {
            if let Some(stone) = host.default_state(STONE) {
                host.set_block(host.dimension(), [pos.0, pos.1, pos.2], stone, UPDATE_ALL);
            }
        }
    }
}

fn set_lit(host: &mut dyn Host, pos: Pos, lit: bool) {
    host.set_block_property(
        [pos.0, pos.1, pos.2],
        "lit",
        if lit { "true" } else { "false" },
    );
}

/// One server tick of a drill block entity at `pos`. Changes the entity only when something changed, so an idle
/// drill does not keep marking its chunk as modified.
pub fn tick(host: &mut dyn Host, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if entity.id != ENTITY_ID {
        return;
    }
    let pos: Pos = (pos[0], pos[1], pos[2]);
    let slots_before = entity.items.clone();
    let mut slots_now = slots_before.clone();
    if slots_now.len() < SLOTS {
        slots_now.resize(SLOTS, None);
    }
    let mut others = Vec::new();
    let mut inv = to_core(&slots_now, &mut others);
    let inv_before = inv;
    let state_before = read_state(&entity.nbt, pos);
    let mut state = state_before.clone();

    // nothing to do for an idle drill: no fuel in the slot and none burning
    let has_fuel = matches!(inv[FUEL_SLOT], Some(Stack { item: Item::Coal, count }) if count > 0);
    if !has_fuel && state.burn_left == 0 {
        set_lit(host, pos, false);
        // an idle drill still tells the screen which ore is under it
        let patches = scan_patches(&*host, pos);
        set_data(entity, &state, &patches);
        return;
    }

    let patches = scan_patches(&*host, pos);
    let out = drill::tick(&mut state, &mut inv, &patches);
    if let Some(change) = out.change {
        apply_change(host, change);
    }
    set_lit(host, pos, out.lit);

    if inv != inv_before {
        let new_slots = from_core(&inv, &others);
        // keep any slots beyond the nine as they were (there are none for this container)
        entity.items = new_slots;
    }
    if state != state_before {
        write_state(&mut entity.nbt, &state);
    }
    set_data(entity, &state, &patches);
}

/// The ore the drill mines, as the screen's code: 0 none, 1 coal, 2 iron, 3 copper.
fn ore_code(resource: Resource) -> i32 {
    match resource {
        Resource::Coal => 1,
        Resource::Iron => 2,
        Resource::Copper => 3,
    }
}

/// The data slots the menu shows: fuel left and mining progress (in ticks), then the ore the drill mines (code, as
/// `ore_code`) and how many units of it are left under the drill (every patch block of that ore in the 5x5 area, at
/// `map::units_for_level` each: at most 25 x 128 = 3,200, which fits a 16-bit data slot). Only written when they change.
fn set_data(entity: &mut ModBlockEntity, state: &DrillState, patches: &[PatchCell]) {
    let (code, units) = match patches.first() {
        Some(target) => (
            ore_code(target.resource),
            patches
                .iter()
                .filter(|p| p.resource == target.resource)
                .map(|p| factory_core::drill::units_for_level(p.level))
                .sum::<u32>() as i32,
        ),
        None => (0, 0),
    };
    let data = vec![state.burn_left as i32, state.progress as i32, code, units];
    if entity.data != data {
        entity.data = data;
    }
}
