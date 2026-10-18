// The "imported" Source: the user's disc image (.cue + .bin file(s)) turned into a cache in the browser by the Rust importer
// (docs/re/web.md section 8). This file has
//  - `resolvePick`   which of the picked/dropped files are the cue sheet and the images it names (the cue is parsed in Rust)
//  - `ImportWorker`  the Web Worker that imports, and afterwards renders the FM sounds into the cache
//  - `ImportedSource` game files, boot code, manifests and decoded AudioBuffers straight from the cache (no ogg anywhere)
import { cacheFile, cacheJson, type Meta } from "./cache";
import type { GameFile, Source } from "./sources";
import { T } from "./i18n";

export interface Pick { cueName: string; cueText: string; files: File[]; notes: string[] }
export type PickError = { code: "nocue" | "manycues" | "nobin" | "badcue" | "toobig"; detail: string };
export interface Progress { phase: string; done: number; total: number }
export interface ImportFail { code: string; detail: string }

const base = (n: string) => n.split(/[\\/]/).pop()!.toLowerCase();

/**
 * Decide which picked files make the disc: exactly one .cue, plus a file for each `FILE` line of it (matched by name, case
 * insensitive, folders ignored). `cueFiles(text)` returns the FILE names (Rust `cue_files`; throws the error JSON of a bad cue).
 */
export async function resolvePick(picked: File[], cueFiles: (text: string) => string[]): Promise<Pick | PickError> {
  const cues = picked.filter((f) => /\.cue$/i.test(f.name));
  if (!cues.length) return { code: "nocue", detail: picked.map((f) => f.name).slice(0, 6).join(", ") };
  if (cues.length > 1) return { code: "manycues", detail: cues.map((c) => c.name).join(", ") };
  const cue = cues[0];
  if (cue.size > 1 << 20) return { code: "toobig", detail: cue.name };
  const cueText = await cue.text();
  let names: string[];
  try { names = cueFiles(cueText); } catch (e: any) {
    let detail = String(e?.message ?? e);
    try { detail = JSON.parse(detail).detail; } catch { /* plain text */ }
    return { code: "badcue", detail };
  }
  const others = picked.filter((f) => f !== cue);
  const byName = new Map(others.map((f) => [base(f.name), f]));
  const notes: string[] = [];
  const files: File[] = [];
  const missing: string[] = [];
  for (const n of names) {
    let f = byName.get(base(n));
    if (!f && names.length === 1) {
      // a renamed image: accept the only picked image-like file
      const imgs = others.filter((o) => /\.(bin|img|iso)$/i.test(o.name));
      if (imgs.length === 1) { f = imgs[0]; notes.push(`${f.name} used for ${n}`); }
    }
    if (f) files.push(f); else missing.push(n);
  }
  if (missing.length) return { code: "nobin", detail: missing.join(", ") };
  return { cueName: cue.name, cueText, files, notes };
}

// ---------------------------------------------------------------- the worker

export class ImportWorker {
  private w: Worker | null = null;
  private fmWait = new Map<number, Array<() => void>>();
  private fmFailed: string | null = null;
  onFm: ((done: number, total: number, complete: boolean) => void) | null = null;
  private onProgress: ((p: Progress) => void) | null = null;
  private job: { ok: (m: Meta) => void; fail: (e: ImportFail) => void } | null = null;
  fmDone = 0; fmTotal = 0; fmComplete = false;
  /** wall time of the background FM rendering in this page visit (ms), 0 until it finished */
  fmMs = 0; private fmT0 = 0;

