// Autosave (docs/re/web.md section 9): one snapshot of the running game (engine/src/snapshot.rs), kept apart from the four save slots.
// The snapshot is about 40-60 KB, so it lives in IndexedDB (database `snatcher.autosave`), not in localStorage; the on/off setting is small and
// uses localStorage (`snatcher.autosave.on`). Either may be blocked: the page then keeps the autosave in memory for this visit and says so.
export const DB_NAME = "snatcher.autosave";
export const PREF_KEY = "snatcher.autosave.on";
const STORE = "slot", KEY = "latest";
/** Format of the stored record (the snapshot inside has its own version and layout fingerprint, checked by the engine). */
export const RECORD_VERSION = 1;

export interface AutoRecord {
  v: number;
  /** Date.now() when it was taken */
  stamp: number;
  /** story act 1-3, 0 = unknown (developer scene menu) */
  act: number;
  /** game clock "HH:MM:SS" */
  play: string;
  /** 1 = command menu, 2 = message waiting for the button */
  kind: number;
  /** the game text language of the snapshot */
  pt: boolean;
  /** sound mode the game had set (the audio side is not part of the engine state) */
  mono: { fm: boolean; pcm: boolean };
  data: Uint8Array;
  /** engine frame of the snapshot (tests, debugging) */
  frame?: number;
}

/** What the game says about itself: the engine's `snapshot_info()` JSON. */
export interface EngineInfo { kind: number; act: number; pt: boolean; play: string; script: number; scene: number; frame: number }

const idb = (): IDBFactory | null => { try { return typeof indexedDB === "undefined" ? null : indexedDB; } catch { return null; } };

function open(): Promise<IDBDatabase> {
  return new Promise((ok, no) => {
    const f = idb();
    if (!f) return no(new Error("IndexedDB is not available"));
    let r: IDBOpenDBRequest;
    try { r = f.open(DB_NAME, 1); } catch (e) { return no(e); }
    r.onupgradeneeded = () => { if (!r.result.objectStoreNames.contains(STORE)) r.result.createObjectStore(STORE); };
    r.onsuccess = () => ok(r.result);
    r.onerror = () => no(r.error ?? new Error("IndexedDB open failed"));
    r.onblocked = () => no(new Error("IndexedDB is blocked"));
  });
}
function tx<T>(mode: IDBTransactionMode, f: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  return open().then((db) => new Promise<T>((ok, no) => {
    const t = db.transaction(STORE, mode);
    const req = f(t.objectStore(STORE));
    let out: T;
    req.onsuccess = () => { out = req.result; };
    t.oncomplete = () => { db.close(); ok(out); };
    t.onerror = () => { db.close(); no(t.error ?? new Error("IndexedDB transaction failed")); };
    t.onabort = () => { db.close(); no(t.error ?? new Error("IndexedDB transaction aborted")); };
  }));
}

/** A record read from storage can be anything; keep only what has the right shape. */
function valid(r: any): r is AutoRecord {
  return !!r && r.v === RECORD_VERSION && typeof r.stamp === "number" && r.data instanceof Uint8Array && r.data.length > 64
    && typeof r.act === "number" && typeof r.play === "string" && typeof r.pt === "boolean" && !!r.mono;
}

export class AutosaveStore {
  /** true when IndexedDB could not be used: the autosave lives in this page only */
  volatile = false;
  private mem: AutoRecord | null = null;
  private memOnly = false;

  async get(): Promise<AutoRecord | null> {
    if (this.memOnly) return this.mem;
    try {
      const r = await tx("readonly", (s) => s.get(KEY));
      this.mem = valid(r) ? r : null;
      return this.mem;
    } catch { this.volatile = true; this.memOnly = true; return this.mem; }
  }
  async put(rec: AutoRecord): Promise<void> {
    this.mem = rec;
    if (this.memOnly) return;
    try { await tx("readwrite", (s) => s.put(rec, KEY)); }
    catch { this.volatile = true; this.memOnly = true; }
  }
  async clear(): Promise<void> {
    this.mem = null;
    try { await tx("readwrite", (s) => s.delete(KEY)); } catch { /* nothing stored or storage blocked: the memory copy is gone */ }
  }
  /** Remove the whole database ("Clear everything"). */
  static async wipe(): Promise<void> {
    const f = idb();
    if (!f) return;
    await new Promise<void>((ok) => {
      try {
        const r = f.deleteDatabase(DB_NAME);
        r.onsuccess = () => ok(); r.onerror = () => ok(); r.onblocked = () => ok();
      } catch { ok(); }
    });
  }
}

