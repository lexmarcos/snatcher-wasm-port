// The display filter (picture styles) in headless Chrome. Software GL is what headless has, so these checks are about pictures and geometry, not speed
// (speed: test/crt-perf.ts). Own server on CRT_TEST_PORT (default 5374).
//   bun web/test/crt.ts [shots-dir]       default work/shots/crt-test (gitignored)
// Part A needs no game data: shader build, settings and their persistence, Clear everything, the layout in whole device pixels (DPR changes), line evenness at
// awkward scales, the mask lined up with device pixels, the warp in the shader against the pointer mapping, WebGL missing, a lost context.
// Part B needs the local extraction (assets/raw) and skips cleanly without it: the frame buffer untouched, the luminance of every preset, the light gun with a
// mouse and fingers with the filter on (with and without curvature and pixel aspect), the layouts on phones. Exit code 1 on any failed check or console error.
import puppeteer, { type Browser, type CDPSession, type Page } from "puppeteer-core";
import { existsSync, mkdirSync, readdirSync, writeFileSync } from "fs";
import { join } from "path";
import { homedir } from "os";

const root = join(import.meta.dir, "..", "..");
const out = process.argv[2] ?? join(root, "work/shots/crt-test");
mkdirSync(out, { recursive: true });
const haveDisc = existsSync(join(root, "assets/raw/filelist.tsv"));
const PART = process.env.CRT_PART ?? "AB";   // CRT_PART=A or B runs one part
const port = Number(process.env.CRT_TEST_PORT ?? 5374);
let failed = 0, errors = 0;
const check = (name: string, ok: boolean, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"} ${name}${extra ? " - " + extra : ""}`); if (!ok) failed++; };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
/** close a page by closing its browser context, never waiting more than 8 s (a page that is reloading after Clear everything can be slow to close) */
const closeP = (p: Page) => Promise.race([p.browserContext().close().catch(() => undefined), sleep(8000)]);

let browser: Browser | undefined;
let server: ReturnType<typeof Bun.spawn> | null = null;
process.on("exit", () => { try { server?.kill(); } catch { /* gone */ } try { browser?.process()?.kill(); } catch { /* gone */ } });
const listening = async () => { try { await fetch(`http://localhost:${port}/`); return true; } catch { return false; } };
if (!(await listening())) {
  server = Bun.spawn(["bun", "server.ts"], { cwd: join(root, "web"), env: { ...process.env, PORT: String(port) }, stdout: "ignore", stderr: "ignore" });
  for (let i = 0; i < 50 && !(await listening()); i++) await sleep(100);
}
const base = join(homedir(), ".cache/puppeteer/chrome-headless-shell");
const chrome = join(base, readdirSync(base).sort().pop()!, "chrome-headless-shell-linux64/chrome-headless-shell");
browser = await puppeteer.launch({ executablePath: chrome, args: ["--autoplay-policy=no-user-gesture-required"] });

interface Opts { w?: number; h?: number; dpr?: number; mobile?: boolean; query?: string; noGL?: boolean; storage?: Record<string, string>; clear?: boolean }
async function open(o: Opts = {}): Promise<{ page: Page; cdp: CDPSession }> {
  const ctx = await browser!.createBrowserContext();
  const page = await ctx.newPage();
  const IPHONE = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
  if (o.mobile) await page.emulate({ viewport: { width: o.w ?? 412, height: o.h ?? 915, deviceScaleFactor: o.dpr ?? 2.625, isMobile: true, hasTouch: true }, userAgent: IPHONE });
  else await page.setViewport({ width: o.w ?? 1000, height: o.h ?? 760, deviceScaleFactor: o.dpr ?? 1 });
  const cdp = await page.createCDPSession();
  page.on("console", (m) => { if (m.type() === "error" && !/404/.test(m.text())) { errors++; console.log("[console error]", m.text()); } });
  page.on("pageerror", (e) => { errors++; console.log("[pageerror]", e.message); });
  await page.evaluateOnNewDocument((noGL, storage) => {
    try { for (const [k, v] of Object.entries(storage)) if (localStorage.getItem("__crt_seeded") === null) localStorage.setItem(k, v); localStorage.setItem("__crt_seeded", "1"); } catch { /* ignore */ }
    if (noGL) {
      const get = HTMLCanvasElement.prototype.getContext;
      (HTMLCanvasElement.prototype as any).getContext = function (type: string, ...a: any[]) { return /webgl/.test(type) ? null : (get as any).call(this, type, ...a); };
    }
  }, !!o.noGL, o.storage ?? {});
  await page.goto(`http://localhost:${port}/${o.query ?? ""}`, { waitUntil: "load" });
  await page.waitForFunction(() => (window as any).snatcherDisplay, { timeout: 15000 });
  await page.evaluate(() => document.fonts.ready);
  await sleep(200);
  return { page, cdp };
}
const dstate = (page: Page) => page.evaluate(() => {
  const d = (window as any).snatcherDisplay, c = document.getElementById("screen")!.getBoundingClientRect(), g = document.getElementById("crt") as HTMLCanvasElement;
  return {
    preset: d.preset as string, params: d.params, unavailable: d.unavailable as boolean, boxCrt: d.boxCrt as boolean, active: d.active as boolean, lost: !!d.filter?.lost, hasFilter: !!d.filter,
    rect: { x: c.left, y: c.top, w: c.width, h: c.height }, dpr: devicePixelRatio, glHidden: g.hidden, glW: g.width, glH: g.height,
    glRect: (() => { const r = g.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; })(),
    view: { ...(window as any).__crtmath.view },
    note: (document.getElementById("crtNote") as HTMLElement).hidden ? "" : document.getElementById("crtNote")!.textContent,
  };
});

// ============================================================ A. no game data needed
if (PART.includes("A")) {
{
  // A1: the shaders build and the filter owns the picture
  const { page } = await open({ query: "?crt=tv" });
  const s = await dstate(page);
  check("shaders compile: the filter exists and software GL was accepted for ?crt=tv", s.hasFilter && !s.unavailable && !s.lost);
  check("before a frame arrives the 2D canvas stays visible (GL canvas hidden)", s.glHidden);
  check("preset forced by the URL, nothing saved", s.preset === "tv" && (await page.evaluate(() => localStorage.getItem("snatcher.crt"))) === null);
  check("the picture box is the 8:7 aspect and a whole number of device pixels", Math.abs(s.rect.w / s.rect.h - (256 * 8) / 7 / 224) < 0.01 && Math.abs(s.rect.w * s.dpr - Math.round(s.rect.w * s.dpr)) < 1e-6 && Math.abs(s.rect.h * s.dpr - Math.round(s.rect.h * s.dpr)) < 1e-6, JSON.stringify(s.rect));
  check("GL canvas backing size equals the box in device pixels", s.glW === Math.round(s.rect.w * s.dpr) && s.glH === Math.round(s.rect.h * s.dpr), `${s.glW}x${s.glH}`);
  check("the pointer view follows (8:7, flat)", s.view.par > 1.14 && s.view.par < 1.15 && s.view.curve === 0, JSON.stringify(s.view));

  // A2: every preset draws; synthetic frames: flat grey and a marker
  const lum = await page.evaluate(async () => {
    const d = (window as any).snatcherDisplay;
    const f = d.filter;
    const W = 256, H = 224, fr = new Uint8Array(W * H * 4);
    const lin = (v: number) => { const c = v / 255; return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4); };
    const res: Record<string, { mean: number; min: number; max: number }> = {};
    for (const p of ["off", "tv", "pvm", "scan"]) {
      if (p === "off") continue;
      d.setPreset(p);
      for (let i = 0; i < fr.length; i += 4) { fr[i] = fr[i + 1] = fr[i + 2] = 128; fr[i + 3] = 255; }
      d.present(fr);
      const px = f.readPixels(); let s = 0, mn = 1, mx = 0;
      for (let i = 0; i < px.length; i += 4) { const y = lin(px[i]) * 0.2126 + lin(px[i + 1]) * 0.7152 + lin(px[i + 2]) * 0.0722; s += y; mn = Math.min(mn, y); mx = Math.max(mx, y); }
      res[p] = { mean: s / (px.length / 4), min: mn, max: mx };
    }
    return { res, want: lin(128) };
  });
  for (const [p, r] of Object.entries(lum.res)) check(`preset ${p}: a flat grey frame keeps its brightness (${r.mean.toFixed(3)} vs ${lum.want.toFixed(3)})`, r.mean > lum.want * 0.9 && r.mean < lum.want * 1.1 && r.max < 1 && r.mean > 0.05);

  // A3: the warp in the shader and the pointer mapping agree: a marker pixel appears where gameToView says
  const warp = await page.evaluate(async () => {
    const d = (window as any).snatcherDisplay, f = d.filter;
    const W = 256, H = 224;
    const rows: { shape: string; curve: number; gx: number; gy: number; ex: number; ey: number; ax: number; ay: number }[] = [];
    for (const shape of ["tv", "square"]) for (const curve of [0, 0.6, 1]) {
      d.setParams({ shape, curve, scan: 0, mask: 0, glow: 0, composite: 0, sharp: 1 });
      await new Promise((r) => setTimeout(r, 120));
      const rect = document.getElementById("screen")!.getBoundingClientRect();
      const view = { par: shape === "tv" ? 8 / 7 : 1, curve };
      const mod = (window as any).__crtmath;
      for (const [gx, gy] of [[128, 112], [10, 10], [245, 12], [12, 212], [246, 213], [128, 8], [4, 112]]) {
        const fr = new Uint8Array(W * H * 4); for (let i = 3; i < fr.length; i += 4) fr[i] = 255;
        for (let y = gy - 1; y <= gy + 1; y++) for (let x = gx - 1; x <= gx + 1; x++) { const i = (y * W + x) * 4; fr[i] = fr[i + 1] = fr[i + 2] = 255; }
        d.present(fr);
        const px = f.readPixels(), { w, h } = f.size;
        let sx = 0, sy = 0, sw = 0;
        for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) { const v = px[(y * w + x) * 4 + 1]; if (v > 8) { sx += (x + 0.5) * v; sy += (y + 0.5) * v; sw += v; } }
        const c = mod.gameToView(gx + 0.5, gy + 0.5, { x: 0, y: 0, w: rect.width, h: rect.height }, 256, 224, view);
        rows.push({ shape, curve, gx, gy, ex: c.x * devicePixelRatio, ey: c.y * devicePixelRatio, ax: sx / sw, ay: sy / sw });
      }
    }
    return rows;
  });
  const err = (r: (typeof warp)[number]) => { const e = Math.hypot(r.ex - r.ax, r.ey - r.ay); return Number.isNaN(e) ? 9999 : e; };
  const worstRow = warp.reduce((a, r) => (err(r) > err(a) ? r : a), warp[0]);
  const worst = err(worstRow);
  check("a marker drawn by the shader sits where the pointer mapping says (all shapes, curvature 0 / 0.6 / 1, corners and edges)", worst < 1.6, `worst error ${worst.toFixed(2)} device px over ${warp.length} points ${worst >= 1.6 ? JSON.stringify(worstRow) : ""}`);
  await closeP(page);
}

