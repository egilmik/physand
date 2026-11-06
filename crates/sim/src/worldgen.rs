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

/// An axis-aligned rectangle of cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }

    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }

    pub fn grow(&self, by: i32) -> Rect {
        Rect {
            x: self.x - by,
            y: self.y - by,
            w: self.w + 2 * by,
            h: self.h + 2 * by,
        }
    }
}

/// A generated level plus gameplay markers.
pub struct Level {
    pub world: World,
    /// Spawn point: the cell the player's feet stand on top of (centre-bottom).
    pub spawn: (i32, i32),
    /// Reaching this region completes the floor.
    pub exit: Option<Rect>,
    /// The wax seal blocking the way to the exit.
    pub seal: Option<Rect>,
    /// Feet positions on open floor, suitable for walking enemies.
    pub ground_spots: Vec<(i32, i32)>,
    /// Open-air positions, suitable for flying enemies.
    pub air_spots: Vec<(i32, i32)>,
    /// Floor positions inside catacomb rooms (for tougher guardians).
    pub room_spots: Vec<(i32, i32)>,
}

impl Level {
    /// A level with just a world and a spawn point (tests, sandboxes).
    pub fn new(world: World, spawn: (i32, i32)) -> Level {
        Level {
            world,
            spawn,
            exit: None,
            seal: None,
            ground_spots: Vec::new(),
            air_spots: Vec::new(),
            room_spots: Vec::new(),
        }
    }
}

/// Block size for the coarse cave automaton; caves are smoothed at this
/// resolution and upsampled so walls are curvy rather than blocky.
const CAVE_BLOCK: i32 = 6;

/// Fill the world with cellular-automaton caves: `fill` is the initial rock
/// density; where `vein` noise exceeds `vein_threshold`, rock becomes sand.
fn cave_fill(world: &mut World, rng: &mut Rng, fill: f32, vein_threshold: f32) {
    let (w, h) = (world.width() as i32, world.height() as i32);
    let (bw, bh) = (w / CAVE_BLOCK + 2, h / CAVE_BLOCK + 2);
    let mut solid: Vec<bool> = (0..bw * bh)
        .map(|i| {
            let (bx, by) = (i % bw, i / bw);
            bx == 0 || by == 0 || bx == bw - 1 || by == bh - 1 || rng.chance(fill)
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

    // Upsample with bilinear smoothing and edge noise.
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
                let vein = value_noise(x, y, 22, vein_seed);
                let mat = if vein > vein_threshold {
                    id::SAND
                } else {
                    id::ROCK
                };
                world.set(x, y, mat);
            }
        }
    }
}

/// Flood every open cell from row `top` down with lava.
fn flood_lava(world: &mut World, top: i32) {
    let (w, h) = (world.width() as i32, world.height() as i32);
    for y in top..h {
        for x in 0..w {
            if world.get(x, y) == id::EMPTY {
                world.set(x, y, id::LAVA);
            }
        }
    }
}

/// Carve the spawn chamber at the top centre with a rock ledge. Returns the spawn point.
fn spawn_chamber(world: &mut World) -> (i32, i32) {
    let (sx, sy) = (world.width() as i32 / 2, 48);
    fill_ellipse(world, sx, sy - 14, 28, 16, id::EMPTY);
    for x in (sx - 16)..=(sx + 16) {
        for y in (sy + 1)..=(sy + 4) {
            world.set(x, y, id::ROCK);
        }
    }
    (sx, sy)
}

/// Settle loose material, then make sure the spawn ledge is clear.
fn settle(world: &mut World, spawn: (i32, i32), ticks: u32) {
    for _ in 0..ticks {
        world.step();
    }
    let (sx, sy) = spawn;
    fill_rect(
        world,
        Rect {
            x: sx - 6,
            y: sy - 14,
            w: 13,
            h: 15,
        },
        id::EMPTY,
    );
    world.wake_all();
}

fn fill_rect(world: &mut World, r: Rect, mat: u8) {
    for y in r.y..r.y + r.h {
        for x in r.x..r.x + r.w {
            world.set(x, y, mat);
        }
    }
}

fn fill_ellipse(world: &mut World, cx: i32, cy: i32, rx: i32, ry: i32, mat: u8) {
    for y in (cy - ry)..=(cy + ry) {
        for x in (cx - rx)..=(cx + rx) {
            let (dx, dy) = ((x - cx) as f32 / rx as f32, (y - cy) as f32 / ry as f32);
            if dx * dx + dy * dy <= 1.0 {
                world.set(x, y, mat);
            }
        }
    }
}

/// Random open cell in the given row band, if one is found quickly.
fn open_spot(world: &World, rng: &mut Rng, y_min: i32, y_max: i32) -> Option<(i32, i32)> {
    let w = world.width() as i32;
    (0..200).find_map(|_| {
        let x = rng.range(8, (w - 9) as u32) as i32;
        let y = rng.range(y_min as u32, y_max.max(y_min) as u32) as i32;
        (world.get(x, y) == id::EMPTY).then_some((x, y))
    })
}

