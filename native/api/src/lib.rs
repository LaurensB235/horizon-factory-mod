//! The Factory trait mod: hook glue between the game and the game-independent logic in `factory-core`.
//!
//! This ticket (#6): the 3x3x3 drill is one object. Using the drill item places all 27 blocks (or says why it
//! cannot), and breaking any of them removes the whole structure.
mod compass;
mod drill;
mod map;
mod maps;
mod sampling;
mod scanner;
mod screen;

use factory_core::blocks::{base_name, is_ground, is_replaceable};
use factory_core::layout::{self, Facing, PlaceError, Pos, WorldView};
use horizon_client::mod_api::{
    BlockEntityContainerEvent, BlockEntityEvent, BlockEntityTickEvent, BlockEntityTicksEvent,
    Compound, DestroyBlockEvent, Flow, Hooks, Host, MenuSlotRulesEvent, ModContainer, NativeScreen,
    NetFromServerEvent, ScreenDrawEvent, ScreenEvent, ScreenMouseButtonPressedPreEvent,
    ScreenMouseButtonReleasedPreEvent, ScreenMouseDraggedPreEvent, ScreenMouseScrolledPreEvent,
    ScreenRenderPostEvent, ServerTickEvent, Text, UseItemEvent, UseItemOnEvent,
};

const DRILL: &str = "horizon_factory:drill";
const PART: &str = "horizon_factory:drill_part";
const AIR: &str = "minecraft:air";
/// `Level.setBlock` flags UPDATE_NEIGHBORS | UPDATE_CLIENTS (vanilla UPDATE_ALL).
const UPDATE_ALL: i32 = 3;
/// Game mode number of creative (`NativeServerPlayer::game_mode`).
const CREATIVE: i32 = 1;

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(FactoryMod)
}

struct FactoryMod;

/// The game answers the layout rules by block name (`Host::block`: air where nothing is loaded).
struct HostView<'a>(&'a dyn Host);

impl WorldView for HostView<'_> {
    fn is_replaceable(&self, pos: Pos) -> bool {
        is_replaceable(&self.0.block([pos.0, pos.1, pos.2]))
    }

    fn is_solid(&self, pos: Pos) -> bool {
        is_ground(&self.0.block([pos.0, pos.1, pos.2]))
    }
}

fn tell(host: &mut dyn Host, text: &str) {
    if let Some(player) = host.acting_player() {
        host.tell(player.uuid, Text::literal(text));
    }
}

fn is_creative(host: &dyn Host) -> bool {
    host.acting_player()
        .is_some_and(|p| p.game_mode == CREATIVE)
}

/// Which way the front of the drill whose controller or part is at `pos` faces. The blocks carry it in their `facing`
/// property; a block without one (a drill placed before drills could turn) faces north, as all of those do.
pub(crate) fn facing_at(host: &dyn Host, pos: Pos) -> Facing {
    host.block_property([pos.0, pos.1, pos.2], "facing")
        .and_then(|v| Facing::parse(&v))
        .unwrap_or_default()
}

/// Give the block at `at` the property value, judged by reading it back: `set_block_property` answers false when the
/// property ALREADY has the value (the default state of a part is the first cell, facing north), so its answer says
/// nothing about success.
fn set_property_checked(host: &mut dyn Host, at: [i32; 3], key: &str, value: &str) -> bool {
    if host.block_property(at, key).as_deref() != Some(value) {
        host.set_block_property(at, key, value);
        return host.block_property(at, key).as_deref() == Some(value);
    }
    true
}

/// Place the controller at `controller` and the 26 parts around and above it, the front facing `facing`. Returns the
/// position that failed.
fn place_structure(host: &mut dyn Host, controller: Pos, facing: Facing) -> Result<(), Pos> {
    let dim = host.dimension();
    let drill = host.default_state(DRILL).ok_or(controller)?;
    let part = host.default_state(PART).ok_or(controller)?;
    for cell in layout::cells() {
        let p = layout::cell_pos(controller, cell, facing);
        let at = [p.0, p.1, p.2];
        match layout::cell_name(cell) {
            None => {
                if !host.set_block(dim, at, drill, UPDATE_ALL)
                    || !set_property_checked(host, at, "facing", facing.name())
                {
                    return Err(p);
                }
            }
            Some(name) => {
                if !host.set_block(dim, at, part, UPDATE_ALL)
                    || !set_property_checked(host, at, "cell", &name)
                    || !set_property_checked(host, at, "facing", facing.name())
                {
                    return Err(p);
                }
            }
        }
    }
    Ok(())
}

