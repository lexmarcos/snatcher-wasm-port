// WebAudio output for the engine's sound events (docs/re/audio.md §5):
//  - "cd"      CD-DA track (assets/web/music/trackNN.ogg), played once
//  - "fm"      FM music (loops between loop_start/loop_end) or FM sfx (assets/audio/web/fm.json)
//  - "voice"   streamed PCM voice clip (pcm.json voices[id])
//  - "pcmsfx"  PCM effect: a segment of the drum kit or of the current bank, resampled
// Files come from the user's local asset pipeline, never from the network.
type Json = any;

export class AudioOut {
  ctx: AudioContext | null = null;
  cache = new Map<string, Promise<AudioBuffer | null>>();
  pcm: Json = null;
  fm: Json = null;
  cdSrc: AudioBufferSourceNode | null = null;
  fmMusic: { src: AudioBufferSourceNode; gain: GainNode; id: number } | null = null;
  fmSfx = new Map<number, AudioBufferSourceNode>();
  voiceSrc: AudioBufferSourceNode | null = null;
  master: GainNode | null = null;

  constructor(private base = "/assets/") {
    const unlock = () => { this.ensure(); this.ctx?.resume(); };
    addEventListener("keydown", unlock); addEventListener("mousedown", unlock); addEventListener("touchstart", unlock);
  }

  /** Load the manifests; returns them so the caller can pass durations to the engine. */
  async loadManifests(): Promise<{ pcm: Json; fm: Json }> {
    const get = (p: string) => fetch(this.base + p).then((r) => (r.ok ? r.json() : null)).catch(() => null);
    [this.pcm, this.fm] = await Promise.all([get("audio/web/pcm.json"), get("audio/web/fm.json")]);
    return { pcm: this.pcm, fm: this.fm };
  }

  ensure(): AudioContext {
    if (!this.ctx) {
      this.ctx = new AudioContext();
      this.master = this.ctx.createGain();
      this.master.connect(this.ctx.destination);
    }
    return this.ctx;
  }

  load(path: string): Promise<AudioBuffer | null> {
    let p = this.cache.get(path);
    if (!p) {
      p = fetch(this.base + path)
        .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject(new Error(r.statusText))))
        .then((b) => this.ensure().decodeAudioData(b))
        .catch(() => null);
      this.cache.set(path, p);
    }
    return p;
  }

  private play(buf: AudioBuffer, opts: { loopStart?: number; loopEnd?: number; rate?: number; gain?: number } = {}) {
    const ctx = this.ensure();
    const src = ctx.createBufferSource();
    src.buffer = buf;
    if (opts.loopStart !== undefined && opts.loopEnd !== undefined) {
      src.loop = true; src.loopStart = opts.loopStart; src.loopEnd = opts.loopEnd;
    }
    if (opts.rate) src.playbackRate.value = opts.rate;
    const g = ctx.createGain();
    g.gain.value = opts.gain ?? 1;
    src.connect(g); g.connect(this.master!);
    src.start();
    return { src, gain: g };
  }

  /** CD-DA track (sequencer cmd 0E / scene code), played once. */
  async cd(track: number) {
    this.cdSrc?.stop(); this.cdSrc = null;
    const buf = await this.load(`web/music/track${String(track).padStart(2, "0")}.ogg`);
    if (buf) this.cdSrc = this.play(buf).src;
  }

  /** FM request (subfn_12): music ids 0x3F–0x7A loop; 0xFF fades the music out; F1/F2 stop. */
  async fmCmd(id: number, music: boolean) {
    const ctx = this.ensure();
    if (id === 0xff || id === 0xf2 || id === 0xf1) {
      const m = this.fmMusic; this.fmMusic = null;
      if (m) {
        const t = id === 0xff ? 85 / 60 : 0;
        m.gain.gain.setValueAtTime(m.gain.gain.value, ctx.currentTime);
        m.gain.gain.linearRampToValueAtTime(0, ctx.currentTime + t);
        m.src.stop(ctx.currentTime + t + 0.01);
      }
      if (id === 0xf1) { for (const s of this.fmSfx.values()) s.stop(); this.fmSfx.clear(); }
      return;
    }
    const info = this.fm?.sounds?.[String(id)];
    if (!info) return;
    const rate = this.fm.rate ?? 48000;
    const buf = await this.load(`audio/web/${info.file}`);
    if (!buf) return;
    const loop = info.loop_start != null ? { loopStart: info.loop_start / rate, loopEnd: info.loop_end / rate } : {};
    if (info.music || music) {
      if (this.fmMusic?.id === id) return;
      this.fmMusic?.src.stop();
      const p = this.play(buf, loop);
      this.fmMusic = { ...p, id };
    } else {
      this.fmSfx.get(id)?.stop();
      this.fmSfx.set(id, this.play(buf, loop).src);
    }
  }

  async voice(id: number) {
    const v = this.pcm?.voices?.[String(id)];
    if (!v) return;
    const buf = await this.load(`audio/web/${v.file}`);
    if (!buf) return;
    this.voiceSrc?.stop();
    this.voiceSrc = this.play(buf, { gain: (v.env ?? 255) / 255 }).src;
  }

  async pcmSfx(id: number, bank: number) {
    if (id === 0xff) { this.voiceSrc?.stop(); return; }
    const s = this.pcm?.sfx?.[String(id)];
    if (!s) return;
    const seg = s.src === "drum" ? this.pcm.drum?.[s.offset] : this.pcm.banks?.[String(bank)]?.[s.offset];
    if (!seg) return;
    const buf = await this.load(`audio/web/${seg.file}`);
    if (!buf) return;
    this.play(buf, { rate: s.rate_hz / seg.rate, gain: (s.env ?? 255) / 255 });
  }
}
