// Asset viewer server (graphics workstream). Usage: bun web/viewer/server.ts [port]
// Serves web/viewer/index.html and the locally extracted, gitignored assets/gfx/ tree.
import { join, normalize } from "path";
const root = join(import.meta.dir, "..", "..");
const gfx = join(root, "assets", "gfx");
const port = Number(process.argv[2] ?? 5310);
Bun.serve({
  port,
  async fetch(req) {
    const url = new URL(req.url);
    let p = decodeURIComponent(url.pathname);
    if (p === "/") return new Response(Bun.file(join(import.meta.dir, "index.html")));
    if (p.startsWith("/gfx/")) {
      const f = normalize(join(gfx, p.slice(5)));
      if (!f.startsWith(gfx)) return new Response("forbidden", { status: 403 });
      const file = Bun.file(f);
      if (await file.exists()) return new Response(file);
    }
    return new Response("not found", { status: 404 });
  },
});
console.log(`viewer: http://localhost:${port}/`);
