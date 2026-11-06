//! Physand gameplay: the alchemist, spells, camera and frame composition.
//! Pure Rust on top of `physand-sim`; the web crate is a thin binding.

pub mod body;
pub mod distillery;
pub mod enemy;
pub mod game;
pub mod input;
pub mod player;
pub mod render;
pub mod run;
pub mod spell;
pub mod wyrm;

pub use game::{Game, Phase, Stage, VIEW_H, VIEW_W, WORLD_H, WORLD_W};
pub use input::Input;

#[cfg(test)]
mod tests;
