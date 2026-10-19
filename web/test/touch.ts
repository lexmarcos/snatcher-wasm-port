// Touch input in headless Chrome with device emulation (phones, tablets), real touch events through the DevTools protocol (multi-touch).
//   bun web/test/touch.ts [shots-dir]       default work/shots/mobile      (screenshots show the game: gitignored, never commit them)
// Needs the local extraction (assets/raw: the dev server serves it); skips cleanly without it. Own server on TOUCH_PORT (default 5360).
// Checks: layouts (no horizontal scroll, controls inside the safe area, picture and controls apart) for 7 device shapes in both modes, the on-screen
// pad (multi-touch, sliding, cancel, hidden page, menu, prompts), touch the game (a tap on a command line, on a message, on a paged menu's arrow,
// the keyboard, a shooting stage: the shot lands where tapped), the settings (persist, default per device, desktop unchanged), the import page on a phone,
// wake lock and haptics (stubs), audio unlock. Exit code 1 on any failed check or console error.
import puppeteer, { type Browser, type BrowserContext, type CDPSession, type Page } from "puppeteer-core";
import { existsSync, mkdirSync, readdirSync } from "fs";
import { join } from "path";
import { homedir } from "os";
import { gameToClient } from "../src/touchmath";

const root = join(import.meta.dir, "..", "..");
const out = process.argv[2] ?? join(root, "work/shots/mobile");
mkdirSync(out, { recursive: true });
if (!existsSync(join(root, "assets/raw/filelist.tsv"))) { console.log("SKIP: assets/raw not present (the dev server needs the local extraction)"); process.exit(0); }
const port = Number(process.env.TOUCH_PORT ?? 5360);
let failed = 0, errors = 0;
const check = (name: string, ok: boolean, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"} ${name}${extra ? " – " + extra : ""}`); if (!ok) failed++; };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

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

const IPHONE = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
const ANDROID = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Mobile Safari/537.36";
const IPAD = "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
interface Dev { name: string; w: number; h: number; dpr: number; ua: string; safe: { top: number; bottom: number; left: number; right: number } }
const DEVICES: Dev[] = [
  { name: "iphone-portrait", w: 390, h: 844, dpr: 3, ua: IPHONE, safe: { top: 47, bottom: 34, left: 0, right: 0 } },
  { name: "iphone-landscape", w: 844, h: 390, dpr: 3, ua: IPHONE, safe: { top: 0, bottom: 21, left: 47, right: 47 } },
  { name: "android-portrait", w: 412, h: 915, dpr: 2.625, ua: ANDROID, safe: { top: 24, bottom: 0, left: 0, right: 0 } },
  { name: "android-landscape", w: 915, h: 412, dpr: 2.625, ua: ANDROID, safe: { top: 0, bottom: 0, left: 0, right: 0 } },
  { name: "ipad-portrait", w: 768, h: 1024, dpr: 2, ua: IPAD, safe: { top: 24, bottom: 20, left: 0, right: 0 } },
  { name: "ipad-landscape", w: 1024, h: 768, dpr: 2, ua: IPAD, safe: { top: 24, bottom: 20, left: 0, right: 0 } },
  { name: "small-landscape", w: 667, h: 375, dpr: 2, ua: IPHONE, safe: { top: 0, bottom: 0, left: 0, right: 0 } },
];
const PHONE360: Dev = { name: "phone-360", w: 360, h: 640, dpr: 3, ua: ANDROID, safe: { top: 0, bottom: 0, left: 0, right: 0 } };

interface Opts { mode?: string; query?: string; stubs?: boolean; keepMode?: boolean; ctx?: BrowserContext }
let safeInsetsWork: boolean | null = null;
async function open(d: Dev, o: Opts = {}): Promise<{ page: Page; cdp: CDPSession; ctx: BrowserContext }> {
  const ctx = o.ctx ?? (await browser!.createBrowserContext()); // every page its own storage: a mode saved by one test never leaks into the next
  const page = await ctx.newPage();
  await page.emulate({ viewport: { width: d.w, height: d.h, deviceScaleFactor: d.dpr, isMobile: true, hasTouch: true }, userAgent: d.ua });
  const cdp = await page.createCDPSession();
  try { await cdp.send("Emulation.setSafeAreaInsetsOverride" as any, { insets: d.safe }); if (safeInsetsWork === null) safeInsetsWork = true; } catch { safeInsetsWork = false; }
  page.on("console", (m) => { if (m.type() === "error" && !/404/.test(m.text())) { errors++; console.log("[console error]", m.text()); } });
  page.on("pageerror", (e) => { errors++; console.log("[pageerror]", e.message); });
  await page.evaluateOnNewDocument((mode, keep, stubs) => {
    try { if (!keep) localStorage.setItem("snatcher.touch.mode", mode); } catch { /* ignore */ }
    if (stubs) {
      const w: any = window; w.__vib = []; w.__wake = { acquired: 0, released: 0, active: 0 };
      (navigator as any).vibrate = (ms: number) => { w.__vib.push(ms); return true; };
      try { (navigator.storage as any).estimate = async () => ({ quota: 600 * 2 ** 20, usage: 20 * 2 ** 20 }); } catch { /* ignore */ } // a phone with little room
      Object.defineProperty(navigator, "wakeLock", { configurable: true, value: { request: async () => { w.__wake.acquired++; w.__wake.active++; const l = new EventTarget() as any; l.released = false; l.release = async () => { if (!l.released) { l.released = true; w.__wake.released++; w.__wake.active--; } }; return l; } } });
    }
  }, o.mode ?? "pad", !!o.keepMode, !!o.stubs);
  await page.goto(`http://localhost:${port}/${o.query ?? ""}`, { waitUntil: "load" });
  await page.evaluate(() => document.fonts.ready);
  return { page, cdp, ctx };
}

/** Real multi-touch through the protocol: every call sends the full list of fingers that are down. */
class Fingers {
  private down = new Map<number, { x: number; y: number }>();
  constructor(private cdp: CDPSession) {}
  private send(type: string) { return this.cdp.send("Input.dispatchTouchEvent", { type: type as any, touchPoints: [...this.down].map(([id, p]) => ({ x: p.x, y: p.y, id })) }); }
  async start(id: number, x: number, y: number) { this.down.set(id, { x, y }); await this.send("touchStart"); }
  async move(id: number, x: number, y: number) { this.down.set(id, { x, y }); await this.send("touchMove"); await sleep(50); /* pointermove is aligned to the next frame */ }
  async end(id: number) { this.down.delete(id); await this.send("touchEnd"); }
  async cancel() { await this.cdp.send("Input.dispatchTouchEvent", { type: "touchCancel", touchPoints: [] }); this.down.clear(); await sleep(50); }
  async tap(x: number, y: number, ms = 70) { await this.start(99, x, y); await sleep(ms); await this.end(99); }
}

const centre = (page: Page, sel: string) => page.$eval(sel, (e) => { const r = e.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2, w: r.width, h: r.height }; });
const rectOf = (page: Page, sel: string) => page.$eval(sel, (e) => { const r = e.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; });
const ui = (page: Page) => page.evaluate(() => JSON.parse((window as any).snatcher.game.touch_ui()));
const frame = (page: Page) => page.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()).frame as number);
const reg = (page: Page, a: number) => page.evaluate((x) => (window as any).snatcher.game.peek_reg(x) as number, a);
const held = (page: Page) => page.evaluate(() => (window as any).snatcher.touch.pad.held as number);
const waitSnatcher = (page: Page) => page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
async function waitUi(page: Page, pred: (u: any) => boolean, ms = 30000, what = ""): Promise<any> {
  const t0 = Date.now();
  for (;;) {
    const u = await ui(page);
    if (pred(u)) return u;
    if (Date.now() - t0 > ms) throw new Error(`timeout waiting for ${what}: ${JSON.stringify({ ...u, rects: u.rects.length })}`);
    await sleep(60);
  }
}
/** Press through messages (a tap on C of the pad, or on the picture) until the command menu is ready. */
async function toMenu(page: Page, f: Fingers, mode: string, ms = 60000): Promise<any> {
  const t0 = Date.now();
  for (;;) {
    const u = await ui(page);
    if (u.kind === "menu" && u.ready) return u;
    if (Date.now() - t0 > ms) throw new Error("no command menu: " + JSON.stringify({ ...u, rects: u.rects.length }));
    if (u.kind === "message" && u.ready) {
      const t = mode === "pad" ? await centre(page, '.pbtn[data-btn="32"]') : await centre(page, "#screen");
      await f.tap(t.x, t.y);
      await sleep(500);
    } else await sleep(100);
  }
}
const shot = (page: Page, name: string) => page.screenshot({ path: join(out, name) });
/** game pixel -> client coordinates of the canvas */
async function gameXY(page: Page, gx: number, gy: number) { const r = await rectOf(page, "#screen"); return gameToClient(gx, gy, r); }
const PAD = { UP: 1, DOWN: 2, LEFT: 4, RIGHT: 8, B: 16, C: 32, A: 64, START: 128 };

