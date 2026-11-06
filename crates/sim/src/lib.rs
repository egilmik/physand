//! Physand simulation core: a deterministic falling-sand cellular automaton.
//!
//! This crate has no web dependencies so it can be unit-tested natively.

pub mod material;
pub mod rng;
pub mod world;
pub mod worldgen;

pub use material::{id, Kind, MATERIALS};
pub use world::{Cell, World};

#[cfg(test)]
mod tests;
