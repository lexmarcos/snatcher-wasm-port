// The import cache: what the disc importer produced, kept in the browser's private file system (OPFS) so that the import
// is a one-time step. Layout (written by the Rust importer through the worker, see core/src/import.rs):
//   snatcher-import-v1/{meta.json, header.bin, ip.bin, files/NAME, voices.pcm, sfx.pcm, cdda.pcm, pcm.json, fm.json, fm/{song,sfx}_XX.s16}
// `meta.json` is written last, so its presence means the disc part is complete. FM audio files are added later, one per sound.
// Nothing here ever leaves the machine; the files are only reachable from this origin.

export const CACHE_DIR = "snatcher-import-v1";

export interface Meta {
  format: number;
  title: string;
  serial: string;
  region: string;
  digest: string;
  files: string[];
  cdda: { copied: boolean; rate: number; tracks: Record<string, { file: number; start: number; bytes: number; off: number }> };
  streams: Record<string, number>;
  /** set by the page after the import: when, and the sizes of the image files (to tell the user what they imported) */
  imported?: number;
}

export const opfsAvailable = (): boolean => typeof navigator !== "undefined" && !!navigator.storage?.getDirectory;

export async function cacheDir(create = false): Promise<FileSystemDirectoryHandle | null> {
  if (!opfsAvailable()) return null;
  try {
    const root = await navigator.storage.getDirectory();
    return await root.getDirectoryHandle(CACHE_DIR, { create });
  } catch { return null; }
}

/** Open a file of the cache by path ("files/SP01.BIN"); null when missing. */
export async function cacheFile(path: string): Promise<File | null> {
  try {
    let d = await cacheDir();
    if (!d) return null;
    const parts = path.split("/");
    for (const p of parts.slice(0, -1)) d = await d.getDirectoryHandle(p);
    return await (await d.getFileHandle(parts[parts.length - 1])).getFile();
  } catch { return null; }
}

export async function cacheJson<T = any>(path: string): Promise<T | null> {
  const f = await cacheFile(path);
  if (!f) return null;
  try { return JSON.parse(await f.text()) as T; } catch { return null; }
}

/** The completed import's meta.json, or null when there is none (or it is from an unknown format). */
export async function readMeta(): Promise<Meta | null> {
  const m = await cacheJson<Meta>("meta.json");
  return m && m.format === 1 ? m : null;
}

/** Remove everything the importer stored. Callers stop the worker first (open handles would block the removal). */
export async function clearCache(): Promise<void> {
  if (!opfsAvailable()) return;
  try {
    const root = await navigator.storage.getDirectory();
    await root.removeEntry(CACHE_DIR, { recursive: true });
  } catch (e) {
    if ((e as DOMException)?.name !== "NotFoundError") throw e;
  }
}

/** Total bytes of the files below the cache directory. */
export async function cacheBytes(): Promise<number> {
  const d = await cacheDir();
  if (!d) return 0;
  let total = 0;
  const walk = async (dir: FileSystemDirectoryHandle) => {
    for await (const [, h] of (dir as any).entries() as AsyncIterable<[string, FileSystemHandle]>) {
      if (h.kind === "file") { try { total += (await (h as FileSystemFileHandle).getFile()).size; } catch { /* being written */ } }
      else await walk(h as FileSystemDirectoryHandle);
    }
  };
  await walk(d);
  return total;
}

export interface StorageInfo { used: number; quota: number; persisted: boolean; cache: number }
export async function storageInfo(): Promise<StorageInfo> {
  let used = 0, quota = 0, persisted = false;
  try { const e = await navigator.storage.estimate(); used = e.usage ?? 0; quota = e.quota ?? 0; } catch { /* unsupported */ }
  try { persisted = (await navigator.storage.persisted?.()) ?? false; } catch { /* unsupported */ }
  return { used, quota, persisted, cache: await cacheBytes() };
}

/** Ask the browser not to evict the cache under storage pressure (it may refuse; the import works either way). */
export async function requestPersist(): Promise<boolean> {
  try { return (await navigator.storage.persist?.()) ?? false; } catch { return false; }
}

export const fmtBytes = (n: number): string =>
  n >= 2 ** 30 ? `${(n / 2 ** 30).toFixed(2)} GB` : n >= 2 ** 20 ? `${(n / 2 ** 20).toFixed(0)} MB` : `${Math.max(0, Math.round(n / 1024))} KB`;
