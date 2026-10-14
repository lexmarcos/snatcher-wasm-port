//! Re-implementation of Snatcher's FM sound driver (Konami 68000 driver living in the main-CPU
//! IP at M:0xFF4000–0xFF6141). Each function is a hand translation of the 68000 routine named in
//! its doc comment (listing: re/out/dis/audio_main.asm). The work RAM layout is kept byte-exact
//! (tracks at M:0xFF7A00 + slot*0x80, globals at M:0xFF7F00..) so that offsets in comments match.
//!
//! Data is NOT embedded: tables are read from the IP image (header[0x200..0x800] ++ ip.bin) and
//! songs/instruments from FMWR_1.BIN (= Word RAM image at M:0x200000) at runtime.
//!
//! Output: YM2612 register writes go to `Ym2612`; PCM-drum triggers for slots 6/7 (normally sent
//! to the sub CPU through COMCMD5..7) are returned as `DrumEvent`s.

use super::ym2612::Ym2612;

const RAM_BASE: u32 = 0xFF7A00;
const RAM_END: u32 = 0xFFA000;
const IP_BASE: u32 = 0xFF0000;

// globals (M: addresses)
const SFXF: u32 = 0xFF7F24; // bit0/1: sfx owns ch 4/5, bit3 muted, bit5 restoring, bit6 in sfx pass, bit7 sfx active
const FLAGS: u32 = 0xFF7F2A; // bit2 freq dirty, bit3 pan mute, bit4 track yield, bit5 sfx start, bit6 first frame
const CHN: u32 = 0xFF7F2D;
const STAGGER: u32 = 0xFF7F30;
const LFO_REG: u32 = 0xFF7F31;
const BASE: u32 = 0xFF7F34;
const DRUM0: u32 = 0xFF7F48;
const DRUM1: u32 = 0xFF7F49;
const DRUM2: u32 = 0xFF7F4A;
const ACTIVE: u32 = 0xFF7F4B;
const STATUS: u32 = 0xFF7F5D;
const VOLW: u32 = 0xFF7F5E;
const SFX_PRIO: u32 = 0xFF7F61;
const SFX_ID0: u32 = 0xFF7F62;
const SFX_ID1: u32 = 0xFF7F63;

/// Drum trigger for the sub-CPU PCM drum kit (COMCMD5..6 bytes, M:0xFF4DDE).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrumEvent { pub b0: u8, pub b1: u8, pub b2: u8 }

impl DrumEvent {
    /// Decode as the sub CPU does (S:0x160BA): up to two (note, volume<<3, logical channel).
    pub fn decode(&self) -> Vec<(u8, u8, u8)> {
        let mut v = Vec::new();
        let n0 = self.b0 & 0x1F;
        if n0 != 0 && n0 < 0x0D {
            let vol = ((self.b1 & 7) | ((self.b2 & 0x60) >> 2)) << 3;
            v.push((n0, vol, 2));
        }
        let n1 = (self.b1 & 0xF8) >> 3;
        if n1 != 0 && n1 < 0x0D {
            v.push((n1, (self.b2 & 0x1F) << 3, 3));
        }
        v
    }
}

pub struct KDriver {
    ram: Vec<u8>,
    ip: Vec<u8>,
    z80: Vec<u8>,
    wram: Vec<u8>,
    pub ym: Ym2612,
    /// port of a3: 0 = 0xA04000, 1 = 0xA04002
    a3: usize,
    pub drums: Vec<DrumEvent>,
    pub e030: u16, // M:$FFE030 bit0 = PAL tempo adjust (0 on NTSC)
    /// diagnostics: how often the vibrato-ramp bug path (see ff4468) was taken
    pub f9_ramp_hits: u32,
    /// number of sequencer ticks of music slot 0 (used for tick-domain loop detection)
    pub tick0: u64,
    /// sequencer ticks per slot (0..9)
    pub ticks: [u64; 10],
    /// diagnostics: executed sequence commands E0..FF
    pub cmd_stats: [u32; 32],
}


impl KDriver {
    /// `ip` = main-CPU IP image starting at M:0xFF0000; `fmwr` = FMWR_1.BIN.
    pub fn new(ip: Vec<u8>, fmwr: Vec<u8>) -> Self {
        let mut d = KDriver { ram: vec![0; (RAM_END - RAM_BASE) as usize], ip, z80: vec![0; 0x2000],
                              wram: fmwr, ym: Ym2612::new(), a3: 0, drums: Vec::new(), e030: 0, f9_ramp_hits: 0, tick0: 0, ticks: [0; 10], cmd_stats: [0; 32] };
        d.boot_load();
        d.wb(VOLW, 0xFF); d.wb(VOLW + 1, 0xFF);
        d
    }

    // ---------------------------------------------------------------- memory model
    fn rb(&self, a: u32) -> u8 {
        let a = a & 0xFFFFFF;
        if (RAM_BASE..RAM_END).contains(&a) { self.ram[(a - RAM_BASE) as usize] }
        else if a >= IP_BASE && ((a - IP_BASE) as usize) < self.ip.len() { self.ip[(a - IP_BASE) as usize] }
        else if (0xA00000..0xA02000).contains(&a) { self.z80[(a - 0xA00000) as usize] }
        else if (0x200000..0x240000).contains(&a) { *self.wram.get((a - 0x200000) as usize).unwrap_or(&0) }
        else { 0 }
    }
    fn wb(&mut self, a: u32, v: u8) {
        let a = a & 0xFFFFFF;
        if (RAM_BASE..RAM_END).contains(&a) { self.ram[(a - RAM_BASE) as usize] = v; }
    }
    fn rw(&self, a: u32) -> u16 { ((self.rb(a) as u16) << 8) | self.rb(a + 1) as u16 }
    fn ww(&mut self, a: u32, v: u16) { self.wb(a, (v >> 8) as u8); self.wb(a + 1, v as u8); }
    fn rl(&self, a: u32) -> u32 { ((self.rw(a) as u32) << 16) | self.rw(a + 2) as u32 }
    fn wl(&mut self, a: u32, v: u32) { self.ww(a, (v >> 16) as u16); self.ww(a + 2, v as u16); }
    fn bset(&mut self, a: u32, bit: u8) { let v = self.rb(a) | (1 << bit); self.wb(a, v); }
    fn bclr(&mut self, a: u32, bit: u8) { let v = self.rb(a) & !(1 << bit); self.wb(a, v); }
    fn btst(&self, a: u32, bit: u8) -> bool { self.rb(a) & (1 << bit) != 0 }

    // ---------------------------------------------------------------- YM access
    /// write via a3 (channel port)
    fn ym3(&mut self, reg: u8, v: u8) { self.ym.write(self.a3, reg, v); }
    /// write via a2 (port 0)
    fn ym2(&mut self, reg: u8, v: u8) { self.ym.write(0, reg, v); }

    /// The recurring guard (e.g. M:0xFF5E5E..0xFF5EB2): true if the music track in slot d3
    /// (4 or 5) must not touch the chip because a sound effect owns that channel.
    fn blocked(&self, d3: u8) -> bool {
        if !self.btst(SFXF, 7) { return false }
        match d3 {
            4 => self.btst(SFXF, 0) && !self.btst(SFXF, 6),
            5 => self.btst(SFXF, 1) && !self.btst(SFXF, 6),
            _ => false,
        }
    }

