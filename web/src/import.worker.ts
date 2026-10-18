// Web Worker of the disc importer. Runs the Rust importer (snengine wasm, `importer.rs`) next to the user's files:
//  - reads the .bin file(s) lazily with FileReaderSync on File.slice (the image is never loaded whole; peak memory is a few MB
//    plus the wasm heap, which holds only the small game files)
//  - writes the output streams into OPFS through synchronous access handles (also streamed, never accumulated)
//  - afterwards renders the FM sounds (the Rust port of the 68000 FM driver + YM2612) one at a time into the cache, the sound
//    the game asks for first, and reports each one finished.
// Messages in:  {type:"import", cue, files: File[], copyCd}  {type:"fm-start"}  {type:"fm-want", id}  {type:"fm-stop"}
// Messages out: {type:"progress", phase, done, total}  {type:"imported", meta}  {type:"error", error}
//               {type:"fm-progress", done, total}  {type:"fm-done", id}  {type:"fm-complete"}  {type:"fm-error", message}
import { CACHE_DIR } from "./cache";

declare const FileReaderSync: { new (): { readAsArrayBuffer(b: Blob): ArrayBuffer } };
const post = (m: unknown) => (self as any).postMessage(m);

let pkgP: Promise<any> | null = null;
function wasm() {
  pkgP ??= (async () => { const pkg: any = await import(/* @vite-ignore */ "/pkg/snengine.js" as string); await pkg.default(); return pkg; })();
  return pkgP;
}

// ---------------------------------------------------------------- OPFS streams
class Streams {
  private open = new Map<string, { h: any; pos: number }>();
  constructor(private dir: FileSystemDirectoryHandle) {}
  private async dirOf(path: string): Promise<[FileSystemDirectoryHandle, string]> {
    let d = this.dir;
    const parts = path.split("/");
    for (const p of parts.slice(0, -1)) d = await d.getDirectoryHandle(p, { create: true });
    return [d, parts[parts.length - 1]];
  }
  /** Synchronous API for the wasm side: handles are opened ahead (see `prepare`) because creating one is async. */
  async prepare(path: string) {
    if (this.open.has(path)) return;
    const [d, name] = await this.dirOf(path);
    const fh = await d.getFileHandle(name, { create: true });
    const h = await (fh as any).createSyncAccessHandle();
    h.truncate(0);
    this.open.set(path, { h, pos: 0 });
  }
  has(path: string) { return this.open.has(path); }
  write(path: string, data: Uint8Array) {
    const s = this.open.get(path);
    if (!s) throw new Error("stream not prepared: " + path);
    let off = 0;
    while (off < data.length) {
      const n = s.h.write(data.subarray(off), { at: s.pos + off });
      if (!n) throw new Error("storage write failed (disk full?)");
      off += n;
    }
    s.pos += data.length;
  }
  finish(path: string) {
    const s = this.open.get(path);
    if (!s) return;
    s.h.flush(); s.h.close(); this.open.delete(path);
  }
  closeAll() { for (const p of [...this.open.keys()]) { try { this.finish(p); } catch { /* ignore */ } } }
}

/** The names of all streams the importer writes, known ahead because a sync handle needs an async open. */
function streamNames(copyCd: boolean): string[] {
  return ["header.bin", "ip.bin", "files.pack", "voices.pcm", "sfx.pcm", "pcm.json", "fm.json", "meta.json", ...(copyCd ? ["cdda.pcm"] : [])];
}

let importing = false;

async function doImport(cue: string, files: File[], copyCd: boolean) {
  if (importing) return;
  importing = true;
  let streams: Streams | null = null;
  const root = await navigator.storage.getDirectory();
  try {
    try { await root.removeEntry(CACHE_DIR, { recursive: true }); } catch { /* none yet */ }
    const dir = await root.getDirectoryHandle(CACHE_DIR, { create: true });
    streams = new Streams(dir);
    const st = streams;
    const pkg = await wasm();
    for (const n of streamNames(copyCd)) await st.prepare(n);
    const reader = new FileReaderSync();
    let last = 0;
    const io = {
      read(file: number, offset: number, buf: Uint8Array) {
        const ab = reader.readAsArrayBuffer(files[file].slice(offset, offset + buf.length));
        if (ab.byteLength !== buf.length) throw new Error(`short read of ${files[file].name} at ${offset}: ${ab.byteLength} of ${buf.length} bytes`);
        buf.set(new Uint8Array(ab));
      },
      write(stream: string, data: Uint8Array) { st.write(stream, data); },
      finish(stream: string) { st.finish(stream); },
      progress(phase: string, done: number, total: number) {
        const now = performance.now();
        if (phase !== "done" && now - last < 150) return;
        last = now; post({ type: "progress", phase, done, total });
      },
    };
    const metaJson: string = pkg.import_disc(cue, files.map((f) => f.size), copyCd, io);
    st.closeAll();
    post({ type: "imported", meta: JSON.parse(metaJson) });
  } catch (e: any) {
    streams?.closeAll();
    try { await root.removeEntry(CACHE_DIR, { recursive: true }); } catch { /* ignore */ }
    let error: { code: string; detail: string };
    try { error = JSON.parse(String(e?.message ?? e)); if (!error.code) throw 0; } catch { error = { code: "io", detail: String(e?.message ?? e) }; }
    post({ type: "error", error });
  } finally { importing = false; }
}

