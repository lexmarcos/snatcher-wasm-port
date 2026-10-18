// bun test web/test/autosave.test.ts – when the autosaver writes (edges, scene changes, throttle, flush, off), the store without IndexedDB, the setting and the error codes.
import { afterEach, expect, test } from "bun:test";
import { Autosaver, AutosaveStore, EVERY_MS, MIN_GAP_MS, PREF_KEY, RECORD_VERSION, readPref, reasonKey, type AutoRecord, type Snapper } from "../src/autosave";

const realNow = Date.now;
let clock = 1_000_000;
Date.now = () => clock;
afterEach(() => { clock = 1_000_000; });
void realNow;

class FakeGame implements Snapper {
  point = 0; key = 1;
  taken = 0;
  autosave_point() { return this.point; }
  autosave_key() { return this.key; }
  snapshot() { this.taken++; return new Uint8Array(200).fill(7); }
  snapshot_info() { return JSON.stringify({ kind: this.point, act: 1, pt: false, play: "00:01:02", script: 63, scene: this.key, frame: this.taken }); }
}

function setup() {
  const g = new FakeGame();
  const store = new AutosaveStore();
  const done: (AutoRecord | null)[] = [];
  const saver = new Autosaver(store, () => g, () => ({ fm: true, pcm: false }), (r) => done.push(r));
  return { g, store, saver, done };
}
const settle = () => new Promise((r) => setTimeout(r, 5));

test("a first quiet moment is saved at once; the record carries the engine's info and the audio state", async () => {
  const { g, saver, store, done } = setup();
  g.point = 0; saver.poll(); await settle();
  expect(done.length).toBe(0);
  g.point = 1; saver.poll(); await settle();
  expect(done.length).toBe(1);
  const r = (await store.get())!;
  expect(r.v).toBe(RECORD_VERSION);
  expect(r.act).toBe(1); expect(r.play).toBe("00:01:02"); expect(r.kind).toBe(1); expect(r.mono).toEqual({ fm: true, pcm: false }); expect(r.data.length).toBe(200);
  expect(saver.stats.saves).toBe(1); expect(saver.stats.lastBytes).toBe(200);
});

test("staying at the same safe point does not save again; a new quiet moment in the same scene waits for the long interval", async () => {
  const { g, saver, done } = setup();
  g.point = 1; saver.poll(); await settle();
  for (let i = 0; i < 100; i++) { clock += 1000; saver.poll(); }
  await settle();
  expect(done.length).toBe(1);                       // idle at the menu: nothing new to save
  g.point = 0; saver.poll(); g.point = 2; clock += 1000; saver.poll(); await settle();
  expect(done.length).toBe(2);                       // a new moment, same scene, 100 s after the last save: above the interval
});

test("same scene: a new quiet moment is saved only after EVERY_MS; a scene change after MIN_GAP_MS", async () => {
  const { g, saver, done } = setup();
  g.point = 1; saver.poll(); await settle();
  expect(done.length).toBe(1);
  const step = async (point: number) => { g.point = 0; saver.poll(); g.point = point; saver.poll(); await settle(); };
  clock += 2000; await step(2);
  expect(done.length).toBe(1);                       // same scene, 2 s later
  clock += EVERY_MS - 2000; await step(1);           // still the same moment of waiting: want stays set, EVERY_MS reached
  expect(done.length).toBe(2);
  g.key = 2;                                         // scene change
  clock += MIN_GAP_MS - 1; await step(2);
  expect(done.length).toBe(2);                       // too soon after the last save
  clock += 5; saver.poll(); await settle();          // the wish stays while the game waits at the safe point
  expect(done.length).toBe(3);
});

test("autosave off writes nothing, on again resumes with the next quiet moment", async () => {
  const { g, saver, done } = setup();
  saver.on = false;
  g.point = 1; saver.poll(); g.point = 0; saver.poll(); g.point = 2; saver.poll(); await settle();
  expect(done.length).toBe(0);
  await saver.flush(); await settle();
  expect(done.length).toBe(0);
  saver.on = true; g.point = 0; saver.poll(); g.point = 1; saver.poll(); await settle();
  expect(done.length).toBe(1);
});

test("flush (page hidden or closed) saves only at a safe point and not twice within a second", async () => {
  const { g, saver, done } = setup();
  await saver.flush(); await settle();
  expect(done.length).toBe(0);                       // not at a safe point: the last one stands
  g.point = 2; await saver.flush(); await settle();
  expect(done.length).toBe(1);
  clock += 500; await saver.flush(); await settle();
  expect(done.length).toBe(1);
  clock += 600; await saver.flush(); await settle();
  expect(done.length).toBe(2);
});

test("adopt: a restored game counts as saved (no immediate second save), a new game as not saved", async () => {
  const { g, saver, done } = setup();
  saver.adopt(g, true);
  g.point = 1; saver.poll(); await settle();
  expect(done.length).toBe(0);
  clock += EVERY_MS + 1; saver.poll(); await settle();
  expect(done.length).toBe(1);
});

test("without IndexedDB the store keeps the record in memory and says so", async () => {
  const s = new AutosaveStore();
  expect(await s.get()).toBeNull();
  expect(s.volatile).toBe(true);
  const rec: AutoRecord = { v: RECORD_VERSION, stamp: 5, act: 2, play: "01:00:00", kind: 1, pt: true, mono: { fm: false, pcm: false }, data: new Uint8Array(100) };
  await s.put(rec);
  expect((await s.get())!.stamp).toBe(5);
  await s.clear();
  expect(await s.get()).toBeNull();
  await AutosaveStore.wipe(); // no IndexedDB: nothing to do, no throw
});

test("a failing snapshot is counted and reported, and the autosaver keeps working", async () => {
  const { g, saver, done } = setup();
  g.snapshot = () => { throw new Error("boom"); };
  g.point = 1; saver.poll(); await settle();
  expect(done).toEqual([null]); expect(saver.stats.failures).toBe(1);
  g.snapshot = () => new Uint8Array(150);
  g.point = 0; saver.poll(); g.point = 1; g.key = 9; clock += MIN_GAP_MS + 1; saver.poll(); await settle();
  expect(done.length).toBe(2); expect(done[1]!.data.length).toBe(150);
});

test("the setting is on unless it was turned off; error codes map to the three messages", () => {
  const ls = (v: string | null) => (k: string) => (k === PREF_KEY ? v : null);
  expect(readPref(ls(null))).toBe(true);
  expect(readPref(ls("1"))).toBe(true);
  expect(readPref(ls("0"))).toBe(false);
  expect(PREF_KEY).toBe("snatcher.autosave.on");
  expect(reasonKey("engine")).toBe("auto.err.engine");
  expect(reasonKey("format")).toBe("auto.err.engine");
  expect(reasonKey("disc")).toBe("auto.err.disc");
  expect(reasonKey("missing:SP06.BIN")).toBe("auto.err.disc");
  expect(reasonKey("corrupt:checksum")).toBe("auto.err.bad");
});
