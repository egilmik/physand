use physand_sim::{id, worldgen::Level, World};

use crate::game::Game;
use crate::input::Input;
use crate::player::{Player, PLAYER_H};

/// A flat room: rock floor at `floor_y`, walls at the edges. Player spawns at x=40.
fn room(w: u32, h: u32, floor_y: i32) -> World {
    let mut world = World::new(w, h, 1);
    for x in 0..w as i32 {
        for y in floor_y..h as i32 {
            world.set(x, y, id::ROCK);
        }
    }
    world
}

fn game_in(world: World, spawn: (i32, i32)) -> Game {
    Game::from_level(Level { world, spawn }, 1)
}

fn hold(g: &mut Game, input: Input, ticks: u32) {
    for _ in 0..ticks {
        g.set_input(input);
        g.step();
    }
}

fn idle() -> Input {
    Input {
        aim_x: 0.0,
        aim_y: 0.0,
        ..Input::default()
    }
}

#[test]
fn player_falls_and_lands() {
    let world = room(200, 120, 100);
    let mut g = game_in(world, (40, 30));
    hold(&mut g, idle(), 120);
    assert!(g.player.grounded);
    assert_eq!(g.player.y + PLAYER_H - 1, 99, "feet rest on the floor");
}

#[test]
fn player_walks_and_is_stopped_by_walls() {
    let mut world = room(200, 120, 100);
    for y in 80..100 {
        world.set(80, y, id::ROCK); // tall wall
    }
    let mut g = game_in(world, (40, 99));
    let x0 = g.player.x;
    hold(
        &mut g,
        Input {
            right: true,
            ..idle()
        },
        30,
    );
    assert!(g.player.x > x0 + 15, "should walk right");
    hold(
        &mut g,
        Input {
            right: true,
            ..idle()
        },
        120,
    );
    assert_eq!(g.player.x + 5, 80, "stops flush against the wall");
}

#[test]
fn player_steps_up_small_ledges() {
    let mut world = room(200, 120, 100);
    for x in 60..200 {
        for y in 97..100 {
            world.set(x, y, id::ROCK); // 3-cell ledge
        }
    }
    let mut g = game_in(world, (40, 99));
    hold(
        &mut g,
        Input {
            right: true,
            ..idle()
        },
        60,
    );
    assert!(g.player.x > 70, "walked onto the ledge");
    assert_eq!(g.player.y + PLAYER_H - 1, 96);
}

#[test]
fn jump_and_levitate() {
    let world = room(200, 200, 180);
    let mut g = game_in(world, (40, 179));
    hold(&mut g, idle(), 5);
    let ground_y = g.player.y;

    // Tap jump.
    hold(
        &mut g,
        Input {
            jump: true,
            ..idle()
        },
        1,
    );
    let mut apex = g.player.y;
    for _ in 0..60 {
        hold(&mut g, idle(), 1);
        apex = apex.min(g.player.y);
    }
    let jump_height = ground_y - apex;
    assert!(
        (12..=30).contains(&jump_height),
        "jump height {jump_height}"
    );

    // Holding jump levitates higher than a plain jump, and drains stamina.
    hold(&mut g, idle(), 30);
    let mut apex = g.player.y;
    let mut min_stamina = f32::MAX;
    for _ in 0..120 {
        hold(
            &mut g,
            Input {
                jump: true,
                ..idle()
            },
            1,
        );
        apex = apex.min(g.player.y);
        min_stamina = min_stamina.min(g.player.stamina);
    }
    assert!(ground_y - apex > jump_height + 10);
    assert!(min_stamina < 1.0, "levitation drains stamina");
}

#[test]
fn player_floats_in_water() {
    let mut world = room(120, 200, 180);
    for y in 100..180 {
        for x in 0..120 {
            world.set(x, y, id::WATER);
        }
    }
    let mut g = game_in(world, (40, 90));
    hold(&mut g, idle(), 200);
    assert!(g.player.submerged > 0.0);
    // Swimming up brings the player back to the surface.
    hold(
        &mut g,
        Input {
            jump: true,
            ..idle()
        },
        120,
    );
    assert!(
        g.player.y + PLAYER_H < 110,
        "should reach the surface, y={}",
        g.player.y
    );
}

#[test]
fn lava_kills_and_player_respawns() {
    let mut world = room(120, 120, 100);
    for x in 0..120 {
        for y in 95..100 {
            world.set(x, y, id::LAVA);
        }
    }
    for x in 10..30 {
        world.set(x, 40, id::ROCK); // safe spawn ledge
    }
    let mut g = game_in(world, (20, 39));
    // Walk off the ledge into the lava.
    for _ in 0..400 {
        if g.deaths > 0 {
            break;
        }
        hold(
            &mut g,
            Input {
                right: true,
                ..idle()
            },
            1,
        );
    }
    assert_eq!(g.deaths, 1);
    // After the respawn timer the player is back on the ledge.
    hold(&mut g, idle(), 200);
    assert!(g.player.alive());
    assert!(g.player.hp > 0.0);
}

