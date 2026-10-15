// End-to-end UI checks in headless Chrome (needs the dev server: bun web/server.ts, PORT env = 5300 by default).
//   bun test/ui.ts [outdir]      screenshots go to outdir (default /tmp)
// Covers: start overlay (served + picked folder with the server's /assets blocked), menu pause, volume/mono/LRU
// audio paths, hidden-tab pause, save import/export, phone width. Exits 1 on any failed check or console error.
import puppeteer, { type Page } from "puppeteer-core";
import { readdirSync, writeFileSync, mkdtempSync } from "fs";
import { join } from "path";
import { homedir, tmpdir } from "os";

const port = process.env.PORT ?? "5300";
const out = process.argv[2] ?? "/tmp";
const assets = join(import.meta.dir, "..", "..", "assets");
const base = join(homedir(), ".cache/puppeteer/chrome-headless-shell");
const chrome = join(base, readdirSync(base).sort().pop()!, "chrome-headless-shell-linux64/chrome-headless-shell");

let failed = 0, errors = 0;
const check = (name: string, ok: boolean, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"} ${name}${extra ? " – " + extra : ""}`); if (!ok) failed++; };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const browser = await puppeteer.launch({ executablePath: chrome, args: ["--autoplay-policy=no-user-gesture-required"] });

async function open(width: number, height: number, blockAssets: boolean | RegExp = false, query = ""): Promise<Page> {
  const page = await browser.newPage();
  await page.setViewport({ width, height });
  page.on("console", (m) => { if (m.type() === "error" && !(blockAssets && /ERR_FAILED/.test(m.text())) && !/404/.test(m.text())) { errors++; console.log("[console error]", m.text()); } });
  page.on("pageerror", (e) => { errors++; console.log("[pageerror]", e.message); });
  if (blockAssets) {
    await page.setRequestInterception(true);
    page.on("request", (r) => (new URL(r.url()).pathname.startsWith("/assets/") && (blockAssets === true || (blockAssets instanceof RegExp && blockAssets.test(r.url()))) ? r.abort() : r.continue()));
  }
  await page.goto(`http://localhost:${port}/${query}`, { waitUntil: "load" });
  return page;
}
const frame = (page: Page) => page.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()).frame as number);
const waitFrames = async (page: Page, n: number) => { const s = await frame(page); await page.waitForFunction((t) => JSON.parse((window as any).snatcher.game.debug_info()).frame >= t, { timeout: 60000, polling: 100 }, s + n); };
const visible = (page: Page, sel: string) => page.$eval(sel, (e) => !(e as HTMLElement).hidden && getComputedStyle(e).display !== "none");

