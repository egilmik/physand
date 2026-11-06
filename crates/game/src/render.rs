//! Overlay drawing: sprites and particles are composited by the shader on
//! top of the cell texture, so they are drawn into a separate RGBA buffer.

use crate::player::PLAYER_H;

pub type Rgba = [u8; 4];

/// The alchemist, facing right. One char per cell (5 × 11).
const SPRITE: [&str; PLAYER_H as usize] = [
    "..H..", ".HHH.", "HHHHH", ".SSE.", ".SSS.", "RRRRF", "RRRRR", "BBBBR", "RRRRR", "RRRRR",
    ".D.D.",
];

fn palette(c: u8, flask: Rgba) -> Option<Rgba> {
    Some(match c {
        b'H' => [74, 52, 128, 255],
        b'S' => [228, 188, 150, 255],
        b'E' => [24, 20, 32, 255],
        b'R' => [112, 72, 176, 255],
        b'B' => [206, 170, 70, 255],
        b'D' => [52, 36, 32, 255],
        b'F' => flask,
        _ => return None,
    })
}

/// Status colouring applied to a whole sprite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tint {
    Hurt,
    Burning,
    Frozen,
    /// Golem heating toward glass; 0..=255 intensity.
    Hot(u8),
}

impl Tint {
    fn apply(self, c: Rgba) -> Rgba {
        let mix = |a: u8, b: u8, t: f32| (a as f32 + (b as f32 - a as f32) * t) as u8;
        match self {
            Tint::Hurt => [255, 255, 255, c[3]],
            Tint::Burning => [
                mix(c[0], 255, 0.6),
                mix(c[1], 120, 0.6),
                mix(c[2], 20, 0.6),
                c[3],
            ],
            Tint::Frozen => [
                mix(c[0], 170, 0.6),
                mix(c[1], 215, 0.6),
                mix(c[2], 255, 0.6),
                c[3],
            ],
            Tint::Hot(k) => {
                let t = k as f32 / 255.0 * 0.8;
                [mix(c[0], 255, t), mix(c[1], 110, t), mix(c[2], 30, t), c[3]]
            }
        }
    }
}

pub const SCARAB: [&str; 4] = [".GGG.", "GSSSE", "SSSSS", "L.L.L"];
pub const SCARAB_STEP: [&str; 4] = [".GGG.", "GSSSE", "SSSSS", ".L.L."];
pub const WRAITH_UP: [&str; 5] = ["W.....W", "WW.B.WW", ".WBEBW.", "..BBB..", "...B..."];
pub const WRAITH_DOWN: [&str; 5] = ["..WBW..", ".WBEBW.", "WWBBBWW", "W.....W", "...B..."];
pub const GOLEM: [&str; 14] = [
    "..SSSSS..",
    ".SSSSSSS.",
    ".SEddSES.",
    ".SSSSSSS.",
    "..SdSdS..",
    "SSSSSSSSS",
    "SSdSSSdSS",
    "SSSSSSSSS",
    "SS.SSS.SS",
    "SS.SdS.SS",
    "...SSS...",
    "..SS.SS..",
    "..SS.SS..",
    ".SSS.SSS.",
];

/// Palette for scarab and wraith sprites.
pub fn creature_palette(c: u8) -> Option<Rgba> {
    Some(match c {
        b'G' => [206, 170, 66, 255],
        b'S' => [40, 92, 104, 255],
        b'L' => [28, 28, 32, 255],
        b'W' => [120, 108, 96, 210],
        b'B' => [168, 156, 136, 255],
        b'E' => [255, 90, 50, 255],
        _ => return None,
    })
}

/// Golem palette (sandy body, glowing eyes).
pub fn golem_palette(c: u8) -> Option<Rgba> {
    Some(match c {
        b'S' => [204, 168, 104, 255],
        b'd' => [150, 118, 70, 255],
        b'E' => [255, 150, 40, 255],
        _ => return None,
    })
}

pub struct Overlay<'a> {
    pub buf: &'a mut [Rgba],
    pub w: i32,
    pub h: i32,
}

