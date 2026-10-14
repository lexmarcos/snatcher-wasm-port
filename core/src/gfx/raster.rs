//! Main-CPU display logic: screen effects (M:0xFF332C, table M:0xFF348E) and the H-INT raster
//! modes (M:0xFF0AD4, table M:0xFF0AF8; handlers M:0xFF0C4A…M:0xFF123A). Produces a per-line
//! raster description consumed by [`super::Vdp::render`]. Evidence: docs/re/graphics.md §7.

/// Per-line overrides produced by the H-INT simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineState {
    pub vscroll_a: i16,
    pub vscroll_b: i16,
    pub display: bool,
}

#[derive(Debug, Clone)]
pub struct Raster {
    pub lines: Vec<LineState>,
    /// First line of the set-B palette stream (H-INT handler A at count 0x88), if any.
    pub pal_split: Option<u16>,
    /// Plane B per-line hscroll for lines 8.. (line-scroll effects 3–6), else None.
    pub line_hscroll_b: Option<Vec<i16>>,
}

/// Main-CPU RAM variables used by the effect / raster code (names are their addresses).
#[derive(Debug, Clone, Default)]
pub struct MainDisplay {
    pub e060: u8, pub e061: u8, pub e062: u16, pub e064: u16, pub e066: u16, pub e068: u16,
    pub e06a: u16, pub e06c: u16, pub e06e: u16, pub e070: u8, pub e071: u8,
    pub e076: u16, pub e078: u16, pub e07a: u16, pub e07c: u16, pub e07e: u16, pub e080: u16,
    pub e082: u16, pub e084: u16, pub e086: u16, pub e08a: u16,
    /// $E036: palette-split request from the sub (M:0xFF3298, mailbox $236405).
    pub e036: u16,
    /// $E032: H-INT handler (low word of the handler address).
    pub e032: u16,
    /// $EB00 (vscroll A,B) and $E700 (hscroll A,B) shadows.
    pub eb00: (u16, u16),
    pub e700: (u16, u16),
    /// $E722.. line hscroll table (128 entries; only the first word of each long is used).
    pub wave: Vec<u16>,
    /// Reg 0x0B low bits (3 = line scroll), $FFEF06.
    pub line_scroll: bool,
    pub frame: u16,
    /// Display-enable state left by the previous frame's H-INT handlers (reg 1 DISP).
    pub disp_off_carry: bool,
}

impl MainDisplay {
    pub fn new() -> Self { MainDisplay { wave: vec![0; 128], ..Default::default() } }

    /// Mailbox from the sub CPU (wram_sync M:0xFF320A): $236408 → $E060, $23640C → $E070.
    pub fn mailbox(&mut self, fx_a: u8, fx_b: u8) {
        if fx_a != 0 { self.e060 = fx_a; }
        if fx_b != 0 { self.e070 = fx_b; }
    }

    fn reset_a(&mut self) { // M:0xFF33A2
        self.e060 = 0; self.e061 = 0; self.e06e = 0; self.e06c = 0; self.e066 = 0; self.e068 = 0; self.e062 = 0; self.e064 = 0;
    }

    fn reset_all(&mut self) { // M:0xFF3410 (+ M:0xFF3438, M:0xFF344C)
        self.e078 = 0; self.e086 = 0; self.e08a = 0;
        self.e060 = 0; self.e061 = 0; self.e062 = 0; self.e064 = 0; self.e06e = 0; self.e06c = 0;
        self.e066 = 0; self.e068 = 0; self.e06a = 0;
        self.e070 = 0; self.e071 = 0; self.line_scroll = false; self.e700 = (0, 0); self.eb00 = (0, 0);
        self.e076 = 0;
    }

