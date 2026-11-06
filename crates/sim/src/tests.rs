use crate::material::id;
use crate::world::World;
use crate::worldgen::demo_scene;

fn run(world: &mut World, ticks: u32) {
    for _ in 0..ticks {
        world.step();
    }
}

#[test]
fn sand_falls_to_the_floor() {
    let mut w = World::new(8, 20, 1);
    w.set(4, 0, id::SAND);
    run(&mut w, 40);
    assert_eq!(w.get(4, 19), id::SAND);
    assert_eq!(w.count(id::SAND), 1);
}

#[test]
fn sand_piles_into_a_slope() {
    let mut w = World::new(31, 20, 2);
    for _ in 0..60 {
        w.set(15, 0, id::SAND);
        run(&mut w, 2);
    }
    run(&mut w, 100);
    // A pile is wider than one column and peaks at the drop point.
    assert_eq!(w.get(15, 19), id::SAND);
    assert_eq!(w.get(10, 19), id::SAND);
    assert_eq!(w.get(20, 19), id::SAND);
    let top = |x: i32| (0..20).find(|&y| w.get(x, y) != id::EMPTY).unwrap_or(20);
    assert!(top(15) < top(10) && top(15) < top(20));
}

#[test]
fn water_levels_out() {
    let mut w = World::new(40, 10, 3);
    for y in 0..10 {
        w.set(0, y, id::WATER);
        w.set(1, y, id::WATER);
    }
    run(&mut w, 400);
    // 20 cells of water over a 40-wide floor: should be a thin flat layer.
    assert_eq!(w.count(id::WATER), 20);
    for y in 0..9 {
        for x in 0..40 {
            assert_ne!(w.get(x, y), id::WATER, "water still stacked at ({x},{y})");
        }
    }
}

#[test]
fn sand_sinks_through_water() {
    let mut w = World::new(3, 10, 4);
    for y in 5..10 {
        for x in 0..3 {
            w.set(x, y, id::WATER);
        }
    }
    w.set(1, 0, id::SAND);
    run(&mut w, 60);
    assert_eq!(w.get(1, 9), id::SAND);
    assert_eq!(w.count(id::WATER), 15);
}

#[test]
fn oil_floats_on_water() {
    let mut w = World::new(4, 10, 5);
    for x in 0..4 {
        w.set(x, 9, id::OIL);
        w.set(x, 0, id::WATER);
    }
    run(&mut w, 200);
    for x in 0..4 {
        assert_eq!(w.get(x, 9), id::WATER);
        assert_eq!(w.get(x, 8), id::OIL);
    }
}

#[test]
fn lava_and_water_make_obsidian() {
    let mut w = World::new(10, 10, 6);
    for x in 0..10 {
        w.set(x, 9, id::LAVA);
        w.set(x, 8, id::WATER);
    }
    run(&mut w, 200);
    assert!(w.count(id::OBSIDIAN) > 0);
    assert!(w.count(id::LAVA) < 10);
}

#[test]
fn fire_burns_wood_and_dies_out() {
    let mut w = World::new(20, 20, 7);
    for x in 0..20 {
        w.set(x, 19, id::WOOD);
    }
    for x in 8..12 {
        w.set(x, 18, id::FIRE);
    }
    run(&mut w, 3000);
    assert!(w.count(id::WOOD) < 10, "fire should spread along the plank");
    assert_eq!(w.count(id::FIRE), 0, "fire should eventually burn out");
}

#[test]
fn simulation_is_deterministic() {
    let mut a = demo_scene(160, 90, 42);
    let mut b = demo_scene(160, 90, 42);
    run(&mut a, 120);
    run(&mut b, 120);
    assert!(a.cells() == b.cells());

    let c = demo_scene(160, 90, 43);
    assert!(demo_scene(160, 90, 42).cells() != c.cells());
}

#[test]
fn paint_and_erase() {
    let mut w = World::new(20, 20, 8);
    w.paint(10, 10, 3, id::ROCK);
    assert_eq!(w.count(id::ROCK), 29); // filled disc of radius 3
    w.paint(10, 10, 5, id::EMPTY);
    assert_eq!(w.count(id::ROCK), 0);
}

