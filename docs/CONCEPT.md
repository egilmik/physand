# Physand — Core Concept

## Context
The `physand` repo is empty (no commits). The goal is a web-based, physics-driven falling-sand game with a roguelite structure. Decisions made with the user:
- **Player role:** an alchemist character who descends through destructible sand levels and casts material-based spells.
- **Run length:** long runs, 45–90 minutes (deeper builds, multiple biomes).
- **Tech:** Rust → WebAssembly simulation + WebGL rendering.

This document defines the game concept and a staged build plan. The first implementation milestone is a playable vertical slice.

---

## 1. Pitch
**"Physand: The Sinking Crucible"** — You are an alchemist descending a shaft through a crumbling world where every pixel is simulated. You don't shoot bullets; you *brew reactions*. Mix materials into spells, reshape the terrain, flood enemies, melt floors, and try to reach the Crucible at the bottom. When you die, your knowledge persists, but the world is rebuilt.

**Core fantasy:** "I understand this world's chemistry better than it does."

## 2. Design Pillars
1. **Everything is material.** Terrain, enemies (partly), loot and spells all exist as cells in the simulation. No hit-scan, no invisible walls.
2. **Emergence over scripting.** Combos come from material rules (water + lava → obsidian + steam), not from hand-made special cases.
3. **Build-crafting through recipes.** Your run build is your set of *reagents* and *catalysts*, which together define what your spells do.
4. **Readable chaos.** Strong visual language (color/glow per material class) so you can predict outcomes.
5. **Fair permadeath.** Deaths should feel like *your* chemical mistake, never like randomness.

## 3. Core Loop
**Moment-to-moment (seconds):** move/jump/dig → aim → cast a spell from a flask → the simulation reacts → enemies and terrain respond.

**Floor loop (5–10 min):** enter floor → explore a procedurally generated cave → fight, collect reagents and gold → find the **descent point** (a crack you must open with material reactions, e.g. melting a seal) → reach the **Distillery** (safe room: shop, recipe altar, flask refill) → go down.

**Run loop (45–90 min):** 4 biomes × 3 floors + biome boss → final Crucible. Mid-run choices: which branch shaft to take (risk/reward), which recipes to keep.

**Meta loop (between runs):** spend **Essence** (earned on death, based on depth and discoveries) in the **Alchemist's Codex**:
- Unlock new materials into the random pool (adds options, not raw power).
- Record discovered reactions (the Codex fills up like a bestiary — a knowledge-based unlock).
- Unlock starting kits / alchemist classes.
- Small number of permanent upgrades (max HP, flask slot), kept deliberately modest.

## 4. The Spell System (the core differentiator)
Each **flask** has slots:
- **Reagent (1–3 slots):** the material emitted — water, sand, oil, acid, lava, ice, gunpowder, slime, spore, mercury…
- **Form (1 slot):** how it's delivered — *spray*, *orb* (bursts on impact), *beam*, *rain* (falls from ceiling above cursor), *wall*, *mine*.
- **Catalyst (0–2 slots):** modifiers — *heat* (+temperature), *frost*, *charge* (electrifies conductive materials), *volatile* (explodes on contact), *bloom* (multiplies volume), *homing*, *transmute* (changes the material it hits into the reagent).

Flasks have **volume** (mana equivalent) that refills slowly and at Distilleries. Multi-reagent flasks mix in the flask itself → can create unique outcomes (oil + heat catalyst = napalm orb).

Examples of emergent combos:
- Water spray + charge catalyst → electrified pool, chain damage.
- Acid rain on the ceiling above enemies → dissolves rock, drops boulders.
- Ice wall + lava floor → instant obsidian bridge.
- Transmute + gunpowder orb → turns an enemy's shell into explosive.

## 5. World & Simulation
**Cell grid**, each cell: material id, temperature, velocity/flags, lifetime, color variation seed.

Material classes:
- **Static solids:** rock, metal, wood (burns), obsidian, crystal.
- **Powders:** sand, gravel, ash, gunpowder, salt.
- **Liquids:** water, oil, acid, lava, slime, mercury, blood — density-based layering.
- **Gases:** steam, smoke, toxic gas, fire, methane.
- **Special:** spores (grow on organics), void (deletes), gold (loot), essence (pickup).

Systems:
- **Reactions table** (data-driven, `materials.ron`/JSON): `(A, B, condition) → (A', B', probability, energy)`.
- **Temperature:** heat diffusion, phase change (ice ⇄ water ⇄ steam, sand → glass, metal melts).
- **Rigid bodies (later milestone):** chunks of solid that detach when unsupported (cave-ins), simulated as rigid bodies, then re-rasterized.
- **Chunked world:** 64×64 cell chunks with dirty-rects; only active chunks update. Camera views ~480×270 cells, world floor size ~1536×2048 cells.