#[test]
fn water_spray_deposits_real_water() {
    let world = room(200, 120, 100);
    let mut g = game_in(world, (40, 99));
    hold(&mut g, idle(), 5);
    let cast = Input {
        cast: true,
        aim_x: 120.0,
        aim_y: 80.0,
        ..idle()
    };
    hold(&mut g, cast, 30);
    assert!(g.flask().volume < g.flask().capacity - 50.0);
    hold(&mut g, idle(), 200);
    assert_eq!(g.particle_count(), 0, "all particles landed");
    let water = g.world.count(id::WATER);
    assert!(water > 80, "water deposited: {water}");
}

#[test]
fn digging_removes_sand_but_not_obsidian() {
    let mut world = room(200, 120, 100);
    for x in 60..70 {
        for y in 80..100 {
            world.set(x, y, id::SAND);
        }
    }
    for x in 20..30 {
        for y in 90..100 {
            world.set(x, y, id::OBSIDIAN);
        }
    }
    let mut g = game_in(world, (45, 99));
    hold(&mut g, idle(), 5);
    let sand = g.world.count(id::SAND);
    hold(
        &mut g,
        Input {
            dig: true,
            aim_x: 62.0,
            aim_y: 93.0,
            ..idle()
        },
        60,
    );
    assert!(g.world.count(id::SAND) < sand - 20);

    let obsidian = g.world.count(id::OBSIDIAN);
    hold(
        &mut g,
        Input {
            dig: true,
            aim_x: 27.0,
            aim_y: 95.0,
            ..idle()
        },
        60,
    );
    assert_eq!(g.world.count(id::OBSIDIAN), obsidian);
}

#[test]
fn falling_sand_is_pushed_off_the_player() {
    let world = room(120, 120, 100);
    let mut g = game_in(world, (40, 99));
    hold(&mut g, idle(), 5);
    let (px, py) = (g.player.x, g.player.y);
    for x in px..px + 5 {
        g.world.set(x, py - 10, id::SAND);
    }
    hold(&mut g, idle(), 60);
    assert_eq!(
        g.world.count(id::SAND),
        5,
        "sand is displaced, not destroyed"
    );
    assert!(!Player::overlaps(&g.world, g.player.x, g.player.y));
}

// ---- M2: spell system -------------------------------------------------------

use crate::spell::{Catalyst, Flask, Form};

const NONE: u8 = 255;

/// Room with a rock floor at y=100 and a ceiling at y=40; player at x=40.
fn arena() -> Game {
    let mut world = room(260, 120, 100);
    for x in 0..260 {
        for y in 0..40 {
            world.set(x, y, id::ROCK);
        }
    }
    let mut g = game_in(world, (40, 99));
    hold(&mut g, idle(), 5);
    g
}

fn cast_at(g: &mut Game, x: f32, y: f32, ticks: u32) {
    hold(
        g,
        Input {
            cast: true,
            aim_x: x,
            aim_y: y,
            ..idle()
        },
        ticks,
    );
}

fn settle(g: &mut Game, ticks: u32) {
    hold(g, idle(), ticks);
}

#[test]
fn starter_flasks_have_generated_names_and_costs() {
    let g = arena();
    let names: Vec<String> = g.flasks.iter().map(Flask::name).collect();
    assert_eq!(
        names,
        [
            "Water Spray",
            "Searing Oil Orb",
            "Searing Sand Wall",
            "Acid Rain"
        ]
    );
    assert!((g.flasks[1].cost() - 39.0).abs() < 1e-3);
}

#[test]
fn flask_configuration_is_validated() {
    let mut g = arena();
    let orb = Form::ALL.iter().position(|&f| f == Form::Orb).unwrap() as u8;
    assert!(
        !g.configure_flask(0, &[NONE, NONE, NONE], orb, &[NONE, NONE]),
        "needs a reagent"
    );
    assert!(
        !g.configure_flask(0, &[id::ROCK, NONE, NONE], orb, &[NONE, NONE]),
        "rock is not a reagent"
    );
    assert!(
        !g.configure_flask(0, &[id::WATER, NONE, NONE], orb, &[0, 0]),
        "no duplicate catalysts"
    );
    assert!(
        !g.configure_flask(9, &[id::WATER, NONE, NONE], orb, &[NONE, NONE]),
        "no such flask"
    );
    assert!(g.configure_flask(0, &[id::WATER, id::OIL, NONE], orb, &[2, NONE]));
    assert_eq!(g.flasks[0].name(), "Volatile Water-Oil Orb");
}

#[test]
fn flask_selection_cycles() {
    let mut g = arena();
    g.cycle_flask(-1);
    assert_eq!(g.active_flask(), 3);
    g.cycle_flask(2);
    assert_eq!(g.active_flask(), 1);
    g.select_flask(2);
    assert_eq!(g.active_flask(), 2);
    g.select_flask(7);
    assert_eq!(g.active_flask(), 2);
}

