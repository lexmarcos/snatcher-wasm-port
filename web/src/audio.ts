// WebAudio output for the engine's sound events (docs/re/audio.md §5):
//  - "cd"      CD-DA track (web/music/trackNN.ogg), played once
//  - "fm"      FM music (loops between loop_start/loop_end) or FM sfx (audio/web/fm.json); ids F1/F2/F3/F4/FF are commands
//  - "voice"   streamed PCM voice clip (pcm.json voices[id])
//  - "pcmsfx"  PCM effect: a segment of the drum kit or of the current bank, resampled; 0xFC/0xFD/0xFF are commands
// Files come from the user's own assets through a reader function (local server or picked folder), never a CDN.
//
// Graph:  CD ─ cdBus ───────────────┐
//         FM music ─ fmMusicBus ┐   │
//         FM sfx ──── fmSfxBus ─┴ fmMix(F3/F4) ─┤
//         voice ──── voiceBus ┐                  ├ master → destination
//         PCM sfx ── pcmSfxBus ┴ pcmMix(FC/FD) ─┘
// Sound mode (docs/re/audio.md §1.3 / §4: FM F3 = force centre pan, F4 = stereo; PCM FC = mono, FD = stereo): the
// fmMix / pcmMix node is forced to one channel in mono mode, which the Web Audio "speakers" mixing rule turns into
// 0.5 (L+R) on both outputs. CD-DA does not pass through the sound drivers, so it is never downmixed (HYPOTHESIS:
// the original option only affects the FM and PCM drivers; docs/re/modules.md §Status said "no audible effect yet").
import { CdClock } from "./cdclock";

type Json = any;
export type Reader = (path: string) => Promise<ArrayBuffer | null>;
export type VolumeKind = "music" | "voice" | "fx";

/** LRU cache of decoded buffers, capped by decoded size (a 4-minute CD track is ~70 MB as float32 stereo). */
class BufferCache {
  private m = new Map<string, { p: Promise<AudioBuffer | null>; bytes: number }>();
  total = 0;
  constructor(public maxBytes = 320 * 2 ** 20, public maxEntries = 400) {}
  get(k: string) {
    const e = this.m.get(k);
    if (e) { this.m.delete(k); this.m.set(k, e); }
    return e?.p;
  }
  set(k: string, p: Promise<AudioBuffer | null>) {
    const e = { p, bytes: 0 };
    this.m.set(k, e);
    p.then((b) => {
      if (!b || this.m.get(k) !== e) return;
      e.bytes = b.length * b.numberOfChannels * 4; this.total += e.bytes;
      this.evict(k);
    });
  }
  private evict(keep: string) {
    for (const [k, e] of this.m) {
      if (this.total <= this.maxBytes && this.m.size <= this.maxEntries) break;
      if (k === keep) continue;
      this.m.delete(k); this.total -= e.bytes;
    }
  }
  drop(k: string) { const e = this.m.get(k); if (e) { this.m.delete(k); this.total -= e.bytes; } }
  get size() { return this.m.size; }
}

export class AudioOut {
  ctx: AudioContext | null = null;
  cache = new BufferCache();
  pcm: Json = null;
  fm: Json = null;
  /** Optional hook for sources that hold decoded audio (the disc importer): returns the AudioBuffer for a path, or undefined to use `read` + decodeAudioData. */
  direct: ((path: string, ctx: BaseAudioContext) => Promise<AudioBuffer | null> | undefined) | null = null;
  /** Called once per file that could not be loaded/decoded (path relative to assets/). */
  onMissing: ((path: string) => void) | null = null;
  missing = new Set<string>();

  cdSrc: AudioBufferSourceNode | null = null;
  private cdClock: CdClock | null = null;
  private cdCtl = 0;
  private fmMusic: { src: AudioBufferSourceNode; gain: GainNode; id: number } | null = null;
  private fmSfx = new Map<number, AudioBufferSourceNode>();
  private voiceSrc: AudioBufferSourceNode | null = null;
  private pcmSfxSrc = new Set<AudioBufferSourceNode>();
  // generation counters: a stop/replace command made while a file is still loading cancels the older request
  private cdGen = 0; private musicGen = 0; private sfxGen = 0; private voiceGen = 0;

  private nodes: Record<string, GainNode> = {};
  private vol: Record<VolumeKind, number> = { music: 1, voice: 1, fx: 1 };
  private muted = false;
  /** Sound mode requested by the game (true = mono). Default stereo (F4/FD are sent at power-on). */
  mono = { fm: false, pcm: false };

  constructor(private read: Reader) {
    const unlock = () => { this.ensure(); void this.ctx?.resume(); };
    for (const ev of ["keydown", "pointerdown", "mousedown", "touchstart"]) addEventListener(ev, unlock, { passive: true });
  }

  /** Load the manifests; returns them so the caller can pass durations to the engine. Missing ones give null. */
  async loadManifests(): Promise<{ pcm: Json; fm: Json }> {
    const get = async (p: string) => {
      try { const b = await this.read(p); return b ? JSON.parse(new TextDecoder().decode(b)) : null; } catch { return null; }
    };
    [this.pcm, this.fm] = await Promise.all([get("audio/web/pcm.json"), get("audio/web/fm.json")]);
    return { pcm: this.pcm, fm: this.fm };
  }

