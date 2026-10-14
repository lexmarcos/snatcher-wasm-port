//! Main-CPU boot screens: copyright notice and the Konami logo with the laser line.
//!
//! This is main-CPU code in the IP (not a DATA module): main state `$E022` = 0
//! (`sub_ff2b66`, M:0xFF2B66, VDP init) and 1 (`sub_ff2be4`, M:0xFF2BE4, 12 steps `$E04A`,
//! table M:0xFF2C22), both run from the VBlank handler while the sub CPU waits in mode 0 for
//! the main to become ready. The code writes the VDP directly; here the same writes go to
//! `sncore::gfx::Vdp` (with `Display::main_only`). Data (LZKN1 tiles/nametables, the
//! palette table M:0xFF1ABE, the copyright strings M:0xFF3094) is read from the user's IP
//! image (`HEADER.BIN` bytes 0x200..0x800 followed by `IP.BIN`, loaded at M:0xFF0000).
//! Evidence and per-step frame timing: docs/re/modules.md §2.
use crate::game::{Engine, Out};
use sncore::gfx::Sprite;

const IP_BASE: u32 = 0xFF0000;

pub struct Boot {
    ip: Vec<u8>,
    /// `$E022` (0 = VDP init routine, 1 = copyright/logo routine, 2 = done)
    e022: u8,
    e04a: u8,
    e04c: i16,
    e04e: i16,
    /// `$E700` word 0: plane A horizontal scroll
    e700: u16,
    /// `$EB00` word 0: plane A vertical scroll (VSRAM)
    eb00: u16,
    /// `$EB80`: 64-colour palette buffer (uploaded to CRAM in VBlank when `$E0E0` bit0)
    pal: [u16; 64],
    /// `$ED00`: the single main-side sprite (laser head)
    spr: Option<[u16; 4]>,
    /// `$E020`: main frame counter
    frame: u16,
    /// extra frames a heavy step keeps the VBlank handler busy (LZKN1 + DMA), calibrated
    /// against the oracle (docs/re/modules.md §2)
    stall: u8,
    pub done: bool,
}

impl Boot {
    /// `None` when the IP image is not available (the front end did not provide it).
    pub fn new(e: &Engine) -> Option<Boot> {
        let h = e.files.get("HEADER.BIN")?;
        let ip = e.files.get("IP.BIN")?;
        if h.len() < 0x800 { return None; }
        let mut img = h[0x200..0x800].to_vec();
        img.extend_from_slice(ip);
        Some(Boot { ip: img, e022: 0, e04a: 0, e04c: 0, e04e: 0, e700: 0, eb00: 0, pal: [0; 64], spr: None,
            frame: 0, stall: 0, done: false })
    }

    fn rd16(&self, a: u32) -> u16 {
        let o = (a - IP_BASE) as usize;
        u16::from_be_bytes([self.ip[o], self.ip[o + 1]])
    }

    /// `lzkn_to_vram` M:0xFF1902: LZKN1 blob at `src` → main RAM → VRAM `dst`.
    fn lzkn_to_vram(&self, e: &mut Engine, src: u32, dst: u16) {
        let o = (src - IP_BASE) as usize;
        match sncore::lzkn::decompress(&self.ip[o..]) {
            Ok((out, _)) => e.disp.vdp.write_vram(dst, &out),
            Err(err) => e.out.push(Out::Log(format!("boot: LZKN1 at {src:#x}: {err:?}"))),
        }
    }

    /// `sub_ff19f2` M:0xFF19F2: palette lines 0..3 = table entries d0 bytes (M:0xFF1ABE + 32*n).
    fn pal_set(&mut self, d0: u32) {
        for line in 0..4 {
            let n = (d0 >> (24 - 8 * line)) & 0xFF;
            for c in 0..16 { self.pal[line * 16 + c] = self.rd16(0xFF1ABE + n * 32 + 2 * c as u32); }
        }
    }

