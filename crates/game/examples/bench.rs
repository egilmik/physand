//! Rough timing of level generation and simulation, plus how much of the
//! world stays active: `cargo run --release -p physand-game --example bench`
use physand_game::{Game, Input};
use physand_sim::id;
use std::time::Instant;

fn main() {
    let t = Instant::now();
    let mut g = Game::new(2481860229);
    println!("level generation: {:?}", t.elapsed());
    let report = |g: &Game, label: &str| {
        println!(
            "{label}: awake {}/{} · enemies {} · glass {} · obsidian {} · lava {}",
            g.world.awake_chunks(),
            g.world.chunk_count(),
            g.enemies_alive(),
            g.world.count(id::GLASS),
            g.world.count(id::OBSIDIAN),
            g.world.count(id::LAVA)
        );
    };
    report(&g, "after gen");

    let idle = Input::default();
    let t = Instant::now();
    for _ in 0..1800 {
        g.set_input(idle);
        g.step();
    }
    println!("idle step: {:?}/tick", t.elapsed() / 1800);
    report(&g, "after 30s idle");

    let input = Input {
        right: true,
        cast: true,
        aim_x: 600.0,
        aim_y: 40.0,
        ..Input::default()
    };
    let ticks = 600;
    let t = Instant::now();
    for _ in 0..ticks {
        g.set_input(input);
        g.step();
        g.render();
    }
    println!("casting step+render: {:?}/tick", t.elapsed() / ticks);
    report(&g, "after casting");

    // The boss arena: the wyrm keeps a lot of sand moving.
    let t = Instant::now();
    g.debug_skip_to_boss();
    println!("arena generation: {:?}", t.elapsed());
    let t = Instant::now();
    for _ in 0..ticks {
        g.set_input(idle);
        g.step();
        g.render();
    }
    println!(
        "boss fight step+render: {:?}/tick · awake {}/{} · boss hp {:?}",
        t.elapsed() / ticks,
        g.world.awake_chunks(),
        g.world.chunk_count(),
        g.boss_health()
    );
}
