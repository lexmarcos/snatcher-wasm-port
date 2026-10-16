// Headless check of the voice subtitles (docs/voice-subtitles.md): open a scene, wait for a clip with a subtitle and save screenshots of the canvas
// while it is up (one every `step` frames, up to `shots`).
// the shot is taken once the subtitle is drawn (it waits for the portraits of the clip), with the layout in the log line.
// usage: PORT=5300 bun test/vsub.ts "scene=SP06:032e&lang=pt-BR" outprefix [shots=4] [step=60] [skipFrames=0]
//   the shots are written to <outprefix>-1.png ...; the first one is taken when the first subtitle appears (frame + 10).
import puppeteer from "puppeteer-core";
import { readdirSync } from "fs";
import { join } from "path";
import { homedir } from "os";

const base = join(homedir(), ".cache/puppeteer/chrome-headless-shell");
const chrome = join(base, readdirSync(base).sort().pop()!, "chrome-headless-shell-linux64/chrome-headless-shell");
const query = process.argv[2] ?? "scene=SP06:032e&lang=pt-BR";
const prefix = process.argv[3] ?? "/tmp/vsub";
const shots = Number(process.argv[4] ?? 4);
const step = Number(process.argv[5] ?? 60);
const skip = Number(process.argv[6] ?? 0);
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
const active = () => page.evaluate(() => (window as any).snatcher.game.voice_subtitle_active() as number);
// 0 = nothing drawn yet (the text waits for the portraits), 1 box, 2 lane, 3 band, 4 compact
const layout = () => page.evaluate(() => (window as any).snatcher.game.voice_subtitle_layout() as number);
const waitFrames = async (n: number) => { const s = await frame(); await page.waitForFunction((t) => JSON.parse((window as any).snatcher.game.debug_info()).frame >= t, { timeout: 120000, polling: 20 }, s + n); };
if (skip) await waitFrames(skip);
// confirm presses keep the script moving (the page auto-presses nothing)
await page.evaluate(() => { const s = (window as any).snatcher; const sc: [number, number][] = []; for (let i = 0; i < 400; i++) sc.push([2, 0x20], [38, 0]); s.input.script = sc; });
let found = 0, guard = 0;
while (found < shots && guard++ < 6000) {
  const a = await active();
  if (a >= 0) {
    let guard2 = 0;
    while ((await layout()) === 0 && guard2++ < 200) await waitFrames(2);
    await waitFrames(4);
    await (await page.$("#screen"))!.screenshot({ path: `${prefix}-${++found}.png` });
    console.log(`shot ${found}: clip ${a} layout ${["none", "box", "lane", "band", "compact"][await layout()]} frame ${await frame()}`);
    await waitFrames(step);
  } else await waitFrames(5);
}
await browser.close();
console.log("errors:", errors, "shots:", found);
process.exit(errors || !found ? 1 : 0);
