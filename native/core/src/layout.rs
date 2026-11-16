//! The 3x3x3 drill multiblock: which cells exist, their `cell` state names, and placement rules.
//! The controller is the front-centre of the bottom layer, cell (1, 0, 0): the only block with a face a player can reach (the front).
//! The cells are described for a drill whose front faces north; a `Facing` turns the whole drill about its controller, so the
//! front can face any of the four horizontal directions (it is placed facing the player).

pub type Pos = (i32, i32, i32);

pub const SIZE: i32 = 3;
pub const CONTROLLER_CELL: (i32, i32, i32) = (1, 0, 0);

/// All 27 cell indices (x, y, z), each 0..3, in a fixed order (y, then x, then z).
pub fn cells() -> Vec<(i32, i32, i32)> {
    let mut v = Vec::with_capacity(27);
    for y in 0..SIZE {
        for x in 0..SIZE {
            for z in 0..SIZE {
                v.push((x, y, z));
            }
        }
    }
    v
}

/// The state value of a `drill_part` cell, for example `x0_y2_z1`. `None` for the controller cell.
pub fn cell_name(cell: (i32, i32, i32)) -> Option<String> {
    if cell == CONTROLLER_CELL {
        None
    } else {
        Some(format!("x{}_y{}_z{}", cell.0, cell.1, cell.2))
    }
}

/// Parse a `cell` state value back to its indices.
pub fn parse_cell(name: &str) -> Option<(i32, i32, i32)> {
    let mut it = name.split('_');
    let (x, y, z) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() {
        return None;
    }
    let idx = |s: &str, p: char| -> Option<i32> {
        let n: i32 = s.strip_prefix(p)?.parse().ok()?;
        (0..SIZE).contains(&n).then_some(n)
    };
    let cell = (idx(x, 'x')?, idx(y, 'y')?, idx(z, 'z')?);
    (cell != CONTROLLER_CELL).then_some(cell)
}

/// The way the drill's front (the controller's face, with the fuel hopper) points. `North` is how the cells are described;
/// the others turn the whole drill clockwise (seen from above) about the controller: east one quarter, south two, west three.
/// The name is the game's `facing` value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Facing {
    #[default]
    North,
    East,
    South,
    West,
}

impl Facing {
    pub const ALL: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];

    /// The value of the game's `facing` property.
    pub fn name(self) -> &'static str {
        match self {
            Facing::North => "north",
            Facing::East => "east",
            Facing::South => "south",
            Facing::West => "west",
        }
    }

    /// The horizontal facing named by a `facing` property value; `None` for anything else (`up`, `down`, garbage).
    pub fn parse(name: &str) -> Option<Facing> {
        Facing::ALL.into_iter().find(|f| f.name() == name)
    }

    /// Clockwise quarter turns from north (0 to 3), also the blockstate `y` rotation divided by 90.
    pub fn quarter_turns(self) -> u32 {
        match self {
            Facing::North => 0,
            Facing::East => 1,
            Facing::South => 2,
            Facing::West => 3,
        }
    }

    /// The facing that points the front at a player looking along `yaw` (the game's: 0 looks south, 90 west, 180 north,
    /// 270 east): the drill's front is toward the player and its body extends away from them. Looking along a diagonal
    /// picks north or south. A yaw that is not a number gives north.
    pub fn toward_looker(yaw: f32) -> Facing {
        let r = yaw.to_radians();
        // the front points the other way from the look direction (-sin, cos)
        let (fx, fz) = (r.sin(), -r.cos());
        if fx.abs() > fz.abs() {
            if fx > 0.0 {
                Facing::East
            } else {
                Facing::West
            }
        } else if fz > 0.0 {
            Facing::South
        } else {
            Facing::North
        }
    }

    /// A horizontal offset (x east, z south) turned from the north description to this facing.
    fn turn(self, (x, z): (i32, i32)) -> (i32, i32) {
        (0..self.quarter_turns()).fold((x, z), |(x, z), _| (-z, x))
    }
}

/// How far `cell` is from the controller on the horizontal plane, in the north description (x east, z south).
fn offset(cell: (i32, i32, i32)) -> (i32, i32) {
    (cell.0 - CONTROLLER_CELL.0, cell.2 - CONTROLLER_CELL.2)
}

/// World position of `cell` when the controller stands at `controller` and the front faces `facing`.
pub fn cell_pos(controller: Pos, cell: (i32, i32, i32), facing: Facing) -> Pos {
    let (dx, dz) = facing.turn(offset(cell));
    (
        controller.0 + dx,
        controller.1 + cell.1 - CONTROLLER_CELL.1,
        controller.2 + dz,
    )
}

/// The controller position for a part at `pos` whose `cell` state is `cell` and whose `facing` state is `facing`.
pub fn controller_of(pos: Pos, cell: (i32, i32, i32), facing: Facing) -> Pos {
    let (dx, dz) = facing.turn(offset(cell));
    (pos.0 - dx, pos.1 - (cell.1 - CONTROLLER_CELL.1), pos.2 - dz)
}

