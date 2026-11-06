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
