// Headless drift check of the CD-DA audio against the engine's CD clock (docs/voice-subtitles.md "CD-DA speech"): runs the title -> prologue (track 3), then logs
// every 5 s the engine's CD position, the audio position (AudioOut.cdOffset) and the playback rate. Real time: `seconds` of wall clock.
// usage: PORT=5300 bun test/cdsync.ts [seconds=60]   (needs the dev server on PORT and the built pack / wasm: scripts/build-web.sh)
import puppeteer from "puppeteer-core";
import { readdirSync } from "fs";
import { join } from "path";
import { homedir } from "os";

const base = join(homedir(), ".cache/puppeteer/chrome-headless-shell");
const chrome = join(base, readdirSync(base).sort().pop()!, "chrome-headless-shell-linux64/chrome-headless-shell");
const seconds = Number(process.argv[2] ?? 60);
const port = process.env.PORT ?? "5300";
const browser = await puppeteer.launch({ executablePath: chrome, args: ["--autoplay-policy=no-user-gesture-required"] });
const page = await browser.newPage();
await page.setViewport({ width: 800, height: 600 });
page.on("pageerror", (e) => console.log("[pageerror]", e.message));
await page.goto(`http://localhost:${port}/?autostart&lang=pt-BR`, { waitUntil: "networkidle0" });
await page.waitForFunction(() => (window as any).snatcher, { timeout: 30000 });
// START on NEW GAME (frame 1301) and START on the options screen (frame 1612), like engine/src/modules/title.rs `title_to_intro`
await page.evaluate(() => { (window as any).snatcher.input.script = [[1300, 0], [1, 0x80], [310, 0], [1, 0x80], [100000, 0]]; });
const sample = () => page.evaluate(() => { const s = (window as any).snatcher; const p = s.game.cd_position(); return { pos: p, off: s.audio.cdOffset(p), rate: s.audio.cdSrc?.playbackRate?.value ?? null, ctx: s.audio.ctx?.state }; });
const t0 = Date.now();
let started: number | null = null, worst = 0, n = 0;
while (Date.now() - t0 < (seconds + 40) * 1000) {
  await new Promise((r) => setTimeout(r, 1000));
  const s = await sample();
  if (s.pos < 0) continue;
  if (started === null) started = Date.now();
  if ((Date.now() - started) / 1000 >= n * 5) { n++; console.log(`pos ${s.pos.toFixed(2)} s  audio-behind ${s.off === null ? "n/a" : (s.off * 1000).toFixed(0) + " ms"}  rate ${s.rate}  ctx ${s.ctx}`); }
  if (s.off !== null && s.pos > 10) worst = Math.max(worst, Math.abs(s.off));
  if ((Date.now() - started) / 1000 > seconds) break;
}
await browser.close();
console.log("worst |engine - audio| after 10 s:", (worst * 1000).toFixed(0), "ms");
process.exit(started === null ? 1 : 0);