#[derive(Debug, PartialEq, Eq)]
pub enum PlaceError {
    /// A cell is not air or replaceable.
    Blocked(Pos),
    /// A block under the footprint is not solid.
    NoFloor(Pos),
}

/// What the placement rules need to know about the world.
pub trait WorldView {
    fn is_replaceable(&self, pos: Pos) -> bool;
    fn is_solid(&self, pos: Pos) -> bool;
}

/// Check that the drill fits with its controller at `controller` and its front facing `facing`: all 27 cells free,
/// all 9 blocks under the bottom layer solid (patch blocks count as solid).
pub fn validate(world: &dyn WorldView, controller: Pos, facing: Facing) -> Result<(), PlaceError> {
    for cell in cells() {
        let p = cell_pos(controller, cell, facing);
        if !world.is_replaceable(p) {
            return Err(PlaceError::Blocked(p));
        }
    }
    for x in 0..SIZE {
        for z in 0..SIZE {
            let below = cell_pos(controller, (x, 0, z), facing);
            let below = (below.0, below.1 - 1, below.2);
            if !world.is_solid(below) {
                return Err(PlaceError::NoFloor(below));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    struct Flat {
        ground_y: i32,
        blocked: Option<Pos>,
        hole: Option<Pos>,
    }
    impl WorldView for Flat {
        fn is_replaceable(&self, p: Pos) -> bool {
            p.1 > self.ground_y && Some(p) != self.blocked
        }
        fn is_solid(&self, p: Pos) -> bool {
            p.1 <= self.ground_y && Some(p) != self.hole
        }
    }

    #[test]
    fn has_27_distinct_cells_and_26_part_names() {
        let c = cells();
        assert_eq!(c.len(), 27);
        assert_eq!(c.iter().collect::<HashSet<_>>().len(), 27);
        let names: HashSet<String> = c.iter().filter_map(|&x| cell_name(x)).collect();
        assert_eq!(names.len(), 26);
        assert!(cell_name(CONTROLLER_CELL).is_none());
    }

    #[test]
    fn names_round_trip() {
        for c in cells() {
            match cell_name(c) {
                Some(n) => assert_eq!(parse_cell(&n), Some(c)),
                None => assert_eq!(c, CONTROLLER_CELL),
            }
        }
        assert_eq!(parse_cell("x1_y0_z0"), None); // the controller is not a part
        assert_eq!(parse_cell("x3_y0_z0"), None);
        assert_eq!(parse_cell("x0_y0"), None);
        assert_eq!(parse_cell("x0_y0_z0_z0"), None);
        assert_eq!(parse_cell("garbage"), None);
    }

    #[test]
    fn controller_is_recovered_from_every_part() {
        let controller = (100, 64, -20);
        for f in Facing::ALL {
            for c in cells() {
                assert_eq!(controller_of(cell_pos(controller, c, f), c, f), controller);
            }
            assert_eq!(cell_pos(controller, CONTROLLER_CELL, f), controller);
        }
    }

    #[test]
    fn footprint_spans_3x3x3_behind_the_controller() {
        let ctrl = (0, 10, 0);
        let ps: Vec<Pos> = cells()
            .into_iter()
            .map(|c| cell_pos(ctrl, c, Facing::North))
            .collect();
        assert_eq!(ps.iter().map(|p| p.0).min(), Some(-1));
        assert_eq!(ps.iter().map(|p| p.0).max(), Some(1));
        assert_eq!(ps.iter().map(|p| p.1).min(), Some(10));
        assert_eq!(ps.iter().map(|p| p.1).max(), Some(12));
        assert_eq!(ps.iter().map(|p| p.2).min(), Some(0)); // the controller is at the front (north) of the footprint
        assert_eq!(ps.iter().map(|p| p.2).max(), Some(2));
    }

    /// The corners of the footprint, as (min x, max x, min z, max z).
    fn bounds(ctrl: Pos, facing: Facing) -> (i32, i32, i32, i32) {
        let ps: Vec<Pos> = cells()
            .into_iter()
            .map(|c| cell_pos(ctrl, c, facing))
            .collect();
        (
            ps.iter().map(|p| p.0).min().unwrap(),
            ps.iter().map(|p| p.0).max().unwrap(),
            ps.iter().map(|p| p.2).min().unwrap(),
            ps.iter().map(|p| p.2).max().unwrap(),
        )
    }

    #[test]
    fn every_facing_fills_a_3x3x3_block_with_the_body_behind_the_front() {
        let ctrl = (0, 10, 0);
        // the body extends away from the front: front north -> body south (+z), east -> west (-x), south -> north, west -> east
        assert_eq!(bounds(ctrl, Facing::North), (-1, 1, 0, 2));
        assert_eq!(bounds(ctrl, Facing::East), (-2, 0, -1, 1));
        assert_eq!(bounds(ctrl, Facing::South), (-1, 1, -2, 0));
        assert_eq!(bounds(ctrl, Facing::West), (0, 2, -1, 1));
        for f in Facing::ALL {
            let ps: std::collections::HashSet<Pos> =
                cells().into_iter().map(|c| cell_pos(ctrl, c, f)).collect();
            assert_eq!(ps.len(), 27, "{f:?}");
        }
    }

    #[test]
    fn the_middle_column_follows_the_turn() {
        let ctrl = (5, 0, 5);
        assert_eq!(cell_pos(ctrl, (1, 0, 1), Facing::North), (5, 0, 6));
        assert_eq!(cell_pos(ctrl, (1, 0, 1), Facing::East), (4, 0, 5));
        assert_eq!(cell_pos(ctrl, (1, 0, 1), Facing::South), (5, 0, 4));
        assert_eq!(cell_pos(ctrl, (1, 0, 1), Facing::West), (6, 0, 5));
    }

    #[test]
    fn a_turned_drill_keeps_its_left_and_right() {
        // north: cell x0 is to the west of the controller; turned east it must be to the north (clockwise quarter turn)
        let ctrl = (0, 0, 0);
        assert_eq!(cell_pos(ctrl, (0, 0, 0), Facing::North), (-1, 0, 0));
        assert_eq!(cell_pos(ctrl, (0, 0, 0), Facing::East), (0, 0, -1));
        assert_eq!(cell_pos(ctrl, (0, 0, 0), Facing::South), (1, 0, 0));
        assert_eq!(cell_pos(ctrl, (0, 0, 0), Facing::West), (0, 0, 1));
    }

    #[test]
    fn facing_names_round_trip_and_reject_other_values() {
        for f in Facing::ALL {
            assert_eq!(Facing::parse(f.name()), Some(f));
        }
        assert_eq!(Facing::parse("up"), None);
        assert_eq!(Facing::parse("down"), None);
        assert_eq!(Facing::parse("North"), None);
        assert_eq!(Facing::default(), Facing::North);
    }

    #[test]
    fn the_front_faces_the_player() {
        // yaw 0 looks south, so the player stands north of the drill and the front faces north
        assert_eq!(Facing::toward_looker(0.0), Facing::North);
        assert_eq!(Facing::toward_looker(90.0), Facing::East); // looking west: the player is east of it
        assert_eq!(Facing::toward_looker(180.0), Facing::South);
        assert_eq!(Facing::toward_looker(-90.0), Facing::West); // 270 is the same as -90: looking east
        assert_eq!(Facing::toward_looker(270.0), Facing::West);
        // a little off the axis still picks that axis
        assert_eq!(Facing::toward_looker(30.0), Facing::North);
        assert_eq!(Facing::toward_looker(-30.0), Facing::North);
        assert_eq!(Facing::toward_looker(60.0), Facing::East);
        assert_eq!(Facing::toward_looker(150.0), Facing::South);
        // a full turn or more gives the same answer
        assert_eq!(Facing::toward_looker(360.0), Facing::North);
        assert_eq!(Facing::toward_looker(-180.0), Facing::South);
        // not a number: the old behaviour
        assert_eq!(Facing::toward_looker(f32::NAN), Facing::North);
    }

    #[test]
    fn the_floor_is_checked_under_the_turned_footprint() {
        let w = Flat {
            ground_y: 9,
            blocked: None,
            hole: Some((-2, 9, 0)),
        };
        // facing east the footprint covers x -2..=0, z -1..=1, so the hole at (-2, 9, 0) is under it
        assert!(matches!(
            validate(&w, (0, 10, 0), Facing::East),
            Err(PlaceError::NoFloor((-2, 9, 0)))
        ));
        // facing north it covers x -1..=1, z 0..=2: the hole is outside, the drill fits
        assert_eq!(validate(&w, (0, 10, 0), Facing::North), Ok(()));
    }

    #[test]
    fn fits_on_flat_ground() {
        let w = Flat {
            ground_y: 9,
            blocked: None,
            hole: None,
        };
        assert_eq!(validate(&w, (0, 10, 0), Facing::North), Ok(()));
    }

    #[test]
    fn a_blocked_cell_is_reported() {
        let w = Flat {
            ground_y: 9,
            blocked: Some((1, 12, 1)),
            hole: None,
        };
        assert_eq!(
            validate(&w, (0, 10, 0), Facing::North),
            Err(PlaceError::Blocked((1, 12, 1)))
        );
    }

    #[test]
    fn a_missing_floor_block_is_reported() {
        let w = Flat {
            ground_y: 9,
            blocked: None,
            hole: Some((-1, 9, 1)),
        };
        assert_eq!(
            validate(&w, (0, 10, 0), Facing::North),
            Err(PlaceError::NoFloor((-1, 9, 1)))
        );
    }

    #[test]
    fn floating_in_the_air_is_rejected() {
        let w = Flat {
            ground_y: 5,
            blocked: None,
            hole: None,
        };
        assert!(matches!(
            validate(&w, (0, 10, 0), Facing::North),
            Err(PlaceError::NoFloor(_))
        ));
    }
}