  private get worker(): Worker {
    if (!this.w) {
      const w = this.w = new Worker("/dist/import.worker.js", { type: "module" });
      w.onmessage = (ev) => this.message(ev.data);
      w.onerror = (ev) => { this.job?.fail({ code: "io", detail: ev.message || "the import worker stopped" }); this.job = null; };
    }
    return this.w;
  }
  private message(m: any) {
    switch (m.type) {
      case "progress": this.onProgress?.(m); break;
      case "imported": this.job?.ok(m.meta); this.job = null; break;
      case "error": this.job?.fail(m.error); this.job = null; break;
      case "fm-progress": this.fmDone = m.done; this.fmTotal = m.total; this.onFm?.(m.done, m.total, false); break;
      case "fm-done": for (const f of this.fmWait.get(m.id) ?? []) f(); this.fmWait.delete(m.id); break;
      case "fm-complete": this.fmComplete = true; this.fmMs = performance.now() - this.fmT0; this.onFm?.(this.fmTotal, this.fmTotal, true); this.finishFm(); break;
      case "fm-error": this.fmFailed = m.message; console.error("[fm]", m.message); this.finishFm(); break;
    }
  }
  private finishFm() {
    for (const [, ws] of this.fmWait) for (const f of ws) f();
    this.fmWait.clear();
    this.terminate();
  }

  import(pick: Pick, copyCd: boolean, onProgress: (p: Progress) => void): Promise<Meta> {
    this.onProgress = onProgress;
    return new Promise<Meta>((ok, fail) => {
      this.job = { ok, fail };
      this.worker.postMessage({ type: "import", cue: pick.cueText, files: pick.files, copyCd });
    });
  }
  /** Stop an import in progress (the half-written cache is removed by the caller). */
  cancel() { this.job?.fail({ code: "cancelled", detail: "" }); this.job = null; this.terminate(); }

  /** Start (or resume) rendering the FM sounds into the cache. */
  fmStart() { if (this.fmComplete || this.fmFailed) return; if (!this.fmT0) this.fmT0 = performance.now(); this.worker.postMessage({ type: "fm-start" }); }
  /** Resolves when sound `id` is in the cache (the worker renders it next). */
  fmWant(id: number): Promise<void> {
    if (this.fmComplete || this.fmFailed) return Promise.resolve();
    return new Promise((ok) => {
      this.worker.postMessage({ type: "fm-start" }); // no-op when it already runs
      const l = this.fmWait.get(id) ?? [];
      l.push(ok); this.fmWait.set(id, l);
      this.worker.postMessage({ type: "fm-want", id });
    });
  }
  terminate() { this.w?.terminate(); this.w = null; }
}

// ---------------------------------------------------------------- the source

interface Loc { stream: string; off: number; bytes: number; kind: "s8" | "fm"; rate: number; id?: number; frames?: number }

export class ImportedSource implements Source {
  private loc = new Map<string, Loc>();
  private blobs = new Map<string, Promise<File | null>>();
  private pcm: any = null;
  private fm: any = null;
  /** shown by the page while the FM sounds are being prepared in the background */
  onStatus: ((text: string | null) => void) | null = null;

  constructor(readonly meta: Meta, private worker: ImportWorker, private sessionFiles: File[] | null) {}

  get label() { return T("src.imported"); }

  /** Read the manifests and index the places of every sound. */
  async init(): Promise<this> {
    [this.pcm, this.fm] = await Promise.all([cacheJson("pcm.json"), cacheJson("fm.json")]);
    const walk = (o: any, rateOf: (e: any) => number) => {
      if (!o || typeof o !== "object") return;
      if (typeof o.file === "string" && Array.isArray(o.at)) {
        this.loc.set("audio/web/" + o.file, { stream: o.at[0], off: o.at[1], bytes: o.at[2], kind: "s8", rate: rateOf(o) });
        return;
      }
      for (const v of Object.values(o)) walk(v, rateOf);
    };
    walk(this.pcm?.voices, () => this.pcm.voice_rate_hz);
    walk(this.pcm?.banks, (e) => e.rate);
    walk(this.pcm?.drum, (e) => e.rate);
    for (const [id, s] of Object.entries<any>(this.fm?.sounds ?? {})) {
      this.loc.set("audio/web/" + s.file, { stream: s.file, off: 0, bytes: s.frames * 4, kind: "fm", rate: this.fm.rate ?? 48000, id: Number(id), frames: s.frames });
    }
    return this;
  }

  private blob(path: string): Promise<File | null> {
    let p = this.blobs.get(path);
    if (!p) { p = cacheFile(path); this.blobs.set(path, p); p.then((f) => { if (!f) this.blobs.delete(path); }); }
    return p;
  }