// A4: line evenness at awkward scales, mask lined up with device pixels, through the real pipeline (flat frames, canvas resized by hand)
{
  const { page } = await open({ query: "?crt=tv" });
  const r = await page.evaluate(() => {
    const d = (window as any).snatcherDisplay, f = d.filter, m = (window as any).__crtmath;
    const lin = (v: number) => { const c = v / 255; return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4); };
    const out: any[] = [];
    const frame = (g: number) => { const fr = new Uint8Array(256 * 224 * 4); for (let i = 0; i < fr.length; i += 4) { fr[i] = g; fr[i + 1] = g; fr[i + 2] = g; fr[i + 3] = 255; } return fr; };
    const draw = (scale: number, par: number, g = 100) => { const { devW, devH } = m.pictureSize(scale, par); f.setSize(devW, devH); f.setParams(d.params); f.upload(frame(g)); f.draw(devH / 224); return { px: f.readPixels(), w: devW, h: devH }; };
    for (const preset of ["tv", "pvm"]) {
      d.setPreset(preset); const par = 8 / 7;
      for (const scale of [2.7, 3.35, 4.82, 6.7]) {
        const { px, w, h } = draw(scale, par);
        const row: number[] = []; for (let y = 0; y < h; y++) { let s = 0; for (let x = w >> 2; x < (w * 3) >> 2; x++) { const i = (y * w + x) * 4; s += lin(px[i + 1]); } row.push(s / (w >> 1)); }
        const s = h / 224, energy: number[] = [], peak: number[] = [];
        for (let l = 20; l < 200; l++) { const y0 = l * s, y1 = (l + 1) * s; let tot = 0, mx = 0; for (let y = Math.floor(y0); y < Math.ceil(y1); y++) { const wgt = Math.min(y + 1, y1) - Math.max(y, y0); if (wgt > 0) { tot += row[y] * wgt; mx = Math.max(mx, row[y]); } } energy.push(tot / s); peak.push(mx); }
        const ratio = (a: number[]) => Math.min(...a) / Math.max(...a);
        out.push({ kind: "even", preset, scale, energy: ratio(energy), peak: ratio(peak) });
      }
    }
    // mask: periodic in whole device pixels (pvm aperture grille, no scanlines; scales 4.82 / 6.7 / 12, and a phone sized picture where it is faded out)
    d.setPreset("pvm");
    d.setParams({ mask: 0.5, scan: 0, glow: 0, composite: 0 });
    for (const scale of [4.82, 6.7, 12, 3.4]) {
      const { px, w, h } = draw(scale, 8 / 7, 90);
      const ml = m.maskLayout(h / 224, "aperture");
      const y = h >> 1; let diff = 0, varies = 0;
      for (let x = 20; x < w - 20 - ml.pitch; x++) { const i = (y * w + x) * 4, j = (y * w + x + ml.pitch) * 4; diff += Math.abs(px[i] - px[j]) + Math.abs(px[i + 1] - px[j + 1]) + Math.abs(px[i + 2] - px[j + 2]); varies += Math.abs(px[i] - px[i + 4]) + Math.abs(px[i + 1] - px[i + 5]); }
      out.push({ kind: "mask", scale, pitch: ml.pitch, periodDiff: diff, varies, fade: ml.fade });
    }
    return out;
  });
  for (const e of r.filter((x: any) => x.kind === "even")) check(`even scanlines, ${e.preset} at ${e.scale}x: line energy min/max ${e.energy.toFixed(3)}, peak min/max ${e.peak.toFixed(3)}`, e.energy > 0.94 && e.peak > 0.8);
  for (const m of r.filter((x: any) => x.kind === "mask")) {
    if (m.fade > 0) check(`mask at ${m.scale}x: exactly periodic with ${m.pitch} whole device pixels, and visible (${m.varies} variation)`, m.periodDiff === 0 && m.varies > 500);
    else check(`mask at ${m.scale}x (phone sized): faded out, no stripes (${m.varies} variation)`, m.varies === 0);
  }
  await closeP(page);
}

