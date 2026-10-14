//! Offline renderer: Snatcher FM driver (kdriver) + YM2612 + PCM drum kit → 48 kHz stereo.
//! Used by `snt audio-fm` to pre-render music (with loop points) and FM sound effects.
//!
//! Loop detection: every track is an independent sequencer; when a track's control state
//! (`KDriver::track_key`, sampled at that track's own ticks) repeats, its future repeats too.
//! The joint loop of a song is the LCM of the track periods, starting after the latest track
//! start. If that is too long (tracks drifting by a tick per loop) or a track never repeats
//! exactly, an approximate loop (longest per-track period, optionally ignoring accumulated
//! loop transposes) is used and flagged.

use super::kdriver::KDriver;
use super::ym2612::CHIP_RATE;
use super::{PcmSfxEntry, parse_segments, DRUM_BASE};
use std::collections::HashMap;

pub const OUT_RATE: u32 = 48_000;
/// NTSC VBlank rate (the driver ticks once per VBlank).
pub const FRAME_RATE: f64 = 59.922_743;
/// seconds rendered past the loop end to verify the loop (correlation check)
pub const VERIFY_SECONDS: f64 = 4.0;
/// longest accepted exact joint loop (ticks)
pub const MAX_LOOP_TICKS: usize = 60 * 60 * 6;

fn gcd(a: usize, b: usize) -> usize { if b == 0 { a } else { gcd(b, a % b) } }
fn lcm(a: usize, b: usize) -> usize { a / gcd(a, b) * b }

/// PCM drum kit as used by music slots 6/7 (sfx ids 1..12, S:0x160BA / S:0x161F4).
pub struct DrumKit { pub notes: Vec<Option<(Vec<i16>, f64, u8, u8)>> } // (samples, step per chip sample, env, pan)

impl DrumKit {
    pub fn new(sfx: &[PcmSfxEntry], drmdt: &[u8]) -> Self {
        let segs = parse_segments(drmdt);
        let mut notes = vec![None; 13];
        for n in 1..13u8 {
            if [6, 8, 9, 10].contains(&n) { continue } // skipped by S:0x161F4
            let Some(e) = sfx.iter().find(|e| e.id == n) else { continue };
            if e.addr < DRUM_BASE { continue }
            let off = (e.addr - DRUM_BASE) as usize;
            if let Some(g) = segs.iter().find(|g| g.offset == off) {
                let step = (e.fd as f64 / 2048.0) * (super::PCM_CLOCK_HZ / CHIP_RATE);
                notes[n as usize] = Some((g.samples.clone(), step, e.env, e.pan));
            }
        }
        DrumKit { notes }
    }
}

struct Voice { note: usize, pos: f64, step: f64, gl: i32, gr: i32 }

