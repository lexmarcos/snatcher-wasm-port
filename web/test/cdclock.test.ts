// bun test web/test/cdclock.test.ts – the CD-DA audio follows the engine's CD clock over a whole track (drift check, see docs/voice-subtitles.md "CD-DA speech").
import { expect, test } from "bun:test";
import { Pacer } from "../src/pacer";
import { CdClock } from "../src/cdclock";

/** Simulate a display of `hz` Hz for `seconds` of wall time: the engine ticks through the pacer, the audio plays on the (true) wall clock. Returns the worst |engine - audio| (s). */
function drift(hz: number, seconds: number, control: boolean, startLate = 0) {
  const p = new Pacer();
  let ticks = 0, worst = 0, lastCtl = 0;
  const clock = new CdClock(startLate, startLate);        // audio started `startLate` s after the engine's track (decoding), at the engine's position
  for (let t = 0; t < seconds * 1000; t += 1000 / hz) {
    ticks += p.advance(t);
    const now = t / 1000, eng = ticks / 60;
    if (control && now - lastCtl >= 0.5) { lastCtl = now; clock.control(now, eng); }
    if (now > startLate + 30) worst = Math.max(worst, Math.abs(eng - clock.position(now)));
  }
  return worst;
}

test("a 59.94 Hz display drifts 0.4 s over a 7 minute track without the correction and stays within 50 ms with it", () => {
  const free = drift(59.94, 420, false);
  expect(free).toBeGreaterThan(0.3);
  expect(drift(59.94, 420, true)).toBeLessThan(0.05);
});

test("60 Hz, 144 Hz and 75 Hz displays stay within 50 ms", () => {
  for (const hz of [60, 144, 75, 59.94, 60.06]) expect(drift(hz, 420, true)).toBeLessThan(0.05);
});

test("the rate never leaves +-0.3 %", () => {
  const c = new CdClock(0, 0);
  for (let t = 0.5; t < 100; t += 0.5) { const r = c.control(t, t + 3); expect(Math.abs(r - 1)).toBeLessThanOrEqual(0.003 + 1e-12); }
});