/** The stored on/off choice; on when nothing is stored. */
export function readPref(get: (k: string) => string | null): boolean { return get(PREF_KEY) !== "0"; }

/** Plain text of an error code of `Game.restore` / `snapshot_header` -> which UI string explains it. */
export function reasonKey(code: string): "auto.err.engine" | "auto.err.disc" | "auto.err.bad" {
  const c = code.split(":")[0];
  return c === "engine" || c === "format" ? "auto.err.engine" : c === "disc" || c === "missing" || c === "lang" ? "auto.err.disc" : "auto.err.bad";
}

// ---- when to save ----
/** After a scene or script change a new safe point saves at once (but not twice within this many ms). */
export const MIN_GAP_MS = 3000;
/** Without a scene change a new safe point saves when the last save is older than this. */
export const EVERY_MS = 30000;

/** What the autosaver needs from the engine (the wasm `Game`). */
export interface Snapper {
  autosave_point(): number;
  autosave_key(): number;
  snapshot(): Uint8Array;
  snapshot_info(): string;
}

export interface SaveStats {
  /** snapshots stored */
  saves: number;
  failures: number;
  /** last / worst / total time (ms) of taking the snapshot in the engine, and of the whole save including the storage write */
  lastSnapMs: number; maxSnapMs: number; totalSnapMs: number; lastTotalMs: number; maxTotalMs: number;
  lastBytes: number; maxBytes: number;
}

export class Autosaver {
  on = true;
  /** the throttle; tests lower them */
  minGap = MIN_GAP_MS;
  every = EVERY_MS;
  stats: SaveStats = { saves: 0, failures: 0, lastSnapMs: 0, maxSnapMs: 0, totalSnapMs: 0, lastTotalMs: 0, maxTotalMs: 0, lastBytes: 0, maxBytes: 0 };
  /** Date.now() of the last save of this visit (0 = none yet) */
  lastSave = 0;
  private lastKey = -1;
  private wasSafe = false;
  private want = false;
  private busy = false;

  constructor(
    private store: AutosaveStore,
    private game: () => Snapper | null,
    private audioState: () => { fm: boolean; pcm: boolean },
    private done: (rec: AutoRecord | null) => void,
  ) {}

  /** The game was restored from a record (or started): count its state as saved. */
  adopt(g: Snapper, savedNow = true) { this.lastKey = g.autosave_key(); this.lastSave = savedNow ? Date.now() : 0; this.wasSafe = false; this.want = false; }

  /** Once per rendered frame while the game runs: cheap (one call into the engine) unless it is time to save. */
  poll() {
    if (!this.on || this.busy) return;
    const g = this.game();
    if (!g) return;
    const safe = g.autosave_point() !== 0;
    if (safe && !this.wasSafe) this.want = true;      // a new quiet moment: the game came back to the command menu or waits for a button
    this.wasSafe = safe;
    if (!safe || !this.want) return;
    const gap = g.autosave_key() !== this.lastKey ? this.minGap : this.every;
    if (Date.now() - this.lastSave >= gap) { this.want = false; void this.save(g); }
  }

  /** The page is being hidden or closed: save now if the game is at a safe point (otherwise the last safe point stands). */
  flush(): Promise<void> {
    const g = this.game();
    if (!this.on || this.busy || !g || !g.autosave_point()) return Promise.resolve();
    if (Date.now() - this.lastSave < 1000) return Promise.resolve();
    return this.save(g);
  }

  private async save(g: Snapper) {
    this.busy = true;
    const t0 = performance.now();
    try {
      const data = g.snapshot();
      const info = JSON.parse(g.snapshot_info()) as EngineInfo;
      const t1 = performance.now();
      if (!data.length) return;
      const rec: AutoRecord = { v: RECORD_VERSION, stamp: Date.now(), act: info.act, play: info.play, kind: info.kind, pt: info.pt, mono: { ...this.audioState() }, data, frame: info.frame };
      this.lastKey = g.autosave_key(); this.lastSave = rec.stamp;
      await this.store.put(rec);
      const t2 = performance.now(), s = this.stats;
      s.saves++; s.lastSnapMs = t1 - t0; s.totalSnapMs += s.lastSnapMs; s.maxSnapMs = Math.max(s.maxSnapMs, s.lastSnapMs);
      s.lastTotalMs = t2 - t0; s.maxTotalMs = Math.max(s.maxTotalMs, s.lastTotalMs); s.lastBytes = data.length; s.maxBytes = Math.max(s.maxBytes, data.length);
      this.done(rec);
    } catch { this.stats.failures++; this.done(null); }
    finally { this.busy = false; }
  }
}

