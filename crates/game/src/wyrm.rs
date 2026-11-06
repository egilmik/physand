//! The Sandwyrm: a chain of packed-sand segments that swims through the
//! dunes and lunges out at the alchemist.
//!
//! It is a material creature. Water erodes its segments; heat glazes them
//! into brittle glass that shatters under explosions and impacts; every
//! destroyed segment spills real sand (or glass) into the arena. Its head is
//! armoured until its body has been whittled down.

use physand_sim::{id, material::kind, rng::Rng, Kind, World};

use crate::body::Body;
use crate::enemy::EnemyEvent;
use crate::spell::{Particle, ParticleKind};

pub const SEGMENTS: usize = 14;
pub const SEGMENT_RADIUS: f32 = 4.0;
pub const HEAD_RADIUS: f32 = 5.0;
const SPACING: f32 = 6.0;
pub const SEGMENT_HP: f32 = 24.0;
pub const HEAD_HP: f32 = 160.0;
/// Heat at which a segment glazes into glass.
pub const GLASS_HEAT: f32 = 100.0;
/// While more segments than this remain, the head shrugs off most damage.
pub const ARMOURED_ABOVE: usize = 6;
const ATTACK_COOLDOWN: u8 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Swimming beneath the player, biding its time.
    Stalk,
    /// Driving up at the player.
    Lunge,
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub x: f32,
    pub y: f32,
    pub hp: f32,
    pub heat: f32,
    pub glassed: bool,
    pub hurt_flash: u8,
}

#[derive(Clone, Debug)]
pub struct Wyrm {
    /// `segments[0]` is the head; its `hp` is the head's health.
    pub segments: Vec<Segment>,
    vx: f32,
    vy: f32,
    mode: Mode,
    timer: u32,
    /// Ticks of frost-slowed movement.
    pub chilled: u16,
    attack_cd: u8,
    pub anim: u32,
    max_hp: f32,
}

impl Wyrm {
    pub fn new(x: f32, y: f32) -> Wyrm {
        let segments = (0..SEGMENTS)
            .map(|i| Segment {
                x: x + i as f32 * SPACING,
                y,
                hp: if i == 0 { HEAD_HP } else { SEGMENT_HP },
                heat: 0.0,
                glassed: false,
                hurt_flash: 0,
            })
            .collect();
        Wyrm {
            segments,
            vx: -1.0,
            vy: 0.0,
            mode: Mode::Stalk,
            timer: 150,
            chilled: 0,
            attack_cd: 0,
            anim: 0,
            max_hp: HEAD_HP + SEGMENT_HP * (SEGMENTS - 1) as f32,
        }
    }

    pub fn alive(&self) -> bool {
        self.segments[0].hp > 0.0
    }

    pub fn head(&self) -> &Segment {
        &self.segments[0]
    }

    pub fn total_hp(&self) -> f32 {
        self.segments.iter().map(|s| s.hp.max(0.0)).sum()
    }

    pub fn max_hp(&self) -> f32 {
        self.max_hp
    }

    pub fn armoured(&self) -> bool {
        self.segments.len() > ARMOURED_ABOVE
    }

    fn radius(i: usize) -> f32 {
        if i == 0 {
            HEAD_RADIUS
        } else {
            SEGMENT_RADIUS
        }
    }

    /// Which segment covers a point.
    pub fn segment_at(&self, x: f32, y: f32) -> Option<usize> {
        self.segments.iter().enumerate().position(|(i, s)| {
            let r = Self::radius(i);
            (s.x - x).powi(2) + (s.y - y).powi(2) <= r * r
        })
    }

    /// Hurt one segment. Glass segments take triple damage; the head is
    /// armoured while the body is long.
    pub fn damage(&mut self, i: usize, amount: f32) {
        let armour = if i == 0 && self.armoured() { 0.25 } else { 1.0 };
        let Some(s) = self.segments.get_mut(i) else {
            return;
        };
        let brittle = if s.glassed { 3.0 } else { 1.0 };
        s.hp -= amount * armour * brittle;
        s.hurt_flash = 5;
    }

    /// Water wears sand away; glass is immune to erosion.
    pub fn erode(&mut self, i: usize, amount: f32) {
        let armour = if i == 0 && self.armoured() { 0.25 } else { 1.0 };
        if let Some(s) = self.segments.get_mut(i) {
            if !s.glassed {
                s.hp -= amount * armour;
                s.hurt_flash = 3;
            }
        }
    }

    pub fn heat(&mut self, i: usize, amount: f32) {
        if let Some(s) = self.segments.get_mut(i) {
            s.heat += amount;
            if s.heat >= GLASS_HEAT {
                s.glassed = true;
            }
        }
    }

