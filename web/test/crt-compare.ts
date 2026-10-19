// The comparison harness for the display filter: renders the captured frames (test/crt-frames.ts) through Off and every preset at several output sizes,
// writes full pictures, 1:1 crops (mask and scanline structure), one Off | TV | PVM strip image per scene, and a luminance table.
//   bun web/test/crt-compare.ts [outdir] [--quick]      default work/shots/crt (gitignored: the pictures show the game, never commit them)
//   env CRT_ONLY=scene1,scene2   CRT_SIZES=1920x1080,...   CRT_MODES=off,tv
// Uses software GL in headless Chrome (deterministic); no game needed beyond the captured frames.
import puppeteer from "puppeteer-core";
import { existsSync, mkdirSync, readdirSync, writeFileSync } from "fs";
import { join } from "path";
import { homedir } from "os";

const root = join(import.meta.dir, "..", "..");
const out = process.argv[2] && !process.argv[2].startsWith("--") ? process.argv[2] : join(root, "work/shots/crt");
const quick = process.argv.includes("--quick");
const framesDir = join(out, "frames");
mkdirSync(out, { recursive: true });
if (!existsSync(framesDir) || !readdirSync(framesDir).some((f) => f.endsWith(".png"))) { console.log(`SKIP: no frames in ${framesDir} (run test/crt-frames.ts)`); process.exit(0); }
const port = Number(process.env.CRT_HARNESS_PORT ?? 5372);

const built = await Bun.build({ entrypoints: [join(import.meta.dir, "crt-harness.ts")], target: "browser" });
if (!built.success) { console.error(built.logs.join("\n")); process.exit(1); }
const js = await built.outputs[0].text();
const server = Bun.serve({
  port,
  async fetch(req) {
    const p = new URL(req.url).pathname;
    if (p === "/") return new Response(Bun.file(join(import.meta.dir, "crt-harness.html")));
    if (p === "/harness.js") return new Response(js, { headers: { "Content-Type": "text/javascript" } });
    if (p.startsWith("/frames/")) { const f = Bun.file(join(framesDir, p.slice(8))); return (await f.exists()) ? new Response(f) : new Response("no", { status: 404 }); }
    return new Response("no", { status: 404 });
  },
});
const base = join(homedir(), ".cache/puppeteer/chrome-headless-shell");
const chrome = join(base, readdirSync(base).sort().pop()!, "chrome-headless-shell-linux64/chrome-headless-shell");
const browser = await puppeteer.launch({ executablePath: chrome });
process.on("exit", () => { try { browser.process()?.kill(); } catch { /* gone */ } });
const page = await browser.newPage();
page.on("pageerror", (e) => console.log("[pageerror]", e.message));
page.on("console", (m) => { if (m.type() === "error" || m.type() === "warning") console.log("[console]", m.text()); });
await page.goto(`http://localhost:${port}/`);
await page.waitForFunction(() => (window as any).crtReady, { timeout: 20000 });

const scenes = (process.env.CRT_ONLY?.split(",") ?? readdirSync(framesDir).filter((f) => f.endsWith(".png")).map((f) => f.slice(0, -4))).sort();
const sizeSpecs = process.env.CRT_SIZES?.split(",") ?? (quick ? ["1920x1080"] : ["1280x720", "1920x1080", "2560x1440", "1071x2379", "2379x1071"]);
const modes = process.env.CRT_MODES?.split(",") ?? ["off", "tv", "pvm", "scan"];
// where to look closely, in game pixels (face / text / texture)
const FOCUS: Record<string, { face: [number, number]; text: [number, number] }> = {
  dialog: { face: [170, 70], text: [60, 182] }, title: { face: [128, 60], text: [128, 186] }, menu: { face: [170, 70], text: [40, 170] }, city: { face: [128, 60], text: [60, 100] },
  night: { face: [80, 60], text: [60, 182] }, konami: { face: [128, 60], text: [128, 160] }, shoot: { face: [128, 70], text: [20, 160] },
};
const rnd = (o: any) => page.evaluate((a) => (window as any).crtRender(a), o) as Promise<{ png: string; scale: number; picture: { x: number; y: number; w: number; h: number }; meanLin: number; meanY: number; ms: number }>;
const write = (name: string, url: string) => writeFileSync(join(out, name), Buffer.from(url.split(",")[1], "base64"));
const lum: Record<string, Record<string, { lin: number; y: number }>> = {};

for (const size of sizeSpecs) {
  const [w, h] = size.split("x").map(Number);
  const phone = Math.min(w, h) < 1200 && size.includes("1071") || size.includes("2379");
  const mx = phone ? 21 : 0, my = phone ? 21 : 0;
  for (const scene of scenes) {
    const strips: string[] = [];
    for (const mode of modes) {
      const r = await rnd({ frame: scene, mode, w, h, mx, my });
      write(`${scene}-${mode}-${size}.png`, r.png);
      if (size === "1920x1080") {
        (lum[scene] ??= {})[mode] = { lin: r.meanLin, y: r.meanY };
        const f = FOCUS[scene] ?? { face: [128, 100], text: [128, 180] };
        for (const [what, [gx, gy]] of Object.entries(f)) {
          const sx = r.picture.w / 256, sy = r.picture.h / 224;
          const cx = Math.round(r.picture.x + gx * sx - 100), cy = Math.round(r.picture.y + gy * sy - 100);
          const c = await rnd({ frame: scene, mode, w, h, mx, my, crop: { x: cx, y: cy, w: 200, h: 200, zoom: 3 } });
          write(`${scene}-${mode}-crop-${what}.png`, c.png);
        }
        if (["off", "tv", "pvm"].includes(mode)) {
          const cx = (FOCUS[scene]?.face[0] ?? 128);
          const s = await rnd({ frame: scene, mode, w, h, mx, my, cols: [Math.max(0, cx - 58), Math.min(256, cx + 58)] });
          strips.push(s.png);
        }
      }
    }
    if (strips.length === 3) write(`compare-${scene}.png`, await page.evaluate((u) => (window as any).crtJoin(u), strips));
  }
}
const names = Object.keys(lum).sort();
const line = (s: string, v: (m: string) => string) => `${s.padEnd(8)}${modes.map((m) => v(m).padStart(9)).join("")}`;
if (names.length) {
  console.log("\nmean linear luminance of the picture area at 1920x1080 (ratio to Off):");
  console.log(line("scene", (m) => m));
  for (const s of names) console.log(line(s, (m) => (lum[s][m] ? `${lum[s][m].lin.toFixed(4)} (${(lum[s][m].lin / lum[s].off.lin).toFixed(2)})` : "-")));
  writeFileSync(join(out, "lum.json"), JSON.stringify(lum, null, 1));
}
await browser.close(); server.stop(); process.exit(0);