  // ---- context / graph ----

  ensure(): AudioContext {
    if (!this.ctx) {
      const ctx = this.ctx = new AudioContext();
      const mk = (name: string, to?: AudioNode) => { const g = ctx.createGain(); g.connect(to ?? ctx.destination); this.nodes[name] = g; return g; };
      const master = mk("master");
      mk("cdBus", master);
      const fmMix = mk("fmMix", master); mk("fmMusicBus", fmMix); mk("fmSfxBus", fmMix);
      const pcmMix = mk("pcmMix", master); mk("voiceBus", pcmMix); mk("pcmSfxBus", pcmMix);
      for (const m of [fmMix, pcmMix]) { m.channelCountMode = "explicit"; m.channelInterpretation = "speakers"; }
      this.applyVolumes(); this.applyMode();
    }
    return this.ctx;
  }

  private applyVolumes() {
    if (!this.ctx) return;
    const g = (k: VolumeKind) => this.vol[k] * this.vol[k]; // slider → perceptual curve
    this.nodes.master.gain.value = this.muted ? 0 : 1;
    this.nodes.cdBus.gain.value = this.nodes.fmMusicBus.gain.value = g("music");
    this.nodes.voiceBus.gain.value = g("voice");
    this.nodes.fmSfxBus.gain.value = this.nodes.pcmSfxBus.gain.value = g("fx");
  }
  private applyMode() {
    if (!this.ctx) return;
    this.nodes.fmMix.channelCount = this.mono.fm ? 1 : 2;
    this.nodes.pcmMix.channelCount = this.mono.pcm ? 1 : 2;
  }

  setVolume(kind: VolumeKind, v: number) { this.vol[kind] = Math.max(0, Math.min(1, v)); this.applyVolumes(); }
  getVolume(kind: VolumeKind) { return this.vol[kind]; }
  setMuted(m: boolean) { this.muted = m; this.applyVolumes(); }
  get isMuted() { return this.muted; }
  /** Sound mode from the game's option (FM F3/F4, PCM FC/FD). */
  setSoundMode(part: "fm" | "pcm", mono: boolean) { this.mono[part] = mono; this.applyMode(); }
  /** Pause/resume everything (tab hidden, menu open). */
  suspend() { void this.ctx?.suspend(); }
  resume() { void this.ctx?.resume(); }

  debugInfo(): string {
    return `audio: ${this.ctx?.state ?? "off"} fm=${this.mono.fm ? "mono" : "stereo"} pcm=${this.mono.pcm ? "mono" : "stereo"}` +
      ` cache=${this.cache.size} (${(this.cache.total / 2 ** 20).toFixed(0)} MB) missing=${this.missing.size}`;
  }

  // ---- loading ----

  load(path: string): Promise<AudioBuffer | null> {
    let p = this.cache.get(path);
    if (!p) {
      if (this.missing.has(path)) return Promise.resolve(null);
      const ctx = this.ensure();
      p = (this.direct?.(path, ctx) ?? this.read(path).then((b) => (b ? ctx.decodeAudioData(b) : null)))
        .then((b) => (b ? b : Promise.reject(new Error("not found"))))
        .catch(() => {
          this.cache.drop(path);
          if (!this.missing.has(path)) { this.missing.add(path); this.onMissing?.(path); }
          return null;
        });
      this.cache.set(path, p);
    }
    return p;
  }

  private play(buf: AudioBuffer, bus: string, opts: { loopStart?: number; loopEnd?: number; rate?: number; gain?: number; offset?: number } = {}) {
    const ctx = this.ensure();
    const src = ctx.createBufferSource();
    src.buffer = buf;
    if (opts.loopStart !== undefined && opts.loopEnd !== undefined) {
      src.loop = true; src.loopStart = opts.loopStart; src.loopEnd = opts.loopEnd;
    }
    if (opts.rate) src.playbackRate.value = opts.rate;
    const g = ctx.createGain();
    g.gain.value = opts.gain ?? 1;
    src.connect(g); g.connect(this.nodes[bus]);
    src.onended = () => { src.disconnect(); g.disconnect(); };
    src.start(0, Math.min(opts.offset ?? 0, Math.max(0, buf.duration - 0.001)));
    return { src, gain: g };
  }

  private static stop(s: AudioScheduledSourceNode | null | undefined) { try { s?.stop(); } catch { /* already stopped */ } }

  // ---- engine events ----