impl Overlay<'_> {
    pub fn clear(&mut self) {
        self.buf.fill([0, 0, 0, 0]);
    }

    #[inline]
    pub fn put(&mut self, x: i32, y: i32, c: Rgba) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.buf[(y * self.w + x) as usize] = c;
        }
    }

    /// Draw the player with its top-left at view cell `(x, y)`.
    pub fn player(&mut self, x: i32, y: i32, facing: i32, hurt: bool, flask: Rgba) {
        let tint = hurt.then_some(Tint::Hurt);
        self.sprite(x, y, &SPRITE, |c| palette(c, flask), facing, tint);
    }

    /// Draw a character-art sprite (one char per cell, `.` = transparent),
    /// mirrored when `facing < 0`.
    pub fn sprite(
        &mut self,
        x: i32,
        y: i32,
        rows: &[&str],
        colors: impl Fn(u8) -> Option<Rgba>,
        facing: i32,
        tint: Option<Tint>,
    ) {
        let w = rows.first().map_or(0, |r| r.len()) as i32;
        for (row, line) in rows.iter().enumerate() {
            for (col, ch) in line.bytes().enumerate() {
                let Some(c) = colors(ch) else { continue };
                let col = col as i32;
                let sx = if facing >= 0 { col } else { w - 1 - col };
                self.put(x + sx, y + row as i32, tint.map_or(c, |t| t.apply(c)));
            }
        }
    }

    /// Swirling ring marking the exit.
    pub fn portal(&mut self, cx: i32, cy: i32, rx: i32, ry: i32, tick: u32) {
        let spin = tick as f32 * 0.05;
        for i in 0..48 {
            let a = i as f32 / 48.0 * std::f32::consts::TAU + spin;
            let pulse = 0.85 + 0.15 * (tick as f32 * 0.1 + i as f32).sin();
            let (px, py) = (
                cx + (a.cos() * rx as f32 * pulse).round() as i32,
                cy + (a.sin() * ry as f32 * pulse).round() as i32,
            );
            let c = if i % 3 == 0 {
                [255, 230, 255, 255]
            } else {
                [190, 110, 255, 230]
            };
            self.put(px, py, c);
        }
        for i in 0..12 {
            let a = -(i as f32) / 12.0 * std::f32::consts::TAU - spin * 1.7;
            self.put(
                cx + (a.cos() * rx as f32 * 0.45) as i32,
                cy + (a.sin() * ry as f32 * 0.45) as i32,
                [150, 80, 230, 200],
            );
        }
    }

    /// Arrow at the view edge pointing toward an off-screen target.
    pub fn compass(&mut self, tx: f32, ty: f32, c: Rgba) {
        let (cx, cy) = (self.w as f32 / 2.0, self.h as f32 / 2.0);
        let margin = 10.0;
        if tx >= margin
            && ty >= margin
            && tx < self.w as f32 - margin
            && ty < self.h as f32 - margin
        {
            return;
        }
        let (dx, dy) = (tx - cx, ty - cy);
        let len = (dx * dx + dy * dy).sqrt().max(0.001);
        let (ux, uy) = (dx / len, dy / len);
        // Scale the direction until it touches the inset view rectangle.
        let kx = if ux.abs() > 1e-4 {
            (cx - margin) / ux.abs()
        } else {
            f32::MAX
        };
        let ky = if uy.abs() > 1e-4 {
            (cy - margin) / uy.abs()
        } else {
            f32::MAX
        };
        let k = kx.min(ky);
        let (ax, ay) = (cx + ux * k, cy + uy * k);
        // A small filled triangle: tip ahead, base behind.
        for t in 0..7 {
            let half = (6 - t) as f32 * 0.6;
            for s in -(half as i32)..=(half as i32) {
                let px = ax + ux * (t as f32 - 3.0) - uy * s as f32;
                let py = ay + uy * (t as f32 - 3.0) + ux * s as f32;
                self.put(px.round() as i32, py.round() as i32, c);
            }
        }
    }

    /// Small crosshair at the aim point.
    pub fn reticle(&mut self, x: i32, y: i32, c: Rgba) {
        for d in 2..=3 {
            self.put(x + d, y, c);
            self.put(x - d, y, c);
            self.put(x, y + d, c);
            self.put(x, y - d, c);
        }
    }

    /// Faint circle outline (dig radius indicator).
    pub fn ring(&mut self, cx: i32, cy: i32, r: i32, c: Rgba) {
        let steps = (r * 8).max(8);
        for i in 0..steps {
            let a = i as f32 / steps as f32 * std::f32::consts::TAU;
            self.put(
                cx + (a.cos() * r as f32).round() as i32,
                cy + (a.sin() * r as f32).round() as i32,
                c,
            );
        }
    }
}
