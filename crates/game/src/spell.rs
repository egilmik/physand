//! The spell system. A [`Flask`] combines:
//!
//! - **reagents** (1–3 materials) — what comes out,
//! - a **form** — how it is delivered (spray, orb, rain, wall),
//! - **catalysts** (0–2) — modifiers (heat, frost, volatile, bloom, transmute).
//!
//! Spells never script special cases. They only place material, change
//! temperature, or trigger explosions, and the simulation does the rest:
//! hot oil ignites, hot sand becomes glass, frost turns lava into obsidian.

use physand_sim::{
    id,
    material::{kind, props},
    rng::Rng,
    Kind, World,
};

pub const MAX_REAGENTS: usize = 3;
pub const MAX_CATALYSTS: usize = 2;

const PARTICLE_MAX_LIFE: u16 = 240;
/// Temperature that Heat gives deposited material (above every ignition,
/// boiling and glass-making threshold).
const HOT_TEMP: u8 = 230;
/// Temperature that Frost gives deposited material.
const COLD_TEMP: u8 = 0;
/// Temperature change applied to whatever a hot/cold spell hits.
const IMPACT_HEAT: i32 = 120;

// ---------------------------------------------------------------------------
// Forms and catalysts
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// Continuous stream of droplets.
    Spray,
    /// A slow projectile that bursts into a blob on impact.
    Orb,
    /// Droplets falling from the ceiling above the aim point.
    Rain,
    /// An instant barrier across the aim direction.
    Wall,
}

impl Form {
    pub const ALL: [Form; 4] = [Form::Spray, Form::Orb, Form::Rain, Form::Wall];

    pub fn name(self) -> &'static str {
        match self {
            Form::Spray => "Spray",
            Form::Orb => "Orb",
            Form::Rain => "Rain",
            Form::Wall => "Wall",
        }
    }

    /// Volume per emission (per droplet for streams, per cast otherwise).
    fn base_cost(self) -> f32 {
        match self {
            Form::Spray => 1.0,
            Form::Orb => 30.0,
            Form::Rain => 0.8,
            Form::Wall => 40.0,
        }
    }

    /// Ticks between casts.
    fn cooldown(self) -> u16 {
        match self {
            Form::Spray | Form::Rain => 0,
            Form::Orb => 24,
            Form::Wall => 30,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Catalyst {
    /// Deposits scalding material and heats what it hits.
    Heat,
    /// Deposits frozen material and chills what it hits.
    Frost,
    /// Explodes on impact.
    Volatile,
    /// More material per cast.
    Bloom,
    /// Turns what it hits into the reagent instead of adding material.
    Transmute,
}

impl Catalyst {
    pub const ALL: [Catalyst; 5] = [
        Catalyst::Heat,
        Catalyst::Frost,
        Catalyst::Volatile,
        Catalyst::Bloom,
        Catalyst::Transmute,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Catalyst::Heat => "Heat",
            Catalyst::Frost => "Frost",
            Catalyst::Volatile => "Volatile",
            Catalyst::Bloom => "Bloom",
            Catalyst::Transmute => "Transmute",
        }
    }

    fn adjective(self) -> &'static str {
        match self {
            Catalyst::Heat => "Searing",
            Catalyst::Frost => "Frozen",
            Catalyst::Volatile => "Volatile",
            Catalyst::Bloom => "Blooming",
            Catalyst::Transmute => "Transmuting",
        }
    }

    fn cost_multiplier(self) -> f32 {
        match self {
            Catalyst::Heat | Catalyst::Frost => 1.3,
            Catalyst::Volatile => 2.0,
            Catalyst::Bloom => 1.6,
            Catalyst::Transmute => 2.5,
        }
    }
}

/// What a projectile carries: the catalysts' combined effect.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Payload {
    /// Temperature for deposited cells (`None` = material default).
    pub temp: Option<u8>,
    /// Temperature change for the cells that are hit.
    pub impact_heat: i32,
    pub volatile: bool,
    pub bloom: bool,
    pub transmute: bool,
}

impl Payload {
    fn from_catalysts(catalysts: &[Catalyst]) -> Payload {
        let has = |c| catalysts.contains(&c);
        // Heat and Frost cancel out.
        let (temp, impact_heat) = match (has(Catalyst::Heat), has(Catalyst::Frost)) {
            (true, false) => (Some(HOT_TEMP), IMPACT_HEAT),
            (false, true) => (Some(COLD_TEMP), -IMPACT_HEAT),
            _ => (None, 0),
        };
        Payload {
            temp,
            impact_heat,
            volatile: has(Catalyst::Volatile),
            bloom: has(Catalyst::Bloom),
            transmute: has(Catalyst::Transmute),
        }
    }
}