  /** CD-DA track (sequencer cmd 0E / scene code), played once. */
  async cd(track: number) {
    const gen = ++this.cdGen;
    AudioOut.stop(this.cdSrc); this.cdSrc = null; this.cdClock = null;
    // the engine's CD clock starts now; the file may still have to be fetched and decoded (seconds for a long track): start where the engine is by then
    const t0 = this.ensure().currentTime;
    const buf = await this.load(`web/music/track${String(track).padStart(2, "0")}.ogg`);
    if (buf && gen === this.cdGen) {
      const late = Math.max(0, this.ctx!.currentTime - t0);   // the audio clock stands still while suspended, like the engine
      if (late >= buf.duration) return;
      this.cdSrc = this.play(buf, "cdBus", { offset: late }).src;
      this.cdClock = new CdClock(this.ctx!.currentTime, late); this.cdCtl = this.ctx!.currentTime;
    }
  }
  stopCd() { this.cdGen++; AudioOut.stop(this.cdSrc); this.cdSrc = null; this.cdClock = null; }
  /** Called every frame with the engine's CD clock (seconds since it started the track, -1 when none): slews the audio rate towards it. */
  syncCd(enginePos: number) {
    const c = this.cdClock, src = this.cdSrc;
    if (!c || !src || !this.ctx || enginePos < 0) return;
    const now = this.ctx.currentTime;
    if (now - this.cdCtl < 0.5) return;
    this.cdCtl = now;
    src.playbackRate.value = c.control(now, enginePos);
  }
  /** Gap (s) between the engine's CD clock and the audio position; for the drift test and the debug panel. */
  cdOffset(enginePos: number) { return this.cdClock && this.ctx && enginePos >= 0 ? enginePos - this.cdClock.position(this.ctx.currentTime) : null; }

  /**
   * FM request (subfn_12): music ids 0x3F–0x7A loop; 0xFF fades the music out (85 frames); F1 stops all FM sound
   * (M:0xFF52AC), F2 stops the music (M:0xFF51E4), F3/F4 = mono/stereo (M:0xFF51A8).
   */
  async fmCmd(id: number, music: boolean) {
    if (id === 0xf3 || id === 0xf4) { this.setSoundMode("fm", id === 0xf3); return; }
    if (id === 0xff || id === 0xf2 || id === 0xf1) {
      this.musicGen++;
      const m = this.fmMusic; this.fmMusic = null;
      if (m && this.ctx) {
        const t = id === 0xff ? 85 / 60 : 0;
        const now = this.ctx.currentTime;
        m.gain.gain.setValueAtTime(m.gain.gain.value, now);
        m.gain.gain.linearRampToValueAtTime(0, now + t);
        if (t > 0) m.src.stop(now + t + 0.01); else AudioOut.stop(m.src);
      }
      if (id === 0xf1) { this.sfxGen++; for (const s of this.fmSfx.values()) AudioOut.stop(s); this.fmSfx.clear(); }
      return;
    }
    const info = this.fm?.sounds?.[String(id)];
    if (!info) return;
    const asMusic = !!(info.music || music);
    if (asMusic && this.fmMusic?.id === id) return; // already playing: the original does not restart it
    const gen = asMusic ? ++this.musicGen : this.sfxGen;
    const rate = this.fm.rate ?? 48000;
    const buf = await this.load(`audio/web/${info.file}`);
    if (!buf) return;
    const loop = info.loop_start != null ? { loopStart: info.loop_start / rate, loopEnd: info.loop_end / rate } : {};
    if (asMusic) {
      if (gen !== this.musicGen) return; // stopped or replaced while loading
      AudioOut.stop(this.fmMusic?.src);
      this.fmMusic = { ...this.play(buf, "fmMusicBus", loop), id };
    } else {
      if (gen !== this.sfxGen) return;
      AudioOut.stop(this.fmSfx.get(id));
      const s = this.play(buf, "fmSfxBus", loop).src;
      this.fmSfx.set(id, s);
      s.addEventListener("ended", () => { if (this.fmSfx.get(id) === s) this.fmSfx.delete(id); });
    }
  }

  async voice(id: number) {
    const v = this.pcm?.voices?.[String(id)];
    if (!v) return;
    const gen = ++this.voiceGen;
    const buf = await this.load(`audio/web/${v.file}`);
    if (!buf || gen !== this.voiceGen) return;
    AudioOut.stop(this.voiceSrc);
    this.voiceSrc = this.play(buf, "voiceBus", { gain: (v.env ?? 255) / 255 }).src;
  }

  /** PCM effect (subfn_06): 0xFF stops all PCM sound, 0xFC/0xFD = mono/stereo (S:0x162E0 command table). */
  async pcmSfx(id: number, bank: number) {
    if (id === 0xfc || id === 0xfd) { this.setSoundMode("pcm", id === 0xfc); return; }
    if (id === 0xff) {
      this.voiceGen++; AudioOut.stop(this.voiceSrc); this.voiceSrc = null;
      for (const s of this.pcmSfxSrc) AudioOut.stop(s);
      this.pcmSfxSrc.clear();
      return;
    }
    const s = this.pcm?.sfx?.[String(id)];
    if (!s) return;
    const seg = s.src === "drum" ? this.pcm.drum?.[s.offset] : this.pcm.banks?.[String(bank)]?.[s.offset];
    if (!seg) return;
    const buf = await this.load(`audio/web/${seg.file}`);
    if (!buf) return;
    const src = this.play(buf, "pcmSfxBus", { rate: s.rate_hz / seg.rate, gain: (s.env ?? 255) / 255 }).src;
    this.pcmSfxSrc.add(src);
    src.addEventListener("ended", () => this.pcmSfxSrc.delete(src));
  }
}
