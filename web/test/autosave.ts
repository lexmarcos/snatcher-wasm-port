// Autosave in headless Chrome (docs/re/web.md section 9). Own server on AUTOSAVE_PORT (default 5340, with the local /assets).
//   bun web/test/autosave.ts [outdir]       screenshots: work/shots/autosave/ (gitignored)
// 1 play into a new game until the first safe point: an autosave is written (IndexedDB, not localStorage, the four save slots untouched);
// 2 reload: "Continue where you left off" shows when and where, the engine frame and the screen after it equal those at the save, the music is back;
// 3 the toggle: off = no new autosave although safe points go by; the setting persists; the delete button (two clicks) removes the autosave;
// 4 "Clear everything" removes it (IndexedDB and settings); 5 an autosave of another engine layout / another disc / damaged is thrown away with a message;
// 6 blocked localStorage and blocked IndexedDB: the game runs, the page says so. Prints the measured save cost.
import puppeteer, { type Browser, type BrowserContext, type Page } from "puppeteer-core";
import { existsSync, mkdirSync, readdirSync } from "fs";
import { join } from "path";
import { homedir } from "os";

const root = join(import.meta.dir, "..", "..");
const out = process.argv[2] ?? join(root, "work/shots/autosave");
mkdirSync(out, { recursive: true });
const port = Number(process.env.AUTOSAVE_PORT ?? 5340);
if (!existsSync(join(root, "assets/raw/filelist.tsv"))) { console.log("assets/raw missing: autosave test skipped"); process.exit(0); }
let failed = 0, errors = 0;
const check = (name: string, ok: boolean, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"} ${name}${extra ? " – " + extra : ""}`); if (!ok) failed++; };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const servers: Array<ReturnType<typeof Bun.spawn>> = [];
let browser: Browser | undefined;
process.on("exit", () => { for (const s of servers) { try { s.kill(); } catch { /* gone */ } } try { browser?.process()?.kill(); } catch { /* gone */ } });
const listening = async (p: number) => { try { await fetch(`http://localhost:${p}/`); return true; } catch { return false; } };
if (!(await listening(port))) {
  servers.push(Bun.spawn(["bun", "server.ts"], { cwd: join(root, "web"), env: { ...process.env, PORT: String(port) }, stdout: "ignore", stderr: "ignore" }));
  for (let i = 0; i < 50 && !(await listening(port)); i++) await sleep(100);
}
const base = join(homedir(), ".cache/puppeteer/chrome-headless-shell");
const chrome = join(base, readdirSync(base).sort().pop()!, "chrome-headless-shell-linux64/chrome-headless-shell");
browser = await puppeteer.launch({ executablePath: chrome, args: ["--autoplay-policy=no-user-gesture-required"] });

let expectErrors = false; // the storage-blocked runs log a warning from the page on purpose? (they must not log errors either)
async function open(ctx: BrowserContext, query = "", pre?: () => void, w = 1000, h = 1000): Promise<Page> {
  const page = await ctx.newPage();
  await page.setViewport({ width: w, height: h });
  page.on("console", (m) => { if (m.type() === "error" && !/404/.test(m.text())) { errors++; console.log("[console error]", m.text()); } });
  page.on("pageerror", (e) => { errors++; console.log("[pageerror]", e.message); });
  if (pre) await page.evaluateOnNewDocument(pre);
  await page.goto(`http://localhost:${port}/${query}`, { waitUntil: "load" });
  await page.evaluate(() => document.fonts.ready);
  await sleep(450); // the start window opens in 320 ms (clicks before that hit the clipped part)
  return page;
}
void expectErrors;
const frame = (page: Page) => page.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()).frame as number);
const waitSnatcher = (page: Page) => page.waitForFunction(() => (window as any).snatcher, { timeout: 90000 });
const shot = (page: Page, name: string) => page.screenshot({ path: join(out, name) });
const visible = (page: Page, sel: string) => page.$eval(sel, (e) => !(e as HTMLElement).hidden && getComputedStyle(e).display !== "none");
const text = (page: Page, sel: string) => page.$eval(sel, (e) => e.textContent ?? "");
const waitSaves = (page: Page, n: number, ms = 90000) => page.waitForFunction((k) => (window as any).snatcherAutosave.saver.stats.saves >= k, { timeout: ms, polling: 100 }, n);