// ================================================================= A. layouts: both modes, seven device shapes
for (const mode of process.env.SKIP_LAYOUT ? [] : ["pad", "game"]) {
  for (const d of DEVICES) {
    const { page, cdp, ctx } = await open(d, { mode, query: "?scene=SP01:0" });
    await waitSnatcher(page);
    await toMenu(page, new Fingers(cdp), mode, 30000);
    await sleep(600);
    const r = await page.evaluate((mode) => {
      const rect = (s: string) => { const e = document.querySelector(s) as HTMLElement | null; if (!e || e.hidden || getComputedStyle(e).display === "none") return null; const b = e.getBoundingClientRect(); return b.width ? { x: b.left, y: b.top, w: b.width, h: b.height } : null; };
      const kids = Array.from(document.querySelectorAll<HTMLElement>("#touch .tbtn, #touch .pbtn, #touch .arm, #touch .dpad, #touch .abc")).filter((e) => e.getClientRects().length > 0 && e.getBoundingClientRect().width > 0 && getComputedStyle(e).visibility !== "hidden")  /* a fixed-position control has no offsetParent */.map((e) => { const b = e.getBoundingClientRect(); return { cls: e.className + (e.id ? "#" + e.id : ""), x: b.left, y: b.top, w: b.width, h: b.height, btn: e.matches(".tbtn, .pbtn, .arm") }; });
      const sai = getComputedStyle(document.getElementById("sai")!);
      return {
        scroll: document.documentElement.scrollWidth > innerWidth || document.body.scrollWidth > innerWidth || document.documentElement.scrollHeight > innerHeight,
        arr: document.documentElement.dataset.arr, touch: document.documentElement.dataset.touch,
        canvas: rect("#screen"), kids, menuUnder: document.documentElement.dataset.menu === "under", bg: getComputedStyle(document.body).backgroundColor, bgImg: getComputedStyle(document.body).backgroundImage, frame: ["screen", "crt"].map((id) => getComputedStyle(document.getElementById(id)!).boxShadow), iw: innerWidth, ih: innerHeight, safe: { t: parseFloat(sai.paddingTop), r: parseFloat(sai.paddingRight), b: parseFloat(sai.paddingBottom), l: parseFloat(sai.paddingLeft) }, mode,
      };
    }, mode);
    const tag = `${mode}/${d.name}`;
    check(`${tag}: no scrolling, touch layout on (${r.arr})`, !r.scroll && r.touch === mode && !!r.arr, JSON.stringify({ scroll: r.scroll, touch: r.touch }));
    const s = safeInsetsWork ? r.safe : { t: 0, r: 0, b: 0, l: 0 };
    const inside = r.kids.every((k: any) => k.x >= s.l - 0.5 && k.y >= s.t - 0.5 && k.x + k.w <= r.iw - s.r + 0.5 && k.y + k.h <= r.ih - s.b + 0.5);
    check(`${tag}: controls inside the safe area`, inside, JSON.stringify(r.kids.filter((k: any) => !(k.x >= s.l - 0.5 && k.y >= s.t - 0.5 && k.x + k.w <= r.iw - s.r + 0.5 && k.y + k.h <= r.ih - s.b + 0.5)).slice(0, 2)) + ` safe ${JSON.stringify(s)}`);
    const c = r.canvas!;
    const fullW = d.h > d.w && Math.min(d.w, d.h) < 600;   // a phone held upright: the picture spans the whole width; elsewhere a whole device-pixel scale
    check(`${tag}: picture inside the safe area, ${fullW ? "the whole width" : "whole device-pixel scale"}`, c.x >= s.l - 0.5 && c.y >= s.t - 0.5 && c.x + c.w <= r.iw - s.r + 0.5 && c.y + c.h <= r.ih - s.b + 0.5 && (fullW ? c.w > r.iw - s.l - s.r - 1 : (() => { const sc = (c.w * d.dpr) / 256; return sc < 1 || Math.abs(sc - Math.round(sc)) < 0.01; })()), JSON.stringify(c));
    const small = r.kids.filter((k: any) => k.btn && (Math.min(k.w, k.h) < 47.5));
    check(`${tag}: buttons are at least 48 px`, small.length === 0, JSON.stringify(small.slice(0, 2)));
    if (r.arr !== "overlay") {
      const hit = r.kids.filter((k: any) => k.x < c.x + c.w && k.x + k.w > c.x && k.y < c.y + c.h && k.y + k.h > c.y);
      check(`${tag}: controls and picture do not overlap (${r.arr})`, hit.length === 0, JSON.stringify(hit.slice(0, 2)));
    }
    check(`${tag}: a black page around the picture while playing`, r.bg === "rgb(0, 0, 0)" && r.bgImg === "none", `${r.bg} ${r.bgImg}`);
    check(`${tag}: no frame lines around the picture`, r.frame.every((v: string) => v === "none"), JSON.stringify(r.frame));
    if (r.arr === "below") {
      const m = r.kids.find((k: any) => /#tMenu$/.test(k.cls));
      const others = r.kids.filter((k: any) => k !== m && !/dpad|abc/.test(k.cls) || /arm/.test(k.cls));
      const clash = m ? others.filter((k: any) => k.x < m.x + m.w && k.x + k.w > m.x && k.y < m.y + m.h && k.y + k.h > m.y) : [];
      check(`${tag}: Menu centred just under the picture, clear of the other controls`, r.menuUnder && !!m && Math.abs(m.x + m.w / 2 - r.iw / 2) < 1 && m.y >= c.y + c.h - 0.5 && m.y - (c.y + c.h) < 24 && clash.length === 0, JSON.stringify({ m, c, clash: clash.slice(0, 2) }));
    }
    await shot(page, `${mode}-${d.name}.png`);
    await ctx.close();
  }
}
console.log(`safe-area insets emulation by the protocol: ${safeInsetsWork ? "works" : "NOT available (insets checked as 0)"}`);

// ================================================================= B. on-screen pad
{
  const d = DEVICES[0];
  const { page, cdp } = await open(d, { mode: "pad", query: "?scene=SP01:0", stubs: true });
  await waitSnatcher(page);
  const f = new Fingers(cdp);
  await toMenu(page, f, "pad", 30000);
  const right = await (async () => { const r = await rectOf(page, "#tDpad"); return { x: r.x + r.w * 5 / 6, y: r.y + r.h / 2, r }; })();
  const dp = await rectOf(page, "#tDpad");
  const arm = (dx: number, dy: number) => ({ x: dp.x + dp.w / 2 + dx * dp.w / 3, y: dp.y + dp.h / 2 + dy * dp.h / 3 });
  const btnC = await centre(page, '.pbtn[data-btn="32"]'), btnB = await centre(page, '.pbtn[data-btn="16"]'), btnA = await centre(page, '.pbtn[data-btn="64"]'), btnS = await centre(page, "#tStart");

  // multi-touch: D-pad right held with one finger, C pressed with another
  await f.start(1, right.x, right.y);
  check("D-pad right: RIGHT bit", (await held(page)) === PAD.RIGHT, String(await held(page)));
  await f.start(2, btnC.x, btnC.y);
  check("multi-touch: D-pad held while C is pressed", (await held(page)) === (PAD.RIGHT | PAD.C), String(await held(page)));
  const sw = await page.$eval("#touch", (e) => (e as HTMLElement).dataset.held);
  check("pad reports its held bits on the root (data-held)", sw === String(PAD.RIGHT | PAD.C), String(sw));
  // diagonals: finger 1 moves to the lower left of the pad centre
  const dl = arm(-0.9, 0.9);
  await f.move(1, dl.x, dl.y);
  check("diagonal: down + left", (await held(page)) === (PAD.DOWN | PAD.LEFT | PAD.C), String(await held(page)));
  const ul = arm(0.9, -0.9);
  await f.move(1, ul.x, ul.y);
  check("diagonal: up + right", (await held(page)) === (PAD.UP | PAD.RIGHT | PAD.C), String(await held(page)));
  // dead zone: back to the middle
  const mid = arm(0, 0);
  await f.move(1, mid.x, mid.y);
  check("dead zone in the middle presses nothing on the D-pad", (await held(page)) === PAD.C, String(await held(page)));
  // sliding a finger from one button to another: C -> B, then B -> A, then off the buttons
  await f.move(2, btnB.x, btnB.y);
  check("sliding from C to B: B pressed, C released", (await held(page)) === PAD.B, String(await held(page)));
  await f.move(2, btnA.x, btnA.y);
  check("sliding from B to A", (await held(page)) === PAD.A, String(await held(page)));
  await f.move(2, btnA.x, btnA.y - 120);
  check("sliding off every button releases it", (await held(page)) === 0, String(await held(page)));
  await f.move(2, btnS.x, btnS.y);
  check("sliding onto Start presses it", (await held(page)) === PAD.START, String(await held(page)));
  // held stays held: wait, still pressed; the engine sees it through Input.poll (same path as the keyboard)
  await sleep(300);
  check("a held button stays held", (await held(page)) === PAD.START);
  const seen = await page.evaluate(() => (window as any).snatcher.input.touch.bits());
  check("Input.touch.bits() is what Input.poll ORs into the pad", seen === PAD.START, String(seen));
  await f.end(2); await f.end(1);
  check("releasing anywhere releases", (await held(page)) === 0, String(await held(page)));
  check("the finger became the last used device; the game text keeps the A B C letters", await page.evaluate(() => (window as any).snatcher.input.device.kind === "touch" && (window as any).snatcher.prompt().style === 0));

  // no stuck input: touchcancel
  await f.start(1, right.x, right.y); await f.start(2, btnC.x, btnC.y);
  await f.cancel();
  check("touchcancel releases everything", (await held(page)) === 0, String(await held(page)));
  // hidden page
  await f.start(1, btnC.x, btnC.y);
  await page.evaluate(() => { Object.defineProperty(document, "hidden", { configurable: true, get: () => true }); document.dispatchEvent(new Event("visibilitychange")); });
  check("a hidden page releases everything", (await held(page)) === 0, String(await held(page)));
  await page.evaluate(() => { delete (document as any).hidden; document.dispatchEvent(new Event("visibilitychange")); });
  await f.end(1);
  // lost focus (blur)
  await f.start(1, btnC.x, btnC.y);
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  check("a lost window focus releases everything", (await held(page)) === 0);
  await f.end(1);
  // the menu: a finger is down, Menu opens (by a second finger), nothing stays pressed
  await f.start(1, btnC.x, btnC.y);
  await page.evaluate(() => (document.getElementById("tMenu") as HTMLElement).click());
  check("opening the menu releases the pad", (await held(page)) === 0 && (await page.evaluate(() => !document.getElementById("panel")!.hidden)));
  await f.end(1);
  await page.keyboard.press("Escape");
  await sleep(200);

  // the pad drives the game: D-pad down moves the debug menu's cursor, C chooses, Start counts as a button
  const u0 = await waitUi(page, (u) => u.kind === "menu" && u.ready, 5000, "menu");
  const dn = arm(0, 0.9);
  await f.start(1, dn.x, dn.y); await sleep(100); await f.end(1);
  const u1 = await waitUi(page, (u) => u.kind === "menu" && u.ready && u.cur !== u0.cur, 5000, "cursor to move");
  check("D-pad down on the screen moves the game's menu cursor", u1.cur === u0.cur + 1, `${u0.cur} -> ${u1.cur}`);
  const hap = await page.evaluate(() => (window as any).__vib.length);
  check("haptics are off by default (no vibration call)", hap === 0, String(hap));
  const wk = await page.evaluate(() => (window as any).__wake);
  check("wake lock taken while playing (touch device default)", wk.active === 1, JSON.stringify(wk));
  await page.keyboard.press("Escape"); await sleep(300);
  const wk2 = await page.evaluate(() => (window as any).__wake);
  check("wake lock released while the menu is open", wk2.active === 0, JSON.stringify(wk2));
  await page.keyboard.press("Escape"); await sleep(300);
  check("wake lock taken again after Resume", (await page.evaluate(() => (window as any).__wake.active)) === 1);
  const fonts = await page.evaluate(() => Array.from(document.querySelectorAll<HTMLElement>("#touch *, #rotateHint, #rotateHint *")).filter((e) => e.childNodes.length && Array.from(e.childNodes).some((n) => n.nodeType === 3 && (n.textContent ?? "").trim()) && e.getClientRects().length).map((e) => ({ t: (e.textContent ?? "").trim().slice(0, 12), fs: parseFloat(getComputedStyle(e).fontSize), fam: getComputedStyle(e).fontFamily.split(",")[0].replace(/["']/g, "") })));
  check("the pad's text follows the type scale (multiples of 16 px, DotGothic16)", fonts.length >= 4 && fonts.every((f: any) => f.fs % 16 === 0 && f.fam === "DotGothic16"), JSON.stringify(fonts.filter((f: any) => f.fs % 16 !== 0 || f.fam !== "DotGothic16")));
  await shot(page, "pad-iphone-portrait-menu.png");
  await page.close();
}

// ================================================================= C. touch the game
/** Wait for a menu state, pressing through the messages that come first (a tap on the picture). */
async function untilMenu(page: Page, f: Fingers, pred: (u: any) => boolean, what: string, ms = 30000): Promise<any> {
  const t0 = Date.now();
  for (;;) {
    const u = await ui(page);
    if (u.kind === "menu" && u.ready && pred(u)) return u;
    if (Date.now() - t0 > ms) throw new Error(`timeout waiting for ${what}: ${JSON.stringify({ ...u, rects: u.rects.length })}`);
    if (process.env.TDEBUG && Math.random() < 0.15) console.log("  untilMenu", u.kind, u.ready, JSON.stringify(await page.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()))));
    if (u.kind === "message") { const c = await centre(page, "#screen"); await f.tap(c.x, c.y, 120); await sleep(400); } else await sleep(80);
  }
}
/** Tap the menu line that holds `text` (flipping the page by tapping its arrow line first when needed). Returns the line tapped. */
async function tapMenuText(page: Page, f: Fingers, text: string): Promise<any> {
  for (let i = 0; i < 6; i++) {
    const u = await waitUi(page, (x) => x.kind === "menu" && x.ready, 20000, "a menu");
    const items: string[] = await page.evaluate(() => JSON.parse((window as any).snatcher.game.menu_items()));
    const vis = u.rects.filter((r: any) => r.k === 0);
    const first = u.page === 2 ? items.length - vis.length : 0;
    const idx = items.findIndex((t) => t.trim() === text);
    if (idx < 0) throw new Error(`no menu item "${text}" in ${JSON.stringify(items)}`);
    const r = idx >= first && idx < first + vis.length ? vis[idx - first] : u.rects.find((x: any) => (u.page === 1 ? x.k === 2 : x.k === 1));
    const p = await gameXY(page, r.x + r.w / 2, r.y + r.h / 2);
    await f.tap(p.x, p.y);
    if (r.k === 0) return { ...r, page: u.page };
    await waitUi(page, (x) => x.kind === "menu" && x.ready && x.page !== u.page, 8000, "page flip");
  }
  throw new Error("no line with " + text);
}
{
  const d = DEVICES[0];
  const { page, cdp } = await open(d, { mode: "game", query: "?scene=SP01:0", stubs: true });
  await waitSnatcher(page);
  const f = new Fingers(cdp);
  const u0 = await toMenu(page, f, "game", 30000);
  const names: string[] = await page.evaluate(() => JSON.parse((window as any).snatcher.game.menu_items()));
  check("the debug menu has a paged list (arrow lines) and items", u0.page > 0 && names.length >= 8 && u0.rects.some((r: any) => r.k === 2), JSON.stringify({ page: u0.page, n: names.length }));
  const arrow = u0.rects.find((r: any) => r.k === 2);
  const pa = await gameXY(page, arrow.x + arrow.w / 2, arrow.y + arrow.h / 2);
  await f.tap(pa.x, pa.y);
  const u1 = await waitUi(page, (u) => u.kind === "menu" && u.ready && u.page !== u0.page, 8000, "page flip");
  check("tapping the paging arrow flips the menu page", u1.page !== u0.page, `${u0.page} -> ${u1.page}`);
  await shot(page, "game-iphone-portrait-menu.png");
  // tap SHOOTING: the game opens its sub-menu (depth 1: Back shows) and remembers the cursor line that was tapped
  const line = await tapMenuText(page, f, "SHOOTING");
  const sub = await untilMenu(page, f, (u) => u.back, "the sub-menu"); void sub;
  const rem = await reg(page, 0x9716);
  check("a tap on a command line selects exactly that command (sub-menu opened; remembered cursor = the tapped line)", (rem >> 8) === line.id, `remembered ${rem >> 8}, tapped line ${line.id}`);
  // Back by a two-finger tap: the game takes it as the cancel button (the menu closes; this debug script then asks the same menu again)
  const closed = async (ms: number) => { const t0 = Date.now(); while (Date.now() - t0 < ms) { if ((await ui(page)).kind !== "menu") return true; await sleep(30); } return false; };
  const c = await centre(page, "#screen");
  await f.start(1, c.x - 30, c.y); await f.start(2, c.x + 30, c.y); await sleep(80); await f.end(2); await f.end(1);
  check("a two-finger tap is Back (the game receives the cancel button: the menu closes)", await closed(3000));
  await untilMenu(page, f, (u) => u.back, "the sub-menu again");
  // the Back button does the same (it is shown only while it does something)
  check("the Back button is enabled in a sub-menu", await page.$eval("#gBack", (e) => !(e as HTMLButtonElement).disabled && (e as HTMLElement).style.visibility === "visible"));
  await page.$eval("#gBack", (e) => (e as HTMLElement).click());
  check("the Back button is the cancel button too", await closed(3000));
  await untilMenu(page, f, (u) => u.back, "the sub-menu again");
  // RETURN TO TOP MENU is the last item: tap it, Back disappears
  const last = (await ui(page)).rects.length - 1;
  const lr = (await ui(page)).rects[last];
  const pl = await gameXY(page, lr.x + lr.w / 2, lr.y + lr.h / 2);
  await f.tap(pl.x, pl.y);
  await untilMenu(page, f, (u) => !u.back, "the top menu");
  check("tapping the last line of a sub-menu (RETURN TO TOP MENU) goes up one level", !(await ui(page)).back);
  await tapMenuText(page, f, "SHOOTING");
  let guard = 0;
  while (guard++ < 40) {
    const u = await ui(page);
    if (u.kind === "shoot") break;
    if (u.kind === "menu" && u.ready) { const r = u.rects.find((x: any) => x.k === 0); const p = await gameXY(page, r.x + r.w / 2, r.y + r.h / 2); await f.tap(p.x, p.y); await sleep(1200); }
    else if (u.kind === "message" && u.ready) { const cc = await centre(page, "#screen"); await f.tap(cc.x, cc.y); await sleep(500); }
    else await sleep(150);
  }
  const us = await waitUi(page, (u) => u.kind === "shoot", 40000, "a shooting stage");
  check("a shooting stage is reached by taps alone", us.kind === "shoot" && !us.drawn);
  await sleep(2500);
  const shots0 = await reg(page, 0x7908);
  const gx = 200, gy = 100;
  const p = await gameXY(page, gx, gy);
  await f.tap(p.x, p.y);
  await sleep(800);
  const shots1 = await reg(page, 0x7908), drawn = await reg(page, 0x78F8);
  const gv = await reg(page, 0x798A), vx = gv >> 8, vy = gv & 0xff; // $798A / $798B: the gun position the game sees (default calibration: x, y - 4)
  check("the first tap in a shooting stage draws the blaster and fires", (drawn & 1) === 1 && shots1 === shots0 + 1, `drawn ${drawn} shots ${shots0}->${shots1}`);
  check("the shot lands on the tapped game pixel (gun registers within 2 px)", Math.abs(vx - gx) <= 2 && Math.abs(vy - (gy - 4)) <= 2, `x ${vx} (want ${gx}) y ${vy} (want ${gy - 4})`);
  const p2 = await gameXY(page, 60, 150);
  await f.tap(p2.x, p2.y);
  await sleep(1200);
  const gv2 = await reg(page, 0x798A), shots2 = await reg(page, 0x7908);
  check("a second tap fires again at its own point", shots2 === shots1 + 1 && Math.abs((gv2 >> 8) - 60) <= 2 && Math.abs((gv2 & 0xff) - 146) <= 2, `shots ${shots1}->${shots2} x ${gv2 >> 8} y ${gv2 & 0xff}`);
  check("a Holster button shows while the blaster is drawn", await page.$eval("#gHolster", (e) => (e as HTMLElement).style.visibility === "visible"));
  await shot(page, "game-iphone-portrait-shoot.png");
  await page.close();
}

// ---- messages, the keyboard and the dark room: states reached through the script debug jumps
{
  const d = DEVICES[0];
  // a message: tap anywhere on the page advances it
  const { page, cdp } = await open(d, { mode: "game", query: "?scene=SP01:0" });
  await waitSnatcher(page);
  const f = new Fingers(cdp);
  await waitUi(page, (u) => u.kind === "message", 60000, "a message");
  const s0 = await page.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()));
  await waitUi(page, (u) => u.kind === "message" && u.ready, 60000, "message waiting for the button");
  const c = await centre(page, "#screen");
  await f.tap(c.x, c.y + 400); // far below the picture: still the game's surface
  await sleep(1500);
  const s1 = await page.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()));
  const u1 = await ui(page);
  check("a tap anywhere advances a message", s1.seq !== s0.seq || s1.pc !== s0.pc || u1.kind !== "message" || !u1.ready, `${s0.seq} -> ${s1.seq}`);
  await page.close();
}
{
  const d = DEVICES[0];
  const { page, cdp } = await open(d, { mode: "game", query: "?scene=SP04:0" });
  await waitSnatcher(page);
  const f = new Fingers(cdp);
  // SP04: "USE CARD" -> ... -> "ENTER NAME" opens the alphabet keyboard
  let guard = 0;
  while (guard++ < 40) {
    const u = await ui(page);
    if (u.kind === "entry" && u.ready && u.rects.length) break;
    if ((u.kind === "menu" || u.kind === "message") && u.ready) { const c = await centre(page, "#screen"); const r = u.rects?.[0]; const p = r ? await gameXY(page, r.x + r.w / 2, r.y + r.h / 2) : c; await f.tap(p.x, p.y); await sleep(1100); } else await sleep(200);
  }
  const ue = await waitUi(page, (u) => u.kind === "entry" && u.ready && u.rects.length > 0, 60000, "the keyboard");
  check("the alphabet keyboard exposes its keys, Back and OK", ue.rects.length === 32 && ue.back && ue.start && ue.label === "ok", JSON.stringify({ n: ue.rects.length, back: ue.back, start: ue.start }));
  await shot(page, "game-iphone-portrait-keyboard.png");
  // tap the key whose cell holds 'J' (grid 0x11512): find its rectangle by the engine's rectangles and the grid layout (cell id order = reading order)
  const letters = ue.rects.slice().sort((a: any, b: any) => a.y - b.y || a.x - b.x);
  const J = letters[9];   // A B C D E F G H I J ...
  const pj = await gameXY(page, J.x + J.w / 2, J.y + J.h / 2);
  await f.tap(pj.x, pj.y);
  await sleep(2500);
  const typed: string = await page.evaluate(() => (window as any).snatcher.game.entry_text());
  check("tapping a key types that letter", typed === "J", JSON.stringify(typed));
  // the Back button is backspace
  await page.$eval("#gBack", (e) => (e as HTMLElement).click());
  await sleep(2000);
  const typed2: string = await page.evaluate(() => (window as any).snatcher.game.entry_text());
  check("Back is backspace in the keyboard", typed2 === "", JSON.stringify(typed2));
  await page.close();
}

// ---- the dark room: touch and drag moves the free view (B held + the direction of the drag)
{
  const d = DEVICES[0];
  const { page, cdp, ctx } = await open(d, { mode: "game", query: "?scene=SP26:0ba4" });
  await waitSnatcher(page);
  const f = new Fingers(cdp);
  await untilMenu(page, f, () => true, "the dark room's menu", 60000);
  await tapMenuText(page, f, "USE METAL GEAR");
  const sm = await untilMenu(page, f, (x) => x.back, "the Metal Gear sub-menu");
  const items: string[] = await page.evaluate(() => JSON.parse((window as any).snatcher.game.menu_items()));
  const li = Math.max(0, items.findIndex((t) => /LIGHT/.test(t)));
  const lr = sm.rects[li];
  const lp = await gameXY(page, lr.x + lr.w / 2, lr.y + lr.h / 2);
  await f.tap(lp.x, lp.y);
  const u = await untilMenu(page, f, (x) => x.pointer, "the menu with the free view (pointer mode)", 90000);
  check("the dark room's pointer mode is reported (menu + pointer)", u.pointer === true);
  const view = async () => (await reg(page, 0x96D0)) << 16 >> 16;
  const x0 = await view();
  const c = await centre(page, "#screen");
  await f.start(1, c.x - 40, c.y - 80);
  await f.move(1, c.x + 20, c.y - 80); await f.move(1, c.x + 40, c.y - 80);
  await sleep(1200);
  const x1 = await view();
  check("dragging right moves the view right (B + RIGHT while the finger stays down)", x1 > x0 + 10, `${x0} -> ${x1}`);
  await f.move(1, c.x - 60, c.y - 80); await f.move(1, c.x - 120, c.y - 80);
  await sleep(1500);
  const x2 = await view();
  check("dragging back moves it left", x2 < x1 - 10, `${x1} -> ${x2}`);
  await f.end(1);
  await sleep(600);
  const x3 = await view(), x4 = await (async () => { await sleep(500); return view(); })();
  check("lifting the finger lets go (the view stops, the menu is back)", x3 === x4 && (await ui(page)).kind === "menu", `${x3} ${x4}`);
  await shot(page, "game-iphone-portrait-darkroom.png");
  await ctx.close();
}

// ---- title, settings, intro: reached from power-on, with taps only
{
  const d = DEVICES[0];
  const { page, cdp, ctx } = await open(d, { mode: "game", query: "?autostart" });
  await waitSnatcher(page);
  const f = new Fingers(cdp);
  const tu = await waitUi(page, (x) => x.kind === "title" && x.ready, 120000, "the title menu");
  check("the title menu is tappable (NEW GAME)", tu.rects.length >= 1 && tu.start);
  await shot(page, "game-iphone-portrait-title.png");
  const r0 = tu.rects[0];
  const pt = await gameXY(page, r0.x + r0.w / 2, r0.y + r0.h / 2);
  await f.tap(pt.x, pt.y);
  const ou = await waitUi(page, (x) => x.kind === "options" && x.ready, 40000, "the settings screen");
  check("tapping NEW GAME opens the settings screen with its rows", ou.rects.length === 4 && ou.cur === 0, JSON.stringify({ n: ou.rects.length, cur: ou.cur }));
  await shot(page, "game-iphone-portrait-options.png");
  // CONTROLS row: a tap walks the cursor there and changes the value (one of 6 button configurations); every further tap changes it again
  const row1 = ou.rects.find((r: any) => r.id === 1);
  const p1 = await gameXY(page, row1.x + row1.w / 2, row1.y + row1.h / 2);
  const cfg0 = await reg(page, 0x78BA);
  await f.tap(p1.x, p1.y);
  await waitUi(page, (x) => x.kind === "options" && x.cur === 1, 8000, "cursor on CONTROLS");
  await sleep(1500);
  const cfg1 = await reg(page, 0x78BA), uc = await ui(page);
  check("a tap on CONTROLS walks there and changes the button configuration (the touch roles follow: confirm / cancel are physical A B C bits)", cfg1 === ((cfg0 + 1) % 6) && uc.kind === "options", `${cfg0} -> ${cfg1}`);
  for (let i = 0; i < 5; i++) { await f.tap(p1.x, p1.y); await sleep(1000); }
  check("six taps cycle through all configurations and come back", (await reg(page, 0x78BA)) === cfg0, String(await reg(page, 0x78BA)));
  // the QUIT button: walks down (GUN ADJUST is skipped without the gun) and starts the game
  const quit = ou.rects.find((r: any) => r.id === 4);
  const pq = await gameXY(page, quit.x + quit.w / 2, quit.y + quit.h / 2);
  await f.tap(pq.x, pq.y);
  const su = await waitUi(page, (x) => x.kind === "skip", 60000, "the intro (skip)");
  check("the intro shows the Start button, and no tap on the picture skips it", su.start && (await page.$eval("#gOk", (e) => (e as HTMLElement).style.visibility)) === "visible");
  const cc = await centre(page, "#screen");
  await f.tap(cc.x, cc.y); await sleep(1500);
  check("a tap on the picture does not skip the intro", (await ui(page)).kind === "skip");
  await shot(page, "game-iphone-portrait-intro.png");
  await page.$eval("#gOk", (e) => (e as HTMLElement).click());
  const gu = await waitUi(page, (x) => x.kind === "message" || x.kind === "menu", 60000, "the game after the intro");
  check("the Start button skips the intro and the game begins", gu.kind === "message" || gu.kind === "menu");
  await ctx.close();
}

// ================================================================= D. settings, defaults, persistence, desktop
{
  const d = DEVICES[0];
  const { page, cdp, ctx } = await open(d, { keepMode: true, query: "?scene=SP01:0" });
  await waitSnatcher(page);
  check("default on a phone: the on-screen pad", (await page.evaluate(() => (window as any).snatcher.touch.mode)) === "pad");
  const f = new Fingers(cdp);
  const m = await centre(page, "#tMenu");
  await f.tap(m.x, m.y);
  await sleep(300);
  check("the Menu button opens the pause panel", await page.evaluate(() => !document.getElementById("panel")!.hidden));
  const tt = await centre(page, "#tabTouch");
  await f.tap(tt.x, tt.y); await sleep(200);
  check("the Touch tab shows", await page.evaluate(() => !document.getElementById("tpTouch")!.hidden));
  await shot(page, "panel-touch-tab.png");
  await page.select("#touchMode", "game");
  await sleep(200);
  check("choosing Touch the game switches the layout at once", await page.evaluate(() => document.documentElement.dataset.touch === "game" && document.getElementById("touch")!.dataset.mode === "game"));
  await page.close();
  const p2 = await open(d, { keepMode: true, ctx, query: "?scene=SP01:0" });
  await waitSnatcher(p2.page);
  check("the mode persists after a reload (snatcher.touch.mode)", (await p2.page.evaluate(() => (window as any).snatcher.touch.mode)) === "game" && (await p2.page.evaluate(() => localStorage.getItem("snatcher.touch.mode"))) === "game");
  await p2.page.close();
}
{
  // desktop: no touch, mouse + keyboard behave as before, no pad
  const page = await browser!.newPage();
  await page.setViewport({ width: 1000, height: 760 });
  page.on("console", (m) => { if (m.type() === "error" && !/404/.test(m.text())) { errors++; console.log("[console error]", m.text()); } });
  await page.goto(`http://localhost:${port}/?scene=SP01:0`, { waitUntil: "load" });
  await waitSnatcher(page);
  const r = await page.evaluate(() => ({ touch: document.documentElement.dataset.touch ?? null, hidden: document.getElementById("touch")!.hidden || getComputedStyle(document.getElementById("touch")!).display === "none", mode: (window as any).snatcher.touch.mode, bar: getComputedStyle(document.getElementById("bar")).display, pad: !!(window as any).snatcher.input.touch }));
  check("desktop: touch controls are off, no pad is drawn, the toolbar stays", r.touch === null && r.hidden && r.mode === "off" && r.bar !== "none" && !r.pad, JSON.stringify(r));
  const cv = await centre(page, "#screen");
  await page.mouse.click(cv.x, cv.y);
  check("desktop: a mouse click still switches the light gun on", await page.evaluate(() => (window as any).snatcher.input.gunMode === true));
  await page.keyboard.down("ArrowDown"); await sleep(120); await page.keyboard.up("ArrowDown");
  check("desktop: the keyboard still plays (device keyboard)", await page.evaluate(() => (window as any).snatcher.input.device.kind === "keyboard" && (window as any).snatcher.input.gunMode === false));
  await page.close();
  // a desktop player can turn the pad on in the menu
  const p2 = await browser!.newPage();
  await p2.setViewport({ width: 1000, height: 760 });
  await p2.goto(`http://localhost:${port}/?scene=SP01:0`, { waitUntil: "load" });
  await waitSnatcher(p2);
  await p2.keyboard.press("Escape");
  await p2.click("#tabTouch");
  await p2.select("#touchMode", "pad");
  await p2.keyboard.press("Escape");
  await sleep(300);
  check("desktop: the player can turn the on-screen pad on in the Touch tab", await p2.evaluate(() => document.documentElement.dataset.touch === "pad" && !document.getElementById("touch")!.hidden));
  await p2.close();
}

// ================================================================= E. the start window and the pause panel on a phone, haptics, audio unlock
{
  const d = PHONE360;
  const { page, cdp } = await open(d, { mode: "pad", stubs: true });
  await page.waitForSelector("#btnPlay:not([disabled])", { timeout: 30000 });
  await sleep(500);
  const r = await page.evaluate(() => ({
    scroll: document.documentElement.scrollWidth > innerWidth,
    accept: document.getElementById("discPick")!.hasAttribute("accept"), touchHelp: !document.getElementById("impTouchHelp")!.hidden && getComputedStyle(document.getElementById("impTouchHelp")!).display !== "none",
  }));
  check("360x640 touch: the start window does not scroll sideways", !r.scroll);
  const ta = await page.evaluate(() => ({ body: getComputedStyle(document.body).touchAction, canvas: getComputedStyle(document.getElementById("screen")!).touchAction, user: getComputedStyle(document.getElementById("wrap")).userSelect, ob: getComputedStyle(document.documentElement).overscrollBehaviorY }));
  check("no double-tap zoom (manipulation), no gestures on the picture (none), no text selection, no pull-to-refresh", /manipulation/.test(ta.body) && ta.canvas === "none" && ta.user === "none" && ta.ob === "none", JSON.stringify(ta));
  const zoom = await page.evaluate(() => {
    const vp = document.querySelector('meta[name="viewport"]')?.getAttribute("content") ?? "";
    const els = Array.from(document.querySelectorAll<HTMLElement>("button, summary, label, a, input, select, .tbtn, .arm, #ovWin, #overlay, .modal"));
    const bad = els.filter((e) => !/manipulation|none|pan-/.test(getComputedStyle(e).touchAction)).map((e) => e.id || e.className || e.tagName);
    return { vp, bad, n: els.length };
  });
  check("the page never zooms: viewport maximum-scale=1 and user-scalable=no, every button and control has no double-tap zoom", /maximum-scale=1/.test(zoom.vp) && /user-scalable=no/.test(zoom.vp) && zoom.n > 20 && zoom.bad.length === 0, JSON.stringify(zoom));
  check("phone: the file picker has no extension filter (iOS Files / Android would grey out .cue and .bin)", !r.accept);
  const q = await page.evaluate(() => { const e = document.getElementById("impQuota")!; return { shown: !e.hidden && getComputedStyle(e).display !== "none", text: e.textContent ?? "", cls: e.className }; });
  check("a phone with little storage is told before importing (extends the storage line, no duplicate)", q.shown && /580 MB|only about/.test(q.text) && /warn/.test(q.cls), JSON.stringify(q));
  await shot(page, "overlay-360x640.png");
  const f = new Fingers(cdp);
  const small = await page.evaluate(() => Array.from(document.querySelectorAll<HTMLElement>("#overlay button, #overlay summary, #overlay label.check")).filter((e) => e.getClientRects().length && !(e as HTMLButtonElement).disabled).map((e) => ({ id: e.id || e.tagName, w: e.getBoundingClientRect().width, h: e.getBoundingClientRect().height })).filter((e) => e.h < 43.5 || e.w < 43.5));
  check("360x640 touch: start window touch targets are at least 44 px", small.length === 0, JSON.stringify(small));
  // tap Play with a finger: the audio context is made inside the tap and is running
  const pb = await centre(page, "#btnPlay");
  await f.tap(pb.x, pb.y);
  await waitSnatcher(page);
  await sleep(1500);
  const au = await page.evaluate(() => (window as any).snatcher.audio.ensure().state); // the context primed inside the tap, taken over by the game's audio
  check("audio context is running after a tap on Play (touch only)", au === "running", String(au));
  // the pause panel at 360 wide with every tab: no sideways scroll, targets
  await page.evaluate(() => (document.getElementById("tMenu") as HTMLElement).click());
  await sleep(300);
  for (const tab of ["Sound", "Display", "Controls", "Touch", "Gamepad", "Saves"]) {
    await page.click(`#tab${tab}`); await sleep(120);
    const ok = await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth && document.getElementById("panel")!.scrollWidth <= innerWidth);
    check(`360x640 touch: panel tab ${tab} has no sideways scroll`, ok);
    if (tab === "Touch") await shot(page, "panel-touch-360x640.png");
    if (tab === "Controls") await shot(page, "panel-controls-360x640.png");
  }
  // haptics on: a press vibrates
  await page.click("#tabTouch");
  await page.$eval("#touchHaptics", (e: any) => { e.checked = true; e.dispatchEvent(new Event("change")); });
  const before = await page.evaluate(() => (window as any).__vib.length);
  await page.keyboard.press("Escape"); await sleep(300);
  const bc = await centre(page, '.pbtn[data-btn="32"]');
  await f.tap(bc.x, bc.y);
  const after = await page.evaluate(() => (window as any).__vib.length);
  check("haptics (when switched on) vibrate on a pad press", after > before, `${before} -> ${after}`);
  await page.close();
}

console.log(`console / page errors: ${errors}`);
await browser!.close();
console.log(failed || errors ? `FAILED: ${failed} checks, ${errors} errors` : "all touch checks passed");
process.exit(failed || errors ? 1 : 0);