    /// Resolve a spell particle striking segment `i`.
    pub fn struck_by(&mut self, i: usize, p: &Particle) {
        let orb = p.kind == ParticleKind::Orb;
        let scale = if orb { 4.0 } else { 1.0 };
        match p.mat() {
            id::WATER => self.erode(i, 3.0 * scale),
            id::ACID => self.damage(i, 3.0 * scale),
            id::LAVA => {
                self.damage(i, 3.0 * scale);
                self.heat(i, 10.0 * scale);
            }
            _ => self.damage(i, if orb { 8.0 } else { 0.5 }),
        }
        if p.payload.impact_heat > 0 {
            self.heat(i, if orb { 60.0 } else { 8.0 });
        } else if p.payload.impact_heat < 0 {
            self.chilled = 90;
            if let Some(s) = self.segments.get_mut(i) {
                s.heat = 0.0;
            }
        }
        if p.payload.transmute {
            self.damage(i, 20.0);
        }
    }

    /// Advance one tick.
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
        self.attack_cd = self.attack_cd.saturating_sub(1);
        self.chilled = self.chilled.saturating_sub(1);
        for s in &mut self.segments {
            s.hurt_flash = s.hurt_flash.saturating_sub(1);
        }

        self.steer(world, target, rng);
        self.carve(world, rng);
        self.follow();
        self.environment(world);
        self.shed_dead_segments(world);

