// Speed of the display filter on the machine's real GPU (not part of test-web.sh: the numbers depend on the machine).
//   bun web/test/crt-perf.ts [frames]            env CRT_PERF_CHROME=/usr/bin/google-chrome (default: that), CRT_PERF_HEADED=1 opens a window on DISPLAY
// Renders the "dialog" frame through each style at 1080p, 1440p and a phone sized surface, upload + draw + gl.finish, and prints ms per frame, the CPU time of the
// GL calls, and the GPU time from EXT_disjoint_timer_query_webgl2 where the browser exposes it. Needs work/shots/crt/frames (test/crt-frames.ts).
import puppeteer from "puppeteer-core";
import { existsSync, readdirSync } from "fs";
import { join } from "path";

const root = join(import.meta.dir, "..", "..");
const framesDir = join(root, "work/shots/crt/frames");
if (!existsSync(join(framesDir, "dialog.png"))) { console.log("SKIP: no frames (run test/crt-frames.ts)"); process.exit(0); }
const frames = Number(process.argv[2] ?? 300);
const built = await Bun.build({ entrypoints: [join(import.meta.dir, "crt-harness.ts")], target: "browser" });
const js = await built.outputs[0].text();
const port = Number(process.env.CRT_HARNESS_PORT ?? 5372);
const server = Bun.serve({ port, async fetch(req) {
  const p = new URL(req.url).pathname;
  if (p === "/") return new Response(Bun.file(join(import.meta.dir, "crt-harness.html")));
  if (p === "/harness.js") return new Response(js, { headers: { "Content-Type": "text/javascript" } });
  if (p.startsWith("/frames/")) return new Response(Bun.file(join(framesDir, p.slice(8))));
  return new Response("no", { status: 404 });
} });
const exe = process.env.CRT_PERF_CHROME ?? "/usr/bin/google-chrome";
const headed = !!process.env.CRT_PERF_HEADED;
const browser = await puppeteer.launch({ executablePath: exe, headless: headed ? false : ("new" as any), args: ["--ignore-gpu-blocklist", "--enable-gpu-rasterization", "--no-sandbox", "--enable-webgl-developer-extensions", ...(process.env.CRT_PERF_ARGS?.split(" ") ?? [])] });
process.on("exit", () => { try { browser.process()?.kill(); } catch { /* gone */ } });
const page = await browser.newPage();
page.on("pageerror", (e) => console.log("[pageerror]", e.message));
await page.goto(`http://localhost:${port}/`);
await page.waitForFunction(() => (window as any).crtReady, { timeout: 20000 });
let renderer = "";
for (const [w, h, label] of [[1920, 1080, "1080p"], [2560, 1440, "1440p"], [3840, 2160, "4K"], [1071, 2379, "phone portrait (DPR 2.6)"]] as const) {
  for (const mode of ["scan", "tv", "pvm"]) {
    const r: any = await page.evaluate((a) => (window as any).crtBench(a), { frame: "dialog", mode, w, h, frames });
    renderer = r.renderer;
    console.log(`${label.padEnd(26)} ${mode.padEnd(4)} scale ${r.scale.toFixed(2)}  ${r.ms.toFixed(3)} ms/frame (upload + draw + finish)  CPU calls ${r.cpuMs.toFixed(3)} ms  GPU timer ${r.gpuMs === null ? "n/a" : r.gpuMs.toFixed(3) + " ms"}`);
  }
}
console.log("renderer:", renderer);
await browser.close(); server.stop(); process.exit(0);