#[test]
fn settled_world_goes_to_sleep() {
    let mut w = World::new(128, 128, 9);
    for x in 0..128 {
        w.set(x, 127, id::ROCK);
    }
    w.paint(64, 20, 6, id::SAND);
    run(&mut w, 400);
    assert_eq!(
        w.awake_chunks(),
        0,
        "nothing moves, so every chunk should sleep"
    );
    // Disturbing the world wakes only the affected area.
    w.set(64, 0, id::SAND);
    assert!(w.awake_chunks() > 0 && w.awake_chunks() < w.chunk_count());
    run(&mut w, 300);
    assert_eq!(w.awake_chunks(), 0);
}

#[test]
fn sleeping_chunks_wake_when_neighbour_changes() {
    // Sand resting on a block of sand that straddles a chunk border must fall
    // once the support below is removed.
    let mut w = World::new(64, 64, 10);
    for y in 20..40 {
        for x in 28..36 {
            w.set(x, y, id::ROCK);
        }
    }
    w.set(31, 19, id::SAND);
    run(&mut w, 100);
    assert_eq!(w.get(31, 19), id::SAND);
    assert_eq!(w.awake_chunks(), 0);
    for y in 20..40 {
        for x in 28..36 {
            w.set(x, y, id::EMPTY);
        }
    }
    run(&mut w, 100);
    assert_eq!(w.get(31, 63), id::SAND);
}

#[test]
fn cave_level_is_deterministic_with_clear_spawn() {
    let a = crate::worldgen::cave_level(384, 256, 77);
    let b = crate::worldgen::cave_level(384, 256, 77);
    assert!(a.world.cells() == b.world.cells());
    let (sx, sy) = a.spawn;
    assert_eq!(a.world.get(sx, sy + 1), id::ROCK, "spawn needs a floor");
    for y in (sy - 12)..=sy {
        for x in (sx - 3)..=(sx + 3) {
            assert_eq!(a.world.get(x, y), id::EMPTY, "spawn blocked at ({x},{y})");
        }
    }
    assert!(a.world.count(id::LAVA) > 0);
}

// ---- M2: temperature, reactions, explosions --------------------------------

/// A sealed rock box so nothing escapes and air cooling is minimal.
fn boxed(w: u32, h: u32, seed: u32) -> World {
    let mut world = World::new(w, h, seed);
    for x in 0..w as i32 {
        world.set(x, h as i32 - 1, id::ROCK);
    }
    world
}

#[test]
fn temperature_travels_with_moving_cells() {
    let mut w = World::new(4, 12, 11);
    w.set_with_temp(1, 0, id::SAND, 200);
    run(&mut w, 12);
    assert_eq!(w.get(1, 11), id::SAND);
    // It cools in the air on the way down, but carries most of its heat.
    assert!(w.temp(1, 11) > 100, "temp {}", w.temp(1, 11));
}

#[test]
fn cold_water_freezes_and_ice_chills_water() {
    // Water cast with frost (cold) turns into ice.
    let mut w = boxed(10, 10, 12);
    w.set_with_temp(5, 8, id::WATER, 5);
    run(&mut w, 5);
    assert_eq!(w.get(5, 8), id::ICE);

    // A big block of ice freezes the smaller amount of water around it.
    let mut w = boxed(20, 20, 13);
    for y in 10..19 {
        for x in 0..20 {
            w.set(
                x,
                y,
                if (4..16).contains(&x) {
                    id::ICE
                } else {
                    id::WATER
                },
            );
        }
    }
    let ice0 = w.count(id::ICE);
    run(&mut w, 300);
    assert!(
        w.count(id::ICE) > ice0,
        "ice should grow into the cold water"
    );
}

#[test]
fn hot_water_boils() {
    let mut w = boxed(10, 10, 14);
    w.set_with_temp(5, 8, id::WATER, 160);
    run(&mut w, 3);
    assert_eq!(w.count(id::WATER), 0);
    assert_eq!(w.count(id::STEAM), 1);
}

