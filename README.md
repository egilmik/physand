# Physand

A physics-based falling-sand roguelite for the web: play an alchemist who
brews material reactions to descend through a fully simulated world.
Rust → WebAssembly simulation, WebGL2 rendering.

See [docs/CONCEPT.md](docs/CONCEPT.md) for the game concept and roadmap.

## Status: M1 — the alchemist

Explore a procedurally generated 1024×768 cave as the alchemist. The camera
follows you through cellular-automaton caves with sand veins (which collapse
when you dig under them), water and oil pockets, wooden scaffolding and a lava
lake at the bottom.

- **Movement:** walk, step up small ledges and sand slopes, jump, hold jump to
  levitate (limited stamina, refills on the ground), swim in liquids.
- **Spell:** a *Water Spray* flask. Particles fly through the air and become real
  water cells when they land, so the simulation takes over: put out fires,
  cool lava into obsidian, flood tunnels.
- **Digging:** soft sand goes quickly, wood slower, rock slowly; obsidian can't be dug.
- **Danger:** fire and lava hurt; at 0 HP you respawn at the start.
- **Brush mode** (Tab) keeps the M0 sandbox tools for experimenting.

| Material | Behaviour |
|----------|-----------|
| Sand | Falls and piles; sinks through liquids; diggable |
| Water | Flows and levels out; puts out fire (→ steam) |
| Oil | Floats on water; very flammable |
| Lava | Slow, viscous; ignites fuel; + water → obsidian + steam |
| Fire | Rises and flickers; clings to and spreads through fuel |
| Wood | Static fuel; burns as stationary embers |
| Steam / Smoke | Rise and dissipate; steam condenses back into water |
| Rock / Obsidian | Static solids (rock is slowly diggable) |

**Controls:** `A`/`D` move · `W`/`Space` jump (hold to levitate or swim) ·
left click cast · right click dig · `Tab` toggle brush mode · `P` pause · `R` new level.
In brush mode: left paint, right erase, wheel or `[` `]` brush size, `1`–`9`/`0` material.

The simulation only updates 32×32 chunks where something changed recently, so
settled terrain is free; a typical tick costs ~2 ms in the browser.

## Running locally

Prerequisites: Rust (stable) with the wasm target, the matching `wasm-bindgen` CLI, and Node 18+.

```bash
rustup target add wasm32-unknown-unknown
```

```bash
cargo install wasm-bindgen-cli --version 0.2.100
```

```bash
cd web && npm install && npm run dev
```

`npm run dev` builds the wasm module (`scripts/build-wasm.sh`) and starts Vite.
After changing Rust code, run `npm run wasm` (or `npm run wasm:dev` for a faster
unoptimised build) and reload the page. `npm run build` produces a static site in `web/dist/`.

## Project layout

```
crates/sim/   Simulation core: materials, chunked cell grid, reactions, worldgen (pure Rust, unit-tested)
crates/game/  Gameplay: player controller, spells/particles, camera, frame composition (unit-tested)
crates/web/   wasm-bindgen glue exposing the game to JS
web/          Vite + TypeScript front end: game loop, input, WebGL2 renderer, UI
scripts/      Build helpers
docs/         Design documents
```

Each frame Rust copies the visible 480×270 window of the grid (4 bytes per cell:
`[material, colour variation, lifetime, tick stamp]`) plus an RGBA overlay of
sprites and particles into buffers that JS uploads straight from wasm memory to
two GPU textures; colouring and compositing happen in the fragment shader.

## Tests

```bash
cargo test --workspace
```

Rough native timing of level generation and a tick:

```bash
cargo run --release -p physand-game --example bench
```
