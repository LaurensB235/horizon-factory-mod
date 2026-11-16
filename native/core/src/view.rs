//! The navigable full-screen map, game independent: the view (where the map is centred and how far it is zoomed),
//! panning, zooming at the cursor, picking the field under the cursor and the text of the hover tooltip.
//!
//! Positions are world blocks (x east, z south); screen positions are GUI pixels from the top left. `scale` is GUI pixels
//! per block.
use crate::drill::Resource;
use crate::map::Entry;
use crate::scan;

pub const MIN_SCALE: f64 = 0.3;
pub const MAX_SCALE: f64 = 8.0;
/// How near the cursor must be to a field marker (GUI pixels) to pick it.
pub const PICK_RADIUS: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// The world position at the middle of the screen.
    pub x: f64,
    pub z: f64,
    pub scale: f64,
}

impl View {
    pub fn new(x: f64, z: f64, scale: f64) -> Self {
        Self {
            x,
            z,
            scale: scale.clamp(MIN_SCALE, MAX_SCALE),
        }
    }

    /// Where a world position falls on a screen of `width` x `height` GUI pixels.
    pub fn to_screen(&self, width: f32, height: f32, wx: f64, wz: f64) -> (f32, f32) {
        (
            width / 2.0 + ((wx - self.x) * self.scale) as f32,
            height / 2.0 + ((wz - self.z) * self.scale) as f32,
        )
    }

    /// The world position under a screen position.
    pub fn to_world(&self, width: f32, height: f32, sx: f32, sy: f32) -> (f64, f64) {
        (
            self.x + f64::from(sx - width / 2.0) / self.scale,
            self.z + f64::from(sy - height / 2.0) / self.scale,
        )
    }

    /// Drag the map by `dx`, `dy` GUI pixels (the world follows the cursor).
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.x -= f64::from(dx) / self.scale;
        self.z -= f64::from(dy) / self.scale;
    }

    /// Zoom by `steps` wheel notches (positive in) keeping the world position under (`sx`, `sy`) fixed.
    pub fn zoom_at(&mut self, width: f32, height: f32, sx: f32, sy: f32, steps: f64) {
        let before = self.to_world(width, height, sx, sy);
        self.scale = (self.scale * 1.25_f64.powf(steps)).clamp(MIN_SCALE, MAX_SCALE);
        let after = self.to_world(width, height, sx, sy);
        self.x += before.0 - after.0;
        self.z += before.1 - after.1;
    }
}

/// The index of the field whose marker is nearest to the cursor, within `PICK_RADIUS`.
pub fn pick(
    entries: &[Entry],
    view: &View,
    width: f32,
    height: f32,
    cx: f32,
    cy: f32,
) -> Option<usize> {
    entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let (sx, sy) = view.to_screen(width, height, f64::from(e.x), f64::from(e.z));
            (i, (sx - cx).powi(2) + (sy - cy).powi(2))
        })
        .filter(|&(_, d)| d <= PICK_RADIUS * PICK_RADIUS)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// The two hover lines of a field: its name and how much ore is left.
pub fn tooltip(entry: &Entry) -> (String, String) {
    let name = scan::resource_name(entry.resource);
    let mut title = name.to_string();
    title[..1].make_ascii_uppercase();
    let left = if entry.units > 0 {
        format!("{} left", group_digits(entry.units))
    } else {
        "not counted yet".to_string()
    };
    (format!("{title} field"), left)
}

