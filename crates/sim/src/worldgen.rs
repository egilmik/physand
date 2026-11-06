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
