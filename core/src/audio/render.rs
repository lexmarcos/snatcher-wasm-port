//! Offline renderer: Snatcher FM driver (kdriver) + YM2612 + PCM drum kit → 48 kHz stereo.
//! Used by `snt audio-fm` to pre-render music (with loop points) and FM sound effects.

use super::kdriver::KDriver;
use super::ym2612::CHIP_RATE;
use super::{PcmSfxEntry, parse_segments, DRUM_BASE};
use std::collections::HashMap;

pub const OUT_RATE: u32 = 48_000;
/// NTSC VBlank rate (the driver ticks once per VBlank).
pub const FRAME_RATE: f64 = 59.922_743;

/// PCM drum kit as used by music slots 6/7 (sfx ids 1..12, S:0x160BA / S:0x161F4).
pub struct DrumKit { pub notes: Vec<Option<(Vec<i16>, f64, u8, u8)>> } // (samples, step/output-sample at chip rate, env, pan)

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

pub struct Rendered { pub samples: Vec<i16>, pub loop_start: Option<usize>, pub loop_end: usize, pub frames: usize }

/// Render sound `id`. Music stops at the first exact repeat of the sequencer state (loop) or when
/// all tracks end; `max_frames` bounds the run.
pub fn render(drv: &mut KDriver, kit: &DrumKit, id: u8, max_frames: usize, gain: f64) -> Rendered {
    let no_drums = std::env::var("SNT_NO_DRUMS").is_ok();
    if let Ok(m) = std::env::var("SNT_FM_MUTE") { drv.ym.mute = u8::from_str_radix(&m, 16).unwrap_or(0); }
    drv.request(id);
    let is_music = drv.sound_type(id) & 0x0F != 0;
    let spf = CHIP_RATE / FRAME_RATE;
    let mut acc = 0.0;
    let mut chip: Vec<(i32, i32)> = Vec::new();
    let mut frame_start_chip = Vec::new();
    let mut seen: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut voices: [Option<Voice>; 2] = [None, None];
    let mut loop_frames: Option<(usize, usize)> = None;
    let mut tail = 0usize;
    let mut f = 0usize;
    while f < max_frames {
        if is_music {
            let key = drv.state_key();
            if let Some(&g) = seen.get(&key) { loop_frames = Some((g, f)); break; }
            seen.insert(key, f);
        }
        frame_start_chip.push(chip.len());
        drv.tick();
        for ev in drv.drums.drain(..) {
            for (note, vol, ch) in ev.decode() {
                if no_drums { continue }
                if let Some(Some((_, step, env, pan))) = kit.notes.get(note as usize) {
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
    // resample chip-rate → 48 kHz (linear)
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
    let to_out = |chip_idx: usize| (chip_idx as f64 / ratio) as usize;
    let (loop_start, loop_end) = match loop_frames {
        Some((g, _)) => (Some(to_out(frame_start_chip[g])), nout),
        None => (None, nout),
    };
    Rendered { samples: out, loop_start, loop_end, frames: f }
}
