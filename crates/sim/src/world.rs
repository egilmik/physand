use crate::material::{can_displace, id, kind, props, Kind, Outcome, AMBIENT, MATERIALS};
use crate::reaction;
use crate::rng::Rng;

const NEIGHBOURS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// Side length of a simulation chunk. Chunks with no activity sleep.
pub const CHUNK: i32 = 32;

/// Fire with more life than this is burning fuel (embers) and does not move.
pub const EMBER_LIFE: u8 = 70;

/// Blast radius of a single exploding gunpowder grain.
pub const GUNPOWDER_RADIUS: i32 = 4;

/// Explosions resolved per tick; the rest carry over so chain reactions
/// ripple outward instead of stalling a frame.
const EXPLOSION_BUDGET: usize = 96;

/// Temperature differences below this do not flow, so neighbours settle
/// and their chunks can go to sleep.
const HEAT_DEADBAND: i32 = 4;

/// Materials that never need an update: empty space, and solids with no
/// lifetime, no heat conduction and no phase changes.
static INERT: [bool; MATERIALS.len()] = {
    let mut t = [false; MATERIALS.len()];
    let mut i = 0;
    while i < MATERIALS.len() {
        let p = &MATERIALS[i];
        t[i] = matches!(p.kind, Kind::Empty)
            || (matches!(p.kind, Kind::Solid)
                && p.conduct == 0.0
                && p.life.1 == 0
                && p.on_hot.is_none()
                && p.on_cold.is_none());
        i += 1;
    }
    t
};

#[inline]
fn is_inert(mat: u8) -> bool {
    INERT[mat as usize]
}

/// One simulated cell.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    /// Material id (see [`crate::material::id`]).
    pub mat: u8,
    /// Per-cell colour variation seed.
    pub var: u8,
    /// Remaining lifetime for transient materials (fire, steam, smoke).
    pub life: u8,
    /// Stamp of the last tick this cell was updated; prevents double moves.
    pub clock: u8,
    /// Temperature (see [`AMBIENT`]).
    pub temp: u8,
}

/// The per-cell data the renderer needs: `[mat, var, life, temp]`, uploaded
/// to the GPU as one RGBA8 texel.
pub type GpuCell = [u8; 4];

impl Cell {
    pub const EMPTY: Cell = Cell {
        mat: id::EMPTY,
        var: 0,
        life: 0,
        clock: 0,
        temp: AMBIENT,
    };

    pub fn new(mat: u8, rng: &mut Rng) -> Cell {
        let p = props(mat);
        let (lo, hi) = p.life;
        let life = if hi == 0 {
            0
        } else {
            rng.range(lo as u32, hi as u32) as u8
        };
        Cell {
            mat,
            var: rng.next_u32() as u8,
            life,
            clock: 0,
            temp: p.base_temp,
        }
    }

    #[inline]
    pub fn gpu(&self) -> GpuCell {
        [self.mat, self.var, self.life, self.temp]
    }
}

/// An explosion that happened, for the game layer (damage, knockback, shake).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Explosion {
    pub x: i32,
    pub y: i32,
    pub radius: i32,
}

/// The falling-sand grid. `y = 0` is the top row.
///
/// The grid is divided into [`CHUNK`]-sized chunks. A chunk is only updated
/// if something changed in it (or next to it) during the previous tick, so
/// settled terrain costs nothing.
pub struct World {
    width: i32,
    height: i32,
    cells: Vec<Cell>,
    rng: Rng,
    tick: u32,
    chunks_w: i32,
    chunks_h: i32,
    /// Chunks to update this tick.
    awake: Vec<bool>,
    /// Chunks woken during this tick, to update next tick.
    next_awake: Vec<bool>,
    /// Explosions queued by chain reactions, resolved at the end of a tick.
    pending: Vec<Explosion>,
    /// Explosions that happened since the game layer last drained them.
    explosions: Vec<Explosion>,
}

