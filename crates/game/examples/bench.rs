//! Rough timing of level generation and simulation: `cargo run --release -p physand-game --example bench`
use physand_game::{Game, Input};
use std::time::Instant;

fn main() {
    let t = Instant::now();
    let mut g = Game::new(12345);
    println!("level generation: {:?}", t.elapsed());
    println!(
        "awake chunks after gen: {}/{}",
        g.world.awake_chunks(),
        g.world.chunk_count()
    );

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
    let per = t.elapsed() / ticks;
    println!(
        "step+render: {per:?}/tick, awake chunks now: {}/{}",
        g.world.awake_chunks(),
        g.world.chunk_count()
    );
}
