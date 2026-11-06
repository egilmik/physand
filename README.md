# Physand

A physics-based falling-sand roguelite for the web: play an alchemist who
brews material reactions to descend through a fully simulated world.
Rust → WebAssembly simulation, WebGL2 rendering.

See [docs/CONCEPT.md](docs/CONCEPT.md) for the game concept and roadmap.

## Status: M0 — sandbox tech core

A playable sandbox: paint materials into a 480×270 world and watch them interact.

| Material | Behaviour |
|----------|-----------|
| Sand | Falls and piles; sinks through liquids |
| Water | Flows and levels out; puts out fire (→ steam) |
| Oil | Floats on water; very flammable |
| Lava | Slow, viscous; ignites fuel; + water → obsidian + steam |
| Fire | Rises and flickers; clings to and spreads through fuel |
| Wood | Static fuel; burns as stationary embers |
| Steam / Smoke | Rise and dissipate; steam condenses back into water |
| Rock / Obsidian | Static solids |

**Controls:** left-drag paint · right-drag erase · mouse wheel or `[` `]` brush size ·
`1`–`9`, `0` select material · `Space` pause · `.` single step · `R` new scene.

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
crates/sim/   Simulation core: materials, cell grid, reactions, worldgen (pure Rust, unit-tested)
crates/web/   wasm-bindgen glue exposing the sim to JS
web/          Vite + TypeScript front end: game loop, input, WebGL2 renderer, UI
scripts/      Build helpers
docs/         Design documents
```

The cell grid is 4 bytes per cell (`[material, colour variation, lifetime, tick stamp]`)
and is uploaded straight from wasm memory to a GPU texture each frame; colouring
happens in the fragment shader.

## Tests

```bash
cargo test --workspace
```
