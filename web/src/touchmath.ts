// Touch input: the pure parts (no DOM). Unit-tested in web/test/touchmath.test.ts. Documentation: docs/re/web.md section 11.
//   * client (CSS pixel) coordinates -> game pixels, with letterboxing
//   * a drag / press offset -> D-pad bits (8 directions)
//   * hit-testing of the rectangles the engine reports (menu lines, keys, slots)
//   * the layout of the picture and the touch controls for a viewport (phones and tablets, portrait and landscape, safe areas)
//   * the saved touch mode and its default
import { PAD, stickDirs } from "./gamepads";
import { crtScale, CRT_MIN_SCALE, pictureSize, view as pageView, viewToGame, gameToView, type View } from "./crtmath";

export interface Box { x: number; y: number; w: number; h: number }

export const GAME_W = 256, GAME_H = 224;

/**
 * Map a client point to game pixels. `el` is the canvas' box in client (CSS) pixels. The picture keeps its aspect inside the box
 * (`object-fit: contain`): when the box is wider or taller than 256:224 there are bars, and a point on a bar is `inside: false`
 * (its x / y are still extrapolated, clamped to the picture's edge pixels, so a drag that leaves the picture keeps working).
 * `x`/`y` are whole game pixels 0..255 / 0..223; `fx`/`fy` the exact (unclamped) position.
 * `v` is how the picture is displayed (pixel aspect 8:7 and the barrel warp of the display filter, crtmath.ts); the default is the page's current view,
 * which is flat (square pixels, no warp) whenever the filter is off.
 */
export function clientToGame(cx: number, cy: number, el: Box, gw = GAME_W, gh = GAME_H, v: View = pageView): { x: number; y: number; fx: number; fy: number; inside: boolean } {
  return viewToGame(cx, cy, el, gw, gh, v);
}

/** Inverse: a game pixel to the client position of its centre (tests, overlays). */
export function gameToClient(gx: number, gy: number, el: Box, gw = GAME_W, gh = GAME_H, v: View = pageView): { x: number; y: number } {
  return gameToView(gx + 0.5, gy + 0.5, el, gw, gh, v);
}

/** Pad bits (UP 1, DOWN 2, LEFT 4, RIGHT 8) for an offset from the D-pad centre: dead zone, then 8 sectors of 45 degrees (diagonals set two bits). */
export function dpadBits(dx: number, dy: number, dead: number): number {
  const m = Math.hypot(dx, dy);
  if (!(m > dead) || !(m > 0)) return 0;
  const d = stickDirs(dx / m, dy / m, 0.5); // unit vector: the sector test is the one of the gamepad stick (22.5 degree boundaries)
  return (d.up ? PAD.UP : 0) | (d.down ? PAD.DOWN : 0) | (d.left ? PAD.LEFT : 0) | (d.right ? PAD.RIGHT : 0);
}

export interface Rect extends Box { id: number; k?: number }

/**
 * The rectangle under (x, y); when none contains the point, the nearest one within `slopX` / `slopY` of its edges (so a thumb that lands
 * between two small lines, or just beside a key, still picks one). Ties go to the first. null = nothing near.
 */
export function hitRect<T extends Rect>(rects: readonly T[], x: number, y: number, slopX = 0, slopY = slopX): T | null {
  let best: T | null = null, bestD = Infinity;
  for (const r of rects) {
    const dx = x < r.x ? r.x - x : x >= r.x + r.w ? x - (r.x + r.w - 1) : 0;
    const dy = y < r.y ? r.y - y : y >= r.y + r.h ? y - (r.y + r.h - 1) : 0;
    if (dx > slopX || dy > slopY) continue;
    const d = Math.hypot(dx, dy);
    if (d < bestD) { best = r; bestD = d; if (d === 0) break; }
  }
  return best;
}

// ---------------------------------------------------------------- saved settings

export type TouchMode = "pad" | "game" | "off";
export const TOUCH_MODES: TouchMode[] = ["pad", "game", "off"];
export const parseTouchMode = (v: unknown): TouchMode | null => (v === "pad" || v === "game" || v === "off" ? v : null);
/** The default: the on-screen pad on a device whose main pointer is a finger, nothing on a computer (a touch screen next to a mouse stays "off" until the player turns it on). */
export const defaultTouchMode = (coarsePrimary: boolean, maxTouchPoints: number): TouchMode => (coarsePrimary && maxTouchPoints > 0 ? "pad" : "off");
export const clampNum = (v: unknown, lo: number, hi: number, dflt: number): number => {
  const n = typeof v === "string" && v.trim() !== "" ? Number(v) : typeof v === "number" ? v : NaN;
  return Number.isFinite(n) ? Math.max(lo, Math.min(hi, n)) : dflt;
};
export const PAD_SIZE_RANGE = [90, 140] as const;      // percent of the base button size (56 px: 50 px at the smallest, still above 48)
export const PAD_OPACITY_RANGE = [30, 100] as const;   // percent

// ---------------------------------------------------------------- layout

