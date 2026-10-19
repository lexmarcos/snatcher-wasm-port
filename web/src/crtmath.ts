// Display filter ("CRT"): the pure parts (no DOM, no GL). Unit-tested in web/test/crtmath.test.ts. Documentation: docs/re/web.md section 12.
//   * the settings, the presets, how a saved value is read back
//   * the geometry shared by the shader and by every pointer path: pixel aspect (8:7), the mild barrel warp and its inverse
//   * the sizes that depend on the output: the picture scale, the mask pitch in whole device pixels, the fade of scanlines and mask when they cannot be resolved

export type CrtPreset = "off" | "tv" | "pvm" | "scan" | "custom";
export type MaskKind = "aperture" | "slot" | "shadow";
export type Shape = "tv" | "square";

export interface CrtParams {
  /** 0..1: how dark the gap between two lines gets (0 = no visible lines). */
  scan: number;
  /** 0..1: strength of the phosphor mask (it fades out by itself when the picture is too small to show it). */
  mask: number;
  maskKind: MaskKind;
  /** 0..1: halation, the soft bloom of bright areas. */
  glow: number;
  /** 0..1: horizontal sharpness (0 = soft like a consumer TV, 1 = crisp like a studio monitor). */
  sharp: number;
  /** 0..1: how much the picture bulges (0 = flat). Pointer input follows the same warp. */
  curve: number;
  /** 0..1: composite video blend (dithering and 1-pixel stripes merge, chroma bleeds a little). */
  composite: number;
  /** "tv": pixels 8:7 wide, as a TV showed the Mega Drive in its 256 pixel mode. "square": square pixels. */
  shape: Shape;
}

export const PRESET_IDS: CrtPreset[] = ["off", "tv", "pvm", "scan"];

/** The three real presets (tuned with the comparison harness, docs/re/web.md 12.5). "off" carries the neutral values so a slider move starts from something sane. */
export const PRESETS: Record<Exclude<CrtPreset, "custom">, CrtParams> = {
  off: { scan: 0, mask: 0, maskKind: "aperture", glow: 0, sharp: 1, curve: 0, composite: 0, shape: "square" },
  tv: { scan: 0.65, mask: 0.2, maskKind: "slot", glow: 0.45, sharp: 0.72, curve: 0, composite: 0.45, shape: "tv" },
  pvm: { scan: 0.9, mask: 0.18, maskKind: "aperture", glow: 0.3, sharp: 0.85, curve: 0, composite: 0, shape: "tv" },
  scan: { scan: 0.65, mask: 0, maskKind: "aperture", glow: 0.25, sharp: 0.7, curve: 0, composite: 0, shape: "tv" },
};

const clamp01 = (v: number) => (v < 0 ? 0 : v > 1 ? 1 : v);
const num = (v: unknown, d: number) => { const n = typeof v === "number" ? v : typeof v === "string" && v.trim() !== "" ? Number(v) : NaN; return Number.isFinite(n) ? clamp01(n) : d; };

export function parsePreset(v: unknown): CrtPreset | null {
  return v === "off" || v === "tv" || v === "pvm" || v === "scan" || v === "custom" ? v : null;
}

/** Make a params object out of anything (a saved value from an older version, a hand-edited key): unknown or bad fields fall back to `base`. */
export function sanitizeParams(v: unknown, base: CrtParams): CrtParams {
  const o = (v && typeof v === "object" ? v : {}) as Record<string, unknown>;
  const kind = o.maskKind === "aperture" || o.maskKind === "slot" || o.maskKind === "shadow" ? o.maskKind : base.maskKind;
  const shape = o.shape === "tv" || o.shape === "square" ? o.shape : base.shape;
  return {
    scan: num(o.scan, base.scan), mask: num(o.mask, base.mask), maskKind: kind, glow: num(o.glow, base.glow), sharp: num(o.sharp, base.sharp),
    curve: num(o.curve, base.curve), composite: num(o.composite, base.composite), shape,
  };
}

export const sameParams = (a: CrtParams, b: CrtParams) =>
  a.scan === b.scan && a.mask === b.mask && a.maskKind === b.maskKind && a.glow === b.glow && a.sharp === b.sharp && a.curve === b.curve && a.composite === b.composite && a.shape === b.shape;

/**
 * The preset a set of values equals, or "custom". "off" is never matched: sliders dragged to zero are still a custom style that draws through the filter
 * (the picture does not switch to plain pixels under the player's hand); the plain picture is chosen only by name.
 */
export function presetOf(p: CrtParams): CrtPreset {
  for (const id of ["tv", "pvm", "scan"] as const) if (sameParams(p, PRESETS[id])) return id;
  return "custom";
}

export interface CrtSaved { preset: CrtPreset; params: CrtParams }

