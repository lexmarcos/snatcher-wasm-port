// End-to-end test of the in-browser disc importer, in headless Chrome, with the dev server's /assets switched OFF (NO_ASSETS=1,
// started here on PORT 5320 unless one is already listening there), so that only the importer can supply the game.
//   bun web/test/import.ts [shots-dir]        default work/shots/import;  ONLY=single-bin|multi-bin|wrong runs one part
// Needs the local disc: work/snatcher.cue + .bin, and (to compare the sound) the reference extraction in assets/.
//   A. single-bin cue picked with the file input: progress bar, import, Play, sound, audio bit-exact against the reference WAVs;
//      then a page RELOAD: plays from the cache without a new import; storage line; "remove imported data".
//   B. Redump-style multi-bin copy (work/split, made by `snt split-cue`) DROPPED on the page: same checks.
//   C. wrong inputs: no cue, missing bin, not a disc, wrong region/edition: a clear message each.
// Exit code 1 on any failed check, console error or page error. Prints the measurements (import time, FM time, peak RSS, cache size).
import puppeteer, { type Browser, type Page } from "puppeteer-core";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync, mkdtempSync, rmSync, statSync } from "fs";
import { join } from "path";
import { homedir, tmpdir } from "os";

const root = join(import.meta.dir, "..", "..");
const out = process.argv[2] ?? join(root, "work/shots/import");
mkdirSync(out, { recursive: true });
const cue1 = join(root, "work/snatcher.cue"), bin1 = join(root, "work/snatcher.bin");
const splitDir = join(root, "work/split");
const port = process.env.IMPORT_PORT ?? "5320"; // not PORT: test-web.sh exports PORT=5301 for its server, which does serve /assets
const ref = join(root, "assets");
if (!existsSync(cue1) || !existsSync(bin1)) { console.log("SKIP: work/snatcher.cue/.bin not present"); process.exit(0); }
const haveRef = existsSync(join(ref, "raw/audio/track04.wav"));

