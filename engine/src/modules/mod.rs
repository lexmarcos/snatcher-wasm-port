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
// pub mod title;  // TODO: DATA_D2 title module not written yet (stopped mid-task)

use crate::game::{cd_time_bcd, Engine, Module, Out, Top};
use sncore::gfx;

/// `module_factory` for [`Engine`]: which module runs for a top-level mode.
pub fn factory(top: Top, part: u8) -> Option<Box<dyn Module>> {
    match top {
        Top::Title => None, // TODO: title module (DATA_D2); None = skip straight to the intro
        // table S:0x14AA0 (`00 03 00 02`): DATA_D1 then DATA_D0
        Top::Intro => Some(Box::new(timeline::Timeline::new(if part == 0 { timeline::Kind::D1 } else { timeline::Kind::D0 }))),
        Top::Ending => Some(Box::new(timeline::Timeline::new(timeline::Kind::Y16))),
        _ => None,
    }
}

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
        }
        self.disp.sub.f788b = 0;
    }

    fn sub_pal(&mut self, a: u32) { self.disp.sub.palch_start(&self.mem, a); }
    fn sub_tile(&mut self, a: u32) { self.disp.sub.tilech_start(&self.mem, a); }
    fn sub_scroll(&mut self, n: u8) { self.disp.sub.scroll_mode(n); }

    fn cd_play(&mut self, track: u16) {
        self.cd_start = Some(self.frame);
        self.out.push(Out::Music { track });
    }
    fn cd_status(&self) -> u16 { if self.cd_start.is_some() { CD_PLAYING } else { 0 } }
    fn cd_time(&self) -> u32 { self.cd_start.map(|s| cd_time_bcd(self.frame - s)).unwrap_or(0) }
    // `clr.l $787A`: momentary – the next sub frame reloads the track-relative time (S:0xD640)
    fn cd_time_reset(&mut self) { self.disp.sub.cd_time = 0; }

    fn sound12(&mut self, n: u16, d1: u8) { self.out.push(Out::Pcm { id: n, kind: d1 }); }
    fn sound06(&mut self, n: u16) { self.out.push(Out::Pcm { id: n, kind: 2 }); }

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