/** Presses confirm every 24 frames and records, on every tick at a safe point, the frame number and a hash of the engine's screen. */
async function drive(page: Page, presses = 600) {
  await page.evaluate((n) => {
    const s = (window as any).snatcher, g = s.game;
    const hash = () => { const b = new Uint8Array(s.memory.buffer, g.framebuffer_ptr(), 256 * 224 * 4); let h = 0x811c9dc5; for (let i = 0; i < b.length; i++) h = Math.imul(h ^ b[i], 0x01000193) >>> 0; return h; };
    const w = window as any;
    w.__caps = {}; w.__edges = 0; w.__safe = false;
    const script: number[][] = []; for (let i = 0; i < n; i++) script.push([1, 0x20], [23, 0]);
    s.input.script = script;
    const orig = g.tick.bind(g);
    g.tick = (pad: number) => {
      orig(pad);
      const p = g.autosave_point();
      if (p && !w.__safe) w.__edges++;
      w.__safe = !!p;
      if (p) w.__caps[JSON.parse(g.debug_info()).frame] = hash();
    };
  }, presses);
}
const stopDriving = (page: Page) => page.evaluate(() => { (window as any).snatcher.input.script = []; });

// ---------------------------------------------------------------- 1 and 2: a new game, a first autosave, the reload
const ctx1 = await browser.createBrowserContext();
let rec: any, capFrame = 0, capHash = 0, musicId: any = null, saveStats: any = null;
{
  const A = await open(ctx1, "?newgame");
  await waitSnatcher(A);
  await drive(A);
  console.log("playing into a new game until the first safe point (about 25 s) ...");
  await waitSaves(A, 1);
  await stopDriving(A);
  await sleep(1500);
  rec = await A.evaluate(() => { const r = (window as any).snatcherAutosave.record(); return r && { act: r.act, play: r.play, kind: r.kind, pt: r.pt, size: r.data.length, frame: r.frame, stamp: r.stamp, head: Array.from(r.data.slice(0, 4)) }; });
  check("an autosave was written at a safe point", !!rec && rec.size > 10000 && rec.size < 400000 && (rec.kind === 1 || rec.kind === 2), JSON.stringify(rec));
  check("it knows the act and the play time", rec.act === 1 && /^\d\d:\d\d:\d\d$/.test(rec.play), `${rec.act} ${rec.play}`);
  const caps = await A.evaluate(() => (window as any).__caps);
  capFrame = rec.frame; capHash = caps[rec.frame];
  check("the screen at the save was recorded", typeof capHash === "number", `frame ${capFrame}`);
  musicId = await A.evaluate(() => (window as any).snatcher.audio.fmMusic?.id ?? null);
  const where = await A.evaluate(async () => {
    const dbs = (await indexedDB.databases()).map((d) => d.name);
    const ls = Object.keys(localStorage).filter((k) => k.startsWith("snatcher"));
    return { dbs, ls, big: Object.values(localStorage).some((v) => (v as string).length > 20000), slots: (window as any).snatcher.store.slots().length };
  });
  check("the snapshot is in IndexedDB, not in localStorage", where.dbs.includes("snatcher.autosave") && !where.big, JSON.stringify(where));
  check("the four save slots are untouched", where.slots === 0 && !where.ls.some((k) => k.startsWith("snatcher.save.")), JSON.stringify(where.ls));
  saveStats = await A.evaluate(() => (window as any).snatcherAutosave.saver.stats);
  console.log("first save:", JSON.stringify(saveStats), "music", musicId);
  await A.evaluate(() => { (window as any).snatcherAutosave.saver.on = false; }); // closing the page would save the later quiet moment too (checked below)
  await A.close();
}
let B: Page;
{
  B = await open(ctx1, "");
  await B.waitForSelector("#btnContinue:not([hidden])", { timeout: 20000 });
  check("reload: Continue where you left off is offered", (await text(B, "#btnContinue")) === "Continue where you left off");
  const st = await text(B, "#autoStatus");
  check("it says when it was saved and the act", /Autosave from .+: act 1, .+ played\./.test(st), st);
  check("Continue is the primary button, Play the other", await B.$eval("#btnContinue", (e) => e.classList.contains("primary")) && !(await B.$eval("#btnPlay", (e) => e.classList.contains("primary"))));
  await sleep(500);
  await shot(B, "start-continue-en.png");
  await B.click("#langOvPt");
  await sleep(300);
  const pt = [await text(B, "#btnContinue"), await text(B, "#autoStatus")];
  check("pt-BR: Continuar de onde parou + the line", pt[0] === "Continuar de onde parou" && /Salvamento automático de .+: ato 1, .+ de jogo\./.test(pt[1]), pt.join(" | "));
  await shot(B, "start-continue-pt.png");
  await B.click("#langOvEn");
  await sleep(200);
  await B.click("#btnContinue");
  await waitSnatcher(B);
  const started = await B.evaluate(() => (window as any).snatcher.started);
  check("Continue brings back the same engine frame", started.resumed && started.frame === capFrame, `${started.frame} vs ${capFrame}`);
  check("... and exactly the same screen", started.fb === capHash, `${started.fb} vs ${capHash}`);
  if (musicId !== null) {
    await B.waitForFunction(() => !!(window as any).snatcher.audio.fmMusic, { timeout: 15000 }).catch(() => {});
    const m = await B.evaluate(() => (window as any).snatcher.audio.fmMusic?.id ?? null);
    check("the music is back", m === musicId, `${m} vs ${musicId}`);
  }
  await B.waitForFunction(() => JSON.parse((window as any).snatcher.game.debug_info()).frame > 0, { timeout: 5000 });
  const f0 = await frame(B); await sleep(600);
  check("the restored game runs on", (await frame(B)) > f0);
  check("the overlay is closed", !(await visible(B, "#overlay")));
}