let failed = 0, errors = 0;
const check = (name: string, ok: boolean, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"} ${name}${extra ? " – " + extra : ""}`); if (!ok) failed++; };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// whatever happens (failed check, exception, early exit) the server and the browser started here must not stay behind
let browser: Browser | undefined, browserPid = 0;
let server: ReturnType<typeof Bun.spawn> | null = null;
process.on("exit", () => { try { server?.kill(); } catch { /* gone */ } try { browser?.process()?.kill(); } catch { /* gone */ } });

// ---- server without /assets
const listening = async () => { try { await fetch(`http://localhost:${port}/`); return true; } catch { return false; } };
if (!(await listening())) {
  server = Bun.spawn(["bun", "server.ts"], { cwd: join(root, "web"), env: { ...process.env, PORT: port, NO_ASSETS: "1" }, stdout: "ignore", stderr: "ignore" });
  for (let i = 0; i < 50 && !(await listening()); i++) await sleep(100);
}
const noAssets = (await fetch(`http://localhost:${port}/assets/raw/filelist.tsv`)).status === 404;
if (!noAssets) { console.log(`FAIL port ${port} serves /assets: start the server with NO_ASSETS=1 (or stop it)`); server?.kill(); process.exit(1); }

const base = join(homedir(), ".cache/puppeteer/chrome-headless-shell");
const chrome = join(base, readdirSync(base).sort().pop()!, "chrome-headless-shell-linux64/chrome-headless-shell");
// One real browser (own temporary profile, so OPFS is on disk like for a user, not the in-memory storage of an incognito context) per run.
async function launch() {
  browser = await puppeteer.launch({ executablePath: chrome, args: ["--autoplay-policy=no-user-gesture-required"] });
  browserPid = browser!.process()!.pid!;
}
await launch();

// ---- memory: RSS of the browser process tree (browser, GPU, renderer with the page and the worker threads), sampled every 250 ms
function rssTree(): { total: number; top: string } {
  const r = Bun.spawnSync(["ps", "-eo", "pid=,ppid=,rss=,args="]).stdout.toString().trim().split("\n").map((l) => { const m = l.trim().match(/^(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/)!; return { pid: +m[1], ppid: +m[2], kb: +m[3], args: m[4] }; });
  const kids = new Map<number, number[]>(); const by = new Map<number, (typeof r)[0]>();
  for (const e of r) { by.set(e.pid, e); kids.set(e.ppid, [...(kids.get(e.ppid) ?? []), e.pid]); }
  let total = 0; const stack = [browserPid]; const parts: string[] = [];
  while (stack.length) {
    const p = stack.pop()!; const e = by.get(p); if (!e) continue;
    total += e.kb / 1024; stack.push(...(kids.get(p) ?? []));
    const ty = /--type=(\w[\w-]*)/.exec(e.args)?.[1] ?? "browser";
    if (e.kb > 60000) parts.push(`${ty} ${(e.kb / 1024).toFixed(0)}`);
  }
  return { total, top: parts.join(", ") };
}
let peak = 0, peakTop = "", sampling = true;
(async () => { while (sampling) { const t = rssTree(); if (t.total > peak) { peak = t.total; peakTop = t.top; } await sleep(250); } })();

const requests: string[] = [];
async function open(b: Browser, query = ""): Promise<Page> {
  const page = await b.newPage();
  await page.setViewport({ width: 1000, height: 820 });
  page.on("console", (m) => { if (m.type() === "error" && !/status of 404/.test(m.text())) { errors++; console.log("[console error]", m.text()); } }); // a 404 is the favicon
  page.on("response", (r) => { if (r.status() >= 400 && !/favicon|\/assets\/raw\/filelist\.tsv/.test(r.url())) { errors++; console.log("[http", r.status() + "]", r.url()); } });
  page.on("pageerror", (e) => { errors++; console.log("[pageerror]", e.message); });
  page.on("request", (r) => requests.push(new URL(r.url()).pathname));
  await page.goto(`http://localhost:${port}/${query}`, { waitUntil: "load" });
  return page;
}
const frame = (page: Page) => page.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()).frame as number);
const waitFrames = async (page: Page, n: number) => { const s = await frame(page); await page.waitForFunction((t) => JSON.parse((window as any).snatcher.game.debug_info()).frame >= t, { timeout: 120000, polling: 100 }, s + n); };
const text = (page: Page, sel: string) => page.$eval(sel, (e) => e.textContent ?? "");
const shot = (page: Page, name: string) => page.screenshot({ path: join(out, name) });
// the states of the real importer also go to work/shots/design (design system review, see test/design.ts for the rest of the screens)
const designOut = process.env.DESIGN_OUT ?? join(root, "work/shots/design");
mkdirSync(designOut, { recursive: true });
const dshot = (page: Page, name: string) => page.screenshot({ path: join(designOut, name) });

/** Wait for the import to end (success line or error) and return what the status line says. */
async function waitImport(page: Page, shotName?: string) {
  let shotDone = !shotName;
  const t0 = Date.now();
  while (Date.now() - t0 < 600000) {
    const st = await page.evaluate(() => ({ prog: !(document.getElementById("impProg") as HTMLElement).hidden, v: (document.getElementById("impBar") as HTMLProgressElement).value, st: document.getElementById("impStatus")!.textContent ?? "", cls: document.getElementById("impStatus")!.className, ph: document.getElementById("impPhase")!.textContent ?? "" }));
    if (!shotDone && st.prog && st.v > 300) { await shot(page, shotName!); await dshot(page, "import-importing-progress.png"); shotDone = true; console.log(`  progress ${st.v / 10}% – ${st.ph}`); }
    if (!st.prog && (st.cls.includes("ok") || st.cls.includes("err")) && st.st) return st;
    await sleep(100);
  }
  throw new Error("import did not finish");
}

/** Samples of a decoded buffer, to compare with the reference WAVs. */
async function sample(page: Page, path: string, from: number, n: number) {
  return page.evaluate(async (p, f, count) => {
    const a = (window as any).snatcher.audio; a.ensure();
    const b: AudioBuffer | null = await a.load(p);
    if (!b) return null;
    const mid = Math.min(f, b.length - count);
    return { rate: b.sampleRate, length: b.length, ch: b.numberOfChannels, from: mid, L: Array.from(b.getChannelData(0).slice(mid, mid + count)), R: b.numberOfChannels > 1 ? Array.from(b.getChannelData(1).slice(mid, mid + count)) : null };
  }, path, from, n);
}
function wavFrames(p: string, ch: number, from: number, n: number) {
  const d = readFileSync(p); const L: number[] = [], R: number[] = [];
  for (let i = from; i < from + n; i++) { L.push(d.readInt16LE(44 + (i * ch) * 2) / 32768); if (ch > 1) R.push(d.readInt16LE(44 + (i * ch + 1) * 2) / 32768); }
  return { L, R: ch > 1 ? R : null, frames: (d.length - 44) / (2 * ch) };
}
const same = (a: number[], b: number[]) => a.length === b.length && a.every((x, i) => x === b[i]);