// ---- A: served assets, overlay → play → menu ----
{
  const page = await open(1000, 760);
  await page.waitForSelector("#btnPlay:not([disabled])", { timeout: 15000 });
  check("overlay shown with Play enabled", await visible(page, "#overlay"));
  check("Play is focused", await page.evaluate(() => document.activeElement?.id === "btnPlay"));
  await page.screenshot({ path: join(out, "ui-overlay.png") });
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  check("overlay hidden after start", !(await visible(page, "#overlay")));
  await waitFrames(page, 30);
  // menu pauses the game
  await page.keyboard.press("Escape");
  check("Esc opens the menu", await visible(page, "#panel"));
  const f0 = await frame(page); await sleep(500);
  check("game paused while the menu is open", (await frame(page)) === f0);
  await page.$eval("#volMusic", (e: any) => { e.value = "40"; e.dispatchEvent(new Event("input")); });
  await page.$eval("#volFx", (e: any) => { e.value = "0"; e.dispatchEvent(new Event("input")); });
  const vols = await page.evaluate(() => { const a = (window as any).snatcher.audio; a.ensure(); return [a.getVolume("music"), a.getVolume("fx"), a.nodes.cdBus.gain.value, a.nodes.fmSfxBus.gain.value]; });
  check("volume sliders drive the buses", Math.abs(vols[0] - 0.4) < 1e-6 && vols[1] === 0 && Math.abs(vols[2] - 0.16) < 1e-6 && vols[3] === 0, JSON.stringify(vols));
  await page.screenshot({ path: join(out, "ui-menu.png") });
  await page.keyboard.press("Escape");
  check("Esc closes the menu", !(await visible(page, "#panel")));
  await waitFrames(page, 10);
  check("game resumes", true);
  // hidden tab pauses
  await page.evaluate(() => { Object.defineProperty(document, "hidden", { configurable: true, get: () => true }); });
  await sleep(200); const f1 = await frame(page); await sleep(500);
  check("game paused while the tab is hidden", (await frame(page)) === f1);
  await page.evaluate(() => { delete (document as any).hidden; });
  await waitFrames(page, 10);
  check("game resumes when visible again", true);
  // sound mode + stop commands
  const mode = await page.evaluate(async () => {
    const a = (window as any).snatcher.audio; a.ensure();
    await a.fmCmd(0xf3, false); await a.pcmSfx(0xfc, 0);
    const mono = [a.mono.fm, a.mono.pcm, a.nodes.fmMix.channelCount, a.nodes.pcmMix.channelCount];
    await a.fmCmd(0xf4, false); await a.pcmSfx(0xfd, 0);
    const st = [a.mono.fm, a.mono.pcm, a.nodes.fmMix.channelCount, a.nodes.pcmMix.channelCount];
    return { mono, st };
  });
  check("F3/FC give mono (1 channel mix), F4/FD stereo", JSON.stringify(mode) === JSON.stringify({ mono: [true, true, 1, 1], st: [false, false, 2, 2] }), JSON.stringify(mode));
  const dm = await page.evaluate(async () => { // same node settings as AudioOut's mix nodes, rendered offline: L=1,R=0 in
    const res: number[][] = [];
    for (const mono of [true, false]) {
      const oc = new OfflineAudioContext(2, 128, 48000);
      const buf = oc.createBuffer(2, 128, 48000); buf.getChannelData(0).fill(1);
      const src = oc.createBufferSource(); src.buffer = buf;
      const mix = oc.createGain(); mix.channelCountMode = "explicit"; mix.channelInterpretation = "speakers"; mix.channelCount = mono ? 1 : 2;
      src.connect(mix); mix.connect(oc.destination); src.start();
      const r = await oc.startRendering(); res.push([r.getChannelData(0)[64], r.getChannelData(1)[64]]);
    }
    return res;
  });
  check("mono mix node downmixes to 0.5 (L+R) on both outputs, stereo keeps the pan", JSON.stringify(dm) === "[[0.5,0.5],[1,0]]", JSON.stringify(dm));
  const stops = await page.evaluate(async () => {
    const a = (window as any).snatcher.audio; a.ensure();
    const id = Object.keys(a.fm.sounds).find((k) => a.fm.sounds[k].music)!;
    await a.fmCmd(Number(id), true); const playing = !!a.fmMusic;
    await a.fmCmd(0xf2, false); const afterF2 = !!a.fmMusic;
    await a.fmCmd(Number(id), true); await a.fmCmd(0xf1, false); const afterF1 = !!a.fmMusic;
    const vid = Object.keys(a.pcm.voices)[0];
    await a.voice(Number(vid)); const v = !!a.voiceSrc; await a.pcmSfx(0xff, 0); const afterFF = !!a.voiceSrc;
    // stop while the file is still loading must cancel the start
    a.cache = new (a.cache.constructor)(); const id2 = Object.keys(a.fm.sounds).find((k) => a.fm.sounds[k].music && k !== id)!;
    const pending = a.fmCmd(Number(id2), true); await a.fmCmd(0xf2, false); await pending; const cancelled = !a.fmMusic;
    return { playing, afterF2, afterF1, v, afterFF, cancelled };
  });
  check("FM F2/F1 and PCM FF stop commands work (also during a pending load)", JSON.stringify(stops) === JSON.stringify({ playing: true, afterF2: false, afterF1: false, v: true, afterFF: false, cancelled: true }), JSON.stringify(stops));
  // LRU cap
  const lru = await page.evaluate(async () => {
    const a = (window as any).snatcher.audio; a.ensure();
    a.cache = new (a.cache.constructor)(8 * 2 ** 20, 5);
    const ids = Object.keys(a.pcm.voices).slice(0, 12);
    for (const id of ids) await a.load(`audio/web/${a.pcm.voices[id].file}`);
    await new Promise((r) => setTimeout(r, 50));
    return { size: a.cache.size, mb: a.cache.total / 2 ** 20 };
  });
  check("buffer cache stays under its cap", lru.size <= 5 && lru.mb <= 8 + 4, JSON.stringify(lru));
  // saves
  const blk = Buffer.alloc(0x1c0, 7).toString("base64");
  const sav = join(mkdtempSync(join(tmpdir(), "snsave-")), "s.json");
  writeFileSync(sav, JSON.stringify({ format: "snatcher-web-saves", version: 1, slots: { "2": blk, "9": blk, "3": "AAAA" } }));
  await page.keyboard.press("Escape");
  await (await page.$("#importFile"))!.uploadFile(sav);
  await sleep(300);
  const imp = await page.evaluate(() => { const s = (window as any).snatcher.store; return { two: s.get(2) !== undefined, three: s.get(3) !== undefined, note: document.getElementById("pnNotes")!.textContent }; });
  check("import accepts the valid slot, rejects the bad ones", imp.two && !imp.three && /slot\(s\) 2/.test(imp.note ?? "") && /slot 9/.test(imp.note ?? "") && /slot 3/.test(imp.note ?? ""), imp.note ?? "");
  const rt = await page.evaluate(() => { const s = (window as any).snatcher.store; const j = s.exportJson(); return JSON.parse(j).slots["2"] === s.get(2); });
  check("export round-trips", rt);
  await page.close();
}