## 6. Biomes (run structure)
| # | Biome | Signature materials | Hazard | Boss |
|---|-------|--------------------|--------|------|
| 1 | Dune Catacombs | sand, wood, oil | cave-ins | The Sandwyrm (body made of sand cells) |
| 2 | Flooded Mines | water, metal, gunpowder | flooding, electric eels | Drowned Foreman |
| 3 | Fungal Hollows | spores, slime, methane | spreading growth, gas explosions | Mycelial Heart |
| 4 | Magma Foundry | lava, obsidian, metal | heat, melting floors | The Smelter |
| — | The Crucible | all | reactive arena | The First Alchemist |

Branch shafts between floors: e.g. "Toxic shortcut" (skip a floor, harsh hazard) vs "Treasure vein" (extra loot, extra elite).

## 7. Enemies
- **Material creatures:** made of cells, so they can be dissolved, frozen, melted (sand golems, slime blobs, magma crawlers).
- **Sprite creatures with material interactions:** bats, miners, eels — take damage from temperature, acid, impact by falling cells, drowning.
- **Elites** have a material affinity (e.g. "Fireproof", "Absorbs water and grows").
- Enemies also use the simulation (a miner throws gunpowder; a fungus spreads spores).

## 8. Player
- Platformer controls: move, jump, short levitation (stamina), dig (slow pick for soft materials).
- HP + status effects: wet, burning, oiled (flammable), frozen, poisoned, electrified — all derived from cells touching the player.
- 4 flask slots (expandable to 6), 1 passive **Relic** slot set (relics = roguelite passives, e.g. "Hydrophobic Robe: water slides off", "Philosopher's Shard: 5% of cells you destroy become gold").

## 9. Roguelite Systems Summary
- Procedural floors (seeded): cellular-automata caves + room templates + material veins + liquid pockets.
- Random rewards: reagents, catalysts, forms, relics, gold. Pick 1 of 3 at altars.
- Shops in Distilleries.
- Seeded runs, daily seed challenge (later).
- Death → Essence → Codex unlocks. Discovered reactions persist.

---

## 10. Technical Architecture
```
physand/
  Cargo.toml (workspace)
  crates/
    sim/        # pure Rust: grid, chunks, materials, reactions, temperature, worldgen (no web deps, unit-testable)
    game/       # player, enemies, spells, run state, rng; depends on sim
    web/        # wasm-bindgen glue: exports tick(), input, exposes pixel buffer ptr
  web/
    index.html, src/main.ts   # Vite + TS: loop, input, WebGL2 renderer, UI (HUD, menus)
    src/render/               # WebGL2: upload color buffer as texture, glow/bloom pass, lighting
  assets/materials.ron        # data-driven materials + reactions
```
- **Simulation:** fixed 60 Hz tick in WASM, Noita-style checkerboard chunk updates (single thread first; later SharedArrayBuffer + workers if needed — requires COOP/COEP headers).
- **Rendering:** WASM writes an RGBA buffer (or material+temp buffer) into linear memory; JS creates a `Uint8Array` view and uploads via `texSubImage2D` to WebGL2. Shader does palette lookup, emissive glow for lava/fire, simple 2D light.
- **Tooling:** `wasm-pack` (or `cargo build --target wasm32-unknown-unknown` + `wasm-bindgen`), Vite with `vite-plugin-wasm`. Persistence of meta progress in `localStorage`/IndexedDB.
- **Determinism:** seeded RNG (`rand_pcg`) for worldgen and sim to support daily seeds and reproducible bugs.

## 11. Milestones
1. **M0 – Sandbox tech core:** Rust sim with sand, water, rock, fire, steam; WASM build; WebGL rendering; mouse brush. Target 60 fps at 480×270 view.
2. **M1 – Alchemist controller:** character physics vs cell grid, digging, camera follow, one spell (water spray).
3. **M2 – Spell system:** flasks with reagent/form/catalyst slots, 8 materials, reaction table, temperature.
4. **M3 – One floor playable:** procedural Dune Catacombs floor, 3 enemy types, descent point, death/restart → **vertical slice**.
5. **M4 – Run structure:** Distillery, rewards (pick 1 of 3), relics, 3 floors + Sandwyrm boss.
6. **M5 – Meta progression:** Essence, Codex, unlocks, save data.
7. **M6 – Content:** biomes 2–4, rigid-body cave-ins, more enemies/relics, Crucible, audio, polish.

## 12. Verification (per milestone)
- `cargo test -p sim` — unit tests for material rules (sand falls, water levels, density layering, reactions conserve expectations, deterministic with seed).
- `wasm-pack build crates/web` + `npm run dev` in `web/`, open in browser; use Playwright (pre-installed Chromium) to load the page, run N frames, screenshot, and check fps counter / no console errors.
- M3 acceptance: a fresh player can finish a floor in 5–10 min, with at least 3 distinct emergent spell combos possible.

