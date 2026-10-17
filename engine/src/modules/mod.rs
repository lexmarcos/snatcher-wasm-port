//! Reimplementations of the sub-CPU code modules that live inside DATA files
//! (docs/re/modules.md): the title module DATA_D2 (with the main-CPU boot screens in
//! front of it), the intro timelines DATA_D1/DATA_D0 and the late-game timeline DATA_Y16.
//!
//! The modules call the same SUBCODE services as the original (`subjt_NN` = `subfn_NN`):
//! scene load (16), scene reset (17), palette channel (08), tile channel (15), scroll/screen
//! effect (13), CD play/status (19/18) and sound (12/06). Those services are implemented by
//! `sncore::gfx` and the helpers below.
pub mod boot;
pub mod timeline;
pub mod title;

use crate::game::{cd_time_bcd, Engine, Module, Out, Top};
use sncore::gfx;

/// `module_factory` for [`Engine`]: which module runs for a top-level mode.
pub fn factory(top: Top, part: u8) -> Option<Box<dyn Module>> {
    match top {
        Top::Title => Some(Box::new(title::Title::new())),
        // table S:0x14AA0 (`00 03 00 02`): DATA_D1 then DATA_D0
        Top::Intro => Some(Box::new(timeline::Timeline::new(if part == 0 { timeline::Kind::D1 } else { timeline::Kind::D0 }))),
        Top::Ending => Some(Box::new(timeline::Timeline::new(timeline::Kind::Y16))),
        _ => None,
    }
}

/// Pseudo register address holding the current CD-DA track (`Regs` is a map, not sub memory).
pub(crate) const CD_TRACK_KEY: u32 = 0x00CD_0000;

/// Track lengths in sectors (1/75 s) of the USA disc (`assets/raw/extract_report.txt`, `snt extract`): the modules poll
/// `CDBSTAT` for "track ended" (DATA_Y16 step 17, S:0x2920A) and the intro tables are consistent with them (D1's last entry
/// 02:12:40 vs track 3 = 02:12:54, D0 track 5 = 00:19:05 vs the track-4 start entry at 00:19:00, D0's last entry 04:30:00 vs
/// track 4 = 04:32:06).
use sncore::audio::CD_TRACK_SECTORS as TRACK_SECTORS;

/// Status word of `cd_status` (S:0xE2FE, CDBSTAT word 0): 0x0100 = playing.
pub const CD_PLAYING: u16 = 0x0100;

/// SUBCODE services used by the modules (sub-CPU semantics, addresses in docs).
pub trait SubServices {
    /// `subfn_16` S:0xF68E: run scene record `n` of the DATA file at 0x28000.
    fn sub_scene(&mut self, n: usize);
    /// `subfn_17` S:0xF5E8: scene reset.
    fn sub_scene_reset(&mut self);
    /// `subfn_08` S:0xE41A: start a palette channel with the descriptor at sub address `a`.
    fn sub_pal(&mut self, a: u32);
    /// `subfn_15` S:0xE81A: start a tile channel with the descriptor at sub address `a`.
    fn sub_tile(&mut self, a: u32);
    /// `subfn_13` S:0xF3F8: scroll mode / screen effect.
    fn sub_scroll(&mut self, n: u8);
    /// `subfn_19` S:0xE30A: CDBIOS MSCPLAY1 (track plays once).
    fn cd_play(&mut self, track: u16);
    /// `subfn_18` S:0xE2FE: CDBSTAT status word.
    fn cd_status(&self) -> u16;
    /// `$787A`: CD relative time (BCD `MM SS FF 00`).
    fn cd_time(&self) -> u32;
    /// Clear `$787A` (the modules do `clr.l $787A` once the track is playing).
    fn cd_time_reset(&mut self);
    /// `subfn_12(n, d1)` (PCM voice/sfx with done flag) and `subfn_06(n, -1)`.
    fn sound12(&mut self, n: u16, d1: u8);
    fn sound06(&mut self, n: u16);
    /// `$7981`: raw newly-pressed pad bits (S A C B R L D U), before the button remap.
    fn raw_pressed(&self) -> u8;
}

impl SubServices for Engine {
    fn sub_scene(&mut self, n: usize) { self.disp.load_scene(&self.mem, n); }

