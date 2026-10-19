// Unit tests for the display filter's pure math (src/crtmath.ts) and its use in the touch layout (src/touchmath.ts).
import { expect, test } from "bun:test";
import {
  CRT_MIN_SCALE, crtScale, curveK, defaultPreset, derive, fitPicture, FLAT, gameToView, maskLayout, parOf, parsePreset, parseSaved, pictureSize, PRESET_IDS, PRESETS,
  presetOf, sameParams, sanitizeParams, scanFade, serializeSaved, TV_PAR, unwarp, viewToGame, warp, type View,
} from "../src/crtmath";
import { clientToGame, gameToClient, planLayout } from "../src/touchmath";

const box = { x: 100, y: 50, w: 800, h: 600 };

test("warp: identity at k = 0, middle of every edge stays on the edge, the whole frame stays inside the box", () => {
  expect(warp(0.3, -0.7, 0)).toEqual([0.3, -0.7]);
  for (const k of [0.02, 0.05, 0.1]) {
    // frame edge midpoints: screen position of the frame's edge middle is still +-1
    expect(warp(1, 0, k)).toEqual([1, 0]);
    expect(warp(0, -1, k)).toEqual([0, -1]);
    // the screen position that shows a frame corner is inside the box (nothing is cut): unwarp the corners
    for (const [cx, cy] of [[1, 1], [-1, 1], [1, -1], [-1, -1]]) {
      const [sx, sy] = unwarp(cx, cy, k);
      expect(Math.abs(sx)).toBeLessThan(1); expect(Math.abs(sy)).toBeLessThan(1);
      // and the frame edge between the corners bows out: along the top edge the screen position of frame y=-1 is never below the box's top
      for (let t = -1; t <= 1; t += 0.1) { const [, ey] = unwarp(t, -1, k); expect(ey).toBeGreaterThanOrEqual(-1 - 1e-9); }
    }
  }
});

test("unwarp inverts warp", () => {
  for (const k of [0, 0.03, 0.1]) {
    for (let x = -1; x <= 1; x += 0.125) for (let y = -1; y <= 1; y += 0.125) {
      const [ux, uy] = warp(x, y, k);
      const [rx, ry] = unwarp(ux, uy, k);
      expect(rx).toBeCloseTo(x, 9); expect(ry).toBeCloseTo(y, 9);
    }
  }
});

test("viewToGame: the flat view is exactly the old mapping", () => {
  expect(viewToGame(100, 50, { x: 100, y: 50, w: 512, h: 448 }, 256, 224, FLAT)).toMatchObject({ x: 0, y: 0, inside: true });
  expect(viewToGame(100 + 256, 50 + 224, { x: 100, y: 50, w: 512, h: 448 }, 256, 224, FLAT)).toMatchObject({ x: 128, y: 112, fx: 128, fy: 112 });
  expect(clientToGame(100 + 511, 50 + 447, { x: 100, y: 50, w: 512, h: 448 })).toMatchObject({ x: 255, y: 223 });
});

const views: [string, View][] = [
  ["flat", FLAT], ["8:7", { par: TV_PAR, curve: 0 }], ["8:7 + curve 0.4", { par: TV_PAR, curve: 0.4 }], ["square + curve 1", { par: 1, curve: 1 }], ["8:7 + curve 1", { par: TV_PAR, curve: 1 }],
];
for (const [name, v] of views) {
  test(`every game pixel is hit at its own centre, letterboxed boxes too (${name})`, () => {
    for (const el of [box, { x: 10, y: 20, w: 1000, h: 300 }, { x: 0, y: 0, w: 300, h: 1000 }, { x: 5, y: 5, w: 256 * v.par * 3.3, h: 224 * 3.3 }]) {
      for (let gy = 0; gy < 224; gy += 7) for (let gx = 0; gx < 256; gx += 5) {
        const c = gameToView(gx + 0.5, gy + 0.5, el, 256, 224, v);
        const g = viewToGame(c.x, c.y, el, 256, 224, v);
        expect(g.x).toBe(gx); expect(g.y).toBe(gy); expect(g.inside).toBe(true);
        expect(g.fx).toBeCloseTo(gx + 0.5, 6); expect(g.fy).toBeCloseTo(gy + 0.5, 6);
      }
    }
  });
}