    // ---------------------------------------------------------------- boot / start
    /// M:0xFF56AE: instruments (Word RAM 0x200000, 0x1E00 bytes) → Z80 RAM, sfx bank
    /// (Word RAM 0x201E00, 0xA46 bytes) → M:0xFF9000.
    fn boot_load(&mut self) {
        for i in 0..0x1E00 { self.z80[i] = *self.wram.get(i).unwrap_or(&0); }
        for i in 0..0xA46u32 { let v = self.rb(0x201E00 + i); self.wb(0xFF9000 + i, v); }
    }

    /// M:0xFF55FE (whole copy at once; the original copies 0x54 longs per frame).
    fn song_copy(&mut self, song: u32, id: u8) {
        let tab = 0x202846u32;
        let src = self.rl(tab + song * 4).wrapping_add(tab);
        let size = self.rw(0xFF5586 + ((id as u32 - 0x3F) * 2)) as u32;
        let size = (size + 3) & !3;
        for i in 0..size.min(0x1000) { let v = self.rb(src + i); self.wb(0xFF8000 + i, v); }
    }

    /// Queue a request like `fm_request` M:0xFF5058 and process it like `fm_update` M:0xFF5106.
    pub fn request(&mut self, id: u8) {
        if id == 0 { return }
        if id >= 0xF1 { self.command(id); return }
        if id >= 0x7B { return }
        self.start(id);
    }

    fn command(&mut self, id: u8) {
        match id {
            0xF1 => self.stop_all(),
            0xF2 => self.cmd_f2(),
            0xF3 => self.bset(FLAGS, 3),
            0xF4 => self.bclr(FLAGS, 3),
            _ => {}
        }
    }

    /// M:0xFF52AC
    fn stop_all(&mut self) {
        for i in 0..=0x573u32 { self.wb(0xFF7A00 + i, 0); }
        // PSG silence (M:0xFF573C) – PSG not modelled
        // M:0xFF575E: TL=0x7F (actually 0xFF) on all operators of the 6 channels
        for d3 in (0..6u8).rev() {
            let port = if d3 >= 3 { 1 } else { 0 };
            let ch = [0u8, 1, 2, 0, 1, 2][d3 as usize];
            self.a3 = port;
            self.ff582a(ch);
            self.key_off(d3); // M:0xFF582A falls through into M:0xFF5890
        }
        self.ym2(0x2B, 0); // M:0xFF5704
    }

    /// M:0xFF582A: write 0xFF to regs 0x80+ch,0x84,0x88,0x8C (SL/RR = max release)
    fn ff582a(&mut self, ch: u8) {
        let mut r = ch.wrapping_add(0x80);
        for _ in 0..4 { self.ym3(r, 0xFF); r = r.wrapping_add(4); }
    }

    /// M:0xFF51E4 (command F2) – stop music tracks (keep sfx)
    fn cmd_f2(&mut self) {
        self.ff578a();
        for i in 0..10u32 { self.wb(0xFF7A00 + i * 0x80, 0); self.wb(0xFF7A01 + i * 0x80, 0); }
        for i in 0..6u32 { self.wb(0xFF7A32 + i * 0x80, 1); }
        self.wb(0xFF7F28, 0);
        self.wb(STAGGER, 0);
    }

    /// M:0xFF578A: silence (volume 0) all active music FM channels
    fn ff578a(&mut self) {
        let mut a1 = 0xFF7C80u32;
        for d3 in (0..6u8).rev() {
            self.a3 = if d3 >= 3 { 1 } else { 0 };
            if self.rb(a1) != 0 {
                let ch = [0u8, 1, 2, 0, 1, 2][d3 as usize];
                self.wb(CHN, ch);
                self.ff5e5e(a1, d3, 0);
            }
            a1 = a1.wrapping_sub(0x80);
        }
    }

    /// M:0xFF523A (called when the music volume word is too low → mute)
    fn ff523a(&mut self) {
        self.ff578a();
        for i in 0..6u32 { self.wb(0xFF7A00 + i * 0x80, 0); self.wb(0xFF7A01 + i * 0x80, 0); }
        for i in 0..6u32 { self.wb(0xFF7A32 + i * 0x80, 1); }
        self.wb(0xFF7F28, 0);
        self.wb(STAGGER, 0);
        for a in [0xFF7D00u32, 0xFF7D80] { self.wb(a, 0); self.wb(a + 1, 0); }
    }

    /// `fm_start` M:0xFF5306
    pub fn start(&mut self, d0: u8) {
        self.wb(ACTIVE, 0);
        self.bclr(SFXF, 3);
        let mut a0 = 0xFF6142 + (d0 as u32) * 8;
        if self.rb(a0) & 0x0F != 0 {
            self.bclr(FLAGS, 5);
            self.wb(STAGGER, 2);
        } else {
            self.bset(FLAGS, 5);
            self.wb(STAGGER, 0);
        }
        if !self.btst(FLAGS, 5) { self.cmd_f2(); }
        self.bclr(FLAGS, 6);
        a0 += 1; // byte0 (d4, unused)
        let mut d1 = self.rb(a0) as u32; a0 += 1;
        if !self.btst(FLAGS, 5) && d1 >= 6 { self.bset(FLAGS, 6); } else { self.bclr(FLAGS, 6); }
        let mut d3 = self.rb(a0); a0 += 1;
        let mut a1 = 0xFF7A00 + (d3 as u32) * 0x80;
        let mut d6: u8 = 0;
        if d0 < 0x32 {
            d6 = self.rb(a0); a0 += 1;
            if d0 == self.rb(SFX_ID1) {
                // reuse slot
            } else if d0 == self.rb(SFX_ID0) {
                a1 -= 0x80; d3 = d3.wrapping_sub(1);
            } else if d6 > self.rb(a1 + 1) {
                self.wb(SFX_ID1, d0);
            } else {
                a1 -= 0x80;
                if d6 < self.rb(a1 + 1) { return }
                self.wb(SFX_ID0, d0);
                d3 = d3.wrapping_sub(1);
            }
        } else {
            a0 += 1;
        }
        let a4;
        if !self.btst(FLAGS, 5) {
            self.wb(SFXF, 0);
            let song = self.rl(a0);
            self.song_copy(song, d0);
            a4 = 0xFF8000u32;
        } else {
            let v = self.rb(SFXF) & 3; self.wb(SFXF, v);
            self.wb(SFX_PRIO, d6);
            let idx = self.rl(a0);
            a4 = self.rl(0xFF9000 + idx * 4).wrapping_add(0xFF9000);
        }
        self.wl(BASE, a4);
        let mut a4 = a4;
        loop {
            if self.btst(FLAGS, 5) && d3 >= 8 {
                if d3 == 8 { self.bset(SFXF, 0); } else { self.bset(SFXF, 1); }
            }
            let p = self.rl(a4).wrapping_add(self.rl(BASE)); a4 += 4;
            for off in [0x6C, 0x64, 0x68, 0x5C] { self.wl(a1 + off, p); }
            self.wb(a1 + 4, 0xFF);
            self.wb(a1 + 0x1E, 1);
            self.wb(a1 + 5, 1);
            self.wb(a1 + 0x3E, 0x7F);
            self.wb(a1 + 0x12, 0x7F);
            self.wb(a1 + 0x23, 0xFF);
            self.wb(a1 + 0x22, 0);
            if self.btst(FLAGS, 5) {
                self.bset(a1 + 0x22, 0);
                let v = self.rb(SFX_PRIO); self.wb(a1 + 1, v);
            } else {
                let v = self.rb(ACTIVE) | (1 << (d3 & 7)); self.wb(ACTIVE, v);
            }
            for off in [0x21u32, 0x20, 0x13, 0x15] { self.wb(a1 + off, 0); }
            for off in [0x30u32, 0x2C, 0x10, 0x06, 0x02] { self.ww(a1 + off, 0); }
            for off in [0x1Au32, 0x34, 0x38, 0x52, 0x70] { self.wl(a1 + off, 0); }
            self.ww(a1 + 0x4A, 0);
            self.wb(a1, d0);
            if d3 < 6 || d3 >= 8 { self.wb(a1 + 0x32, 1); }
            a1 += 0x80;
            d3 = d3.wrapping_add(1);
            if d1 == 0 { break }
            d1 -= 1;
        }
    }

