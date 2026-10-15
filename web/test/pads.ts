// Controller checks in headless Chrome with a stubbed navigator.getGamepads (dev server needed, PORT env, default 5300).
//   bun test/pads.ts [outdir]     screenshots (one per family) go to outdir (default ../work/shots, gitignored)
// Covers: family detection from a fake Chrome/Firefox id, last-used-device switching (keyboard <-> pads, hot-plug), confirm/cancel per family,
// the icon the game prints for {EC} (SP07:0159 "Push the {EC} BUTTON to draw your blaster."), non-standard (Firefox style) pads,
// View/Share/- opening the web menu, remapping (saved in localStorage) and its effect on the icon. Exits 1 on a failed check or console error.
import puppeteer, { type Page } from "puppeteer-core";
import { mkdirSync, readdirSync } from "fs";
import { join } from "path";
import { homedir } from "os";

const port = process.env.PORT ?? "5300";
const out = process.argv[2] ?? join(import.meta.dir, "../../work/shots");
mkdirSync(out, { recursive: true });
const base = join(homedir(), ".cache/puppeteer/chrome-headless-shell");
const chrome = join(base, readdirSync(base).sort().pop()!, "chrome-headless-shell-linux64/chrome-headless-shell");

let failed = 0, errors = 0;
const check = (name: string, ok: boolean, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"} ${name}${extra ? " – " + extra : ""}`); if (!ok) failed++; };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const browser = await puppeteer.launch({ executablePath: chrome, args: ["--autoplay-policy=no-user-gesture-required"] });

interface FakePad { id: string; mapping: string; buttons: number; axes: number }
const PADS = {
  xbox: { id: "Xbox 360 Controller (STANDARD GAMEPAD Vendor: 045e Product: 028e)", mapping: "standard", buttons: 17, axes: 4 },
  dualsense: { id: "DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)", mapping: "standard", buttons: 18, axes: 4 },
  switch: { id: "Pro Controller (STANDARD GAMEPAD Vendor: 057e Product: 2009)", mapping: "standard", buttons: 17, axes: 4 },
  firefox: { id: "045e-028e-Microsoft X-Box 360 pad", mapping: "", buttons: 11, axes: 8 },
  firefoxPs: { id: "054c-09cc-Sony Interactive Entertainment Wireless Controller", mapping: "", buttons: 13, axes: 8 },
} satisfies Record<string, FakePad>;

async function open(query: string, pads: FakePad[] = []): Promise<Page> {
  const page = await browser.newPage();
  await page.setViewport({ width: 1100, height: 1000 });
  page.on("console", (m) => { if (m.type() === "error" && !/404/.test(m.text())) { errors++; console.log("[console error]", m.text()); } });
  page.on("pageerror", (e) => { errors++; console.log("[pageerror]", e.message); });
  await page.evaluateOnNewDocument((defs) => {
    // pads[i] = { def, down: Set<number>, axes: number[] } ; the test changes window.__down / __axes
    const w = window as any;
    w.__pads = defs.map((d: any, i: number) => ({ def: d, index: i, down: new Set<number>(), axes: new Array(d.axes).fill(0) }));
    if (!defs.length) w.__pads = [];
    navigator.getGamepads = () => w.__pads.map((p: any) => p && ({
      id: p.def.id, index: p.index, connected: true, mapping: p.def.mapping, timestamp: performance.now(),
      buttons: Array.from({ length: p.def.buttons }, (_: unknown, i: number) => ({ pressed: p.down.has(i), touched: p.down.has(i), value: p.down.has(i) ? 1 : 0 })),
      axes: p.axes.slice(),
    })) as any;
    w.__press = (i: number, ...b: number[]) => { for (const x of b) w.__pads[i].down.add(x); };
    w.__release = (i: number, ...b: number[]) => { for (const x of b) w.__pads[i].down.delete(x); };
    w.__axes = (i: number, a: number[]) => { a.forEach((v, k) => (w.__pads[i].axes[k] = v)); };
  }, pads as any);
  await page.goto(`http://localhost:${port}/?${query}`, { waitUntil: "load" });
  return page;
}
const info = (page: Page) => page.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()));
const waitFrames = async (page: Page, n: number) => { const s = (await info(page)).frame; await page.waitForFunction((t) => JSON.parse((window as any).snatcher.game.debug_info()).frame >= t, { timeout: 60000, polling: 50 }, s + n); };
const prompt = (page: Page) => page.evaluate(() => (window as any).snatcher.prompt());
const device = (page: Page) => page.evaluate(() => { const d = (window as any).snatcher.input.device; return d.kind === "pad" ? `pad:${d.family}` : d.kind; });
const canvasHash = (page: Page) => page.evaluate(() => {
  const c = document.getElementById("screen") as HTMLCanvasElement;
  const d = c.getContext("2d")!.getImageData(0, 150, 220, 40).data; // message text rows (not the animated cursor at the right edge)
  let h = 0; for (let i = 0; i < d.length; i++) h = (h * 31 + d[i]) >>> 0; return h;
});
const shot = async (page: Page, name: string) => { await (await page.$("canvas"))!.screenshot({ path: join(out, name) }); };

