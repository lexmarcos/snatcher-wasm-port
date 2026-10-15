// bun test web/test/pace.test.ts – frame pacing of the fixed 60 Hz step on other display refresh rates.
import { expect, test } from "bun:test";
import { Pacer } from "../src/pacer";

function run(hz: number, seconds: number, jitterMs = 0, seed = 1) {
  const p = new Pacer();
  let rnd = seed;
  const rand = () => ((rnd = (rnd * 1664525 + 1013904223) >>> 0) / 2 ** 32) * 2 - 1;
  let ticks = 0, maxRun = 0, zeroRuns = 0, zeros = 0, frames = 0;
  const per: number[] = [];
  for (let t = 0; t < seconds * 1000; t += 1000 / hz) {
    const n = p.advance(t + rand() * jitterMs);
    ticks += n; frames++; per.push(n);
    maxRun = Math.max(maxRun, n);
    if (n === 0) { zeros++; zeroRuns = Math.max(zeroRuns, zeros); } else zeros = 0;
  }
  return { ticks, maxRun, zeroRuns, frames, per };
}

test("60 Hz with jitter: exactly one tick per frame", () => {
  const r = run(60, 20, 0.6);
  expect(r.per.slice(1).every((n) => n === 1)).toBe(true);
});
test("120 Hz: 60 ticks per second, never two ticks in one frame, no gap longer than one frame", () => {
  const r = run(120, 20, 0.25);
  expect(Math.abs(r.ticks - 1200)).toBeLessThanOrEqual(2);
  expect(r.maxRun).toBe(1);
  expect(r.zeroRuns).toBeLessThanOrEqual(2); // 2 only at start-up (first frame primes the clock)
  expect(r.per.slice(3).some((n, i, a) => n === 0 && a[i + 1] === 0)).toBe(false);
});
test("144 Hz: average 60 ticks per second", () => {
  const r = run(144, 20, 0.3);
  expect(Math.abs(r.ticks - 1200)).toBeLessThanOrEqual(2);
  expect(r.maxRun).toBe(1);
});
test("30 Hz: two ticks per frame", () => {
  const r = run(30, 10, 0.5);
  expect(Math.abs(r.ticks - 600)).toBeLessThanOrEqual(3);
  expect(r.maxRun).toBe(2);
});
test("a long stall is dropped, not replayed", () => {
  const p = new Pacer();
  p.advance(0); p.advance(16.7);
  expect(p.advance(5000)).toBeLessThanOrEqual(6);
});
