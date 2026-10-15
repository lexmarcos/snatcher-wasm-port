// Save slots: base64 of the 0x1C0-byte save block (S:$9C00..$9DBF, docs/re/actions.md §6), slots 1..4.
// Kept in localStorage (`snatcher.save.N`) with an in-memory mirror, so a blocked/private-window storage
// still works for the session (and the player is told to export). Export/import = small JSON file.
export const SAVE_SIZE = 0x1C0;
export const SLOTS = [1, 2, 3, 4];

const key = (n: number) => `snatcher.save.${n}`;

export function b64ToBytes(s: string): Uint8Array {
  return Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
}
export function bytesToB64(b: Uint8Array): string {
  let s = "";
  for (let i = 0; i < b.length; i += 0x8000) s += String.fromCharCode(...b.subarray(i, i + 0x8000));
  return btoa(s);
}

export class SaveStore {
  private mem = new Map<number, string>();
  /** true when localStorage could not be written (saves live only in this page → export them). */
  volatile = false;

  constructor() {
    for (const n of SLOTS) {
      try { const d = localStorage.getItem(key(n)); if (d) this.mem.set(n, d); } catch { this.volatile = true; }
    }
  }

  get(n: number): string | undefined { return this.mem.get(n); }
  slots(): [number, string][] { return [...this.mem].sort((a, b) => a[0] - b[0]); }

  set(n: number, b64: string) {
    if (!SLOTS.includes(n)) return;
    this.mem.set(n, b64);
    try { localStorage.setItem(key(n), b64); } catch { this.volatile = true; }
  }

  /** The save block as the engine wants it (the engine keeps the first 0x1C0 bytes, game.rs cmd 0x1E). */
  bytes(n: number): Uint8Array | null {
    const d = this.mem.get(n);
    if (!d) return null;
    try { const b = b64ToBytes(d); return b.length >= SAVE_SIZE ? b.subarray(0, SAVE_SIZE) : null; } catch { return null; }
  }

  exportJson(): string {
    const slots: Record<string, string> = {};
    for (const [n, d] of this.slots()) slots[String(n)] = d;
    return JSON.stringify({ format: "snatcher-web-saves", version: 1, size: SAVE_SIZE, slots }, null, 1);
  }

  /** Validate and store; returns the imported slot numbers and rejected-entry messages. */
  importJson(text: string): { imported: number[]; errors: string[] } {
    const imported: number[] = [], errors: string[] = [];
    let j: any;
    try { j = JSON.parse(text); } catch { return { imported, errors: ["not a JSON file"] }; }
    if (!j || j.format !== "snatcher-web-saves" || typeof j.slots !== "object" || !j.slots) return { imported, errors: ["not a Snatcher save export"] };
    for (const [k, v] of Object.entries<any>(j.slots)) {
      const n = Number(k);
      if (!SLOTS.includes(n)) { errors.push(`slot ${k}: out of range`); continue; }
      try {
        if (typeof v !== "string") throw new Error("not a string");
        const b = b64ToBytes(v);
        if (b.length < SAVE_SIZE) throw new Error(`only ${b.length} bytes`);
        this.set(n, bytesToB64(b.subarray(0, SAVE_SIZE)));
        imported.push(n);
      } catch (e) { errors.push(`slot ${k}: invalid data (${(e as Error).message})`); }
    }
    return { imported, errors };
  }
}