const TYPED = 300; // frames until the typewriter has printed the line and the pad icon is on screen
const SCENE = "scene=SP07:159"; // "Push the {c2}{EC} BUTTON{c1} to draw your blaster."  (pad configuration 0: EC = Genesis A)
const KEYS = { xbox: [2, 1, 0, 8], dualsense: [2, 1, 0, 8], switch: [2, 0, 1, 8] }; // control ids behind Genesis A, B, C, START by default

// ---- A: one screenshot per family; the icon shown is the one of the physical button behind Genesis A ----
const hashes: Record<string, number> = {};
{
  const page = await open(SCENE);
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await waitFrames(page, TYPED);
  check("keyboard is the default device: keyboard icons", (await device(page)) === "keyboard" && (await prompt(page)).style === 4);
  hashes.keyboard = await canvasHash(page);
  await shot(page, "pads-keyboard.png");
  await page.close();
}
for (const [name, def] of [["xbox", PADS.xbox], ["dualsense", PADS.dualsense], ["switch", PADS.switch], ["firefox", PADS.firefox], ["firefoxPs", PADS.firefoxPs]] as const) {
  const page = await open(SCENE, [def]);
  // the pad is "active" from the first frame: its unassigned top face button is held (index 3 = Y / triangle / X)
  await page.evaluate(() => (window as any).__press(0, 3));
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await waitFrames(page, TYPED);
  const fam = name === "dualsense" || name === "firefoxPs" ? "playstation" : name === "switch" ? "switch" : "xbox";
  check(`${name}: last used device is the ${fam} pad`, (await device(page)) === `pad:${fam}`, await device(page));
  const p = await prompt(page);
  const want = name === "dualsense" || name === "firefoxPs" ? KEYS.dualsense : name === "switch" ? KEYS.switch : KEYS.xbox;
  check(`${name}: engine got style ${fam} and the default control ids`, p.style === { xbox: 1, playstation: 2, switch: 3 }[fam] && JSON.stringify(p.ids) === JSON.stringify(want), JSON.stringify(p));
  hashes[name] = await canvasHash(page);
  await shot(page, `pads-${name}.png`);
  await page.close();
}
check("every family draws a different icon (screens differ pairwise, except identical layouts)", (() => {
  const h = hashes;
  return new Set([h.keyboard, h.xbox, h.dualsense, h.switch]).size === 4 && h.firefox === h.xbox && h.firefoxPs === h.dualsense;
})(), JSON.stringify(hashes));

// ---- A2: the word START in the code-entry text (SP09:0542 "press the {c3}START{c1} button") becomes the Menu / Options / + icon ----
{
  const sh: Record<string, number> = {};
  for (const [name, def] of [["keyboard", null], ["xbox", PADS.xbox], ["dualsense", PADS.dualsense], ["switch", PADS.switch]] as const) {
    const page = await open("scene=SP09:542", def ? [def] : []);
    if (def) await page.evaluate(() => (window as any).__press(0, 3));
    await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
    await waitFrames(page, TYPED);
    sh[name] = await canvasHash(page);
    await shot(page, `pads-start-${name}.png`);
    await page.close();
  }
  check("START icon differs per device (keyboard Enter, Menu, Options, +)", new Set(Object.values(sh)).size === 4, JSON.stringify(sh));
}

