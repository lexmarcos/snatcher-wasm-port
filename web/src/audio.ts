// WebAudio output: CD-DA music tracks, voice clips and sound effects.
// Files come from the local asset pipeline (assets/web/...), never from the network.
export class AudioOut {
  ctx: AudioContext | null = null;
  musicGain: GainNode | null = null;
  musicSrc: AudioBufferSourceNode | null = null;
  voiceSrc: AudioBufferSourceNode | null = null;
  cache = new Map<string, Promise<AudioBuffer | null>>();
  currentTrack = -1;

  constructor(private base: string) {
    const unlock = () => { this.ensure(); this.ctx?.resume(); };
    addEventListener("keydown", unlock); addEventListener("mousedown", unlock); addEventListener("touchstart", unlock);
  }

  ensure(): AudioContext {
    if (!this.ctx) {
      this.ctx = new AudioContext();
      this.musicGain = this.ctx.createGain();
      this.musicGain.connect(this.ctx.destination);
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

  async music(track: number, loop: boolean) {
    if (track === this.currentTrack && this.musicSrc) return;
    this.stopMusic(0);
    this.currentTrack = track;
    const buf = await this.load(`music/track${String(track).padStart(2, "0")}.ogg`);
    if (!buf || this.currentTrack !== track) return;
    const ctx = this.ensure();
    const src = ctx.createBufferSource();
    src.buffer = buf; src.loop = loop;
    this.musicGain!.gain.cancelScheduledValues(ctx.currentTime);
    this.musicGain!.gain.setValueAtTime(1, ctx.currentTime);
    src.connect(this.musicGain!); src.start();
    this.musicSrc = src;
  }

  stopMusic(fadeFrames: number) {
    this.currentTrack = -1;
    const src = this.musicSrc; this.musicSrc = null;
    if (!src || !this.ctx) return;
    const t = fadeFrames / 60;
    if (t > 0) {
      this.musicGain!.gain.setValueAtTime(this.musicGain!.gain.value, this.ctx.currentTime);
      this.musicGain!.gain.linearRampToValueAtTime(0, this.ctx.currentTime + t);
      src.stop(this.ctx.currentTime + t);
    } else src.stop();
  }

  async voice(id: number) {
    const buf = await this.load(`voice/${id}.ogg`);
    if (!buf) return;
    const ctx = this.ensure();
    this.voiceSrc?.stop();
    const src = ctx.createBufferSource(); src.buffer = buf; src.connect(ctx.destination); src.start();
    this.voiceSrc = src;
  }

  async sfx(id: number) {
    const buf = await this.load(`sfx/${id}.ogg`);
    if (!buf) return;
    const ctx = this.ensure();
    const src = ctx.createBufferSource(); src.buffer = buf; src.connect(ctx.destination); src.start();
  }
}
