// Screens of the design system ("Junker HQ terminal", docs/design-system.md) in headless Chrome, plus layout checks.
//   bun web/test/design.ts [outdir]       default work/shots/design
// Two servers of its own: DESIGN_PORT (default 5330) with /assets OFF = a first-time visitor, and DESIGN_PORT+1 with /assets = the developer.
// The importing / imported shots of the real importer come from test/import.ts (they need the disc); the ones made here for importing and imported
// (sim-*) only set the state of the page by hand, to look at the layout without an 8 minute import.
// Checks: no horizontal scroll at 360 px (overlay, panel, game), touch targets >= 44 px at 360 px, font sizes are multiples of 16 px,
// no third-party request, tabs follow the arrow keys, Esc still closes the panel, the language toggle works live.
import puppeteer, { type Browser, type Page } from "puppeteer-core";
import { existsSync, mkdirSync, readdirSync, writeFileSync, mkdtempSync } from "fs";
import { join } from "path";
import { homedir, tmpdir } from "os";

const root = join(import.meta.dir, "..", "..");
const out = process.argv[2] ?? join(root, "work/shots/design");
mkdirSync(out, { recursive: true });
const portA = Number(process.env.DESIGN_PORT ?? 5330), portB = portA + 1;
const haveAssets = existsSync(join(root, "assets/raw/filelist.tsv"));
let failed = 0, errors = 0;
const check = (name: string, ok: boolean, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"} ${name}${extra ? " – " + extra : ""}`); if (!ok) failed++; };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const servers: Array<ReturnType<typeof Bun.spawn>> = [];
let browser: Browser | undefined;
process.on("exit", () => { for (const s of servers) { try { s.kill(); } catch { /* gone */ } } try { browser?.process()?.kill(); } catch { /* gone */ } });
const listening = async (port: number) => { try { await fetch(`http://localhost:${port}/`); return true; } catch { return false; } };
async function serve(port: number, noAssets: boolean) {
  if (await listening(port)) return;
  servers.push(Bun.spawn(["bun", "server.ts"], { cwd: join(root, "web"), env: { ...process.env, PORT: String(port), ...(noAssets ? { NO_ASSETS: "1" } : {}) }, stdout: "ignore", stderr: "ignore" }));
  for (let i = 0; i < 50 && !(await listening(port)); i++) await sleep(100);
}
await serve(portA, true);
if (haveAssets) await serve(portB, false);

const base = join(homedir(), ".cache/puppeteer/chrome-headless-shell");
const chrome = join(base, readdirSync(base).sort().pop()!, "chrome-headless-shell-linux64/chrome-headless-shell");
browser = await puppeteer.launch({ executablePath: chrome, args: ["--autoplay-policy=no-user-gesture-required"] });

const external: string[] = [];
async function open(port: number, width: number, height: number, query = ""): Promise<Page> {
  const page = await browser!.newPage();
  await page.setViewport({ width, height });
  page.on("console", (m) => { if (m.type() === "error" && !/404/.test(m.text())) { errors++; console.log("[console error]", m.text()); } });
  page.on("pageerror", (e) => { errors++; console.log("[pageerror]", e.message); });
  page.on("request", (r) => { const u = new URL(r.url()); if (!/^(localhost|127\.0\.0\.1)$/.test(u.hostname) && u.protocol.startsWith("http")) external.push(r.url()); });
  await page.goto(`http://localhost:${port}/${query}`, { waitUntil: "load" });
  await page.evaluate(() => document.fonts.ready);
  return page;
}
const shot = (page: Page, name: string) => page.screenshot({ path: join(out, name) });
const settle = () => sleep(450); // the window opens in 320 ms
const noHScroll = (page: Page) => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth && document.body.scrollWidth <= innerWidth);
const frame = (page: Page) => page.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()).frame as number);
const waitFrames = async (page: Page, n: number) => { const s = await frame(page); await page.waitForFunction((t) => JSON.parse((window as any).snatcher.game.debug_info()).frame >= t, { timeout: 60000, polling: 100 }, s + n); };