/// Replace every cell of the structure that is still a drill block with air. Blocks of other kinds that someone put
/// into a cell are left alone. With `drop_item`, the controller is destroyed through its loot table (one Drill drops);
/// without it (creative mode, or undoing a failed placement) every block is simply replaced with air.
fn clear_structure(host: &mut dyn Host, controller: Pos, facing: Facing, drop_item: bool) {
    let dim = host.dimension();
    let Some(air) = host.default_state(AIR) else {
        return;
    };
    // the controller first, so its loot (one Drill) is rolled once, then the 26 parts (their loot is empty)
    let mut cells = layout::cells();
    cells.sort_by_key(|&c| c != layout::CONTROLLER_CELL);
    for cell in cells {
        let p = layout::cell_pos(controller, cell, facing);
        let at = [p.0, p.1, p.2];
        let name = host.block(at);
        let name = base_name(&name);
        if name != DRILL && name != PART {
            continue;
        }
        if cell == layout::CONTROLLER_CELL && drop_item {
            // Level.destroyBlock with the block's loot table, the drop popped at the block
            host.destroy_block(at, None, false);
        } else {
            host.set_block(dim, at, air, UPDATE_ALL);
        }
    }
}

impl Hooks for FactoryMod {
    /// Using the drill item on a block places the whole structure against the clicked face, its front facing the player
    /// (the body extends away from them), or tells the player why not.
    /// Always claimed, so the game never places the single controller block by itself.
    fn use_item_on(&self, host: &mut dyn Host, event: &mut UseItemOnEvent<'_>) -> Flow {
        if &*event.stack.item != DRILL {
            return Flow::Continue;
        }
        let controller: Pos = (
            event.pos[0] + event.face[0],
            event.pos[1] + event.face[1],
            event.pos[2] + event.face[2],
        );
        let facing = host
            .acting_player()
            .map(|p| Facing::toward_looker(p.yaw))
            .unwrap_or_default();
        let verdict = layout::validate(&HostView(&*host), controller, facing);
        match verdict {
            Ok(()) => match place_structure(host, controller, facing) {
                Ok(()) => {
                    if !is_creative(&*host) {
                        host.take_items(DRILL, 1);
                    }
                }
                Err(failed) => {
                    // undo whatever was placed: nothing may be left half built
                    clear_structure(host, controller, facing, false);
                    tell(
                        host,
                        &format!(
                            "The drill could not be built (a block at {} {} {} did not take).",
                            failed.0, failed.1, failed.2
                        ),
                    );
                }
            },
            Err(PlaceError::Blocked(p)) => tell(
                host,
                &format!(
                    "The drill needs a free 3x3x3 space: a block is in the way at {} {} {}.",
                    p.0, p.1, p.2
                ),
            ),
            Err(PlaceError::NoFloor(p)) => tell(
                host,
                &format!(
                    "The drill needs solid ground under all of it: nothing to stand on at {} {} {}.",
                    p.0, p.1, p.2
                ),
            ),
        }
        Flow::Cancel
    }