    // ---------------------------------------------------------------- per-frame
    /// `fm_tick` M:0xFF408A (called once per VBlank).
    pub fn tick(&mut self) {
        let vw = self.rw(VOLW);
        let hi = vw & 0xFF00;
        let lo = vw & 0x00FF;
        let mute = hi == 0 || hi < 0x1500 || lo == 0 || lo < 0x15;
        if mute {
            if !self.btst(SFXF, 3) { self.bset(SFXF, 3); self.ff523a(); }
            self.ff4e08();
            if self.rb(STAGGER) != 0 { let v = self.rb(STAGGER) - 1; self.wb(STAGGER, v); }
            return;
        }
        self.bclr(SFXF, 3);
        if !self.btst(FLAGS, 6) {
            let mut a1 = 0xFF7A00u32;
            self.a3 = 0;
            for d3 in 0..6u8 {
                let ch = self.rb(0xFF4000 + d3 as u32);
                self.wb(CHN, ch);
                if d3 == 3 { self.a3 = 1; }
                if self.rb(a1 + 0x32) != 0 {
                    self.ff5a16(a1, d3);
                    self.ff57cc(d3);
                    self.wb(a1 + 0x32, 0);
                }
                if self.rb(a1) != 0 {
                    let st = self.rb(STAGGER);
                    let skip = st != 0 && ((st & 1) ^ (d3 & 1)) == 0;
                    if !skip { self.track_step(a1, d3); }
                }
                a1 += 0x80;
            }
        }
        self.ff4d22();
        if self.btst(FLAGS, 6) {
            self.bclr(FLAGS, 6);
            return;
        }
        self.ff4e08();
        if self.rb(STAGGER) != 0 { let v = self.rb(STAGGER) - 1; self.wb(STAGGER, v); }
    }

    /// tempo accumulate + step (M:0xFF417E..0xFF41AC)
    fn track_step(&mut self, a1: u32, d3: u8) {
        let t = self.rb(a1 + 4);
        let (s, c) = self.rb(a1 + 5).overflowing_add(t);
        self.wb(a1 + 5, s);
        let mut a0 = self.rl(a1 + 0x6C);
        if !c {
            self.wb(a1 + 0x24, 0);
            self.ff425a(a1, d3, &mut a0);
        } else {
            self.wb(a1 + 0x24, 1);
            if a1 == 0xFF7A00 { self.tick0 += 1; }
            self.ticks[((a1 - 0xFF7A00) / 0x80) as usize] += 1;
            self.ff4202(a1, d3, &mut a0);
        }
        self.wl(a1 + 0x6C, a0);
    }

    /// M:0xFF4202: one sequencer tick
    fn ff4202(&mut self, a1: u32, d3: u8, a0: &mut u32) {
        let v = self.rb(a1 + 0x1E).wrapping_sub(1);
        self.wb(a1 + 0x1E, v);
        if v == 0 { self.read_events(a1, d3, a0); return; }
        if self.rb(a1 + 0x16) != 0 { self.ff4372(a1, d3); }
        if self.rb(a1 + 0x2C) != 0 { self.ff43ac(a1, d3); }
        if self.rb(a1 + 0x2D) != 0 { self.ff43c6(a1, d3); }
        let d6 = self.rb(a1 + 0x1C);
        if d6 != 0 && d6 != 0x7F { self.ff43e0(a1, d3); }
        if self.rb(a1 + 0x1E) == 1 { self.ff4412(a1, d3, a0); }
        self.ff425a(a1, d3, a0);
    }

    /// M:0xFF425A: per-frame modulation updates
    fn ff425a(&mut self, a1: u32, d3: u8, _a0: &mut u32) {
        self.bclr(FLAGS, 2);
        if self.rb(a1 + 6) != 0 { self.ff442a(a1); }
        let vib = self.ff4468(a1);
        self.ff44d2(a1, d3, vib);
        if self.btst(a1 + 0x22, 5) { self.bclr(a1 + 0x22, 5); return; }
        if self.rb(a1 + 0x13) != 0 { self.ff44f4(a1); }
        if self.rw(VOLW) != 0xFFFF { self.mark_all_vol(); }
        if self.rb(a1 + 0x31) != 0 { self.ff451a(a1, d3); }
    }

    fn mark_all_vol(&mut self) {
        for off in [0x031u32, 0x0B1, 0x131, 0x1B1, 0x231, 0x2B1, 0x431, 0x4B1] { self.wb(0xFF7A00 + off, 1); }
    }

    /// M:0xFF42E8: read events until a note (or yield)
    fn read_events(&mut self, a1: u32, d3: u8, a0: &mut u32) {
        loop {
            let d6 = self.rb(*a0); *a0 += 1;
            if d6 == 0 { continue }
            if d6 >= 0xE0 {
                self.bclr(FLAGS, 4);
                self.seq_cmd(a1, d3, a0, d6);
                if self.btst(FLAGS, 4) { return }
                continue;
            }
            let d5 = d6 & 0x7F;
            if d5 < 0x60 { self.ff4662(a1, d3, a0, d6); return; }
            // 0x60..0x7F / 0xE0+: ignored here (music FM tracks)
        }
    }

    /// M:0xFF4372: auto-pan sequence (table M:0xFF4332)
    fn ff4372(&mut self, a1: u32, d3: u8) {
        let v = self.rb(a1 + 0x15).wrapping_sub(1);
        self.wb(a1 + 0x15, v);
        if v != 0 { return }
        let r = self.rb(a1 + 0x16); self.wb(a1 + 0x15, r);
        let mut d5 = self.rb(a1 + 0x19);
        let d6 = (self.rb(a1 + 0x18) << 4).wrapping_add(d5);
        let d0 = self.rb(0xFF4332 + d6 as u32);
        self.wb(a1 + 0x17, d0);
        d5 += 1; if d5 > 0x0B { d5 = 0; }
        self.wb(a1 + 0x19, d5);
        self.ff5a3a(a1, d3, d0);
    }

    /// M:0xFF43AC: delayed AMS
    fn ff43ac(&mut self, a1: u32, d3: u8) {
        let v = self.rb(a1 + 0x2E);
        if v == 0 { return }
        self.wb(a1 + 0x2E, v - 1);
        if v - 1 != 0 { return }
        let d0 = self.rb(a1 + 0x2A);
        self.ff5a22(a1, d3, d0);
    }

