//! Overlay drawing: sprites and particles are composited by the shader on
//! top of the cell texture, so they are drawn into a separate RGBA buffer.

use crate::player::{PLAYER_H, PLAYER_W};

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
        for (row, line) in SPRITE.iter().enumerate() {
            for (col, ch) in line.bytes().enumerate() {
                let Some(mut c) = palette(ch, flask) else {
                    continue;
                };
                if hurt {
                    c = [255, c[1] / 3, c[2] / 3, 255];
                }
                let col = col as i32;
                let sx = if facing >= 0 { col } else { PLAYER_W - 1 - col };
                self.put(x + sx, y + row as i32, c);
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