  gameFiles(): GameFile[] {
    return this.meta.files.map((f: any) => ({
      name: f.name,
      read: async () => {
        const pack = await this.blob("files.pack");
        if (!pack) throw new Error("files.pack missing");
        return pack.slice(f.off, f.off + f.size).arrayBuffer();
      },
    }));
  }
  async boot(name: string) { const f = await cacheFile(name.toLowerCase()); return f ? f.arrayBuffer() : null; }
  async audio(path: string) {
    const f = path === "audio/web/pcm.json" ? await cacheFile("pcm.json") : path === "audio/web/fm.json" ? await cacheFile("fm.json") : null;
    return f ? f.arrayBuffer() : null;
  }
  summary() {
    const l = [T("src.imp.title", { title: this.meta.title, files: this.meta.files.length })];
    l.push(this.meta.cdda.copied ? T("src.imp.cdkept") : (this.sessionFiles ? T("src.imp.cdsession") : T("src.imp.cdnone")));
    return l;
  }

  /** FM sounds are rendered in the background; the engine's first use of one may wait for its turn. */
  start() {
    this.worker.onFm = (done, total, complete) => {
      this.onStatus?.(complete ? null : T("bg.fm", { done, total }));
    };
    this.worker.fmStart();
  }
  dispose() { this.worker.onFm = null; this.worker.terminate(); }

  /** Decoded audio for a path the page asks AudioOut for; undefined when the path is not ours. */
  direct(path: string, ctx: BaseAudioContext): Promise<AudioBuffer | null> | undefined {
    const cd = /^web\/music\/track(\d+)\.ogg$/.exec(path);
    if (cd) return this.cdTrack(Number(cd[1]), ctx);
    const l = this.loc.get(path);
    if (!l) return undefined;
    return l.kind === "s8" ? this.s8(l, ctx) : this.fmSound(l, ctx);
  }

  private async s8(l: Loc, ctx: BaseAudioContext): Promise<AudioBuffer | null> {
    const f = await this.blob(l.stream);
    if (!f) return null;
    const b = new Int8Array(await f.slice(l.off, l.off + l.bytes).arrayBuffer());
    const buf = ctx.createBuffer(1, Math.max(1, b.length), l.rate);
    const ch = buf.getChannelData(0);
    for (let i = 0; i < b.length; i++) ch[i] = b[i] / 128;
    return buf;
  }

  private async fmSound(l: Loc, ctx: BaseAudioContext): Promise<AudioBuffer | null> {
    let f = await cacheFile(l.stream);
    if (!f || f.size !== l.bytes) {
      await this.worker.fmWant(l.id!);
      f = await cacheFile(l.stream);
      if (!f || f.size !== l.bytes) return null;
    }
    return s16Buffer(ctx, f, 0, l.bytes, l.rate);
  }

  private async cdTrack(n: number, ctx: BaseAudioContext): Promise<AudioBuffer | null> {
    const t = this.meta.cdda.tracks[String(n)];
    if (!t) return null;
    if (this.meta.cdda.copied) {
      const f = await this.blob("cdda.pcm");
      return f ? s16Buffer(ctx, f, t.off, t.bytes, this.meta.cdda.rate) : null;
    }
    const src = this.sessionFiles?.[t.file];
    return src ? s16Buffer(ctx, src, t.start, t.bytes, this.meta.cdda.rate) : null;
  }
}

/** 16-bit little-endian stereo PCM of `file[off, off+bytes)` as an AudioBuffer, converted piece by piece (no second full-size copy). */
export async function s16Buffer(ctx: BaseAudioContext, file: Blob, off: number, bytes: number, rate: number): Promise<AudioBuffer> {
  const frames = bytes >> 2;
  const buf = ctx.createBuffer(2, Math.max(1, frames), rate);
  const L = buf.getChannelData(0), R = buf.getChannelData(1);
  const STEP = 1 << 18;
  for (let f = 0; f < frames; f += STEP) {
    const n = Math.min(STEP, frames - f);
    const s = new Int16Array(await file.slice(off + f * 4, off + (f + n) * 4).arrayBuffer());
    for (let i = 0; i < n; i++) { L[f + i] = s[2 * i] / 32768; R[f + i] = s[2 * i + 1] / 32768; }
  }
  return buf;
}