    fn vdp_word(e: &mut Engine, addr: u16, w: u16) {
        let a = addr as usize & 0xFFFF;
        e.disp.vdp.vram[a] = (w >> 8) as u8;
        e.disp.vdp.vram[(a + 1) & 0xFFFF] = w as u8;
    }

    /// One main VBlank. Returns true when the boot screens are over (`$E022` → 2).
    pub fn tick(&mut self, e: &mut Engine) -> bool {
        if self.done { return true; }
        self.frame = self.frame.wrapping_add(1);
        e.disp.main_only = true;
        if self.stall > 0 { self.stall -= 1; self.present(e); return false; }
        let before = (self.e022, self.e04a);
        match self.e022 {
            0 => self.state0(e),
            _ => self.state1(e),
        }
        if e.trace && before != (self.e022, self.e04a) {
            e.out.push(Out::Log(format!("boot e022={} e04a={:#x}", self.e022, self.e04a)));
        }
        // after `finish` the in-game register set is active (M:0xFF1538): do not push the boot state again
        if !self.done { self.present(e); }
        self.done
    }

    /// Push the main-side VDP state into the renderer (CRAM DMA M:0xFF1A5A, scroll M:0xFF3CC6,
    /// sprite DMA M:0xFF179C with `$E048` = 0 → SAT 0xBE00).
    fn present(&self, e: &mut Engine) {
        let v = &mut e.disp.vdp;
        v.plane_a = 0xE000; v.plane_b = 0xC000; v.plane_w = 64; v.plane_h = 64; // regs 82 38 84 06 90 11
        v.split_line = None;
        v.cram = self.pal;
        v.cram_b = self.pal;
        v.hscroll_a = self.e700 as i16; v.hscroll_b = 0;
        v.vscroll_a = self.eb00 as i16; v.vscroll_b = 0;
        v.backdrop = 0;
        e.disp.main_sat = match self.spr {
            Some(s) if s[0] != 0 => vec![Sprite { y: s[0] as i16, size: (s[1] >> 8) as u8, link: s[1] as u8 & 0x7F, attr: s[2], x: s[3] as i16 }],
            _ => vec![],
        };
    }

    /// `sub_ff2b66` (M:0xFF2B66): `$E04A` 0..6.
    fn state0(&mut self, e: &mut Engine) {
        match self.e04a {
            0 => {
                // reg 7 = 0, clear scroll, FM request 0xF2, sub_ff1528: display off, clear VRAM,
                // CRAM, VSRAM; reg 16 = 0x11 (64×64) and the boot register table M:0xFF158E
                self.e700 = 0; self.eb00 = 0;
                e.out.push(Out::Log("fm 0xF2".into()));
                e.disp.vdp.vram.iter_mut().for_each(|b| *b = 0);
                self.pal = [0; 64];
                self.spr = None;
                self.e04a = 1;
            }
            3 => {
                self.lzkn_to_vram(e, 0xFF1B9E, 0x0000);
                self.e04a = 4;
                self.stall = 3; // oracle: e04a 3 → 5 between frames 3 and 7
            }
            6 => { self.e022 = 1; self.e04a = 0; self.e04c = 0; }
            _ => self.e04a += 1,
        }
    }

