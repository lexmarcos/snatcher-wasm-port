// Where the page's own files live. Every URL is built from the page's location (document.baseURI, or the worker's own URL), never from "/",
// so the same build works at the root of the dev server and under a sub-path such as https://host/snatcher-wasm-port/ (GitHub Pages).
// BUILD is replaced by scripts/build-static.sh with a hash of the whole build; it is appended as ?v=... to the wasm, JS, pack and strings URLs,
// so a new deploy never pairs an old cached file with a new one (Pages caches for 10 minutes). In the dev server it stays a placeholder: no query.
const BUILD: string = "@@BUILD@@";
const q = BUILD.startsWith("@@") ? "" : "?v=" + BUILD;

/** Root folder of the site (the folder that holds index.html, dist/, pkg/, i18n/). */
function root(): string {
  return typeof document !== "undefined" ? document.baseURI : new URL("../", self.location.href).href; // workers live in dist/
}

/** Absolute URL of a site file given relative to the site root ("pkg/snengine.js"), with the build version. */
export function siteUrl(path: string): string {
  return new URL(path, root()).href + q;
}

/** True when the page was published without a served game copy (static hosting): then the /assets probe is not even tried. */
export function noServedAssets(): boolean {
  return typeof document !== "undefined" && document.querySelector('meta[name="sn-served-assets"]')?.getAttribute("content") === "none";
}

/** Load and start the engine package (wasm-bindgen glue + the wasm file, both with the build version). */
export async function loadEngine(): Promise<{ pkg: any; wasm: any }> {
  const pkg: any = await import(/* @vite-ignore */ siteUrl("pkg/snengine.js") as string);
  const wasm = await pkg.default({ module_or_path: siteUrl("pkg/snengine_bg.wasm") });
  return { pkg, wasm };
}
