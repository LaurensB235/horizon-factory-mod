//! The drill's screen. The container is the content menu type `horizon_factory:drill` (slot 0 fuel, slots 1 to 8
//! output that players only take from, two data values); this file lays its slots out and draws the labels and the two
//! bars. A Horizon client shows a content menu type only through a native screen like this one, so a player needs the
//! native mod to open the drill.
//!
//! Layout in GUI pixels from the panel's top left (the panel is 176 x 166, the player's inventory starts at y 84):
//! the fuel slot on the left with a burn bar under it, an arrow bar for mining progress, and the eight output slots as
//! two rows of four on the right.
use crate::drill::{ENTITY_ID, MENU};
use factory_core::drill::{BURN_TICKS, FIRST_OUTPUT, FUEL_SLOT, MINE_INTERVAL, SLOTS};
use horizon_client::mod_api::{
    Host, MenuSlotRulesEvent, NativeScreen, ScreenDrawEvent, ScreenEvent,
};

const WIDTH: f32 = 176.0;
const HEIGHT: f32 = 166.0;
const FUEL_AT: [f32; 2] = [26.0, 36.0];
const OUTPUT_AT: [f32; 2] = [90.0, 28.0];
const COLUMNS: usize = 4;
const STEP: f32 = 18.0;

/// Panel text colour (vanilla's menu labels).
const LABEL: u32 = 0x40_40_40;
const BAR_BACK: u32 = 0xFF_37_37_37;
const BURN: u32 = 0xFF_E8_A0_30;
const PROGRESS: u32 = 0xFF_4F_A8_4F;
/// Where the ore under the drill is written: on the title line, right of "Drill" (the panel is 176 wide).
const ORE_AT: [f32; 2] = [50.0, 1.0];

/// The top-left of container slot `slot`, as the screen wants it.
fn slot_at(slot: usize) -> [f32; 2] {
    if slot == FUEL_SLOT {
        return FUEL_AT;
    }
    let i = slot - FIRST_OUTPUT;
    [
        OUTPUT_AT[0] + (i % COLUMNS) as f32 * STEP,
        OUTPUT_AT[1] + (i / COLUMNS) as f32 * STEP,
    ]
}

/// The layout of the drill's screen; `None` for any other block.
pub fn screen(event: &ScreenEvent<'_>) -> Option<NativeScreen> {
    if event.block != ENTITY_ID {
        return None;
    }
    Some(NativeScreen {
        width: WIDTH,
        height: HEIGHT,
        slots: (0..SLOTS).map(|s| Some(slot_at(s))).collect(),
        inventory_y: 84.0,
        inventory_x: 8.0,
        own_labels: false,
        own_background: false,
    })
}

/// A horizontal bar `w` wide at (`x`, `y`), `fraction` (0 to 1) filled in `colour`.
fn bar(
    host: &dyn Host,
    event: &mut ScreenDrawEvent<'_>,
    at: [f32; 2],
    w: f32,
    fraction: f32,
    colour: u32,
) {
    let (x, y) = (event.origin[0] + at[0], event.origin[1] + at[1]);
    host.fill(event.batch, [x, y, w, 4.0], BAR_BACK);
    let filled = (w * fraction.clamp(0.0, 1.0)).round();
    if filled > 0.0 {
        host.fill(event.batch, [x, y, filled, 4.0], colour);
    }
}

/// Labels and bars. `data` is [burn ticks left, mining progress in ticks], empty until the server's first values.
pub fn draw(host: &dyn Host, event: &mut ScreenDrawEvent<'_>) {
    if event.block != ENTITY_ID {
        return;
    }
    let burn = event.data.first().copied().unwrap_or(0).max(0) as f32;
    let progress = event.data.get(1).copied().unwrap_or(0).max(0) as f32;
    let (ox, oy) = (event.origin[0], event.origin[1]);
    host.text(
        event.batch,
        "Fuel",
        [ox + FUEL_AT[0], oy + 20.0],
        LABEL,
        false,
    );
    host.text(
        event.batch,
        "Output",
        [ox + OUTPUT_AT[0], oy + 16.0],
        LABEL,
        false,
    );
    // the fuel left in the burning coal, under the fuel slot
    bar(
        host,
        event,
        [FUEL_AT[0], FUEL_AT[1] + 20.0],
        16.0,
        burn / BURN_TICKS as f32,
        BURN,
    );
    // which ore the drill mines: the item it produces (drawn by the game exactly as in the inventory), its name and how
    // many units of it are left under the drill
    let code = event.data.get(2).copied().unwrap_or(0);
    let units = event.data.get(3).copied().unwrap_or(0);
    let (name, icon, swatch) = match code {
        1 => ("Coal", "minecraft:coal", 0xFF_3A_3A_3A),
        2 => ("Iron", "minecraft:raw_iron", 0xFF_C0_7A_45),
        3 => ("Copper", "minecraft:raw_copper", 0xFF_D2_61_2E),
        _ => ("", "", 0),
    };
    if swatch == 0 {
        host.text(
            event.batch,
            "No ore below",
            [ox + ORE_AT[0], oy + ORE_AT[1] + 4.0],
            0x80_30_30,
            false,
        );
    } else {
        let (ix, iy) = (ox + ORE_AT[0], oy + ORE_AT[1]);
        // the real item, 16 GUI pixels square like in a slot; a coloured square stands in if it cannot be drawn
        if !host.item(event.batch, icon, [ix, iy], 16.0) {
            host.fill(event.batch, [ix + 4.0, iy + 4.0, 8.0, 8.0], swatch);
        }
        let label = format!("{name}: {units} left");
        host.text(event.batch, &label, [ix + 20.0, iy + 4.0], LABEL, false);
    }
    // the progress to the next unit, between the fuel slot and the outputs
    bar(
        host,
        event,
        [52.0, FUEL_AT[1] + 6.0],
        28.0,
        progress / MINE_INTERVAL as f32,
        PROGRESS,
    );
}

/// The fuel slot takes coal only (the output slots refuse everything: they are `results` of the menu type).
pub fn slot_rules(event: &mut MenuSlotRulesEvent<'_>) {
    if event.menu == MENU && event.slot == FUEL_SLOT {
        event.may_place = &*event.stack.item == "minecraft:coal";
    }
}