#[test]
fn searing_sand_wall_becomes_glass() {
    let mut g = arena();
    g.select_flask(2);
    cast_at(&mut g, 70.0, 85.0, 1);
    settle(&mut g, 10);
    assert!(
        g.world.count(id::GLASS) >= 20,
        "glass: {}",
        g.world.count(id::GLASS)
    );
    assert_eq!(g.world.count(id::SAND), 0);
}

#[test]
fn searing_oil_orb_starts_a_fire() {
    let mut g = arena();
    g.select_flask(1);
    cast_at(&mut g, 120.0, 95.0, 1);
    let mut burned = false;
    for _ in 0..120 {
        settle(&mut g, 1);
        burned |= g.world.count(id::FIRE) > 10;
    }
    assert!(burned, "hot oil should ignite on landing");
}

#[test]
fn frozen_water_spray_makes_ice() {
    let mut g = arena();
    let spray = 0;
    assert!(g.configure_flask(0, &[id::WATER, NONE, NONE], spray, &[1, NONE]));
    cast_at(&mut g, 90.0, 98.0, 20);
    settle(&mut g, 60);
    assert!(
        g.world.count(id::ICE) > 30,
        "ice: {}",
        g.world.count(id::ICE)
    );
}

#[test]
fn frost_orb_turns_lava_to_obsidian() {
    let mut world = room(260, 120, 100);
    for x in 100..200 {
        for y in 90..100 {
            world.set(x, y, id::LAVA);
        }
    }
    let mut g = game_in(world, (40, 99));
    let orb = 1;
    assert!(g.configure_flask(0, &[id::ICE, NONE, NONE], orb, &[1, NONE]));
    cast_at(&mut g, 130.0, 80.0, 1);
    settle(&mut g, 80);
    assert!(
        g.world.count(id::OBSIDIAN) > 10,
        "obsidian: {}",
        g.world.count(id::OBSIDIAN)
    );
}

#[test]
fn volatile_orb_blasts_a_crater_and_hurts_up_close() {
    let mut g = arena();
    let orb = 1;
    assert!(g.configure_flask(0, &[id::WATER, NONE, NONE], orb, &[2, NONE]));
    let rock = g.world.count(id::ROCK);
    cast_at(&mut g, 150.0, 101.0, 1);
    settle(&mut g, 120);
    let carved = rock - g.world.count(id::ROCK);
    assert!(carved > 15, "crater carved: {carved}");
    assert_eq!(g.player.hp, 100.0, "a distant blast does not hurt");

    // Blasting your own feet hurts and knocks you back.
    settle(&mut g, 30);
    let x0 = g.player.x;
    cast_at(&mut g, 47.0, 101.0, 1);
    settle(&mut g, 10);
    assert!(g.player.hp < 100.0);
    assert_ne!(g.player.x, x0, "knockback moved the player");
}

#[test]
fn transmute_orb_converts_what_it_hits() {
    let mut g = arena();
    let orb = 1;
    assert!(g.configure_flask(0, &[id::SAND, NONE, NONE], orb, &[4, NONE]));
    let rock = g.world.count(id::ROCK);
    cast_at(&mut g, 120.0, 101.0, 1);
    settle(&mut g, 60);
    assert!(g.world.count(id::ROCK) < rock);
    assert!(g.world.count(id::SAND) > 10, "rock became sand");
}

#[test]
fn rain_falls_from_the_ceiling() {
    let mut g = arena();
    g.select_flask(3);
    cast_at(&mut g, 150.0, 95.0, 2);
    // Droplets start just under the ceiling (y=40), far above the aim point.
    hold(&mut g, idle(), 1);
    assert!(g.particle_count() > 0);
    settle(&mut g, 200);
    let acid_or_eaten = g.world.count(id::ACID) > 0 || g.world.count(id::SMOKE) > 0;
    assert!(acid_or_eaten, "acid reached the floor");
}

#[test]
fn multi_reagent_spray_mixes_materials() {
    let mut g = arena();
    assert!(g.configure_flask(0, &[id::WATER, id::SAND, NONE], 0, &[NONE, NONE]));
    cast_at(&mut g, 90.0, 95.0, 20);
    settle(&mut g, 100);
    assert!(g.world.count(id::WATER) > 10);
    assert!(g.world.count(id::SAND) > 10);
}

#[test]
fn heat_and_frost_cancel() {
    let f = Flask::new(
        &[id::WATER],
        Form::Spray,
        &[Catalyst::Heat, Catalyst::Frost],
    )
    .unwrap();
    assert_eq!(f.name(), "Searing Frozen Water Spray");
    let mut g = arena();
    assert!(g.configure_flask(0, &[id::WATER, NONE, NONE], 0, &[0, 1]));
    cast_at(&mut g, 90.0, 98.0, 20);
    settle(&mut g, 60);
    assert_eq!(g.world.count(id::ICE), 0);
    assert_eq!(g.world.count(id::STEAM), 0);
    assert!(g.world.count(id::WATER) > 30);
}
