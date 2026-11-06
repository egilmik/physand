use crate::material::{can_displace, id, kind, props, Kind};
use crate::rng::Rng;

const NEIGHBOURS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// Fire with more life than this is burning fuel (embers) and does not move.
pub const EMBER_LIFE: u8 = 70;

/// One simulated cell. Four bytes, laid out so the whole grid can be
/// uploaded to the GPU as an RGBA8 texture without conversion.
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
}

impl Cell {
    pub const EMPTY: Cell = Cell {
        mat: id::EMPTY,
        var: 0,
        life: 0,
        clock: 0,
    };

    pub fn new(mat: u8, rng: &mut Rng) -> Cell {
        let (lo, hi) = props(mat).life;
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
        }
    }
}

/// The falling-sand grid. `y = 0` is the top row.
pub struct World {
    width: i32,
    height: i32,
    cells: Vec<Cell>,
    rng: Rng,
    tick: u32,
}

impl World {
    pub fn new(width: u32, height: u32, seed: u32) -> World {
        assert!(width > 0 && height > 0);
        World {
            width: width as i32,
            height: height as i32,
            cells: vec![Cell::EMPTY; (width * height) as usize],
            rng: Rng::new(seed),
            tick: 0,
        }
    }

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
    fn in_bounds(&self, x: i32, y: i32) -> bool {
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

    pub fn set(&mut self, x: i32, y: i32, mat: u8) {
        if self.in_bounds(x, y) {
            let c = Cell::new(mat, &mut self.rng);
            let i = self.idx(x, y);
            self.cells[i] = c;
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
                } else if self.get(x, y) == id::EMPTY && (solid || self.rng.chance(0.55)) {
                    self.set(x, y, mat);
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::EMPTY);
    }

    /// Advance the simulation one step.
    pub fn step(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        let stamp = self.tick as u8;
        let left_to_right = self.tick & 1 == 0;

        // Bottom-to-top so falling material moves at most once per tick;
        // alternate horizontal direction to avoid directional bias.
        for y in (0..self.height).rev() {
            for i in 0..self.width {
                let x = if left_to_right { i } else { self.width - 1 - i };
                let idx = self.idx(x, y);
                let cell = self.cells[idx];
                if cell.mat == id::EMPTY || cell.clock == stamp {
                    continue;
                }
                self.cells[idx].clock = stamp;
                self.update_cell(x, y, stamp);
            }
        }
    }

    fn update_cell(&mut self, x: i32, y: i32, stamp: u8) {
        let mat = self.get(x, y);

        // Reactions and lifetimes may transform or remove the cell.
        if !self.react(x, y, mat) {
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

    /// Material interactions. Returns `false` if the cell no longer needs to move.
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
        }

        if mat == id::FIRE {
            return self.react_fire(x, y);
        }

        // Check one random neighbour per tick (cheap, and looks organic).
        let (dx, dy) = NEIGHBOURS[(self.rng.next_u32() % 4) as usize];
        let (nx, ny) = (x + dx, y + dy);
        if !self.in_bounds(nx, ny) {
            return true;
        }
        let other = self.get(nx, ny);

        match (mat, other) {
            (id::LAVA, id::WATER) => {
                self.replace(x, y, id::OBSIDIAN);
                self.replace(nx, ny, id::STEAM);
                return false;
            }
            (id::LAVA, id::WOOD | id::OIL) if self.rng.chance(0.1) => self.ignite(nx, ny, 160, 230),
            (id::LAVA, id::EMPTY) if dy == -1 && self.rng.chance(0.002) => {
                self.replace(nx, ny, id::FIRE)
            }
            _ => {}
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
            match self.get(nx, ny) {
                id::WOOD | id::OIL => {
                    touching_fuel = true;
                    if self.get(nx, ny) == id::WOOD && self.rng.chance(0.08) {
                        self.ignite(nx, ny, 160, 230);
                    } else if self.get(nx, ny) == id::OIL && self.rng.chance(0.3) {
                        self.ignite(nx, ny, 90, 130);
                    }
                }
                id::WATER => {
                    self.replace(x, y, id::STEAM);
                    if self.rng.chance(0.2) {
                        self.replace(nx, ny, id::STEAM);
                    }
                    return true;
                }
                _ => {}
            }
        }
        !(touching_fuel && self.rng.chance(0.95))
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