// A5: settings: defaults, persistence, reset, Clear everything
{
  const a = await open({ query: "" });
  let s = await dstate(a.page);
  check("default for a computer: consumer TV (software GL declines it: the plain picture and a note)", s.preset === "tv" && !s.hasFilter && s.unavailable, `preset ${s.preset}, filter ${s.hasFilter}, note ${s.note}`);
  await closeP(a.page);
  // a user choice survives a reload; sliders make it custom
  const b = await open({ query: "?crtsoft=1" });
  await b.page.evaluate(() => { const sel = document.getElementById("crtPreset") as HTMLSelectElement; sel.value = "pvm"; sel.dispatchEvent(new Event("change")); });
  s = await dstate(b.page);
  check("choosing a style in the Display tab applies it and saves it", s.preset === "pvm" && (await b.page.evaluate(() => localStorage.getItem("snatcher.crt"))) === '{"preset":"pvm"}');
  await b.page.evaluate(() => {
    const e = document.getElementById("crtGlow") as HTMLInputElement; e.value = "80"; e.dispatchEvent(new Event("input"));
    const k = document.getElementById("crtCurve") as HTMLInputElement; k.value = "30"; k.dispatchEvent(new Event("input"));
    const sh = document.getElementById("crtShape") as HTMLSelectElement; sh.value = "square"; sh.dispatchEvent(new Event("change"));
  });
  s = await dstate(b.page);
  check("a slider makes the style custom and saves every value", s.preset === "custom" && Math.abs(s.params.glow - 0.8) < 1e-9 && Math.abs(s.params.curve - 0.3) < 1e-9 && s.params.shape === "square");
  check("the pointer view follows curvature and pixel shape", Math.abs(s.view.curve - 0.3) < 1e-9 && s.view.par === 1, JSON.stringify(s.view));
  const saved = await b.page.evaluate(() => localStorage.getItem("snatcher.crt"));
  await b.page.reload({ waitUntil: "load" }); await b.page.waitForFunction(() => (window as any).snatcherDisplay); await sleep(300);
  s = await dstate(b.page);
  check("custom settings come back after a reload", s.preset === "custom" && Math.abs(s.params.glow - 0.8) < 1e-9 && s.params.shape === "square" && (await b.page.evaluate(() => localStorage.getItem("snatcher.crt"))) === saved, String(saved).slice(0, 140));
  check("the Display tab shows the saved values", await b.page.evaluate(() => (document.getElementById("crtPreset") as HTMLSelectElement).value === "custom" && (document.getElementById("crtGlow") as HTMLInputElement).value === "80" && document.getElementById("crtGlowO")!.textContent === "80%"));
  await b.page.evaluate(() => (document.getElementById("crtReset") as HTMLElement).click());
  s = await dstate(b.page);
  check("Reset returns to the default style for this device", s.preset === "tv" && s.params.curve === 0 && s.params.shape === "tv");
  await b.page.evaluate(() => localStorage.setItem("snatcher.crt", "{broken"));
  await b.page.reload({ waitUntil: "load" }); await b.page.waitForFunction(() => (window as any).snatcherDisplay);
  check("a damaged saved value is ignored", (await dstate(b.page)).preset === "tv");
  // Clear everything removes the key (the wipe button asks twice)
  await b.page.evaluate(() => { const sel = document.getElementById("crtPreset") as HTMLSelectElement; sel.value = "scan"; sel.dispatchEvent(new Event("change")); });
  check("the style is saved before the wipe", (await b.page.evaluate(() => localStorage.getItem("snatcher.crt"))) === '{"preset":"scan"}');
  await b.page.evaluate(() => { const w = document.getElementById("btnWipe") as HTMLElement; w.click(); w.click(); });
  await b.page.waitForFunction(() => localStorage.getItem("snatcher.crt") === null, { timeout: 15000 }).then(() => check("Clear everything removes snatcher.crt", true), () => check("Clear everything removes snatcher.crt", false));
  await closeP(b.page);
  // a phone defaults to scanlines only
  const c = await open({ mobile: true, query: "" });
  s = await dstate(c.page);
  check("default for a phone: scanlines only", s.preset === "scan", s.preset);
  await closeP(c.page);
}