#[test]
fn sand_sinking_into_lava_turns_to_glass() {
    // Lava holds a finite amount of heat: a few grains dropped into a deep
    // pool vitrify, while the pool barely cools.
    let mut w = boxed(12, 16, 15);
    for x in 0..12 {
        for y in 6..15 {
            w.set(x, y, id::LAVA);
        }
    }
    for x in 4..8 {
        w.set(x, 0, id::SAND);
    }
    run(&mut w, 600);
    assert!(
        w.count(id::GLASS) >= 2,
        "sand sank into the lava and became glass"
    );
    assert!(w.count(id::LAVA) > 90, "the pool is mostly still molten");
}

#[test]
fn chilled_lava_becomes_obsidian() {
    let mut w = boxed(10, 10, 16);
    w.set_with_temp(5, 8, id::LAVA, 100);
    run(&mut w, 3);
    assert_eq!(w.get(5, 8), id::OBSIDIAN);
}

#[test]
fn acid_dissolves_rock_but_not_glass() {
    let mut w = World::new(20, 12, 17);
    for x in 0..20 {
        w.set(x, 11, if x < 10 { id::ROCK } else { id::GLASS });
    }
    // Keep acid on each half with a wall.
    for y in 0..11 {
        w.set(10, y, id::GLASS);
    }
    for x in 0..10 {
        w.set(x, 10, id::ACID);
    }
    for x in 11..20 {
        w.set(x, 10, id::ACID);
    }
    let glass0 = w.count(id::GLASS);
    run(&mut w, 1500);
    assert!(w.count(id::ROCK) < 10, "acid ate the rock floor");
    assert_eq!(w.count(id::GLASS), glass0, "glass is acid-proof");
}

#[test]
fn fire_sets_off_gunpowder_chain() {
    let mut w = boxed(60, 30, 18);
    for x in 5..55 {
        for y in 26..29 {
            w.set(x, y, id::GUNPOWDER);
        }
    }
    w.set(4, 27, id::FIRE);
    w.set(4, 28, id::FIRE);
    run(&mut w, 200);
    assert_eq!(w.count(id::GUNPOWDER), 0, "the whole trail should detonate");
    assert!(!w.take_explosions().is_empty());
}

#[test]
fn explosion_breaks_sand_rock_partially_and_spares_obsidian() {
    let mut w = World::new(40, 40, 19);
    for y in 0..40 {
        for x in 0..40 {
            let m = if x < 14 {
                id::SAND
            } else if x < 27 {
                id::ROCK
            } else {
                id::OBSIDIAN
            };
            w.set(x, y, m);
        }
    }
    let (s, r, o) = (w.count(id::SAND), w.count(id::ROCK), w.count(id::OBSIDIAN));
    w.explode(20, 20, 12);
    assert!(w.count(id::SAND) < s);
    assert!(w.count(id::ROCK) < r && w.count(id::ROCK) > r / 2);
    assert_eq!(w.count(id::OBSIDIAN), o);
    assert_eq!(w.take_explosions().len(), 1);
}

#[test]
fn heated_world_settles_and_sleeps() {
    // A pool of lava under rock (an insulator) must not keep chunks awake.
    let mut w = boxed(96, 96, 20);
    for x in 0..96 {
        for y in 80..95 {
            w.set(x, y, id::LAVA);
        }
        for y in 70..80 {
            w.set(x, y, id::ROCK);
        }
    }
    run(&mut w, 600);
    assert_eq!(w.awake_chunks(), 0);
}

#[test]
fn reaction_table_is_symmetric_on_lookup() {
    use crate::material::Outcome;
    use crate::reaction::lookup;
    let (a, b, _) = lookup(id::WATER, id::LAVA).unwrap();
    assert_eq!(a, Outcome::Become(id::STEAM));
    assert_eq!(b, Outcome::Become(id::OBSIDIAN));
    assert!(lookup(id::ROCK, id::SAND).is_none());
}