/** The saved choice (JSON of {preset, params}); anything unreadable gives `dflt`. A preset other than "custom" always means its own values. */
export function parseSaved(raw: string | null, dflt: CrtSaved): CrtSaved {
  if (!raw) return dflt;
  let o: any;
  try { o = JSON.parse(raw); } catch { return dflt; }
  const preset = parsePreset(o?.preset);
  if (!preset) return dflt;
  if (preset !== "custom") return { preset, params: { ...PRESETS[preset] } };
  const params = sanitizeParams(o.params, PRESETS.tv);
  return { preset: presetOf(params), params };
}
export const serializeSaved = (s: CrtSaved) => JSON.stringify(s.preset === "custom" ? s : { preset: s.preset });

/**
 * The preset a new player gets. A big enough picture shows the consumer TV (slot mask, composite blend, glow); a phone or tablet (finger as main pointer) gets
 * the lighter "scanlines only": its picture is about 3.4 device pixels per game line, where the mask cannot be resolved, and the slim look keeps the battery.
 */
export function defaultPreset(coarsePrimary: boolean): CrtPreset { return coarsePrimary ? "scan" : "tv"; }

// ---------------------------------------------------------------- geometry

export const GAME_W = 256, GAME_H = 224;
/** Pixel aspect (width / height of one game pixel) of the Mega Drive's 256 pixel mode on a 4:3 TV: 8:7. */
export const TV_PAR = 8 / 7;
export const parOf = (shape: Shape) => (shape === "tv" ? TV_PAR : 1);

/** How the picture is displayed: what the pointer code has to undo. */
export interface View { par: number; curve: number }
export const FLAT: View = { par: 1, curve: 0 };
/** The one view of the page (set by the display code, read by clientToGame). */
export const view: View = { par: 1, curve: 0 };
export function setView(par: number, curve: number) { view.par = par; view.curve = curve; }

/** Warp strength for a slider value: at most 10 % of bulge, enough to see the tube, nothing is cut (see `warp`). */
export const curveK = (curve: number) => clamp01(curve) * 0.1;

/**
 * Screen -> picture. (nx, ny) is a point of the picture's box in -1..1 on both axes; the result is the point of the game frame in the same range.
 * u = n * (1 + k * (ny^2, nx^2)): the middle of every edge of the frame lands on the middle of the box edge and the corners are pulled in, so the whole frame stays
 * inside the box (nothing is cut, the text at the edges stays), the edges bow outward like a tube's. Exactly what the shader computes.
 */
export function warp(nx: number, ny: number, k: number): [number, number] {
  return [nx * (1 + k * ny * ny), ny * (1 + k * nx * nx)];
}
/** Picture -> screen (the inverse of `warp`): fixed-point iteration, converges fast for k <= 0.1. */
export function unwarp(ux: number, uy: number, k: number): [number, number] {
  let nx = ux, ny = uy;
  for (let i = 0; i < 24; i++) { nx = ux / (1 + k * ny * ny); ny = uy / (1 + k * nx * nx); }
  return [nx, ny];
}

export interface Box { x: number; y: number; w: number; h: number }

/**
 * Client (CSS pixel) point -> game pixels for a canvas box that holds the picture (letterboxed when the box is wider or taller than the picture's aspect).
 * `x`/`y` are whole game pixels clamped to the frame, `fx`/`fy` the exact position, `inside` says whether the point is on the frame at all.
 */
export function viewToGame(cx: number, cy: number, el: Box, gw: number, gh: number, v: View): { x: number; y: number; fx: number; fy: number; inside: boolean } {
  const vw = gw * v.par;
  const s = Math.min(el.w / vw, el.h / gh);
  if (!(s > 0)) return { x: 0, y: 0, fx: 0, fy: 0, inside: false };
  const ox = el.x + (el.w - vw * s) / 2, oy = el.y + (el.h - gh * s) / 2;
  let fx: number, fy: number;
  if (v.curve === 0) { fx = (cx - ox) / (s * v.par); fy = (cy - oy) / s; }
  else {
    const [ux, uy] = warp(((cx - ox) / (vw * s)) * 2 - 1, ((cy - oy) / (gh * s)) * 2 - 1, curveK(v.curve));
    fx = ((ux + 1) / 2) * gw; fy = ((uy + 1) / 2) * gh;
  }
  const clamp = (n: number, hi: number) => Math.max(0, Math.min(hi - 1, Math.floor(n)));
  return { x: clamp(fx, gw), y: clamp(fy, gh), fx, fy, inside: fx >= 0 && fx < gw && fy >= 0 && fy < gh };
}

/** Game pixel (fractional) -> client position of that point: the inverse of `viewToGame` (tests, overlays). */
export function gameToView(gx: number, gy: number, el: Box, gw: number, gh: number, v: View): { x: number; y: number } {
  const vw = gw * v.par;
  const s = Math.min(el.w / vw, el.h / gh);
  const ox = el.x + (el.w - vw * s) / 2, oy = el.y + (el.h - gh * s) / 2;
  if (v.curve === 0) return { x: ox + gx * s * v.par, y: oy + gy * s };
  let nx = (gx / gw) * 2 - 1, ny = (gy / gh) * 2 - 1;
  if (v.curve !== 0) [nx, ny] = unwarp(nx, ny, curveK(v.curve));
  return { x: ox + ((nx + 1) / 2) * vw * s, y: oy + ((ny + 1) / 2) * gh * s };
}

