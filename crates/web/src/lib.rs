//! Browser bindings. JS owns the frame loop, input capture and WebGL; each
//! frame it pushes input, advances fixed ticks, asks Rust to compose the
//! view, then uploads the view and overlay buffers straight from wasm memory.

use physand_game::{
    player,
    spell::{Catalyst, Flask, Form},
    Game, Input, VIEW_H, VIEW_W,
};
use physand_sim::{material::reagents, MATERIALS};
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

    /// Start a new run (floor 1) with this seed.
    pub fn reset(&mut self, seed: u32) {
        self.game = Game::new(seed);
    }

    // --- Run state -----------------------------------------------------------

    /// 0 = playing, 1 = dead (run over), 2 = floor cleared.
    pub fn phase(&self) -> u8 {
        self.game.phase.code()
    }

    /// Enter the next floor after clearing this one. Returns `false` otherwise.
    pub fn descend(&mut self) -> bool {
        self.game.descend()
    }

    pub fn depth(&self) -> u32 {
        self.game.depth
    }

    pub fn kills(&self) -> u32 {
        self.game.kills
    }

    /// Seconds played this run (60 ticks per second).
    pub fn run_seconds(&self) -> f32 {
        self.game.run_ticks as f32 / 60.0
    }

    pub fn floor_seconds(&self) -> f32 {
        self.game.floor_ticks as f32 / 60.0
    }

    pub fn enemies_alive(&self) -> u32 {
        self.game.enemies_alive() as u32
    }

    pub fn seal_open(&self) -> bool {
        self.game.seal_open()
    }

    /// World position the compass points at (seal, then exit); NaN if none.
    pub fn objective_x(&self) -> f32 {
        self.game.objective().map_or(f32::NAN, |o| o.0)
    }

    pub fn objective_y(&self) -> f32 {
        self.game.objective().map_or(f32::NAN, |o| o.1)
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

    // --- Flasks ---------------------------------------------------------------

    pub fn flask_count(&self) -> u32 {
        self.game.flasks.len() as u32
    }

    pub fn active_flask(&self) -> u32 {
        self.game.active_flask() as u32
    }

    pub fn select_flask(&mut self, index: u32) {
        self.game.select_flask(index as usize);
    }

    pub fn cycle_flask(&mut self, delta: i32) {
        self.game.cycle_flask(delta);
    }

    pub fn flask_name(&self, index: u32) -> String {
        self.flask(index).map_or_else(String::new, |f| f.name())
    }

    pub fn flask_volume(&self, index: u32) -> f32 {
        self.flask(index).map_or(0.0, |f| f.volume)
    }

    pub fn flask_capacity(&self, index: u32) -> f32 {
        self.flask(index).map_or(1.0, |f| f.capacity)
    }

    pub fn flask_cost(&self, index: u32) -> f32 {
        self.flask(index).map_or(0.0, |f| f.cost())
    }

    /// Colour of the flask's first reagent, `0xRRGGBB`.
    pub fn flask_color(&self, index: u32) -> u32 {
        self.flask(index).map_or(0, |f| pack(f.color()))
    }

    /// Reagent in `slot` (0..3), or 255 if empty.
    pub fn flask_reagent(&self, index: u32, slot: u32) -> u8 {
        self.flask(index)
            .and_then(|f| f.reagents().get(slot as usize).copied())
            .unwrap_or(255)
    }

    /// Index into `form_name`.
    pub fn flask_form(&self, index: u32) -> u8 {
        self.flask(index)
            .and_then(|f| Form::ALL.iter().position(|&x| x == f.form()))
            .unwrap_or(0) as u8
    }

    /// Catalyst in `slot` (0..2) as an index into `catalyst_name`, or 255.
    pub fn flask_catalyst(&self, index: u32, slot: u32) -> u8 {
        self.flask(index)
            .and_then(|f| f.catalysts().get(slot as usize))
            .and_then(|c| Catalyst::ALL.iter().position(|x| x == c))
            .map_or(255, |i| i as u8)
    }

    /// Set a flask's recipe. Slots use 255 for "empty". Returns `false` if invalid.
    #[allow(clippy::too_many_arguments)]
    pub fn configure_flask(
        &mut self,
        index: u32,
        r0: u8,
        r1: u8,
        r2: u8,
        form: u8,
        c0: u8,
        c1: u8,
    ) -> bool {
        self.game
            .configure_flask(index as usize, &[r0, r1, r2], form, &[c0, c1])
    }

    pub fn is_dead(&self) -> bool {
        !self.game.player.alive()
    }

    pub fn deaths(&self) -> u32 {
        self.game.deaths
    }

    // --- Debug -------------------------------------------------------------

    /// Debug: teleport next to the floor's seal.
    pub fn debug_warp_to_seal(&mut self) {
        self.game.debug_warp_to_seal();
    }

    /// Debug: end the run.
    pub fn debug_kill(&mut self) {
        self.game.player.kill();
    }

    pub fn player_x(&self) -> i32 {
        self.game.player.x
    }

    pub fn player_y(&self) -> i32 {
        self.game.player.y
    }

    pub fn particle_count(&self) -> u32 {
        self.game.particle_count() as u32
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

impl Physand {
    fn flask(&self, index: u32) -> Option<&Flask> {
        self.game.flasks.get(index as usize)
    }
}

fn pack([r, g, b]: [u8; 3]) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

/// Material ids that can be loaded into a flask.
#[wasm_bindgen]
pub fn reagent_ids() -> Vec<u8> {
    reagents().collect()
}

#[wasm_bindgen]
pub fn form_count() -> u32 {
    Form::ALL.len() as u32
}

#[wasm_bindgen]
pub fn form_name(i: u32) -> String {
    Form::ALL
        .get(i as usize)
        .map_or("?", |f| f.name())
        .to_string()
}

#[wasm_bindgen]
pub fn catalyst_count() -> u32 {
    Catalyst::ALL.len() as u32
}

#[wasm_bindgen]
pub fn catalyst_name(i: u32) -> String {
    Catalyst::ALL
        .get(i as usize)
        .map_or("?", |c| c.name())
        .to_string()
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
    MATERIALS.get(mat as usize).map_or(0, |m| pack(m.color))
}
