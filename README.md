# Physand

A physics-based falling-sand roguelite for the web: play an alchemist who
brews material reactions to descend through a fully simulated world.
Rust → WebAssembly simulation, WebGL2 rendering.

See [docs/CONCEPT.md](docs/CONCEPT.md) for the game concept and roadmap.

## Status: M2 — the spell system

Explore a procedurally generated 1024×768 cave as the alchemist, crafting
spells from materials. Caves have sand veins (which collapse when you dig
under them), water and oil pockets, wooden scaffolding and a lava lake.

### Spellcraft

Each of your 4 flasks combines:

| Slot | Options |
|------|---------|
| **Reagents** (1–3) | Water, Sand, Oil, Acid, Lava, Ice, Gunpowder, Wood |
| **Form** (1) | **Spray** (stream) · **Orb** (bursts into a blob) · **Rain** (falls from the ceiling above the cursor) · **Wall** (instant barrier) |
| **Catalysts** (0–2) | **Heat** (scalding material, heats targets) · **Frost** (frozen material, chills targets) · **Volatile** (explodes on impact) · **Bloom** (more material) · **Transmute** (turns what it hits into the reagent) |

Spells only place material, change temperature or trigger explosions; the
simulation does the rest. Some combos to try: *Searing Sand Wall* (glass
barrier), *Searing Oil Orb* (napalm), *Frozen Water Wall* (ice bridge), *Frost
Orb* on lava (obsidian), *Volatile Water Orb* (digging bomb), *Searing
Gunpowder Orb* (big bomb), *Transmuting Sand Orb* (turn rock into sand),
*Acid Rain* (eat through ceilings). Names are generated from the recipe.
Open the **Spellcraft** panel (`C`) to edit flasks. In later milestones
reagents and catalysts become loot.

### Chemistry

Every cell has a temperature. Heat flows between conductive materials and
cools in open air; rock and obsidian insulate. Materials change phase at
thresholds (water ⇄ ice/steam, sand → glass, chilled lava → obsidian, hot
oil/wood ignite, hot gunpowder detonates). Pairwise reactions are a data
table (`crates/sim/src/reaction.rs`): lava + water → obsidian + steam, acid
eats rock/sand/wood (not glass or obsidian), fire spreads through fuel, and
so on. Explosions break diggable material, chain through gunpowder, and hurt
and knock back the player. Hot cells glow and frozen cells frost over.

| Material | Behaviour |
|----------|-----------|
| Sand | Falls and piles; sinks through liquids; melts into glass |
| Water | Flows; freezes below 20, boils at 100; puts out fire |
| Oil | Floats on water; ignites when hot or touched by fire |
| Lava | Viscous, very hot; ignites fuel; + water → obsidian; chilled → obsidian |
| Acid | Dissolves rock, sand, wood, ice, gunpowder; harmless to glass/obsidian; hurts |
| Ice | Melts when warmed; chills neighbouring water |
| Gunpowder | Powder that detonates in chains when hot or burning |
| Glass | See-through, acid-proof barrier |
| Fire | Rises and flickers; clings to and spreads through fuel |
| Wood | Static fuel; burns as stationary embers |
| Steam / Smoke | Rise and dissipate; steam condenses back into water |
| Rock / Obsidian | Static insulating solids (rock is slowly diggable, obsidian immune to spells) |

**Controls:** `A`/`D` move · `W`/`Space` jump (hold to levitate or swim) ·
left click cast · right click dig · `1`–`4` or mouse wheel select flask ·
`C` spellcraft · `Tab` brush mode · `P` pause · `R` new level.
In brush mode: left paint, right erase, wheel or `[` `]` brush size, number keys pick material.
Add `?seed=123` to the URL to replay a level.

The simulation only updates 32×32 chunks where something changed recently (or
where a reaction is still possible), so settled terrain is free; a typical tick
costs 2–5 ms in the browser.

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
