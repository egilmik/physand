import init, {
  Physand,
  catalyst_count,
  catalyst_name,
  form_count,
  form_name,
  material_color,
  material_count,
  material_name,
  reagent_ids,
} from "./wasm/physand.js";
import { Renderer, type MaterialIds } from "./renderer";

const TICK_HZ = 60;
const TICK_MS = 1000 / TICK_HZ;
/** Cap catch-up work after a stall (e.g. a background tab) to avoid a spiral. */
const MAX_TICKS_PER_FRAME = 4;

// Must match `Input::*` bit flags in crates/game/src/input.rs.
const INPUT = { LEFT: 1, RIGHT: 2, JUMP: 4, CAST: 8, DIG: 16 } as const;

type Mode = "alchemist" | "brush";

const $ = <T extends HTMLElement>(sel: string) => document.querySelector(sel) as T;

async function main() {
  const wasm = await init();

  // Material table comes from Rust so there is a single source of truth.
  const count = material_count();
  const names: string[] = [];
  const palette = new Float32Array(count * 3);
  for (let i = 0; i < count; i++) {
    names.push(material_name(i));
    const c = material_color(i);
    palette[i * 3] = ((c >> 16) & 255) / 255;
    palette[i * 3 + 1] = ((c >> 8) & 255) / 255;
    palette[i * 3 + 2] = (c & 255) / 255;
  }
  const idOf = (name: string) => {
    const i = names.indexOf(name);
    if (i < 0) throw new Error(`Unknown material ${name}`);
    return i;
  };
  const ids: MaterialIds = {
    fire: idOf("Fire"),
    lava: idOf("Lava"),
    water: idOf("Water"),
    steam: idOf("Steam"),
    smoke: idOf("Smoke"),
    acid: idOf("Acid"),
    ice: idOf("Ice"),
    glass: idOf("Glass"),
  };
  const cssColor = (mat: number) =>
    `rgb(${Array.from(palette.slice(mat * 3, mat * 3 + 3), (v) => Math.round(v * 255)).join(",")})`;

  const newSeed = () => (Math.random() * 2 ** 32) >>> 0;
  // `?seed=123` replays a specific level.
  const seedParam = Number(new URLSearchParams(location.search).get("seed"));
  let seed = Number.isInteger(seedParam) && seedParam > 0 ? seedParam >>> 0 : newSeed();
  const game = new Physand(seed);
  const VIEW_W = game.view_width();
  const VIEW_H = game.view_height();

  const canvas = $<HTMLCanvasElement>("#view");
  const screen = $<HTMLDivElement>("#screen");
  const toolbar = $<HTMLElement>("#toolbar");
  const renderer = new Renderer(canvas, VIEW_W, VIEW_H, palette, ids);

  // ---- Layout -------------------------------------------------------------
  const stage = $<HTMLDivElement>("#stage");
  const fit = () => renderer.resize(stage.clientWidth - 16, stage.clientHeight - 16);
  new ResizeObserver(fit).observe(stage);
  fit();

  // ---- State --------------------------------------------------------------
  const state = {
    mode: "alchemist" as Mode,
    material: idOf("Sand"),
    brush: 4,
    paused: false,
  };
  const keys = new Set<string>();
  const mouse = { x: VIEW_W / 2, y: VIEW_H / 2, left: false, right: false };

  // ---- Mode toggle ----------------------------------------------------------
  const modeButtons = document.querySelectorAll<HTMLButtonElement>("#modes button");
  const setMode = (mode: Mode) => {
    state.mode = mode;
    modeButtons.forEach((b) => b.setAttribute("aria-checked", String(b.dataset.mode === mode)));
    screen.classList.toggle("alchemist", mode === "alchemist");
    toolbar.classList.toggle("alchemist", mode === "alchemist");
  };
  modeButtons.forEach((b) => b.addEventListener("click", () => setMode(b.dataset.mode as Mode)));
  setMode(state.mode);

  // ---- Brush toolbar --------------------------------------------------------
  const paintable = names.map((_, i) => i).filter((i) => i !== 0);
  const materialsEl = $<HTMLDivElement>("#materials");
  const buttons = new Map<number, HTMLButtonElement>();
  paintable.forEach((mat, n) => {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.setAttribute("role", "radio");
    const key = n < 9 ? String(n + 1) : n === 9 ? "0" : "";
    btn.innerHTML = `<span class="swatch" style="background:${cssColor(mat)}"></span>${names[mat]}${key ? ` <kbd>${key}</kbd>` : ""}`;
    btn.addEventListener("click", () => {
      selectMaterial(mat);
      setMode("brush");
    });
    materialsEl.append(btn);
    buttons.set(mat, btn);
  });
  const selectMaterial = (mat: number) => {
    state.material = mat;
    buttons.forEach((b, m) => b.setAttribute("aria-checked", String(m === mat)));
  };
  selectMaterial(state.material);

  const brushInput = $<HTMLInputElement>("#brush");
  const brushLabel = $<HTMLSpanElement>("#brush-size");
  const setBrush = (r: number) => {
    state.brush = Math.max(1, Math.min(24, Math.round(r)));
    brushInput.value = String(state.brush);
    brushLabel.textContent = String(state.brush);
  };
  brushInput.addEventListener("input", () => setBrush(Number(brushInput.value)));

  const pauseBtn = $<HTMLButtonElement>("#pause");
  const togglePause = () => {
    state.paused = !state.paused;
    pauseBtn.textContent = state.paused ? "Play" : "Pause";
  };
  pauseBtn.addEventListener("click", togglePause);
  const newLevel = () => {
    seed = newSeed();
    game.reset(seed);
    renderRecipes();
  };
  $<HTMLButtonElement>("#reset").addEventListener("click", newLevel);

  // ---- Keyboard -------------------------------------------------------------
  const GAME_KEYS = new Set(["KeyA", "KeyD", "KeyW", "ArrowLeft", "ArrowRight", "ArrowUp", "Space"]);
  window.addEventListener("keydown", (e) => {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
    if (GAME_KEYS.has(e.code)) {
      e.preventDefault();
      keys.add(e.code);
      return;
    }
    if (e.repeat) return;
    if (e.code === "Tab") {
      e.preventDefault();
      setMode(state.mode === "alchemist" ? "brush" : "alchemist");
    } else if (e.code === "KeyP") {
      togglePause();
    } else if (e.code === "KeyR") {
      newLevel();
    } else if (e.code === "Enter" && game.phase() === 2) {
      e.preventDefault();
      advance();
    } else if (e.code === "KeyC") {
      toggleSpellcraft();
    } else if (e.code === "Escape" && !spellcraft.hidden) {
      toggleSpellcraft(false);
    } else if (/^Digit[0-9]$/.test(e.code)) {
      const d = Number(e.code.slice(5));
      if (state.mode === "alchemist") {
        if (d >= 1 && d <= game.flask_count()) game.select_flask(d - 1);
        return;
      }
      const mat = paintable[d === 0 ? 9 : d - 1];
      if (mat !== undefined) selectMaterial(mat);
    } else if (e.key === "[") {
      setBrush(state.brush - 1);
    } else if (e.key === "]") {
      setBrush(state.brush + 1);
    }
  });
  window.addEventListener("keyup", (e) => keys.delete(e.code));
  window.addEventListener("blur", () => {
    keys.clear();
    mouse.left = mouse.right = false;
  });

  // ---- Mouse ----------------------------------------------------------------
  const toView = (e: PointerEvent | MouseEvent) => {
    const rect = canvas.getBoundingClientRect();
    mouse.x = ((e.clientX - rect.left) / rect.width) * VIEW_W;
    mouse.y = ((e.clientY - rect.top) / rect.height) * VIEW_H;
  };
  canvas.addEventListener("contextmenu", (e) => e.preventDefault());
  canvas.addEventListener("pointerdown", (e) => {
    canvas.setPointerCapture(e.pointerId);
    toView(e);
    if (e.button === 0) mouse.left = true;
    if (e.button === 2) mouse.right = true;
    lastPaint = null;
  });
  canvas.addEventListener("pointermove", toView);
  canvas.addEventListener("pointerup", (e) => {
    if (e.button === 0) mouse.left = false;
    if (e.button === 2) mouse.right = false;
  });
  canvas.addEventListener("pointercancel", () => (mouse.left = mouse.right = false));
  canvas.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      if (state.mode === "alchemist") game.cycle_flask(e.deltaY < 0 ? -1 : 1);
      else setBrush(state.brush + (e.deltaY < 0 ? 1 : -1));
    },
    { passive: false },
  );

  // Brush strokes are interpolated so fast drags leave a continuous line.
  let lastPaint: [number, number] | null = null;
  const applyBrush = () => {
    if (state.mode !== "brush" || !(mouse.left || mouse.right)) {
      lastPaint = null;
      return;
    }
    const mat = mouse.right ? 0 : state.material;
    const p: [number, number] = [Math.floor(mouse.x), Math.floor(mouse.y)];
    const from = lastPaint ?? p;
    const steps = Math.max(1, Math.ceil(Math.hypot(p[0] - from[0], p[1] - from[1]) / Math.max(1, state.brush / 2)));
    for (let s = 1; s <= steps; s++) {
      const t = s / steps;
      game.paint(
        Math.round(from[0] + (p[0] - from[0]) * t),
        Math.round(from[1] + (p[1] - from[1]) * t),
        state.brush,
        mat,
      );
    }
    lastPaint = p;
  };

  const inputBits = () => {
    let bits = 0;
    if (keys.has("KeyA") || keys.has("ArrowLeft")) bits |= INPUT.LEFT;
    if (keys.has("KeyD") || keys.has("ArrowRight")) bits |= INPUT.RIGHT;
    if (keys.has("KeyW") || keys.has("ArrowUp") || keys.has("Space")) bits |= INPUT.JUMP;
    if (state.mode === "alchemist") {
      if (mouse.left) bits |= INPUT.CAST;
      if (mouse.right) bits |= INPUT.DIG;
    }
    return bits;
  };

  // ---- HUD ------------------------------------------------------------------
  const hpFill = $<HTMLSpanElement>("#hp-bar .fill");
  const staminaFill = $<HTMLSpanElement>("#stamina-bar .fill");
  const banner = $<HTMLDivElement>("#banner");
  const bannerTitle = $<HTMLHeadingElement>("#banner-title");
  const bannerStats = $<HTMLParagraphElement>("#banner-stats");
  const bannerAction = $<HTMLButtonElement>("#banner-action");
  const runStats = $<HTMLDivElement>("#run-stats");
  const objective = $<HTMLDivElement>("#objective");
  const clock = (secs: number) => {
    const s = Math.floor(secs);
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  };
  let shownPhase = -1;
  bannerAction.addEventListener("click", () => advance());
  const flaskName = $<HTMLDivElement>("#flask-name");
  const hexColor = (c: number) => `#${c.toString(16).padStart(6, "0")}`;

  // Flask hotbar: one tile per flask, its fill level shows the volume.
  const flasksEl = $<HTMLDivElement>("#flasks");
  const flaskTiles: { el: HTMLDivElement; level: HTMLSpanElement }[] = [];
  for (let i = 0; i < game.flask_count(); i++) {
    const el = document.createElement("div");
    el.className = "flask";
    el.setAttribute("role", "radio");
    el.innerHTML = `<span class="level"></span><span class="key">${i + 1}</span>`;
    flasksEl.append(el);
    flaskTiles.push({ el, level: el.querySelector(".level") as HTMLSpanElement });
  }

  const setFill = (el: HTMLElement, v: number, max: number) => {
    el.style.transform = `scaleX(${Math.max(0, Math.min(1, v / max))})`;
  };
  const updateHud = () => {
    setFill(hpFill, game.hp(), game.max_hp());
    setFill(staminaFill, game.stamina(), game.max_stamina());
    const active = game.active_flask();
    flaskTiles.forEach(({ el, level }, i) => {
      const vol = game.flask_volume(i);
      el.classList.toggle("active", i === active);
      el.classList.toggle("empty", vol < game.flask_cost(i));
      el.setAttribute("aria-checked", String(i === active));
      el.style.setProperty("--flask", hexColor(game.flask_color(i)));
      level.style.transform = `scaleY(${Math.max(0, Math.min(1, vol / game.flask_capacity(i)))})`;
    });
    const name = game.flask_name(active);
    if (flaskName.textContent !== name) flaskName.textContent = name;

    setText(
      runStats,
      `Floor ${game.depth()} · ${game.kills()} slain · ${clock(game.run_seconds())}`,
    );
    setText(
      objective,
      game.seal_open()
        ? "The seal is breached — enter the rift below"
        : "Find the wax seal at the bottom (follow the arrow). Heat melts wax.",
    );

    // Run over / floor cleared screens.
    const phase = game.phase();
    if (phase !== shownPhase) {
      shownPhase = phase;
      banner.hidden = phase === 0;
      banner.classList.toggle("dead", phase === 1);
      if (phase === 1) {
        bannerTitle.textContent = "The catacombs claim you";
        bannerStats.innerHTML =
          `Reached floor ${game.depth()} · ${game.kills()} creatures slain · ${clock(game.run_seconds())}`;
        bannerAction.textContent = "New run (R)";
      } else if (phase === 2) {
        bannerTitle.textContent = `Floor ${game.depth()} cleared`;
        bannerStats.innerHTML =
          `Floor time ${clock(game.floor_seconds())} · ${game.kills()} slain so far` +
          `<br>Descending restores some health and refills your flasks.`;
        bannerAction.textContent = "Descend (Enter)";
      }
      if (phase !== 0) bannerAction.focus({ preventScroll: true });
    }
  };
  const setText = (el: HTMLElement, text: string) => {
    if (el.textContent !== text) el.textContent = text;
  };
  /** Banner action: descend after a clear, or start over after death. */
  function advance() {
    const phase = game.phase();
    if (phase === 2) game.descend();
    else if (phase === 1) newLevel();
    renderRecipes();
  }

  // ---- Spellcraft ------------------------------------------------------------
  const spellcraft = $<HTMLElement>("#spellcraft");
  const recipesEl = $<HTMLDivElement>("#recipes");
  const reagentIds = Array.from(reagent_ids());
  const forms = Array.from({ length: form_count() }, (_, i) => form_name(i));
  const catalysts = Array.from({ length: catalyst_count() }, (_, i) => catalyst_name(i));
  const NONE = 255;

  const select = (label: string, options: [number, string][], value: number, onChange: (v: number) => void) => {
    const wrap = document.createElement("label");
    wrap.textContent = label;
    const sel = document.createElement("select");
    for (const [v, text] of options) sel.add(new Option(text, String(v), false, v === value));
    sel.addEventListener("change", () => onChange(Number(sel.value)));
    wrap.append(sel);
    return wrap;
  };

  function renderRecipes() {
    recipesEl.replaceChildren();
    for (let i = 0; i < game.flask_count(); i++) {
      const card = document.createElement("div");
      card.className = "recipe";
      card.classList.toggle("active", i === game.active_flask());
      const recipe = {
        r: [0, 1, 2].map((s) => game.flask_reagent(i, s)),
        form: game.flask_form(i),
        c: [0, 1].map((s) => game.flask_catalyst(i, s)),
      };
      const title = document.createElement("h3");
      title.innerHTML = `<span>${i + 1}. ${game.flask_name(i)}</span><span class="cost">${game.flask_cost(i).toFixed(1)} / cast</span>`;
      card.append(title);

      const apply = () => {
        const ok = game.configure_flask(i, recipe.r[0], recipe.r[1], recipe.r[2], recipe.form, recipe.c[0], recipe.c[1]);
        if (ok) {
          renderRecipes();
        } else {
          card.classList.add("invalid");
          title.firstElementChild!.textContent = `${i + 1}. Invalid recipe`;
        }
      };

      const slots = document.createElement("div");
      slots.className = "slots";
      const reagentOpts: [number, string][] = reagentIds.map((m) => [m, names[m]]);
      recipe.r.forEach((v, s) => {
        const opts: [number, string][] = s === 0 ? reagentOpts : [[NONE, "—"], ...reagentOpts];
        slots.append(select(`Reagent ${s + 1}`, opts, v, (nv) => ((recipe.r[s] = nv), apply())));
      });
      slots.append(
        select("Form", forms.map((f, k) => [k, f]), recipe.form, (nv) => ((recipe.form = nv), apply())),
      );
      const catOpts: [number, string][] = [[NONE, "—"], ...catalysts.map((c, k): [number, string] => [k, c])];
      recipe.c.forEach((v, s) => {
        slots.append(select(`Catalyst ${s + 1}`, catOpts, v, (nv) => ((recipe.c[s] = nv), apply())));
      });
      card.append(slots);
      card.addEventListener("click", () => {
        game.select_flask(i);
        recipesEl.querySelectorAll(".recipe").forEach((el, k) => el.classList.toggle("active", k === i));
      });
      recipesEl.append(card);
    }
  }

  function toggleSpellcraft(open = spellcraft.hidden) {
    spellcraft.hidden = !open;
    if (open) renderRecipes();
  }
  $<HTMLButtonElement>("#spellcraft-open").addEventListener("click", () => toggleSpellcraft());
  $<HTMLButtonElement>("#spellcraft-close").addEventListener("click", () => toggleSpellcraft(false));
  renderRecipes();

  // ---- Main loop ------------------------------------------------------------
  const statsEl = $<HTMLDivElement>("#stats");
  let last = performance.now();
  let acc = 0;
  let frames = 0;
  let fpsTime = last;
  let simMs = 0;

  const frame = (now: number) => {
    acc += now - last;
    last = now;

    applyBrush();
    game.set_input(inputBits(), mouse.x, mouse.y);

    let ticks = 0;
    if (state.paused) {
      acc = 0;
    } else {
      ticks = Math.min(MAX_TICKS_PER_FRAME, Math.floor(acc / TICK_MS));
      acc = ticks === MAX_TICKS_PER_FRAME ? 0 : acc - ticks * TICK_MS;
    }
    if (ticks > 0) {
      const t0 = performance.now();
      game.tick(ticks);
      simMs = (performance.now() - t0) / ticks;
    }

    game.render();
    // Re-create views every frame: wasm memory may have grown.
    const size = VIEW_W * VIEW_H * 4;
    const cells = new Uint8Array(wasm.memory.buffer, game.view_ptr(), size);
    const overlay = new Uint8Array(wasm.memory.buffer, game.overlay_ptr(), size);
    renderer.draw(cells, overlay, game.camera_x(), game.camera_y(), now / 1000);
    updateHud();

    frames++;
    if (now - fpsTime >= 500) {
      const fps = (frames * 1000) / (now - fpsTime);
      statsEl.textContent =
        `${fps.toFixed(0)} fps · sim ${simMs.toFixed(2)} ms/tick · ` +
        `chunks ${game.awake_chunks()}/${game.chunk_count()} · seed ${seed}`;
      frames = 0;
      fpsTime = now;
    }
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);

  // Expose for debugging and automated smoke tests.
  (window as unknown as { physand: unknown }).physand = { game, ids, names, INPUT };
}

main().catch((err) => {
  console.error(err);
  document.body.innerHTML = `<pre style="color:#f88;padding:16px">Failed to start Physand:\n${String(err)}</pre>`;
});