    /// M:0xFF43C6: delayed PMS
    fn ff43c6(&mut self, a1: u32, d3: u8) {
        let v = self.rb(a1 + 0x2F);
        if v == 0 { return }
        self.wb(a1 + 0x2F, v - 1);
        if v - 1 != 0 { return }
        let d0 = self.rb(a1 + 0x2B);
        self.ff5a2e(a1, d3, d0);
    }

    /// M:0xFF43E0: gate time (key-off before note end) and decay-volume
    fn ff43e0(&mut self, a1: u32, d3: u8) {
        let g = self.rb(a1 + 0x1A);
        if g != 0 {
            self.wb(a1 + 0x1A, g - 1);
            if g - 1 != 0 { return }
            self.key_off(d3);
        }
        let d6 = self.rw(a1 + 0x54);
        if d6 == 0 { return }
        self.wb(a1 + 0x31, 1);
        let (s, c) = self.rw(a1 + 0x56).overflowing_add(d6);
        self.ww(a1 + 0x56, if c { 0xFFFF } else { s });
    }

    /// M:0xFF4412: lookahead instrument change (only odd slots)
    fn ff4412(&mut self, a1: u32, d3: u8, a0: &mut u32) {
        if d3 & 1 == 0 { return }
        if self.rb(*a0) != 0xE2 { return }
        *a0 += 1;
        self.ff4938(a1, d3, a0);
    }

    /// M:0xFF442A: portamento
    fn ff442a(&mut self, a1: u32) {
        if self.rb(a1 + 8) != 0 {
            if self.rb(a1 + 0x24) != 0 { let v = self.rb(a1 + 8) - 1; self.wb(a1 + 8, v); }
            return;
        }
        let v = self.rb(a1 + 6).wrapping_sub(1); self.wb(a1 + 6, v);
        if v == 0 {
            let t = self.rw(a1 + 0x46); self.ww(a1 + 0x42, t);
        } else {
            let s = self.rw(a1 + 0x42).wrapping_add(self.rw(a1 + 0x44)); self.ww(a1 + 0x42, s);
        }
        self.bset(FLAGS, 2);
    }

    /// M:0xFF4468: vibrato (sine table M:0xFF3DC0)
    fn ff4468(&mut self, a1: u32) -> i16 {
        if self.rb(a1 + 0x11) == 0 { return 0 }
        if self.rb(a1 + 0x0A) != 0 {
            if self.rb(a1 + 0x24) != 0 { let v = self.rb(a1 + 0x0A) - 1; self.wb(a1 + 0x0A, v); }
            return 0;
        }
        if self.rb(a1 + 0x0D) != 0 && self.rb(a1 + 0x24) != 0 {
            let v = self.rb(a1 + 0x0D) - 1; self.wb(a1 + 0x0D, v);
            if v == 0 {
                let x = self.rb(a1 + 0x11); self.wb(a1 + 0x4A, x); self.wb(a1 + 0x4B, 0);
            } else {
                // BUG in original: `move.w $4C(a1),d0 ; add.w d6,$4A(a1)` adds the stale d6
                // (M:0xFF44B0). We add the evident intended value $4C. HYPOTHESIS-level fidelity.
                let v = self.rw(a1 + 0x4A).wrapping_add(self.rw(a1 + 0x4C));
                self.ww(a1 + 0x4A, v);
                self.f9_ramp_hits += 1;
            }
        }
        let d0 = self.rb(a1 + 0x0C).wrapping_add(self.rb(a1 + 0x0B));
        self.wb(a1 + 0x0B, d0);
        let v = self.ff4cb8(a1, d0);
        self.bset(FLAGS, 2);
        v
    }

    /// M:0xFF4CB8: vibrato offset for phase d0
    fn ff4cb8(&self, a1: u32, d0: u8) -> i16 {
        let d6 = d0;
        let idx = ((d0 >> 1) & 0x3F) as u32;
        let s = self.rb(0xFF3DC0 + idx) as u32;
        let mut v = (s * self.rb(a1 + 0x4A) as u32) as i32;
        if self.rb(a1 + 0x11) & 0x80 == 0 { v >>= 2; }
        if d6 & 0x80 != 0 { v = -v; }
        v as i16
    }

    /// M:0xFF44D2: compose final pitch and write if changed
    fn ff44d2(&mut self, a1: u32, d3: u8, vib: i16) {
        let d1 = self.ff4cea(a1);
        if d1 != 0 || self.btst(FLAGS, 2) {
            let d0 = (vib as u16).wrapping_add(d1).wrapping_add(self.rw(a1 + 0x42));
            self.ff5f0a(a1, d3, d0);
        }
    }

    /// M:0xFF4CEA: random pitch noise (table M:0xFF3E00)
    fn ff4cea(&mut self, a1: u32) -> u16 {
        let d6 = self.rb(a1 + 0x20);
        if d6 == 0 { return 0 }
        let (s, c) = self.rb(a1 + 0x1F).overflowing_add(d6);
        self.wb(a1 + 0x1F, s);
        if !c { return 0 }
        let i = self.rw(a1 + 0x4E) as u32;
        let v = ((self.rb(0xFF3E00 + i) as u16) << 8) | self.rb(0xFF3E01 + i) as u16;
        let d1 = v & self.rw(a1 + 0x50);
        self.ww(a1 + 0x4E, ((i + 1) & 0x1FF) as u16);
        d1
    }

    /// M:0xFF44F4: volume slide
    fn ff44f4(&mut self, a1: u32) {
        let v = self.rb(a1 + 0x13).wrapping_sub(1); self.wb(a1 + 0x13, v);
        if v == 0 {
            let t = self.rb(a1 + 0x14); self.wb(a1 + 0x3E, t); self.wb(a1 + 0x3F, 0);
        } else {
            let s = self.rw(a1 + 0x3E).wrapping_add(self.rw(a1 + 0x40)); self.ww(a1 + 0x3E, s);
        }
        self.wb(a1 + 0x31, 1);
    }

    /// M:0xFF451A: compute output volume and write TLs
    fn ff451a(&mut self, a1: u32, d3: u8) {
        let mut d0: i32 = self.rb(a1 + 0x12) as i32;
        let d2 = self.rb(a1 + 0x56) as i32;
        let d5 = self.rb(a1 + 0x3E) as i32;
        let d4 = (0x7F - d5) & 0xFF;
        d0 = d0 - d2 - d4;
        d0 = (d0 + self.rw(a1 + 0x34) as i16 as i32 + self.rw(a1 + 0x36) as i16 as i32) & 0xFFFF;
        let mut v: u32 = if d0 >= 0x8000 { 0 } else if d0 >= 0x80 { 0x7F } else { (((d0 as u8 as i8) >> 1) as u8).wrapping_add(0x40) as u32 };
        self.wb(a1 + 0x70, v as u8);
        let vw = self.rw(VOLW);
        if vw != 0xFFFF {
            let lo = (vw & 0xFF) as u32;
            if lo != 0xFF { v = (v * (lo >> 1)) >> 7; }
            let hi = vw & 0xFF00;
            if hi != 0xFF00 {
                let skip = self.btst(SFXF, 6) && ((d3 == 4 && self.btst(SFXF, 0)) || (d3 == 5 && self.btst(SFXF, 1)));
                if !skip { v = (v * (((hi >> 8) as u32) >> 1)) >> 7; }
            }
        }
        self.ff5e5e(a1, d3, v as u8);
        self.wb(a1 + 0x31, 0);
    }