impl World {
    pub fn new(width: u32, height: u32, seed: u32) -> World {
        assert!(width > 0 && height > 0);
        let chunks_w = (width as i32 + CHUNK - 1) / CHUNK;
        let chunks_h = (height as i32 + CHUNK - 1) / CHUNK;
        let n = (chunks_w * chunks_h) as usize;
        World {
            width: width as i32,
            height: height as i32,
            cells: vec![Cell::EMPTY; (width * height) as usize],
            rng: Rng::new(seed),
            tick: 0,
            chunks_w,
            chunks_h,
            awake: vec![true; n],
            next_awake: vec![true; n],
            pending: Vec::new(),
            explosions: Vec::new(),
        }
    }

    // ---------------------------------------------------------------------
    // Chunk activity
    // ---------------------------------------------------------------------

    /// Mark the chunk containing `(x, y)` for update next tick, plus the
    /// neighbouring chunk(s) when the cell sits on a chunk border.
    #[inline]
    fn wake(&mut self, x: i32, y: i32) {
        let (cx, cy) = (x / CHUNK, y / CHUNK);
        let (lx, ly) = (x % CHUNK, y % CHUNK);
        let x0 = if lx == 0 { cx - 1 } else { cx };
        let x1 = if lx == CHUNK - 1 { cx + 1 } else { cx };
        let y0 = if ly == 0 { cy - 1 } else { cy };
        let y1 = if ly == CHUNK - 1 { cy + 1 } else { cy };
        for ny in y0.max(0)..=y1.min(self.chunks_h - 1) {
            for nx in x0.max(0)..=x1.min(self.chunks_w - 1) {
                self.next_awake[(ny * self.chunks_w + nx) as usize] = true;
            }
        }
    }

    /// Number of chunks that will be simulated next tick (debug/perf HUD).
    pub fn awake_chunks(&self) -> usize {
        self.next_awake.iter().filter(|&&a| a).count()
    }

    pub fn chunk_count(&self) -> usize {
        self.next_awake.len()
    }

    /// Wake every chunk, e.g. after bulk edits.
    pub fn wake_all(&mut self) {
        self.next_awake.fill(true);
    }

    // ---------------------------------------------------------------------
    // Access
    // ---------------------------------------------------------------------

    pub fn width(&self) -> u32 {
        self.width as u32
    }

    pub fn height(&self) -> u32 {
        self.height as u32
    }

    pub fn tick_count(&self) -> u32 {
        self.tick
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    #[inline]
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width && y < self.height
    }

