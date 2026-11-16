//! The messages that carry the known world to the map screen, game independent. A mod message holds at most 30,000
//! bytes, so the tiles are packed several to a message and the fields in one. Every message starts with `v,<token>`:
//! the client starts a fresh picture when the token changes, so a new right click never mixes with the old data.
//!
//! Tiles: after the header, for each tile a line `tx,tz` and the tile text (see `tiles`).
//! Fields: after the header, the field lines of `map::encode`.
//! Everything a client receives is untrusted: bad parts are skipped, nothing is trusted for its length.
use crate::map::{self, Entry};
use crate::tiles::{self, TileId};

/// The channels (`mod_id:name`): tiles, fields, and the message that opens the map screen (`token,x,z` of the player).
pub const TILES_CHANNEL: &str = "factory:tiles";
pub const FIELDS_CHANNEL: &str = "factory:fields";
pub const OPEN_CHANNEL: &str = "factory:open";

/// The open message: the view token and where the player stands.
pub fn pack_open(token: u64, x: f64, z: f64) -> String {
    format!("{token},{x:.1},{z:.1}")
}

/// Read the open message: token, x, z.
pub fn unpack_open(text: &str) -> Option<(u64, f64, f64)> {
    let mut p = text.split(',');
    let token = p.next()?.trim().parse().ok()?;
    let x: f64 = p.next()?.trim().parse().ok()?;
    let z: f64 = p.next()?.trim().parse().ok()?;
    (p.next().is_none() && x.is_finite() && z.is_finite()).then_some((token, x, z))
}

/// The most bytes put in one message (the game's cap is 30,000).
pub const MESSAGE_BYTES: usize = 28_000;
/// The most fields sent to one client.
pub const MAX_FIELDS: usize = 600;

fn header(token: u64) -> String {
    format!("v,{token}\n")
}

fn read_header(text: &str) -> Option<(u64, &str)> {
    let (first, rest) = text.split_once('\n')?;
    let token = first.strip_prefix("v,")?.trim().parse().ok()?;
    Some((token, rest))
}

/// Pack tiles into as few messages as fit.
pub fn pack_tiles(token: u64, tiles: &[(TileId, Vec<u8>)]) -> Vec<String> {
    let mut messages = Vec::new();
    let mut current = header(token);
    let mut count = 0;
    for (id, classes) in tiles {
        let part = format!("{},{}\n{}\n", id.0, id.1, tiles::encode_tile(classes));
        if count > 0 && current.len() + part.len() > MESSAGE_BYTES {
            messages.push(std::mem::replace(&mut current, header(token)));
            count = 0;
        }
        current.push_str(&part);
        count += 1;
    }
    if count > 0 {
        messages.push(current);
    }
    messages
}

/// Read a tile message: its token and the valid tiles in it.
pub fn unpack_tiles(text: &str) -> Option<(u64, Vec<(TileId, Vec<u8>)>)> {
    let (token, rest) = read_header(text)?;
    let mut lines = rest.lines();
    let mut out = Vec::new();
    while let Some(id_line) = lines.next() {
        let Some(body) = lines.next() else { break };
        let mut p = id_line.split(',');
        let (Some(x), Some(z)) = (
            p.next().and_then(|v| v.trim().parse().ok()),
            p.next().and_then(|v| v.trim().parse().ok()),
        ) else {
            continue;
        };
        if let Some(classes) = tiles::decode_tile(body) {
            out.push(((x, z), classes));
        }
    }
    Some((token, out))
}

/// The field message: at most `MAX_FIELDS` fields, the nearest to (`x`, `z`) first when more are known.
pub fn pack_fields(token: u64, entries: &[Entry], x: i32, z: i32) -> String {
    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by_key(|e| i64::from(e.x - x).pow(2) + i64::from(e.z - z).pow(2));
    let kept: Vec<Entry> = sorted.into_iter().take(MAX_FIELDS).cloned().collect();
    let mut text = header(token);
    text.push_str(&map::encode(&kept));
    text
}

pub fn unpack_fields(text: &str) -> Option<(u64, Vec<Entry>)> {
    let (token, rest) = read_header(text)?;
    Some((token, map::decode(rest)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drill::Resource;
    use crate::tiles::TILE;

    fn tile(class: u8) -> Vec<u8> {
        vec![class; TILE * TILE]
    }

    #[test]
    fn tiles_round_trip_and_split_into_messages_under_the_cap() {
        let tiles: Vec<(TileId, Vec<u8>)> = (0..20)
            .map(|i| ((i, -i), tile((i % 5) as u8 + 1)))
            .collect();
        let messages = pack_tiles(7, &tiles);
        assert!(messages.len() > 1);
        assert!(messages.iter().all(|m| m.len() <= MESSAGE_BYTES));
        let mut back = Vec::new();
        for m in &messages {
            let (token, part) = unpack_tiles(m).unwrap();
            assert_eq!(token, 7);
            back.extend(part);
        }
        assert_eq!(back, tiles);
        assert!(pack_tiles(1, &[]).is_empty());
    }

    #[test]
    fn the_open_message_round_trips_and_refuses_junk() {
        assert_eq!(
            unpack_open(&pack_open(4, -42.04, 70.5)),
            Some((4, -42.0, 70.5))
        );
        assert!(unpack_open("").is_none());
        assert!(unpack_open("1,2").is_none());
        assert!(unpack_open("1,NaN,3").is_none());
        assert!(unpack_open("1,2,3,4").is_none());
    }

    #[test]
    fn bad_tile_messages_give_nothing_or_skip_the_bad_part() {
        assert!(unpack_tiles("").is_none());
        assert!(unpack_tiles("hello\n").is_none());
        assert!(unpack_tiles("v,abc\n").is_none());
        let good = pack_tiles(3, &[((0, 0), tile(2))]).remove(0);
        let damaged = format!("{good}x,y\n{}\n1,1\nshort\n", "1".repeat(10));
        let (_, tiles) = unpack_tiles(&damaged).unwrap();
        assert_eq!(tiles.len(), 1);
    }

    fn entry(x: i32, z: i32) -> Entry {
        Entry {
            resource: Resource::Iron,
            x,
            z,
            richest: 4,
            seen: 9,
            units: 100,
        }
    }

    #[test]
    fn fields_round_trip_and_the_nearest_survive_the_cap() {
        let entries: Vec<Entry> = (0..(MAX_FIELDS as i32 + 50))
            .map(|i| entry(i * 10, 0))
            .collect();
        let text = pack_fields(5, &entries, 0, 0);
        assert!(text.len() < MESSAGE_BYTES);
        let (token, back) = unpack_fields(&text).unwrap();
        assert_eq!(token, 5);
        assert_eq!(back.len(), MAX_FIELDS);
        assert!(back.iter().any(|e| e.x == 0));
        assert!(
            !back.iter().any(|e| e.x == (MAX_FIELDS as i32 + 49) * 10),
            "the farthest are dropped"
        );
        assert!(unpack_fields("nope").is_none());
    }
}
