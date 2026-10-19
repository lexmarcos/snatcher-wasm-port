// bun test web/test/touchmath.test.ts – the pure parts of the touch input: coordinate transform with letterboxing, D-pad angles, hit-testing of
// menu rectangles, the layout for phones and tablets, the saved mode.
import { describe, expect, test } from "bun:test";
import { PAD } from "../src/gamepads";
import {
  BTN_MIN, clampNum, clientToGame, defaultTouchMode, dpadBits, gameToClient, hitRect, parseTouchMode, planLayout, type Insets, type Rect,
} from "../src/touchmath";

describe("client -> game pixel transform", () => {
  test("exact aspect box: linear, offsets from the box", () => {
    const el = { x: 100, y: 50, w: 512, h: 448 }; // 2x
    expect(clientToGame(100, 50, el)).toMatchObject({ x: 0, y: 0, inside: true });
    expect(clientToGame(100 + 511, 50 + 447, el)).toMatchObject({ x: 255, y: 223, inside: true });
    expect(clientToGame(100 + 256, 50 + 224, el)).toMatchObject({ x: 128, y: 112, inside: true });
    expect(clientToGame(100 + 255.9, 50 + 223.9, el)).toMatchObject({ x: 127, y: 111 });
  });
  test("a box wider than the picture has bars left and right (pillarbox)", () => {
    const el = { x: 0, y: 0, w: 800, h: 448 }; // picture 512 x 448, bars of 144
    expect(clientToGame(144 + 256, 224, el)).toMatchObject({ x: 128, y: 112, inside: true });
    const bar = clientToGame(100, 224, el);
    expect(bar.inside).toBe(false);
    expect(bar.x).toBe(0); // clamped to the edge pixel, still usable for a drag that leaves the picture
    expect(clientToGame(700, 224, el).x).toBe(255);
    expect(clientToGame(144, 0, el)).toMatchObject({ x: 0, y: 0, inside: true });
  });
  test("a box taller than the picture has bars above and below (letterbox)", () => {
    const el = { x: 10, y: 10, w: 512, h: 700 }; // picture 512 x 448, bars of 126
    expect(clientToGame(10 + 256, 10 + 126 + 224, el)).toMatchObject({ x: 128, y: 112, inside: true });
    expect(clientToGame(10 + 256, 10 + 50, el).inside).toBe(false);
    expect(clientToGame(10 + 256, 10 + 50, el).y).toBe(0);
    expect(clientToGame(10 + 256, 10 + 126 + 447, el)).toMatchObject({ y: 223, inside: true });
  });
  test("fractional device scales (phones) and the inverse", () => {
    const el = { x: 7.25, y: 19.5, w: 256 * 1.3125, h: 224 * 1.3125 }; // 1.3125 CSS px per game pixel (dpr 2.625 at scale 3.45...)
    for (const [gx, gy] of [[0, 0], [1, 1], [100, 77], [255, 223], [128, 112]]) {
      const c = gameToClient(gx, gy, el);
      expect(clientToGame(c.x, c.y, el)).toMatchObject({ x: gx, y: gy, inside: true });
    }
  });
  test("a degenerate box does not produce NaN", () => {
    expect(clientToGame(5, 5, { x: 0, y: 0, w: 0, h: 0 })).toMatchObject({ x: 0, y: 0, inside: false });
  });
});

describe("D-pad angle to direction", () => {
  const A = PAD.UP, V = PAD.DOWN, L = PAD.LEFT, R = PAD.RIGHT;
  // angle measured clockwise from "right" on the screen (y grows down)
  const at = (deg: number, m = 40) => dpadBits(Math.cos((deg * Math.PI) / 180) * m, Math.sin((deg * Math.PI) / 180) * m, 10);
  test("the four cardinal directions and the four diagonals", () => {
    expect(at(0)).toBe(R); expect(at(90)).toBe(V); expect(at(180)).toBe(L); expect(at(270)).toBe(A);
    expect(at(45)).toBe(R | V); expect(at(135)).toBe(L | V); expect(at(225)).toBe(L | A); expect(at(315)).toBe(R | A);
  });
  test("sector borders sit at 22.5 degrees", () => {
    expect(at(20)).toBe(R); expect(at(25)).toBe(R | V);
    expect(at(65)).toBe(R | V); expect(at(70)).toBe(V);
    expect(at(-20)).toBe(R); expect(at(-25)).toBe(R | A);
    expect(at(160)).toBe(L); expect(at(155)).toBe(L | V);
  });
  test("dead zone: nothing inside, a direction just outside", () => {
    expect(dpadBits(0, 0, 10)).toBe(0);
    expect(dpadBits(5, -5, 10)).toBe(0);
    expect(dpadBits(10, 0, 10)).toBe(0);
    expect(dpadBits(10.5, 0, 10)).toBe(R);
  });
  test("every one of 360 whole degrees yields 1 or 2 bits, never opposite directions", () => {
    for (let d = 0; d < 360; d++) {
      const b = at(d);
      expect(b).not.toBe(0);
      expect(!!(b & A) && !!(b & V)).toBe(false);
      expect(!!(b & L) && !!(b & R)).toBe(false);
    }
  });
});