    /// Per-frame effect dispatcher M:0xFF332C.
    pub fn effects_step(&mut self) {
        let d0 = self.e060;
        if !(d0 == self.e061 && self.e061 != 0) {
            if d0 == 0xFE {
                if self.e061 == 0x17 {
                    self.eb00 = (0, 0);
                    if self.e06a != 0 { self.eb00 = (0, 0); self.e06a = 0; self.reset_a(); self.e060 = 0x0F; self.e061 = 0x0F; }
                    else { self.reset_a(); }
                } else { self.reset_a(); }
            } else if d0 != 0 {
                if d0 == 0x0F && self.e061 == 0x17 { self.e060 = 0x17; self.e06a = 1; }
                else { self.e061 = d0; self.e066 = 0; self.e068 = 0; self.e062 = 0; self.e064 = 0; }
            }
        }
        // second channel (M:0xFF33BC)
        let d0 = self.e070;
        if !(d0 == self.e071 && self.e071 != 0) {
            match d0 {
                0xFF | 0xFC => { self.reset_all(); }
                0xFD => { self.e078 |= 0x80; }
                0 => {}
                _ => {
                    self.e071 = d0;
                    if d0 < 3 { self.e076 = 0; } else { self.e078 = 0; self.e086 = 0; }
                }
            }
        }
        // run (M:0xFF345A)
        if self.e078 != 0 && self.e078 & 0x80 == 0 { let e = self.e078 as u8; self.effect(e, 0); }
        if self.e070 != 0 { let d1 = self.e062; self.effect(self.e070, d1); }
        if self.e060 != 0 { let d1 = self.e062; self.effect(self.e060, d1); }
    }

    fn wave_init(&mut self) { // M:0xFF34E0
        self.e078 = self.e070 as u16; self.e070 = 0; self.e07a = 0x7F; self.e076 = 0;
        self.e07c = 0; self.e07e = 0; self.e080 = 0; self.e082 = 0; self.e084 = 0;
        for w in self.wave.iter_mut() { *w = 0; }
        self.line_scroll = true;
    }

    fn wave_add(&mut self, first: usize, count: usize, d: i16) {
        for i in first..first + count { if i < 128 { self.wave[i] = (self.wave[i] as i16).wrapping_add(d) as u16 & 0x3FF; } }
    }