    /// `sub_ff2be4` (M:0xFF2BE4).
    fn state1(&mut self, e: &mut Engine) {
        // Start skips from step 2 on (COMCMD0 lo bit7 = newly pressed Start)
        let start = e.input.as_ref().map(|i| i.pressed & 0x80 != 0).unwrap_or(false);
        if self.e04a >= 2 && start {
            e.out.push(Out::Log("fm 0xF2".into()));
            self.e700 = 0; self.eb00 = 0;
            self.finish(e);
            return;
        }
        match self.e04a {
            0 => {
                // cold boot: "SEGA CD SNATCHER" signature absent at M:$FFEF10 → copyright screen
                self.lzkn_to_vram(e, 0xFF286E, 0x2000);
                self.pal_set(0x0606_0606);
                // strings M:0xFF3094 (0x6B bytes) → words (byte | 0x100) at M:$FFBC00, then six
                // DMA-queue entries (M:0xFF2CBA…): (source byte offset, VRAM dest, length in bytes)
                const LINES: [(u32, u16, u16); 6] = [
                    (0x00, 0xE294, 0x16), (0x18, 0xE39C, 0x04), (0x1C, 0xE482, 0x3C),
                    (0x58, 0xE704, 0x3A), (0x94, 0xEA94, 0x16), (0xAC, 0xEB8C, 0x2A),
                ];
                for (src, dst, len) in LINES {
                    for k in 0..(len / 2) as u32 {
                        let i = src / 2 + k;
                        let b = self.ip[(0xFF3094 - IP_BASE + i) as usize] as u16;
                        Self::vdp_word(e, dst + 2 * k as u16, b | 0x100);
                    }
                }
                self.e04a = 1;
                self.stall = 5; // oracle: e04a 0 → 1 between frames 9 and 13 (watch: 13)
            }
            1 => {
                // 0xB4 frames (and sub ready, $E044/$E02A – already true here)
                self.e04c += 1;
                if self.e04c >= 0xB4 { self.e04a = 2; self.logo_palette(); }
            }
            2 => {
                // display off (CRAM[0] = 0), plane B nametable
                self.lzkn_to_vram(e, 0xFF25C8, 0xC000);
                self.e04c = 2;
                self.e04a = 3;
                self.stall = 2; // oracle watch: e04a 2 → 3 at frame 198
            }
            3 => {
                self.e04c -= 1;
                if self.e04c != 0 { return; }
                self.lzkn_to_vram(e, 0xFF24B4, 0xE000);
                // fills: 0xE140 0x600×0x6001, 0xE0C0 32×0x6002, 0xE140 32×0x6003, 0xE080 0x6002, 0xE100 0x6003
                for k in 0..0x600u16 { Self::vdp_word(e, 0xE140 + 2 * k, 0x6001); }
                for k in 0..0x20u16 { Self::vdp_word(e, 0xE0C0 + 2 * k, 0x6002); }
                for k in 0..0x20u16 { Self::vdp_word(e, 0xE140 + 2 * k, 0x6003); }
                Self::vdp_word(e, 0xE080, 0x6002);
                Self::vdp_word(e, 0xE100, 0x6003);
                self.spr = Some([0x0088, 0x0500, 0xE004, 0x0080]);
                self.e04a = 4;
                self.stall = 2; // oracle watch: e04a 3 → 4 at frame 202
            }
            4 => {
                // laser head: tile = 4/8/12/16 by frame, x += 6, plane A hscroll += 6 up to 0x100
                if let Some(s) = self.spr.as_mut() {
                    const T: [u16; 4] = [4, 8, 12, 16];
                    s[2] = (s[2] & 0xFF00) | T[((self.frame & 0xFF) as usize >> 1) & 3];
                    s[3] = s[3].wrapping_add(6);
                }
                let d = (self.e700.wrapping_add(6)) & 0x1FF;
                self.e700 = d;
                if d >= 0x100 { self.e700 = 0x100; self.e04c = 0x10; self.e04e = 5; self.e04a = 5; }
            }
            5 => {
                let c = self.e04c;
                self.e04c -= 1;
                if self.e04c < 0 { self.e04a = 6; return; }
                if c & 7 == 0 {
                    // addi.w #$20,-(a0) for e04e+1 words below $EBF2 (palette line 3, colours 8..)
                    for k in 0..=(self.e04e as usize) {
                        let i = (0xEBF2 - 0xEB80) / 2 - 1 - k;
                        self.pal[i] = self.pal[i].wrapping_add(0x20);
                    }
                    self.e04e += 1;
                }
            }
            6 => {
                // plane A scrolls up 2 lines per frame until VSRAM < 0x318
                let d = self.eb00.wrapping_sub(2) & 0x3FF;
                self.eb00 = d;
                if d < 0x318 {
                    self.e04a = 7;
                    self.e04e = 0x1E;
                    for c in 0..32 { self.pal[16 + c] = self.rd16(0xFF1ADE + 2 * c as u32); } // lines 1,2
                    e.out.push(Out::Log("fm 0x3F".into()));
                }
            }
            7 | 8 => {
                if self.frame & 1 != 0 { return; }
                let d1 = (self.e04e & 0xFF) as usize; // byte offset into the line
                let n = ((0x20 - d1) >> 1) as usize;
                for k in 0..n {
                    let (src, dst) = (2 * k, d1 + 2 * k);
                    if self.e04a == 7 {
                        self.pal[(0x20 + dst) / 2] = self.rd16(0xFF1ABE + 0x2C + src as u32);
                        self.pal[(0x40 + dst) / 2] = self.rd16(0xFF1ABE + 0x4C + src as u32);
                    } else {
                        self.pal[dst / 2] = self.rd16(0xFF1ABE + 0x0C + src as u32);
                    }
                }
                self.e04e -= 2;
                if self.e04e == 0xA {
                    if self.e04a == 7 {
                        self.e04e = 0x1E;
                        for c in 0..16 { self.pal[c] = self.rd16(0xFF1ABE + 2 * c as u32); }
                        self.pal[0] = 0;
                        self.e04a = 8;
                    } else { self.e04e = 0; self.e04c = 3; self.e04a = 9; }
                }
            }
            9 => {
                self.e04c -= 1;
                if self.e04c != 0 { return; }
                self.e04c = 3;
                self.pal[0] = self.pal[0].wrapping_add(0x222);
                if self.pal[0] == 0xEEE {
                    self.pal_set(0x0001_0203);
                    self.e04e = 0x200;
                    self.e04a = 10;
                }
            }
            10 => { self.e04e -= 1; if self.e04e == 0 { self.e04e = 0x28; self.e04a = 11; } }
            11 => {
                self.e04e -= 1;
                if self.e04e == 0 { self.e700 = 0; self.eb00 = 0; self.finish(e); return; }
                if self.e04e & 1 != 0 { return; }
                // fade all 64 colours one step toward black (M:0xFF3036)
                for c in self.pal.iter_mut() {
                    let (mut b, mut g, mut r) = (*c & 0xE00, *c & 0xE0, *c & 0xE);
                    if b != 0 { b -= 0x200; }
                    if g != 0 { g -= 0x20; }
                    if r != 0 { r -= 2; }
                    *c = b | g | r;
                }
            }
            _ => {}
        }
    }