// ---------------------------------------------------------------------------
// Particles
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleKind {
    Droplet,
    Orb,
}

/// A spell projectile. It is not part of the grid while airborne; when it
/// hits something it becomes real cells and the simulation takes over.
#[derive(Clone, Debug)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub kind: ParticleKind,
    /// Reagents (droplets carry one; orbs carry the whole mix).
    mats: [u8; MAX_REAGENTS],
    n_mats: u8,
    pub payload: Payload,
    gravity: f32,
    life: u16,
}

/// Airborne particles fly through empty space and gases.
#[inline]
fn passable(mat: u8) -> bool {
    mat == id::EMPTY || kind(mat) == Kind::Gas
}

/// Spells cannot alter obsidian: it is the one reliable ward.
#[inline]
fn transmutable(mat: u8, into: u8) -> bool {
    mat != id::EMPTY && mat != id::OBSIDIAN && mat != into && kind(mat) != Kind::Gas
}

impl Particle {
    /// Primary material (used for colour).
    pub fn mat(&self) -> u8 {
        self.mats[0]
    }

    fn pick(&self, rng: &mut Rng) -> u8 {
        self.mats[(rng.next_u32() % self.n_mats as u32) as usize]
    }

    /// Advance one tick. Returns `false` once it has landed (or expired).
    pub fn update(&mut self, world: &mut World, rng: &mut Rng) -> bool {
        self.vy += self.gravity;
        self.life += 1;
        // Sub-step so fast particles never tunnel through thin walls.
        let steps = self.vx.abs().max(self.vy.abs()).ceil().max(1.0) as i32;
        let (sx, sy) = (self.vx / steps as f32, self.vy / steps as f32);
        for _ in 0..steps {
            let (nx, ny) = (self.x + sx, self.y + sy);
            let hit = (nx.floor() as i32, ny.floor() as i32);
            if !passable(world.get(hit.0, hit.1)) {
                self.land(world, rng, Some(hit));
                return false;
            }
            self.x = nx;
            self.y = ny;
        }
        if self.life >= PARTICLE_MAX_LIFE {
            self.land(world, rng, None);
            return false;
        }
        true
    }

    /// Resolve the impact. `hit` is the solid cell struck, if any.
    fn land(&self, world: &mut World, rng: &mut Rng, hit: Option<(i32, i32)>) {
        let here = (self.x.floor() as i32, self.y.floor() as i32);
        let pl = self.payload;
        let radius = match (self.kind, pl.bloom) {
            (ParticleKind::Droplet, _) => 0,
            (ParticleKind::Orb, false) => 4,
            (ParticleKind::Orb, true) => 6,
        };

        if pl.volatile {
            let blast = if self.kind == ParticleKind::Orb { 7 } else { 2 };
            world.explode(here.0, here.1, blast);
        }

        // Heat or chill whatever was struck.
        if pl.impact_heat != 0 {
            let centre = hit.unwrap_or(here);
            for_disc(centre, radius.max(1), |x, y| {
                world.add_temp(x, y, pl.impact_heat)
            });
        }

        if pl.transmute {
            if let Some(centre) = hit {
                for_disc(centre, radius.max(1), |x, y| {
                    let into = self.pick(rng);
                    if transmutable(world.get(x, y), into) {
                        place(world, x, y, into, pl.temp);
                    }
                });
            }
            return;
        }

        match self.kind {
            ParticleKind::Droplet => {
                let count = if pl.bloom { 2 } else { 1 };
                let mut placed = 0;
                for (dx, dy) in [(0, 0), (0, -1), (-1, 0), (1, 0), (0, -2)] {
                    if placed == count {
                        break;
                    }
                    let (x, y) = (here.0 + dx, here.1 + dy);
                    if passable(world.get(x, y)) {
                        place(world, x, y, self.mats[0], pl.temp);
                        placed += 1;
                    }
                }
            }
            ParticleKind::Orb => {
                for_disc(here, radius, |x, y| {
                    if passable(world.get(x, y)) {
                        let m = self.pick(rng);
                        place(world, x, y, m, pl.temp);
                    }
                });
            }
        }
    }
}

fn place(world: &mut World, x: i32, y: i32, mat: u8, temp: Option<u8>) {
    match temp {
        Some(t) => world.set_with_temp(x, y, mat, t),
        None => world.set(x, y, mat),
    }
}

