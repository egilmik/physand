//! Spells and the free-flying particles they emit. A particle is not part of
//! the grid while airborne; when it hits something it is deposited as a real
//! cell and the simulation takes over (pillar: everything is material).

use physand_sim::{id, material::kind, Kind, World};

use physand_sim::rng::Rng;

const PARTICLE_GRAVITY: f32 = 0.06;
const PARTICLE_MAX_LIFE: u16 = 180;

#[derive(Clone, Debug)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub mat: u8,
    life: u16,
}

/// Airborne particles fly through empty space and gases.
#[inline]
fn passable(mat: u8) -> bool {
    mat == id::EMPTY || kind(mat) == Kind::Gas
}

impl Particle {
    /// Advance one tick. Returns `false` once it has landed (or expired).
    pub fn update(&mut self, world: &mut World) -> bool {
        self.vy += PARTICLE_GRAVITY;
        self.life += 1;
        // Sub-step so fast particles never tunnel through thin walls.
        let steps = self.vx.abs().max(self.vy.abs()).ceil().max(1.0) as i32;
        let (sx, sy) = (self.vx / steps as f32, self.vy / steps as f32);
        for _ in 0..steps {
            let (nx, ny) = (self.x + sx, self.y + sy);
            if !passable(world.get(nx.floor() as i32, ny.floor() as i32)) {
                self.deposit(world);
                return false;
            }
            self.x = nx;
            self.y = ny;
        }
        if self.life >= PARTICLE_MAX_LIFE {
            self.deposit(world);
            return false;
        }
        true
    }

    fn deposit(&self, world: &mut World) {
        let (cx, cy) = (self.x.floor() as i32, self.y.floor() as i32);
        for (x, y) in [(cx, cy), (cx, cy - 1)] {
            if passable(world.get(x, y)) {
                world.set(x, y, self.mat);
                return;
            }
        }
    }
}

/// A flask holding one reagent, delivered as a spray. (M2 generalises this
/// into reagent / form / catalyst slots.)
#[derive(Clone, Debug)]
pub struct Flask {
    pub name: &'static str,
    pub material: u8,
    pub volume: f32,
    pub capacity: f32,
    /// Volume regained per tick.
    pub regen: f32,
    /// Volume spent per particle.
    pub cost: f32,
    /// Particles per tick while casting.
    pub rate: u32,
    pub speed: f32,
    /// Half-angle of the spray cone in radians.
    pub spread: f32,
}

impl Flask {
    pub fn water_spray() -> Flask {
        Flask {
            name: "Water Spray",
            material: id::WATER,
            volume: 300.0,
            capacity: 300.0,
            regen: 0.6,
            cost: 1.0,
            rate: 4,
            speed: 3.2,
            spread: 0.10,
        }
    }

    pub fn refill(&mut self) {
        self.volume = self.capacity;
    }

    /// Emit particles from `origin` toward `target`.
    pub fn cast(
        &mut self,
        origin: (f32, f32),
        target: (f32, f32),
        rng: &mut Rng,
        out: &mut Vec<Particle>,
    ) {
        let (dx, dy) = (target.0 - origin.0, target.1 - origin.1);
        let base = dy.atan2(dx);
        for _ in 0..self.rate {
            if self.volume < self.cost {
                break;
            }
            self.volume -= self.cost;
            let angle = base + (rng.f32() * 2.0 - 1.0) * self.spread;
            let speed = self.speed * (0.85 + 0.3 * rng.f32());
            out.push(Particle {
                x: origin.0,
                y: origin.1,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed,
                mat: self.material,
                life: 0,
            });
        }
    }

    pub fn tick(&mut self) {
        self.volume = (self.volume + self.regen).min(self.capacity);
    }
}