export interface Insets { t: number; r: number; b: number; l: number }
export type Arrangement = "below" | "sides" | "overlay";
export interface LayoutIn {
  vw: number; vh: number; dpr: number; safe: Insets;
  mode: "pad" | "game";
  /** user size, percent (PAD_SIZE_RANGE) */
  size: number;
  /** the display filter wants the picture: pixels `par` wide, a fractional scale is fine. Falls back to the plain layout when the picture would be under CRT_MIN_SCALE. */
  crt?: { par: number };
}
export interface LayoutOut {
  arr: Arrangement;
  landscape: boolean;
  /** base button size in CSS px: the D-pad is 3 x u, the A B C block 2 x u + gap */
  u: number;
  /** picture size in CSS px and the scale it was derived from (device pixels per game pixel) */
  canvasW: number; canvasH: number; scale: number;
  /** the picture is laid out for the display filter (pixel aspect, whole device pixels); false = the plain layout */
  crt: boolean;
  /** space reserved under the picture (below) or at each side (sides) for the controls, CSS px */
  reserveBottom: number; reserveSide: number;
}

export const PAD_GAP = 8;            // gap between buttons
export const PAD_MARGIN = 12;        // distance from the screen edge (on top of the safe area)
export const BTN_MIN = 48;           // smallest button, CSS px
const BASE_U = 56;
const TOP_GAP = 8;

/**
 * Where the picture and the controls go. Portrait: the picture on top, the controls below (D-pad left, A B C right, a Menu / Start row above them).
 * Landscape: the controls in the free space left and right of the picture ("sides"); when the free space is too narrow for a clearly smaller
 * picture to be worth it, the controls are drawn over the picture's edges ("overlay") and the picture keeps its larger scale.
 * The picture always gets a whole device-pixel scale (crisp pixels) except below 1x.
 */
export function planLayout(i: LayoutIn): LayoutOut {
  if (i.crt) {
    const o = planPicture(i, i.crt.par, true);
    if (o.scale >= CRT_MIN_SCALE) return o;
  }
  return planPicture(i, 1, false);
}

function planPicture(i: LayoutIn, par: number, crt: boolean): LayoutOut {
  const { vw, vh, dpr } = i;
  const availW = vw - i.safe.l - i.safe.r, availH = vh - i.safe.t - i.safe.b;
  const landscape = vw > vh;
  const tablet = Math.min(vw, vh) >= 600;
  let u = (BASE_U * i.size) / 100 * (tablet ? 1.15 : 1);
  const scaleFor = (w: number, h: number) => {
    const raw = Math.min((w * dpr) / (GAME_W * par), (h * dpr) / GAME_H);
    return crt ? crtScale(raw) : raw >= 1 ? Math.floor(raw) : Math.max(0.25, raw);
  };
  const size = (s: number) => (crt ? (({ devW, devH }) => ({ w: devW / dpr, h: devH / dpr }))(pictureSize(s, par)) : { w: (GAME_W * s) / dpr, h: (GAME_H * s) / dpr });
  const out = (arr: Arrangement, s: number, reserveBottom: number, reserveSide: number): LayoutOut =>
    ({ arr, landscape, u, canvasW: size(s).w, canvasH: size(s).h, scale: s, crt, reserveBottom, reserveSide });
  const rowH = BTN_MIN;                                   // the Menu / Start / Back row
  if (!landscape) {
    // the two clusters (3u and 2u + gap wide) plus a gap and the margins must fit the width
    u = Math.max(BTN_MIN, Math.min(u, (availW - PAD_GAP - 2 * PAD_MARGIN - PAD_GAP) / 5));
    const bottom = (i.mode === "pad" ? rowH + PAD_GAP + 3 * u : rowH) + PAD_MARGIN + PAD_GAP;
    return out("below", scaleFor(availW - 2 * PAD_GAP, availH - bottom - TOP_GAP), bottom, 0);
  }
  // landscape: the largest picture scale (not below 3/4 of the largest that fits at all) that leaves each side column wide enough for the controls
  // at their size (the buttons shrink to the room, never below 48 px); else the controls go over the picture's edges with the smallest buttons
  const full = scaleFor(availW - 2 * PAD_GAP, availH - 2 * TOP_GAP);
  const uBase = u;
  for (let s = full; s >= 1 && s >= 0.75 * full; s = Math.ceil(s) - 1) {   // full, then the whole scales below it
    const free = (availW - size(s).w) / 2;
    if (i.mode === "pad") {
      const uFit = (free - PAD_MARGIN - 4) / 3;                // the D-pad (3u) is the widest cluster
      if (uFit >= BTN_MIN) { u = Math.min(uBase, uFit); return out("sides", s, 0, 3 * u + PAD_MARGIN); }
    } else if (free >= 96 + PAD_MARGIN + 4) return out("sides", s, 0, 96 + PAD_MARGIN);
  }
  u = i.mode === "pad" ? BTN_MIN : u;
  return out("overlay", full, 0, 0);
}