    /// Effect table M:0xFF348E.
    fn effect(&mut self, n: u8, d1: u16) {
        match n {
            1 | 2 => { self.line_scroll = false; self.e076 = 1; }
            3 => {
                if self.e078 == 0 { self.wave_init(); return; }
                self.e07c += 1; self.e07e += 1; self.e080 += 1; self.e082 += 1; self.e084 += 1;
                let f = self.frame;
                if f & 1 == 0 { self.wave_add(0, 8, -1); }
                if f & 3 == 0 { self.wave_add(8, 9, -1); }
                if self.e07c == 6 { self.e07c = 0; self.wave_add(17, 12, -1); }
                if f & 7 == 0 { self.wave_add(29, 10, -1); }
                if self.e07e == 10 { self.e07e = 0; self.wave_add(39, 8, -1); }
                if self.e080 == 12 { self.e080 = 0; self.wave_add(47, 6, -1); }
                if self.e082 == 14 { self.e082 = 0; self.wave_add(53, 5, -1); }
                if f & 15 == 0 { self.wave_add(58, 4, -1); }
                if self.e084 == 18 { self.e084 = 0; self.wave_add(62, 3, -1); }
            }
            4 => {
                if self.e078 == 0 { self.wave_init(); return; }
                self.e07c += 1; self.e07e += 1; self.e080 += 1; self.e082 += 1; self.e084 += 1;
                if self.e07c == 7 { self.e07c = 0; self.wave_add(0, 0x70, 1); }
                if self.e07e == 6 { self.e07e = 0; self.wave_add(0x70, 4, 1); }
                if self.e080 == 5 { self.e080 = 0; self.wave_add(0x74, 4, 1); }
                if self.e082 == 4 { self.e082 = 0; self.wave_add(0x78, 4, 1); }
                if self.e084 == 3 { self.e084 = 0; self.wave_add(0x7C, 4, 1); }
            }
            5 => match self.e086 {
                0 => { self.e086 = 1; self.wave_init(); self.e078 = 5; }
                1 => {
                    self.e086 += 1; self.e07a = 0x6F;
                    for (s, c, v) in [(0x30usize, 3usize, 0x3F8u16), (0x33, 2, 0x3F4), (0x35, 3, 0x3F8), (0x48, 24, 2), (0x60, 16, 4)] {
                        for i in s..s + c { if i < 128 { self.wave[i] = v; } }
                    }
                }
                2 => { self.e07c += 1; if self.e07c >= 8 { self.line_scroll = false; self.e086 += 1; } }
                3 => { self.e07e += 1; if self.e07e >= 13 { self.line_scroll = true; self.e086 += 1; } }
                _ => { self.e080 += 1; if self.e080 >= 7 { self.reset_all(); } }
            },
            6 => {
                if self.e078 == 0 { self.wave_init(); return; }
                self.wave_add(0x70, 9, -2);
                self.wave_add(0x79, 7, 2);
            }
            7 => match d1 {
                0 => { self.e06c = 1; self.e032 = 0x0CCE; self.e064 = 5; self.e062 += 1; self.e070 = 0; self.e071 = 0; self.line_scroll = false; self.e700 = (0, 0); self.eb00 = (0, 0); self.e076 = 0; }
                _ => { if (self.e064 as i16) < 0 { self.reset_all(); } }
            },
            8..=12 => {
                if self.e062 < 1 {
                    self.e064 = 0x17; self.e06c = (self.e060 as u16).wrapping_sub(6) & 0xFF; self.e032 = 0x0D6E;
                    self.e062 += 1; self.e070 = 0; self.e071 = 0; self.line_scroll = false; self.e700 = (0, 0); self.eb00 = (0, 0); self.e076 = 0;
                    let lo = self.e064 as u8; self.e064 = ((lo >> 2) as u16) << 8 | lo as u16;
                } else {
                    let lo = (self.e064 as u8).wrapping_sub(1);
                    if (lo as i8) >= 0 { self.e064 = ((lo >> 2) as u16) << 8 | lo as u16; }
                    else { self.e064 = 0; if self.e060 == 0x0C { self.reset_all(); } }
                }
            }
            13 => match d1 {
                0 => { self.e06c = 7; self.e032 = 0x0EA8; self.e064 = 0xFC; self.e066 = 0x90; self.e062 += 1; }
                1 => {
                    self.e064 = self.e064.wrapping_sub(0x13); self.e066 = self.e066.wrapping_add(0x13);
                    if (self.e066 as i16) >= 0x100 { self.e064 = 0x88; self.e066 = 0x100; self.e06e = 1; self.e062 += 1; }
                }
                _ => {}
            },
            14 => match d1 {
                0 => { self.e06c = 8; self.e032 = 0x0EA8; self.e064 = 0x8C; self.e066 = 0x100; self.e062 += 1; }
                1 => {
                    self.e064 = self.e064.wrapping_add(0x13); self.e066 = self.e066.wrapping_sub(0x13);
                    if (self.e066 as i16) < 0x90 { self.e066 = 0x90; self.e062 += 1; }
                }
                _ => { self.reset_a(); }
            },
            15 => if d1 == 0 { self.e06c = 9; self.e032 = 0x0EFA; self.e064 = 0x100; self.e062 += 1; },
            16 => match d1 {
                0 => { self.e06c = 10; self.e032 = 0x0F4C; self.e064 = 0x64; self.e062 += 1; }
                1 => { self.e064 += 3; if self.e064 >= 0x75 { self.e064 = 0x75; self.e032 = 0x0FA2; self.e062 += 1; } }
                2 => { self.e064 = 0; self.e062 += 1; }
                3 => { self.e064 += 10; if self.e064 >= 0x70 { self.e064 = 0x70; self.e062 += 1; } }
                4 => { self.e06e = 1; self.e062 += 1; }
                _ => {}
            },
            17 => match d1 {
                0 => { self.e06c = 11; self.e032 = 0x1016; self.e068 = 0x75; self.e064 = 0x70; self.e066 = 0x140; self.e062 += 1; }
                1 => { self.e068 = self.e068.wrapping_sub(3); self.e066 += 3; if (self.e068 as i16) < 0x64 { self.e068 = 0x65; self.e062 += 1; } }
                2 => { self.e064 = 0x70; self.e062 += 1; }
                3 => { self.e032 = 0x1092; self.e062 += 1; }
                4 => { let v = self.e064 as i16 - 10; if v < 0 { self.e064 = 0; self.e062 += 1; } else { self.e064 = v as u16; } }
                _ => { self.reset_a(); }
            },
            18 => match d1 {
                0 => { self.e06c = 12; self.e032 = 0x10D4; self.e064 = 0x90; self.e062 += 1; }
                1 => { self.e064 += 0x13; if self.e064 >= 0xF0 { self.e064 = 0xF0; self.e06e = 1; self.e062 += 1; } }
                _ => {}
            },
            19 => match d1 {
                0 => { self.e06c = 13; self.e032 = 0x10D4; self.e064 = 0xF0; self.e062 += 1; }
                1 => { self.e064 = self.e064.wrapping_sub(0x13); if (self.e064 as i16) < 0x90 { self.e064 = 0x90; self.e062 += 1; } }
                _ => { self.reset_a(); }
            },
            20 => if d1 == 0 { self.e06c = 14; self.e032 = 0x10D4; self.e064 = 0xF0; self.e062 += 1; },
            21 => if d1 == 0 { self.e06c = 15; self.e032 = 0x1114; self.e064 = 0x148; self.e062 += 1; },
            22 => if d1 == 0 { self.e06c = 16; self.e032 = 0x1154; self.e064 = 0xE8; self.e062 += 1; },
            23 => match self.e066 { // step counter is $E066 here (M:0xFF3B3E)
                0 => { self.e068 = 0x30; self.e066 += 1; }
                1 => {
                    self.e068 += 8;
                    if self.e068 >= 0x60 { self.e068 = 0x60; self.e066 += 1; }
                    self.e06c = 17; self.e032 = 0x1194;
                }
                _ => { self.e06c = 17; self.e032 = 0x1194; }
            },
            24 => match d1 {
                0 => { self.e06c = 18; self.e032 = 0x11F4; self.e064 = 0xFFC0; self.e062 += 1; }
                1 => { let v = (self.e064 as i16).wrapping_add(8); if v >= 0 { self.e064 = 0; self.e062 += 1; } else { self.e064 = v as u16; } }
                _ => { self.e06e = 0; self.e06c = 0; self.e066 = 0; self.e068 = 0; self.e062 = 0; self.e064 = 0; self.e060 = 0; self.e061 = 0; }
            },
            25 => if d1 == 0 { self.e06c = 19; self.e032 = 0x11F4; self.e064 = 0x60; self.e062 += 1; },
            26 | 29 => match d1 {
                0 => { self.e06c = if n == 26 { 20 } else { 23 }; self.e032 = if n == 26 { 0x11F4 } else { 0x123A }; self.e064 = 0x60; self.e062 += 1; }
                1 => { self.e064 = self.e064.wrapping_sub(8); if (self.e064 as i16) < 0x30 { self.e064 = 0x30; self.e062 += 1; } }
                _ => { self.eb00 = (0, 0); self.e06a = 0; self.reset_a(); }
            },
            27 => if d1 == 0 { self.e06c = 21; self.e032 = 0x11F4; self.e064 = 0xFFC0; self.e062 += 1; },
            28 => if d1 == 0 { self.e06c = 22; self.e032 = 0x123A; self.e064 = 0x60; self.e062 += 1; },
            _ => {}
        }
    }