fn for_disc(centre: (i32, i32), r: i32, mut f: impl FnMut(i32, i32)) {
    for y in (centre.1 - r)..=(centre.1 + r) {
        for x in (centre.0 - r)..=(centre.0 + r) {
            if (x - centre.0).pow(2) + (y - centre.1).pow(2) <= r * r {
                f(x, y);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Flasks
// ---------------------------------------------------------------------------

/// Everything a cast needs from the game.
pub struct CastCtx<'a> {
    pub world: &'a mut World,
    pub rng: &'a mut Rng,
    pub particles: &'a mut Vec<Particle>,
    /// Where the spell leaves the caster.
    pub origin: (f32, f32),
    /// Aim point.
    pub target: (f32, f32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlaskError {
    NoReagent,
    TooManyReagents,
    TooManyCatalysts,
    NotAReagent(u8),
    DuplicateCatalyst,
}

#[derive(Clone, Debug)]
pub struct Flask {
    reagents: Vec<u8>,
    form: Form,
    catalysts: Vec<Catalyst>,
    pub volume: f32,
    pub capacity: f32,
    /// Volume regained per tick.
    pub regen: f32,
    cooldown: u16,
    /// Round-robin index for multi-reagent streams.
    cursor: usize,
}

impl Flask {
    pub fn new(reagents: &[u8], form: Form, catalysts: &[Catalyst]) -> Result<Flask, FlaskError> {
        let mut f = Flask {
            reagents: vec![id::WATER],
            form,
            catalysts: Vec::new(),
            volume: 300.0,
            capacity: 300.0,
            regen: 0.6,
            cooldown: 0,
            cursor: 0,
        };
        f.configure(reagents, form, catalysts)?;
        Ok(f)
    }

    /// Change the recipe. The volume is kept.
    pub fn configure(
        &mut self,
        reagents: &[u8],
        form: Form,
        catalysts: &[Catalyst],
    ) -> Result<(), FlaskError> {
        if reagents.is_empty() {
            return Err(FlaskError::NoReagent);
        }
        if reagents.len() > MAX_REAGENTS {
            return Err(FlaskError::TooManyReagents);
        }
        if catalysts.len() > MAX_CATALYSTS {
            return Err(FlaskError::TooManyCatalysts);
        }
        if let Some(&bad) = reagents.iter().find(|&&m| !props(m).reagent) {
            return Err(FlaskError::NotAReagent(bad));
        }
        if catalysts.len() == 2 && catalysts[0] == catalysts[1] {
            return Err(FlaskError::DuplicateCatalyst);
        }
        self.reagents = reagents.to_vec();
        self.form = form;
        self.catalysts = catalysts.to_vec();
        self.cursor = 0;
        Ok(())
    }

    pub fn reagents(&self) -> &[u8] {
        &self.reagents
    }

    pub fn form(&self) -> Form {
        self.form
    }

    pub fn catalysts(&self) -> &[Catalyst] {
        &self.catalysts
    }

    /// Generated name, e.g. "Searing Oil Orb" or "Volatile Water-Acid Spray".
    pub fn name(&self) -> String {
        let mut parts: Vec<String> = self
            .catalysts
            .iter()
            .map(|c| c.adjective().to_string())
            .collect();
        let reagents: Vec<&str> = self.reagents.iter().map(|&m| props(m).name).collect();
        parts.push(reagents.join("-"));
        parts.push(self.form.name().to_string());
        parts.join(" ")
    }

    /// Colour of the first reagent (HUD, flask glow).
    pub fn color(&self) -> [u8; 3] {
        props(self.reagents[0]).color
    }

    /// Volume spent per emission.
    pub fn cost(&self) -> f32 {
        self.catalysts
            .iter()
            .fold(self.form.base_cost(), |c, cat| c * cat.cost_multiplier())
    }

    pub fn refill(&mut self) {
        self.volume = self.capacity;
    }

    pub fn tick(&mut self) {
        self.volume = (self.volume + self.regen).min(self.capacity);
        self.cooldown = self.cooldown.saturating_sub(1);
    }

    fn next_reagent(&mut self) -> u8 {
        let m = self.reagents[self.cursor % self.reagents.len()];
        self.cursor = self.cursor.wrapping_add(1);
        m
    }

    fn spend(&mut self) -> bool {
        let cost = self.cost();
        if self.volume < cost {
            return false;
        }
        self.volume -= cost;
        true
    }

    fn droplet(&mut self, x: f32, y: f32, vx: f32, vy: f32, gravity: f32) -> Particle {
        let mat = self.next_reagent();
        Particle {
            x,
            y,
            vx,
            vy,
            kind: ParticleKind::Droplet,
            mats: [mat; MAX_REAGENTS],
            n_mats: 1,
            payload: Payload::from_catalysts(&self.catalysts),
            gravity,
            life: 0,
        }
    }

    /// Cast for one tick (call every tick the button is held).
    pub fn cast(&mut self, ctx: &mut CastCtx) {
        if self.cooldown > 0 {
            return;
        }
        let (ox, oy) = ctx.origin;
        let (tx, ty) = ctx.target;
        let aim = (ty - oy).atan2(tx - ox);

        match self.form {
            Form::Spray => {
                for _ in 0..4 {
                    if !self.spend() {
                        break;
                    }
                    let angle = aim + (ctx.rng.f32() * 2.0 - 1.0) * 0.10;
                    let speed = 3.2 * (0.85 + 0.3 * ctx.rng.f32());
                    let p = self.droplet(ox, oy, angle.cos() * speed, angle.sin() * speed, 0.06);
                    ctx.particles.push(p);
                }
            }
            Form::Rain => {
                let ceiling = ceiling_above(ctx.world, tx, ty, 60);
                let half_width = if self.catalysts.contains(&Catalyst::Bloom) {
                    16.0
                } else {
                    10.0
                };
                for _ in 0..3 {
                    if !self.spend() {
                        break;
                    }
                    let x = tx + (ctx.rng.f32() * 2.0 - 1.0) * half_width;
                    let p = self.droplet(x, ceiling, 0.0, 0.5, 0.06);
                    ctx.particles.push(p);
                }
            }
            Form::Orb => {
                if !self.spend() {
                    return;
                }
                let speed = 2.8;
                let mut mats = [self.reagents[0]; MAX_REAGENTS];
                mats[..self.reagents.len()].copy_from_slice(&self.reagents);
                ctx.particles.push(Particle {
                    x: ox,
                    y: oy,
                    vx: aim.cos() * speed,
                    vy: aim.sin() * speed,
                    kind: ParticleKind::Orb,
                    mats,
                    n_mats: self.reagents.len() as u8,
                    payload: Payload::from_catalysts(&self.catalysts),
                    gravity: 0.035,
                    life: 0,
                });
            }
            Form::Wall => {
                if !self.spend() {
                    return;
                }
                self.build_wall(ctx, aim);
            }
        }
        self.cooldown = self.form.cooldown();
    }

    /// Place a two-cell-thick barrier across the aim direction.
    fn build_wall(&mut self, ctx: &mut CastCtx, aim: f32) {
        const REACH: f32 = 40.0;
        let (ox, oy) = ctx.origin;
        let (tx, ty) = ctx.target;
        let dist = ((tx - ox).powi(2) + (ty - oy).powi(2)).sqrt().min(REACH);
        let (dx, dy) = (aim.cos(), aim.sin());
        let (cx, cy) = (ox + dx * dist, oy + dy * dist);
        let payload = Payload::from_catalysts(&self.catalysts);
        let half = if payload.bloom { 14 } else { 8 };

        if payload.volatile {
            ctx.world.explode(cx as i32, cy as i32, 5);
        }
        for i in -half..=half {
            for j in 0..2 {
                let x = (cx - dy * i as f32 + dx * j as f32).round() as i32;
                let y = (cy + dx * i as f32 + dy * j as f32).round() as i32;
                let mat = self.next_reagent();
                let here = ctx.world.get(x, y);
                let ok = if payload.transmute {
                    transmutable(here, mat)
                } else {
                    passable(here)
                };
                if ok && ctx.world.in_bounds(x, y) {
                    place(ctx.world, x, y, mat, payload.temp);
                }
            }
        }
    }
}

/// The y just below the first solid cell above `(x, y)`, searching up to `max` cells.
fn ceiling_above(world: &World, x: f32, y: f32, max: i32) -> f32 {
    let cx = x.floor() as i32;
    let mut cy = y.floor() as i32;
    // Aiming into solid ground rains onto its surface.
    for _ in 0..max {
        if passable(world.get(cx, cy)) {
            break;
        }
        cy -= 1;
    }
    let mut top = cy;
    for dy in 1..=max {
        if !passable(world.get(cx, cy - dy)) {
            break;
        }
        top = cy - dy;
    }
    top as f32 + 0.5
}

/// The flasks a new run starts with.
pub fn starter_flasks() -> Vec<Flask> {
    let mk = |r: &[u8], f, c: &[Catalyst]| Flask::new(r, f, c).expect("valid starter flask");
    vec![
        mk(&[id::WATER], Form::Spray, &[]),
        mk(&[id::OIL], Form::Orb, &[Catalyst::Heat]),
        mk(&[id::SAND], Form::Wall, &[Catalyst::Heat]),
        mk(&[id::ACID], Form::Rain, &[]),
    ]
}