// A6: no WebGL at all, and a context that is lost and comes back
{
  const a = await open({ noGL: true, query: "?crt=tv" });
  const s = await dstate(a.page);
  check("WebGL missing: the plain picture, GL canvas hidden, a note in the Display tab", s.unavailable && s.glHidden && !s.boxCrt && s.note.length > 10 && s.view.par === 1, s.note);
  await closeP(a.page);
  const b = await open({ query: "?crt=tv" });
  await b.page.evaluate(() => { const d = (window as any).snatcherDisplay; d.present(new Uint8Array(256 * 224 * 4).fill(200)); });
  let s2 = await dstate(b.page);
  check("with a frame the GL canvas is shown over the 2D canvas, same box", s2.active && !s2.glHidden && Math.abs(s2.glRect.x - s2.rect.x) < 0.01 && Math.abs(s2.glRect.w - s2.rect.w) < 0.01 && Math.abs(s2.glRect.h - s2.rect.h) < 0.01);
  check("the GL canvas does not catch pointer events", await b.page.evaluate(() => getComputedStyle(document.getElementById("crt")!).pointerEvents === "none"));
  await b.page.evaluate(() => { const e = (window as any).snatcherDisplay.filter.gl.getExtension("WEBGL_lose_context"); (window as any).__lose = e; e.loseContext(); });
  await sleep(300);
  s2 = await dstate(b.page);
  check("a lost context falls back to the plain picture without errors", s2.lost && s2.glHidden && !s2.boxCrt && s2.view.par === 1);
  await b.page.evaluate(() => (window as any).__lose.restoreContext());
  await sleep(800);
  s2 = await dstate(b.page);
  check("a restored context brings the filter back", !s2.lost && s2.boxCrt && s2.view.par > 1.14, JSON.stringify({ lost: s2.lost, box: s2.boxCrt }));
  await closeP(b.page);
}

