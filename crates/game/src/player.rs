//! The alchemist: steps up small ledges, swims, levitates and takes damage
//! from hot and corrosive materials. Movement physics live in [`Body`].

use std::ops::{Deref, DerefMut};

use physand_sim::World;

use crate::body::Body;
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
/// Invulnerability after being hit by an enemy.
const INVULN_TICKS: u8 = 40;

#[derive(Clone, Debug)]
pub struct Player {
    pub body: Body,
    pub facing: i32,
    pub hp: f32,
    pub stamina: f32,
    /// Ticks left showing the damage flash.
    pub hurt_flash: u8,
    /// Ticks of immunity to enemy contact damage.
    pub invuln: u8,
    coyote: u8,
    jump_buffer: u8,
}

impl Deref for Player {
    type Target = Body;
    fn deref(&self) -> &Body {
        &self.body
    }
}

impl DerefMut for Player {
    fn deref_mut(&mut self) -> &mut Body {
        &mut self.body
    }
}

impl Player {
    /// Spawn with feet resting on top of cell row `feet_y + 1`, centred on `cx`.
    pub fn new(cx: i32, feet_y: i32) -> Player {
        Player {
            body: Body::standing(cx, feet_y, PLAYER_W, PLAYER_H),
            facing: 1,
            hp: MAX_HP,
            stamina: MAX_STAMINA,
            hurt_flash: 0,
            invuln: 0,
            coyote: 0,
            jump_buffer: 0,
        }
    }

    pub fn alive(&self) -> bool {
        self.hp > 0.0
    }

    /// Register a jump press; it is buffered for a few ticks.
    pub fn press_jump(&mut self) {
        self.jump_buffer = JUMP_BUFFER_TICKS;
    }

    /// Whether a player-sized body placed at `(x, y)` overlaps a blocking cell.
    pub fn overlaps(world: &World, x: i32, y: i32) -> bool {
        Body::new(x, y, PLAYER_W, PLAYER_H).overlaps_at(world, x, y)
    }

    /// Advance one tick. Returns `true` if the player just died.
    pub fn update(&mut self, world: &mut World, input: &Input) -> bool {
        if !self.alive() {
            return false;
        }
        self.hurt_flash = self.hurt_flash.saturating_sub(1);
        self.invuln = self.invuln.saturating_sub(1);
        self.body.resolve_overlap(world);
        self.body.sense(world);
        let (grounded, submerged) = (self.grounded, self.submerged);

        // --- Horizontal control -------------------------------------------
        let dir = input.right as i32 - input.left as i32;
        if dir != 0 {
            self.facing = dir;
        }
        let accel = if grounded { GROUND_ACCEL } else { AIR_ACCEL };
        let target = dir as f32 * WALK_SPEED * (1.0 - 0.4 * submerged);
        if dir != 0 {
            self.vx += (target - self.vx).clamp(-accel, accel);
        } else {
            self.vx *= if grounded { 0.6 } else { 0.95 };
        }

        // --- Vertical: jump, levitate, swim, gravity ------------------------
        if grounded {
            self.coyote = COYOTE_TICKS;
            self.stamina = (self.stamina + STAMINA_REGEN).min(MAX_STAMINA);
        } else {
            self.coyote = self.coyote.saturating_sub(1);
        }

        if self.jump_buffer > 0 && self.coyote > 0 {
            self.vy = -JUMP_SPEED;
            self.jump_buffer = 0;
            self.coyote = 0;
        } else if input.jump && submerged > 0.3 {
            self.vy -= SWIM_ACCEL * submerged;
        } else if input.jump && !grounded && self.vy > -1.0 && self.stamina > 0.0 {
            self.vy = (self.vy - LEVITATE_ACCEL).max(-LEVITATE_MAX_RISE);
            self.stamina = (self.stamina - STAMINA_DRAIN).max(0.0);
        }
        self.jump_buffer = self.jump_buffer.saturating_sub(1);

        // Buoyancy: gravity weakens as the body sinks into liquid.
        self.vy += GRAVITY * (1.0 - 1.1 * submerged);
        let drag = 1.0 - 0.12 * submerged;
        self.vx *= drag;
        self.vy *= drag;
        let max_fall = MAX_FALL - 3.0 * submerged;
        self.vy = self.vy.clamp(-MAX_FALL, max_fall);

        // --- Integrate one cell at a time --------------------------------
        let step_up = if grounded || self.coyote > 0 {
            STEP_UP
        } else {
            1
        };
        self.body.move_x(world, step_up);
        self.body.move_y(world);
        self.body.sense(world);

        // --- Hazards ---------------------------------------------------------
        // Fire hurts but leaves time to escape; lava is nearly instant death.
        let c = self.body.contact(world);
        let dmg = ((c.fire as f32 * 0.03).min(0.6)
            + c.lava as f32 * 0.5
            + (c.acid as f32 * 0.1).min(1.0)
            + (c.hot as f32 * 0.02).min(0.4))
        .min(4.0);
        if dmg > 0.0 {
            self.damage(dmg);
        }
        !self.alive()
    }

    pub fn kill(&mut self) {
        self.hp = 0.0;
    }

    pub fn damage(&mut self, amount: f32) {
        self.hp = (self.hp - amount).max(0.0);
        self.hurt_flash = 6;
    }

    /// Contact damage from an enemy, respecting invulnerability frames.
    /// Returns `true` if it landed.
    pub fn hit(&mut self, amount: f32, knock_x: f32) -> bool {
        if self.invuln > 0 || !self.alive() {
            return false;
        }
        self.damage(amount);
        self.invuln = INVULN_TICKS;
        self.vx += knock_x;
        self.vy -= 1.2;
        true
    }

    pub fn heal(&mut self, amount: f32) {
        self.hp = (self.hp + amount).min(MAX_HP);
    }
}
