// Headless check: load the page, optionally drive scripted pad input, save a screenshot.
// usage: bun test/shot.ts [url-query] [out.png] [frames-to-wait] [inputs-json]
//   inputs-json: [[frames,bits],...] fed through window.snatcher.input.script
import puppeteer from "puppeteer-core";
import { readdirSync } from "fs";
import { join } from "path";
import { homedir } from "os";

function chromePath(): string {
  const base = join(homedir(), ".cache/puppeteer/chrome-headless-shell");
  const v = readdirSync(base).sort().pop()!;
  return join(base, v, "chrome-headless-shell-linux64/chrome-headless-shell");
}

const query = process.argv[2] ?? "";
const out = process.argv[3] ?? "/tmp/snatcher-shot.png";
const frames = Number(process.argv[4] ?? 120);
const inputs = process.argv[5] ? JSON.parse(process.argv[5]) : [];
const port = process.env.PORT ?? "5300";

const browser = await puppeteer.launch({ executablePath: chromePath(), args: ["--autoplay-policy=no-user-gesture-required"] });
const page = await browser.newPage();
await page.setViewport({ width: 1000, height: 760 });
page.on("console", (m) => console.log("[page]", m.text()));
page.on("pageerror", (e) => console.log("[pageerror]", e.message));
await page.goto(`http://localhost:${port}/?${query}`, { waitUntil: "networkidle0" });
await page.waitForFunction(() => (window as any).snatcher, { timeout: 30000 });
if (inputs.length) await page.evaluate((s) => { (window as any).snatcher.input.script = s; }, inputs);
const start = await page.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()).frame);
await page.waitForFunction((f) => JSON.parse((window as any).snatcher.game.debug_info()).frame >= f, { timeout: 120000, polling: 100 }, start + frames);
const el = await page.$("canvas");
await el!.screenshot({ path: out });
console.log("debug:", await page.evaluate(() => (window as any).snatcher.game.debug_info()));
console.log("msg:", await page.evaluate(() => document.getElementById("msg")!.textContent));
await browser.close();