    #[inline]
    fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.width + x) as usize
    }

    /// Material at `(x, y)`; out of bounds reads as rock (the world is walled).
    #[inline]
    pub fn get(&self, x: i32, y: i32) -> u8 {
        if self.in_bounds(x, y) {
            self.cells[self.idx(x, y)].mat
        } else {
            id::ROCK
        }
    }

    /// Full cell at `(x, y)`, if in bounds.
    #[inline]
    pub fn cell(&self, x: i32, y: i32) -> Option<Cell> {
        self.in_bounds(x, y).then(|| self.cells[self.idx(x, y)])
    }

    /// Replace the cell at `(x, y)` with a fresh cell of `mat`.
    pub fn set(&mut self, x: i32, y: i32, mat: u8) {
        if self.in_bounds(x, y) {
            let c = Cell::new(mat, &mut self.rng);
            let i = self.idx(x, y);
            self.cells[i] = c;
            self.wake(x, y);
        }
    }

    /// Like [`World::set`] but with an explicit temperature.
    pub fn set_with_temp(&mut self, x: i32, y: i32, mat: u8, temp: u8) {
        if self.in_bounds(x, y) {
            self.set(x, y, mat);
            let i = self.idx(x, y);
            self.cells[i].temp = temp;
        }
    }

    /// Temperature at `(x, y)` (ambient out of bounds).
    pub fn temp(&self, x: i32, y: i32) -> u8 {
        self.cell(x, y).map_or(AMBIENT, |c| c.temp)
    }

    /// Heat (positive) or chill (negative) a non-empty cell.
    pub fn add_temp(&mut self, x: i32, y: i32, delta: i32) {
        if !self.in_bounds(x, y) {
            return;
        }
        let i = self.idx(x, y);
        if self.cells[i].mat == id::EMPTY {
            return;
        }
        self.cells[i].temp = (self.cells[i].temp as i32 + delta).clamp(0, 255) as u8;
        self.wake(x, y);
    }

    /// Copy a `w × h` window with its top-left at `(x0, y0)` into `out`
    /// (row-major) in GPU layout. Out-of-bounds cells read as rock.
    pub fn copy_region(&self, x0: i32, y0: i32, w: i32, h: i32, out: &mut [GpuCell]) {
        assert_eq!(out.len(), (w * h) as usize);
        let rock: GpuCell = [id::ROCK, 0, 0, AMBIENT];
        for row in 0..h {
            let y = y0 + row;
            let dst = &mut out[(row * w) as usize..((row + 1) * w) as usize];
            if y < 0 || y >= self.height {
                dst.fill(rock);
                continue;
            }
            for (col, d) in dst.iter_mut().enumerate() {
                let x = x0 + col as i32;
                *d = if x >= 0 && x < self.width {
                    self.cells[self.idx(x, y)].gpu()
                } else {
                    rock
                };
            }
        }
    }

    /// Count cells of a given material (handy for tests and debug HUDs).
    pub fn count(&self, mat: u8) -> usize {
        self.cells.iter().filter(|c| c.mat == mat).count()
    }

    /// Paint a filled circle. Painting `EMPTY` erases; other materials only
    /// fill empty cells, and loose materials are scattered for a natural look.
    pub fn paint(&mut self, cx: i32, cy: i32, radius: i32, mat: u8) {
        let r = radius.max(0);
        let solid = kind(mat) == Kind::Solid;
        for y in (cy - r)..=(cy + r) {
            for x in (cx - r)..=(cx + r) {
                let (dx, dy) = (x - cx, y - cy);
                if dx * dx + dy * dy > r * r || !self.in_bounds(x, y) {
                    continue;
                }
                if mat == id::EMPTY {
                    let i = self.idx(x, y);
                    self.cells[i] = Cell::EMPTY;
                    self.wake(x, y);
                } else if self.get(x, y) == id::EMPTY && (solid || self.rng.chance(0.55)) {
                    self.set(x, y, mat);
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::EMPTY);
        self.pending.clear();
        self.wake_all();
    }

    /// Drain the explosions that happened since the last call.
    pub fn take_explosions(&mut self) -> Vec<Explosion> {
        std::mem::take(&mut self.explosions)
    }

    // ---------------------------------------------------------------------
    // Explosions
    // ---------------------------------------------------------------------

    /// Blast a circle: breaks diggable material (rock partly, obsidian never),
    /// sets gunpowder off in a chain, sprays fire and heats what survives.
    pub fn explode(&mut self, cx: i32, cy: i32, radius: i32) {
        let r = radius.max(1);
        self.explosions.push(Explosion {
            x: cx,
            y: cy,
            radius: r,
        });
        for y in (cy - r)..=(cy + r) {
            for x in (cx - r)..=(cx + r) {
                let d2 = (x - cx).pow(2) + (y - cy).pow(2);
                if d2 > r * r || !self.in_bounds(x, y) {
                    continue;
                }
                let t = (d2 as f32).sqrt() / r as f32; // 0 at centre, 1 at edge
                let mat = self.get(x, y);
                let p = props(mat);
                match p.kind {
                    _ if mat == id::GUNPOWDER => {
                        self.set(x, y, id::EMPTY);
                        self.pending.push(Explosion {
                            x,
                            y,
                            radius: GUNPOWDER_RADIUS,
                        });
                    }
                    Kind::Empty | Kind::Gas => {
                        if t > 0.4 && self.rng.chance(0.25) {
                            self.set(x, y, id::FIRE);
                        }
                    }
                    Kind::Liquid => {
                        if t < 0.5 && self.rng.chance(0.3) {
                            self.set(x, y, id::EMPTY);
                        } else {
                            self.add_temp(x, y, (60.0 * (1.0 - t)) as i32);
                        }
                    }
                    Kind::Solid | Kind::Powder => {
                        let strength = (p.dig * 16.0).min(1.0) * (1.0 - 0.5 * t);
                        if self.rng.chance(strength) {
                            let debris = if self.rng.chance(0.15) {
                                id::FIRE
                            } else {
                                id::EMPTY
                            };
                            self.set(x, y, debris);
                        } else {
                            self.add_temp(x, y, (60.0 * (1.0 - t)) as i32);
                        }
                    }
                }
            }
        }
    }

    fn resolve_pending_explosions(&mut self) {
        let n = self.pending.len().min(EXPLOSION_BUDGET);
        let batch: Vec<Explosion> = self.pending.drain(..n).collect();
        for e in batch {
            self.explode(e.x, e.y, e.radius);
        }
    }

    // ---------------------------------------------------------------------
    // Simulation step
    // ---------------------------------------------------------------------

    /// Advance the simulation one step.
    pub fn step(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        let stamp = self.tick as u8;
        let left_to_right = self.tick & 1 == 0;
        std::mem::swap(&mut self.awake, &mut self.next_awake);
        self.next_awake.fill(false);

        // Bottom-to-top so falling material moves at most once per tick;
        // alternate horizontal direction to avoid directional bias.
        for y in (0..self.height).rev() {
            let cy = y / CHUNK;
            for ci in 0..self.chunks_w {
                let cx = if left_to_right {
                    ci
                } else {
                    self.chunks_w - 1 - ci
                };
                if !self.awake[(cy * self.chunks_w + cx) as usize] {
                    continue;
                }
                let x_start = cx * CHUNK;
                let x_end = (x_start + CHUNK).min(self.width);
                for i in 0..(x_end - x_start) {
                    let x = if left_to_right {
                        x_start + i
                    } else {
                        x_end - 1 - i
                    };
                    let idx = self.idx(x, y);
                    let cell = self.cells[idx];
                    // Inert solids (rock) never act on their own; anything that
                    // reacts with them (acid) starts the reaction itself.
                    if is_inert(cell.mat) || cell.clock == stamp {
                        continue;
                    }
                    self.cells[idx].clock = stamp;
                    self.update_cell(x, y, stamp);
                }
            }
        }

        self.resolve_pending_explosions();
    }

    fn update_cell(&mut self, x: i32, y: i32, stamp: u8) {
        let mat = self.get(x, y);

        // Reactions and lifetimes may transform or remove the cell.
        if !self.react(x, y, mat) {
            return;
        }
        // Heat flow and phase changes (melting, freezing, boiling, igniting).
        if self.thermal(x, y) {
            return;
        }
        let mat = self.get(x, y);

        match kind(mat) {
            Kind::Powder => self.move_powder(x, y, stamp),
            Kind::Liquid => {
                // Lava is viscous: it only flows on some ticks.
                if mat != id::LAVA || self.rng.chance(0.35) {
                    self.move_liquid(x, y, props(mat).spread as i32, stamp);
                }
            }
            Kind::Gas if mat == id::FIRE && self.cells[self.idx(x, y)].life > EMBER_LIFE => {
                // Long-lived fire is burning fuel: it stays put and throws off flames.
                if self.rng.chance(0.3) && self.get(x, y - 1) == id::EMPTY {
                    self.replace(x, y - 1, id::FIRE);
                }
            }
            Kind::Gas => self.move_gas(x, y, stamp),
            Kind::Solid | Kind::Empty => {}
        }
    }

    /// Lifetimes and pairwise reactions. Returns `false` if the cell no
    /// longer needs to move (it was consumed or transformed).
    fn react(&mut self, x: i32, y: i32, mat: u8) -> bool {
        // Transient materials age.
        if props(mat).life.1 > 0 {
            let i = self.idx(x, y);
            let life = self.cells[i].life;
            if life == 0 {
                let next = match mat {
                    id::FIRE if self.rng.chance(0.25) => id::SMOKE,
                    id::STEAM if self.rng.chance(0.35) => id::WATER,
                    _ => id::EMPTY,
                };
                self.replace(x, y, next);
                return next != id::EMPTY;
            }
            self.cells[i].life = life - 1;
            self.wake(x, y);
        }

        if mat == id::FIRE {
            return self.react_fire(x, y);
        }

        // Check one random neighbour per tick (cheap, and looks organic).
        let (dx, dy) = NEIGHBOURS[(self.rng.next_u32() % 4) as usize];
        let (nx, ny) = (x + dx, y + dy);
        let other = self.get(nx, ny);
        if self.in_bounds(nx, ny) {
            if other == id::EMPTY {
                // Lava occasionally spits a flame into the air above it.
                if mat == id::LAVA && dy == -1 && self.rng.chance(0.002) {
                    self.replace(nx, ny, id::FIRE);
                }
            } else if let Some((a_to, b_to, chance)) = reaction::lookup(mat, other) {
                if self.rng.chance(chance) {
                    self.apply(nx, ny, b_to);
                    return !self.apply(x, y, a_to);
                }
            }
        }
        // Probabilistic reactions may take many ticks to fire. While one is
        // still possible with any neighbour, keep this chunk awake.
        if NEIGHBOURS.iter().any(|&(dx, dy)| {
            let n = self.get(x + dx, y + dy);
            self.in_bounds(x + dx, y + dy) && n != id::EMPTY && reaction::lookup(mat, n).is_some()
        }) {
            self.wake(x, y);
        }
        true
    }

    /// Fire is rare, so it can afford to look at all four neighbours; that
    /// keeps ignition reliable even for short-lived flames. Flames touching
    /// fuel mostly cling to it instead of rising away.
    fn react_fire(&mut self, x: i32, y: i32) -> bool {
        let mut touching_fuel = false;
        for (dx, dy) in NEIGHBOURS {
            let (nx, ny) = (x + dx, y + dy);
            if !self.in_bounds(nx, ny) {
                continue;
            }
            let other = self.get(nx, ny);
            touching_fuel |= reaction::is_fuel(other);
            if let Some((a_to, b_to, chance)) = reaction::lookup(id::FIRE, other) {
                if self.rng.chance(chance) {
                    self.apply(nx, ny, b_to);
                    if self.apply(x, y, a_to) {
                        return false;
                    }
                }
            }
        }
        !(touching_fuel && self.rng.chance(0.95))
    }

    /// Apply a reaction outcome to a cell. Returns `true` if it changed.
    fn apply(&mut self, x: i32, y: i32, outcome: Outcome) -> bool {
        match outcome {
            Outcome::Keep => false,
            Outcome::Become(m) | Outcome::BecomeFresh(m) => {
                self.replace(x, y, m);
                true
            }
            Outcome::BecomeChance(m, p) => {
                let hit = self.rng.chance(p);
                if hit {
                    self.replace(x, y, m);
                }
                hit
            }
            Outcome::Ignite => {
                let (lo, hi) = if self.get(x, y) == id::OIL {
                    (90, 130)
                } else {
                    (160, 230)
                };
                self.ignite(x, y, lo, hi);
                true
            }
            Outcome::Explode => {
                self.replace(x, y, id::FIRE);
                self.pending.push(Explosion {
                    x,
                    y,
                    radius: GUNPOWDER_RADIUS,
                });
                true
            }
        }
    }

    /// Phase changes, then heat exchange with one random neighbour and air
    /// cooling. Returns `true` if the cell transformed.
    ///
    /// Phase changes are checked first so a cell created hot or cold (by a
    /// spell, say) reacts to that temperature before it starts to equalise.
    fn thermal(&mut self, x: i32, y: i32) -> bool {
        let i = self.idx(x, y);
        let mat = self.cells[i].mat;
        let p = props(mat);

        let t = self.cells[i].temp;
        if let Some((threshold, outcome)) = p.on_hot {
            if t >= threshold {
                return self.phase_change(x, y, outcome);
            }
        }
        if let Some((threshold, outcome)) = p.on_cold {
            if t <= threshold {
                return self.phase_change(x, y, outcome);
            }
        }

        if p.conduct > 0.0 {
            let (dx, dy) = NEIGHBOURS[(self.rng.next_u32() % 4) as usize];
            let (nx, ny) = (x + dx, y + dy);
            if self.in_bounds(nx, ny) {
                let j = self.idx(nx, ny);
                let other = self.cells[j].mat;
                let ta = self.cells[i].temp as i32;
                if other == id::EMPTY {
                    // Proportional cooling: hot things cool quickly, then settle.
                    let off = ta - AMBIENT as i32;
                    if off.abs() >= HEAT_DEADBAND && self.rng.chance(p.air_cool) {
                        let step = (off.abs() / 8).max(2) * off.signum();
                        self.cells[i].temp = (ta - step) as u8;
                        self.wake(x, y);
                    }
                } else {
                    let k = p.conduct.min(props(other).conduct);
                    let tb = self.cells[j].temp as i32;
                    let diff = tb - ta;
                    if k > 0.0 && diff.abs() >= HEAT_DEADBAND {
                        let mut d = (diff as f32 * k * 0.5).round() as i32;
                        if d == 0 {
                            d = diff.signum();
                        }
                        self.cells[i].temp = (ta + d).clamp(0, 255) as u8;
                        self.cells[j].temp = (tb - d).clamp(0, 255) as u8;
                        self.wake(x, y);
                        self.wake(nx, ny);
                    }
                }
            }
        }
        false
    }

    /// Like [`World::apply`], but a material change keeps the temperature,
    /// so freshly made glass still glows and melted ice is still cold.
    fn phase_change(&mut self, x: i32, y: i32, outcome: Outcome) -> bool {
        match outcome {
            Outcome::Become(m) => {
                let temp = self.temp(x, y);
                self.replace(x, y, m);
                let i = self.idx(x, y);
                self.cells[i].temp = temp;
                true
            }
            other => self.apply(x, y, other),
        }
    }

    /// Set fuel on fire. Fuel fire lives longer than [`EMBER_LIFE`], so it
    /// stays in place as embers before dying down into ordinary flames.
    fn ignite(&mut self, x: i32, y: i32, lo: u8, hi: u8) {
        self.replace(x, y, id::FIRE);
        let i = self.idx(x, y);
        self.cells[i].life = self.rng.range(lo as u32, hi as u32) as u8;
    }

    fn replace(&mut self, x: i32, y: i32, mat: u8) {
        let i = self.idx(x, y);
        let clock = self.cells[i].clock;
        self.cells[i] = Cell::new(mat, &mut self.rng);
        self.cells[i].clock = clock;
        self.wake(x, y);
    }

    /// Swap `(x, y)` into `(nx, ny)` if allowed. Both cells are stamped.
    fn try_move(&mut self, x: i32, y: i32, nx: i32, ny: i32, stamp: u8) -> bool {
        if !self.in_bounds(nx, ny) {
            return false;
        }
        let (a, b) = (self.idx(x, y), self.idx(nx, ny));
        if !can_displace(self.cells[a].mat, self.cells[b].mat) {
            return false;
        }
        self.cells.swap(a, b);
        self.cells[a].clock = stamp;
        self.cells[b].clock = stamp;
        self.wake(x, y);
        self.wake(nx, ny);
        true
    }

    fn move_powder(&mut self, x: i32, y: i32, stamp: u8) {
        if self.try_move(x, y, x, y + 1, stamp) {
            return;
        }
        let d = self.rng.dir();
        if !self.try_move(x, y, x + d, y + 1, stamp) {
            self.try_move(x, y, x - d, y + 1, stamp);
        }
    }

    fn move_liquid(&mut self, x: i32, y: i32, spread: i32, stamp: u8) {
        if self.try_move(x, y, x, y + 1, stamp) {
            return;
        }
        let d = self.rng.dir();
        if self.try_move(x, y, x + d, y + 1, stamp) || self.try_move(x, y, x - d, y + 1, stamp) {
            return;
        }
        // Flow sideways as far as the path is clear.
        let mat = self.get(x, y);
        for dir in [d, -d] {
            let mut target = 0;
            for step in 1..=spread {
                let nx = x + dir * step;
                if !self.in_bounds(nx, y) || !can_displace(mat, self.get(nx, y)) {
                    break;
                }
                target = step;
                // Stop at a ledge so liquid drips down instead of skating over it.
                if can_displace(mat, self.get(nx, y + 1)) {
                    break;
                }
            }
            if target > 0 {
                self.try_move(x, y, x + dir * target, y, stamp);
                return;
            }
        }
    }

    fn move_gas(&mut self, x: i32, y: i32, stamp: u8) {
        let d = self.rng.dir();
        // Drift sideways sometimes so plumes billow instead of forming columns.
        if self.rng.chance(0.3) && self.try_move(x, y, x + d, y, stamp) {
            return;
        }
        if self.try_move(x, y, x, y - 1, stamp)
            || self.try_move(x, y, x + d, y - 1, stamp)
            || self.try_move(x, y, x - d, y - 1, stamp)
        {
            return;
        }
        self.try_move(x, y, x + d, y, stamp);
    }
}