// ---- B: picked folder only (the server's /assets is blocked after the page loaded) ----
{
  const page = await open(1000, 760, true);
  await page.waitForSelector("#btnPick", { timeout: 15000 });
  await sleep(500);
  check("no local files: Play disabled, note visible", await page.$eval("#btnPlay", (e: any) => e.disabled) && /No local game files/.test(await page.$eval("#srcStatus", (e) => e.textContent ?? "")));
  // a directory path makes Chrome fill the input like a real folder pick (with webkitRelativePath)
  const track = "track02.ogg";
  await (await page.$("#pick"))!.uploadFile(assets);
  await page.waitForSelector("#btnPlay:not([disabled])", { timeout: 15000 });
  console.log("picked summary:", (await page.$eval("#srcStatus", (e) => e.textContent ?? "")).slice(0, 200));
  await page.click("#btnPlay");
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await waitFrames(page, 60);
  check("game runs from picked files", true, `frame ${await frame(page)}`);
  const au = await page.evaluate(async (t) => {
    const a = (window as any).snatcher.audio;
    const buf = await a.load(`web/music/${t}`);
    const miss = await a.load("web/music/track99.ogg");
    return { manifests: !!a.pcm && !!a.fm, track: !!buf && buf.duration > 1, missing: miss === null };
  }, track);
  check("audio manifests and a CD track load from picked files", au.manifests && au.track && au.missing, JSON.stringify(au));
  await page.screenshot({ path: join(out, "ui-picked.png") });
  await page.close();
}

// ---- D: optional assets missing (header.bin, ip.bin, audio manifests): visible note, game still runs ----
{
  const page = await open(1000, 760, /header\.bin|ip\.bin|pcm\.json|fm\.json/, "?autostart");
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await waitFrames(page, 60);
  check("game runs without header/ip/manifests", true);
  await page.keyboard.press("Escape");
  const t = await page.$eval("#pnNotes", (e) => e.textContent ?? "");
  check("menu lists the missing optional assets", /header\.bin \/ ip\.bin not found/.test(t) && /manifests missing/.test(t), t);
  await page.screenshot({ path: join(out, "ui-degraded.png") });
  // engine events keep flowing with no manifests: nothing throws
  await page.evaluate(async () => { const s = (window as any).snatcher; for (const e of [{ t: "fm", id: 0x3f, music: true }, { t: "voice", id: 40 }, { t: "pcmsfx", id: 5, bank: 1 }, { t: "music", track: 99 }, { t: "music_stop" }]) s.handleEvent(e); });
  await sleep(300);
  // a save event reaches the store and the engine
  const blk = Buffer.alloc(0x1c0, 9).toString("base64");
  const sv = await page.evaluate((b) => { const s = (window as any).snatcher; s.handleEvent({ t: "save", slot: 1, data: b }); return [s.store.get(1) === b, localStorage.getItem("snatcher.save.1") === b]; }, blk);
  check("save event stored (memory + localStorage)", sv[0] && sv[1]);
  await page.evaluate(() => localStorage.removeItem("snatcher.save.1"));
  await page.close();
}

// ---- C: phone width ----
{
  const page = await open(360, 640);
  await page.waitForSelector("#btnPlay:not([disabled])", { timeout: 15000 });
  const noScroll = await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth);
  check("overlay fits phone width without horizontal scroll", noScroll);
  await page.screenshot({ path: join(out, "ui-phone-overlay.png") });
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await waitFrames(page, 30);
  await page.keyboard.press("Escape");
  check("menu fits phone width", await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  await page.screenshot({ path: join(out, "ui-phone-menu.png") });
  await page.keyboard.press("Escape");
  const cw = await page.$eval("#screen", (e) => (e as HTMLElement).getBoundingClientRect().width);
  check("canvas fits phone width", cw <= 360, `${cw}px`);
  await page.screenshot({ path: join(out, "ui-phone-game.png") });
  await page.close();
}

console.log(`console errors: ${errors}, failed checks: ${failed}`);
await browser.close();
process.exit(failed || errors ? 1 : 0);