// ---------------------------------------------------------------- FM rendering
let fm: { render(id: number, io: any): number } | null = null;
let fmQueue: number[] = [];
let fmInfo: Record<string, { file: string; frames: number }> = {};
let fmDone = new Set<number>();
let fmTotal = 0;
let fmRunning = false;
let fmStopped = false;
const fmWanted: number[] = [];

async function readCache(path: string): Promise<Uint8Array> {
  let d = await (await navigator.storage.getDirectory()).getDirectoryHandle(CACHE_DIR);
  const parts = path.split("/");
  for (const p of parts.slice(0, -1)) d = await d.getDirectoryHandle(p);
  return new Uint8Array(await (await (await d.getFileHandle(parts[parts.length - 1])).getFile()).arrayBuffer());
}

async function fmSize(file: string): Promise<number> {
  try {
    let d = await (await navigator.storage.getDirectory()).getDirectoryHandle(CACHE_DIR);
    const parts = file.split("/");
    for (const p of parts.slice(0, -1)) d = await d.getDirectoryHandle(p);
    return (await (await d.getFileHandle(parts[parts.length - 1])).getFile()).size;
  } catch { return -1; }
}

async function fmStart() {
  if (fmRunning) return;
  fmRunning = true; fmStopped = false;
  try {
    const manifest = JSON.parse(new TextDecoder().decode(await readCache("fm.json")));
    fmInfo = manifest.sounds;
    const ids = Object.keys(fmInfo).map(Number);
    // shortest first: most sounds are playable soonest, the 6 minute loops come last
    ids.sort((a, b) => fmInfo[a].frames - fmInfo[b].frames);
    fmTotal = ids.length;
    fmQueue = [];
    for (const id of ids) {
      if ((await fmSize(fmInfo[id].file)) === fmInfo[id].frames * 4) fmDone.add(id); else fmQueue.push(id);
    }
    post({ type: "fm-progress", done: fmDone.size, total: fmTotal });
    if (!fmQueue.length) { post({ type: "fm-complete" }); fmRunning = false; return; }
    const pkg = await wasm();
    const meta = JSON.parse(new TextDecoder().decode(await readCache("meta.json")));
    const pack = await readCache("files.pack");
    const part = (name: string) => { const f = meta.files.find((x: any) => x.name === name); return pack.subarray(f.off, f.off + f.size); };
    const [header, ip] = await Promise.all(["header.bin", "ip.bin"].map(readCache));
    const [fmwr, lt, dr] = ["FMWR_1.BIN", "PCMLT_01.BIN", "PCMDRMDT.BIN"].map(part);
    fm = new pkg.FmRender(header, ip, fmwr, lt, dr);
    const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle(CACHE_DIR);
    while ((fmQueue.length || fmWanted.length) && !fmStopped) {
      let id = fmWanted.shift();
      if (id === undefined || fmDone.has(id)) id = undefined;
      if (id === undefined) id = fmQueue.find((x) => !fmDone.has(x));
      if (id === undefined) break;
      fmQueue = fmQueue.filter((x) => x !== id);
      const streams = new Streams(dir);
      const name = fmInfo[id].file;
      await streams.prepare(name);
      try {
        const frames = fm!.render(id, { write: (s: string, d: Uint8Array) => streams.write(s, d), finish: (s: string) => streams.finish(s) });
        if (frames !== fmInfo[id].frames) throw new Error(`sound ${id.toString(16)}: rendered ${frames} frames, manifest says ${fmInfo[id].frames}`);
      } finally { streams.closeAll(); }
      fmDone.add(id);
      post({ type: "fm-done", id });
      post({ type: "fm-progress", done: fmDone.size, total: fmTotal });
      // let queued messages (fm-want, fm-stop) in
      await new Promise((r) => setTimeout(r, 0));
    }
    if (!fmStopped) post({ type: "fm-complete" });
  } catch (e: any) {
    post({ type: "fm-error", message: String(e?.message ?? e) });
  } finally { fmRunning = false; fm = null; }
}

self.onmessage = (ev: MessageEvent) => {
  const m = ev.data;
  switch (m.type) {
    case "import": void doImport(m.cue, m.files, m.copyCd); break;
    case "fm-start": void fmStart(); break;
    case "fm-want":
      if (fmDone.has(m.id)) post({ type: "fm-done", id: m.id });
      else if (!fmWanted.includes(m.id)) fmWanted.push(m.id);
      break;
    case "fm-stop": fmStopped = true; break;
  }
};
