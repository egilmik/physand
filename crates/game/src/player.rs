//! The alchemist: an axis-aligned box that moves cell by cell through the
//! sand grid, steps up small ledges, swims, levitates and takes damage from
//! hot materials.

use physand_sim::{id, material::kind, Kind, World};

use crate::input::Input;

pub const PLAYER_W: i32 = 5;
pub const PLAYER_H: i32 = 11;
pub const MAX_HP: f32 = 100.0;
pub const MAX_STAMINA: f32 = 100.0;

const GRAVITY: f32 = 0.13;
const MAX_FALL: f32 = 4.0;
const WALK_SPEED: f32 = 0.9;
const GROUND_ACCEL: f32 = 0.25;
const AIR_ACCEL: f32 = 0.12;
const JUMP_SPEED: f32 = 2.3;
const LEVITATE_ACCEL: f32 = 0.24;
const LEVITATE_MAX_RISE: f32 = 1.6;
const STAMINA_DRAIN: f32 = 1.8;
const STAMINA_REGEN: f32 = 2.0;
const SWIM_ACCEL: f32 = 0.22;
/// Highest ledge (in cells) the player walks up without jumping.
const STEP_UP: i32 = 3;
/// Grace ticks for jumping just after leaving a ledge / just before landing.
const COYOTE_TICKS: u8 = 6;
const JUMP_BUFFER_TICKS: u8 = 6;
const RESPAWN_TICKS: u16 = 120;

/// Cells the player cannot pass through.
#[inline]
pub fn blocks_player(mat: u8) -> bool {
    matches!(kind(mat), Kind::Solid | Kind::Powder)
}

#[derive(Clone, Debug)]
pub struct Player {
    /// Top-left cell of the bounding box.
    pub x: i32,
    pub y: i32,
    /// Sub-cell movement carried between ticks.
    rem_x: f32,
    rem_y: f32,
    pub vx: f32,
    pub vy: f32,
    pub grounded: bool,
    /// Fraction of the body inside liquid, `0..=1`.
    pub submerged: f32,
    pub facing: i32,
    pub hp: f32,
    pub stamina: f32,
    /// Ticks left showing the damage flash.
    pub hurt_flash: u8,
    /// Ticks until respawn while dead (`0` = alive).
    pub dead_timer: u16,
    coyote: u8,
    jump_buffer: u8,
}

impl Player {
    /// Spawn with feet resting on top of cell row `feet_y + 1`, centred on `cx`.
    pub fn new(cx: i32, feet_y: i32) -> Player {
        Player {
            x: cx - PLAYER_W / 2,
            y: feet_y - PLAYER_H + 1,
            rem_x: 0.0,
            rem_y: 0.0,
            vx: 0.0,
            vy: 0.0,
            grounded: false,
            submerged: 0.0,
            facing: 1,
            hp: MAX_HP,
            stamina: MAX_STAMINA,
            hurt_flash: 0,
            dead_timer: 0,
            coyote: 0,
            jump_buffer: 0,
        }
    }

    pub fn alive(&self) -> bool {
        self.dead_timer == 0
    }

    /// Centre of the body in world coordinates.
    pub fn center(&self) -> (f32, f32) {
        (
            self.x as f32 + PLAYER_W as f32 / 2.0,
            self.y as f32 + PLAYER_H as f32 / 2.0,
        )
    }

    /// Register a jump press; it is buffered for a few ticks.
    pub fn press_jump(&mut self) {
        self.jump_buffer = JUMP_BUFFER_TICKS;
    }

    /// Whether the body placed at `(x, y)` overlaps any blocking cell.
    pub fn overlaps(world: &World, x: i32, y: i32) -> bool {
        (y..y + PLAYER_H).any(|cy| (x..x + PLAYER_W).any(|cx| blocks_player(world.get(cx, cy))))
    }

    /// Advance one tick. Returns `true` if the player just died.
    pub fn update(&mut self, world: &mut World, input: &Input) -> bool {
        if !self.alive() {
            return false;
        }
        self.hurt_flash = self.hurt_flash.saturating_sub(1);
        self.resolve_overlap(world);
        self.sense(world);

        // --- Horizontal control -------------------------------------------
        let dir = input.right as i32 - input.left as i32;
        if dir != 0 {
            self.facing = dir;
        }
        let accel = if self.grounded {
            GROUND_ACCEL
        } else {
            AIR_ACCEL
        };
        let target = dir as f32 * WALK_SPEED * (1.0 - 0.4 * self.submerged);
        if dir != 0 {
            self.vx += (target - self.vx).clamp(-accel, accel);
        } else {
            self.vx *= if self.grounded { 0.6 } else { 0.95 };
        }

        // --- Vertical: jump, levitate, swim, gravity ------------------------
        if self.grounded {
            self.coyote = COYOTE_TICKS;
            self.stamina = (self.stamina + STAMINA_REGEN).min(MAX_STAMINA);
        } else {
            self.coyote = self.coyote.saturating_sub(1);
        }

        if self.jump_buffer > 0 && self.coyote > 0 {
            self.vy = -JUMP_SPEED;
            self.jump_buffer = 0;
            self.coyote = 0;
        } else if input.jump && self.submerged > 0.3 {
            self.vy -= SWIM_ACCEL * self.submerged;
        } else if input.jump && !self.grounded && self.vy > -1.0 && self.stamina > 0.0 {
            self.vy = (self.vy - LEVITATE_ACCEL).max(-LEVITATE_MAX_RISE);
            self.stamina = (self.stamina - STAMINA_DRAIN).max(0.0);
        }
        self.jump_buffer = self.jump_buffer.saturating_sub(1);

        // Buoyancy: gravity weakens as the body sinks into liquid.
        self.vy += GRAVITY * (1.0 - 1.1 * self.submerged);
        let drag = 1.0 - 0.12 * self.submerged;
        self.vx *= drag;
        self.vy *= drag;
        let max_fall = MAX_FALL - 3.0 * self.submerged;
        self.vy = self.vy.clamp(-MAX_FALL, max_fall);

        // --- Integrate one cell at a time --------------------------------
        self.move_x(world);
        self.move_y(world);
        self.sense(world);

        self.take_environment_damage(world)
    }