    /// M:0xFF4662: note event (d6 = event byte < 0xE0 with low 7 bits < 0x60)
    fn ff4662(&mut self, a1: u32, d3: u8, a0: &mut u32, d6_in: u8) {
        if self.rb(a1 + 0x33) != 0 { self.wb(a1 + 0x33, 0); self.key_off(d3); }
        self.wb(a1 + 0x31, 1);
        let d0 = d6_in;
        let mut d6 = self.rb(a1 + 0x1B);
        if d0 & 0x80 == 0 { d6 = self.rb(*a0); *a0 += 1; }
        self.wb(a1 + 0x1B, d6);
        self.wb(a1 + 0x1E, d6);
        let mut d6 = self.rb(*a0); *a0 += 1;
        let d5 = d6;
        if d6 & 0x80 != 0 { d6 = self.rb(a1 + 0x1C); }
        if d6 != 0 && d6 != 0x7F && self.rb(a1 + 0x1B) == 1 { self.wb(a1 + 0x33, 1); }
        self.wb(a1 + 0x1C, d6);
        let mut d6 = d5;
        if d6 & 0x80 == 0 { d6 = self.rb(*a0); *a0 += 1; }
        self.wb(a1 + 0x12, d6 & 0x7F);
        // gate length
        let len = self.rb(a1 + 0x1B) as u32;
        let g = self.rb(a1 + 0x1C) as u32;
        let gate = if g == 0 || g == 0x7F { len as u8 } else {
            let x = (((g + g) * len) as u16 as i16 >> 8) as u8;
            if x == 0 { 1 } else { x }
        };
        self.wb(a1 + 0x1A, gate);
        let d1 = self.rw(a1 + 0x42);
        let mut w = ((d0.wrapping_add(self.rb(a1 + 0x21)) as u16) << 8) & 0xFF00;
        w = w.wrapping_add(self.rw(a1 + 0x38)).wrapping_add(self.rw(a1 + 0x3A)).wrapping_add(self.rw(a1 + 0x52)) & 0x7FFF;
        self.ww(a1 + 0x42, w);
        let x = self.rb(a1 + 0x0F); self.wb(a1 + 0x0A, x);
        let x = self.rb(a1 + 0x2C); self.wb(a1 + 0x2E, x);
        let x = self.rb(a1 + 0x2D); self.wb(a1 + 0x2F, x);
        let x = self.rb(a1 + 0x10); self.wb(a1 + 0x0D, x);
        if x != 0 { self.ww(a1 + 0x4A, 0); }
        self.wb(a1 + 0x0B, 0);
        self.ww(a1 + 0x56, 0);
        if self.rb(*a0) == 0 { *a0 += 1; }
        if self.rb(*a0) == 0xF3 {
            self.ff4812(a1, a0);
        } else {
            let x = self.rb(a1 + 7); self.wb(a1 + 6, x);
            if x != 0 { self.ff488c(a1, d1); }
        }
        let f = self.rw(a1 + 0x42);
        self.ff5f0a(a1, d3, f);
        if self.rb(a1 + 0x2C) != 0 { self.ff5a22(a1, d3, 0); }
        if self.rb(a1 + 0x2D) != 0 { self.ff5a2e(a1, d3, 0); }
        if self.btst(a1 + 0x22, 5) {
            self.bclr(a1 + 0x22, 5);
        } else {
            if self.rb(a1 + 0x13) != 0 { self.ff44f4(a1); }
            if self.rw(VOLW) != 0xFFFF { self.mark_all_vol(); }
            if self.rb(a1 + 0x31) != 0 { self.ff451a(a1, d3); }
        }
        if self.rb(a1 + 0x1D) != 0x7F { self.key_on(d3); }
        let x = self.rb(a1 + 0x1C); self.wb(a1 + 0x1D, x);
    }

    /// M:0xFF4812: inline portamento (F3 prefix after a note)
    fn ff4812(&mut self, a1: u32, a0: &mut u32) {
        *a0 += 1;
        let x = self.rb(*a0); *a0 += 1; self.wb(a1 + 8, x);
        let x = self.rb(*a0); *a0 += 1; self.wb(a1 + 6, x);
        let d6 = self.rb(*a0).wrapping_add(self.rb(a1 + 0x21)); *a0 += 1;
        self.ww(a1 + 0x46, (d6 as u16) << 8);
        if self.rb(a1 + 6) != 0 {
            *a0 += 1;
            let x = self.rb(*a0); *a0 += 1; self.wb(a1 + 0x45, x);
            let x = self.rb(*a0); *a0 += 1; self.wb(a1 + 0x44, x);
        }
    }

    /// M:0xFF483E: compute portamento step towards d0
    fn ff483e(&mut self, a1: u32, mut d0: u16) {
        if d0 >= 0x6000 { d0 = if d0 >= 0x8000 { 0 } else { 0x5FFF }; }
        self.ww(a1 + 0x46, d0);
        let mut diff = d0.wrapping_sub(self.rw(a1 + 0x42));
        let borrow = d0 < self.rw(a1 + 0x42);
        if diff == 0 { diff = 1; }
        let n = self.rb(a1 + 6) as i16 as i32;
        if n == 0 { return }
        let q = if borrow { -((((diff as i16).wrapping_neg()) as i32 / n) as i32) } else { (diff as i16 as i32) / n };
        self.ww(a1 + 0x44, q as u16);
    }

    /// M:0xFF488C: start portamento from previous pitch d1
    fn ff488c(&mut self, a1: u32, d1: u16) {
        let x = self.rb(a1 + 9); self.wb(a1 + 8, x);
        let d0 = self.rw(a1 + 0x42);
        if !self.btst(a1 + 0x22, 4) {
            self.ww(a1 + 0x46, d0);
            let v = d0.wrapping_sub(self.rw(a1 + 0x48));
            self.ww(a1 + 0x42, v);
        } else {
            self.ww(a1 + 0x42, d1);
            self.ff483e(a1, d0);
        }
    }

    /// M:0xFF4938: instrument change (E2)
    fn ff4938(&mut self, a1: u32, d3: u8, a0: &mut u32) {
        let d0 = self.rb(*a0); *a0 += 1;
        if d0 == self.rb(a1 + 0x23) { return }
        self.wb(a1 + 0x23, d0);
        self.ww(a1 + 0x54, 0);
        self.ff5ad6(a1, d3, d0);
    }

