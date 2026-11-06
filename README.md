# Physand

A physics-based falling-sand roguelite for the web: play an alchemist who
brews material reactions to descend through a fully simulated world.
Rust → WebAssembly simulation, WebGL2 rendering.

See [docs/CONCEPT.md](docs/CONCEPT.md) for the game concept and roadmap.

## Status: M4 — a complete run

A run is **Floor 1 → Distillery → Floor 2 → Distillery → Floor 3 →
Distillery → the Sandwyrm's lair**. Death is permanent: the run ends and you
start again on a fresh floor 1. Slay the Sandwyrm and take its rift to win.

**Floors.** Each floor of the **Dune Catacombs** is generated from a seed: a
winding route links sandstone burial rooms full of dunes, oil pools and *sand
traps* (sand held up by wooden beams — burn or dig them and it all comes
down), with a lava lake at the bottom. Somewhere below lies an obsidian vault
whose shaft is closed by an **ancient wax seal**. Nothing breaks wax — not
digging, acid, explosions or transmutation — but heat melts it into oil. Melt
a way through and drop into the rift. An arrow at the screen edge points to
the seal, then the rift. **Gold** dust lies on cave floors, is sealed in the
walls, and spills from slain creatures; walk over it (or dig it) to collect it.

**The Distillery** is a safe room between floors. Walk up to a station and
press `E`:
- **Three pedestals** — take one reward (a new reagent, form, catalyst or
  relic); the other two vanish.
- **Shop** — a healing draught (30g), bigger flasks (50g), and one more
  component or relic (60–90g).
- **Fountain** — refills every flask and restores a quarter of your health, once.
- **Workbench** — the only place you can rework flasks in Spellcraft.
- **The rift** — on to the next floor (or, after floor 3, the boss).

You start with Water/Oil/Sand, the Spray/Orb/Wall forms and the Heat catalyst;
everything else is found or bought.

**Relics** bend the rules for the rest of the run: Ember Ward (fire barely
burns), Glass Skin (acid barely hurts), Scarab Carapace (+25 max HP),
Levitation Charm (+60% levitation), Bottomless Flask (+50% flask refill),
Volatile Heart (explosions can't hurt you), Philosopher's Shard (digging
sometimes makes gold), Magnet Ring (gold flies to you).

**The Sandwyrm** swims through the dunes of its lair, a chain of packed-sand
segments that stalks beneath you and bursts out to lunge. It moves the sand
rather than deleting it, and grinds rock into sand. Water erodes its segments;
heat glazes them into brittle glass that takes triple damage and shatters
under explosions; every destroyed segment spills real sand or glass. Its head
is armoured until the body has been whittled down to six segments. Its hoard
spills out beneath the rift it opens.

### Creatures

| Creature | Behaviour | Material weaknesses |
|----------|-----------|---------------------|
| **Scarab** | Wanders, chases on sight, hops over obstacles | Burns — and a burning scarab lights the fuel it runs past; acid; frost freezes it |
| **Dust Wraith** | Drifts, then swoops at you through the air | Fire; drowns in any liquid; frozen wraiths drop like stones |
| **Sand Golem** | Slow, tough guardian of the catacomb rooms | Water erodes it, acid eats it (×2.5), enough heat bakes it into a glass statue; slain golems collapse into real sand |

Spells hit creatures directly; explosions hurt and knock back everything
(including you). Spray and Orb spells arc so they land where you click.

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
Rework flasks in the **Spellcraft** panel at a Distillery workbench, using only
the components you own.

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
| Sandstone | Catacomb masonry: diggable, eaten by acid |
| Wax | Floor seals: immune to everything except heat, which melts it into oil |
| Gold | Heavy dust that sinks through liquids (lost in lava); collected by touch |

**Controls:** `A`/`D` move · `W`/`Space` jump (hold to levitate or swim) ·
left click cast · right click dig · `1`–`4` or mouse wheel select flask ·
`E` use a Distillery station · `C` spellcraft (at a workbench) · `Enter`
continue after a floor / start over · `R` new run · `Tab` brush mode · `P` pause.
In brush mode: left paint, right erase, wheel or `[` `]` brush size, number keys pick material.
Add `?seed=123` to the URL to replay a run's first floor.

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
crates/game/  Gameplay: run flow (floors, Distillery, boss), inventory/relics/gold, player,
              enemies and the Sandwyrm (shared body physics), spells, camera,
              frame composition (unit-tested)
crates/web/   wasm-bindgen glue exposing the game to JS
web/          Vite + TypeScript front end: game loop, input, WebGL2 renderer, UI
scripts/      Build helpers
docs/         Design documents
```

Each frame Rust copies the visible 480×270 window of the grid (4 bytes per cell:
`[material, colour variation, lifetime, temperature]`) plus an RGBA overlay of
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
