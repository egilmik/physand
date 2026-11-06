//! Axis-aligned bodies that move cell by cell through the sand grid. Shared
//! by the player and enemies.

use physand_sim::{id, material::kind, Kind, World};

/// Cells creatures cannot pass through.
#[inline]
pub fn blocks(mat: u8) -> bool {
    matches!(kind(mat), Kind::Solid | Kind::Powder)
}

/// What a body is touching (its box plus a one-cell ring), in cells.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Contact {
    pub fire: u32,
    pub lava: u32,
    pub acid: u32,
    pub water: u32,
    /// Non-empty cells at or above "scalding" temperature (excluding fire/lava).
    pub hot: u32,
}

impl Contact {
    /// Damage per tick from what is touched, with per-material weights.
    pub fn damage(&self, fire: f32, lava: f32, acid: f32, hot: f32) -> f32 {
        self.fire as f32 * fire
            + self.lava as f32 * lava
            + self.acid as f32 * acid
            + self.hot as f32 * hot
    }
}

#[derive(Clone, Debug)]
pub struct Body {
    /// Top-left cell of the bounding box.
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// Sub-cell movement carried between ticks.
    rem_x: f32,
    rem_y: f32,
    pub vx: f32,
    pub vy: f32,
    pub grounded: bool,
    /// Fraction of the body inside liquid, `0..=1`.
    pub submerged: f32,
}

impl Body {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Body {
        Body {
            x,
            y,
            w,
            h,
            rem_x: 0.0,
            rem_y: 0.0,
            vx: 0.0,
            vy: 0.0,
            grounded: false,
            submerged: 0.0,
        }
    }

    /// A body of size `w × h` standing with its feet on row `feet_y`, centred on `cx`.
    pub fn standing(cx: i32, feet_y: i32, w: i32, h: i32) -> Body {
        Body::new(cx - w / 2, feet_y - h + 1, w, h)
    }

    pub fn center(&self) -> (f32, f32) {
        (
            self.x as f32 + self.w as f32 / 2.0,
            self.y as f32 + self.h as f32 / 2.0,
        )
    }

    /// Whether this body placed at `(x, y)` would overlap a blocking cell.
    pub fn overlaps_at(&self, world: &World, x: i32, y: i32) -> bool {
        (y..y + self.h).any(|cy| (x..x + self.w).any(|cx| blocks(world.get(cx, cy))))
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x as f32
            && py >= self.y as f32
            && px < (self.x + self.w) as f32
            && py < (self.y + self.h) as f32
    }

    pub fn intersects(&self, other: &Body) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }

    /// Recompute `grounded` and `submerged`.
    pub fn sense(&mut self, world: &World) {
        self.grounded = self.overlaps_at(world, self.x, self.y + 1);
        let mut liquid = 0;
        for cy in self.y..self.y + self.h {
            for cx in self.x..self.x + self.w {
                if kind(world.get(cx, cy)) == Kind::Liquid {
                    liquid += 1;
                }
            }
        }
        self.submerged = liquid as f32 / (self.w * self.h) as f32;
    }

    /// Move horizontally by `vx`, stepping up ledges up to `step_up` cells.
    /// Returns `true` if blocked.
    pub fn move_x(&mut self, world: &World, step_up: i32) -> bool {
        self.rem_x += self.vx;
        let steps = self.rem_x.round() as i32;
        self.rem_x -= steps as f32;
        let sign = steps.signum();
        for _ in 0..steps.abs() {
            if !self.overlaps_at(world, self.x + sign, self.y) {
                self.x += sign;
                continue;
            }
            if let Some(up) =
                (1..=step_up).find(|&up| !self.overlaps_at(world, self.x + sign, self.y - up))
            {
                self.x += sign;
                self.y -= up;
            } else {
                self.vx = 0.0;
                self.rem_x = 0.0;
                return true;
            }
        }
        false
    }

    /// Move vertically by `vy`. Returns `true` if blocked.
    pub fn move_y(&mut self, world: &World) -> bool {
        self.rem_y += self.vy;
        let steps = self.rem_y.round() as i32;
        self.rem_y -= steps as f32;
        let sign = steps.signum();
        for _ in 0..steps.abs() {
            if self.overlaps_at(world, self.x, self.y + sign) {
                self.vy = 0.0;
                self.rem_y = 0.0;
                return true;
            }
            self.y += sign;
        }
        false
    }

    /// The sim does not know about bodies, so sand can fall into them. Push
    /// loose grains out sideways, then climb out of anything solid.
    pub fn resolve_overlap(&mut self, world: &mut World) {
        for cy in self.y..self.y + self.h {
            for cx in self.x..self.x + self.w {
                let mat = world.get(cx, cy);
                if kind(mat) != Kind::Powder {
                    continue;
                }
                // Shove toward the nearer side of the body.
                let side = if cx - self.x < self.w / 2 { -1 } else { 1 };
                let edge = if side < 0 {
                    self.x - 1
                } else {
                    self.x + self.w
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
        if self.overlaps_at(world, self.x, self.y) {
            if let Some(up) = (1..=4).find(|&up| !self.overlaps_at(world, self.x, self.y - up)) {
                self.y -= up;
            }
        }
    }

    /// Count hazardous cells in and around the body.
    pub fn contact(&self, world: &World) -> Contact {
        let mut c = Contact::default();
        for cy in self.y - 1..=self.y + self.h {
            for cx in self.x - 1..=self.x + self.w {
                let Some(cell) = world.cell(cx, cy) else {
                    continue;
                };
                match cell.mat {
                    id::FIRE => c.fire += 1,
                    id::LAVA => c.lava += 1,
                    id::ACID => c.acid += 1,
                    id::WATER => c.water += 1,
                    id::EMPTY => {}
                    _ if cell.temp >= 180 => c.hot += 1,
                    _ => {}
                }
            }
        }
        c
    }
}