/// `12345` as `12,345`.
pub fn group_digits(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The map colour (0xRRGGBB) of a resource's marker. Coal is black.
pub fn resource_rgb(resource: Resource) -> u32 {
    match resource {
        Resource::Coal => 0x111111,
        Resource::Iron => 0xE8C8A8,
        Resource::Copper => 0xE8805A,
    }
}

/// The outline (0xRRGGBB) around a resource's marker: black markers get a light ring so they show on dark ground,
/// the others a black one.
pub fn outline_rgb(resource: Resource) -> u32 {
    match resource {
        Resource::Coal => 0xE6E6E6,
        _ => 0x000000,
    }
}

/// The colour (0xRRGGBB) of a resource's name in the legend: its marker colour, except that black text would not read.
pub fn label_rgb(resource: Resource) -> u32 {
    match resource {
        Resource::Coal => 0xD0D0D0,
        other => resource_rgb(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channels(rgb: u32) -> [u32; 3] {
        [(rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF]
    }

    #[test]
    fn coal_is_black_with_a_light_ring_and_a_readable_label() {
        assert!(
            channels(resource_rgb(Resource::Coal))
                .iter()
                .all(|&c| c <= 0x20)
        );
        assert!(
            channels(outline_rgb(Resource::Coal))
                .iter()
                .all(|&c| c >= 0xC0)
        );
        assert!(
            channels(label_rgb(Resource::Coal))
                .iter()
                .all(|&c| c >= 0xC0)
        );
    }

    #[test]
    fn the_other_resources_keep_their_colours() {
        assert_eq!(resource_rgb(Resource::Iron), 0xE8C8A8);
        assert_eq!(resource_rgb(Resource::Copper), 0xE8805A);
        assert_eq!(outline_rgb(Resource::Iron), 0x000000);
        assert_eq!(label_rgb(Resource::Copper), 0xE8805A);
    }

    fn entry(resource: Resource, x: i32, z: i32, units: u32) -> Entry {
        Entry {
            resource,
            x,
            z,
            richest: 5,
            seen: 0,
            units,
        }
    }

    #[test]
    fn screen_and_world_positions_round_trip() {
        let v = View::new(100.0, -50.0, 2.0);
        let (sx, sy) = v.to_screen(800.0, 600.0, 110.0, -40.0);
        assert_eq!((sx, sy), (420.0, 320.0));
        let (wx, wz) = v.to_world(800.0, 600.0, sx, sy);
        assert!((wx - 110.0).abs() < 1e-9 && (wz + 40.0).abs() < 1e-9);
    }

    #[test]
    fn dragging_moves_the_world_with_the_cursor() {
        let mut v = View::new(0.0, 0.0, 2.0);
        v.pan(20.0, -10.0); // drag right and up: the world moves right and up, so the centre moves left and down
        assert_eq!((v.x, v.z), (-10.0, 5.0));
    }

    #[test]
    fn zooming_keeps_the_point_under_the_cursor_and_clamps() {
        let mut v = View::new(0.0, 0.0, 1.0);
        let before = v.to_world(800.0, 600.0, 600.0, 200.0);
        v.zoom_at(800.0, 600.0, 600.0, 200.0, 3.0);
        let after = v.to_world(800.0, 600.0, 600.0, 200.0);
        assert!(v.scale > 1.9 && v.scale < 2.0);
        assert!((before.0 - after.0).abs() < 1e-9 && (before.1 - after.1).abs() < 1e-9);
        v.zoom_at(800.0, 600.0, 400.0, 300.0, 100.0);
        assert_eq!(v.scale, MAX_SCALE);
        v.zoom_at(800.0, 600.0, 400.0, 300.0, -100.0);
        assert_eq!(v.scale, MIN_SCALE);
    }

    #[test]
    fn the_nearest_marker_within_reach_is_picked() {
        let v = View::new(0.0, 0.0, 1.0);
        let entries = vec![
            entry(Resource::Iron, 10, 0, 100),
            entry(Resource::Coal, 14, 0, 200),
            entry(Resource::Copper, 300, 300, 5),
        ];
        // cursor at screen (410, 300) is world (10, 0) on an 800 x 600 screen
        assert_eq!(pick(&entries, &v, 800.0, 600.0, 410.0, 300.0), Some(0));
        assert_eq!(pick(&entries, &v, 800.0, 600.0, 413.0, 300.0), Some(1));
        assert_eq!(pick(&entries, &v, 800.0, 600.0, 100.0, 100.0), None);
    }

    #[test]
    fn tooltip_gives_the_name_and_the_count_left() {
        assert_eq!(
            tooltip(&entry(Resource::Iron, 0, 0, 12_345)),
            ("Iron field".to_string(), "12,345 left".to_string())
        );
        assert_eq!(
            tooltip(&entry(Resource::Coal, 0, 0, 0)).1,
            "not counted yet"
        );
        assert_eq!(group_digits(7), "7");
        assert_eq!(group_digits(1000), "1,000");
        assert_eq!(group_digits(1_234_567), "1,234,567");
    }
}
