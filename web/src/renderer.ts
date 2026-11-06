// WebGL2 renderer: the wasm cell grid is uploaded each frame as an RGBA8
// texture ([mat, var, life, clock]) and coloured in the fragment shader.

const VERT = `#version 300 es
out vec2 v_uv;
void main() {
  // Full-screen triangle from gl_VertexID; no vertex buffers needed.
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  v_uv = p; // uv (0,0) = top-left, matching texture row 0 = top of the world
  gl_Position = vec4(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
}`;

const fragSource = (ids: MaterialIds, count: number) => `#version 300 es
precision highp float;
uniform sampler2D u_cells;
uniform vec3 u_palette[${count}];
uniform float u_time;
in vec2 v_uv;
out vec4 o_color;

const int EMPTY = 0;
const int FIRE = ${ids.fire};
const int LAVA = ${ids.lava};
const int WATER = ${ids.water};
const int STEAM = ${ids.steam};
const int SMOKE = ${ids.smoke};

void main() {
  vec4 c = texture(u_cells, v_uv);
  int mat = int(c.r * 255.0 + 0.5);
  float v = c.g;                 // per-cell variation, 0..1
  float life = c.b * 255.0;

  // Cave backdrop.
  vec3 col = mix(vec3(0.07, 0.06, 0.10), vec3(0.03, 0.03, 0.05), v_uv.y);

  if (mat != EMPTY) {
    col = u_palette[mat] * (0.88 + 0.24 * v);
  }
  if (mat == WATER) {
    col *= 0.92 + 0.08 * sin(u_time * 3.0 + v * 6.2831 + v_uv.x * 80.0);
  } else if (mat == FIRE) {
    // Hot yellow when fresh, deep red as it dies; embers glow orange.
    float t = clamp(life / 60.0, 0.0, 1.0);
    col = mix(vec3(0.75, 0.12, 0.02), vec3(1.0, 0.86, 0.35), t);
    col *= 0.85 + 0.3 * fract(v * 7.0 + u_time * 5.0);
  } else if (mat == LAVA) {
    col *= 1.0 + 0.25 * sin(u_time * 1.7 + v * 6.2831);
  } else if (mat == STEAM || mat == SMOKE) {
    // Gases fade out as they age.
    float a = clamp(life / 90.0, 0.25, 0.85);
    vec3 bg = mix(vec3(0.07, 0.06, 0.10), vec3(0.03, 0.03, 0.05), v_uv.y);
    col = mix(bg, u_palette[mat] * (0.88 + 0.24 * v), a);
  }
  o_color = vec4(col, 1.0);
}`;

export interface MaterialIds {
  fire: number;
  lava: number;
  water: number;
  steam: number;
  smoke: number;
}

export class Renderer {
  private gl: WebGL2RenderingContext;
  private program: WebGLProgram;
  private texture: WebGLTexture;
  private uTime: WebGLUniformLocation | null;

  constructor(
    private canvas: HTMLCanvasElement,
    private width: number,
    private height: number,
    palette: Float32Array,
    ids: MaterialIds,
  ) {
    const gl = canvas.getContext("webgl2", { antialias: false, alpha: false });
    if (!gl) throw new Error("WebGL2 is not supported in this browser.");
    this.gl = gl;

    this.program = link(gl, VERT, fragSource(ids, palette.length / 3));
    gl.useProgram(this.program);
    gl.uniform3fv(gl.getUniformLocation(this.program, "u_palette"), palette);
    gl.uniform1i(gl.getUniformLocation(this.program, "u_cells"), 0);
    this.uTime = gl.getUniformLocation(this.program, "u_time");

    // An empty VAO is required for attribute-less drawing.
    gl.bindVertexArray(gl.createVertexArray());

    const tex = gl.createTexture();
    if (!tex) throw new Error("Failed to create texture");
    this.texture = tex;
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texStorage2D(gl.TEXTURE_2D, 1, gl.RGBA8, width, height);
  }

  /** Fit the canvas to its container at an integer scale of the world size. */
  resize(maxW: number, maxH: number): number {
    const scale = Math.max(1, Math.floor(Math.min(maxW / this.width, maxH / this.height)));
    const cssW = this.width * scale;
    const cssH = this.height * scale;
    const dpr = window.devicePixelRatio || 1;
    this.canvas.style.width = `${cssW}px`;
    this.canvas.style.height = `${cssH}px`;
    this.canvas.width = Math.round(cssW * dpr);
    this.canvas.height = Math.round(cssH * dpr);
    return scale;
  }

  draw(cells: Uint8Array, timeSeconds: number): void {
    const gl = this.gl;
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, this.width, this.height, gl.RGBA, gl.UNSIGNED_BYTE, cells);
    gl.viewport(0, 0, this.canvas.width, this.canvas.height);
    gl.uniform1f(this.uTime, timeSeconds);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }
}

function compile(gl: WebGL2RenderingContext, type: number, src: string): WebGLShader {
  const shader = gl.createShader(type);
  if (!shader) throw new Error("Failed to create shader");
  gl.shaderSource(shader, src);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(`Shader compile error: ${gl.getShaderInfoLog(shader)}`);
  }
  return shader;
}

function link(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const program = gl.createProgram();
  if (!program) throw new Error("Failed to create program");
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, vs));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, fs));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(`Program link error: ${gl.getProgramInfoLog(program)}`);
  }
  return program;
}
