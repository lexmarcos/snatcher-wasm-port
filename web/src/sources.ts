// Where the user's own game data comes from. Two interchangeable sources:
//  - served: the local dev server (`bun web/server.ts`) serving the extracted ./assets directory
//  - picked: a folder chosen with <input webkitdirectory> (File objects, read lazily, never uploaded)
// Nothing here talks to a remote host; game data is never bundled or published.
import { T, type UiKey } from "./i18n";
import { noServedAssets, siteUrl } from "./urls";

export interface GameFile { name: string; read(): Promise<ArrayBuffer> }

export interface Source {
  label: string;
  /** Disc files the engine needs (assets/raw/files/*). */
  gameFiles(): GameFile[];
  /** Main-CPU boot code: "HEADER.BIN" / "IP.BIN" (optional). */
  boot(name: string): Promise<ArrayBuffer | null>;
  /** Audio file or manifest, path relative to assets/ (e.g. "audio/web/pcm.json", "web/music/track05.ogg"). */
  audio(path: string): Promise<ArrayBuffer | null>;
  /** Human-readable lines describing what was found. */
  summary(): string[];
  /** Sources that hold decoded audio (the disc importer's cache) hand out AudioBuffers directly; undefined = not handled, use `audio()` + decodeAudioData. */
  direct?(path: string, ctx: BaseAudioContext): Promise<AudioBuffer | null> | undefined;
  /** Called once the game runs (the importer starts its background FM rendering here). */
  start?(): void;
  /** Called when the source is replaced or removed. */
  dispose?(): void;
}

/** Files the engine never needs (PCMLD_01.BIN is the big PCM loader image, played via the audio pipeline instead). */
const SKIP = new Set(["PCMLD_01.BIN"]);

// ---------------------------------------------------------------- served

async function getBuf(url: string): Promise<ArrayBuffer | null> {
  try { const r = await fetch(url); return r.ok ? await r.arrayBuffer() : null; } catch { return null; }
}

/** `error` is a UI string key (`err.*`), translated by the caller. */
export async function probeServed(): Promise<{ src?: Source; error?: UiKey }> {
  if (noServedAssets()) return {}; // static hosting (GitHub Pages): no served copy exists, so do not ask (a 404 would show in the console)
  let text: string;
  try {
    const r = await fetch(siteUrl("assets/raw/filelist.tsv"));
    if (!r.ok) return { error: "err.noassets" };
    text = await r.text();
  } catch { return { error: "err.noserver" }; }
  const names = text.split("\n").slice(1).map((l) => l.split("\t")[0].trim()).filter((n) => n && !SKIP.has(n));
  if (!names.length) return { error: "err.empty" };
  const src: Source = {
    get label() { return T("src.server"); },
    gameFiles: () => names.map((name) => ({
      name,
      read: async () => { const b = await getBuf(siteUrl("assets/raw/files/" + name)); if (!b) throw new Error("missing " + name); return b; },
    })),
    boot: (name) => getBuf(siteUrl("assets/raw/" + name.toLowerCase())),
    audio: (path) => getBuf(siteUrl("assets/" + path)),
    summary: () => [T("src.listed", { n: names.length })],
  };
  return { src };
}

// ---------------------------------------------------------------- picked folder

const relPath = (f: File) => ((f as any).webkitRelativePath || f.name).replace(/\\/g, "/") as string;

/**
 * Build a source from the files of a picked folder. The folder may be the project's `assets/`, `assets/raw`,
 * `assets/raw/files`, or any parent of them; files are recognised by name and by the directory names above them:
 *  - game files: everything directly inside a directory named `files` (fallback when the browser gives no
 *    directory information: *.BIN / *.TXT files that are not header/ip/sp)
 *  - header.bin / ip.bin by name
 *  - audio: .ogg / .json looked up by the longest matching path suffix ("audio/web/fm/sfx_01.ogg",
 *    then "web/fm/sfx_01.ogg", ...) so any pick depth works
 */
export function pickedSource(list: File[]): Source {
  const files = list.map((f) => ({ f, parts: relPath(f).split("/") }));
  const lname = (p: string[]) => p[p.length - 1].toLowerCase();
  const isAudio = (n: string) => n.endsWith(".ogg") || n.endsWith(".json");

  let games = files.filter((x) => x.parts.length >= 2 && x.parts[x.parts.length - 2].toLowerCase() === "files" && !isAudio(lname(x.parts)));
  if (!games.length) {
    games = files.filter((x) => /\.(bin|txt)$/i.test(x.parts[x.parts.length - 1]) && !["header.bin", "ip.bin", "sp.bin"].includes(lname(x.parts)));
  }
  games = games.filter((x) => !SKIP.has(x.parts[x.parts.length - 1].toUpperCase()));
  const gameByName = new Map<string, File>();
  for (const x of games) gameByName.set(x.parts[x.parts.length - 1].toUpperCase(), x.f);

  const bootBy = new Map<string, File>();
  for (const x of files) { const n = lname(x.parts); if ((n === "header.bin" || n === "ip.bin") && !bootBy.has(n)) bootBy.set(n, x.f); }

  // suffix index for audio files
  const idx = new Map<string, File>();
  let oggs = 0;
  for (const x of files) {
    const n = lname(x.parts);
    if (!isAudio(n)) continue;
    if (n.endsWith(".ogg")) oggs++;
    const low = x.parts.map((s) => s.toLowerCase());
    for (let i = 0; i < low.length; i++) { const k = low.slice(i).join("/"); if (!idx.has(k)) idx.set(k, x.f); }
  }
  const flat = files.every((x) => x.parts.length === 1); // no directory information at all (rare): match by file name
  const findAudio = (path: string): File | null => {
    const q = path.toLowerCase().split("/");
    const isJson = q[q.length - 1].endsWith(".json");
    // json manifests need at least "web/name.json" (assets/audio/fm.json is the unrelated raw table)
    for (let i = 0; i < q.length - (isJson && !flat ? 1 : 0); i++) { const f = idx.get(q.slice(i).join("/")); if (f) return f; }
    return null;
  };

  return {
    get label() { return T("src.picked"); },
    gameFiles: () => [...gameByName].map(([name, f]) => ({ name, read: () => f.arrayBuffer() })),
    boot: async (name) => { const f = bootBy.get(name.toLowerCase()); return f ? f.arrayBuffer() : null; },
    audio: async (path) => { const f = findAudio(path); return f ? f.arrayBuffer() : null; },
    summary: () => [
      T("src.files", { n: gameByName.size }),
      T("src.boot", { h: bootBy.has("header.bin") ? T("src.found") : T("src.missing"), i: bootBy.has("ip.bin") ? T("src.found") : T("src.missing") }),
      T("src.audio", { n: oggs, p: findAudio("audio/web/pcm.json") ? T("src.found") : T("src.missing"), f: findAudio("audio/web/fm.json") ? T("src.found") : T("src.missing") }),
    ],
  };
}
