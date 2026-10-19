// Display filter: a small WebGL2 pipeline that draws the engine's 256x224 frame like a good CRT. Written from the physics (beam, mask, halation), no code taken
// from any other shader. Documentation: docs/re/web.md section 12. The pure math (settings, warp, sizes) is in crtmath.ts.
//
// Passes (all textures SRGB8_ALPHA8: 8 bits, but the hardware decodes on read and encodes on write, so every blend is done in linear light):
//   0  upload     the frame goes into `src` (texSubImage2D straight from the wasm memory)
//   1  composite  (optional, 256x224) NTSC-like luma / chroma softening in the signal (gamma) domain, so dither patterns and 1-pixel stripes blend
//   2  glow       frame -> 128x112 -> blur H, V -> 64x56 -> blur H, V (halation, only redone when a new frame came)
//   3  screen     at the canvas' own device resolution: warp, horizontal reconstruction, vertical beam per source line, mask, glow, encode
// Nothing here touches the engine's frame buffer: the 2D canvas keeps being drawn as before (tests and the fallback read it).
import { derive, type CrtParams } from "./crtmath";

const W = 256, H = 224;

const VS = `#version 300 es
out vec2 vUv;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  vUv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

const HEAD = `#version 300 es
precision highp float;
precision highp int;
in vec2 vUv;
out vec4 o;
`;

// 1: composite. Input texels are decoded (linear), the filter works on the encoded signal like a TV's decoder does.
const FS_COMPOSITE = HEAD + `
uniform sampler2D uSrc;
uniform float uAmt;
vec3 enc(vec3 c) { return mix(c * 12.92, 1.055 * pow(max(c, 0.0), vec3(1.0 / 2.4)) - 0.055, step(0.0031308, c)); }
vec3 dec(vec3 c) { return mix(c / 12.92, pow((max(c, 0.0) + 0.055) / 1.055, vec3(2.4)), step(0.04045, c)); }
vec3 yiq(vec3 c) { return vec3(dot(c, vec3(0.299, 0.587, 0.114)), dot(c, vec3(0.596, -0.274, -0.322)), dot(c, vec3(0.211, -0.523, 0.312))); }
vec3 rgb(vec3 q) { return vec3(q.x + 0.956 * q.y + 0.621 * q.z, q.x - 0.272 * q.y - 0.647 * q.z, q.x - 1.106 * q.y + 1.703 * q.z); }
vec3 tap(ivec2 p, int dx) { return yiq(enc(texelFetch(uSrc, ivec2(clamp(p.x + dx, 0, 255), p.y), 0).rgb)); }
void main() {
  ivec2 p = ivec2(gl_FragCoord.xy);
  vec3 c0 = tap(p, 0), cm = tap(p, -1), cp = tap(p, 1);
  float a = 0.22 * uAmt;                       // luma: a short low-pass (1 - 2a at the centre) that takes a 1-pixel pattern down to about 1 - 4a
  float y = a * (cm.x + cp.x) + (1.0 - 2.0 * a) * c0.x;
  // chroma has a much smaller bandwidth: 7 taps, triangular
  vec2 iq = 4.0 * c0.yz;
  iq += 3.0 * (cm.yz + cp.yz);
  iq += 2.0 * (tap(p, -2).yz + tap(p, 2).yz);
  iq += 1.0 * (tap(p, -3).yz + tap(p, 3).yz);
  iq /= 16.0;
  iq = mix(c0.yz, iq, uAmt);
  o = vec4(dec(clamp(rgb(vec3(y, iq)), 0.0, 1.0)), 1.0);
}`;

// 2: glow. Down-sample with 4 bilinear taps, blur with the usual 5 linear taps (a 9 tap gaussian).
const FS_DOWN = HEAD + `
uniform sampler2D uSrc;
uniform vec2 uTexel;                 // texel size of the INPUT
void main() {
  vec2 d = uTexel * 0.5;
  o = 0.25 * (texture(uSrc, vUv + vec2(-d.x, -d.y)) + texture(uSrc, vUv + vec2(d.x, -d.y)) + texture(uSrc, vUv + vec2(-d.x, d.y)) + texture(uSrc, vUv + vec2(d.x, d.y)));
}`;
const FS_BLUR = HEAD + `
uniform sampler2D uSrc;
uniform vec2 uDir;                   // one texel step along the blur axis
void main() {
  vec4 c = texture(uSrc, vUv) * 0.2270270270;
  c += (texture(uSrc, vUv + uDir * 1.3846153846) + texture(uSrc, vUv - uDir * 1.3846153846)) * 0.3162162162;
  c += (texture(uSrc, vUv + uDir * 3.2307692308) + texture(uSrc, vUv - uDir * 3.2307692308)) * 0.0702702703;
  o = c;
}`;

// 3: the screen
const FS_SCREEN = HEAD + `
uniform sampler2D uSrc;              // decoded frame (the composite pass output when it is on)
uniform sampler2D uGlow1;            // 128x112
uniform sampler2D uGlow2;            // 64x56
uniform vec2 uOut;                   // canvas size, device pixels
uniform float uK;                    // warp
uniform vec2 uSig;                   // vertical beam sigma for dark / bright pixels, in game lines
uniform float uSigH;                 // horizontal sigma in game pixels
uniform float uMask;                 // mask strength after the fade
uniform float uPitch;                // triad width in whole device pixels
uniform int uKind;                   // 0 aperture grille, 1 slot, 2 shadow
uniform float uGlow;
uniform float uGain;

