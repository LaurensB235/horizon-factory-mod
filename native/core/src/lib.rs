//! Game-independent logic of the Factory mod. No Horizon types here, so `cargo test` needs no game.
//! The hook glue in `../api` calls into this crate.
pub mod blocks;
pub mod drill;
pub mod layout;
pub mod map;
pub mod scan;
pub mod terrain;
pub mod tiles;
pub mod view;
pub mod wire;