async function audioIsBitExact(page: Page, tag: string) {
  if (!haveRef) { console.log("  (no reference extraction in assets/: bit-exact audio comparison skipped)"); return; }
  const N = 4096;
  // CD track 4 (a track from the middle of the disc), the first voice, an FM sfx, a drum segment
  const cd = await sample(page, "web/music/track04.ogg", 100000, N);
  const w = wavFrames(join(ref, "raw/audio/track04.wav"), 2, cd?.from ?? 0, N);
  check(`${tag}: CD track 04 equals assets/raw/audio/track04.wav`, !!cd && cd.rate === 44100 && cd.length === w.frames && same(cd.L, w.L) && same(cd.R!, w.R!), cd ? `${cd.length} frames` : "null");
  const vid = await page.evaluate(() => Object.keys((window as any).snatcher.audio.pcm.voices)[5]);
  const vfile = await page.evaluate((id) => (window as any).snatcher.audio.pcm.voices[id].file as string, vid);
  const v = await sample(page, "audio/web/" + vfile, 3000, N);
  const vw = wavFrames(join(ref, "audio/voice", `v${vid.padStart(4, "0")}.wav`), 1, v?.from ?? 0, N);
  check(`${tag}: voice ${vid} equals the reference WAV`, !!v && v.length === vw.frames && same(v.L, vw.L), v ? `${v.length} samples @${v.rate}` : "null");
  const ffile = await page.evaluate(() => (window as any).snatcher.audio.fm.sounds["1"].file as string);
  const f = await sample(page, "audio/web/" + ffile, 2000, N);
  const fw = wavFrames(join(ref, "audio/fm/sfx_01.wav"), 2, f?.from ?? 0, N);
  check(`${tag}: FM effect 01 (rendered in the browser) equals the reference WAV`, !!f && f.rate === 48000 && f.length === fw.frames && same(f.L, fw.L) && same(f.R!, fw.R!), f ? `${f.length} frames` : "null");
}

const flavours: Array<{ name: string; files: string[]; drop: boolean; reloadCheck: boolean }> = [
  { name: "single-bin", files: [cue1, bin1], drop: false, reloadCheck: true },
];
const splitCue = join(splitDir, "Snatcher (USA).cue");
if (existsSync(splitCue)) flavours.push({ name: "multi-bin", files: [splitCue, ...readdirSync(splitDir).filter((f) => f.endsWith(".bin")).sort().map((f) => join(splitDir, f))], drop: true, reloadCheck: false });
else console.log("NOTE: work/split missing, multi-bin run skipped (snt split-cue work/snatcher.cue work/split \"Snatcher (USA)\")");