// normal CDF (tanh form, error < 5e-4); a box seen through a gaussian is a difference of two of these
float cdf(float z) { z = clamp(z, -12.0, 12.0); return 0.5 + 0.5 * tanh(0.7978845608 * (z + 0.044715 * z * z * z)); }
vec3 cdf3(vec3 z) { z = clamp(z, -12.0, 12.0); return 0.5 + 0.5 * tanh(0.7978845608 * (z + 0.044715 * z * z * z)); }
vec3 enc(vec3 c) { return mix(c * 12.92, 1.055 * pow(max(c, 0.0), vec3(1.0 / 2.4)) - 0.055, step(0.0031308, c)); }

void main() {
  vec2 uv = vec2(vUv.x, 1.0 - vUv.y);                       // top-left origin
  vec2 n = uv * 2.0 - 1.0;
  vec2 u = n * (1.0 + uK * vec2(n.y * n.y, n.x * n.x));     // screen -> frame (same formula as crtmath.warp)
  vec2 sp = (u * 0.5 + 0.5) * vec2(256.0, 224.0);           // frame position in game pixels
  vec2 fw = max(fwidth(sp), vec2(1e-4));
  float cov = clamp(min(sp.x, 256.0 - sp.x) / fw.x + 0.5, 0.0, 1.0) * clamp(min(sp.y, 224.0 - sp.y) / fw.y + 0.5, 0.0, 1.0);
  if (cov <= 0.0) { o = vec4(0.0, 0.0, 0.0, 1.0); return; }

  // horizontal: each game pixel is a box, seen through a gaussian (the beam's and the electronics' bandwidth)
  float fx = floor(sp.x);
  float ws[5]; float wsum = 0.0;
  for (int t = 0; t < 5; t++) {
    float i = fx + float(t - 2);
    ws[t] = cdf((sp.x - i) / uSigH) - cdf((sp.x - i - 1.0) / uSigH);
    wsum += ws[t];
  }
  float wn = 1.0 / wsum;

  // vertical: every game line is a beam. Its width grows with the brightness of that channel; its energy is the pixel's value, so the average brightness is kept
  float dy = max(abs(dFdy(sp.y)), 1e-3);
  float y0 = sp.y - 0.5 * dy, y1 = sp.y + 0.5 * dy;
  int j0 = int(floor(sp.y - 0.5)) - 1;
  vec3 beam = vec3(0.0);
  for (int l = 0; l < 4; l++) {
    int j = j0 + l;
    if (j < 0 || j > 223) continue;
    vec3 c = vec3(0.0);
    for (int t = 0; t < 5; t++) c += ws[t] * texelFetch(uSrc, ivec2(clamp(int(fx) + t - 2, 0, 255), j), 0).rgb;
    c *= wn;
    float lum = dot(c, vec3(0.2126, 0.7152, 0.0722));
    vec3 b = sqrt(clamp(0.5 * vec3(lum) + 0.5 * c, 0.0, 1.0));
    vec3 sg = mix(vec3(uSig.x), vec3(uSig.y), b);
    float cy = float(j) + 0.5;
    vec3 w = (cdf3((y1 - cy) / sg) - cdf3((y0 - cy) / sg)) / dy;
    beam += c * w;
  }

  // mask, in whole device pixels (gl_FragCoord is a pixel centre: floor() is the pixel's index)
  vec3 m = vec3(1.0);
  if (uMask > 0.0) {
    float xd = floor(gl_FragCoord.x), yd = floor(uOut.y - gl_FragCoord.y);
    float gap = 1.0;
    float sx = xd;
    if (uKind == 1) {
      // slot: the bridge between two slots sits on the boundary of two game lines, staggered from one triad to the next
      float tri = floor(xd / uPitch);
      float row = floor(sp.y);
      float d = min(fract(sp.y), 1.0 - fract(sp.y));
      float bridge = 1.0 - smoothstep(0.0, 0.2, d);
      gap = 1.0 - 0.85 * bridge * mod(tri + row, 2.0);
    } else if (uKind == 2) {
      // shadow: dots in rows, every other row shifted half a triad
      float rp = max(2.0, floor(uPitch * 0.866 + 0.5));
      float rowi = floor(yd / rp);
      sx = xd + mod(rowi, 2.0) * floor(uPitch * 0.5);
      gap = 1.0 - 0.85 * (1.0 - step(1.0, mod(yd, rp)));
    }
    float a = mod(sx, uPitch);
    float sw = uPitch / 3.0;
    vec3 T;
    for (int c = 0; c < 3; c++) {
      float lo = float(c) * sw, hi = lo + sw;
      T[c] = max(0.0, min(a + 1.0, hi) - max(a, lo)) + max(0.0, min(a + 1.0, hi + uPitch) - max(a, lo + uPitch));
    }
    // a bright phosphor blooms over its neighbours: the mask fades with the channel's own brightness, so it never clips and white keeps its level
    vec3 sc = uMask * (1.0 - 0.85 * smoothstep(0.3, 1.0, beam));
    m = mix(vec3(1.0), T * 3.0 * gap, sc);
  }

  vec3 g = texture(uGlow1, u * 0.5 + 0.5).rgb * 0.55 + texture(uGlow2, u * 0.5 + 0.5).rgb * 0.45;
  vec3 col = (beam * m + uGlow * g) * uGain;
  // shoulder instead of a hard clip: keeps the hue of the brightest areas
  vec3 hi = max(col - 0.85, 0.0);
  col = min(col, 0.85) + 0.15 * tanh(hi / 0.15);
  o = vec4(enc(clamp(col * cov, 0.0, 1.0)), 1.0);
}`;

interface Prog { p: WebGLProgram; u: Record<string, WebGLUniformLocation | null> }
interface Target { tex: WebGLTexture; fb: WebGLFramebuffer; w: number; h: number }

export interface CrtStats { frames: number; cpuMs: number; gpuMs: number | null }

/** The filter. Create with `CrtFilter.create(canvas)` (null when WebGL2 is not there or the shaders do not build). */
export class CrtFilter {
  readonly gl: WebGL2RenderingContext;
  lost = false;
  onLost: (() => void) | null = null;
  onRestored: (() => void) | null = null;
  /** frames drawn, the sum of CPU time spent in draw() and (where the browser has timer queries) the GPU time, for the performance numbers */
  stats: CrtStats = { frames: 0, cpuMs: 0, gpuMs: null };
  private progs!: { composite: Prog; down: Prog; blur: Prog; screen: Prog };
  private src!: WebGLTexture;
  private comp!: Target;
  private g1a!: Target; private g1b!: Target; private g2a!: Target; private g2b!: Target;
  private vao!: WebGLVertexArrayObject;
  private params: CrtParams | null = null;
  private devW = 0; private devH = 0;
  private haveFrame = false;
  private glowDirty = true;
  private timerExt: any = null;
  private query: WebGLQuery | null = null;

  private constructor(readonly canvas: HTMLCanvasElement, gl: WebGL2RenderingContext) {
    this.gl = gl;
    this.timerExt = gl.getExtension("EXT_disjoint_timer_query_webgl2");
    canvas.addEventListener("webglcontextlost", (e) => { e.preventDefault(); this.lost = true; this.onLost?.(); });
    canvas.addEventListener("webglcontextrestored", () => { try { this.init(); this.lost = false; this.haveFrame = false; this.onRestored?.(); } catch (e) { console.warn("display filter: could not restore", e); this.lost = true; } });
  }

  /**
   * `allowSoftware` false (the page's default): a browser that would run WebGL on a software rasterizer (SwiftShader, llvmpipe: no usable GPU) gets null, because the
   * filter would make the game crawl there and the plain picture is the better choice. The harness and the tests allow it (they check pictures, not speed).
   */
  static create(canvas: HTMLCanvasElement, allowSoftware = false): CrtFilter | null {
    let gl: WebGL2RenderingContext | null = null;
    try {
      gl = canvas.getContext("webgl2", { alpha: false, antialias: false, depth: false, stencil: false, premultipliedAlpha: false, preserveDrawingBuffer: false, failIfMajorPerformanceCaveat: !allowSoftware }) as WebGL2RenderingContext | null;
    } catch { gl = null; }
    if (!gl) return null;
    if (!allowSoftware) {
      // browsers that do not honour the caveat flag: look at the renderer name
      const info = gl.getExtension("WEBGL_debug_renderer_info");
      const name = info ? String(gl.getParameter(info.UNMASKED_RENDERER_WEBGL)) : "";
      if (/swiftshader|llvmpipe|softpipe|software|basic render/i.test(name)) return null;
    }
    try { const f = new CrtFilter(canvas, gl); f.init(); return f; } catch (e) { console.warn("display filter unavailable:", e); return null; }
  }

  private compile(type: number, src: string): WebGLShader {
    const gl = this.gl, s = gl.createShader(type)!;
    gl.shaderSource(s, src); gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error("shader: " + gl.getShaderInfoLog(s));
    return s;
  }
  private program(fs: string, names: string[]): Prog {
    const gl = this.gl, p = gl.createProgram()!;
    gl.attachShader(p, this.compile(gl.VERTEX_SHADER, VS)); gl.attachShader(p, this.compile(gl.FRAGMENT_SHADER, fs));
    gl.linkProgram(p);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error("link: " + gl.getProgramInfoLog(p));
    const u: Record<string, WebGLUniformLocation | null> = {};
    for (const n of names) u[n] = gl.getUniformLocation(p, n);
    return { p, u };
  }
  private target(w: number, h: number): Target {
    const gl = this.gl, tex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texStorage2D(gl.TEXTURE_2D, 1, gl.SRGB8_ALPHA8, w, h);
    this.sampler(gl.LINEAR);
    const fb = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, fb);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
    if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) throw new Error("framebuffer incomplete");
    return { tex, fb, w, h };
  }
  private sampler(filter: number) {
    const gl = this.gl;
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, filter); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, filter);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  }

  /** (Re)build every GL object: first time, and after a lost context. */
  private init() {
    const gl = this.gl;
    this.progs = {
      composite: this.program(FS_COMPOSITE, ["uSrc", "uAmt"]),
      down: this.program(FS_DOWN, ["uSrc", "uTexel"]),
      blur: this.program(FS_BLUR, ["uSrc", "uDir"]),
      screen: this.program(FS_SCREEN, ["uSrc", "uGlow1", "uGlow2", "uOut", "uK", "uSig", "uSigH", "uMask", "uPitch", "uKind", "uGlow", "uGain"]),
    };
    this.src = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, this.src);
    gl.texStorage2D(gl.TEXTURE_2D, 1, gl.SRGB8_ALPHA8, W, H);
    this.sampler(gl.LINEAR);
    this.comp = this.target(W, H);
    this.g1a = this.target(128, 112); this.g1b = this.target(128, 112);
    this.g2a = this.target(64, 56); this.g2b = this.target(64, 56);
    this.vao = gl.createVertexArray()!;
    gl.bindVertexArray(this.vao);
    gl.disable(gl.DEPTH_TEST); gl.disable(gl.BLEND);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 4);
    this.glowDirty = true;
  }

  setParams(p: CrtParams) { this.params = p; }
  /** Backing size of the canvas in device pixels (set by the page from the same numbers that size the box). */
  setSize(devW: number, devH: number) {
    if (devW === this.devW && devH === this.devH) return;
    this.devW = devW; this.devH = devH;
    this.canvas.width = devW; this.canvas.height = devH;
  }
  get size() { return { w: this.devW, h: this.devH }; }

  /** Copy a 256x224 RGBA frame into the source texture (no allocation: the view is the caller's). */
  upload(frame: Uint8Array) {
    if (this.lost) return;
    const gl = this.gl;
    gl.bindTexture(gl.TEXTURE_2D, this.src);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, W, H, gl.RGBA, gl.UNSIGNED_BYTE, frame);
    this.haveFrame = true; this.glowDirty = true;
  }

  private pass(prog: Prog, t: Target | null, w: number, h: number) {
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, t ? t.fb : null);
    gl.viewport(0, 0, w, h);
    gl.useProgram(prog.p);
  }
  private bind(unit: number, tex: WebGLTexture, loc: WebGLUniformLocation | null) {
    const gl = this.gl;
    gl.activeTexture(gl.TEXTURE0 + unit); gl.bindTexture(gl.TEXTURE_2D, tex); gl.uniform1i(loc, unit);
  }

  /** Draw the last uploaded frame at scale `scale` (device pixels per game line; decides the mask pitch and the fades). */
  draw(scale: number) {
    if (this.lost || !this.params || !this.haveFrame || !this.devW) return;
    const gl = this.gl, P = this.progs, d = derive(this.params, scale);
    const t0 = performance.now();
    let q: WebGLQuery | null = null;
    if (this.timerExt && !this.query) { q = gl.createQuery()!; gl.beginQuery(this.timerExt.TIME_ELAPSED_EXT, q); }
    let main = this.src;
    if (d.composite > 0) {
      this.pass(P.composite, this.comp, W, H);
      this.bind(0, this.src, P.composite.u.uSrc); gl.uniform1f(P.composite.u.uAmt, d.composite);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      main = this.comp.tex;
    }
    if (d.glow > 0 && this.glowDirty) {
      this.pass(P.down, this.g1a, 128, 112);
      this.bind(0, this.src, P.down.u.uSrc); gl.uniform2f(P.down.u.uTexel, 1 / W, 1 / H); gl.drawArrays(gl.TRIANGLES, 0, 3);
      this.blur(this.g1a, this.g1b, 128, 112);
      this.pass(P.down, this.g2a, 64, 56);
      this.bind(0, this.g1a.tex, P.down.u.uSrc); gl.uniform2f(P.down.u.uTexel, 1 / 128, 1 / 112); gl.drawArrays(gl.TRIANGLES, 0, 3);
      this.blur(this.g2a, this.g2b, 64, 56);
      this.glowDirty = false;
    }
    const S = P.screen;
    this.pass(S, null, this.devW, this.devH);
    this.bind(0, main, S.u.uSrc); this.bind(1, this.g1a.tex, S.u.uGlow1); this.bind(2, this.g2a.tex, S.u.uGlow2);
    gl.uniform2f(S.u.uOut, this.devW, this.devH);
    gl.uniform1f(S.u.uK, d.k); gl.uniform2f(S.u.uSig, d.sigLo, d.sigHi); gl.uniform1f(S.u.uSigH, d.sigH);
    gl.uniform1f(S.u.uMask, d.mask); gl.uniform1f(S.u.uPitch, d.pitch); gl.uniform1i(S.u.uKind, d.maskKind);
    gl.uniform1f(S.u.uGlow, d.glow); gl.uniform1f(S.u.uGain, d.gain);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    if (q) { gl.endQuery(this.timerExt.TIME_ELAPSED_EXT); this.query = q; }
    this.stats.frames++; this.stats.cpuMs += performance.now() - t0;
    this.pollTimer();
  }
  /** blur `a` in place through `b` (H into b, V back into a) */
  private blur(a: Target, b: Target, w: number, h: number) {
    const gl = this.gl, B = this.progs.blur;
    this.pass(B, b, w, h); this.bind(0, a.tex, B.u.uSrc); gl.uniform2f(B.u.uDir, 1 / w, 0); gl.drawArrays(gl.TRIANGLES, 0, 3);
    this.pass(B, a, w, h); this.bind(0, b.tex, B.u.uSrc); gl.uniform2f(B.u.uDir, 0, 1 / h); gl.drawArrays(gl.TRIANGLES, 0, 3);
  }
  private pollTimer() {
    const gl = this.gl, q = this.query, ext = this.timerExt;
    if (!q || !ext) return;
    if (!gl.getQueryParameter(q, gl.QUERY_RESULT_AVAILABLE)) return;
    if (!gl.getParameter(ext.GPU_DISJOINT_EXT)) { const ms = (gl.getQueryParameter(q, gl.QUERY_RESULT) as number) / 1e6; this.stats.gpuMs = this.stats.gpuMs === null ? ms : this.stats.gpuMs * 0.95 + ms * 0.05; }
    gl.deleteQuery(q); this.query = null;
  }

  /** The canvas' pixels as RGBA (top row first), read right after a draw (tests, luminance numbers). */
  readPixels(): Uint8Array {
    const gl = this.gl, w = this.devW, h = this.devH, raw = new Uint8Array(w * h * 4);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, raw);
    const out = new Uint8Array(w * h * 4);
    for (let y = 0; y < h; y++) out.set(raw.subarray((h - 1 - y) * w * 4, (h - y) * w * 4), y * w * 4);
    return out;
  }

  dispose() { try { this.gl.getExtension("WEBGL_lose_context")?.loseContext(); } catch { /* gone */ } }
}