    /// M:0xFF48BC: sequence command dispatch (E0..FF), table M:0xFF400A
    fn seq_cmd(&mut self, a1: u32, d3: u8, a0: &mut u32, c: u8) {
        self.cmd_stats[(c - 0xE0) as usize] += 1;
        match c {
            0xE0 => { // rest
                let x = self.rb(*a0); self.wb(a1 + 0x1B, x); self.wb(a1 + 0x1E, x); *a0 += 1;
                for off in [0x1Cu32, 0x1D, 0x1A, 0x12] { self.wb(a1 + off, 0); }
                self.key_off(d3);
                self.bset(FLAGS, 4);
            }
            0xE1 => { // tie/hold with length
                let d0 = self.rb(*a0); let d1 = self.rb(*a0 + 1); *a0 += 2;
                self.wb(a1 + 0x1B, d0); self.wb(a1 + 0x1E, d0);
                self.wb(a1 + 0x1C, d1); self.wb(a1 + 0x1D, d1);
                let mut g = (((d1 << 1) as u32 * d0 as u32) as u16 as i16 >> 8) as u8;
                if g == 0 { g = 1; }
                self.wb(a1 + 0x1A, g);
                self.bset(FLAGS, 4);
            }
            0xE2 => self.ff4938(a1, d3, a0),
            0xE3 => { // pan
                let i = self.rb(*a0) as u32; *a0 += 1;
                let d0 = self.rb(0xFF4952 + i);
                self.wb(a1 + 0x17, d0); self.wb(a1 + 0x16, 0);
                self.ff5a3a(a1, d3, d0);
            }
            0xE4 => { // vibrato
                let x = self.rb(*a0); self.wb(a1 + 0x0F, x);
                let x = self.rb(*a0 + 1); self.wb(a1 + 0x0C, x);
                let d2 = self.rb(*a0 + 2); *a0 += 3;
                self.wb(a1 + 0x11, d2);
                self.ww(a1 + 0x4A, (d2 as u16) << 8);
                self.wb(a1 + 0x10, 0);
            }
            0xE5 => { // random pitch
                let x = self.rb(*a0); self.wb(a1 + 0x20, x);
                let x = self.rb(*a0 + 1); self.wb(a1 + 0x50, x);
                let x = self.rb(*a0 + 2); self.wb(a1 + 0x51, x); *a0 += 3;
                self.wb(a1 + 0x1F, 0); self.ww(a1 + 0x4E, 0);
            }
            0xE6 => { self.wl(a1 + 0x64, *a0); }
            0xE8 => { self.wl(a1 + 0x68, *a0); self.wb(a1 + 3, 0); self.ww(a1 + 0x36, 0); self.ww(a1 + 0x3A, 0); }
            0xE7 => self.loop_cmd(a1, a0, 2, 0x34, 0x38, 0x64),
            0xE9 => self.loop_cmd(a1, a0, 3, 0x36, 0x3A, 0x68),
            0xEA => { // tempo
                let x = self.rb(*a0); *a0 += 1;
                let v = if self.e030 & 1 == 0 { x } else { let t = (x as u32 * 6 / 5) + 2; if t >= 0x100 { 0xFF } else { t as u8 } };
                self.wb(a1 + 4, v);
            }
            0xEB => { // auto-pan
                let d0 = self.rb(*a0); let d1 = self.rb(*a0 + 1); *a0 += 2;
                self.wb(a1 + 0x16, d0); self.wb(a1 + 0x15, d0); self.wb(a1 + 0x18, d1);
                let p = if d1 == 3 { 0x40 } else { 0x80 };
                self.wb(a1 + 0x17, p);
                self.ff5a3a(a1, d3, p);
                self.wb(a1 + 0x19, 0);
            }
            0xEC => { let x = self.rb(*a0); *a0 += 1; self.wb(a1 + 0x21, x); } // transpose
            0xED => { // LFO
                let d0 = self.rb(*a0); *a0 += 1;
                self.wb(LFO_REG, d0);
                self.ff59a0(d3, d0 | 8);
            }
            0xEE => { let d0 = self.rb(*a0); *a0 += 1; self.ww(a1 + 0x3E, (d0 as u16) << 8); self.wb(a1 + 0x13, 0); }
            0xEF => { // volume slide
                let d0 = self.rb(*a0); let d1 = self.rb(*a0 + 1); *a0 += 2;
                self.wb(a1 + 0x13, d0);
                let n = if d0 == 0 { 1 } else { d0 as i32 };
                self.wb(a1 + 0x14, d1);
                let delta = (((d1 as u16) << 8).wrapping_sub(self.rw(a1 + 0x3E)) as i16) as i32;
                self.ww(a1 + 0x40, (delta / n) as i16 as u16);
            }
            0xF0 => { let x = self.rb(*a0); *a0 += 1; self.wb(a1 + 7, x); self.bset(a1 + 0x22, 4); self.wb(a1 + 9, 0); }
            0xF1 => {
                let x = self.rb(*a0); self.wb(a1 + 9, x);
                let x = self.rb(*a0 + 1); self.wb(a1 + 7, x);
                let d2 = self.rb(*a0 + 2); *a0 += 3;
                self.ww(a1 + 0x48, (d2 as u16) << 8);
                self.bclr(a1 + 0x22, 4);
            }
            0xF2 => { let d0 = self.rb(*a0) as i8 as i16; *a0 += 1; self.ww(a1 + 0x52, (d0 * 4) as u16); }
            0xF3 | 0xF8 | 0xFA => {}
            0xF4 => {
                let d1 = self.rb(*a0); let d0 = self.rb(*a0 + 1); *a0 += 2;
                self.wb(a1 + 0x2A, d0); self.wb(a1 + 0x2C, d1);
                if d1 == 0 { self.ff5a22(a1, d3, d0); }
            }
            0xF5 => {
                let d0 = self.rb(*a0); let d1 = self.rb(*a0 + 1) << 4; *a0 += 2;
                self.wb(a1 + 0x2B, d1); self.wb(a1 + 0x2D, d0);
                if d0 == 0 { self.ff5a2e(a1, d3, d1); }
            }
            0xF6 => { self.wl(a1 + 0x5C, *a0); let v = self.rb(a1 + 0x22) & 0x3F; self.wb(a1 + 0x22, v); }
            0xF7 => {
                if self.btst(a1 + 0x22, 6) {
                    self.bclr(a1 + 0x22, 6); self.bset(a1 + 0x22, 7);
                    self.wl(a1 + 0x60, *a0);
                    *a0 = self.rl(a1 + 0x5C);
                } else if self.btst(a1 + 0x22, 7) {
                    self.bset(a1 + 0x22, 6); self.bclr(a1 + 0x22, 7);
                    *a0 = self.rl(a1 + 0x60);
                } else {
                    self.bset(a1 + 0x22, 6);
                }
            }
            0xF9 => {
                let d0 = self.rb(*a0); *a0 += 1;
                self.wb(a1 + 0x10, d0);
                if d0 != 0 { let d6 = (self.rb(a1 + 0x11) as u16) << 8; self.ww(a1 + 0x4C, d6 / d0 as u16); }
            }
            0xFB => { let d0 = self.rb(*a0) as u16; *a0 += 1; self.ww(a1 + 0x54, d0 << 5); }
            0xFC => { let x = self.rb(*a0); self.wb(a1 + 0x45, x); let x = self.rb(*a0 + 1); self.wb(a1 + 0x44, x); *a0 += 2; }
            0xFD => { self.rel_jump(a0); }
            0xFE => { self.wl(a1 + 0x58, *a0 + 4); self.bset(a1 + 0x22, 2); self.rel_jump(a0); }
            0xFF => self.end_cmd(a1, d3, a0),
            _ => {}
        }
    }

    /// E7 / E9 loops (M:0xFF49F2 / M:0xFF49B4)
    fn loop_cmd(&mut self, a1: u32, a0: &mut u32, cnt_off: u32, voff: u32, poff: u32, toff: u32) {
        let d0 = self.rb(*a0); *a0 += 1;
        if d0 != 0 {
            let c = self.rb(a1 + cnt_off).wrapping_add(1); self.wb(a1 + cnt_off, c);
            if c == d0 {
                if cnt_off == 2 { self.wb(a1 + 2, 0); self.ww(a1 + 0x34, 0); self.ww(a1 + 0x38, 0); }
                *a0 += 2;
                return;
            }
        }
        let d1 = self.rb(*a0) as i8 as i16; *a0 += 1;
        if d1 != 0 { self.wb(a1 + 0x31, 1); let v = self.rw(a1 + voff).wrapping_add(d1 as u16); self.ww(a1 + voff, v); }
        let d2 = (self.rb(*a0) as i8 as i16) << 3; *a0 += 1;
        let v = self.rw(a1 + poff).wrapping_add(d2 as u16); self.ww(a1 + poff, v);
        *a0 = self.rl(a1 + toff);
    }