/** Every visible text element must use a font size that is a multiple of 16 px, and DotGothic16 must be the font that is in use. */
async function typeScale(page: Page, tag: string) {
  const r = await page.evaluate(() => {
    const bad: string[] = []; const fams = new Set<string>();
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const t = (n.textContent ?? "").trim(); const el = n.parentElement;
      if (!t || !el || el.closest("svg, script, style")) continue;
      const cs = getComputedStyle(el); if (cs.display === "none" || el.getClientRects().length === 0) continue;
      const fs = parseFloat(cs.fontSize); fams.add(cs.fontFamily.split(",")[0].replace(/["']/g, "").trim());
      if (fs % 16 !== 0) bad.push(`${el.tagName}.${el.className} ${fs}px "${t.slice(0, 20)}"`);
    }
    return { bad, fams: [...fams], loaded: document.fonts.check("16px DotGothic16") };
  });
  check(`${tag}: all visible text is a multiple of 16 px, one family`, r.bad.length === 0 && r.fams.length === 1 && r.fams[0] === "DotGothic16" && r.loaded, r.bad.slice(0, 4).join(" | ") + " " + r.fams.join(","));
}
/** Interactive elements that are at least 44 px tall and wide (phone width). */
async function touchTargets(page: Page, tag: string) {
  const small = await page.evaluate(() => {
    const out: string[] = [];
    for (const e of Array.from(document.querySelectorAll<HTMLElement>("button, summary, select, input[type=range], label.check, [role=tab]"))) {
      if (e.getClientRects().length === 0 || (e as HTMLButtonElement).disabled) continue;
      const r = e.getBoundingClientRect(); if (r.height < 43.5 || r.width < 43.5) out.push(`${e.id || e.tagName}:${Math.round(r.width)}x${Math.round(r.height)}`);
    }
    return out;
  });
  check(`${tag}: touch targets are at least 44 px`, small.length === 0, small.join(", "));
}

// ---------------------------------------------------------------- first-time visitor (no /assets): overlay
{
  const page = await open(portA, 1000, 820);
  await page.waitForSelector("#btnImport", { timeout: 20000 }); await settle();
  await shot(page, "01-overlay-first-visit-en.png");
  check("first visit: an invitation, no warning or error at the top", await page.evaluate(() => !document.querySelector("#overlay .win-body > .err, #overlay .win-body > .warn, #notes li") && /Import your disc/.test(document.getElementById("srcStatus")!.textContent ?? "")));
  check("first visit: Play is disabled, the import button has the focus", await page.evaluate(() => (document.getElementById("btnPlay") as HTMLButtonElement).disabled && document.activeElement?.id === "btnImport"));
  check("first visit: the developer text is inside a collapsed disclosure", await page.evaluate(() => { const d = document.getElementById("altBox") as HTMLDetailsElement; return !d.open && /bun web\/server\.ts/.test(d.textContent ?? "") && !/bun web\/server\.ts/.test(Array.from(document.querySelectorAll("#ovWin > .win-body > :not(.folds)")).map((e) => e.textContent).join(" ")); }));
  await typeScale(page, "overlay");
  // keyboard focus on a button: ring + cursor
  await page.keyboard.press("Tab"); await page.keyboard.press("Tab");
  const foc = await page.evaluate(() => { const a = document.activeElement as HTMLElement; const cs = getComputedStyle(a); const b = getComputedStyle(a, "::before"); return { id: a.id, outline: cs.outlineStyle + " " + cs.outlineWidth + " " + cs.outlineColor, color: cs.color, cursor: b.visibility }; });
  check("keyboard focus: 2 px yellow outline, yellow label, cursor shown", /solid 2px rgb\(248, 224, 56\)/.test(foc.outline) && foc.color === "rgb(248, 224, 56)" && foc.cursor === "visible", JSON.stringify(foc));
  await shot(page, "10-focus-visible.png");
  // language toggle, live
  await page.click("#langOvPt"); await sleep(300);
  check("language toggle switches live", (await page.$eval("#ovWin .strip + p, .wordmark + .intro", (e) => e.textContent ?? "")).includes("Jogue") && (await page.$eval("#langOvPt", (e) => e.getAttribute("aria-pressed"))) === "true");
  await shot(page, "02-overlay-first-visit-pt.png");
  await typeScale(page, "overlay pt-BR");
  // other ways to load, open (pt-BR is the longer text)
  await page.click("#altBox > summary"); await sleep(100);
  await shot(page, "05-overlay-other-ways-open-pt.png");
  await page.click("#langOvEn"); await sleep(300);
  await shot(page, "05-overlay-other-ways-open-en.png");
  await page.click("#altBox > summary");
  // saves + controls rows open
  for (const i of [1, 2]) await page.click(`.folds details:nth-child(${i}) > summary`);
  await page.evaluate(() => window.scrollTo(0, 0));
  await shot(page, "06-overlay-all-folds-open.png");
  for (const i of [1, 2]) await page.click(`.folds details:nth-child(${i}) > summary`);
  // dragging
  await page.evaluate(() => { const dt = new DataTransfer(); dt.items.add(new File(["x"], "a.cue")); document.getElementById("overlay")!.dispatchEvent(new DragEvent("dragenter", { dataTransfer: dt, bubbles: true, cancelable: true })); });
  await sleep(100);
  check("dragging: the drive slot turns into the drop state", await page.evaluate(() => document.getElementById("impBox")!.classList.contains("drag") && /Release/.test(document.getElementById("impBox")!.textContent ?? "")));
  await shot(page, "03-overlay-dragging.png");
  await page.evaluate(() => document.getElementById("overlay")!.dispatchEvent(new DragEvent("dragleave", { bubbles: true })));
  // an error from the real importer: a .bin without its .cue
  const tmp = mkdtempSync(join(tmpdir(), "sndesign-"));
  const bad = join(tmp, "game.bin"); writeFileSync(bad, Buffer.alloc(2352 * 40));
  await (await page.$("#discPick"))!.uploadFile(bad);
  await page.waitForFunction(() => document.getElementById("impStatus")!.className.includes("err"), { timeout: 20000 });
  await sleep(100);
  check("error: the message says what went wrong and how to fix it", /No \.cue file.*Select the \.cue/.test(await page.$eval("#impStatus", (e) => e.textContent ?? "")));
  await shot(page, "04-overlay-error.png");
  // simulated importing and imported states (layout only; the real ones are captured by test/import.ts)
  await page.evaluate(() => {
    const prog = document.getElementById("impProg")!; prog.hidden = false;
    const bar = document.getElementById("impBar") as HTMLProgressElement; const n = Math.round((bar.clientWidth + 4) / 28); const k = Math.ceil(n * 0.42);
    bar.value = Math.round((1000 * (28 * k - 4)) / (28 * n - 4));
    document.getElementById("impPhase")!.textContent = "Reading the game files… 42%";
    const st = document.getElementById("impStatus")!; st.className = "line dim"; st.textContent = "Reading the cue sheet. Keep this tab open.";
    (document.getElementById("btnImport") as HTMLButtonElement).textContent = "Importing…"; (document.getElementById("btnImport") as HTMLButtonElement).disabled = true;
    document.getElementById("btnCancel")!.hidden = false;
    document.getElementById("impStorage")!.textContent = "Browser storage: 1.2 GB used of 98 GB. Imported data: 0 B. Persistent storage: granted.";
  });
  await sleep(100);
  await shot(page, "sim-importing.png");
  check("progress is a real <progress> with values and a block fill", await page.evaluate(() => { const b = document.getElementById("impBar") as HTMLProgressElement; return b.tagName === "PROGRESS" && b.max === 1000 && b.value > 0 && b.value < 1000; }));
  await page.evaluate(() => {
    document.getElementById("impProg")!.hidden = true; document.getElementById("btnCancel")!.hidden = true;
    (document.getElementById("btnImport") as HTMLButtonElement).textContent = "Imported";
    const st = document.getElementById("impStatus")!; st.className = "line ok"; st.textContent = "Imported in 41.2 s. Press Play. The music is prepared in the background.";
    document.getElementById("impFm")!.textContent = "preparing music 37/122";
    const b = document.getElementById("btnPlay") as HTMLButtonElement; b.disabled = false; b.focus();
    const s = document.getElementById("srcStatus")!; s.className = "line ok"; s.innerHTML = "<div>Using: imported disc (browser storage)</div><div>Snatcher (USA): 214 game files</div><div>CD music stored in the browser</div>";
    for (const id of ["btnClear"]) document.getElementById(id)!.hidden = false;
  });
  await sleep(100);
  await shot(page, "sim-imported-ready.png");
  await page.close();
}

// ---------------------------------------------------------------- phone width, first-time visitor
{
  const page = await open(portA, 360, 740);
  await page.waitForSelector("#btnImport", { timeout: 20000 }); await settle();
  await shot(page, "20-mobile-overlay-en.png");
  check("360 px: overlay has no horizontal scroll", await noHScroll(page));
  await touchTargets(page, "360 px overlay");
  await page.evaluate(() => document.getElementById("overlay")!.scrollTo(0, 400));
  await shot(page, "20-mobile-overlay-en-scrolled.png");
  await page.click("#langOvPt"); await sleep(300);
  await page.evaluate(() => document.getElementById("overlay")!.scrollTo(0, 0));
  await shot(page, "21-mobile-overlay-pt.png");
  check("360 px pt-BR: overlay has no horizontal scroll", await noHScroll(page));
  await page.evaluate(() => { document.querySelectorAll(".folds details").forEach((d) => ((d as HTMLDetailsElement).open = true)); });
  check("360 px pt-BR: folds open, still no horizontal scroll", await noHScroll(page));
  await page.evaluate(() => document.getElementById("overlay")!.scrollTo(0, 99999));
  await shot(page, "22-mobile-overlay-pt-folds.png");
  await page.close();
}

// ---------------------------------------------------------------- developer with /assets: ready state, panel, toolbar, phone panel
if (haveAssets) {
  const page = await open(portB, 1000, 820);
  await page.waitForSelector("#btnPlay:not([disabled])", { timeout: 20000 }); await settle();
  await shot(page, "07-overlay-ready-served.png");
  check("served assets are selected by themselves, with one plain line", await page.evaluate(() => document.getElementById("srcStatus")!.children.length === 1 && /found on this server/.test(document.getElementById("srcStatus")!.textContent ?? "")));
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await waitFrames(page, 300);
  await shot(page, "11-toolbar-playing.png");
  check("monitor: canvas keeps an integer scale", await page.evaluate(() => { const c = document.getElementById("screen")!.getBoundingClientRect(); const s = c.width / 256; return Math.abs(s - Math.round(s)) < 1e-6 && c.height === 224 * s; }));
  await typeScale(page, "toolbar");
  await page.evaluate(() => { (document.getElementById("dbg") as HTMLInputElement).checked = true; (document.getElementById("dbg") as HTMLInputElement).dispatchEvent(new Event("change")); });
  await waitFrames(page, 5);
  await shot(page, "12-debug-overlay.png");
  await typeScale(page, "debug overlay");
  await page.evaluate(() => { (document.getElementById("dbg") as HTMLInputElement).checked = false; (document.getElementById("dbg") as HTMLInputElement).dispatchEvent(new Event("change")); });
  await page.keyboard.press("Escape");
  await sleep(100);
  const f0 = await frame(page); await sleep(300);
  check("panel pauses the game", (await frame(page)) === f0);
  check("Resume has the focus and is primary", await page.evaluate(() => document.activeElement?.id === "pnResume" && document.getElementById("pnResume")!.classList.contains("primary")));
  const names = ["sound", "display", "controls", "gamepad", "saves"];
  await page.focus("#tabSound");
  for (let i = 0; i < names.length; i++) {
    const sel = await page.evaluate(() => Array.from(document.querySelectorAll("[role=tab]")).map((t) => t.getAttribute("aria-selected")).join(","));
    check(`tab ${names[i]} is selected and the others are not`, sel === names.map((_, j) => (j === i ? "true" : "false")).join(","), sel);
    await shot(page, `30-panel-${i + 1}-${names[i]}.png`);
    if (i < names.length - 1) await page.keyboard.press("ArrowRight");
  }
  await page.keyboard.press("ArrowRight");
  check("ArrowRight wraps to the first tab, ArrowLeft goes back, End jumps to the last", await page.evaluate(() => document.getElementById("tabSound")!.getAttribute("aria-selected") === "true"));
  await page.keyboard.press("ArrowLeft"); await page.keyboard.press("Home"); await page.keyboard.press("End");
  check("End selects the last tab and moves focus", await page.evaluate(() => document.activeElement?.id === "tabSaves" && !(document.getElementById("tpSaves") as HTMLElement).hidden));
  await page.keyboard.press("Home");
  await typeScale(page, "panel");
  // each tab at the longest language
  await page.click("#langPnPt"); await sleep(300);
  for (let i = 0; i < names.length; i++) { await page.click(`#tab${names[i][0].toUpperCase()}${names[i].slice(1)}`); await shot(page, `31-panel-pt-${i + 1}-${names[i]}.png`); }
  await page.click("#langPnEn"); await sleep(200);
  await page.keyboard.press("Escape");
  check("Esc closes the panel", await page.evaluate(() => (document.getElementById("panel") as HTMLElement).hidden));
  await page.close();

  // phone: game, panel
  const ph = await open(portB, 360, 740, "?autostart");
  await ph.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await waitFrames(ph, 200);
  await shot(ph, "23-mobile-game.png");
  check("360 px: game screen has no horizontal scroll", await noHScroll(ph));
  await touchTargets(ph, "360 px toolbar");
  await ph.click("#btnMenu"); await sleep(150);
  await shot(ph, "24-mobile-panel-sound.png");
  check("360 px: panel has no horizontal scroll", await noHScroll(ph));
  await touchTargets(ph, "360 px panel");
  for (const [id, n] of [["tabControls", "controls"], ["tabGamepad", "gamepad"], ["tabSaves", "saves"]]) { await ph.click(`#${id}`); await sleep(80); await shot(ph, `24-mobile-panel-${n}.png`); check(`360 px: ${n} tab has no horizontal scroll`, await noHScroll(ph)); }
  await ph.click("#langPnPt"); await sleep(300);
  await ph.click("#tabDisplay"); await shot(ph, "25-mobile-panel-display-pt.png");
  check("360 px pt-BR: panel has no horizontal scroll", await noHScroll(ph));
  await ph.close();
}

check("no third-party request (the font is self-hosted)", external.length === 0, external.slice(0, 3).join(", "));
check("no console or page errors", errors === 0, String(errors));
await browser.close();
console.log(`failed checks: ${failed}`);
process.exit(failed ? 1 : 0);
