//! Catacomb creatures. They live in the same material world as the player:
//! fire sets them alight, water drowns or erodes them, acid eats them, and a
//! dead golem is a pile of real sand.

use physand_sim::{id, material::props, rng::Rng, World};

use crate::body::{blocks, Body};
use crate::spell::{Particle, ParticleKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnemyKind {
    /// Scuttling beetle: walks, chases, hops over obstacles. Burns like a torch.
    Scarab,
    /// Flying dust spirit: swoops at you, drowns in liquid.
    Wraith,
    /// Slow sand guardian: eroded by water, eaten by acid, baked to glass by heat.
    Golem,
}

impl EnemyKind {
    pub fn size(self) -> (i32, i32) {
        match self {
            EnemyKind::Scarab => (5, 4),
            EnemyKind::Wraith => (7, 5),
            EnemyKind::Golem => (9, 14),
        }
    }

    fn max_hp(self) -> f32 {
        match self {
            EnemyKind::Scarab => 18.0,
            EnemyKind::Wraith => 12.0,
            EnemyKind::Golem => 70.0,
        }
    }

    fn speed(self) -> f32 {
        match self {
            EnemyKind::Scarab => 0.75,
            EnemyKind::Wraith => 0.9,
            EnemyKind::Golem => 0.35,
        }
    }

    fn contact_damage(self) -> f32 {
        match self {
            EnemyKind::Scarab => 8.0,
            EnemyKind::Wraith => 6.0,
            EnemyKind::Golem => 16.0,
        }
    }

    fn sight(self) -> f32 {
        match self {
            EnemyKind::Scarab => 110.0,
            EnemyKind::Wraith => 140.0,
            EnemyKind::Golem => 90.0,
        }
    }
}

/// Golems turn to glass at this temperature.
pub const BAKE_TEMP: f32 = 200.0;
const BURN_TICKS: u16 = 150;
const FREEZE_TICKS: u16 = 90;
const ATTACK_COOLDOWN: u8 = 30;
const GRAVITY: f32 = 0.15;

/// Something an enemy did this tick that the game must resolve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EnemyEvent {
    /// Touched the player.
    Hit { damage: f32, knock_x: f32 },
}

#[derive(Clone, Debug)]
pub struct Enemy {
    pub kind: EnemyKind,
    pub body: Body,
    pub hp: f32,
    pub facing: i32,
    /// Ticks left on fire.
    pub burning: u16,
    /// Ticks left frozen solid.
    pub frozen: u16,
    /// Golem core temperature.
    pub temp: f32,
    pub hurt_flash: u8,
    /// Whether the player has hurt it (kills only count if so).
    pub credited: bool,
    alert: u16,
    think: u8,
    attack_cd: u8,
    home: (f32, f32),
    pub anim: u32,
}

impl Enemy {
    /// Place an enemy with its feet at `(x, feet_y)` (walkers) or centred there (flyers).
    pub fn new(kind: EnemyKind, x: i32, y: i32) -> Enemy {
        let (w, h) = kind.size();
        let body = match kind {
            EnemyKind::Wraith => Body::new(x - w / 2, y - h / 2, w, h),
            _ => Body::standing(x, y, w, h),
        };
        Enemy {
            kind,
            body,
            hp: kind.max_hp(),
            facing: if x % 2 == 0 { 1 } else { -1 },
            burning: 0,
            frozen: 0,
            temp: 40.0,
            hurt_flash: 0,
            credited: false,
            alert: 0,
            think: (x as u32 % 8) as u8,
            attack_cd: 0,
            home: (x as f32, y as f32),
            anim: x as u32,
        }
    }

    pub fn alive(&self) -> bool {
        self.hp > 0.0
    }

    pub fn alerted(&self) -> bool {
        self.alert > 0
    }

    /// Golems that died hot become glass statues.
    pub fn baked(&self) -> bool {
        self.kind == EnemyKind::Golem && self.temp >= BAKE_TEMP
    }

    pub fn damage(&mut self, amount: f32) {
        let mult = if self.frozen > 0 { 1.5 } else { 1.0 };
        self.hp -= amount * mult;
        self.hurt_flash = 5;
        // Getting hurt reveals where the attacker is.
        self.alert = self.alert.max(180);
    }

    /// Set alight (golems heat up instead of burning).
    pub fn ignite(&mut self) {
        match self.kind {
            EnemyKind::Golem => self.temp += 40.0,
            _ => self.burning = BURN_TICKS,
        }
        self.frozen = 0;
    }

    pub fn freeze(&mut self) {
        self.frozen = FREEZE_TICKS;
        self.burning = 0;
    }

