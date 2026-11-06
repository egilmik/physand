//! Browser bindings. JS owns the frame loop, input capture and WebGL; each
//! frame it pushes input, advances fixed ticks, asks Rust to compose the
//! view, then uploads the view and overlay buffers straight from wasm memory.

use physand_game::{
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

    /// 0 = playing, 1 = dead (run over), 2 = floor cleared, 3 = victory.
    pub fn phase(&self) -> u8 {
        self.game.phase.code()
    }

    /// 0 = catacomb floor, 1 = Distillery, 2 = boss lair.
    pub fn stage(&self) -> u8 {
        self.game.stage.code()
    }

    /// Enter the Distillery after clearing a floor. Returns `false` otherwise.
    pub fn descend(&mut self) -> bool {
        self.game.descend()
    }

    pub fn floors_per_run(&self) -> u32 {
        physand_game::run::FLOORS_PER_RUN
    }

    // --- Economy, relics, inventory -----------------------------------------

    pub fn gold(&self) -> u32 {
        self.game.gold
    }

    pub fn relic_count(&self) -> u32 {
        self.game.inventory.relics.len() as u32
    }

    pub fn relic_name(&self, i: u32) -> String {
        self.relic(i)
            .map_or_else(String::new, |r| r.name().to_string())
    }

    pub fn relic_description(&self, i: u32) -> String {
        self.relic(i)
            .map_or_else(String::new, |r| r.description().to_string())
    }

    pub fn relic_color(&self, i: u32) -> u32 {
        self.relic(i).map_or(0, |r| pack(r.color()))
    }

    /// Owned reagent material ids.
    pub fn owned_reagents(&self) -> Vec<u8> {
        self.game.inventory.reagents.clone()
    }

    /// Owned forms, as indices into `form_name`.
    pub fn owned_forms(&self) -> Vec<u8> {
        let inv = &self.game.inventory;
        (0..Form::ALL.len() as u8)
            .filter(|&i| inv.forms.contains(&Form::ALL[i as usize]))
            .collect()
    }

    /// Owned catalysts, as indices into `catalyst_name`.
    pub fn owned_catalysts(&self) -> Vec<u8> {
        let inv = &self.game.inventory;
        (0..Catalyst::ALL.len() as u8)
            .filter(|&i| inv.catalysts.contains(&Catalyst::ALL[i as usize]))
            .collect()
    }

    /// Whether flasks may be edited right now (at a Distillery workbench).
    pub fn can_edit_flasks(&self) -> bool {
        self.game.can_edit_flasks()
    }

    // --- Distillery prompt and events ----------------------------------------

    pub fn has_prompt(&self) -> bool {
        self.game.interact_prompt().is_some()
    }

    pub fn prompt_title(&self) -> String {
        self.game
            .interact_prompt()
            .map_or_else(String::new, |p| p.title)
    }

    pub fn prompt_detail(&self) -> String {
        self.game
            .interact_prompt()
            .map_or_else(String::new, |p| p.detail)
    }

    /// Price in gold, or -1 if free.
    pub fn prompt_price(&self) -> i32 {
        self.game
            .interact_prompt()
            .and_then(|p| p.price)
            .map_or(-1, |p| p as i32)
    }

    pub fn prompt_affordable(&self) -> bool {
        self.game.interact_prompt().is_some_and(|p| p.affordable)
    }

    /// The last interaction's outcome, consumed: `"spellcraft"` to open the
    /// Spellcraft panel, a message for the HUD, or `""` if nothing happened.
    pub fn take_event(&mut self) -> String {
        use physand_game::distillery::Interaction;
        match self.game.last_interaction.take() {
            None => String::new(),
            Some(Interaction::OpenSpellcraft) => "spellcraft".into(),
            Some(Interaction::Took(item)) => format!("Gained {}", item.name()),
            Some(Interaction::Bought(item)) => format!("Bought {}", item.name()),
            Some(Interaction::TooPoor { price }) => format!("Not enough gold — {price} needed"),
            Some(Interaction::Refilled) => "Flasks refilled, wounds eased".into(),
            Some(Interaction::AlreadyUsed) => "The fountain has run dry".into(),
        }
    }

    // --- Boss ------------------------------------------------------------------

    /// Current boss health, or NaN if there is no boss.
    pub fn boss_hp(&self) -> f32 {
        self.game.boss_health().map_or(f32::NAN, |h| h.0)
    }

    pub fn boss_max_hp(&self) -> f32 {
        self.game.boss_health().map_or(f32::NAN, |h| h.1)
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
        self.game.player.max_hp
    }

    pub fn stamina(&self) -> f32 {
        self.game.player.stamina
    }

    pub fn max_stamina(&self) -> f32 {
        self.game.player.max_stamina
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

    /// Debug: count the current floor as cleared.
    pub fn debug_clear_floor(&mut self) {
        self.game.debug_clear_floor();
    }

    /// Debug: jump to the Sandwyrm.
    pub fn debug_skip_to_boss(&mut self) {
        self.game.debug_skip_to_boss();
    }

    /// Debug: slay the Sandwyrm.
    pub fn debug_slay_boss(&mut self) {
        self.game.debug_slay_boss();
    }

    /// Debug: step into the current exit.
    pub fn debug_enter_exit(&mut self) {
        self.game.debug_enter_exit();
    }

    /// Debug: add gold.
    pub fn debug_add_gold(&mut self, amount: u32) {
        self.game.gold += amount;
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

    fn relic(&self, i: u32) -> Option<physand_game::run::Relic> {
        self.game.inventory.relics.get(i as usize).copied()
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
