//! Procedural scenes. M0 only has a sandbox demo; real floor generation
//! (caves, rooms, veins) comes in M3.

use crate::material::id;
use crate::rng::Rng;
use crate::world::World;

/// A small seeded sandbox: rolling rock terrain, a lake, a lava pocket,
/// a wooden bridge and some sand and oil to play with.
pub fn demo_scene(width: u32, height: u32, seed: u32) -> World {
    let mut world = World::new(width, height, seed);
    let mut rng = Rng::new(seed ^ 0xA5A5_5A5A);
    let (w, h) = (width as i32, height as i32);

    // Terrain height from a few random sine octaves.
    let phases: Vec<f32> = (0..3).map(|_| rng.f32() * std::f32::consts::TAU).collect();
    let ground: Vec<i32> = (0..w)
        .map(|x| {
            let t = x as f32 / w as f32 * std::f32::consts::TAU;
            let n = (t + phases[0]).sin() * 0.5
                + (t * 2.0 + phases[1]).sin() * 0.3
                + (t * 5.0 + phases[2]).sin() * 0.2;
            (h as f32 * (0.72 + 0.12 * n)) as i32
        })
        .collect();

    for x in 0..w {
        for y in ground[x as usize]..h {
            world.set(x, y, id::ROCK);
        }
    }

    // Lake: fill everything below the waterline in the middle third.
    let waterline = (h as f32 * 0.74) as i32;
    for x in (w / 3)..(2 * w / 3) {
        for y in waterline..ground[x as usize] {
            world.set(x, y, id::WATER);
        }
    }

    // Lava pocket carved into the right side, sealed by rock.
    let (lx, ly) = (w * 5 / 6, h - h / 10);
    for y in (ly - 8)..(ly + 4) {
        for x in (lx - 18)..(lx + 18) {
            if ((x - lx) as f32 / 18.0).powi(2) + ((y - ly) as f32 / 8.0).powi(2) <= 1.0 {
                world.set(x, y, id::LAVA);
            }
        }
    }

    // Wooden bridge over the lake.
    let bridge_y = waterline - 30;
    for x in (w / 3 - 10)..(2 * w / 3 + 10) {
        world.set(x, bridge_y, id::WOOD);
        world.set(x, bridge_y + 1, id::WOOD);
    }

    // Loose material dropped from above.
    world.paint(w / 6, h / 5, 14, id::SAND);
    world.paint(w / 2, bridge_y - 12, 8, id::OIL);
    world
}

/// A generated level plus where the player starts.
pub struct Level {
    pub world: World,
    /// Spawn point: the cell the player's feet stand on top of (centre-bottom).
    pub spawn: (i32, i32),
}

/// Block size for the coarse cave automaton; caves are smoothed at this
/// resolution and upsampled so walls are curvy rather than blocky.
const CAVE_BLOCK: i32 = 6;

