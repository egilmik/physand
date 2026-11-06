//! Physand gameplay: the alchemist, spells, camera and frame composition.
//! Pure Rust on top of `physand-sim`; the web crate is a thin binding.

pub mod game;
pub mod input;
pub mod player;
pub mod render;
pub mod spell;

pub use game::{Game, VIEW_H, VIEW_W, WORLD_H, WORLD_W};
pub use input::Input;

#[cfg(test)]
mod tests;
