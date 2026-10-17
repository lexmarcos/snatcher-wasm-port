// Keeps the CD-DA audio on the engine's CD clock (docs/voice-subtitles.md "CD-DA speech"). The engine counts frames since it started the track (60 Hz, the same
// clock the scene timelines and the CD-DA subtitles use); the audio runs on the audio hardware clock. Two clocks never agree for long (a display at 59.94 Hz makes
// the engine 0.1 % slower than the audio: 0.4 s over a 7 minute track), so the track's playback rate is nudged (at most +-0.3 %, about 5 cents, inaudible) to follow
// the engine, and a track that starts late (the file has to be decoded first) starts at the position the engine is already at.
export class CdClock {
  /** track position `pos` (s) at audio time `t` (s), playing at `rate` */
  constructor(public t = 0, public pos = 0, public rate = 1) {}
  position(now: number) { return this.pos + (now - this.t) * this.rate; }
  /** Rebase on `now` and return the playback rate that closes the gap to the engine position `enginePos` (s): a dead band of 15 ms, a time constant of 20 s. */
  control(now: number, enginePos: number, maxDev = 0.003) {
    const p = this.position(now);
    const err = enginePos - p;                       // > 0: the audio is behind the engine
    const dev = Math.abs(err) < 0.015 ? 0 : Math.max(-maxDev, Math.min(maxDev, err * 0.05));
    this.pos = p; this.t = now; this.rate = 1 + dev;
    return this.rate;
  }
}