/// Cellular-automaton caves with sand veins, water and oil pockets, wooden
/// scaffolding and a lava lake at the bottom. The spawn chamber is at the top.
pub fn cave_level(width: u32, height: u32, seed: u32) -> Level {
    let mut world = World::new(width, height, seed);
    let mut rng = Rng::new(seed ^ 0xC0FF_EE11);
    let (w, h) = (width as i32, height as i32);

    // --- Coarse cave field ---------------------------------------------------
    let (bw, bh) = (w / CAVE_BLOCK + 2, h / CAVE_BLOCK + 2);
    let mut solid: Vec<bool> = (0..bw * bh)
        .map(|i| {
            let (bx, by) = (i % bw, i / bw);
            bx == 0 || by == 0 || bx == bw - 1 || by == bh - 1 || rng.chance(0.47)
        })
        .collect();
    for _ in 0..5 {
        let prev = solid.clone();
        for by in 1..bh - 1 {
            for bx in 1..bw - 1 {
                let mut n = 0;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if (dx, dy) != (0, 0) && prev[((by + dy) * bw + bx + dx) as usize] {
                            n += 1;
                        }
                    }
                }
                let was = prev[(by * bw + bx) as usize];
                solid[(by * bw + bx) as usize] = n >= 5 || (was && n == 4);
            }
        }
    }

    // --- Upsample with bilinear smoothing and edge noise ----------------------
    let field = |bx: i32, by: i32| -> f32 {
        let (bx, by) = (bx.clamp(0, bw - 1), by.clamp(0, bh - 1));
        if solid[(by * bw + bx) as usize] {
            1.0
        } else {
            0.0
        }
    };
    let seed_noise = rng.next_u32();
    let vein_seed = rng.next_u32();
    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 / CAVE_BLOCK as f32 - 0.5;
            let fy = y as f32 / CAVE_BLOCK as f32 - 0.5;
            let (ix, iy) = (fx.floor() as i32, fy.floor() as i32);
            let (tx, ty) = (fx - ix as f32, fy - iy as f32);
            let top = field(ix, iy) * (1.0 - tx) + field(ix + 1, iy) * tx;
            let bot = field(ix, iy + 1) * (1.0 - tx) + field(ix + 1, iy + 1) * tx;
            let v = top * (1.0 - ty) + bot * ty;
            let jitter = (value_noise(x, y, 5, seed_noise) - 0.5) * 0.35;
            if v + jitter > 0.5 {
                // Diggable sand veins run through the rock.
                let vein = value_noise(x, y, 22, vein_seed);
                let mat = if vein > 0.72 { id::SAND } else { id::ROCK };
                world.set(x, y, mat);
            }
        }
    }

    // --- Lava lake: flood the open bottom rows ---------------------------------
    let lava_top = h - h / 10;
    for y in lava_top..h {
        for x in 0..w {
            if world.get(x, y) == id::EMPTY {
                world.set(x, y, id::LAVA);
            }
        }
    }

    // --- Pockets of liquid dropped into open caves ------------------------------
    let open_spot = |world: &World, rng: &mut Rng, y_min: i32, y_max: i32| -> Option<(i32, i32)> {
        (0..200).find_map(|_| {
            let x = rng.range(8, (w - 9) as u32) as i32;
            let y = rng.range(y_min as u32, y_max as u32) as i32;
            (world.get(x, y) == id::EMPTY).then_some((x, y))
        })
    };
    for _ in 0..(w * h / 40_000).max(3) {
        if let Some((x, y)) = open_spot(&world, &mut rng, h / 4, lava_top - 40) {
            let r = rng.range(8, 16) as i32;
            world.paint(x, y, r, id::WATER);
        }
    }
    for _ in 0..(w * h / 150_000).max(1) {
        if let Some((x, y)) = open_spot(&world, &mut rng, h / 4, lava_top - 40) {
            world.paint(x, y, 7, id::OIL);
        }
    }

    // --- Wooden scaffolding across open spaces -------------------------------
    for _ in 0..(w * h / 25_000).max(4) {
        let Some((x, y)) = open_spot(&world, &mut rng, 40, lava_top - 20) else {
            continue;
        };
        let reach = |dir: i32| {
            (1..40)
                .take_while(|i| world.get(x + dir * i, y) == id::EMPTY)
                .count() as i32
        };
        let (l, r) = (reach(-1), reach(1));
        // Only span gaps that are bounded on both sides (looks built, not floating).
        if l + r >= 12 && l < 39 && r < 39 {
            for bx in (x - l)..=(x + r) {
                world.set(bx, y, id::WOOD);
                world.set(bx, y + 1, id::WOOD);
            }
        }
    }

    // --- Spawn chamber at the top centre ------------------------------------------
    let (sx, sy) = (w / 2, 48);
    for y in (sy - 30)..=(sy + 2) {
        for x in (sx - 28)..=(sx + 28) {
            let (dx, dy) = ((x - sx) as f32 / 28.0, (y - sy) as f32 / 30.0);
            if dx * dx + dy * dy <= 1.0 {
                world.set(x, y, id::EMPTY);
            }
        }
    }
    for x in (sx - 16)..=(sx + 16) {
        for y in (sy + 1)..=(sy + 4) {
            world.set(x, y, id::ROCK);
        }
    }

    // Let loose material settle so the level starts calm.
    for _ in 0..200 {
        world.step();
    }
    // The spawn ledge must stay clear even if something fell onto it.
    for y in (sy - 14)..=sy {
        for x in (sx - 6)..=(sx + 6) {
            world.set(x, y, id::EMPTY);
        }
    }
    world.wake_all();

    Level {
        world,
        spawn: (sx, sy),
    }
}

/// Smooth 2D value noise in `[0, 1)` with feature size `scale` cells.
fn value_noise(x: i32, y: i32, scale: i32, seed: u32) -> f32 {
    let hash = |ix: i32, iy: i32| -> f32 {
        let mut n =
            (ix as u32).wrapping_mul(0x27D4_EB2D) ^ (iy as u32).wrapping_mul(0x1656_67B1) ^ seed;
        n ^= n >> 15;
        n = n.wrapping_mul(0x85EB_CA6B);
        n ^= n >> 13;
        (n & 0xFFFF) as f32 / 65536.0
    };
    let fx = x as f32 / scale as f32;
    let fy = y as f32 / scale as f32;
    let (ix, iy) = (fx.floor() as i32, fy.floor() as i32);
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let (tx, ty) = (smooth(fx - ix as f32), smooth(fy - iy as f32));
    let top = hash(ix, iy) * (1.0 - tx) + hash(ix + 1, iy) * tx;
    let bot = hash(ix, iy + 1) * (1.0 - tx) + hash(ix + 1, iy + 1) * tx;
    top * (1.0 - ty) + bot * ty
}
