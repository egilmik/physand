use physand_sim::{id, worldgen::Level, World};

use crate::game::{Game, Phase};
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
    Game::from_level(Level::new(world, spawn), 1)
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
fn lava_kills_and_ends_the_run() {
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
    hold(
        &mut g,
        Input {
            right: true,
            ..idle()
        },
        400,
    );
    assert_eq!(g.phase, Phase::Dead);
    assert_eq!(g.deaths, 1);
    assert!(!g.player.alive());
    // Permadeath: no respawn, and input no longer does anything.
    let ticks = g.run_ticks;
    hold(
        &mut g,
        Input {
            cast: true,
            aim_x: 60.0,
            aim_y: 30.0,
            ..idle()
        },
        60,
    );
    assert_eq!(g.phase, Phase::Dead);
    assert_eq!(g.particle_count(), 0);
    assert_eq!(g.run_ticks, ticks, "the run clock stops");
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
fn ballistic_aim_lands_where_you_click() {
    use crate::spell::ballistic_angle;
    // Simulate the integrator particles use and check where the arc lands.
    for &(dx, dy) in &[
        (80.0f32, 0.0f32),
        (-60.0, 20.0),
        (50.0, -30.0),
        (120.0, 10.0),
    ] {
        let (v, g) = (2.8, 0.035);
        let a = ballistic_angle(dx, dy, v, g);
        let (mut x, mut y, vx, mut vy) = (0.0f32, 0.0f32, a.cos() * v, a.sin() * v);
        let mut best = f32::MAX;
        for _ in 0..400 {
            vy += g;
            x += vx;
            y += vy;
            best = best.min((x - dx).hypot(y - dy));
        }
        assert!(best < 3.0, "({dx},{dy}) missed by {best}");
    }
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

// ---- M3: enemies, run flow, the seal -------------------------------------------

use crate::enemy::EnemyKind;

/// Flat room, player on the left at x=40.
fn den() -> Game {
    let mut g = game_in(room(300, 120, 100), (40, 99));
    hold(&mut g, idle(), 3);
    g
}

#[test]
fn scarab_chases_and_bites_with_invulnerability_frames() {
    let mut g = den();
    g.add_enemy(EnemyKind::Scarab, 110, 99);
    let mut first_hit = None;
    for t in 0..400 {
        hold(&mut g, idle(), 1);
        if g.player.hp < 100.0 && first_hit.is_none() {
            first_hit = Some(t);
            assert_eq!(g.player.hp, 92.0, "one bite");
        }
    }
    assert!(first_hit.is_some(), "the scarab should reach the player");
    // Invulnerability spaces the bites out: no more than one per 40 ticks.
    assert!(g.player.hp >= 100.0 - 8.0 * 11.0);
}

#[test]
fn acid_spray_kills_a_scarab() {
    let mut g = den();
    g.add_enemy(EnemyKind::Scarab, 90, 99);
    assert!(g.configure_flask(0, &[id::ACID, NONE, NONE], 0, &[NONE, NONE]));
    for _ in 0..90 {
        let aim = g.enemies.first().map_or((90.0, 97.0), |e| e.body.center());
        cast_at(&mut g, aim.0, aim.1, 1);
    }
    settle(&mut g, 30);
    assert!(g.enemies.is_empty(), "scarab dissolved");
    assert_eq!(g.kills, 1);
}

#[test]
fn searing_orb_sets_a_scarab_alight_and_it_lights_wood() {
    let mut world = room(300, 120, 100);
    for x in 60..200 {
        world.set(x, 100, id::WOOD); // wooden floor strip
    }
    let mut g = game_in(world, (40, 99));
    hold(&mut g, idle(), 3);
    g.add_enemy(EnemyKind::Scarab, 120, 99);
    g.select_flask(1); // Searing Oil Orb
    let mut burned = false;
    for t in 0..200 {
        // Keep lobbing orbs at the (moving) scarab until it catches fire.
        match g.enemies.first() {
            Some(e) if !burned && t % 25 == 0 => {
                let (ex, ey) = e.body.center();
                cast_at(&mut g, ex, ey, 1);
            }
            _ => settle(&mut g, 1),
        }
        burned |= g.enemies.first().is_some_and(|e| e.burning > 0);
    }
    assert!(burned, "the scarab caught fire");
    assert!(
        g.world.count(id::WOOD) < 140,
        "fire spread into the wooden floor"
    );
}

#[test]
fn frost_freezes_an_enemy_in_place() {
    let mut g = den();
    g.add_enemy(EnemyKind::Scarab, 120, 99);
    assert!(g.configure_flask(0, &[id::WATER, NONE, NONE], 0, &[1, NONE])); // frozen spray
    let mut froze = false;
    for _ in 0..40 {
        let (ex, ey) = g.enemies[0].body.center();
        cast_at(&mut g, ex, ey, 1);
        froze |= g.enemies[0].frozen > 0;
    }
    assert!(froze, "frost spray froze the scarab");
    let e = &g.enemies[0];
    let x0 = e.body.x;
    settle(&mut g, 20);
    assert_eq!(g.enemies[0].body.x, x0, "frozen enemies don't move");
}

#[test]
fn heat_bakes_a_golem_into_glass() {
    let mut g = den();
    g.add_enemy(EnemyKind::Golem, 130, 99);
    g.select_flask(1); // Searing Oil Orb
    let glass0 = g.world.count(id::GLASS);
    for _ in 0..8 {
        if let Some(e) = g.enemies.first() {
            let (ex, ey) = e.body.center();
            cast_at(&mut g, ex, ey, 1);
        }
        settle(&mut g, 30);
    }
    assert!(g.enemies.is_empty(), "the golem was baked");
    assert!(
        g.world.count(id::GLASS) > glass0 + 40,
        "it left a glass statue"
    );
}

#[test]
fn slain_golem_collapses_into_sand() {
    let mut g = den();
    g.add_enemy(EnemyKind::Golem, 130, 99);
    let sand0 = g.world.count(id::SAND);
    g.enemies[0].damage(1000.0);
    settle(&mut g, 1);
    assert!(g.enemies.is_empty());
    assert!(g.world.count(id::SAND) > sand0 + 60);
}

#[test]
fn wraith_drowns_in_water() {
    let mut world = room(200, 120, 100);
    for x in 0..200 {
        for y in 60..100 {
            world.set(x, y, id::WATER);
        }
    }
    for x in 10..30 {
        world.set(x, 30, id::ROCK);
    }
    let mut g = game_in(world, (20, 29));
    g.add_enemy(EnemyKind::Wraith, 150, 80);
    settle(&mut g, 300);
    assert!(g.enemies.is_empty(), "a submerged wraith drowns");
}

#[test]
fn explosions_hurt_enemies() {
    let mut g = den();
    g.add_enemy(EnemyKind::Golem, 150, 99);
    let hp = g.enemies[0].hp;
    g.world.explode(150, 95, 7);
    settle(&mut g, 1);
    assert!(g.enemies[0].hp < hp - 20.0);
}

#[test]
fn melting_the_seal_opens_it_and_reaching_the_exit_clears_the_floor() {
    use physand_sim::worldgen::{Level, Rect};
    let mut world = room(200, 140, 120);
    // A wax plug in a gap of the floor, with the exit below it.
    let seal = Rect {
        x: 100,
        y: 120,
        w: 12,
        h: 4,
    };
    for y in 120..140 {
        for x in 100..112 {
            world.set(x, y, if y < 124 { id::WAX } else { id::EMPTY });
        }
    }
    let mut level = Level::new(world, (60, 119));
    level.seal = Some(seal);
    level.exit = Some(Rect {
        x: 100,
        y: 128,
        w: 12,
        h: 12,
    });
    let mut g = Game::from_level(level, 3);
    settle(&mut g, 20);
    assert!(!g.seal_open());
    let (ox, oy) = g.objective().unwrap();
    assert!(
        (ox - 106.0).abs() < 1.0 && (oy - 122.0).abs() < 1.0,
        "compass points at the seal"
    );

    // Melt it with searing oil orbs, aiming at whatever wax is left.
    g.select_flask(1);
    for _ in 0..4 {
        let target = (100..112).find(|&x| (120..124).any(|y| g.world.get(x, y) == id::WAX));
        if let Some(x) = target {
            cast_at(&mut g, x as f32 + 1.5, 119.0, 1);
        }
        settle(&mut g, 60);
    }
    settle(&mut g, 60);
    assert!(g.seal_open(), "heat melted a way through");
    let (_, oy) = g.objective().unwrap();
    assert!(oy > 128.0, "compass now points at the exit");

    // Walk over and drop through.
    for _ in 0..600 {
        if g.phase == Phase::Cleared {
            break;
        }
        let px = g.player.center().0;
        let (right, left) = (px < 106.0, px > 107.0);
        hold(
            &mut g,
            Input {
                right,
                left,
                ..idle()
            },
            1,
        );
    }
    assert_eq!(g.phase, Phase::Cleared);
}

#[test]
fn a_real_floor_is_populated_and_descending_builds_the_next() {
    let mut g = Game::new(2024);
    assert_eq!(g.depth, 1);
    let n = g.enemies_alive();
    assert!(n >= 15, "enemies: {n}");
    let kinds = |g: &Game, k| g.enemies.iter().filter(|e| e.kind == k).count();
    assert!(
        kinds(&g, EnemyKind::Scarab) > 0
            && kinds(&g, EnemyKind::Wraith) > 0
            && kinds(&g, EnemyKind::Golem) > 0
    );
    assert!(g.objective().is_some());
    // No enemy starts next to the player.
    let (px, py) = g.player.center();
    assert!(g.enemies.iter().all(|e| {
        let (ex, ey) = e.body.center();
        (ex - px).hypot(ey - py) > 80.0
    }));
    settle(&mut g, 120);
    assert!(!g.descend(), "can't descend before clearing the floor");

    g.phase = Phase::Cleared;
    g.player.hp = 50.0;
    assert!(g.descend());
    assert_eq!(g.depth, 2);
    assert_eq!(g.phase, Phase::Playing);
    assert_eq!(g.player.hp, 80.0, "descending heals a little");
    assert!(g.enemies_alive() > n, "deeper floors are busier");
}
