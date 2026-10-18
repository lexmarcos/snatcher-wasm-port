// Language switch checks in headless Chrome (needs the dev server and scripts/build-web.sh output incl. i18n/build/pt-BR.pack):
//   PORT=5301 bun test/lang.ts [outdir]
// Covers: ?lang=pt-BR (overlay in Portuguese, html lang), the EN | PT-BR toggle (persisted in localStorage, back to English),
// the engine: first message of a new game in pt-BR differs from English, switching during play, console errors.
import puppeteer, { type Page } from "puppeteer-core";
import { readdirSync } from "fs";
import { join } from "path";
import { homedir } from "os";

const port = process.env.PORT ?? "5301";
const out = process.argv[2] ?? "/tmp";
const base = join(homedir(), ".cache/puppeteer/chrome-headless-shell");
const chrome = join(base, readdirSync(base).sort().pop()!, "chrome-headless-shell-linux64/chrome-headless-shell");
let failed = 0, errors = 0;
const check = (name: string, ok: boolean, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"} ${name}${extra ? " – " + extra : ""}`); if (!ok) failed++; };
const browser = await puppeteer.launch({ executablePath: chrome, args: ["--autoplay-policy=no-user-gesture-required"] });

async function open(query: string): Promise<Page> {
  const page = await browser.newPage();
  await page.setViewport({ width: 1000, height: 760 });
  page.on("console", (m) => { if (m.type() === "error") { errors++; console.log("[console error]", m.text()); } });
  page.on("pageerror", (e) => { errors++; console.log("[pageerror]", e.message); });
  await page.goto(`http://localhost:${port}/${query}`, { waitUntil: "load" });
  return page;
}
const frame = (p: Page) => p.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()).frame as number);
const waitFrames = async (p: Page, n: number) => { const s = await frame(p); await p.waitForFunction((t) => JSON.parse((window as any).snatcher.game.debug_info()).frame >= t, { timeout: 90000, polling: 100 }, s + n); };
const canvasHash = (p: Page) => p.evaluate(() => { const c = document.getElementById("screen") as HTMLCanvasElement; const d = c.getContext("2d")!.getImageData(0, 150, 256, 74).data; let h = 0; for (let i = 0; i < d.length; i += 4) h = (h * 31 + d[i] + d[i + 1] * 3 + d[i + 2] * 7) | 0; return h; });

// ---- A: overlay in Portuguese, selector, persistence ----
{
  const page = await open("?lang=pt-BR");
  await page.waitForSelector("#btnPlay:not([disabled])", { timeout: 15000 });
  check("html lang is pt-BR", await page.evaluate(() => document.documentElement.lang) === "pt-BR");
  check("Play button text is Jogar", (await page.$eval("#btnPlay", (e) => e.textContent)) === "Jogar");
  check("overlay language toggle shows pt-BR as pressed", (await page.$eval("#langOvPt", (e) => e.getAttribute("aria-pressed"))) === "true" && (await page.$eval("#langOvEn", (e) => e.getAttribute("aria-pressed"))) === "false");
  await page.screenshot({ path: join(out, "lang-overlay-pt.png") });
  await page.click("#langOvEn");
  await new Promise((r) => setTimeout(r, 300));
  check("switching to English restores Play", (await page.$eval("#btnPlay", (e) => e.textContent)) === "Play");
  check("choice is stored", await page.evaluate(() => localStorage.getItem("snatcher.lang")) === "en");
  await page.click("#langOvPt");
  await new Promise((r) => setTimeout(r, 300));
  check("choice stored as pt-BR", await page.evaluate(() => localStorage.getItem("snatcher.lang")) === "pt-BR");
  await page.close();
  const p2 = await open("");  // no ?lang: the stored choice applies
  await p2.waitForSelector("#btnPlay:not([disabled])", { timeout: 15000 });
  check("stored language applies on the next visit", (await p2.$eval("#btnPlay", (e) => e.textContent)) === "Jogar");
  await p2.evaluate(() => localStorage.removeItem("snatcher.lang"));
  await p2.close();
}

// ---- B: the engine speaks Portuguese, switching during play ----
{
  const shot = async (lang: string) => {
    const page = await open(`?scene=SP06:0&lang=${lang}`);
    await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
    // Mika's first line: ACT card, scene, typewriter; confirm presses are not needed for the first message
    await waitFrames(page, 1500);
    const h = await canvasHash(page);
    await page.screenshot({ path: join(out, `lang-game-${lang}.png`) });
    return { page, h };
  };
  const en = await shot("en");
  const pt = await shot("pt-BR");
  check("game text differs between en and pt-BR at the same frame", en.h !== pt.h);
  const lang = await pt.page.evaluate(() => (window as any).snatcher.language());
  check("window.snatcher.language() is pt-BR", lang === "pt-BR");
  // switch back to English while playing: the engine keeps running without errors
  await pt.page.evaluate(() => (window as any).snatcher.setLanguage("en", false));
  await waitFrames(pt.page, 60);
  check("switching to English during play keeps the game running", true);
  await en.page.close(); await pt.page.close();
}
await browser.close();
check("no console or page errors", errors === 0, String(errors));
process.exit(failed ? 1 : 0);
