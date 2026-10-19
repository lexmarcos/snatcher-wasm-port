// Page side of the comparison harness (test/crt-compare.ts bundles this with Bun.build): renders a stored game frame through the real filter
// (src/crt.ts) or the plain nearest-neighbour path, onto a black surface of a given size in device pixels, and hands the PNG back.
import { CrtFilter } from "../src/crt";
import { derive, fitPicture, parOf, PRESETS, sanitizeParams, GAME_W, GAME_H, type CrtParams } from "../src/crtmath";

const gl = document.getElementById("gl") as HTMLCanvasElement, out = document.getElementById("out") as HTMLCanvasElement;
let filter: CrtFilter | null = null;
const frames = new Map<string, Uint8Array>();

async function frame(name: string): Promise<Uint8Array> {
  let f = frames.get(name);
  if (f) return f;
  const img = new Image(); img.src = `/frames/${name}.png`; await img.decode();
  const c = document.createElement("canvas"); c.width = GAME_W; c.height = GAME_H;
  const x = c.getContext("2d")!; x.drawImage(img, 0, 0);
  f = new Uint8Array(x.getImageData(0, 0, GAME_W, GAME_H).data.buffer.slice(0));
  frames.set(name, f); return f;
}
const lin = (v: number) => { const c = v / 255; return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4); };

export interface RenderIn {
  /** a frame name (frames/NAME.png) or "flat:R,G,B" for a uniform test frame */
  frame: string;
  /** "off" or a preset id, or a full params object */
  mode: string | Partial<CrtParams>;
  /** surface size in device pixels, and the margin kept free around the picture (device px) */
  w: number; h: number; mx?: number; my?: number;
  /** device pixels per CSS pixel is only used for the report: everything here is in device pixels */
  crop?: { x: number; y: number; w: number; h: number; zoom: number };
  /** keep only this game-pixel column range (the compare strips): the output is cropped to the picture rows and these columns */
  cols?: [number, number];
  /** also return the pixels' statistics of the picture area */
  stats?: boolean;
}
export interface RenderOut { png: string; scale: number; picture: { x: number; y: number; w: number; h: number }; meanLin: number; meanY: number; lineProfile?: number[]; ms: number }

async function render(a: RenderIn): Promise<RenderOut> {
  let fr: Uint8Array;
  if (a.frame.startsWith("flat:")) {
    const [r, g, b] = a.frame.slice(5).split(",").map(Number);
    fr = new Uint8Array(GAME_W * GAME_H * 4); for (let i = 0; i < fr.length; i += 4) { fr[i] = r; fr[i + 1] = g; fr[i + 2] = b; fr[i + 3] = 255; }
  } else fr = await frame(a.frame);
  out.width = a.w; out.height = a.h;
  const ctx = out.getContext("2d")!;
  ctx.fillStyle = "#000"; ctx.fillRect(0, 0, a.w, a.h);
  const availW = a.w - 2 * (a.mx ?? 0), availH = a.h - 2 * (a.my ?? 0);
  let pic: { x: number; y: number; w: number; h: number }, scale: number, ms = 0;
  if (a.mode === "off") {
    scale = Math.max(1, Math.floor(Math.min(availW / GAME_W, availH / GAME_H)));
    const w = GAME_W * scale, h = GAME_H * scale;
    pic = { x: Math.floor((a.w - w) / 2), y: Math.floor((a.h - h) / 2), w, h };
    const src = document.createElement("canvas"); src.width = GAME_W; src.height = GAME_H;
    src.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(fr.buffer.slice(0) as ArrayBuffer), GAME_W, GAME_H), 0, 0);
    ctx.imageSmoothingEnabled = false; ctx.drawImage(src, pic.x, pic.y, w, h);
  } else {
    const p: CrtParams = typeof a.mode === "string" ? { ...(PRESETS as any)[a.mode] } : sanitizeParams(a.mode, PRESETS.tv);
    const fit = fitPicture(availW, availH, 1, parOf(p.shape), true);
    scale = fit.scale;
    pic = { x: Math.floor((a.w - fit.devW) / 2), y: Math.floor((a.h - fit.devH) / 2), w: fit.devW, h: fit.devH };
    filter ??= CrtFilter.create(gl, true);
    if (!filter) throw new Error("no WebGL2");
    filter.setSize(fit.devW, fit.devH); filter.setParams(p);
    filter.upload(fr);
    const t0 = performance.now();
    filter.draw(scale);
    const px = filter.readPixels();
    ms = performance.now() - t0;
    ctx.putImageData(new ImageData(new Uint8ClampedArray(px.buffer as ArrayBuffer), fit.devW, fit.devH), pic.x, pic.y);
  }
  // statistics over the picture area
  const d = ctx.getImageData(pic.x, pic.y, pic.w, pic.h).data;
  let sl = 0, sy = 0;
  for (let i = 0; i < d.length; i += 4) { const y = 0.2126 * lin(d[i]) + 0.7152 * lin(d[i + 1]) + 0.0722 * lin(d[i + 2]); sl += y; sy += 0.2126 * d[i] + 0.7152 * d[i + 1] + 0.0722 * d[i + 2]; }
  const n = d.length / 4;
  let lineProfile: number[] | undefined;
  if (a.stats) {
    // mean linear luminance of every output row of the picture (the scanline profile)
    lineProfile = [];
    for (let y = 0; y < pic.h; y++) { let s = 0; for (let x = 0; x < pic.w; x++) { const i = (y * pic.w + x) * 4; s += 0.2126 * lin(d[i]) + 0.7152 * lin(d[i + 1]) + 0.0722 * lin(d[i + 2]); } lineProfile.push(s / pic.w); }
  }
  let region = { x: 0, y: 0, w: a.w, h: a.h, zoom: 1 };
  if (a.cols) {
    const sx = pic.w / GAME_W;
    region = { x: Math.round(pic.x + a.cols[0] * sx), y: pic.y, w: Math.round((a.cols[1] - a.cols[0]) * sx), h: pic.h, zoom: 1 };
  }
  if (a.crop) region = a.crop;
  const o2 = document.createElement("canvas"); o2.width = region.w * region.zoom; o2.height = region.h * region.zoom;
  const c2 = o2.getContext("2d")!; c2.imageSmoothingEnabled = false;
  c2.drawImage(out, region.x, region.y, region.w, region.h, 0, 0, o2.width, o2.height);
  return { png: o2.toDataURL("image/png"), scale, picture: pic, meanLin: sl / n, meanY: sy / n / 255, lineProfile, ms };
}
(window as any).crtRender = render;
(window as any).crtReady = true;

