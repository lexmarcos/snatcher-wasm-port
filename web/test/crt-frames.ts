// Capture the raw 256x224 frames the display filter is judged on (work/shots/crt/frames/*.png: they show the game, gitignored, never commit them).
//   bun web/test/crt-frames.ts [outdir]      needs the local extraction (assets/raw: the dev server serves it); skips cleanly without it. Own server on CRT_PORT (5371).
// Scenes: title (dithered gold, noise), konami (bright), city (dithered skyline), dialog (portrait, text), menu (command menu), night (dark apartment, dithered),
// shoot (the shooting practice). Frame numbers are game frames, so the captures are repeatable.
import puppeteer, { type Page } from "puppeteer-core";
import { existsSync, mkdirSync, readdirSync, writeFileSync } from "fs";
import { join } from "path";
import { homedir } from "os";

const root = join(import.meta.dir, "..", "..");
const out = process.argv[2] ?? join(root, "work/shots/crt/frames");
mkdirSync(out, { recursive: true });
if (!existsSync(join(root, "assets/raw/filelist.tsv"))) { console.log("SKIP: assets/raw not present"); process.exit(0); }
const port = Number(process.env.CRT_PORT ?? 5371);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
let server: ReturnType<typeof Bun.spawn> | null = null;
const listening = async () => { try { await fetch(`http://localhost:${port}/`); return true; } catch { return false; } };
if (!(await listening())) {
  server = Bun.spawn(["bun", "server.ts"], { cwd: join(root, "web"), env: { ...process.env, PORT: String(port) }, stdout: "ignore", stderr: "ignore" });
  for (let i = 0; i < 50 && !(await listening()); i++) await sleep(100);
}
const base = join(homedir(), ".cache/puppeteer/chrome-headless-shell");
const chrome = join(base, readdirSync(base).sort().pop()!, "chrome-headless-shell-linux64/chrome-headless-shell");
const browser = await puppeteer.launch({ executablePath: chrome, args: ["--autoplay-policy=no-user-gesture-required"] });
process.on("exit", () => { try { server?.kill(); } catch { /* gone */ } try { browser.process()?.kill(); } catch { /* gone */ } });

async function open(query: string): Promise<Page> {
  const p = await browser.newPage();
  await p.setViewport({ width: 1000, height: 760 });
  // frames are identical with the filter on or off (the 2D canvas is always drawn), but keep the page cheap
  await p.evaluateOnNewDocument(() => { try { localStorage.setItem("snatcher.crt", JSON.stringify({ preset: "off" })); } catch { /* ignore */ } });
  await p.goto(`http://localhost:${port}/?${query}`, { waitUntil: "networkidle0" });
  await p.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  return p;
}
const frame = (p: Page) => p.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()).frame as number);
const waitFrame = (p: Page, f: number) => p.waitForFunction((t) => JSON.parse((window as any).snatcher.game.debug_info()).frame >= t, { timeout: 300000, polling: 50 }, f);
const ui = (p: Page) => p.evaluate(() => JSON.parse((window as any).snatcher.game.touch_ui()));
const save = async (p: Page, name: string) => {
  const d = await p.evaluate(() => (document.getElementById("screen") as HTMLCanvasElement).toDataURL("image/png"));
  writeFileSync(join(out, name + ".png"), Buffer.from(d.split(",")[1], "base64"));
  console.log("captured", name);
};
const press = (p: Page, bits: number) => p.evaluate((b) => { (window as any).snatcher.input.script = [[3, b], [2, 0]]; }, bits);

// boot: copyright, Konami logo (bright), title
{
  const p = await open("autostart");
  await waitFrame(p, 600); await save(p, "konami");
  await waitFrame(p, 1440); await save(p, "title");
  await p.close();
}
// new game: skyline, Junker HQ dialogue, command menu
{
  const p = await open("newgame");
  await waitFrame(p, 240); await save(p, "city");
  for (let i = 0; i < 800; i++) { const u = await ui(p); if (u.kind === "message" && u.ready && (await frame(p)) > 1300) break; await sleep(100); }
  await sleep(1500); await save(p, "dialog");
  await press(p, 0x20);
  for (let i = 0; i < 400; i++) { const u = await ui(p); if (u.kind === "menu" && u.ready) break; await press(p, 0x20).catch(() => 0); await sleep(500); }
  await sleep(800); await save(p, "menu");
  await p.close();
}
// night apartment
{
  const p = await open("scene=SP03:0");
  await waitFrame(p, 200); await save(p, "night");
  await p.close();
}
// shooting practice through the debug menu: SHOOTING, first entries, then aim and wait for targets
{
  const p = await open("scene=SP01:0");
  let shot = 0;
  for (let i = 0; i < 400 && shot < 4; i++) {
    await sleep(250);
    const s = await p.evaluate(() => { const g = (window as any).snatcher.game; return { u: JSON.parse(g.touch_ui()), items: JSON.parse(g.menu_items()) }; });
    const u = s.u; let bits = 0;
    if (u.kind === "message" && u.ready) bits = 0x20;
    else if (u.kind === "menu" && u.ready) { const idx = s.items.findIndex((t: string) => t.trim() === "SHOOTING"); bits = idx >= 0 && idx !== u.cur ? (idx > u.cur ? 2 : 1) : 0x20; }
    else if (u.kind === "shoot") {
      await p.evaluate(() => { const inp = (window as any).snatcher.input; inp.gunMode = true; inp.mouseX = 128; inp.mouseY = 100; });
      if (!u.drawn) bits = 0x40;
      await sleep(3500);
      if (shot++ === 1) await save(p, "shoot");
    }
    if (bits) await press(p, bits);
  }
  await p.close();
}
await browser.close();
server?.kill();
