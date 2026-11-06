//! Top-level game state: world, player, spell particles and camera.

use physand_sim::{
    id,
    material::props,
    rng::Rng,
    worldgen::{cave_level, Level},
    Cell, World,
};

use crate::input::Input;
use crate::player::{Player, PLAYER_H, PLAYER_W};
use crate::render::{Overlay, Rgba};
use crate::spell::{Flask, Particle};

pub const WORLD_W: u32 = 1024;
pub const WORLD_H: u32 = 768;
pub const VIEW_W: u32 = 480;
pub const VIEW_H: u32 = 270;

/// Dig reach from the player's centre, in cells.
pub const DIG_REACH: f32 = 22.0;
pub const DIG_RADIUS: i32 = 4;
const CAMERA_LERP: f32 = 0.12;
/// How far the camera leans toward the aim point (0 = locked on player).
const CAMERA_LOOKAHEAD: f32 = 0.2;

pub struct Game {
    pub world: World,
    pub player: Player,
    pub flask: Flask,
    particles: Vec<Particle>,
    spawn: (i32, i32),
    input: Input,
    prev_jump: bool,
    rng: Rng,
    cam_x: f32,
    cam_y: f32,
    view_cells: Vec<Cell>,
    overlay: Vec<Rgba>,
    pub deaths: u32,
}

impl Game {
    pub fn new(seed: u32) -> Game {
        Self::with_size(WORLD_W, WORLD_H, seed)
    }

    pub fn with_size(width: u32, height: u32, seed: u32) -> Game {
        Self::from_level(cave_level(width, height, seed), seed)
    }

    /// Start on a prepared level (used by tests and, later, by floor transitions).
    pub fn from_level(level: Level, seed: u32) -> Game {
        let Level { world, spawn } = level;
        let player = Player::new(spawn.0, spawn.1);
        let mut game = Game {
            world,
            player,
            flask: Flask::water_spray(),
            particles: Vec::new(),
            spawn,
            input: Input::default(),
            prev_jump: false,
            rng: Rng::new(seed ^ 0x5EED_0001),
            cam_x: 0.0,
            cam_y: 0.0,
            view_cells: vec![Cell::EMPTY; (VIEW_W * VIEW_H) as usize],
            overlay: vec![[0; 4]; (VIEW_W * VIEW_H) as usize],
            deaths: 0,
        };
        let (tx, ty) = game.camera_target();
        game.cam_x = tx;
        game.cam_y = ty;
        game
    }

    pub fn set_input(&mut self, input: Input) {
        if input.jump && !self.prev_jump {
            self.player.press_jump();
        }
        self.prev_jump = input.jump;
        self.input = input;
    }

    /// Camera position (top-left of the view, world cells), already rounded.
    pub fn camera(&self) -> (i32, i32) {
        (self.cam_x.round() as i32, self.cam_y.round() as i32)
    }