// ---------------------------------------------------------------- 3 the toggle and the delete button
{
  // the page is hidden: the quiet moment it is at is saved at once (the default throttle would wait 30 s)
  await sleep(1500);
  const h0 = await B.evaluate(() => (window as any).snatcherAutosave.record());
  await B.evaluate(() => { Object.defineProperty(document, "hidden", { configurable: true, get: () => true }); document.dispatchEvent(new Event("visibilitychange")); });
  await B.waitForFunction((st) => (window as any).snatcherAutosave.record().stamp > st, { timeout: 10000 }, h0.stamp).catch(() => {});
  const h1 = await B.evaluate(() => (window as any).snatcherAutosave.record());
  check("hiding the page saves the quiet moment it is at", h1.stamp > h0.stamp && h1.frame > h0.frame, `${h0.frame} -> ${h1.frame}`);
  await B.evaluate(() => { delete (document as any).hidden; });
  const stamp0 = await B.evaluate(() => (window as any).snatcherAutosave.record().stamp);
  await B.evaluate(() => { const sv = (window as any).snatcherAutosave.saver; sv.minGap = 0; sv.every = 0; });
  await drive(B, 2000);
  await waitSaves(B, 2, 30000);
  check("with autosave on a safe point is saved again (throttle lowered for the test)", (await B.evaluate(() => (window as any).snatcherAutosave.record().stamp)) > stamp0);
  await B.keyboard.press("Escape");
  await B.click("#tabSaves");
  await sleep(300);
  check("the panel has the Autosave check box, on by default", (await B.$eval("#autoBox", (e: any) => e.checked)) && (await text(B, "#tpSaves")).includes("Autosave"));
  const line = await text(B, "#pnAuto");
  check("... and a line about the last autosave", /Autosave from/.test(line), line);
  await shot(B, "panel-saves-en.png");
  await B.click("#autoBox");
  await sleep(200);
  check("turning it off is stored (snatcher.autosave.on = 0)", (await B.evaluate(() => localStorage.getItem("snatcher.autosave.on"))) === "0");
  const off = await text(B, "#pnAuto");
  check("the line says it is off and that the old autosave stays", /Autosave is off/.test(off) && /stays until you delete it/.test(off), off);
  await B.click("#langPnPt");
  await sleep(300);
  await shot(B, "panel-saves-pt.png");
  const ptLine = await text(B, "#pnAuto");
  check("pt-BR panel text", /desligado/.test(ptLine) && /apagá-lo/.test(ptLine), ptLine);
  await B.click("#langPnEn");
  await B.keyboard.press("Escape");
  const before = await B.evaluate(async () => ({ saves: (window as any).snatcherAutosave.saver.stats.saves, stamp: (await (window as any).snatcherAutosave.store.get()).stamp, edges: (window as any).__edges }));
  await drive(B, 1500);
  await sleep(14000);
  const after = await B.evaluate(async () => ({ saves: (window as any).snatcherAutosave.saver.stats.saves, stamp: (await (window as any).snatcherAutosave.store.get()).stamp, edges: (window as any).__edges }));
  check("autosave off: safe points went by", after.edges >= 2, `${after.edges} safe points`);
  check("autosave off: no new autosave is written", after.saves === before.saves && after.stamp === before.stamp, JSON.stringify([before, after]));
  await stopDriving(B);
  console.log("save cost over the saves of this page:", JSON.stringify(await B.evaluate(() => (window as any).snatcherAutosave.saver.stats)));
  await B.close();
  // the setting persists
  B = await open(ctx1, "");
  await B.waitForSelector("#btnContinue:not([hidden])", { timeout: 20000 });
  check("the off setting survives a reload and the autosave is still offered", (await B.evaluate(() => (window as any).snatcherAutosave.saver.on)) === false);
  // delete (two clicks)
  await B.click("summary[data-i18n='ov.saves']");
  await sleep(150);
  check("the start window's Saves row shows the autosave and a delete button", /Autosave from/.test(await text(B, "#ovAuto")) && (await visible(B, "#ovAutoDelete")));
  await B.click("#ovAutoDelete");
  check("the delete button asks again first (nothing is deleted silently)", (await text(B, "#ovAutoDelete")) === "Click again to delete" && !!(await B.evaluate(() => (window as any).snatcherAutosave.record())));
  await B.click("#ovAutoDelete");
  await sleep(300);
  check("second click deletes the autosave", !(await B.evaluate(() => (window as any).snatcherAutosave.record())) && !(await B.evaluate(async () => await (window as any).snatcherAutosave.store.get())) && !(await visible(B, "#btnContinue")));
  await B.close();
}