    fn sub_scene_reset(&mut self) {
        // S:0xF5E8: black-out (S:0xF560) unless $788B, palette channels off (subfn_14)
        if self.disp.sub.f788b == 0 { black_out(self); }
        self.disp.sub.palch = Default::default();
        if self.disp.sub.f7866 != 0 {
            self.disp.sub.f7866 = 0;
            // S:0xF600: default text palette into set A colours 0–7
            const TEXT_PAL: [u16; 8] = [0x000, 0xEEE, 0x0EE, 0x86E, 0xEE0, 0xE42, 0x4E0, 0x000];
            self.disp.sub.shadow[..8].copy_from_slice(&TEXT_PAL);
        }
        // S:0xF648: tile channels off (S:0xE806), scroll reset (subfn_13(0xFC)),
        // objects 1..63 without the persistent bit (+$4E bit0) killed (subfn_11)
        self.disp.sub.tilech = Default::default();
        self.disp.sub.scroll_mode(0xFC);
        for o in self.disp.sub.objs.iter_mut().skip(1) {
            if o.f4e & 1 == 0 && o.active != 0 { *o = Default::default(); }
        }
        // $7887 = 1 unless $788B bit1 → main clears planes A/B (M:0xFF32F0: sub_ff16bc 0xE000 and
        // sub_ff16c8 0xC000, 0x1000 words each)
        if self.disp.sub.f788b & 2 == 0 {
            for a in 0xC000..0xD000 { self.disp.vdp.vram[a] = 0; }
            for a in 0xE000..0xF000 { self.disp.vdp.vram[a] = 0; }
            self.disp.sub.f7887 = 1; // S:0xF682: `$7887 = 1` (handshake, blocks the mode logic for 2 frames)
        }
        self.disp.sub.f788b = 0;
    }

    fn sub_pal(&mut self, a: u32) { self.disp.sub.palch_start(&self.mem, a); }
    fn sub_tile(&mut self, a: u32) { self.disp.sub.tilech_start(&self.mem, a); }
    fn sub_scroll(&mut self, n: u8) { self.disp.sub.scroll_mode(n); }

    fn cd_play(&mut self, track: u16) {
        self.cd_start = Some(self.frame);
        self.regs.set(CD_TRACK_KEY, track);
        self.out.push(Out::Music { track });
    }
    // CDBSTAT status: 0x0100 playing, 0 stopped (also once the track has played to its end)
    fn cd_status(&self) -> u16 {
        let Some(s) = self.cd_start else { return 0 };
        let tr = self.regs.get(CD_TRACK_KEY);
        match TRACK_SECTORS.iter().find(|t| t.0 == tr) {
            Some(&(_, sectors)) if (self.frame - s) * 75 / 60 >= sectors as u64 => 0,
            _ => CD_PLAYING,
        }
    }
    fn cd_time(&self) -> u32 { self.cd_start.map(|s| cd_time_bcd(self.frame - s)).unwrap_or(0) }
    // `clr.l $787A`: momentary – the next sub frame reloads the track-relative time (S:0xD640)
    fn cd_time_reset(&mut self) { self.disp.sub.cd_time = 0; }

    // subfn_12(n, 0) = FM sfx with done flag `$980F`, subfn_12(n, 1) = FM music with `$980E` (S:0x1011E/0x1013E)
    fn sound12(&mut self, n: u16, d1: u8) { self.fm_cmd(n, d1 == 1); }
    // subfn_06(n, -1): PCM effect (muted by `$7890`, ignored while a voice plays; S:0x1530E)
    fn sound06(&mut self, n: u16) {
        if self.regs.get_b(0x7890) == 0 && self.voice_left == 0 { self.out.push(Out::PcmSfx { id: n, bank: self.pcm_bank }); }
    }

    fn raw_pressed(&self) -> u8 {
        // invert the S:0xDFF6 remap: logical = raw&0x8F | A→c[0] | B→c[1] | C→c[2]
        let Some(i) = self.input.as_ref() else { return 0 };
        let c = i.cfg_table[i.config.min(5)];
        let l = i.pressed;
        let mut r = l & 0x8F;
        if l & (1 << c[0]) != 0 { r |= 0x40; }
        if l & (1 << c[1]) != 0 { r |= 0x10; }
        if l & (1 << c[2]) != 0 { r |= 0x20; }
        r
    }
}

impl Engine {
    /// Debug / test entry: run the late-game module DATA_Y16 (file 0x34) as the mode machine does for `SETSTATE(1)`.
    pub fn start_ending(&mut self) {
        self.top = Top::Ending;
        self.enter_module(Top::Ending, 0x34);
    }

    /// The sub frame skips the mode machine and object updates while a VRAM list (`$7886`) or the
    /// scene-reset handshake (`$7887`) is pending (S:0xD6EE–0xD71E).
    pub fn mode_blocked(&self) -> bool { self.disp.sub.vram_list.is_some() || self.disp.sub.f7887 != 0 }
}

/// S:0xF560 palette black-out (same as `Engine::black_out`, which is private to game.rs).
fn black_out(e: &mut Engine) {
    let sub = &mut e.disp.sub;
    if sub.f788a & 1 != 0 { return; }
    sub.f788a |= 1;
    let all = sub.f788a & 2 != 0 || sub.f7866 != 0;
    sub.f788a &= !2;
    let from = if all { 0 } else { 16 };
    for c in sub.shadow[from..64].iter_mut() { *c = 0; }
    if sub.f788a & 4 == 0 { for c in sub.shadow[64 + 16..128].iter_mut() { *c = 0; } }
    sub.f788a &= !4;
}

/// Big-endian reads from a DATA file mapped at 0x28000.
pub fn data_u16(e: &Engine, a: u32) -> u16 { e.mem.u16(a) }
pub fn data_u32(e: &Engine, a: u32) -> u32 { e.mem.u32(a) }

/// Re-exported for the modules.
pub use gfx::DATA_BASE;
