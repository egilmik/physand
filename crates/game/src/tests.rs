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
    assert!(g.flask.volume < g.flask.capacity - 50.0);
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