// ---- B: confirm / cancel per family and hot-plug / last device ----
{
  const page = await open("scene=SP07:159&autostart", [PADS.xbox, PADS.dualsense, PADS.switch]);
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await waitFrames(page, TYPED);
  check("idle pads do not claim the device", (await device(page)) === "keyboard");
  // pad 0 (Xbox) top button, then pad 2 (Switch) top button: the last one wins; each switch is live
  await page.evaluate(() => (window as any).__press(0, 3)); await waitFrames(page, 3);
  check("Xbox pad used -> xbox", (await device(page)) === "pad:xbox" && (await prompt(page)).style === 1);
  await page.evaluate(() => { (window as any).__release(0, 3); (window as any).__press(2, 3); }); await waitFrames(page, 3);
  check("then the Switch pad -> switch", (await device(page)) === "pad:switch" && (await prompt(page)).style === 3);
  await page.evaluate(() => (window as any).__release(2, 3));
  await page.keyboard.down("KeyZ"); await waitFrames(page, 3); await page.keyboard.up("KeyZ");
  check("then the keyboard -> keyboard", (await device(page)) === "keyboard" && (await prompt(page)).style === 4);
  // mouse click on the canvas does not change the icon set
  await page.mouse.click(300, 300); await waitFrames(page, 3);
  check("mouse does not change the icon set", (await prompt(page)).style === 4);
  // disconnect of the active pad -> keyboard
  await page.evaluate(() => { (window as any).__press(1, 3); }); await waitFrames(page, 3);
  check("DualSense -> playstation", (await device(page)) === "pad:playstation" && (await prompt(page)).style === 2);
  await page.evaluate(() => { (window as any).__release(1, 3); (window as any).__pads[1] = null; const e: any = new Event("gamepaddisconnected"); e.gamepad = { index: 1 }; dispatchEvent(e); });
  await waitFrames(page, 3);
  check("unplugging the active pad falls back to the keyboard", (await device(page)) === "keyboard");
  // connect event refreshes the list
  await page.evaluate(() => { dispatchEvent(new Event("gamepadconnected")); });
  await page.close();
}

// per family: the confirm button advances the dialogue, the cancel button does not.
// SP07:0135 says "All right, we're going in..." (@0395) and then, with the pad, "Push the {EC} BUTTON..." (@03fe); debug_info().seq shows the offset.
for (const [name, def, confirm, cancel] of [["xbox", PADS.xbox, 0, 1], ["dualsense", PADS.dualsense, 0, 1], ["switch", PADS.switch, 1, 0]] as const) {
  const page = await open("scene=SP07:135", [def]);
  await page.evaluate(() => (window as any).__press(0, 3));
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await waitFrames(page, TYPED);
  await page.evaluate(() => (window as any).__release(0, 3));
  const s0 = (await info(page)).seq;
  await page.evaluate((b) => (window as any).__press(0, b), cancel); await waitFrames(page, 6);
  await page.evaluate((b) => (window as any).__release(0, b), cancel); await waitFrames(page, 30);
  const s1 = (await info(page)).seq;
  await page.evaluate((b) => (window as any).__press(0, b), confirm); await waitFrames(page, 6);
  await page.evaluate((b) => (window as any).__release(0, b), confirm); await waitFrames(page, 60);
  const s2 = (await info(page)).seq;
  check(`${name}: cancel button leaves the message, confirm button advances it`, s0 === "08[2, 917]" && s1 === s0 && s2 === "08[2, 1022]", `${s0} / ${s1} / ${s2}`);
  await page.close();
}

// ---- C: non-standard (Firefox style) pad: D-pad on hat axes, stick dead zone ----
{
  const page = await open(SCENE, [PADS.firefox]);
  await page.evaluate(() => (window as any).__press(0, 3));
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await waitFrames(page, 5);
  await page.evaluate(() => { (window as any).__release(0, 3); (window as any).__axes(0, [0, 0, -1, 0, 0, -1, 0, 1]); });
  await waitFrames(page, 3);
  const dir = await page.evaluate(() => { const i = (window as any).snatcher.input; i.scan(); return i.poll(); });
  check("raw xbox: hat axis down = D-pad down (bit 2), idle triggers (-1) give nothing", dir === 2, String(dir));
  await page.evaluate(() => (window as any).__axes(0, [0.3, 0.2, -1, 0, 0, -1, 0, 0]));
  await waitFrames(page, 3);
  const dz = await page.evaluate(() => { const i = (window as any).snatcher.input; i.scan(); return i.poll(); });
  check("stick outside the radial dead zone (0.3, 0.2: length 0.36) is read as a diagonal", dz === 2 + 8, String(dz));
  await page.evaluate(() => (window as any).__axes(0, [0.2, 0.1, -1, 0, 0, -1, 0, 0]));
  await waitFrames(page, 3);
  const dz2 = await page.evaluate(() => { const i = (window as any).snatcher.input; i.scan(); return i.poll(); });
  check("stick inside the dead zone gives no direction", dz2 === 0, String(dz2));
  await page.close();
}