    /// M:0xFF4BFA: a0 += long at a0 (relative to the offset field)
    fn rel_jump(&mut self, a0: &mut u32) {
        let off = self.rl(*a0);
        *a0 = a0.wrapping_add(off) & 0xFFFFFF;
    }

    /// FF: return from FE call or end of track (M:0xFF4C12)
    fn end_cmd(&mut self, a1: u32, d3: u8, a0: &mut u32) {
        if self.btst(a1 + 0x22, 2) {
            self.bclr(a1 + 0x22, 2);
            *a0 = self.rl(a1 + 0x58);
            return;
        }
        self.wb(a1, 0);
        if d3 < 6 || d3 >= 8 {
            self.bset(a1 + 0x22, 5);
            self.ff57cc(d3);
            self.key_off(d3);
            self.ff4eb6(a1, d3);
        }
        self.bset(FLAGS, 4);
        if !self.btst(SFXF, 6) {
            let v = self.rb(ACTIVE) & !(1 << (d3 & 7)); self.wb(ACTIVE, v);
            if v == 0 { self.bset(STATUS, 7); }
        }
        // status byte to COMCMD7 (M:0xFF4C7A..) – not modelled
    }

    /// M:0xFF4EB6: after an sfx ends on ch 4/5, restore the music instrument/volume
    fn ff4eb6(&mut self, a1: u32, d3: u8) {
        if !self.btst(SFXF, 7) { self.after_eb6(); return }
        let (bit, idaddr) = match d3 { 4 => (0, SFX_ID0), 5 => (1, SFX_ID1), _ => { self.after_eb6(); return } };
        if !self.btst(SFXF, bit) { self.after_eb6(); return }
        self.bclr(SFXF, bit);
        self.wb(a1 + 1, 0);
        self.wb(idaddr, 0);
        let d0 = self.rb(a1 + 0x23);
        let m = if d3 == 4 { 0xFF7C00 } else { 0xFF7C80 };
        self.ww(m + 0x56, 0xFFFF);
        self.ff451a(m, d3);
        self.bset(SFXF, 5);
        if d0 != self.rb(m + 0x23) {
            let ins = self.rb(m + 0x23);
            self.ff5ad6(m, d3, ins);
        } else {
            self.ff5e2c(m, d3);
        }
        self.bclr(SFXF, 5);
        self.after_eb6();
    }

    fn after_eb6(&mut self) {
        if self.rb(SFXF) & 3 == 0 { self.bclr(SFXF, 7); self.bset(STATUS, 6); }
    }

    /// M:0xFF4D22: PCM drum tracks (slots 6,7 at M:0xFF7D00)
    fn ff4d22(&mut self) {
        self.wb(DRUM0, 0); self.wb(DRUM1, 0); self.wb(DRUM2, 0);
        let mut a1 = 0xFF7D00u32;
        for d3 in 6..8u8 {
            if self.rb(a1) != 0 {
                let t = self.rb(a1 + 4);
                let (s, c) = self.rb(a1 + 5).overflowing_add(t);
                self.wb(a1 + 5, s);
                if c {
                    self.ticks[d3 as usize] += 1;
                    let mut a0 = self.rl(a1 + 0x6C);
                    let v = self.rb(a1 + 0x1E).wrapping_sub(1); self.wb(a1 + 0x1E, v);
                    if v == 0 {
                        loop {
                            let d6 = self.rb(a0); a0 += 1;
                            if d6 < 0x60 { continue }
                            if d6 >= 0xE0 {
                                self.bclr(FLAGS, 4);
                                self.seq_cmd(a1, d3, &mut a0, d6);
                                if self.btst(FLAGS, 4) { break }
                                continue;
                            }
                            self.ff45ea(a1, d3, &mut a0, d6);
                            break;
                        }
                        self.wl(a1 + 0x6C, a0);
                    }
                }
            }
            a1 += 0x80;
        }
        let (x, y, z) = (self.rb(DRUM0), self.rb(DRUM1), self.rb(DRUM2));
        if x != 0 || y != 0 || z != 0 { self.drums.push(DrumEvent { b0: x, b1: y, b2: z }); }
    }

    /// M:0xFF45EA: drum note (0x60..0xDF) on slot 6/7
    fn ff45ea(&mut self, a1: u32, d3: u8, a0: &mut u32, d5: u8) {
        if d5 == 0xE0 || d5 == 0xE2 { return }
        let n = d5.wrapping_sub(0x60);
        if d3 != 7 { let v = self.rb(DRUM0) | n; self.wb(DRUM0, v); }
        else { let v = self.rb(DRUM1) | (n << 3); self.wb(DRUM1, v); }
        let x = self.rb(*a0); self.wb(a1 + 0x1B, x); self.wb(a1 + 0x1E, x); *a0 += 1;
        let x = self.rb(*a0); self.wb(a1 + 0x1D, x); *a0 += 1;
        let d6 = self.rb(*a0) as u32; *a0 += 1;
        let v = (((d6 * self.rb(a1 + 0x3E) as u32) & 0xFFFF) >> 7) >> 2;
        if d3 != 7 {
            let w = self.rb(DRUM1) | (v as u8 & 7); self.wb(DRUM1, w);
            let w = self.rb(DRUM2) | ((v << 2) as u8 & 0x60); self.wb(DRUM2, w);
        } else {
            let w = self.rb(DRUM2) | v as u8; self.wb(DRUM2, w);
        }
    }

    /// M:0xFF4E08: sound-effect FM tracks (slots 8,9 at M:0xFF7E00 → channels 4,5)
    fn ff4e08(&mut self) {
        if self.btst(FLAGS, 6) { self.bclr(SFXF, 6); return }
        let v = self.rb(SFXF) & 3; // `andi.b #3,$FF7F24` modifies memory (M:0xFF4E14)
        self.wb(SFXF, v);
        if v == 0 { return }
        self.bset(SFXF, 7); self.bset(SFXF, 6);
        let mut a1 = 0xFF7E00u32;
        self.a3 = 1;
        for d3 in 4..6u8 {
            let ch = self.rb(0xFF4000 + d3 as u32);
            self.wb(CHN, ch);
            if self.rb(a1 + 0x32) != 0 {
                self.ff5a16(a1, d3);
                self.ff57cc(d3);
                self.wb(a1 + 0x32, 0);
            }
            if self.rb(a1) != 0 { self.track_step(a1, d3); }
            a1 += 0x80;
        }
        self.bclr(SFXF, 6);
    }

    // ---------------------------------------------------------------- chip helpers
    /// M:0xFF57CC → M:0xFF5824: release (SL/RR=FF) for the channel unless blocked; falls into key-off
    fn ff57cc(&mut self, d3: u8) {
        if self.blocked(d3) { self.key_off(d3); return }
        let ch = self.rb(CHN);
        self.ff582a(ch);
        self.key_off(d3);
    }