/** Tile PNG data URLs side by side with a 2 px divider (the compare images). */
async function join(urls: string[]): Promise<string> {
  const imgs = await Promise.all(urls.map(async (u) => { const i = new Image(); i.src = u; await i.decode(); return i; }));
  const w = imgs.reduce((s, i) => s + i.width, 0) + 2 * (imgs.length - 1), h = Math.max(...imgs.map((i) => i.height));
  const c = document.createElement("canvas"); c.width = w; c.height = h;
  const x = c.getContext("2d")!; x.fillStyle = "#808080"; x.fillRect(0, 0, w, h);
  let px = 0; for (const i of imgs) { x.drawImage(i, px, 0); px += i.width + 2; }
  return c.toDataURL("image/png");
}
(window as any).crtJoin = join;

/** Throughput of the filter alone (upload + draw, then gl.finish): ms per frame; the CPU side (calls only) is reported apart. */
async function bench(a: { frame: string; mode: string | Partial<CrtParams>; w: number; h: number; frames: number }): Promise<{ ms: number; cpuMs: number; gpuMs: number | null; renderer: string; scale: number }> {
  const fr = await frame(a.frame);
  const p: CrtParams = typeof a.mode === "string" ? { ...(PRESETS as any)[a.mode] } : sanitizeParams(a.mode, PRESETS.tv);
  const fit = fitPicture(a.w, a.h, 1, parOf(p.shape), true);
  filter ??= CrtFilter.create(gl, true);
  if (!filter) throw new Error("no WebGL2");
  filter.setSize(fit.devW, fit.devH); filter.setParams(p);
  const g = filter.gl;
  const info = g.getExtension("WEBGL_debug_renderer_info");
  const run = (n: number) => { let cpu = 0; for (let i = 0; i < n; i++) { const c = performance.now(); filter!.upload(fr); filter!.draw(fit.scale); cpu += performance.now() - c; } const one = new Uint8Array(4); g.bindFramebuffer(g.FRAMEBUFFER, null); g.readPixels(0, 0, 1, 1, g.RGBA, g.UNSIGNED_BYTE, one); return cpu; };   // reading a pixel waits for the GPU (finish() does not in Chrome)
  run(20);
  filter.stats.frames = 0; filter.stats.cpuMs = 0; filter.stats.gpuMs = null;
  const t0 = performance.now();
  const cpu = run(a.frames);
  const ms = (performance.now() - t0) / a.frames;
  // GPU time from timer queries (they resolve between frames: draw one frame per animation frame)
  for (let i = 0; i < 80; i++) { filter.upload(fr); filter.draw(fit.scale); await new Promise((r) => requestAnimationFrame(r)); }
  return { ms, cpuMs: cpu / a.frames, gpuMs: filter.stats.gpuMs, renderer: info ? String(g.getParameter(info.UNMASKED_RENDERER_WEBGL)) : "?", scale: fit.scale };
}
(window as any).crtBench = bench;
