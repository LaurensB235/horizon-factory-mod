//! The resource scanner: the item `horizon_factory:scanner`. Right click scans around the player (updating the map
//! database), sends the client everything known nearby and opens the full-screen map (see `map` and `maps`). It says
//! nothing in chat and shows no distances: the map is the whole interface.
use crate::map;
use horizon_client::mod_api::{Flow, Host, ItemStack, Text, UseItemEvent};

pub const SCANNER: &str = "horizon_factory:scanner";

pub(crate) fn is_scanner(stack: &ItemStack) -> bool {
    &*stack.item == SCANNER
}

/// Right click with the scanner. Always claims the use of a scanner.
pub fn use_scanner(host: &mut dyn Host, event: &mut UseItemEvent<'_>) -> Flow {
    if !is_scanner(event.stack) {
        return Flow::Continue;
    }
    let Some(player) = host.acting_player() else {
        return Flow::Cancel;
    };
    if !map::scan_player(host, &player) {
        host.tell(
            player.uuid,
            Text::literal("The scanner only works in the overworld."),
        );
        return Flow::Cancel;
    }
    map::send_view(host, &player);
    Flow::Cancel
}