// A7: the picture follows the window and the device pixel ratio, always in whole device pixels
{
  const { page, cdp } = await open({ query: "?crt=tv", w: 1280, h: 720 });
  for (const [w, h, dpr] of [[1280, 720, 1], [1280, 720, 1.5], [1280, 720, 2], [1100, 640, 1.25], [900, 600, 2.625], [1920, 1080, 1], [800, 500, 1]] as const) {
    await cdp.send("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: dpr, mobile: false });
    await sleep(900);   // the page looks at devicePixelRatio twice a second (not every browser reports a change)
    const s = await dstate(page);
    const whole = Math.abs(s.rect.w * dpr - Math.round(s.rect.w * dpr)) < 0.05 && Math.abs(s.rect.h * dpr - Math.round(s.rect.h * dpr)) < 0.05;   // layout units are 1/64 css px (0.04 device px at DPR 2.625)
    check(`${w}x${h} at DPR ${dpr}: box ${s.rect.w.toFixed(2)}x${s.rect.h.toFixed(2)} css, whole device pixels, GL canvas ${s.glW}x${s.glH}`, whole && s.rect.w <= w && s.rect.h <= h && (s.boxCrt ? s.glW === Math.round(s.rect.w * dpr) && s.glH === Math.round(s.rect.h * dpr) : (s.rect.h * dpr) / 224 < 2.01 && s.glHidden));
  }
  await closeP(page);
}
}
// ============================================================ B. with the game (assets/raw)
const peek = (page: Page, a: number) => page.evaluate((x) => (window as any).snatcher.game.peek_reg(x) as number, a);
const ui = (page: Page) => page.evaluate(() => JSON.parse((window as any).snatcher.game.touch_ui()));
const frameNo = (page: Page) => page.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()).frame as number);
const waitFrames = async (page: Page, n: number) => { const f = await frameNo(page); await page.waitForFunction((t) => JSON.parse((window as any).snatcher.game.debug_info()).frame >= t, { timeout: 120000, polling: 50 }, f + n); };
async function tap(cdp: CDPSession, x: number, y: number) {
  await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x, y, id: 1 }] });
  await sleep(60);
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
}
if (!PART.includes("B")) { /* part B not asked for */ }
else if (!haveDisc) console.log("SKIP part B: assets/raw not present (the dev server needs the local extraction)");
else {
  // B1: the engine frame is untouched, and every style keeps the brightness of the picture
  {
    const { page } = await open({ w: 760, h: 600, query: "?scene=SP06:0&crt=tv" });   // a small window: software GL is slow
    await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
    await waitFrames(page, 240);
    const s = await dstate(page);
    check("in game: the filter is on screen over the 2D canvas, same box, whole device pixels", s.active && !s.glHidden && Math.abs(s.glRect.w - s.rect.w) < 0.01 && s.glW === Math.round(s.rect.w * s.dpr), JSON.stringify(s.rect));
    const same = await page.evaluate(() => {
      const sn = (window as any).snatcher, mem = new Uint8Array(sn.memory.buffer, sn.game.framebuffer_ptr(), 256 * 224 * 4);
      const c = (document.getElementById("screen") as HTMLCanvasElement).getContext("2d")!.getImageData(0, 0, 256, 224).data;
      let diff = 0; for (let i = 0; i < mem.length; i++) if (mem[i] !== c[i]) diff++;
      return diff;
    });
    check("the engine's frame and the 2D canvas are identical with the filter on (display only)", same === 0, `${same} bytes differ`);
    const lum = await page.evaluate(() => {
      const d = (window as any).snatcherDisplay, f = d.filter, sn = (window as any).snatcher;
      const lin = (v: number) => { const c = v / 255; return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4); };
      const mean = (a: Uint8Array | Uint8ClampedArray) => { let s = 0; for (let i = 0; i < a.length; i += 4) s += lin(a[i]) * 0.2126 + lin(a[i + 1]) * 0.7152 + lin(a[i + 2]) * 0.0722; return s / (a.length / 4); };
      const base = mean(new Uint8Array(sn.memory.buffer, sn.game.framebuffer_ptr(), 256 * 224 * 4));
      const res: Record<string, number> = {};
      for (const p of ["tv", "pvm", "scan"]) { d.setPreset(p); d.redraw(); res[p] = mean(f.readPixels()); }
      return { base, res };
    });
    for (const [p, v] of Object.entries(lum.res)) check(`style ${p}: not black, brightness ${(v / lum.base).toFixed(2)} of the plain picture (${v.toFixed(4)} vs ${lum.base.toFixed(4)})`, v > 0.01 && v / lum.base > 0.88 && v / lum.base < 1.12);
    await page.screenshot({ path: join(out, "page-desktop-tv.png") });
    await closeP(page);
  }

  // B2: the light gun with a mouse: every combination of pixel shape and curvature lands on the right game pixel
  {
    const { page } = await open({ w: 760, h: 600, query: "?scene=SP06:0&crt=tv" });
    await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
    await waitFrames(page, 60);
    let bad = 0, n = 0, detail = "";
    for (const shape of ["tv", "square"]) for (const curve of [0, 0.5, 1]) {
      await page.evaluate((shape, curve) => (window as any).snatcherDisplay.setParams({ shape, curve }), shape, curve);
      await sleep(250);
      // the browser rounds a mouse position to whole CSS pixels: the points stay a little inside the corners (the shader test in part A checks the extreme corners in device pixels)
      for (const [gx, gy] of [[128, 112], [5, 5], [250, 6], [6, 217], [249, 218], [128, 6], [4, 112], [200, 60], [60, 160]]) {
        const p = await page.evaluate((gx, gy) => {
          const m = (window as any).__crtmath, r = document.getElementById("screen")!.getBoundingClientRect();
          return m.gameToView(gx + 0.5, gy + 0.5, { x: r.left, y: r.top, w: r.width, h: r.height }, 256, 224, { ...m.view });
        }, gx, gy);
        await page.mouse.move(p.x, p.y);
        const g = await page.evaluate(() => { const i = (window as any).snatcher.input; return [i.mouseX, i.mouseY]; });
        n++; if (g[0] !== gx || g[1] !== gy) { bad++; detail += ` ${shape}/${curve}: wanted ${gx},${gy} got ${g}`; }
      }
    }
    check("mouse light gun: game pixel under the pointer is exact for 8:7 and square, curvature 0 / 0.5 / 1", bad === 0, `${n - bad}/${n}${detail.slice(0, 200)}`);
    // the gun position the engine sees follows (set_gun gets the same pixel on the next tick)
    await page.evaluate(() => { const i = (window as any).snatcher.input; i.gunMode = true; });
    await page.evaluate(() => (window as any).snatcherDisplay.setParams({ shape: "tv", curve: 0.5 }));
    await sleep(250);
    const p = await page.evaluate(() => { const m = (window as any).__crtmath, r = document.getElementById("screen")!.getBoundingClientRect(); return m.gameToView(200.5, 100.5, { x: r.left, y: r.top, w: r.width, h: r.height }, 256, 224, { ...m.view }); });
    await page.mouse.move(p.x, p.y); await waitFrames(page, 3);
    const gun = await page.evaluate(() => { const ix = (window as any).snatcher.game; return JSON.parse(ix.debug_info()); });
    void gun;
    await closeP(page);
  }

  // B3: fingers: touch the game with the filter on (phone, 8:7 aspect, with and without curvature)
  {
    const { page, cdp } = await open({ mobile: true, w: 360, h: 640, dpr: 2, query: "?scene=SP01:0&crt=scan", storage: { "snatcher.touch.mode": "game" } });
    await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
    const s0 = await dstate(page);
    check("phone: the filter lays the picture out (8:7, whole device pixels, inside the screen)", s0.boxCrt && s0.active !== undefined && Math.abs(s0.rect.w / s0.rect.h - (256 * 8) / 7 / 224) < 0.01 && s0.rect.w <= 360 && Math.abs(s0.rect.w * s0.dpr - Math.round(s0.rect.w * s0.dpr)) < 0.02, JSON.stringify(s0.rect));
    check("phone: no horizontal scroll", await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    const centre = async () => { const r = (await dstate(page)).rect; return { x: r.x + r.w / 2, y: r.y + r.h / 2 }; };
    for (let i = 0; i < 400; i++) { const u = await ui(page); if (u.kind === "menu" && u.ready) break; if (u.kind === "message" && u.ready) { const c = await centre(); await tap(cdp, c.x, c.y); await sleep(500); } else await sleep(100); }
    // back to the top menu: the Back button (a page button, shown while a sub menu is open) is the cancel button
    const toTop = async () => { for (let i = 0; i < 80; i++) { const u = await ui(page); if (u.kind === "menu" && u.ready && !u.back) return; if (u.kind === "menu" && u.ready && u.back) { await page.$eval("#gBack", (e) => (e as HTMLElement).click()); await sleep(500); } else await sleep(150); } };
    for (const curve of [0, 0.5]) {
      await page.evaluate((c) => (window as any).snatcherDisplay.setParams({ curve: c }), curve);
      await sleep(400);
      // come back to the top menu (the previous tap may have opened a sub menu)
      await toTop();
      const u = await ui(page);
      const lines = u.rects.filter((r: any) => r.k === 0);
      let okAll = true, info = "";
      for (const line of lines.slice(0, 3)) {
        const p = await page.evaluate((x, y) => { const m = (window as any).__crtmath, r = document.getElementById("screen")!.getBoundingClientRect(); return m.gameToView(x, y, { x: r.left, y: r.top, w: r.width, h: r.height }, 256, 224, { ...m.view }); }, line.x + line.w / 2, line.y + line.h / 2);
        await tap(cdp, p.x, p.y);
        await sleep(900);
        const rem = (await peek(page, 0x9716)) >> 8;
        if (rem !== line.id) { okAll = false; info += ` tapped line ${line.id} remembered ${rem}`; }
        // back out for the next line
        await toTop();
      }
      check(`touch the game: a tap on a menu line selects that line (phone, 8:7, curvature ${curve})`, okAll && lines.length >= 3, info);
    }
    await page.screenshot({ path: join(out, "page-phone-scan.png") });
    await closeP(page);
  }

  // B4: WebGL missing while the game runs
  {
    const { page } = await open({ noGL: true, query: "?scene=SP06:0&crt=tv" });
    await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
    await waitFrames(page, 120);
    const s = await dstate(page);
    check("no WebGL: the game runs on the plain canvas (no errors, GL hidden, flat pointer view)", s.unavailable && s.glHidden && s.view.par === 1 && s.view.curve === 0);
    const lit = await page.evaluate(() => { const d = (document.getElementById("screen") as HTMLCanvasElement).getContext("2d")!.getImageData(0, 0, 256, 224).data; let n = 0; for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] > 60) n++; return n; });
    check("no WebGL: the 2D canvas shows the game", lit > 2000, `${lit} lit pixels`);
    await closeP(page);
  }
}

console.log(`\n${failed ? "FAILED: " + failed + " check(s)" : "all checks passed"}; console/page errors: ${errors}`);
await browser.close();
process.exit(failed || errors ? 1 : 0);