    /// Advance one tick. `target` is the player's body (if alive).
    pub fn update(
        &mut self,
        world: &mut World,
        target: Option<&Body>,
        rng: &mut Rng,
    ) -> Option<EnemyEvent> {
        if !self.alive() {
            return None;
        }
        self.anim = self.anim.wrapping_add(1);
        self.hurt_flash = self.hurt_flash.saturating_sub(1);
        self.attack_cd = self.attack_cd.saturating_sub(1);
        self.alert = self.alert.saturating_sub(1);

        self.body.resolve_overlap(world);
        self.body.sense(world);
        self.environment(world, rng);
        if !self.alive() {
            return None;
        }

        // --- Perception ---------------------------------------------------------
        self.think = self.think.wrapping_sub(1);
        if self.think == 0 || self.think > 8 {
            self.think = 8;
            if let Some(t) = target {
                let (ex, ey) = self.body.center();
                let (tx, ty) = t.center();
                let d2 = (tx - ex).powi(2) + (ty - ey).powi(2);
                if d2 < self.kind.sight().powi(2) && line_of_sight(world, (ex, ey), (tx, ty)) {
                    self.alert = 240;
                }
            }
        }

        // --- Movement -------------------------------------------------------------
        if self.frozen > 0 {
            // Frozen solid: no control, and even wraiths drop like stones.
            self.body.vx = 0.0;
            self.fall(world);
        } else {
            match self.kind {
                EnemyKind::Scarab | EnemyKind::Golem => self.walk(world, target, rng),
                EnemyKind::Wraith => self.fly(world, target),
            }
        }
        self.body.sense(world);

        // --- Contact attack ---------------------------------------------------------
        let t = target?;
        let reach = Body::new(
            self.body.x - 1,
            self.body.y - 1,
            self.body.w + 2,
            self.body.h + 2,
        );
        if self.frozen == 0 && self.attack_cd == 0 && reach.intersects(t) {
            self.attack_cd = ATTACK_COOLDOWN;
            let dir = if t.center().0 >= self.body.center().0 {
                1.0
            } else {
                -1.0
            };
            let knock = if self.kind == EnemyKind::Golem {
                3.5
            } else {
                2.0
            };
            return Some(EnemyEvent::Hit {
                damage: self.kind.contact_damage(),
                knock_x: dir * knock,
            });
        }
        None
    }

    /// Hazards, burning, freezing, drowning, erosion and baking.
    fn environment(&mut self, world: &mut World, rng: &mut Rng) {
        let c = self.body.contact(world);
        let golem = self.kind == EnemyKind::Golem;

        let acid_mult = if golem { 2.5 } else { 1.0 };
        let mut dmg = c.lava as f32 * 0.8 + c.acid as f32 * 0.25 * acid_mult;
        if !golem {
            dmg += c.hot as f32 * 0.08;
        }

        if c.fire > 0 || c.lava > 0 {
            if golem {
                self.temp += c.fire as f32 * 0.5 + c.lava as f32 * 3.0;
            } else if self.burning == 0 {
                self.ignite();
            }
        }
        if c.water > 0 {
            // Water douses flames; it erodes golems.
            self.burning = 0;
            if golem {
                dmg += (c.water as f32 * 0.05).min(1.0);
            }
        }
        if self.kind == EnemyKind::Wraith && self.body.submerged > 0.3 {
            dmg += 0.4; // drowning
        }

        if self.burning > 0 {
            self.burning -= 1;
            dmg += if self.kind == EnemyKind::Wraith {
                0.4
            } else {
                0.25
            };
            // A burning creature is a torch: it lights what it runs past.
            if rng.chance(0.08) {
                let x = self.body.x - 1 + rng.range(0, self.body.w as u32 + 1) as i32;
                let y = self.body.y - 1 + rng.range(0, self.body.h as u32 + 1) as i32;
                if world.get(x, y) == id::EMPTY {
                    world.set(x, y, id::FIRE);
                }
            }
        }
        if self.frozen > 0 {
            self.frozen -= 1;
        }
        if golem {
            // Cool back toward ambient.
            self.temp += (40.0 - self.temp) * 0.004;
            if self.temp >= BAKE_TEMP {
                self.hp = 0.0;
                return;
            }
        }
        if dmg > 0.0 {
            self.damage(dmg.min(6.0));
        }
    }

    fn fall(&mut self, world: &World) {
        self.body.vy =
            (self.body.vy + GRAVITY * (1.0 - 1.1 * self.body.submerged)).clamp(-4.0, 4.0);
        self.body.move_y(world);
    }