// ---------------------------------------------------------------- 4 Clear everything
{
  const P = await open(ctx1, "?scene=SP09:0");
  await waitSnatcher(P);
  await P.evaluate(() => { (window as any).snatcherAutosave.saver.on = true; });
  await waitSaves(P, 1, 30000);
  await P.close();
  const C = await open(ctx1, "");
  await C.waitForSelector("#btnContinue:not([hidden])", { timeout: 20000 });
  await C.click("summary[data-i18n='ov.wipe']");
  await C.click("#btnWipe");
  await C.click("#btnWipe");
  await C.waitForFunction(() => !document.getElementById("btnContinue") || (document.getElementById("btnContinue") as HTMLElement).hidden, { timeout: 15000 });
  await sleep(1000);
  // the fresh page writes its own keys again (snatcher.smooth) and opens an empty database: what matters is that the old content is gone
  const left = await C.evaluate(async () => ({ rec: await (window as any).snatcherAutosave.store.get(), on: localStorage.getItem("snatcher.autosave.on"), lang: localStorage.getItem("snatcher.lang"), vols: localStorage.getItem("snatcher.vol.music") }));
  check("Clear everything removes the autosave (IndexedDB) and the autosave setting", left.rec === null && left.on === null, JSON.stringify({ rec: left.rec && "still there", on: left.on }));
  check("... and Continue is gone", !(await visible(C, "#btnContinue")));
  await C.close();
}