const only = process.env.ONLY; // ONLY=single-bin | multi-bin | wrong: run just that part
const results: Record<string, any> = {};
for (const fl of flavours) {
  if (only && only !== fl.name) continue;
  console.log(`\n=== ${fl.name}${fl.drop ? " (drag and drop)" : ""} ===`);
  await launch(); // own profile: nothing cached from the other run
  const page = await open(browser!);
  await page.waitForSelector("#btnImport", { timeout: 20000 });
  await sleep(500);
  check(`${fl.name}: no local files, Play disabled`, await page.$eval("#btnPlay", (e: any) => e.disabled));
  const privacy = await text(page, "#ovWin");
  check(`${fl.name}: the page says files are processed locally and never uploaded`, /never uploaded/i.test(privacy) && /nothing is uploaded/i.test(privacy) && /\.cue/.test(privacy));
  check(`${fl.name}: first visit shows step 1 only, no error or warning`, await page.evaluate(() => !document.querySelector("#overlay .err, #overlay .warn") && document.getElementById("ovAfter")!.hidden));
  await shot(page, `${fl.name}-1-overlay.png`);
  const baseline = rssTree().total;
  peak = 0;
  if (fl.drop) {
    // put the real files into an input of the page to get File objects, then fire a drop event with them
    await page.evaluate(() => { const i = document.createElement("input"); i.type = "file"; i.multiple = true; i.id = "testFiles"; document.body.append(i); });
    await (await page.$("#testFiles"))!.uploadFile(...fl.files);
    await page.evaluate(() => {
      const files = (document.getElementById("testFiles") as HTMLInputElement).files!;
      const dt = new DataTransfer(); for (const f of Array.from(files)) dt.items.add(f);
      const ov = document.getElementById("overlay")!;
      for (const t of ["dragenter", "dragover", "drop"]) ov.dispatchEvent(new DragEvent(t, { dataTransfer: dt, bubbles: true, cancelable: true }));
    });
  } else {
    await (await page.$("#discPick"))!.uploadFile(...fl.files);
  }
  const st = await waitImport(page, `${fl.name}-2-importing.png`);
  check(`${fl.name}: import finished`, st.cls.includes("ok"), st.st);
  const stats = await page.evaluate(() => { const s = (window as any).snatcherImport; return { runs: s.runs, ms: s.lastMs }; });
  console.log(`  import time (pick -> Play enabled): ${(stats.ms / 1000).toFixed(1)} s; browser RSS: idle ${baseline.toFixed(0)} MB, peak during the import ${peak.toFixed(0)} MB (${peakTop})`);
  const importPeak = peak;
  await page.waitForSelector("#btnPlay:not([disabled])", { timeout: 10000 });
  await shot(page, `${fl.name}-3-imported.png`);
  if (fl.name === "single-bin") {
    await dshot(page, "import-imported-ready.png");
    const lab = await page.evaluate(() => ({ step1: (document.getElementById("ovStep1") as HTMLElement).hidden, stage: document.getElementById("ovWin")!.dataset.stage, st: document.getElementById("srcStatus")!.textContent, c: document.getElementById("btnClear")!.textContent, ch: document.getElementById("btnClear")!.hidden }));
    check(`${fl.name}: once imported the import part is hidden, the result is told under Play and Remove imported data is offered`, lab.step1 && lab.stage === "imported" && /Imported in/.test(lab.st ?? "") && lab.c === "Remove imported data" && !lab.ch, JSON.stringify(lab));
    check(`${fl.name}: Play is focused when the import ends`, await page.evaluate(() => document.activeElement?.id === "btnPlay"));
  }
  check(`${fl.name}: after the import step 2 (Play) is shown`, await page.evaluate(() => !document.getElementById("ovAfter")!.hidden));
  const summary = await text(page, "#srcStatus");
  check(`${fl.name}: Play tells the imported disc (Snatcher (USA)) and the import result`, /Imported in/.test(summary) && /Snatcher \(USA\)/.test(summary), summary.slice(0, 120));
  const storageLine = await text(page, "#impStorage");
  check(`${fl.name}: storage used is shown`, /Browser storage: .* used of .*Imported data: .*Persistent storage:/.test(storageLine), storageLine);

  // play: boot sequence of a new game with sound
  await page.click("#btnPlay");
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await waitFrames(page, 120);
  check(`${fl.name}: game runs from the imported data`, true, `frame ${await frame(page)}`);
  await audioIsBitExact(page, fl.name);
  // FM background rendering
  const fmT0 = Date.now();
  await page.waitForFunction(() => (window as any).snatcherImport.worker.fmComplete, { timeout: 900000, polling: 500 });
  const fmInfo = await page.evaluate(() => { const w = (window as any).snatcherImport.worker; return { ms: w.fmMs, total: w.fmTotal, heapImport: w.importHeap, heapFm: w.fmHeap }; });
  console.log(`  worker wasm heap: after the import ${(fmInfo.heapImport / 2 ** 20).toFixed(0)} MB, after the FM rendering ${(fmInfo.heapFm / 2 ** 20).toFixed(0)} MB`);
  console.log(`  FM sounds rendered in the browser (worker, background): ${fmInfo.total} sounds in ${(fmInfo.ms / 1000).toFixed(1)} s (waited ${((Date.now() - fmT0) / 1000).toFixed(0)} s more after Play)`);
  check(`${fl.name}: all FM sounds are cached`, fmInfo.total === 122);
  console.log(`  peak RSS over import + FM rendering + game: ${peak.toFixed(0)} MB (${peakTop})`);
  results[fl.name] = { importMs: Math.round(stats.ms), fmMs: Math.round(fmInfo.ms), wasmHeapImportMB: Math.round(fmInfo.heapImport / 2 ** 20), wasmHeapFmMB: Math.round(fmInfo.heapFm / 2 ** 20), idleRssMB: Math.round(baseline), importPeakRssMB: Math.round(importPeak), overallPeakRssMB: Math.round(peak) };
  const cache = await page.evaluate(async () => {
    const d = await (await navigator.storage.getDirectory()).getDirectoryHandle("snatcher-import-v1");
    let n = 0, total = 0;
    const walk = async (dir: any) => { for await (const [, h] of dir.entries()) { if (h.kind === "file") { n++; total += (await h.getFile()).size; } else await walk(h); } };
    await walk(d);
    return { n, total, est: await navigator.storage.estimate(), persisted: await navigator.storage.persisted() };
  });
  console.log(`  cache: ${cache.n} files, ${(cache.total / 2 ** 20).toFixed(0)} MB; storage estimate usage ${((cache.est.usage ?? 0) / 2 ** 20).toFixed(0)} MB, persisted=${cache.persisted}`);
  results[fl.name].cacheMB = cache.total / 2 ** 20;
  check(`${fl.name}: cache holds the pack, voices, FM and (kept) CD audio`, cache.total > 1.0e9 && cache.n > 120, `${cache.n} files`);

  // let the game run a while with music, then screenshot
  await waitFrames(page, 240);
  const snd = await page.evaluate(() => { const a = (window as any).snatcher.audio; return { state: a.ctx?.state, music: !!a.fmMusic || !!a.cdSrc, cached: a.cache.size, missing: [...a.missing] }; });
  check(`${fl.name}: audio context running, nothing missing`, snd.state === "running" && snd.missing.length === 0, JSON.stringify(snd));
  await shot(page, `${fl.name}-4-title.png`);
  if (fl.name === "single-bin") {
    await dshot(page, "import-playing-toolbar.png");
    await page.keyboard.press("Escape");
    await dshot(page, "import-paused-panel.png");
    await page.keyboard.press("Escape");
  }

  if (!fl.reloadCheck) {
    // ---- clear the site: imported disc + every snatcher.* key, then a first visit
    await page.goto(`http://localhost:${port}/`, { waitUntil: "load" }); // back to the start window (the game is running)
    await page.waitForSelector("#btnPlay:not([disabled])", { timeout: 30000 }); await sleep(500);
    await page.evaluate(() => { localStorage.setItem("snatcher.test", "1"); localStorage.setItem("snatcher.save.7", "AAAA"); localStorage.setItem("other.app", "kept"); });
    await page.click("#wipeBox > summary"); await sleep(100);
    await page.evaluate(() => document.getElementById("wipeBox")!.scrollIntoView({ block: "center" }));
    await dshot(page, "import-wipe-open.png");
    await page.click("#langOvPt"); await sleep(300);
    await dshot(page, "import-wipe-open-pt.png");
    await page.click("#langOvEn"); await sleep(300);
    await page.click("#btnWipe");
    check(`${fl.name}: clearing the site needs a second click`, /again/i.test(await text(page, "#btnWipe")));
    await Promise.all([page.waitForNavigation({ waitUntil: "load", timeout: 30000 }), page.click("#btnWipe")]);
    await page.waitForSelector("#btnImport", { timeout: 20000 }); await sleep(500);
    const w = await page.evaluate(async () => {
      let opfs = true; try { await (await navigator.storage.getDirectory()).getDirectoryHandle("snatcher-import-v1"); } catch { opfs = false; }
      return { opfs, keys: Object.keys(localStorage).filter((k) => k.startsWith("snatcher")) /* the fresh page may write its defaults again (snatcher.smooth) */, other: localStorage.getItem("other.app"), first: (document.getElementById("ovAfter") as HTMLElement).hidden && !(document.getElementById("ovStep1") as HTMLElement).hidden };
    });
    check(`${fl.name}: "clear everything" removes the imported disc and the site's keys, then opens as a first visit`, !w.opfs && !w.keys.includes("snatcher.test") && !w.keys.some((k) => k.startsWith("snatcher.save.")) && w.other === "kept" && w.first, JSON.stringify(w));
    await shot(page, `${fl.name}-5-cleared.png`);
  }
  if (fl.reloadCheck) {
    // ---- reload: no new import, plays from the cache
    const before = requests.length;
    await page.reload({ waitUntil: "load" });
    await page.waitForSelector("#btnPlay:not([disabled])", { timeout: 30000 });
    const s2 = await text(page, "#srcStatus"), i2 = await text(page, "#impStatus");
    check(`${fl.name}: after reload the imported disc is found and Play is enabled without picking files`, /imported disc is in this browser/i.test(s2) && /imported disc is in this browser/i.test(i2), `${s2.slice(0, 60)} | ${i2}`);
    check(`${fl.name}: after reload only Play and the rest are shown, not the import`, await page.evaluate(() => (document.getElementById("ovStep1") as HTMLElement).hidden && !(document.getElementById("ovAfter") as HTMLElement).hidden));
    await sleep(500); // the start window opens in 320 ms
    await shot(page, `${fl.name}-5-reloaded.png`);
    await dshot(page, "import-cached-ready.png");
    await page.setViewport({ width: 360, height: 740 });
    await sleep(200);
    await dshot(page, "import-mobile-cached-ready.png");
    check(`${fl.name}: 360 px cached state has no horizontal scroll`, await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.setViewport({ width: 1000, height: 820 });
    await page.goto(`http://localhost:${port}/?newgame`, { waitUntil: "load" });
    await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
    await waitFrames(page, 200);
    const runs = await page.evaluate(() => (window as any).snatcherImport.runs);
    check(`${fl.name}: new game (Junker HQ) from the cache, 0 imports in this page`, runs === 0, `runs=${runs}`);
    await audioIsBitExact(page, fl.name + " (cache)");
    await waitFrames(page, 300);
    const snd2 = await page.evaluate(() => { const a = (window as any).snatcher.audio; return { state: a.ctx?.state, buffers: a.cache.size, playing: !!a.fmMusic || !!a.cdSrc || !!a.voiceSrc || a.pcmSfxSrc.size > 0 || a.fmSfx.size > 0, missing: [...a.missing] }; });
    check(`${fl.name}: new game: sounds were loaded from the cache and nothing is missing`, snd2.state === "running" && snd2.buffers > 0 && snd2.missing.length === 0, JSON.stringify(snd2));
    await shot(page, `${fl.name}-6-newgame.png`);
    const assetReqs = requests.slice(before).filter((p) => p.startsWith("/assets/") && p !== "/assets/raw/filelist.tsv"); // that one is the page's probe for a served copy (404 here)
    check(`${fl.name}: nothing but the served-copy probe was requested under /assets`, assetReqs.length === 0, assetReqs.slice(0, 3).join(", "));
    // ---- remove
    await page.goto(`http://localhost:${port}/`, { waitUntil: "load" });
    await page.waitForSelector("#btnClear:not([hidden])", { timeout: 20000 });
    await sleep(500); // the start window opens in 320 ms (clip-path steps); clicks during the reveal would hit the clipped part
    await page.click("#wipeBox > summary"); await sleep(100);
    await page.click("#btnClear");
    check(`${fl.name}: removing needs a second click`, /again/i.test(await text(page, "#btnClear")));
    await page.click("#btnClear");
    await page.waitForFunction(() => /removed/i.test(document.getElementById("impStatus")!.textContent ?? ""), { timeout: 20000 });
    const left = await page.evaluate(async () => { try { await (await navigator.storage.getDirectory()).getDirectoryHandle("snatcher-import-v1"); return true; } catch { return false; } });
    check(`${fl.name}: "remove imported data" empties the cache`, !left);
    await shot(page, `${fl.name}-7-removed.png`);
    await dshot(page, "import-removed.png");
    check(`${fl.name}: after removal only step 1 is shown again`, await page.evaluate(() => document.getElementById("ovAfter")!.hidden));
    check(`${fl.name}: after removal the import button reads Import disc image again`, await page.evaluate(() => document.getElementById("btnImport")!.textContent === "Import disc image…" && !(document.getElementById("btnImport") as HTMLButtonElement).disabled));
    await page.reload({ waitUntil: "load" });
    await page.waitForSelector("#btnImport", { timeout: 20000 }); await sleep(500);
    check(`${fl.name}: after removal Play is disabled again`, await page.$eval("#btnPlay", (e: any) => e.disabled));
  }
  await page.close();
  await browser!.close();
}

// ---- C: wrong inputs (small synthetic files)
console.log("\n=== wrong inputs ===");
if (!only || only === "wrong") {
  const tmp = mkdtempSync(join(tmpdir(), "snimport-"));
  let caseNo = 0; // every case has its own directory (all of them name their image game.bin)
  const w = (n: string, d: string | Buffer) => { const dir = join(tmp, String(caseNo)); mkdirSync(dir, { recursive: true }); const p = join(dir, n); writeFileSync(p, d); return p; };
  const cleanCue = 'FILE "game.bin" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n';
  const zeros = Buffer.alloc(2352 * 40);
  const mode1 = (title: string, serial: string, region: string) => {
    const b = Buffer.alloc(2352 * 40);
    for (let s = 0; s < 40; s++) { b.fill(0xff, s * 2352 + 1, s * 2352 + 11); b[s * 2352 + 15] = 1; }
    const h = Buffer.alloc(2048, 0x20); h.write("SEGADISCSYSTEM  ", 0); h.write(title, 0x120); h.write(title, 0x150); h.write(serial, 0x180); h.write(region, 0x1f0);
    h.writeUInt32BE(0x800, 0x30); h.writeUInt32BE(0x600, 0x34);
    h.copy(b, 16);
    return b;
  };
  // (a raw MODE1 image whose first sector is blank has valid sync patterns but no SEGADISCSYSTEM)
  const cases: Array<[string, () => string[], RegExp]> = [
    ["only a .bin, no .cue", () => [w("a.bin", zeros)], /No \.cue file/],
    ["cue without its .bin", () => [w("b.cue", cleanCue)], /game\.bin/],
    ["two cues", () => [w("c1.cue", cleanCue), w("c2.cue", cleanCue), w("game.bin", zeros)], /More than one \.cue/],
    ["not a cue file", () => [w("d.cue", "hello world"), w("game.bin", zeros)], /cannot be used|no TRACK/i],
    ["not a Sega CD disc", () => [w("e.cue", cleanCue), w("game.bin", mode1("", "", "").fill(0, 16, 2064))], /not a Sega CD disc/],
    ["another Sega CD game", () => [w("f.cue", cleanCue), w("game.bin", mode1("SONIC CD", "GM MK-4407 -00", "U"))], /not Snatcher/],
    ["another edition of Snatcher", () => [w("g.cue", cleanCue), w("game.bin", mode1("SNATCHER", "GM T-95015 -00", "J"))], /another edition of Snatcher.*T-95015/],
    ["right serial, other region", () => [w("h.cue", cleanCue), w("game.bin", mode1("SNATCHER", "GM T-95035 -00", "E"))], /Wrong region code/],
    ["image cut short", () => [w("i.cue", cleanCue), w("game.bin", Buffer.alloc(2352 * 40 + 100))], /cue sheet does not match|multiple of 2352/],
    ["not a raw image (plain 2048-byte data)", () => [w("j.cue", cleanCue), w("game.bin", Buffer.alloc(2352 * 40, 7))], /not a raw 2352|not a Sega/],
  ];
  await launch();
  const page = await open(browser!);
  await page.waitForSelector("#btnImport", { timeout: 20000 });
  for (const [name, mk, re] of cases) {
    caseNo++;
    const files = mk();
    await page.evaluate(() => { const e = document.getElementById("impStatus")!; e.className = "dim"; e.textContent = ""; });
    await (await page.$("#discPick"))!.uploadFile(...files);
    const st = await waitImport(page).catch(() => ({ st: "TIMEOUT", cls: "" }));
    check(`message for: ${name}`, st.cls.includes("err") && re.test(st.st), st.st.slice(0, 200));
    if (name === "another edition of Snatcher") await shot(page, "wrong-edition.png");
    if (name === "cue without its .bin") await shot(page, "wrong-missing-bin.png");
    check(`  (${name}) nothing was left in the cache`, !(await page.evaluate(async () => { try { await (await navigator.storage.getDirectory()).getDirectoryHandle("snatcher-import-v1"); return true; } catch { return false; } })));
    check(`  (${name}) Play stays disabled`, await page.$eval("#btnPlay", (e: any) => e.disabled));
  }
  await page.close(); await browser!.close();
  rmSync(tmp, { recursive: true, force: true });
}

sampling = false;
console.log("\nresults:", JSON.stringify(results));
console.log(`console errors: ${errors}, failed checks: ${failed}`);
server?.kill();
process.exit(failed || errors ? 1 : 0);
void statSync;