describe("hit-testing the engine's rectangles", () => {
  // the 3-line command menu of Junker HQ: lines of 9 px from y = 143, x 8..248
  const lines: Rect[] = [0, 1, 2].map((i) => ({ x: 8, y: 143 + 9 * i, w: 240, h: 9, id: i }));
  test("inside a rectangle", () => {
    expect(hitRect(lines, 100, 143)?.id).toBe(0);
    expect(hitRect(lines, 100, 151)?.id).toBe(0);
    expect(hitRect(lines, 100, 152)?.id).toBe(1);
    expect(hitRect(lines, 247, 169)?.id).toBe(2);
  });
  test("without slop nothing outside matches; with slop the nearest line does", () => {
    expect(hitRect(lines, 100, 140)).toBeNull();
    expect(hitRect(lines, 100, 172)).toBeNull();
    expect(hitRect(lines, 100, 140, 0, 4)?.id).toBe(0);
    expect(hitRect(lines, 100, 173, 0, 4)?.id).toBe(2);
    expect(hitRect(lines, 100, 150, 0, 4)?.id).toBe(0);
    expect(hitRect(lines, 3, 160, 8, 0)?.id).toBe(1);
    expect(hitRect(lines, 100, 100, 0, 4)).toBeNull();
  });
  test("keys of the keypad: the nearest key wins in the gap", () => {
    const keys: Rect[] = [];
    for (let r = 0; r < 4; r++) for (let c = 0; c < 3; c++) keys.push({ x: 196 + 16 * c, y: 147 + 16 * r, w: 16, h: 16, id: r * 3 + c });
    expect(hitRect(keys, 204, 155)?.id).toBe(0);
    expect(hitRect(keys, 235, 203)?.id).toBe(11);
    expect(hitRect(keys, 190, 155, 8)?.id).toBe(0);
    expect(hitRect(keys, 150, 155, 8)).toBeNull();
  });
});

describe("saved settings", () => {
  test("mode parsing and default", () => {
    expect(parseTouchMode("pad")).toBe("pad"); expect(parseTouchMode("game")).toBe("game"); expect(parseTouchMode("off")).toBe("off");
    expect(parseTouchMode("x")).toBeNull(); expect(parseTouchMode(null)).toBeNull();
    expect(defaultTouchMode(true, 5)).toBe("pad");
    expect(defaultTouchMode(false, 5)).toBe("off"); // a laptop with a touch screen and a mouse
    expect(defaultTouchMode(true, 0)).toBe("off");
  });
  test("clamping", () => {
    expect(clampNum("120", 90, 140, 100)).toBe(120); expect(clampNum("500", 90, 140, 100)).toBe(140); expect(clampNum("x", 90, 140, 100)).toBe(100);
    expect(clampNum(null, 90, 140, 100)).toBe(100); expect(clampNum("", 90, 140, 100)).toBe(100);
  });
});

