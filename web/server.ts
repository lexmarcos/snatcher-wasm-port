// Local dev server for the Snatcher web port.
// Serves the page, the wasm package and the user's locally extracted assets (never published).
// Usage: bun web/server.ts  (port 5300, override with PORT=...)
import { join, normalize } from "path";

const root = import.meta.dir;
const assets = join(root, "..", "assets");
const i18n = join(root, "..", "i18n");
const port = Number(process.env.PORT ?? 5300);

function safe(base: string, rel: string): string | null {
  const p = normalize(join(base, rel));
  return p.startsWith(base) ? p : null;
}

Bun.serve({
  port,
  async fetch(req) {
    const url = new URL(req.url);
    let path = decodeURIComponent(url.pathname);
    let file: string | null = null;
    if (path === "/" || path === "/index.html") file = join(root, "index.html");
    else if (path.startsWith("/assets/")) file = safe(assets, path.slice(8));
    // translation: the page strings (i18n/pt-BR/ui.json) and the engine's pack (built by scripts/build-web.sh, `snt i18n-build`)
    else if (path === "/i18n/pt-BR.pack") file = join(i18n, "build", "pt-BR.pack");
    else if (path === "/i18n/pt-BR/ui.json") file = join(i18n, "pt-BR", "ui.json");
    else if (path.startsWith("/dist/") || path.startsWith("/pkg/")) file = safe(root, path.slice(1));
    if (!file) return new Response("not found", { status: 404 });
    const f = Bun.file(file);
    if (!(await f.exists())) return new Response("not found", { status: 404 });
    return new Response(f, { headers: { "Cache-Control": "no-cache" } });
  },
});
console.log(`Snatcher web port: http://localhost:${port}/`);
