//! The resource compass: a vanilla compass marked with `minecraft:custom_data` (`factory_compass` = the resource it
//! looks for), one recipe per resource. Right click re-scans the area and tells the player, in chat, how far and in
//! which direction the nearest field of that resource is. It never changes the item: the game refuses inventory
//! edits from inside a player's own action, so a needle that moves (a changing stack) is not possible this way.
//! The rules are in `factory_core::scan`; the sampling is shared with the scanner.
use crate::sampling;
use factory_core::drill::Resource;
use factory_core::scan;
use horizon_client::mod_api::{Flow, Host, ItemStack, Tag, Text, UseItemEvent};

const COMPASS: &str = "minecraft:compass";
const MARKER: &str = "minecraft:custom_data";
const KEY: &str = "factory_compass";

/// The resource a marked compass looks for, or `None` when the stack is not one of ours.
fn compass_resource(stack: &ItemStack) -> Option<Resource> {
    if &*stack.item != COMPASS {
        return None;
    }
    let data = stack.extra.as_ref()?;
    let Tag::Compound(map) = data.other(MARKER)? else {
        return None;
    };
    map.iter()
        .find(|(k, _)| k.to_string_lossy() == KEY)
        .and_then(|(_, tag)| match tag {
            Tag::String(s) => scan::parse_resource(&s.to_string_lossy()),
            _ => None,
        })
}

/// Right click with a resource compass in the main hand. Always claims the use of a compass of ours.
pub fn use_compass(host: &mut dyn Host, event: &mut UseItemEvent<'_>) -> Flow {
    let Some(resource) = compass_resource(event.stack) else {
        return Flow::Continue;
    };
    let Some(player) = host.acting_player() else {
        return Flow::Cancel;
    };
    let line = match sampling::sample_area(host, &player) {
        Err(why) => format!("The compass could not read the world: {why}"),
        Ok(area) => {
            let fields = scan::find_fields(&area.samples);
            scan::compass_line(resource, scan::nearest_of(&fields, resource))
        }
    };
    host.tell(player.uuid, Text::literal(line));
    Flow::Cancel
}