    /// Convert a view-space position to world space.
    pub fn view_to_world(&self, vx: f32, vy: f32) -> (f32, f32) {
        let (cx, cy) = self.camera();
        (vx + cx as f32, vy + cy as f32)
    }

    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }

    pub fn step(&mut self) {
        let input = self.input;

        if self.player.alive() {
            if input.dig {
                self.dig(input.aim_x, input.aim_y);
            }
            if input.cast {
                let origin = self.hand();
                self.flask.cast(
                    origin,
                    (input.aim_x, input.aim_y),
                    &mut self.rng,
                    &mut self.particles,
                );
            }
            if self.player.update(&mut self.world, &input) {
                self.deaths += 1;
            }
        } else {
            self.player.dead_timer -= 1;
            if self.player.dead_timer == 0 {
                self.respawn();
            }
        }

        self.flask.tick();
        let world = &mut self.world;
        self.particles.retain_mut(|p| p.update(world));
        self.world.step();
        self.update_camera();
    }

    fn respawn(&mut self) {
        self.player = Player::new(self.spawn.0, self.spawn.1);
        self.flask.refill();
    }

    /// Where spells leave the player: the flask hand.
    fn hand(&self) -> (f32, f32) {
        let (cx, _) = self.player.center();
        (
            cx + self.player.facing as f32 * 2.0,
            self.player.y as f32 + 5.5,
        )
    }

    /// Aim point clamped to dig reach.
    pub fn dig_target(&self, aim_x: f32, aim_y: f32) -> (i32, i32) {
        let (px, py) = self.player.center();
        let (dx, dy) = (aim_x - px, aim_y - py);
        let dist = (dx * dx + dy * dy).sqrt();
        let k = if dist > DIG_REACH {
            DIG_REACH / dist
        } else {
            1.0
        };
        ((px + dx * k).floor() as i32, (py + dy * k).floor() as i32)
    }

    fn dig(&mut self, aim_x: f32, aim_y: f32) {
        let (tx, ty) = self.dig_target(aim_x, aim_y);
        let r = DIG_RADIUS;
        for y in (ty - r)..=(ty + r) {
            for x in (tx - r)..=(tx + r) {
                if (x - tx).pow(2) + (y - ty).pow(2) > r * r {
                    continue;
                }
                let chance = props(self.world.get(x, y)).dig;
                if chance > 0.0 && self.rng.chance(chance) && self.world.cell(x, y).is_some() {
                    self.world.set(x, y, id::EMPTY);
                }
            }
        }
    }

    fn camera_target(&self) -> (f32, f32) {
        let (px, py) = self.player.center();
        let lead_x = (self.input.aim_x - px) * CAMERA_LOOKAHEAD;
        let lead_y = (self.input.aim_y - py) * CAMERA_LOOKAHEAD;
        let lead = |v: f32| {
            if v.is_finite() {
                v.clamp(-60.0, 60.0)
            } else {
                0.0
            }
        };
        let max_x = (self.world.width() as f32 - VIEW_W as f32).max(0.0);
        let max_y = (self.world.height() as f32 - VIEW_H as f32).max(0.0);
        (
            (px + lead(lead_x) - VIEW_W as f32 / 2.0).clamp(0.0, max_x),
            (py + lead(lead_y) - VIEW_H as f32 / 2.0).clamp(0.0, max_y),
        )
    }

    fn update_camera(&mut self) {
        let (tx, ty) = self.camera_target();
        self.cam_x += (tx - self.cam_x) * CAMERA_LERP;
        self.cam_y += (ty - self.cam_y) * CAMERA_LERP;
    }

    /// Compose the visible window: cells into `view_cells`, sprites into `overlay`.
    pub fn render(&mut self) {
        let (cx, cy) = self.camera();
        let (vw, vh) = (VIEW_W as i32, VIEW_H as i32);
        self.world.copy_region(cx, cy, vw, vh, &mut self.view_cells);

        let flask_color = {
            let [r, g, b] = props(self.flask.material).color;
            let glow = if self.flask.volume >= self.flask.cost {
                255
            } else {
                120
            };
            [r, g, b, glow]
        };
        let dig_ring = self
            .input
            .dig
            .then(|| self.dig_target(self.input.aim_x, self.input.aim_y));
        let mut o = Overlay {
            buf: &mut self.overlay,
            w: vw,
            h: vh,
        };
        o.clear();

        for p in &self.particles {
            let [r, g, b] = props(p.mat).color;
            let lighten = |c: u8| c.saturating_add(40);
            o.put(
                p.x.floor() as i32 - cx,
                p.y.floor() as i32 - cy,
                [lighten(r), lighten(g), lighten(b), 255],
            );
        }

        let pl = &self.player;
        if pl.alive() {
            o.player(
                pl.x - cx,
                pl.y - cy,
                pl.facing,
                pl.hurt_flash > 0,
                flask_color,
            );
            let (ax, ay) = (
                self.input.aim_x.floor() as i32,
                self.input.aim_y.floor() as i32,
            );
            if let Some((tx, ty)) = dig_ring {
                o.ring(tx - cx, ty - cy, DIG_RADIUS, [255, 220, 150, 140]);
            }
            o.reticle(ax - cx, ay - cy, [255, 255, 255, 180]);
        }
    }

    /// RGBA8 cell data for the view, `VIEW_W * VIEW_H * 4` bytes.
    pub fn view_cells(&self) -> &[Cell] {
        &self.view_cells
    }

    pub fn overlay(&self) -> &[Rgba] {
        &self.overlay
    }

    /// Paint in view coordinates (sandbox brush).
    pub fn paint_view(&mut self, vx: i32, vy: i32, radius: i32, mat: u8) {
        let (cx, cy) = self.camera();
        self.world.paint(vx + cx, vy + cy, radius, mat);
    }

    pub fn player_size() -> (i32, i32) {
        (PLAYER_W, PLAYER_H)
    }
}
