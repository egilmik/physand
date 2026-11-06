//! Browser bindings. The JS side owns the game loop, input and WebGL
//! rendering; it reads the cell grid straight out of wasm memory.

use physand_sim::{worldgen, World, MATERIALS};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Game {
    world: World,
}

#[wasm_bindgen]
impl Game {
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32, seed: u32) -> Game {
        Game {
            world: worldgen::demo_scene(width, height, seed),
        }
    }

    pub fn width(&self) -> u32 {
        self.world.width()
    }

    pub fn height(&self) -> u32 {
        self.world.height()
    }

    /// Advance the simulation by `steps` fixed ticks.
    pub fn tick(&mut self, steps: u32) {
        for _ in 0..steps {
            self.world.step();
        }
    }

    pub fn paint(&mut self, x: i32, y: i32, radius: i32, mat: u8) {
        self.world.paint(x, y, radius, mat);
    }

    /// Regenerate the demo scene with a new seed.
    pub fn reset(&mut self, seed: u32) {
        self.world = worldgen::demo_scene(self.world.width(), self.world.height(), seed);
    }

    pub fn clear(&mut self) {
        self.world.clear();
    }

    /// Pointer to `width * height` cells of 4 bytes `[mat, var, life, clock]`.
    pub fn cells_ptr(&self) -> *const u8 {
        self.world.cells().as_ptr() as *const u8
    }

    pub fn count(&self, mat: u8) -> u32 {
        self.world.count(mat) as u32
    }
}

#[wasm_bindgen]
pub fn material_count() -> u32 {
    MATERIALS.len() as u32
}

#[wasm_bindgen]
pub fn material_name(mat: u8) -> String {
    MATERIALS
        .get(mat as usize)
        .map_or("?", |m| m.name)
        .to_string()
}

/// Base colour packed as `0xRRGGBB`.
#[wasm_bindgen]
pub fn material_color(mat: u8) -> u32 {
    MATERIALS.get(mat as usize).map_or(0, |m| {
        let [r, g, b] = m.color;
        (r as u32) << 16 | (g as u32) << 8 | b as u32
    })
}