    fn walk(&mut self, world: &World, target: Option<&Body>, rng: &mut Rng) {
        let speed = self.kind.speed() * (1.0 - 0.5 * self.body.submerged);
        let chasing = self.alerted() && target.is_some();
        if chasing {
            let tx = target.unwrap().center().0;
            let ex = self.body.center().0;
            if (tx - ex).abs() > 2.0 {
                self.facing = if tx > ex { 1 } else { -1 };
            }
        } else if self.body.grounded {
            // Wander: turn at walls and drop-offs, and now and then on a whim.
            let ahead_x = if self.facing > 0 {
                self.body.x + self.body.w
            } else {
                self.body.x - 1
            };
            let feet = self.body.y + self.body.h;
            let drop = (0..4).all(|d| !blocks(world.get(ahead_x, feet + d)));
            if drop || rng.chance(0.004) {
                self.facing = -self.facing;
            }
        }
        let pace = if chasing { speed * 1.3 } else { speed * 0.6 };
        self.body.vx += (self.facing as f32 * pace - self.body.vx) * 0.3;

        let step_up = match self.kind {
            EnemyKind::Golem => 3,
            _ => 2,
        };
        let blocked = self.body.move_x(world, step_up);
        if blocked {
            if chasing && self.kind == EnemyKind::Scarab && self.body.grounded {
                self.body.vy = -2.1; // hop the obstacle
            } else if !chasing {
                self.facing = -self.facing;
            }
        }
        self.fall(world);
    }

    fn fly(&mut self, world: &World, target: Option<&Body>) {
        let (ex, ey) = self.body.center();
        let bob = ((self.anim as f32) * 0.08).sin();
        let (gx, gy) = match target {
            Some(t) if self.alerted() => {
                let (tx, ty) = t.center();
                (tx, ty + bob * 8.0)
            }
            _ => (
                self.home.0 + ((self.anim as f32) * 0.013).sin() * 30.0,
                self.home.1 + bob * 6.0,
            ),
        };
        let (dx, dy) = (gx - ex, gy - ey);
        let d = (dx * dx + dy * dy).sqrt().max(0.001);
        let speed = if self.alerted() {
            self.kind.speed()
        } else {
            self.kind.speed() * 0.4
        };
        self.body.vx += (dx / d * speed - self.body.vx) * 0.08;
        self.body.vy += (dy / d * speed - self.body.vy) * 0.08;
        if self.body.submerged > 0.3 {
            self.body.vy += GRAVITY; // waterlogged wings
        }
        if dx.abs() > 1.0 {
            self.facing = if dx > 0.0 { 1 } else { -1 };
        }
        if self.body.move_x(world, 0) {
            self.body.vx = -self.body.vx * 0.5;
        }
        if self.body.move_y(world) {
            self.body.vy = -self.body.vy * 0.5;
        }
    }

    /// Resolve a spell particle striking this enemy.
    pub fn struck_by(&mut self, p: &Particle) {
        self.credited = true;
        let mat = p.mat();
        let golem = self.kind == EnemyKind::Golem;
        let mut dmg = match mat {
            id::ACID => 2.5 * if golem { 2.5 } else { 1.0 },
            id::LAVA => 3.0,
            id::WATER => {
                self.burning = 0;
                if golem {
                    1.5
                } else {
                    0.0
                }
            }
            id::SAND | id::WOOD | id::ICE => 0.8,
            _ => 0.4,
        };
        if p.kind == ParticleKind::Orb {
            dmg = 8.0 + dmg * 4.0;
        }
        if p.payload.transmute {
            dmg += 30.0;
        }
        if p.payload.impact_heat > 0 || mat == id::LAVA {
            if golem {
                self.temp += if p.kind == ParticleKind::Orb {
                    90.0
                } else {
                    6.0
                };
            } else {
                self.ignite();
            }
        } else if p.payload.impact_heat < 0 {
            if golem {
                self.temp = (self.temp - 30.0).max(0.0);
            }
            self.freeze();
        }
        if dmg > 0.0 {
            self.damage(dmg);
        }
    }

    /// Leave remains in the world: golems collapse into sand (or stand as glass).
    pub fn remains(&self, world: &mut World) {
        let b = &self.body;
        let mat = match self.kind {
            EnemyKind::Golem if self.baked() => id::GLASS,
            EnemyKind::Golem => id::SAND,
            EnemyKind::Scarab => id::EMPTY,
            EnemyKind::Wraith => id::SMOKE,
        };
        if mat == id::EMPTY {
            return;
        }
        for y in b.y..b.y + b.h {
            for x in b.x..b.x + b.w {
                // Roughly the creature's silhouette.
                let edge = (x == b.x || x == b.x + b.w - 1) && (y == b.y || y == b.y + b.h - 1);
                if !edge
                    && (world.get(x, y) == id::EMPTY
                        || props(world.get(x, y)).kind == physand_sim::Kind::Gas)
                {
                    world.set(x, y, mat);
                }
            }
        }
    }
}

/// Whether nothing solid lies between two points.
pub fn line_of_sight(world: &World, a: (f32, f32), b: (f32, f32)) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let steps = (dx.abs().max(dy.abs()) / 2.0).ceil().max(1.0) as i32;
    (1..steps).all(|i| {
        let t = i as f32 / steps as f32;
        !blocks(world.get((a.0 + dx * t) as i32, (a.1 + dy * t) as i32))
    })
}
