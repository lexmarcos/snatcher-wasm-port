// Fixed-step pacer: the engine advances in exact 1/60 s steps whatever the display refresh rate.
// advance(now) takes the requestAnimationFrame timestamp and returns how many engine ticks to run.
// Timestamps that are close to a common refresh period (60/120/180 Hz) are snapped to it, so jitter on a
// 60 Hz display never produces a 0-tick or 2-tick frame, and a 120 Hz display alternates 0/1 ticks evenly.
export class Pacer {
  private acc = 0;
  private last: number | null = null;
  /** Longest wall-clock gap that is caught up (ms); larger gaps (hidden tab, debugger) are dropped. */
  constructor(readonly step = 1000 / 60, readonly maxCatchup = 100) {}

  reset() { this.last = null; this.acc = 0; }

  advance(now: number): number {
    if (this.last === null) { this.last = now; return 0; }
    let dt = Math.max(0, now - this.last);
    this.last = now;
    dt = Math.min(dt, this.maxCatchup);
    for (const k of [0.5, 1, 2, 3]) {
      const p = this.step / k;
      if (Math.abs(dt - p) < Math.min(p * 0.08, 1.4)) { dt = p; break; }
    }
    this.acc += dt;
    let n = 0;
    while (this.acc >= this.step - 1e-6) { this.acc -= this.step; n++; }
    if (this.acc < 0) this.acc = 0;
    return n;
  }
}