/// Cellular-automaton caves with sand veins, water and oil pockets, wooden
/// scaffolding and a lava lake at the bottom. The spawn chamber is at the top.
pub fn cave_level(width: u32, height: u32, seed: u32) -> Level {
    let mut world = World::new(width, height, seed);
    let mut rng = Rng::new(seed ^ 0xC0FF_EE11);
    let (w, h) = (width as i32, height as i32);

    cave_fill(&mut world, &mut rng, 0.47, 0.72);
    let lava_top = h - h / 10;
    flood_lava(&mut world, lava_top);

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

    // Wooden scaffolding across open spaces.
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

    let spawn = spawn_chamber(&mut world);
    settle(&mut world, spawn, 120);
    Level::new(world, spawn)
}

// ---------------------------------------------------------------------------
// Biome 1: the Dune Catacombs
// ---------------------------------------------------------------------------

/// Thickness of the obsidian vault around the exit.
const VAULT_WALL: i32 = 5;

/// A floor of the Dune Catacombs: sandstone burial rooms strung along a
/// winding tunnel, sand dunes and hanging sand traps held up by wooden
/// beams, oil pools, and at the bottom a wax-sealed obsidian vault holding
/// the descent.
pub fn dune_catacombs(width: u32, height: u32, seed: u32) -> Level {
    let mut world = World::new(width, height, seed);
    let mut rng = Rng::new(seed ^ 0xD00E_CA7A);
    let (w, h) = (width as i32, height as i32);

    cave_fill(&mut world, &mut rng, 0.5, 0.68);
    let spawn = spawn_chamber(&mut world);

    // --- Route: spawn → rooms → vault, zig-zagging down the floor ----------
    let lava_top = h - 40;
    let seal_y = h - 150;
    let vault_x = rng.range(140, (w - 140) as u32) as i32;
    let n_rooms = 4;
    let mut waypoints = vec![(spawn.0, spawn.1 - 6)];
    for i in 0..n_rooms {
        let t = (i + 1) as f32 / (n_rooms + 1) as f32;
        let y = spawn.1 + ((seal_y - 30 - spawn.1) as f32 * t) as i32;
        let (lo, hi) = if i % 2 == 0 {
            (90, w / 2 - 40)
        } else {
            (w / 2 + 40, w - 90)
        };
        let x = rng.range(lo as u32, hi as u32) as i32;
        waypoints.push((x, y));
    }
    waypoints.push((vault_x, seal_y - 8));

    // --- Rooms: sandstone shells at the intermediate waypoints -------------
    let mut rooms = Vec::new();
    for &(x, y) in &waypoints[1..waypoints.len() - 1] {
        let (rw, rh) = (rng.range(60, 84) as i32, rng.range(28, 36) as i32);
        let room = Rect {
            x: x - rw / 2,
            y: y - rh / 2,
            w: rw,
            h: rh,
        };
        fill_rect(&mut world, room.grow(3), id::SANDSTONE);
        fill_rect(&mut world, room, id::EMPTY);
        rooms.push(room);
    }

    // --- Tunnels: a guaranteed path through everything ------------------------
    let meander_seed = rng.next_u32();
    for pair in waypoints.windows(2) {
        carve_tunnel(&mut world, pair[0], pair[1], meander_seed);
    }
    // Antechamber above the seal.
    fill_rect(
        &mut world,
        Rect {
            x: vault_x - 16,
            y: seal_y - 18,
            w: 33,
            h: 18,
        },
        id::EMPTY,
    );

    // --- Decorate the rooms ---------------------------------------------------
    let dune_seed = rng.next_u32();
    for room in &rooms {
        let floor = room.y + room.h - 1;
        // Dunes along the floor.
        for x in room.x..room.x + room.w {
            let height = 2 + (value_noise(x, room.y, 14, dune_seed) * 9.0) as i32;
            for y in (floor - height + 1)..=floor {
                world.set(x, y, id::SAND);
            }
        }
        // Some rooms keep an oil pool in a sunken basin.
        if rng.chance(0.4) {
            let bw = room.w / 3;
            let bx = room.x + rng.range(4, (room.w - bw - 4) as u32) as i32;
            fill_rect(
                &mut world,
                Rect {
                    x: bx,
                    y: floor - 6,
                    w: bw,
                    h: 10,
                },
                id::EMPTY,
            );
            fill_rect(
                &mut world,
                Rect {
                    x: bx,
                    y: floor - 1,
                    w: bw,
                    h: 5,
                },
                id::OIL,
            );
        }
        // Sand traps: a wooden false ceiling holding back a load of sand.
        if rng.chance(0.6) {
            let beam_y = room.y + 9;
            fill_rect(
                &mut world,
                Rect {
                    x: room.x,
                    y: room.y,
                    w: room.w,
                    h: 9,
                },
                id::SAND,
            );
            fill_rect(
                &mut world,
                Rect {
                    x: room.x,
                    y: beam_y,
                    w: room.w,
                    h: 2,
                },
                id::WOOD,
            );
        }
    }

    // Wooden props in the tunnels (fuel for fires, supports to burn away).
    for _ in 0..10 {
        let Some((x, y)) = open_spot(&world, &mut rng, spawn.1 + 30, seal_y - 30) else {
            continue;
        };
        let reach = |dir: i32| {
            (1..30)
                .take_while(|i| world.get(x + dir * i, y) == id::EMPTY)
                .count() as i32
        };
        let (l, r) = (reach(-1), reach(1));
        if l + r >= 10 && l < 29 && r < 29 {
            fill_rect(
                &mut world,
                Rect {
                    x: x - l,
                    y,
                    w: l + r + 1,
                    h: 2,
                },
                id::WOOD,
            );
        }
    }

    flood_lava(&mut world, lava_top);

    // --- The sealed vault ---------------------------------------------------
    let shaft = Rect {
        x: vault_x - 8,
        y: seal_y,
        w: 17,
        h: 30,
    };
    let chamber = Rect {
        x: vault_x - 20,
        y: seal_y + 30,
        w: 41,
        h: 30,
    };
    let outer = Rect {
        x: chamber.x - VAULT_WALL,
        y: seal_y,
        w: chamber.w + 2 * VAULT_WALL,
        h: chamber.y + chamber.h + VAULT_WALL - seal_y,
    };
    fill_rect(&mut world, outer, id::OBSIDIAN);
    fill_rect(&mut world, shaft, id::EMPTY);
    fill_rect(&mut world, chamber, id::EMPTY);
    let seal = Rect {
        x: shaft.x,
        y: seal_y,
        w: shaft.w,
        h: 6,
    };
    fill_rect(&mut world, seal, id::WAX);
    let exit = Rect {
        x: vault_x - 6,
        y: chamber.y + 10,
        w: 13,
        h: chamber.h - 10,
    };

    settle(&mut world, spawn, 120);

    // --- Enemy placement candidates -------------------------------------------
    let keep_out = |x: i32, y: i32| {
        let near_spawn = (x - spawn.0).pow(2) + (y - spawn.1).pow(2) < 110 * 110;
        near_spawn || outer.grow(10).contains(x, y) || y > lava_top - 12
    };
    let mut ground_spots: Vec<(i32, i32)> = Vec::new();
    let mut air_spots: Vec<(i32, i32)> = Vec::new();
    let far_from = |spots: &[(i32, i32)], x: i32, y: i32, d: i32| {
        spots
            .iter()
            .all(|&(sx, sy)| (sx - x).pow(2) + (sy - y).pow(2) >= d * d)
    };
    for _ in 0..6000 {
        let x = rng.range(10, (w - 11) as u32) as i32;
        let y = rng.range(20, (lava_top - 13) as u32) as i32;
        if keep_out(x, y) || world.get(x, y) != id::EMPTY {
            continue;
        }
        let clear = |x0: i32, y0: i32, x1: i32, y1: i32| {
            (y0..=y1).all(|cy| (x0..=x1).all(|cx| world.get(cx, cy) == id::EMPTY))
        };
        if clear(x - 5, y - 5, x + 5, y + 5) && far_from(&air_spots, x, y, 40) {
            air_spots.push((x, y));
        }
        // Drop straight down to the floor below this open cell.
        let mut fy = y;
        while fy < lava_top - 13 && world.get(x, fy + 1) == id::EMPTY {
            fy += 1;
        }
        let solid_below = matches!(
            crate::material::kind(world.get(x, fy + 1)),
            crate::Kind::Solid | crate::Kind::Powder
        );
        if solid_below
            && !keep_out(x, fy)
            && clear(x - 4, fy - 14, x + 4, fy)
            && far_from(&ground_spots, x, fy, 24)
        {
            ground_spots.push((x, fy));
        }
    }
    let room_spots = ground_spots
        .iter()
        .copied()
        .filter(|&(x, y)| rooms.iter().any(|r| r.grow(2).contains(x, y)))
        .collect();

    Level {
        world,
        spawn,
        exit: Some(exit),
        seal: Some(seal),
        ground_spots,
        air_spots,
        room_spots,
    }
}

/// Carve a meandering tunnel from `a` to `b`, wide enough to walk through.
fn carve_tunnel(world: &mut World, a: (i32, i32), b: (i32, i32), seed: u32) {
    let (dx, dy) = ((b.0 - a.0) as f32, (b.1 - a.1) as f32);
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let (nx, ny) = (-dy / len, dx / len);
    let steps = (len / 2.0) as i32;
    for i in 0..=steps {
        let t = i as f32 / steps.max(1) as f32;
        // Sideways wobble that fades out at both ends so tunnels meet their targets.
        let envelope = (t * std::f32::consts::PI).sin();
        let wobble = (value_noise((t * 400.0) as i32, a.1, 60, seed) - 0.5) * 60.0 * envelope;
        let x = a.0 as f32 + dx * t + nx * wobble;
        let y = a.1 as f32 + dy * t + ny * wobble;
        let r = 7 + (value_noise(x as i32, y as i32, 20, seed ^ 0x55) * 4.0) as i32;
        fill_ellipse(world, x as i32, y as i32, r, r, id::EMPTY);
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
