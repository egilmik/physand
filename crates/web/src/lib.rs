//! Browser bindings. JS owns the frame loop, input capture and WebGL; each
//! frame it pushes input, advances fixed ticks, asks Rust to compose the
//! view, then uploads the view and overlay buffers straight from wasm memory.

use physand_game::{player, Game, Input, VIEW_H, VIEW_W};
use physand_sim::MATERIALS;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Physand {
    game: Game,
}

#[wasm_bindgen]
impl Physand {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u32) -> Physand {
        Physand {
            game: Game::new(seed),
        }
    }

    /// Generate a fresh level.
    pub fn reset(&mut self, seed: u32) {
        self.game = Game::new(seed);
    }

    /// Input bitmask (see `Input::LEFT` etc.) and aim in *view* cell coordinates.
    pub fn set_input(&mut self, bits: u32, aim_view_x: f32, aim_view_y: f32) {
        let (ax, ay) = self.game.view_to_world(aim_view_x, aim_view_y);
        self.game.set_input(Input::from_bits(bits, ax, ay));
    }

    /// Advance `steps` fixed ticks.
    pub fn tick(&mut self, steps: u32) {
        for _ in 0..steps {
            self.game.step();
        }
    }

    /// Compose the current view into the view and overlay buffers.
    pub fn render(&mut self) {
        self.game.render();
    }

    pub fn view_width(&self) -> u32 {
        VIEW_W
    }

    pub fn view_height(&self) -> u32 {
        VIEW_H
    }

    /// `VIEW_W * VIEW_H` cells of `[mat, var, life, clock]`.
    pub fn view_ptr(&self) -> *const u8 {
        self.game.view_cells().as_ptr() as *const u8
    }

    /// `VIEW_W * VIEW_H` RGBA8 overlay pixels (sprites, particles, UI marks).
    pub fn overlay_ptr(&self) -> *const u8 {
        self.game.overlay().as_ptr() as *const u8
    }

    pub fn camera_x(&self) -> i32 {
        self.game.camera().0
    }

    pub fn camera_y(&self) -> i32 {
        self.game.camera().1
    }

    /// Sandbox brush, in view coordinates.
    pub fn paint(&mut self, view_x: i32, view_y: i32, radius: i32, mat: u8) {
        self.game.paint_view(view_x, view_y, radius, mat);
    }

    // --- HUD -------------------------------------------------------------

    pub fn hp(&self) -> f32 {
        self.game.player.hp
    }

    pub fn max_hp(&self) -> f32 {
        player::MAX_HP
    }

    pub fn stamina(&self) -> f32 {
        self.game.player.stamina
    }

    pub fn max_stamina(&self) -> f32 {
        player::MAX_STAMINA
    }

    pub fn flask_volume(&self) -> f32 {
        self.game.flask.volume
    }

    pub fn flask_capacity(&self) -> f32 {
        self.game.flask.capacity
    }

    pub fn flask_name(&self) -> String {
        self.game.flask.name.to_string()
    }

    pub fn flask_material(&self) -> u8 {
        self.game.flask.material
    }

    pub fn is_dead(&self) -> bool {
        !self.game.player.alive()
    }

    pub fn deaths(&self) -> u32 {
        self.game.deaths
    }

    // --- Debug -------------------------------------------------------------

    pub fn player_x(&self) -> i32 {
        self.game.player.x
    }

    pub fn player_y(&self) -> i32 {
        self.game.player.y
    }

    pub fn count(&self, mat: u8) -> u32 {
        self.game.world.count(mat) as u32
    }

    pub fn awake_chunks(&self) -> u32 {
        self.game.world.awake_chunks() as u32
    }

    pub fn chunk_count(&self) -> u32 {
        self.game.world.chunk_count() as u32
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