        // Contact damage.
        let t = target?;
        if self.attack_cd > 0 {
            return None;
        }
        let reach = Body::new(t.x - 1, t.y - 1, t.w + 2, t.h + 2);
        for (i, s) in self.segments.iter().enumerate() {
            let r = Self::radius(i);
            let hit = (s.x + r) as i32 >= reach.x
                && ((s.x - r) as i32) < reach.x + reach.w
                && (s.y + r) as i32 >= reach.y
                && ((s.y - r) as i32) < reach.y + reach.h;
            if hit {
                self.attack_cd = ATTACK_COOLDOWN;
                let dir = if t.center().0 >= s.x { 1.0 } else { -1.0 };
                return Some(EnemyEvent::Hit {
                    damage: if i == 0 { 20.0 } else { 10.0 },
                    knock_x: dir * 3.0,
                });
            }
        }
        None
    }

    /// Whether the head is surrounded by something it can swim through. The
    /// head keeps its own spot hollow, so look at a ring just outside it.
    fn buried(&self, world: &World) -> bool {
        let h = self.head();
        let r = HEAD_RADIUS + 2.0;
        let solid = (0..8)
            .filter(|&k| {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let (x, y) = (h.x + a.cos() * r, h.y + a.sin() * r);
                matches!(
                    kind(world.get(x as i32, y as i32)),
                    Kind::Solid | Kind::Powder
                )
            })
            .count();
        solid >= 3
    }

    fn steer(&mut self, world: &World, target: Option<&Body>, rng: &mut Rng) {
        let buried = self.buried(world);
        let slow = if self.chilled > 0 { 0.4 } else { 1.0 };
        let (hx, hy) = (self.head().x, self.head().y);

        if buried {
            self.timer = self.timer.saturating_sub(1);
            let (goal, speed) = match (self.mode, target) {
                (Mode::Lunge, Some(t)) => (t.center(), 2.6),
                (Mode::Stalk, Some(t)) => {
                    let (tx, ty) = t.center();
                    let sway = ((self.anim as f32) * 0.03).sin() * 40.0;
                    // Stay under the surface: push the goal down into the ground.
                    let gx = tx + sway;
                    let mut gy = ty + 45.0;
                    while gy < ty + 160.0
                        && !matches!(
                            kind(world.get(gx as i32, gy as i32)),
                            Kind::Solid | Kind::Powder
                        )
                    {
                        gy += 4.0;
                    }
                    ((gx, gy + 10.0), 1.5)
                }
                (_, None) => ((hx + self.vx * 20.0, hy), 1.2),
            };
            if self.mode == Mode::Stalk && self.timer == 0 {
                self.mode = Mode::Lunge;
                self.timer = 90;
            } else if self.mode == Mode::Lunge && self.timer == 0 {
                // Lunge timed out without breaking the surface: try again later.
                self.mode = Mode::Stalk;
                self.timer = rng.range(120, 220);
            }
            // Turn gradually toward the goal.
            let want = (goal.1 - hy).atan2(goal.0 - hx);
            let have = self.vy.atan2(self.vx);
            let mut diff = want - have;
            while diff > std::f32::consts::PI {
                diff -= std::f32::consts::TAU;
            }
            while diff < -std::f32::consts::PI {
                diff += std::f32::consts::TAU;
            }
            let angle = have + diff.clamp(-0.09, 0.09);
            self.vx = angle.cos() * speed * slow;
            self.vy = angle.sin() * speed * slow;
        } else {
            // Airborne (or swimming in liquid): a ballistic arc back down.
            if self.mode == Mode::Lunge {
                self.mode = Mode::Stalk;
                self.timer = rng.range(150, 260);
            }
            self.vy = (self.vy + 0.09).min(3.0);
        }
        let h = &mut self.segments[0];
        h.x += self.vx;
        h.y += self.vy;
        // Stay inside the world.
        let (w, hgt) = (world.width() as f32, world.height() as f32);
        if h.x < 8.0 || h.x > w - 8.0 {
            self.vx = -self.vx;
            h.x = h.x.clamp(8.0, w - 8.0);
        }
        if h.y < 8.0 || h.y > hgt - 8.0 {
            self.vy = -self.vy.abs() * if h.y < 8.0 { -1.0 } else { 1.0 };
            h.y = h.y.clamp(8.0, hgt - 8.0);
        }
    }

    /// The head swims through sand and grinds rock into sand (never obsidian
    /// or wax). Displaced material is pushed behind the head into the tunnel
    /// it just left, so the dunes close up again rather than emptying.
    fn carve(&mut self, world: &mut World, rng: &mut Rng) {
        let (hx, hy) = (self.head().x, self.head().y);
        let speed = (self.vx * self.vx + self.vy * self.vy).sqrt().max(0.01);
        let (bx, by) = (-self.vx / speed, -self.vy / speed);
        let r = HEAD_RADIUS as i32;
        let (cx, cy) = (hx as i32, hy as i32);
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                if (x - cx).pow(2) + (y - cy).pow(2) > r * r || !world.in_bounds(x, y) {
                    continue;
                }
                let m = world.get(x, y);
                if matches!(m, id::EMPTY | id::OBSIDIAN | id::WAX)
                    || !matches!(kind(m), Kind::Solid | Kind::Powder)
                {
                    continue;
                }
                let moved = if kind(m) == Kind::Powder { m } else { id::SAND };
                // Find an open cell behind the head (with a little scatter);
                // only carve if the material has somewhere to go, so the
                // arena's sand is conserved.
                let jitter = (rng.f32() - 0.5) * 1.2;
                let (dx, dy) = (bx - by * jitter, by + bx * jitter);
                for step in (r + 1)..(3 * r + 6) {
                    let (tx, ty) = (
                        (hx + dx * step as f32).round() as i32,
                        (hy + dy * step as f32).round() as i32,
                    );
                    if world.get(tx, ty) == id::EMPTY && world.in_bounds(tx, ty) {
                        world.set(x, y, id::EMPTY);
                        world.set(tx, ty, moved);
                        break;
                    }
                }
            }
        }
    }

    /// Body segments trail the head at fixed spacing.
    fn follow(&mut self) {
        for i in 1..self.segments.len() {
            let (px, py) = (self.segments[i - 1].x, self.segments[i - 1].y);
            let s = &mut self.segments[i];
            let (dx, dy) = (px - s.x, py - s.y);
            let d = (dx * dx + dy * dy).sqrt();
            if d > SPACING {
                s.x = px - dx / d * SPACING;
                s.y = py - dy / d * SPACING;
            }
        }
    }

    /// Water erodes, fire and lava glaze, heat slowly fades.
    fn environment(&mut self, world: &World) {
        for i in 0..self.segments.len() {
            let (x, y) = (self.segments[i].x as i32, self.segments[i].y as i32);
            let r = Self::radius(i) as i32;
            let (mut water, mut hot) = (0, 0.0);
            for (dx, dy) in [(0, 0), (r, 0), (-r, 0), (0, r), (0, -r)] {
                match world.get(x + dx, y + dy) {
                    id::WATER => water += 1,
                    id::FIRE => hot += 2.0,
                    id::LAVA => hot += 6.0,
                    _ => {}
                }
            }
            if water > 0 {
                self.erode(i, 0.15 * water as f32);
            }
            if hot > 0.0 {
                self.heat(i, hot);
            }
            let s = &mut self.segments[i];
            s.heat = (s.heat - 0.3).max(0.0);
        }
    }

    /// Remove destroyed body segments, spilling them into the world.
    fn shed_dead_segments(&mut self, world: &mut World) {
        let mut i = 1;
        while i < self.segments.len() {
            if self.segments[i].hp <= 0.0 {
                let s = self.segments.remove(i);
                spill(world, &s, SEGMENT_RADIUS);
            } else {
                i += 1;
            }
        }
    }

    /// The whole body collapses into the arena.
    pub fn remains(&self, world: &mut World) {
        for (i, s) in self.segments.iter().enumerate() {
            spill(world, s, Self::radius(i));
        }
    }
}

fn spill(world: &mut World, s: &Segment, r: f32) {
    let mat = if s.glassed { id::GLASS } else { id::SAND };
    let (cx, cy, r) = (s.x as i32, s.y as i32, r as i32 - 1);
    for y in cy - r..=cy + r {
        for x in cx - r..=cx + r {
            if (x - cx).pow(2) + (y - cy).pow(2) <= r * r
                && world.get(x, y) == id::EMPTY
                && world.in_bounds(x, y)
            {
                world.set(x, y, mat);
            }
        }
    }
}
