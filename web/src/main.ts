import init, { Game, material_color, material_count, material_name } from "./wasm/physand.js";
import { Renderer, type MaterialIds } from "./renderer";

const WORLD_W = 480;
const WORLD_H = 270;
const TICK_HZ = 60;
const TICK_MS = 1000 / TICK_HZ;
/** Cap catch-up work after a stall (e.g. a background tab) to avoid a spiral. */
const MAX_TICKS_PER_FRAME = 4;

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
  };

  let seed = (Math.random() * 2 ** 32) >>> 0;
  const game = new Game(WORLD_W, WORLD_H, seed);
  const canvas = $<HTMLCanvasElement>("#view");
  const renderer = new Renderer(canvas, WORLD_W, WORLD_H, palette, ids);

  // ---- Layout -------------------------------------------------------------
  const stage = $<HTMLDivElement>("#stage");
  const fit = () => renderer.resize(stage.clientWidth - 16, stage.clientHeight - 16);
  new ResizeObserver(fit).observe(stage);
  fit();

  // ---- UI -----------------------------------------------------------------
  const state = {
    material: idOf("Sand"),
    brush: 4,
    paused: false,
    stepOnce: false,
  };

  const paintable = names.map((_, i) => i).filter((i) => i !== 0);
  const materialsEl = $<HTMLDivElement>("#materials");
  const buttons = new Map<number, HTMLButtonElement>();
  paintable.forEach((mat, n) => {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.setAttribute("role", "radio");
    const key = n < 9 ? String(n + 1) : n === 9 ? "0" : "";
    const swatch = `rgb(${palette.slice(mat * 3, mat * 3 + 3).map((v) => Math.round(v * 255)).join(",")})`;
    btn.innerHTML = `<span class="swatch" style="background:${swatch}"></span>${names[mat]}${key ? ` <kbd>${key}</kbd>` : ""}`;
    btn.addEventListener("click", () => selectMaterial(mat));
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
  $<HTMLButtonElement>("#step").addEventListener("click", () => (state.stepOnce = true));
  const newScene = () => {
    seed = (Math.random() * 2 ** 32) >>> 0;
    game.reset(seed);
  };
  $<HTMLButtonElement>("#reset").addEventListener("click", newScene);
  $<HTMLButtonElement>("#clear").addEventListener("click", () => game.clear());

  window.addEventListener("keydown", (e) => {
    if (e.target instanceof HTMLInputElement) return;
    if (e.code === "Space") {
      e.preventDefault();
      togglePause();
    } else if (e.key === ".") {
      state.stepOnce = true;
    } else if (e.key === "r" || e.key === "R") {
      newScene();
    } else if (/^[0-9]$/.test(e.key)) {
      const n = e.key === "0" ? 9 : Number(e.key) - 1;
      if (paintable[n] !== undefined) selectMaterial(paintable[n]);
    } else if (e.key === "[") {
      setBrush(state.brush - 1);
    } else if (e.key === "]") {
      setBrush(state.brush + 1);
    }
  });

  // ---- Painting -----------------------------------------------------------
  // Pointer positions are queued and painted on the next frame, interpolated
  // between samples so fast strokes leave a continuous line.
  let painting: { erase: boolean; last: [number, number] | null } | null = null;
  let pending: [number, number][] = [];

  const toWorld = (e: PointerEvent): [number, number] => {
    const rect = canvas.getBoundingClientRect();
    return [
      Math.floor(((e.clientX - rect.left) / rect.width) * WORLD_W),
      Math.floor(((e.clientY - rect.top) / rect.height) * WORLD_H),
    ];
  };

  canvas.addEventListener("contextmenu", (e) => e.preventDefault());
  canvas.addEventListener("pointerdown", (e) => {
    canvas.setPointerCapture(e.pointerId);
    painting = { erase: e.button === 2, last: null };
    pending.push(toWorld(e));
  });
  canvas.addEventListener("pointermove", (e) => {
    if (painting) pending.push(toWorld(e));
  });
  const stop = () => (painting = null);
  canvas.addEventListener("pointerup", stop);
  canvas.addEventListener("pointercancel", stop);
  canvas.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      setBrush(state.brush + (e.deltaY < 0 ? 1 : -1));
    },
    { passive: false },
  );

  const applyPaint = () => {
    if (!painting) {
      pending = [];
      return;
    }
    const mat = painting.erase ? 0 : state.material;
    for (const p of pending) {
      const from = painting.last ?? p;
      const dist = Math.hypot(p[0] - from[0], p[1] - from[1]);
      const steps = Math.max(1, Math.ceil(dist / Math.max(1, state.brush / 2)));
      for (let s = 1; s <= steps; s++) {
        const t = s / steps;
        game.paint(
          Math.round(from[0] + (p[0] - from[0]) * t),
          Math.round(from[1] + (p[1] - from[1]) * t),
          state.brush,
          mat,
        );
      }
      painting.last = p;
    }
    // Holding still keeps pouring at the last position.
    if (pending.length === 0 && painting.last) {
      game.paint(painting.last[0], painting.last[1], state.brush, mat);
    }
    pending = [];
  };

  // ---- Main loop ----------------------------------------------------------
  const statsEl = $<HTMLDivElement>("#stats");
  let last = performance.now();
  let acc = 0;
  let frames = 0;
  let fpsTime = last;
  let simMs = 0;

  const frame = (now: number) => {
    acc += now - last;
    last = now;

    applyPaint();

    let ticks = 0;
    if (state.paused) {
      acc = 0;
      if (state.stepOnce) ticks = 1;
    } else {
      ticks = Math.min(MAX_TICKS_PER_FRAME, Math.floor(acc / TICK_MS));
      acc = ticks === MAX_TICKS_PER_FRAME ? 0 : acc - ticks * TICK_MS;
    }
    state.stepOnce = false;
    if (ticks > 0) {
      const t0 = performance.now();
      game.tick(ticks);
      simMs = (performance.now() - t0) / ticks;
    }

    // Re-create the view every frame: wasm memory may have grown.
    const cells = new Uint8Array(wasm.memory.buffer, game.cells_ptr(), WORLD_W * WORLD_H * 4);
    renderer.draw(cells, now / 1000);

    frames++;
    if (now - fpsTime >= 500) {
      const fps = (frames * 1000) / (now - fpsTime);
      statsEl.textContent = `${fps.toFixed(0)} fps · sim ${simMs.toFixed(2)} ms/tick · seed ${seed}`;
      frames = 0;
      fpsTime = now;
    }
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);

  // Expose for debugging and automated smoke tests.
  (window as unknown as { physand: unknown }).physand = { game, ids, names };
}

main().catch((err) => {
  console.error(err);
  document.body.innerHTML = `<pre style="color:#f88;padding:16px">Failed to start Physand:\n${String(err)}</pre>`;
});
