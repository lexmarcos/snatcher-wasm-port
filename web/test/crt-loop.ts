// Is the page's frame loop slowed by the filter? Plays the game in headless Chrome on the real GPU (ANGLE/Vulkan flags as in crt-perf.ts) with the plain
// picture and with each style, and reports the animation frame rate, the JS time of the frame callback (whole `frame()`: ticks, 2D copy, filter calls) and the
// filter's own CPU time. Needs the game (assets/raw) and a dev server on LOOP_PORT (default 5370; start `PORT=5370 bun web/server.ts`).
//   CRT_PERF_ARGS="--enable-features=Vulkan --use-angle=vulkan --disable-vulkan-surface" bun web/test/crt-loop.ts
import puppeteer from "puppeteer-core";

const port = process.env.LOOP_PORT ?? "5370";
const exe = process.env.CRT_PERF_CHROME ?? "/usr/bin/google-chrome";
const browser = await puppeteer.launch({ executablePath: exe, headless: process.env.CRT_PERF_HEADED ? false : ("new" as any), args: ["--ignore-gpu-blocklist", "--no-sandbox", "--autoplay-policy=no-user-gesture-required", ...(process.env.CRT_PERF_ARGS?.split(" ") ?? [])] });
process.on("exit", () => { try { browser.process()?.kill(); } catch { /* gone */ } });
for (const [w, h, dpr, label] of [[1920, 1080, 1, "1080p"], [2560, 1440, 1, "1440p"]] as const) {
  for (const style of ["off", "scan", "tv", "pvm"]) {
    const page = await browser.newPage();
    await page.setViewport({ width: w, height: h, deviceScaleFactor: dpr });
    await page.goto(`http://localhost:${port}/?scene=SP06:0&crt=${style}`, { waitUntil: "load" });
    await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
    await new Promise((r) => setTimeout(r, 2500));
    const r: any = await page.evaluate(async () => {
      const sn = (window as any).snatcher, d = sn.display;
      const f0 = JSON.parse(sn.game.debug_info()).frame, s0 = d.filter ? { ...d.filter.stats } : null;
      // frame callback time: wrap requestAnimationFrame callbacks
      let n = 0, busy = 0, last = performance.now(); const gaps: number[] = [];
      const raf = window.requestAnimationFrame.bind(window);
      (window as any).requestAnimationFrame = (cb: FrameRequestCallback) => raf((t) => { const a = performance.now(); gaps.push(t - last); last = t; cb(t); busy += performance.now() - a; n++; });
      await new Promise((r) => setTimeout(r, 4000));
      const f1 = JSON.parse(sn.game.debug_info()).frame, s1 = d.filter ? { ...d.filter.stats } : null;
      gaps.shift(); gaps.sort((a, b) => a - b);
      return { fps: n / 4, jsMs: busy / n, gameFrames: f1 - f0, p95gap: gaps[Math.floor(gaps.length * 0.95)], filterMs: s0 && s1 ? (s1.cpuMs - s0.cpuMs) / Math.max(1, s1.frames - s0.frames) : null, active: d.active };
    });
    console.log(`${label} ${style.padEnd(4)} rAF ${r.fps.toFixed(1)}/s, p95 gap ${r.p95gap.toFixed(1)} ms, JS per frame ${r.jsMs.toFixed(2)} ms (filter calls ${r.filterMs === null ? "-" : r.filterMs.toFixed(2) + " ms"}), game frames in 4 s ${r.gameFrames}, filter on screen: ${r.active}`);
    await page.close();
  }
}
await browser.close(); process.exit(0);