// ---------------------------------------------------------------- 5 other engine / other disc / damaged
{
  const fnv = `(b) => { let h = 0xcbf29ce484222325n; for (const x of b) { h = ((h ^ BigInt(x)) * 0x100000001b3n) & 0xffffffffffffffffn; } return h; }`;
  const mutate = async (page: Page, how: "engine" | "disc" | "junk") => page.evaluate(async (how, fnvSrc) => {
    const fnv = eval(fnvSrc) as (b: Uint8Array) => bigint;
    const sv = (window as any).snatcherAutosave.store;
    const r = await sv.get();
    const d: Uint8Array = r.data.slice();
    if (how === "junk") { for (let i = 20; i < d.length; i += 7) d[i] ^= 0x5a; }
    else {
      d[how === "engine" ? 6 : 14] ^= 0x01;
      const n = d.length - 8, h = fnv(d.subarray(0, n));
      for (let i = 0; i < 8; i++) d[n + i] = Number((h >> BigInt(8 * (7 - i))) & 0xffn);
    }
    r.data = d;
    await sv.put(r);
  }, how, fnv);
  for (const how of ["engine", "disc", "junk"] as const) {
    const P = await open(ctx1, "?scene=SP09:0");
    await waitSnatcher(P);
    await waitSaves(P, 1, 30000);
    await mutate(P, how);
    await P.close();
    const R = await open(ctx1, "");
    await sleep(1500);
    if (how === "disc") {
      await R.waitForSelector("#btnContinue:not([hidden])", { timeout: 20000 });
      await R.click("#btnContinue");
      await R.waitForFunction(() => /another disc/.test(document.getElementById("autoStatus")!.textContent ?? ""), { timeout: 30000 });
    }
    const msg = await text(R, "#autoStatus");
    const re = { engine: /another version of the game engine, so it was removed/, disc: /belongs to another disc, so it was removed/, junk: /damaged, so it was removed/ }[how];
    check(`autosave from ${how === "engine" ? "another engine layout" : how === "disc" ? "another disc" : "damaged data"}: plain message, removed`, re.test(msg) && !(await visible(R, "#btnContinue")), msg);
    check(`  (${how}) the start window stays usable: Play is enabled, no game was started`, !(await R.$eval("#btnPlay", (e: any) => e.disabled)) && (await visible(R, "#overlay")) && !(await R.evaluate(() => (window as any).snatcher)));
    check(`  (${how}) nothing is left in storage`, !(await R.evaluate(async () => await (window as any).snatcherAutosave.store.get())));
    if (how === "engine") await shot(R, "start-incompatible-en.png");
    await R.close();
  }
}

// ---------------------------------------------------------------- 6 blocked storage
{
  const ctx = await browser.createBrowserContext();
  const noLs = () => { Object.defineProperty(window, "localStorage", { get() { throw new DOMException("blocked", "SecurityError"); } }); };
  const P = await open(ctx, "?scene=SP09:0", noLs);
  await waitSnatcher(P);
  await waitSaves(P, 1, 30000);
  check("localStorage blocked: the game runs and the autosave still works (IndexedDB)", (await frame(P)) > 0);
  check("... autosave defaults to on", await P.$eval("#autoBox", (e: any) => e.checked));
  await P.close();
  const ctx2 = await browser.createBrowserContext();
  const noIdb = () => { Object.defineProperty(window, "indexedDB", { get() { return undefined; } }); };
  const Q = await open(ctx2, "?scene=SP09:0", noIdb);
  await waitSnatcher(Q);
  await waitSaves(Q, 1, 30000);
  await Q.keyboard.press("Escape");
  await Q.click("#tabSaves");
  const notes = await text(Q, "#pnNotes"), line = await text(Q, "#pnAuto");
  check("IndexedDB blocked: the page says the autosave lasts only until the page closes", /Browser storage is blocked: the autosave lasts only until this page closes/.test(notes) && /lasts only until this page closes/.test(line), notes + " | " + line);
  await shot(Q, "panel-saves-blocked-en.png");
  await Q.close();
  await ctx.close(); await ctx2.close();
}

console.log("save cost (first save, ms / bytes):", JSON.stringify(saveStats));
console.log(`console errors: ${errors}, failed checks: ${failed}`);
await browser.close();
process.exit(failed || errors ? 1 : 0);
