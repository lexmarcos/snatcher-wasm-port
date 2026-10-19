// Check of the static site (scripts/build-static.sh -> build/site) in headless Chrome, served the way GitHub Pages serves it: from a SUB-PATH,
// by a plain static server that sets no special header (no COOP/COEP, no no-cache) and answers 404 for everything it does not have.
//   bun web/test/static.ts [shots-dir]                  serves build/site at http://localhost:5350/snatcher-wasm-port/ (STATIC_PORT overrides)
//   SITE_URL=https://markzuel.com/snatcher-wasm-port/ bun web/test/static.ts      checks a live site instead (no local server)
//   FULL=1 ...                                           also imports the local disc (work/snatcher.cue + .bin) and plays; skipped without the disc
// Checks: 0 console errors, 0 failed requests, no /assets request at all, only step 1 on a first visit, pt-BR switch (strings + pack), and with FULL=1 import + Play.
// Screenshots (title of the game included) go to work/shots/pages by default, which is gitignored: never commit them.
import puppeteer, { type Browser, type Page } from "puppeteer-core";
import { existsSync, mkdirSync, readdirSync, readFileSync } from "fs";
import { join, normalize } from "path";
import { homedir } from "os";

const root = join(import.meta.dir, "..", "..");
const out = process.argv[2] ?? join(root, "work/shots/pages");
mkdirSync(out, { recursive: true });
const port = Number(process.env.STATIC_PORT ?? 5350);
const prefix = "/snatcher-wasm-port/";
const live = process.env.SITE_URL;
const url = live ?? `http://localhost:${port}${prefix}`;
const cue = join(root, "work/snatcher.cue"), bin = join(root, "work/snatcher.bin");
const full = !!process.env.FULL && existsSync(cue) && existsSync(bin);
if (process.env.FULL && !full) console.log("NOTE: FULL requested but work/snatcher.cue/.bin missing: import part skipped");