// ---- D: web menu from the pad, remap, persistence, icon follows the remap ----
{
  const page = await open(SCENE, [PADS.xbox]);
  await page.evaluate(() => (window as any).__press(0, 3));
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await waitFrames(page, TYPED);
  await page.evaluate(() => (window as any).__release(0, 3));
  const vis = (sel: string) => page.$eval(sel, (e) => !(e as HTMLElement).hidden);
  await page.evaluate(() => (window as any).__press(0, 8)); await sleep(150);
  await page.evaluate(() => (window as any).__release(0, 8)); await sleep(100);
  check("View button opens the web menu (game paused)", await vis("#panel"));
  const rows = await page.$$eval("#padMapTable tr", (r) => r.length);
  check("remap table has 4 rows", rows === 4, String(rows));
  await page.screenshot({ path: join(out, "pads-menu.png") });
  // assign Genesis A (first row: confirm/C is the first row, A the third) to RB (index 5)
  const btns = await page.$$("#padMapTable button");
  await btns[2].click();   // "Draw blaster (Genesis A)"
  const label = await page.$eval("#padMapTable", (e) => e.textContent ?? "");
  check("assign shows the waiting prompt", /Press a button/.test(label));
  await sleep(150);
  await page.evaluate(() => (window as any).__press(0, 5)); await sleep(200);
  await page.evaluate(() => (window as any).__release(0, 5)); await sleep(100);
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem("snatcher.padmaps")!));
  check("RB assigned to Genesis A and saved in localStorage", saved.xbox.A === "r1" && saved.xbox.C === "south", JSON.stringify(saved.xbox));
  const p2 = await prompt(page);
  check("engine told: A is now control r1 (id 5)", p2.ids[0] === 5, JSON.stringify(p2));
  await page.screenshot({ path: join(out, "pads-menu-remapped.png") });
  // Esc while capturing cancels the capture only
  await (await page.$$("#padMapTable button"))[1].click();
  await page.keyboard.press("Escape");
  check("Esc cancels the capture and keeps the menu open", (await vis("#panel")) && !(await page.$eval("#padMapTable", (e) => /Press a button/.test(e.textContent ?? ""))));
  // swap: give B the south button (C had it) -> C gets B's old control
  await (await page.$$("#padMapTable button"))[1].click(); await sleep(150);
  await page.evaluate(() => (window as any).__press(0, 0)); await sleep(200);
  await page.evaluate(() => (window as any).__release(0, 0)); await sleep(100);
  const sw = await page.evaluate(() => JSON.parse(localStorage.getItem("snatcher.padmaps")!).xbox);
  check("assigning a used control swaps (B on south, C on east)", sw.B === "south" && sw.C === "east", JSON.stringify(sw));
  // close with View again, reload: map persists, and the prompt uses it
  await page.evaluate(() => (window as any).__press(0, 8)); await sleep(150);
  await page.evaluate(() => (window as any).__release(0, 8)); await sleep(150);
  check("View closes the menu", !(await vis("#panel")));
  await page.reload({ waitUntil: "load" });
  await page.evaluate(() => (window as any).__press(0, 3));
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await waitFrames(page, 30);
  const p3 = await prompt(page);
  check("after reload the saved map is used", JSON.stringify(p3.ids) === JSON.stringify([5, 0, 1, 8]), JSON.stringify(p3));
  await waitFrames(page, TYPED);
  hashes.remapped = await canvasHash(page);
  check("the icon on screen changed with the remap (RB instead of X)", hashes.remapped !== hashes.xbox);
  await shot(page, "pads-xbox-remapped-rb.png");
  // reset to defaults
  await page.evaluate(() => (window as any).__press(0, 8)); await sleep(150);
  await page.evaluate(() => (window as any).__release(0, 8)); await sleep(150);
  await page.click("#padReset"); await sleep(100);
  const rs = await page.evaluate(() => JSON.parse(localStorage.getItem("snatcher.padmaps")!).xbox);
  check("reset restores the defaults", rs.C === "south" && rs.B === "east" && rs.A === "west", JSON.stringify(rs));
  // original letters option
  await page.select("#promptMode", "letters"); await sleep(100);
  check("'Original letters' mode sends style 0", (await prompt(page)).style === 0);
  await page.close();
}

// ---- E: storage blocked: the game still works and maps just are not saved ----
{
  const page = await browser.newPage();
  await page.evaluateOnNewDocument(() => {
    Object.defineProperty(window, "localStorage", { get() { throw new Error("blocked"); } });
    navigator.getGamepads = () => []; // a real pad attached to the test machine must not take part
  });
  page.on("pageerror", (e) => { errors++; console.log("[pageerror]", e.message); });
  await page.goto(`http://localhost:${port}/?${SCENE}`, { waitUntil: "load" });
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await waitFrames(page, 30);
  check("works with localStorage blocked", (await prompt(page)).style === 4, JSON.stringify(await prompt(page)) + " " + (await device(page)));
  await page.close();
}

console.log(`console errors: ${errors}, failed checks: ${failed}; screenshots in ${out}`);
await browser.close();
process.exit(failed || errors ? 1 : 0);