    /// Palette for the logo (M:0xFF2D76): table entries 4,4,4,5 and a green ramp at $EBE4.
    fn logo_palette(&mut self) {
        self.pal_set(0x0404_0405);
        // M:0xFF2D80: `clr.l (a0)+` (colours 50,51), `$200040`, `$600080`, `$00A0` (colours 52..56)
        for (k, v) in [0u16, 0, 0x20, 0x40, 0x60, 0x80, 0xA0].iter().enumerate() { self.pal[(0xEBE4 - 0xEB80) / 2 + k] = *v; }
    }

    /// `sub_ff1538` (M:0xFF1538): clear VRAM/CRAM/VSRAM, in-game register set, `$E026` = 1.
    fn finish(&mut self, e: &mut Engine) {
        e.disp.vdp.vram.iter_mut().for_each(|b| *b = 0);
        let d = sncore::gfx::Vdp::default();
        let v = &mut e.disp.vdp;
        v.cram = [0; 64]; v.cram_b = [0; 64];
        v.plane_a = d.plane_a; v.plane_b = d.plane_b; v.plane_w = d.plane_w; v.plane_h = d.plane_h;
        v.split_line = d.split_line;
        v.hscroll_a = 0; v.hscroll_b = 0; v.vscroll_a = 0; v.vscroll_b = 0;
        e.disp.main_sat.clear();
        e.disp.main_only = false;
        self.e022 = 2;
        self.done = true;
    }
}