test("viewToGame with a pixel aspect: a game pixel is 8/7 wider on screen", () => {
  const el = { x: 0, y: 0, w: 256 * TV_PAR * 4, h: 224 * 4 };
  const a = gameToView(10.5, 10.5, el, 256, 224, { par: TV_PAR, curve: 0 }), b = gameToView(11.5, 10.5, el, 256, 224, { par: TV_PAR, curve: 0 });
  expect(b.x - a.x).toBeCloseTo(4 * TV_PAR, 9);
  expect(viewToGame(el.w - 0.01, el.h - 0.01, el, 256, 224, { par: TV_PAR, curve: 0 })).toMatchObject({ x: 255, y: 223, inside: true });
});

test("with curvature the picture's corners are pulled in, the middle of each edge is not", () => {
  const v = { par: 1, curve: 1 }, el = { x: 0, y: 0, w: 1024, h: 896 };
  expect(viewToGame(512, 0.01, el, 256, 224, v).inside).toBe(true);        // top middle
  expect(viewToGame(0.5, 0.5, el, 256, 224, v).inside).toBe(false);        // the box corner now shows black: past the frame
  const c = gameToView(0, 0, el, 256, 224, v);                            // the frame's own corner is inside the box
  expect(c.x).toBeGreaterThan(0); expect(c.y).toBeGreaterThan(0);
  expect(viewToGame(c.x + 0.01, c.y + 0.01, el, 256, 224, v)).toMatchObject({ x: 0, y: 0 });
  expect(curveK(1)).toBeLessThanOrEqual(0.1);
});

test("crtScale keeps whole scales within 10 %, a fractional scale otherwise", () => {
  expect(crtScale(4.82)).toBe(4.82);
  expect(crtScale(4.3)).toBe(4);
  expect(crtScale(4.0)).toBe(4);
  expect(crtScale(6.43)).toBe(6);      // 1440p fullscreen: 7 % under the next whole scale
  expect(crtScale(6.7)).toBe(6.7);
  expect(crtScale(2.15)).toBe(2);
  expect(crtScale(1.9)).toBe(1.9);
  expect(crtScale(0)).toBe(0.25);
});

test("fitPicture: whole device pixels on both axes, whole scale lines are exactly 224 x scale tall", () => {
  for (const dpr of [1, 1.25, 1.5, 2, 2.625, 3]) {
    for (const [w, h] of [[1280, 720], [1920, 1080], [2560, 1440], [412, 915], [915, 412], [777, 531]]) {
      for (const par of [1, TV_PAR]) {
        const f = fitPicture(w, h, dpr, par, true);
        expect(Number.isInteger(f.devW) && Number.isInteger(f.devH)).toBe(true);
        expect(f.cssW * dpr).toBeCloseTo(f.devW, 9); expect(f.cssH * dpr).toBeCloseTo(f.devH, 9);
        expect(f.cssW).toBeLessThanOrEqual(w + 1 / dpr); expect(f.cssH).toBeLessThanOrEqual(h + 1 / dpr);
        if (Number.isInteger(f.scale)) expect(f.devH).toBe(224 * f.scale);
        const p = pictureSize(f.scale, par);
        expect(f.devW).toBe(p.devW);
      }
    }
  }
  // plain rule: whole scale, fractional only under 1x
  expect(fitPicture(1000, 700, 1, 1, false).scale).toBe(3);
  expect(fitPicture(200, 150, 1, 1, false).scale).toBeCloseTo(0.67, 1);
});