#[derive(Default)]
struct TrackHist {
    exact: HashMap<Vec<u8>, usize>,
    relaxed: HashMap<Vec<u8>, usize>,
    ex: Option<(usize, usize)>,
    rx: Option<(usize, usize)>,
    frames: Vec<usize>,
    last: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoopKind { Exact, Approx }

pub struct Rendered {
    pub samples: Vec<i16>,
    pub loop_start: Option<usize>,
    pub loop_end: usize,
    pub frames: usize,
    pub loop_corr: Option<f64>,
    pub loop_kind: Option<LoopKind>,
}

/// Render sound `id`; `max_frames` bounds the run.
pub fn render(drv: &mut KDriver, kit: &DrumKit, id: u8, max_frames: usize, gain: f64) -> Rendered {
    let no_drums = std::env::var("SNT_NO_DRUMS").is_ok();
    if let Ok(m) = std::env::var("SNT_FM_MUTE") { drv.ym.mute = u8::from_str_radix(&m, 16).unwrap_or(0); }
    drv.request(id);
    let is_music = drv.sound_type(id) & 0x0F != 0;
    let slots: Vec<usize> = if is_music { (0..8).collect() } else { (8..10).filter(|&s| drv.track_active(s as u32)).collect() };
    let refslot = slots.iter().copied().find(|&s| drv.track_active(s as u32)).unwrap_or(0);
    let spf = CHIP_RATE / FRAME_RATE;
    let mut acc = 0.0;
    let mut chip: Vec<(i32, i32)> = Vec::new();
    let mut frame_start_chip = Vec::new();
    let mut hist: Vec<TrackHist> = (0..10).map(|_| TrackHist::default()).collect();
    let mut target: Option<(usize, usize)> = None; // (start tick, length) in refslot ticks
    let mut loop_frames: Option<(usize, usize, LoopKind)> = None;
    let mut extra_until = usize::MAX;
    let mut voices: [Option<Voice>; 2] = [None, None];
    let mut tail = 0usize;
    let mut f = 0usize;
    while f < max_frames {
        // ---- loop bookkeeping (state after the previous frame's tick)
        for &tr in &slots {
            let h = &mut hist[tr];
            if drv.ticks[tr] == h.last { continue }
            h.last = drv.ticks[tr];
            let t = h.frames.len();
            h.frames.push(f);
            if h.ex.is_none() {
                let key = drv.track_key(tr as u32);
                if let Some(&g) = h.exact.get(&key) { h.ex = Some((g, t - g)); } else { h.exact.insert(key.clone(), t); }
                if h.rx.is_none() {
                    let mut rk = key;
                    // relaxed: ignore loop transposes $34..$3B and $52..$53 (key layout skips some
                    // offsets; recompute from raw offsets instead)
                    rk.clear();
                    let a = 0xFF7A00 + tr as u32 * 0x80;
                    for off in [0x00u32, 0x02, 0x03, 0x04, 0x1E, 0x1B, 0x1C, 0x22, 0x58, 0x59, 0x5A, 0x5B, 0x5C, 0x5D, 0x5E, 0x5F,
                                0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F] {
                        rk.push(drv.peek(a + off));
                    }
                    if let Some(&g) = h.relaxed.get(&rk) { h.rx = Some((g, t - g)); } else { h.relaxed.insert(rk, t); }
                }
            }
        }
        if target.is_none() && f > 8 && slots.iter().all(|&tr| hist[tr].ex.is_some() || !drv.track_active(tr as u32)) {
            let (mut l, mut sf) = (1usize, 0usize);
            for &tr in &slots { if let Some((g, p)) = hist[tr].ex { l = lcm(l, p); sf = sf.max(hist[tr].frames[g]); } }
            if l <= MAX_LOOP_TICKS && drv.track_active(refslot as u32) {
                let k0 = hist[refslot].frames.iter().position(|&fr| fr >= sf).unwrap_or(hist[refslot].frames.len());
                target = Some((k0, l));
            } else {
                target = Some((usize::MAX, 0)); // decide approx at the end
            }
        }
        if let Some((k0, l)) = target {
            if k0 != usize::MAX && loop_frames.is_none() && hist[refslot].frames.len() > k0 + l {
                let fr = &hist[refslot].frames;
                loop_frames = Some((fr[k0], fr[k0 + l], LoopKind::Exact));
                extra_until = f + (FRAME_RATE * VERIFY_SECONDS) as usize;
            }
        }
        if loop_frames.is_some() && f >= extra_until { break }

        // ---- one VBlank
        frame_start_chip.push(chip.len());
        drv.tick();
        for ev in drv.drums.drain(..) {
            for (note, vol, ch) in ev.decode() {
                if no_drums { continue }
                if let Some(Some((_, step, env, pan))) = kit.notes.get(note as usize) {
                    // S:0x16260: volume 0 keeps the table ENV, otherwise ENV*vol>>8
                    let env = if vol == 0 { *env as i32 } else { (*env as i32 * vol as i32) >> 8 };
                    // driver writes PAN nibble-swapped (S:0x154A2); RF5C164 low nibble = left (HYPOTHESIS)
                    let p = ((*pan >> 4) | (*pan << 4)) as i32;
                    let (pl, pr) = (p & 15, (p >> 4) & 15);
                    voices[(ch - 2) as usize] = Some(Voice { note: note as usize, pos: 0.0, step: *step, gl: env * pl, gr: env * pr });
                }
            }
        }
        acc += spf;
        let n = acc as usize; acc -= n as f64;
        for _ in 0..n {
            let (mut l, mut r) = drv.ym.clock();
            for v in voices.iter_mut() {
                if let Some(vo) = v {
                    let smp = &kit.notes[vo.note].as_ref().unwrap().0;
                    let i = vo.pos as usize;
                    if i >= smp.len() { *v = None; continue }
                    let s = (smp[i] >> 2) as i32; // ±8128
                    l += s * vo.gl / (255 * 15);
                    r += s * vo.gr / (255 * 15);
                    vo.pos += vo.step;
                }
            }
            chip.push((l, r));
        }
        f += 1;
        let active = if is_music { drv.music_active() } else { drv.sfx_active() };
        if !active { tail += 1; if tail > (FRAME_RATE * if is_music { 2.0 } else { 0.6 }) as usize { break } }
    }
    // ---- approximate loop if no exact one was found but the sound is still running
    let still = if is_music { drv.music_active() } else { drv.sfx_active() };
    if loop_frames.is_none() && still {
        let mut p = 0usize; let mut sf = 0usize; let mut ok = true;
        for &tr in &slots {
            if !drv.track_active(tr as u32) { continue }
            match hist[tr].ex.or(hist[tr].rx) {
                Some((g, pp)) => { p = p.max(pp); sf = sf.max(hist[tr].frames[g]); }
                None => ok = false,
            }
        }
        let fr = &hist[refslot].frames;
        if ok && p > 0 {
            if let Some(k0) = fr.iter().position(|&x| x >= sf) {
                if k0 + p < fr.len() { loop_frames = Some((fr[k0], fr[k0 + p], LoopKind::Approx)); }
            }
        }
    }
    // ---- resample chip-rate → 48 kHz (linear)
    let ratio = CHIP_RATE / OUT_RATE as f64;
    let nout = (chip.len() as f64 / ratio) as usize;
    let mut out = Vec::with_capacity(nout * 2);
    for i in 0..nout {
        let x = i as f64 * ratio;
        let j = x as usize; let t = x - j as f64;
        let a = chip[j]; let b = *chip.get(j + 1).unwrap_or(&a);
        let l = (a.0 as f64 * (1.0 - t) + b.0 as f64 * t) * gain;
        let r = (a.1 as f64 * (1.0 - t) + b.1 as f64 * t) * gain;
        out.push(l.clamp(-32768.0, 32767.0) as i16);
        out.push(r.clamp(-32768.0, 32767.0) as i16);
    }
    let to_out = |fr: usize| ((*frame_start_chip.get(fr).unwrap_or(&chip.len())) as f64 / ratio) as usize;
    let (loop_start, loop_end, loop_kind) = match loop_frames {
        Some((g, e, k)) => (Some(to_out(g)), to_out(e).min(nout), Some(k)),
        None => (None, nout, None),
    };
    let loop_corr = loop_start.and_then(|ls| verify(&out, ls, loop_end));
    let mut out = out;
    if std::env::var("SNT_NO_TRUNC").is_err() { out.truncate(loop_end * 2); }
    Rendered { samples: out, loop_start, loop_end, frames: f, loop_corr, loop_kind }
}

/// Best normalized correlation (lag within ±1.5 frames) between the audio following the loop
/// start and the audio following the loop end.
fn verify(out: &[i16], ls: usize, le: usize) -> Option<f64> {
    let v = (OUT_RATE as f64 * VERIFY_SECONDS * 0.8) as usize;
    let maxlag = (OUT_RATE as f64 / FRAME_RATE * 1.5) as isize;
    if (le + v) * 2 + (maxlag as usize) * 2 > out.len() || (ls as isize) < maxlag { return None }
    let mono = |i: usize| (out[i * 2] as f64 + out[i * 2 + 1] as f64) * 0.5;
    let mut best = -1f64;
    let mut lag = -maxlag;
    while lag <= maxlag {
        let (mut ab, mut aa, mut bb) = (0f64, 0f64, 0f64);
        let mut i = 0;
        while i < v {
            let a = mono((ls as isize + lag) as usize + i); let b = mono(le + i);
            ab += a * b; aa += a * a; bb += b * b; i += 3;
        }
        let c = if aa > 0.0 && bb > 0.0 { ab / (aa * bb).sqrt() } else { 1.0 };
        if c > best { best = c; }
        lag += 4;
    }
    Some(best)
}