describe("layout for phones and tablets", () => {
  const none: Insets = { t: 0, r: 0, b: 0, l: 0 };
  // [name, width, height, dpr, safe insets]
  const devices: [string, number, number, number, Insets][] = [
    ["iPhone SE portrait", 375, 667, 2, { t: 20, r: 0, b: 0, l: 0 }],
    ["360x640 Android portrait", 360, 640, 3, none],
    ["iPhone 14 portrait", 390, 844, 3, { t: 47, r: 0, b: 34, l: 0 }],
    ["Pixel 412x915 portrait", 412, 915, 2.625, { t: 24, r: 0, b: 0, l: 0 }],
    ["iPhone 14 landscape", 844, 390, 3, { t: 0, r: 47, b: 21, l: 47 }],
    ["iPhone SE landscape", 667, 375, 2, none],
    ["Pixel 915x412 landscape", 915, 412, 2.625, { t: 0, r: 0, b: 0, l: 0 }],
    ["iPad portrait", 768, 1024, 2, { t: 24, r: 0, b: 20, l: 0 }],
    ["iPad landscape", 1024, 768, 2, { t: 24, r: 0, b: 20, l: 0 }],
    ["iPad Pro 12.9 landscape", 1366, 1024, 2, { t: 24, r: 0, b: 20, l: 0 }],
  ];
  for (const mode of ["pad", "game"] as const) for (const [name, vw, vh, dpr, safe] of devices) {
    test(`${mode}: ${name}`, () => {
      const L = planLayout({ vw, vh, dpr, safe, mode, size: 100 });
      const aw = vw - safe.l - safe.r, ah = vh - safe.t - safe.b;
      // the picture fits the safe area and has a whole device scale (or is below 1x)
      expect(L.canvasW).toBeLessThanOrEqual(aw + 1e-6); expect(L.canvasH).toBeLessThanOrEqual(ah + 1e-6);
      expect(L.canvasW / L.canvasH).toBeCloseTo(256 / 224, 6);
      if (L.scale >= 1) expect(Number.isInteger(L.scale)).toBe(true);
      // buttons are at least 48 px
      expect(L.u).toBeGreaterThanOrEqual(BTN_MIN);
      expect(L.landscape).toBe(vw > vh);
      if (!L.landscape) {
        expect(L.arr).toBe("below");
        expect(L.canvasH + L.reserveBottom).toBeLessThanOrEqual(ah + 1e-6);   // picture and controls do not overlap
        expect(L.u * 5 + 8 + 24 + 8).toBeLessThanOrEqual(aw + 1e-6 || Infinity); // the two clusters fit side by side
      } else if (L.arr === "sides") {
        expect(L.canvasW + 2 * L.reserveSide).toBeLessThanOrEqual(aw + 1e-6);
      } else {
        expect(L.arr).toBe("overlay");
      }
    });
  }
  test("a phone in landscape never gets a smaller picture than 3/4 of the largest one that fits", () => {
    for (const [, vw, vh, dpr, safe] of devices.filter((d) => d[1] > d[2])) {
      const L = planLayout({ vw, vh, dpr, safe, mode: "pad", size: 100 });
      const fullRaw = Math.floor(Math.min(((vw - safe.l - safe.r - 16) * dpr) / 256, ((vh - safe.t - safe.b - 16) * dpr) / 224));
      expect(L.scale).toBeGreaterThanOrEqual(0.75 * fullRaw);
    }
  });
  test("narrow landscape phone: pad over the picture; roomy landscape: pad beside it", () => {
    expect(planLayout({ vw: 667, vh: 375, dpr: 2, safe: none, mode: "pad", size: 100 }).arr).toBe("overlay");
    expect(planLayout({ vw: 915, vh: 412, dpr: 2.625, safe: none, mode: "pad", size: 100 }).arr).toBe("sides");
    expect(planLayout({ vw: 1366, vh: 1024, dpr: 2, safe: none, mode: "pad", size: 100 }).arr).toBe("sides");
    const ipad = planLayout({ vw: 1024, vh: 768, dpr: 2, safe: { t: 24, r: 0, b: 20, l: 0 }, mode: "pad", size: 100 });
    expect(ipad.arr).toBe("sides"); expect(ipad.scale).toBe(5); expect(ipad.u).toBeGreaterThanOrEqual(BTN_MIN); // the buttons shrink a little to fit beside the picture
    const se = planLayout({ vw: 667, vh: 375, dpr: 2, safe: none, mode: "pad", size: 100 });
    expect(se.u).toBe(BTN_MIN); expect(se.scale).toBe(3); // over the picture: the smallest buttons, so only the picture's black edge is covered
  });
  test("touch the game keeps more room for the picture than the pad does", () => {
    const pad = planLayout({ vw: 390, vh: 844, dpr: 3, safe: none, mode: "pad", size: 100 });
    const game = planLayout({ vw: 390, vh: 844, dpr: 3, safe: none, mode: "game", size: 100 });
    expect(game.reserveBottom).toBeLessThan(pad.reserveBottom);
    expect(game.scale).toBeGreaterThanOrEqual(pad.scale);
  });
  test("size setting scales the buttons, never below 48 px", () => {
    const small = planLayout({ vw: 844, vh: 390, dpr: 3, safe: none, mode: "pad", size: 90 });
    const big = planLayout({ vw: 844, vh: 390, dpr: 3, safe: none, mode: "pad", size: 140 });
    expect(small.u).toBeGreaterThanOrEqual(48); expect(big.u).toBeGreaterThan(small.u);
  });
});