test("maskLayout: triad width in whole device pixels, at least 3; the mask fades out on small pictures", () => {
  for (let s = 1; s <= 10; s += 0.25) for (const k of ["aperture", "slot", "shadow"] as const) {
    const m = maskLayout(s, k);
    expect(Number.isInteger(m.pitch)).toBe(true); expect(m.pitch).toBeGreaterThanOrEqual(3);
    expect(m.fade).toBeGreaterThanOrEqual(0); expect(m.fade).toBeLessThanOrEqual(1);
  }
  expect(maskLayout(3.4, "slot").fade).toBe(0);      // a phone: 412 CSS px at DPR 2.6, about 3.4 device px per game line
  expect(maskLayout(2.0, "aperture").fade).toBe(0);
  expect(maskLayout(4.82, "slot").fade).toBe(1);     // 1080p fullscreen
  expect(maskLayout(6.43, "aperture").pitch).toBe(3);        // 0.34 of 6.43 is 2.2: the floor of 3 device pixels (one per colour)
  expect(maskLayout(10, "aperture").pitch).toBe(3);
  expect(maskLayout(11, "aperture").pitch).toBe(4);
  expect(maskLayout(12, "slot").pitch).toBe(6);
  expect(scanFade(1)).toBe(0); expect(scanFade(2.5)).toBe(1); expect(scanFade(2.0)).toBeGreaterThan(0.2);
});

test("derive: sane shader inputs at every scale and setting", () => {
  for (const id of PRESET_IDS) {
    for (let s = 2; s <= 12; s += 0.5) {
      const d = derive(PRESETS[id as Exclude<typeof id, "custom">], s);
      expect(d.sigLo).toBeGreaterThan(0.1); expect(d.sigHi).toBeGreaterThanOrEqual(d.sigLo - 1e-9); expect(d.sigHi).toBeLessThanOrEqual(0.7);
      expect(d.sigH).toBeGreaterThan(0.1); expect(d.sigH).toBeLessThanOrEqual(0.55);
      expect(d.gain).toBeGreaterThan(0.8); expect(d.gain).toBeLessThan(1.25);
      expect(Number.isInteger(d.pitch)).toBe(true);
    }
  }
  // no mask on a phone sized picture, even when the slider is up
  expect(derive({ ...PRESETS.tv, mask: 1 }, 3.4).mask).toBe(0);
  // a beam is never thinner than half a device pixel
  expect(derive({ ...PRESETS.pvm, scan: 1 }, 12).sigLo * 12).toBeGreaterThanOrEqual(0.55 - 1e-9);
});

test("settings: saved value round trip, bad values fall back, presets compare", () => {
  const dflt = { preset: "tv" as const, params: { ...PRESETS.tv } };
  expect(parseSaved(null, dflt)).toEqual(dflt);
  expect(parseSaved("not json", dflt)).toEqual(dflt);
  expect(parseSaved('{"preset":"nope"}', dflt)).toEqual(dflt);
  expect(parseSaved('{"preset":"pvm"}', dflt)).toEqual({ preset: "pvm", params: PRESETS.pvm });
  expect(parseSaved('{"preset":"off"}', dflt).params.scan).toBe(0);
  const custom = { preset: "custom" as const, params: { ...PRESETS.tv, scan: 0.9, curve: 0.2 } };
  expect(parseSaved(serializeSaved(custom), dflt)).toEqual(custom);
  expect(serializeSaved({ preset: "tv", params: PRESETS.tv })).toBe('{"preset":"tv"}');
  // out of range and wrong types are clamped / ignored
  const p = sanitizeParams({ scan: 5, mask: -2, glow: "0.4", sharp: "x", maskKind: "weird", shape: "square", curve: null }, PRESETS.tv);
  expect(p).toMatchObject({ scan: 1, mask: 0, glow: 0.4, sharp: PRESETS.tv.sharp, maskKind: PRESETS.tv.maskKind, shape: "square", curve: PRESETS.tv.curve });
  // a custom save that equals a preset becomes that preset
  expect(parseSaved(JSON.stringify({ preset: "custom", params: PRESETS.pvm }), dflt).preset).toBe("pvm");
  expect(presetOf({ ...PRESETS.tv, glow: 0.51 })).toBe("custom");
  expect(sameParams(PRESETS.tv, { ...PRESETS.tv })).toBe(true);
  expect(parsePreset("tv")).toBe("tv"); expect(parsePreset("x")).toBeNull();
  for (const id of PRESET_IDS) expect(presetOf(PRESETS[id as Exclude<typeof id, "custom">])).toBe(id);
});