// ---------------------------------------------------------------- sizes

/**
 * The scale (device pixels per game line) of the picture with the filter on, from the largest scale that fits. The scanlines come from the position of the
 * source line, so a fractional scale works, but a whole one gives perfectly even lines: when the whole scale below is within 10 % of the largest, it is used.
 */
export function crtScale(raw: number): number {
  if (!(raw > 0)) return 0.25;
  const k = Math.floor(raw);
  if (k >= 2 && (raw - k) / k < 0.1) return k;
  return raw;
}
/** Below this many device pixels per game line the lines and the mask cannot show: the page draws the plain picture instead. */
export const CRT_MIN_SCALE = 2;

/** Picture size in device pixels for a scale: whole device pixels on both axes (height = 224 x scale when the scale is whole). */
export function pictureSize(scale: number, par: number): { devW: number; devH: number } {
  return { devW: Math.max(1, Math.round(GAME_W * par * scale)), devH: Math.max(1, Math.round(GAME_H * scale)) };
}

const smooth = (a: number, b: number, x: number) => { const t = clamp01((x - a) / (b - a)); return t * t * (3 - 2 * t); };

/** Scanlines fade out between 1.5 and 2.5 device pixels per line (moire guard; the page does not use the filter under 2 anyway). */
export const scanFade = (scale: number) => smooth(1.5, 2.5, scale);

/**
 * Mask layout for a scale: `pitch` is the width of one RGB triad in WHOLE device pixels (at least 3: one device pixel per colour), `fade` 0..1 multiplies the
 * strength. The pitch follows the picture size (about 0.5 game pixel per triad for a TV mask, 0.34 for an aperture grille) so the mask looks the same size at
 * every scale; the fade switches the mask off when fewer than about 3.5 device pixels cover a game line (phones), where it could only show as colour fringes.
 */
export function maskLayout(scale: number, kind: MaskKind): { pitch: number; fade: number } {
  const q = kind === "aperture" ? 0.34 : 0.5;
  return { pitch: Math.max(3, Math.round(scale * q)), fade: smooth(3.5, 4.5, scale) };
}

/**
 * Biggest picture for a free area (CSS px) at a device pixel ratio: the scale, and the box in CSS pixels that is a whole number of device pixels
 * (so the canvas backing store maps 1:1 to the screen and the mask lines up). `crt` false = the plain rule: a whole scale, fractional only below 1x.
 */
export function fitPicture(availW: number, availH: number, dpr: number, par: number, crt: boolean): { scale: number; cssW: number; cssH: number; devW: number; devH: number } {
  const raw = Math.min((availW * dpr) / (GAME_W * par), (availH * dpr) / GAME_H);
  const scale = crt ? crtScale(raw) : raw >= 1 ? Math.floor(raw) : Math.max(0.25, raw);
  const { devW, devH } = pictureSize(scale, par);
  return { scale, cssW: devW / dpr, cssH: devH / dpr, devW, devH };
}

// ---------------------------------------------------------------- shader inputs

export interface CrtDerived {
  /** warp strength (shader `uK`) */
  k: number;
  /** vertical beam width (sigma, in game lines) for a black and for a full-bright pixel; the width grows with the brightness in between */
  sigLo: number; sigHi: number;
  /** horizontal sigma in game pixels (the reconstruction between two pixels) */
  sigH: number;
  /** mask strength after the fade, triad pitch in whole device pixels, kind as 0 aperture, 1 slot, 2 shadow */
  mask: number; pitch: number; maskKind: number;
  glow: number;
  /** composite blend amount (0 = off) */
  composite: number;
  /** brightness compensation for what the lines and the mask take away (shader `uGain`) */
  gain: number;
}

/** Everything the shader needs for a set of settings at a picture scale (device pixels per game line). */
export function derive(p: CrtParams, scale: number): CrtDerived {
  const scanE = p.scan * scanFade(scale);
  const ml = maskLayout(scale, p.maskKind);
  const maskE = p.mask * ml.fade;
  // a beam thinner than about 0.85 device pixel is not drawn evenly: keep a floor in device pixels
  const floorV = 0.85 / scale;
  const sigLo = Math.max(floorV, 0.60 - 0.45 * scanE), sigHi = Math.max(floorV, 0.60 - 0.30 * scanE);
  const sigH = Math.max(0.5 / scale, 0.52 - 0.38 * p.sharp);
  const glow = 0.22 * p.glow;
  const gain = (1 + 0.04 * maskE + 0.06 * scanE + 0.08 * p.composite) / (1 + glow);
  return {
    k: curveK(p.curve), sigLo, sigHi, sigH,
    mask: maskE, pitch: ml.pitch, maskKind: p.maskKind === "aperture" ? 0 : p.maskKind === "slot" ? 1 : 2,
    glow, composite: p.composite, gain,
  };
}