    /// M:0xFF5890: key off
    fn key_off(&mut self, d3: u8) {
        if self.blocked(d3) || d3 >= 6 { return }
        let v = [0u8, 1, 2, 4, 5, 6][d3 as usize];
        self.ym2(0x28, v);
    }

    /// M:0xFF5918: key on
    fn key_on(&mut self, d3: u8) {
        if self.blocked(d3) || d3 >= 6 { return }
        let v = [0xF0u8, 0xF1, 0xF2, 0xF4, 0xF5, 0xF6][d3 as usize];
        self.ym2(0x28, v);
    }

    /// M:0xFF59A0: LFO register
    fn ff59a0(&mut self, d3: u8, d0: u8) {
        if self.blocked(d3) { return }
        self.ym2(0x22, d0);
    }

    /// M:0xFF5A16 / 5A22 / 5A2E / 5A3A: B4 (pan/AMS/PMS) register with field masks
    fn ff5a16(&mut self, a1: u32, _d3: u8) { let d5 = self.rb(a1 + 0x30) & 0xC0; self.write_b4(a1, d5); }
    fn ff5a22(&mut self, a1: u32, d3: u8, d0: u8) { let d5 = self.rb(a1 + 0x30) & 0xF0; self.b4_field(a1, d3, d5, d0); }
    fn ff5a2e(&mut self, a1: u32, d3: u8, d0: u8) { let d5 = self.rb(a1 + 0x30) & 0xCF; self.b4_field(a1, d3, d5, d0); }
    fn ff5a3a(&mut self, a1: u32, d3: u8, d0: u8) { let d5 = self.rb(a1 + 0x30) & 0x3F; self.b4_field(a1, d3, d5, d0); }
    fn b4_field(&mut self, a1: u32, d3: u8, d5: u8, d0: u8) {
        if self.blocked(d3) { return }
        self.write_b4(a1, d5 | d0);
    }
    fn write_b4(&mut self, a1: u32, mut d5: u8) {
        if self.btst(FLAGS, 3) { d5 |= 0xC0; }
        self.wb(a1 + 0x30, d5);
        let r = self.rb(CHN).wrapping_add(0xB4);
        self.ym3(r, d5);
    }

    /// M:0xFF5AD6: load instrument d0 (Z80 RAM table) into the channel
    fn ff5ad6(&mut self, a1: u32, d3: u8, d0: u8) {
        if self.blocked(d3) { return }
        let tbl = 0xA00000u32;
        let off = self.rl(tbl + (d0 as u32) * 4);
        let mut a6 = tbl.wrapping_add(off);
        let ch = self.rb(CHN);
        let fbalg = self.rb(a6); a6 += 1;
        self.ym3(ch.wrapping_add(0xB0), fbalg);
        let alg = fbalg & 7;
        self.wb(a1 + 0x25, alg);
        let mut carriers = self.rb(0xFF5E52 + alg as u32) >> 4;
        // operators in register order 0x30, 0x38, 0x34, 0x3C; TL stored at $26..$29
        for (k, base) in [0x30u8, 0x38, 0x34, 0x3C].iter().enumerate() {
            let mut r = ch.wrapping_add(*base);
            let dtmul = self.rb(a6); a6 += 1;
            self.ym3(r, dtmul); r = r.wrapping_add(0x10);
            let mut tl = self.rb(a6); a6 += 1;
            self.wb(a1 + 0x26 + k as u32, tl);
            if carriers & 1 != 0 { tl = 0xFF; }
            carriers >>= 1;
            self.ym3(r, tl); r = r.wrapping_add(0x10);
            for _ in 0..4 { let v = self.rb(a6); a6 += 1; self.ym3(r, v); r = r.wrapping_add(0x10); }
        }
        self.ff5e2c(a1, d3);
    }

    /// M:0xFF5E2C: restore pan after instrument load
    fn ff5e2c(&mut self, a1: u32, d3: u8) {
        if !self.btst(SFXF, 5) {
            let d0 = 0xC0; self.wb(a1 + 0x17, d0); self.wb(a1 + 0x16, 0);
            self.ff5a3a(a1, d3, d0);
        } else {
            let d0 = self.rb(a1 + 0x17);
            self.ff5a3a(a1, d3, d0);
        }
    }

    /// M:0xFF5E5E: write carrier TLs for volume d0 (0..0x7F)
    fn ff5e5e(&mut self, a1: u32, d3: u8, d0: u8) {
        if self.blocked(d3) { return }
        let ch = self.rb(CHN);
        let alg = self.rb(a1 + 0x25) as u32;
        let mut mask = self.rb(0xFF5E52 + (alg & 7));
        for d2 in (0..4u32).rev() {
            let reg = self.rb(0xFF5E5A + d2).wrapping_add(ch);
            let carry = mask & 0x80 != 0;
            mask <<= 1;
            if carry {
                let mut d5 = (self.rb(a1 + 0x26 + d2) as i32 + 0x7F - d0 as i32) & 0xFFFF;
                if (d5 & 0xFF) >= 0x80 { d5 = 0x7F; }
                self.ym3(reg, d5 as u8);
            }
        }
    }

    /// M:0xFF5F0A: write frequency for pitch word d0 (note<<8 | fraction)
    fn ff5f0a(&mut self, a1: u32, d3: u8, d0: u16) {
        if self.blocked(d3) { return }
        self.ww(a1 + 0x72, d0);
        let frac = (d0 & 0xFF) as u32;
        let note = (d0 >> 8) as u32;
        let step = self.rb(0xFF60CA + note) as u32;
        let base = self.rw(0xFF5FCA + note * 2) as u32;
        let v = (((step * frac) & 0xFFFF) >> 8) + base;
        let ch = self.rb(CHN);
        self.ym3(ch.wrapping_add(0xA4), (v >> 8) as u8);
        self.ym3(ch.wrapping_add(0xA0), v as u8);
    }

    /// Per-track control-state snapshot used for loop detection (`slot` 0..9). All 0x80 bytes of
    /// the track except free-running/derived per-frame fields (tempo accumulator, vibrato/auto-pan/
    /// noise phases, portamento and volume-slide progress, current pitch/volume, dirty flags).
    pub fn track_key(&self, slot: u32) -> Vec<u8> {
        const SKIP: &[u32] = &[0x05, 0x06, 0x0B, 0x13, 0x15, 0x17, 0x19, 0x1F, 0x24, 0x30, 0x31, 0x32, 0x33,
            0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x4A, 0x4B, 0x4E, 0x4F, 0x56, 0x57, 0x70, 0x71, 0x72, 0x73];
        let a = 0xFF7A00 + slot * 0x80;
        (0..0x80u32).filter(|o| !SKIP.contains(o)).map(|o| self.rb(a + o)).collect()
    }

    pub fn peek(&self, a: u32) -> u8 { self.rb(a) }

    pub fn track_active(&self, slot: u32) -> bool { self.rb(0xFF7A00 + slot * 0x80) != 0 }

    /// true while any music track (slots 0..7) is active
    pub fn music_active(&self) -> bool { (0..8u32).any(|s| self.rb(0xFF7A00 + s * 0x80) != 0) }
    pub fn sfx_active(&self) -> bool { (8..10u32).any(|s| self.rb(0xFF7A00 + s * 0x80) != 0) }
    pub fn sound_type(&self, id: u8) -> u8 { self.rb(0xFF6142 + id as u32 * 8) }
}