    /// One VBlank: H-INT setup for this frame (M:0xFF08E4/0xFF0AD4), then the scroll update
    /// (M:0xFF3CAA) and the effects step; returns the raster program of the displayed frame.
    /// `sub_scroll` = COMSTAT3..6 (A.x, A.y, B.x, B.y) written by the sub CPU.
    pub fn vblank(&mut self, sub_scroll: [i16; 4]) -> Raster {
        let mut hint_n: Option<u16> = None; // reg 0x8A value when H-INT is enabled this frame
        let mut vs = self.eb00;
        if self.e06c != 0 || self.e036 != 0 {
            let mode = self.e06c;
            let mut enable = true;
            let n: u16 = match mode {
                0 => { self.e032 = 0x0C4A; 0 }
                1 => {
                    self.eb00 = (0, 0);
                    self.e064 = self.e064.wrapping_sub(1);
                    if self.e064 == 0 || (self.e064 as i16) < 0 { enable = false; }
                    else if (self.e064 as i16) >= 2 { self.eb00 = (0x30, 0x30); } else { self.eb00 = (0x10, 0x10); }
                    15
                }
                2..=6 | 12..=16 => 7,
                7 | 8 | 10 | 11 => 0,
                9 => 7,
                17 => { self.eb00 = (0, self.e068); 7 }
                18..=21 => { self.eb00 = (0, self.e064); 7 }
                22 | 23 => { self.eb00 = (0, self.e064); 0 }
                _ => 7,
            };
            if matches!(mode, 7..=16) { vs = self.eb00; }
            if enable { hint_n = Some(n); }
        }
        // scroll_update: VSRAM = -COMSTAT4/6 when sub-driven ($E076), else $EB00.
        if self.e078 == 0 {
            if self.e076 != 0 {
                self.eb00 = ((-sub_scroll[1]) as u16 & 0x3FF, (-sub_scroll[3]) as u16 & 0x3FF);
                self.e700 = (sub_scroll[0] as u16 & 0x3FF, sub_scroll[2] as u16 & 0x3FF);
            }
            vs = self.eb00;
        }
        // HYPOTHESIS (docs/re/modules.md §3.2): the display bit left cleared by the last band frame of
        // fx 8–12 (M:0xFF0E14) is only meaningful while an H-INT mode runs; once no H-INT mode is
        // active the screen is on again. The original IP code never sets it (only fx 7's handler end
        // M:0xFF0D32, B6406 requests and the boot code do) and the oracle stays black for the whole intro
        // after fx 12, although the intro obviously displays pictures (forced-on oracle run).
        let mut st = LineState { vscroll_a: vs.0 as i16, vscroll_b: vs.1 as i16, display: !self.disp_off_carry || hint_n.is_none() };
        let mut lines = vec![st; 224];
        let mut pal_split = None;
        if let Some(n) = hint_n {
            let mut k: u16 = 0; // $E034
            let mut line = n as usize; // first H-INT at end of line n
            let mut active = true;
            while line < 224 && active {
                let next = line + 1;
                let set = |st: &mut LineState, a: Option<u16>, b: Option<u16>, d: Option<bool>| {
                    if let Some(a) = a { st.vscroll_a = a as i16; }
                    if let Some(b) = b { st.vscroll_b = b as i16; }
                    if let Some(d) = d { st.display = d; }
                };
                let neg = |v: u16| (v as i16).wrapping_neg() as u16 & 0x3FF;
                match self.e032 {
                    0x0C4A => if k == 0x88 { pal_split = Some(next as u16); active = false; },
                    0x0CCE => {
                        let e = self.e064 as i16;
                        if e <= 0 { set(&mut st, None, None, Some(true)); active = false; }
                        else if e >= 2 {
                            match k { 7 => set(&mut st, Some(0x3D0), None, None), 10 => { set(&mut st, None, None, Some(true)); active = false; }
                                      4 => set(&mut st, None, None, Some(false)), _ => {} }
                        } else {
                            match k { 7 => set(&mut st, Some(0x3F0), None, None), 8 => { set(&mut st, None, None, Some(true)); active = false; }
                                      6 => set(&mut st, None, None, Some(false)), _ => {} }
                        }
                    }
                    0x0D6E => {
                        // hintc_sub0..4 (M:0xFF0DB0…): mode 2: k0 on+(-v), k7 +v, k8 off;
                        // modes 3..6: k0 on, k(9+3(m-3)) -v, +1 → +v, +2 → display off.
                        let (minus, plus, off) = if self.e06c == 2 { (0u16, 7u16, 8u16) } else {
                            let m = 9 + 3 * (self.e06c - 3); (m, m + 1, m + 2) };
                        let v = self.e064 >> 8; // byte $E064
                        if k == 0 { set(&mut st, None, None, Some(true)); if self.e06c == 2 { set(&mut st, Some(neg(v)), None, None); } }
                        if k == minus && self.e06c != 2 { set(&mut st, Some(neg(v)), None, None); }
                        if k == plus { set(&mut st, Some(v), None, None); }
                        if k == off { set(&mut st, None, None, Some(false)); active = false; }
                    }
                    0x0EA8 => if k == self.e064 { set(&mut st, Some(self.e066), Some(self.e066), None); active = false; },
                    0x0EFA => if k == 0x11 { set(&mut st, Some(self.e064), Some(self.e064), None); active = false; },
                    0x0F4C => {
                        if k == 0x63 { set(&mut st, Some(0x140), Some(0x140), None); }
                        else if k == self.e064 { set(&mut st, Some(self.eb00.0), Some(self.eb00.1), None); active = false; }
                    }
                    0x0FA2 => match k {
                        0x63 => set(&mut st, Some(0x140), Some(0x140), None),
                        0x8F => { set(&mut st, Some(self.e064), Some(self.e064), None); active = false; }
                        0x75 => set(&mut st, Some(self.eb00.0), Some(self.eb00.1), None),
                        _ => {}
                    },
                    0x1016 => {
                        if k == 0x63 { set(&mut st, Some(self.e066), Some(self.e066), None); }
                        else if k == 0x8F { set(&mut st, Some(0x70), Some(0x70), None); active = false; }
                        else if k == self.e068 { set(&mut st, Some(self.eb00.0), Some(self.eb00.1), None); }
                    }
                    0x1092 => if k == 0x8F { set(&mut st, Some(self.e064), Some(self.e064), None); active = false; },
                    0x10D4 => if k == 0x13 { set(&mut st, Some(self.e064), None, None); active = false; },
                    0x1114 => if k == 0x08 { set(&mut st, Some(self.e064), None, None); active = false; },
                    0x1154 => if k == 0x14 { set(&mut st, Some(self.e064), None, None); active = false; },
                    0x1194 => {
                        if k == 9 { set(&mut st, Some(0), Some(neg(self.e068)), None); }
                        else if k == 0x11 { set(&mut st, Some(self.eb00.0), Some(self.eb00.1), None); active = false; }
                    }
                    0x11F4 => if k == 9 { set(&mut st, Some(0), Some(neg(self.e064)), None); active = false; },
                    0x123A => {
                        if k == 0x88 { pal_split = Some(next as u16); active = false; }
                        else if k == 0x48 { set(&mut st, Some(0), Some(neg(self.e064)), None); }
                    }
                    _ => { active = false; }
                }
                for l in next..224 { lines[l] = st; }
                k += 1;
                line += n as usize + 1;
            }
        }
        self.disp_off_carry = !st.display;
        let line_hscroll_b = if self.line_scroll {
            let mut v = vec![0i16; 224];
            for i in 0..=(self.e07a.min(127) as usize) { if 8 + i < 224 { v[8 + i] = self.wave[i] as i16; } }
            Some(v)
        } else { None };
        self.effects_step();
        self.frame = self.frame.wrapping_add(1);
        Raster { lines, pal_split, line_hscroll_b }
    }
}