    /// Breaking any of the 27 blocks removes the whole structure, dropping one Drill (none in creative mode).
    /// The break itself is cancelled because the structure is removed here.
    fn destroy_block(&self, host: &mut dyn Host, event: &mut DestroyBlockEvent) -> Flow {
        let name = host.block(event.pos);
        let pos: Pos = (event.pos[0], event.pos[1], event.pos[2]);
        let (controller, facing) = match base_name(&name) {
            DRILL => (pos, facing_at(&*host, pos)),
            PART => {
                let cell = host
                    .block_state_property(event.state, "cell")
                    .and_then(layout::parse_cell);
                let facing = host
                    .block_state_property(event.state, "facing")
                    .and_then(|v| Facing::parse(&v))
                    .unwrap_or_default();
                match cell {
                    Some(cell) => (layout::controller_of(pos, cell, facing), facing),
                    None => return Flow::Continue,
                }
            }
            _ => return Flow::Continue,
        };
        let creative = is_creative(&*host);
        clear_structure(host, controller, facing, !creative);
        Flow::Cancel
    }

    /// The drill controller gets a block entity (saved data and item slots).
    fn block_entity(
        &self,
        _host: &mut dyn Host,
        event: &mut BlockEntityEvent<'_>,
    ) -> Option<(String, Compound)> {
        (event.block == drill::DRILL_BLOCK).then(drill::new_entity)
    }

    /// The drill entity ticks on the server.
    fn block_entity_ticks(&self, _host: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        if event.block == drill::DRILL_BLOCK {
            event.ticks = true;
        }
    }

    /// Nine item slots with the vanilla 3x3 menu: fuel in slot 0, output in 1 to 8.
    fn block_entity_container(
        &self,
        _host: &dyn Host,
        event: &mut BlockEntityContainerEvent<'_>,
    ) -> Option<ModContainer> {
        (event.block == drill::DRILL_BLOCK).then(drill::container)
    }

    /// Mining, fuel and depletion (see `drill.rs` and `factory-core`).
    fn block_entity_tick(&self, host: &mut dyn Host, event: &mut BlockEntityTickEvent<'_>) {
        drill::tick(host, event.pos, event.entity);
    }

    /// The server keeps the map database (the web map and the scanner's map screen) up to date, see `map`.
    fn server_tick(&self, host: &mut dyn Host, event: &mut ServerTickEvent<'_>) {
        map::server_tick(host, event);
    }

    /// Client: the map data and the order to open the map screen from the server.
    fn net_from_server(&self, host: &mut dyn Host, event: &mut NetFromServerEvent<'_>) {
        maps::net_from_server(host, event);
    }

    /// Client: the full-screen map, drawn over the world in the mod's own screen.
    fn screen_render_post(&self, host: &dyn Host, event: &mut ScreenRenderPostEvent<'_>) {
        maps::render_screen(host, event);
    }

    /// Client: dragging pans the map and the wheel zooms it.
    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn Host,
        event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        maps::mouse_pressed(host, event)
    }

    fn screen_mouse_button_released_pre(
        &self,
        host: &dyn Host,
        event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
    ) -> Flow {
        maps::mouse_released(host, event)
    }

    fn screen_mouse_dragged_pre(
        &self,
        host: &dyn Host,
        event: &mut ScreenMouseDraggedPreEvent<'_>,
    ) -> Flow {
        maps::mouse_dragged(host, event)
    }

    fn screen_mouse_scrolled_pre(
        &self,
        host: &dyn Host,
        event: &mut ScreenMouseScrolledPreEvent<'_>,
    ) -> Flow {
        maps::mouse_scrolled(host, event)
    }

    /// The drill's screen: one fuel slot, eight output slots, two bars (see `screen`).
    fn screen(&self, _host: &dyn Host, event: &mut ScreenEvent<'_>) -> Option<NativeScreen> {
        screen::screen(event)
    }

    fn screen_draw(&self, host: &dyn Host, event: &mut ScreenDrawEvent<'_>) {
        screen::draw(host, event);
    }

    /// The drill's fuel slot takes coal only.
    fn menu_slot_rules(&self, event: &mut MenuSlotRulesEvent<'_>) {
        screen::slot_rules(event);
    }

    /// Right click with the scanner: scan, send the map data and open the map; with a compass: tell where the field is.
    fn use_item(&self, host: &mut dyn Host, event: &mut UseItemEvent<'_>) -> Flow {
        if let Flow::Cancel = scanner::use_scanner(host, event) {
            return Flow::Cancel;
        }
        compass::use_compass(host, event)
    }
}