    /// Recompute contact state.
    fn sense(&mut self, world: &World) {
        self.grounded = Self::overlaps(world, self.x, self.y + 1);
        let mut liquid = 0;
        for cy in self.y..self.y + PLAYER_H {
            for cx in self.x..self.x + PLAYER_W {
                if kind(world.get(cx, cy)) == Kind::Liquid {
                    liquid += 1;
                }
            }
        }
        self.submerged = liquid as f32 / (PLAYER_W * PLAYER_H) as f32;
    }

    fn move_x(&mut self, world: &World) {
        self.rem_x += self.vx;
        let steps = self.rem_x.round() as i32;
        self.rem_x -= steps as f32;
        let sign = steps.signum();
        for _ in 0..steps.abs() {
            if !Self::overlaps(world, self.x + sign, self.y) {
                self.x += sign;
                continue;
            }
            // Walk up slopes and small ledges (sand piles!).
            let max_up = if self.grounded || self.coyote > 0 {
                STEP_UP
            } else {
                1
            };
            if let Some(up) =
                (1..=max_up).find(|&up| !Self::overlaps(world, self.x + sign, self.y - up))
            {
                self.x += sign;
                self.y -= up;
            } else {
                self.vx = 0.0;
                self.rem_x = 0.0;
                break;
            }
        }
    }

    fn move_y(&mut self, world: &World) {
        self.rem_y += self.vy;
        let steps = self.rem_y.round() as i32;
        self.rem_y -= steps as f32;
        let sign = steps.signum();
        for _ in 0..steps.abs() {
            if Self::overlaps(world, self.x, self.y + sign) {
                self.vy = 0.0;
                self.rem_y = 0.0;
                break;
            }
            self.y += sign;
        }
    }

    /// The sim does not know about the player, so sand can fall into the
    /// body. Push loose grains out sideways, then climb out of anything solid.
    fn resolve_overlap(&mut self, world: &mut World) {
        for cy in self.y..self.y + PLAYER_H {
            for cx in self.x..self.x + PLAYER_W {
                let mat = world.get(cx, cy);
                if kind(mat) != Kind::Powder {
                    continue;
                }
                // Shove toward the nearer side of the body.
                let side = if cx - self.x < PLAYER_W / 2 { -1 } else { 1 };
                let edge = if side < 0 {
                    self.x - 1
                } else {
                    self.x + PLAYER_W
                };
                let dest = (0..6)
                    .map(|i| edge + side * i)
                    .find(|&nx| world.get(nx, cy) == id::EMPTY);
                world.set(cx, cy, id::EMPTY);
                if let Some(nx) = dest {
                    world.set(nx, cy, mat);
                }
            }
        }
        if Self::overlaps(world, self.x, self.y) {
            if let Some(up) = (1..=4).find(|&up| !Self::overlaps(world, self.x, self.y - up)) {
                self.y -= up;
            }
        }
    }

    /// Fire, lava, acid and scalding cells touching the body hurt.
    /// Returns `true` on death.
    fn take_environment_damage(&mut self, world: &World) -> bool {
        let mut dmg: f32 = 0.0;
        for cy in self.y - 1..=self.y + PLAYER_H {
            for cx in self.x - 1..=self.x + PLAYER_W {
                let Some(cell) = world.cell(cx, cy) else {
                    continue;
                };
                dmg += match cell.mat {
                    id::FIRE => 0.08,
                    id::LAVA => 0.5,
                    id::ACID => 0.15,
                    id::EMPTY => 0.0,
                    _ if cell.temp >= 180 => 0.05,
                    _ => 0.0,
                };
            }
        }
        let dmg = dmg.min(4.0);
        if dmg > 0.0 {
            self.damage(dmg);
        }
        if self.hp <= 0.0 {
            self.kill();
            return true;
        }
        false
    }

    pub fn kill(&mut self) {
        self.hp = 0.0;
        self.dead_timer = RESPAWN_TICKS;
    }

    pub fn damage(&mut self, amount: f32) {
        self.hp -= amount;
        self.hurt_flash = 6;
    }
}
