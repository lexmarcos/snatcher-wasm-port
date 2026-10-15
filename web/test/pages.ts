// Headless check of dialogue pages (docs/i18n.md "Page breaks in the middle of a sentence"): jump to a scene, and every time the screen has settled
// (the typewriter finished, the game waits for a button) save a screenshot of the canvas and press confirm, N times.
// usage: PORT=5300 bun test/pages.ts "scene=SP02:424&lang=pt-BR" outprefix [shots=3] [skipFrames=0]
//   the shots are written to <outprefix>-1.png, <outprefix>-2.png ...; a shot is taken when the text area did not change for 45 frames; jump to one message with ?scene=SPnn:<pc> (`snt i18n-ctx` prints pc=).
//   skipFrames: frames to run before the first shot (scenes that start with an ACT card or a long animation)
import puppeteer from "puppeteer-core";
import { readdirSync } from "fs";
import { join } from "path";
import { homedir } from "os";

const base = join(homedir(), ".cache/puppeteer/chrome-headless-shell");
const chrome = join(base, readdirSync(base).sort().pop()!, "chrome-headless-shell-linux64/chrome-headless-shell");
const query = process.argv[2] ?? "scene=SP06:0&lang=pt-BR";
const prefix = process.argv[3] ?? "/tmp/pages";
const shots = Number(process.argv[4] ?? 3);
const skip = Number(process.argv[5] ?? 0);
const port = process.env.PORT ?? "5300";

const browser = await puppeteer.launch({ executablePath: chrome, args: ["--autoplay-policy=no-user-gesture-required"] });
const page = await browser.newPage();
await page.setViewport({ width: 1000, height: 760 });
let errors = 0;
page.on("pageerror", (e) => { errors++; console.log("[pageerror]", e.message); });
page.on("console", (m) => { if (m.type() === "error") { errors++; console.log("[console error]", m.text()); } });
await page.goto(`http://localhost:${port}/?${query}`, { waitUntil: "networkidle0" });
await page.waitForFunction(() => (window as any).snatcher, { timeout: 30000 });
const frame = () => page.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()).frame as number);
// hash of the text rows of the message box (not the blinking page marker)
const hash = () => page.evaluate(() => { const c = document.getElementById("screen") as HTMLCanvasElement; const d = c.getContext("2d")!.getImageData(0, 150, 216, 70).data; let h = 0; for (let i = 0; i < d.length; i += 4) h = (h * 31 + d[i] + d[i + 1] * 3 + d[i + 2] * 7) | 0; return h; });
const waitFrames = async (n: number) => { const s = await frame(); await page.waitForFunction((t) => JSON.parse((window as any).snatcher.game.debug_info()).frame >= t, { timeout: 120000, polling: 50 }, s + n); };
if (skip) await waitFrames(skip);
for (let k = 1; k <= shots; k++) {
  // wait until the picture is stable for 45 frames
  let last = await hash(), stable = 0, guard = 0;
  while (stable < 45 && guard++ < 400) { await waitFrames(5); const h = await hash(); if (h === last) stable += 5; else { stable = 0; last = h; } }
  await (await page.$("#screen"))!.screenshot({ path: `${prefix}-${k}.png` });
  console.log(`shot ${k}: frame ${await frame()} stable=${stable >= 45}`);
  await page.evaluate(() => { (window as any).snatcher.input.script = [[4, 0x20], [2, 0]]; });
  await waitFrames(8);
}
await browser.close();
console.log("errors:", errors);
process.exit(errors ? 1 : 0);