test("defaults: the consumer TV on a computer, scanlines only on a phone; curvature is off in every preset", () => {
  expect(defaultPreset(false)).toBe("tv");
  expect(defaultPreset(true)).toBe("scan");
  for (const id of PRESET_IDS) expect(PRESETS[id as Exclude<typeof id, "custom">].curve).toBe(0);
  expect(parOf("tv")).toBeCloseTo(8 / 7, 12); expect(parOf("square")).toBe(1);
});

test("planLayout for the filter: whole device pixels, the pixel aspect, falls back when too small", () => {
  const safe = { t: 0, r: 0, b: 0, l: 0 };
  for (const [vw, vh, dpr] of [[412, 915, 2.625], [915, 412, 2.625], [390, 844, 3], [844, 390, 3], [768, 1024, 2], [1024, 768, 2]] as const) {
    for (const mode of ["pad", "game"] as const) {
      const L = planLayout({ vw, vh, dpr, safe, mode, size: 100, crt: { par: TV_PAR } });
      if (L.crt) {
        expect(L.scale).toBeGreaterThanOrEqual(CRT_MIN_SCALE);
        expect(Number.isInteger(Math.round(L.canvasW * dpr * 1e6) / 1e6)).toBe(true);
        expect(L.canvasW * dpr).toBeCloseTo(Math.round(L.canvasW * dpr), 6); expect(L.canvasH * dpr).toBeCloseTo(Math.round(L.canvasH * dpr), 6);
        expect(L.canvasW / L.canvasH).toBeCloseTo((256 * TV_PAR) / 224, 2);
        expect(L.canvasW).toBeLessThanOrEqual(vw); expect(L.canvasH).toBeLessThanOrEqual(vh);
      } else {
        expect(L.canvasW * dpr / L.scale).toBeCloseTo(256, 6);   // the plain layout: whole scale, square pixels
      }
    }
  }
  // a tiny window falls back to the plain layout
  const tiny = planLayout({ vw: 320, vh: 200, dpr: 1, safe, mode: "game", size: 100, crt: { par: TV_PAR } });
  expect(tiny.crt).toBe(false);
  // no crt input = exactly the old layout
  const a = planLayout({ vw: 390, vh: 844, dpr: 3, safe, mode: "pad", size: 100 }), b = planLayout({ vw: 390, vh: 844, dpr: 3, safe, mode: "pad", size: 100, crt: undefined });
  expect(a).toEqual(b); expect(a.crt).toBe(false);
});

test("the pointer lands on the right game pixel in a filter layout (phone sizes, 8:7, with and without curvature)", () => {
  const safe = { t: 0, r: 0, b: 0, l: 0 };
  for (const curve of [0, 0.5]) {
    const L = planLayout({ vw: 412, vh: 915, dpr: 2.625, safe, mode: "game", size: 100, crt: { par: TV_PAR } });
    expect(L.crt).toBe(true);
    const el = { x: (412 - L.canvasW) / 2, y: 20, w: L.canvasW, h: L.canvasH };
    const v = { par: TV_PAR, curve };
    for (const [gx, gy] of [[0, 0], [255, 0], [0, 223], [255, 223], [128, 112], [37, 190], [200, 14]]) {
      const c = gameToClient(gx, gy, el, 256, 224, v);
      expect(clientToGame(c.x, c.y, el, 256, 224, v)).toMatchObject({ x: gx, y: gy, inside: true });
    }
  }
});