let failed = 0, problems = 0;
const check = (name: string, ok: boolean, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"} ${name}${extra ? " - " + extra : ""}`); if (!ok) failed++; };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// ---- the plain static host: only what is under build/site, only under the sub-path, the content type from the extension, nothing else in the headers
let server: ReturnType<typeof Bun.serve> | null = null;
if (!live) {
  const site = join(root, "build/site");
  if (!existsSync(join(site, "index.html"))) { console.log("FAIL build/site missing: run scripts/build-static.sh"); process.exit(1); }
  const types: Record<string, string> = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8", ".wasm": "application/wasm", ".woff2": "font/woff2", ".webmanifest": "application/manifest+json", ".png": "image/png", ".json": "application/json", ".txt": "text/plain; charset=utf-8", ".pack": "application/octet-stream" };
  server = Bun.serve({
    port,
    fetch(req) {
      const p = decodeURIComponent(new URL(req.url).pathname);
      if (p === prefix.slice(0, -1)) return new Response(null, { status: 301, headers: { Location: prefix } });
      if (!p.startsWith(prefix)) return new Response("not found", { status: 404 });
      let rel = p.slice(prefix.length); if (rel === "" || rel.endsWith("/")) rel += "index.html";
      const f = normalize(join(site, rel));
      if (!f.startsWith(site) || !existsSync(f)) return new Response("not found", { status: 404 });
      const ext = f.slice(f.lastIndexOf("."));
      return new Response(readFileSync(f), { headers: { "Content-Type": types[ext] ?? "application/octet-stream" } });
    },
  });
}

const base = join(homedir(), ".cache/puppeteer/chrome-headless-shell");
const chrome = join(base, readdirSync(base).sort().pop()!, "chrome-headless-shell-linux64/chrome-headless-shell");
let browser: Browser | undefined;
process.on("exit", () => { try { server?.stop(true); } catch { /* gone */ } try { browser?.process()?.kill(); } catch { /* gone */ } });
browser = await puppeteer.launch({ executablePath: chrome, args: ["--autoplay-policy=no-user-gesture-required"] });

const seen = new Map<string, number>();
async function open(query = ""): Promise<Page> {
  const page = await browser!.newPage();
  await page.setViewport({ width: 1000, height: 820 });
  page.on("console", (m) => { if (m.type() === "error" || m.type() === "warn") { problems++; console.log(`[console ${m.type()}]`, m.text()); } });
  page.on("pageerror", (e) => { problems++; console.log("[pageerror]", e.message); });
  page.on("requestfailed", (r) => { problems++; console.log("[request failed]", r.url(), r.failure()?.errorText); });
  page.on("response", (r) => { seen.set(new URL(r.url()).pathname, r.status()); if (r.status() >= 400) { problems++; console.log(`[http ${r.status()}]`, r.url()); } });
  await page.goto(url + query, { waitUntil: "load" });
  return page;
}
const frame = (page: Page) => page.evaluate(() => JSON.parse((window as any).snatcher.game.debug_info()).frame as number);

// ---- 1. first visit
console.log(`site: ${url}`);
let page = await open();
await page.waitForSelector("#btnImport", { timeout: 30000 });
await sleep(1500); // the page's start-up (language, probe, cache) has finished
check("page loads without console error, page error, failed or 4xx/5xx request", problems === 0, `${problems} problem(s)`);
check("first visit shows only step 1 (the import), no error or warning text", await page.evaluate(() => document.getElementById("ovAfter")!.hidden && !document.querySelector("#overlay .err, #overlay .warn") && !(document.getElementById("ovStep1") as HTMLElement).hidden));
const visible = await page.evaluate(() => (document.getElementById("ovWin") as HTMLElement).innerText);
check("no probe text for developers is shown (no 'no local assets', no '/assets')", !/assets|server/i.test(visible.replace(/never uploaded/g, "")), visible.replace(/\s+/g, " ").slice(0, 160));
check("no request under /assets", ![...seen.keys()].some((p) => p.includes("/assets/")));
const js = [...seen.keys()].filter((p) => /\.(js|wasm)$/.test(p));
console.log("  loaded:", [...seen.keys()].map((p) => p.replace(prefix, "")).join(", "));
check("engine files come from the sub-path", js.length >= 2 && js.every((p) => p.startsWith(prefix)), js.join(" "));
check("isolation headers are not needed (page works without crossOriginIsolated)", !(await page.evaluate(() => (window as any).crossOriginIsolated)));
await page.screenshot({ path: join(out, "1-first-visit.png") });

// ---- 2. pt-BR
const introEn = await page.$eval(".intro", (e) => e.textContent ?? "");
await page.click("#langOvPt");
await page.waitForFunction((en) => document.querySelector(".intro")!.textContent !== en, { timeout: 15000 }, introEn);
const introPt = await page.$eval(".intro", (e) => e.textContent ?? "");
check("switching to pt-BR loads the strings", /jogo|disco|navegador/i.test(introPt), introPt);
check("the strings file was fetched with the build version", [...seen.keys()].some((p) => p.endsWith("/i18n/pt-BR/ui.json")));
await page.screenshot({ path: join(out, "2-pt-BR.png") });
check("still no problem after the switch", problems === 0, `${problems}`);
await page.close();

// ---- 3. full import + Play (optional)
if (full) {
  console.log("\nfull import from the sub-path");
  page = await open("?lang=pt-BR");
  await page.waitForSelector("#btnImport", { timeout: 30000 });
  await (await page.$("#discPick"))!.uploadFile(cue, bin);
  const t0 = Date.now();
  for (;;) {
    const st = await page.evaluate(() => ({ prog: !(document.getElementById("impProg") as HTMLElement).hidden, st: document.getElementById("impStatus")!.textContent ?? "", cls: document.getElementById("impStatus")!.className }));
    if (!st.prog && (st.cls.includes("ok") || st.cls.includes("err")) && st.st) { check("import finished", st.cls.includes("ok"), st.st); break; }
    if (Date.now() - t0 > 900000) { check("import finished in time", false); break; }
    await sleep(500);
  }
  console.log(`  import took ${((Date.now() - t0) / 1000).toFixed(0)} s`);
  await page.waitForSelector("#btnPlay:not([disabled])", { timeout: 15000 });
  await page.screenshot({ path: join(out, "3-imported.png") });
  await page.click("#btnPlay");
  await page.waitForFunction(() => (window as any).snatcher, { timeout: 60000 });
  await page.waitForFunction(() => JSON.parse((window as any).snatcher.game.debug_info()).frame >= 300, { timeout: 120000, polling: 200 });
  check("the game draws frames", (await frame(page)) >= 300, `frame ${await frame(page)}`);
  check("the pt-BR pack was fetched from the sub-path", [...seen.entries()].some(([p, s]) => p.endsWith("/i18n/pt-BR.pack") && s === 200));
  const px = await page.evaluate(() => { const c = document.getElementById("screen") as HTMLCanvasElement; const t = document.createElement("canvas"); t.width = c.width; t.height = c.height; const x = t.getContext("2d")!; x.drawImage(c, 0, 0); const d = x.getImageData(0, 0, t.width, t.height).data; const set = new Set<number>(); for (let i = 0; i < d.length; i += 4) set.add((d[i] << 16) | (d[i + 1] << 8) | d[i + 2]); return set.size; });
  check("the canvas shows more than a flat colour", px > 3, `${px} colours`);
  await sleep(4000);
  await page.screenshot({ path: join(out, "4-playing.png") });
  check("no console error or failed request during import and play", problems === 0, `${problems}`);
  await page.close();
}

await browser.close();
server?.stop(true);
console.log(failed ? `\n${failed} check(s) FAILED` : "\nall checks passed");
process.exit(failed ? 1 : 0);
