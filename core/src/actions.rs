//! Engine building blocks reimplemented from the sub-CPU "actions" layer of SUBCODE.BIN
//! (see docs/re/actions.md). Every function cites the original routine it mirrors.
//!
//! Addresses are sub-CPU addresses; SUBCODE.BIN is loaded at S:0xD400.
//! Nothing here contains game data: tables are read from the SUBCODE.BIN bytes at runtime.

pub const SUBCODE_BASE: u32 = 0xD400;
/// 1bpp 8x8 font, glyph = code - 0x20 (S:0x14B7C).
pub const FONT_ADDR: u32 = 0x14B7C;
/// Advance widths, one byte per glyph (S:0x14EAC).
pub const WIDTH_ADDR: u32 = 0x14EAC;
pub const GLYPH_COUNT: usize = ((WIDTH_ADDR - FONT_ADDR) / 8) as usize; // 102
/// Speaker-name table: 48 word offsets relative to the table (S:0x13552).
pub const NAMES_ADDR: u32 = 0x13552;
pub const NAME_COUNT: usize = 48;
/// Button-config table, 3 bytes per config: logical bit for physical A, B, C (S:0xE034).
pub const BUTTON_CFG_ADDR: u32 = 0xE034;
/// Text window bitmap size (32x8 tiles of 4bpp, S:0xBE000).
pub const WIN_W: usize = 256;
pub const WIN_H: usize = 64;

fn sc_at(_sc: &[u8], addr: u32) -> usize { (addr - SUBCODE_BASE) as usize }

// ---------------------------------------------------------------- font / names

pub struct Font {
    pub glyphs: Vec<[u8; 8]>,
    pub widths: Vec<u8>,
}

impl Font {
    pub fn from_subcode(sc: &[u8]) -> Font {
        let f = sc_at(sc, FONT_ADDR);
        let w = sc_at(sc, WIDTH_ADDR);
        let glyphs = (0..GLYPH_COUNT).map(|g| {
            let mut a = [0u8; 8];
            a.copy_from_slice(&sc[f + g * 8..f + g * 8 + 8]);
            a
        }).collect();
        Font { glyphs, widths: sc[w..w + GLYPH_COUNT].to_vec() }
    }
}

/// Speaker names (S:0x13552): returns (colour, text bytes) per index; index 0 is empty.
/// First byte: bit7 set → colour 3 and the byte is part of the text, else colour = b&7 (skipped).
pub fn speaker_names(sc: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let t = sc_at(sc, NAMES_ADDR);
    (0..NAME_COUNT).map(|i| {
        let o = i16::from_be_bytes([sc[t + 2 * i], sc[t + 2 * i + 1]]) as isize;
        let mut p = (t as isize + o) as usize;
        let c = sc[p];
        let col = if c & 0x80 != 0 { 3 } else { c & 7 };
        if c & 0x80 == 0 { p += 1; }
        let end = sc[p..].iter().position(|&b| b == 0xFF).unwrap_or(0);
        (col, sc[p..p + end.min(0x19)].to_vec())
    }).collect()
}

// ---------------------------------------------------------------- glyph renderer

/// The text-window glyph renderer (`txt_tick` S:0x13BC8) and its 256x64 4bpp bitmap.
/// `pix` holds one palette index (0..15) per pixel, row-major.
pub struct TextWindow {
    pub pix: Vec<u8>,
    pub busy: bool,      // $9720
    pub paused: bool,    // $972C (set by FA)
    pub counter: u8,     // $9721
    pub delay: u8,       // $9722
    pub colour: u8,      // $9723
    pub x: u8,           // $972A
    pub y: u8,           // $972B
    pub fixed: bool,     // $9728
    pub dirty_rows: u8,  // $9729 (bit per tile row, informative)
    stream: Vec<u8>,
    pos: usize,
}

impl Default for TextWindow { fn default() -> Self { Self::new() } }

impl TextWindow {
    pub fn new() -> Self {
        let mut w = TextWindow { pix: vec![0; WIN_W * WIN_H], busy: false, paused: false, counter: 0,
            delay: 0, colour: 0, x: 0, y: 0, fixed: false, dirty_rows: 0, stream: Vec::new(), pos: 0 };
        w.clear();
        w
    }
    /// `txt_clear` S:0x13D84: bitmap to 0, last tile (cell 255) filled with colour 7.
    pub fn clear(&mut self) {
        self.pix.iter_mut().for_each(|p| *p = 0);
        for y in 56..64 { for x in 248..256 { self.pix[y * WIN_W + x] = 7; } }
        self.busy = false;
    }
    /// `sub_013DD0` / sequencer cmd 05: start a render stream (counter and delay reset to 0).
    pub fn start(&mut self, stream: &[u8]) {
        self.stream = stream.to_vec();
        self.pos = 0;
        self.delay = 0;
        self.counter = 0;
        self.busy = true;
    }
    /// Force "fast text" (message box when confirm is held: `$9722 = 0`).
    pub fn set_fast(&mut self) { self.delay = 0; }
    fn byte(&mut self) -> u8 { let b = self.stream.get(self.pos).copied().unwrap_or(0xFF); self.pos += 1; b }

    fn glyph(&mut self, font: &Font, c: u8) {
        let g = c.wrapping_sub(0x20) as usize;
        // sub_013E46: in fixed-pitch mode '1' and 'I' are shifted 1 px right
        let x0 = self.x as usize + if self.fixed && (g == 0x11 || g == 0x29) { 1 } else { 0 };
        let y0 = self.y as usize;
        self.dirty_rows |= 1 << ((y0 >> 3) & 7);
        self.dirty_rows |= 1 << (((y0 + 7) >> 3) & 7);
        if let Some(bits) = font.glyphs.get(g) {
            for (r, &row) in bits.iter().enumerate() {
                for k in 0..8 {
                    if row & (0x80 >> k) != 0 {
                        let (px, py) = (x0 + k, y0 + r);
                        if px < WIN_W && py < WIN_H { self.pix[py * WIN_W + px] = self.colour & 15; }
                    }
                }
            }
        }
        // sub_013F86
        let adv = if self.fixed { 9 } else { font.widths.get(g).copied().unwrap_or(8) };
        self.x = self.x.wrapping_add(adv);
    }

    /// One frame of `txt_tick` (S:0x13BC8).
    pub fn tick(&mut self, font: &Font) {
        self.dirty_rows = 0;
        if self.paused || !self.busy { return; }
        'outer: loop {
            // L_be8
            if self.counter != 0 { self.counter -= 1; return; }
            loop {
                // L_bfa
                self.counter = self.delay;
                let c = self.byte();
                match c {
                    0xFF => { self.busy = false; return; }
                    0xFE => { self.colour = self.byte(); }
                    0xFD => {
                        self.y = self.y.wrapping_add(9); self.x = 0x18;
                        self.pos += 1; // shares the 00-1F path: skips one byte
                    }
                    0xFC => { let n = self.byte(); self.delay = n; self.counter = n; continue 'outer; }
                    0xF9 => { self.pos += 1; }
                    0xFB => { self.x = self.byte(); self.y = self.byte(); }
                    0xFA => { self.pos += 2; self.paused = true; return; }
                    0xF8 => { self.pos += 2; }
                    0xF7 => { self.fixed = self.byte() != 0; }
                    0x00..=0x1F => { self.pos += 1; }
                    _ => { self.glyph(font, c); continue 'outer; }
                }
            }
        }
    }
}

// ---------------------------------------------------------------- message box

/// Script-text control codes (txt_build_line S:0x13AC0).
pub const TXT_END: u8 = 0xFF;
pub const TXT_EOL: u8 = 0xF2;
pub const TXT_PAGE: u8 = 0xF6;
pub const TXT_KEEP_ANIM: u8 = 0xF4;
pub const LINE_MAX_CHARS: usize = 0x32;

/// Effects the message box asks the host to perform (they map to `subfn_13` / sounds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgEvent {
    None,
    /// `subfn_13(n)`: main-CPU window/screen effect (0x0F open, 0x15 narration, 0xFE page advance…)
    ScreenCmd(u8),
    /// Window must be cleared (`txt_window_init`).
    ClearWindow,
    /// Finished; the sequencer advances past command 08.
    Done,
}

/// Sequencer command 08 (S:0x135B2). `text` = script text area (file offset 0x3800..).
pub struct MessageBox {
    pub state: u8,       // $9740 (9 = start)
    pub speaker: i16,    // $9744: -1 none, 0 narration, n name index
    pub pos: usize,      // $9746 (offset into text)
    pub colour: u8,      // $9745
    pub line_y: u8,      // $9741
    pub end: bool,       // $9749
    pub stop: u8,        // $974A
    pub keep_anim: bool, // $97CC hi = 2 (F4)
    pub name_y: u8,      // $971D (never written in SUBCODE → 0)
    pub wide_mode: u8,   // $7893 (portrait mode 0/1/2)
}

impl MessageBox {
    /// `spk_word` = first command argument (lo byte = speaker+1), `text_off` = second.
    pub fn new(spk_word: u16, text_off: u16, wide_mode: u8) -> Self {
        MessageBox { state: 9, speaker: (spk_word & 0xFF) as i16 - 1, pos: text_off as usize, colour: 1, line_y: 0,
            end: false, stop: 0, keep_anim: spk_word >> 8 == 2, name_y: 0, wide_mode }
    }
    fn open_cmd(&self) -> u8 { match self.wide_mode { 1 => 0x14, 2 => 0x16, _ => 0x0F } }

    /// Build one line's render stream (`txt_build_line` with the header of S:0x13A0E).
    pub fn build_line(&mut self, text: &[u8]) -> Vec<u8> {
        let mut s = vec![0xFC, 0x01, 0xF9, 0x01, 0xFE, self.colour, 0xFB,
            if self.speaker == 0 { 0x28 } else { 0x18 }, self.line_y];
        self.stop = 0;
        let mut n = 0usize;
        while n < LINE_MAX_CHARS {
            let c = text.get(self.pos).copied().unwrap_or(TXT_END);
            self.pos += 1;
            match c {
                TXT_END => { self.end = true; break; }
                TXT_EOL => { self.stop = 1; break; }
                TXT_PAGE => { self.stop = 3; break; }
                TXT_KEEP_ANIM => { self.keep_anim = true; }
                0xEC | 0xEE | 0xF0 => { s.push(c); n += 1; } // host substitutes button letters
                0xF8..=0xFE => { self.colour = c & 7; s.push(0xFE); s.push(self.colour); }
                _ => { s.push(c); n += 1; }
            }
        }
        s.push(0xFF);
        s
    }

    /// One frame. `confirm_pressed` = logical bit5 of $7991. Only call when `win.busy == false`
    /// (the original returns early while the glyph renderer is busy).
    pub fn tick(&mut self, win: &mut TextWindow, text: &[u8], names: &[(u8, Vec<u8>)], confirm_pressed: bool) -> MsgEvent {
        if win.busy { return MsgEvent::None; }
        match self.state {
            9 => { self.state = 10; MsgEvent::None } // effect task S:0xECB6 (window open anim)
            10 => { win.clear(); self.colour = 1; self.end = false; self.state = 2; MsgEvent::ScreenCmd(self.open_cmd()) }
            2 => {
                self.state = 3;
                if self.speaker == 0 { self.line_y = self.name_y; return MsgEvent::ScreenCmd(0x15); }
                if self.speaker < 0 { self.line_y = self.name_y; return MsgEvent::None; }
                if let Some((col, name)) = names.get(self.speaker as usize) {
                    let mut s = vec![0xFC, 0x00, 0xF9, 0x01, 0xFB, 0x10, self.name_y, 0xFE, *col];
                    s.extend_from_slice(name);
                    s.push(0xFF);
                    win.start(&s);
                }
                self.line_y = self.name_y + 9;
                MsgEvent::None
            }
            3 => {
                let s = self.build_line(text);
                win.start(&s);
                // sub_013A90
                if self.end || self.stop == 3 { self.state = 4; } else {
                    self.line_y += 9;
                    if self.line_y >= 0x39 { self.state = 4; }
                }
                MsgEvent::None
            }
            4 => { self.state = 5; MsgEvent::None }
            5 => {
                if !confirm_pressed { return MsgEvent::None; }
                self.state = if self.end { 6 } else { 7 };
                MsgEvent::ScreenCmd(0xFE)
            }
            7 => { win.clear(); self.state = 8; MsgEvent::ClearWindow }
            8 => { self.state = 2; MsgEvent::ScreenCmd(self.open_cmd()) }
            _ => MsgEvent::Done,
        }
    }
}

// ---------------------------------------------------------------- input

/// Button bits as sent by the main CPU in COMCMD0 (M:0xFF1298): S A C B R L D U.
pub mod btn {
    pub const UP: u8 = 0x01; pub const DOWN: u8 = 0x02; pub const LEFT: u8 = 0x04; pub const RIGHT: u8 = 0x08;
    pub const B: u8 = 0x10; pub const C: u8 = 0x20; pub const A: u8 = 0x40; pub const START: u8 = 0x80;
    /// Logical bits after remapping (S:0xDFF6): confirm = bit5, cancel = bit4, extra = bit6.
    pub const L_CANCEL: u8 = 0x10; pub const L_CONFIRM: u8 = 0x20; pub const L_EXTRA: u8 = 0x40;
}

/// Input pipeline of the sub CPU (S:0xD7B2, S:0xDF86, S:0xDFD6).
pub struct Input {
    pub config: usize,          // $78BA (0..5)
    pub cfg_table: [[u8; 3]; 6],
    prev_held: u8,
    counters: [u8; 8],          // $7994
    pub held: u8,               // $7990 (logical)
    pub pressed: u8,            // $7991
    pub repeat: u8,             // $7992
}

impl Input {
    pub fn from_subcode(sc: &[u8]) -> Self {
        let t = sc_at(sc, BUTTON_CFG_ADDR);
        let mut cfg = [[0u8; 3]; 6];
        for (i, c) in cfg.iter_mut().enumerate() { c.copy_from_slice(&sc[t + 3 * i..t + 3 * i + 3]); }
        Input { config: 0, cfg_table: cfg, prev_held: 0, counters: [0; 8], held: 0, pressed: 0, repeat: 0 }
    }
    /// Main-CPU pad cleanup: opposite directions (M:0xFF12D2).
    pub fn clean(raw: u8) -> u8 {
        let mut d = raw;
        if d & 3 == 3 { d &= !1; }
        if d & 0xC == 0xC { d &= !4; }
        d
    }
    fn remap(&self, v: u8) -> u8 {
        let c = self.cfg_table[self.config.min(5)];
        let mut o = v & 0x8F;
        if v & 0x40 != 0 { o |= 1 << c[0]; }
        if v & 0x10 != 0 { o |= 1 << c[1]; }
        if v & 0x20 != 0 { o |= 1 << c[2]; }
        o
    }
    /// Call once per frame with the physical held state (S A C B R L D U).
    pub fn update(&mut self, raw_held: u8) {
        let held = Self::clean(raw_held);
        let pressed = held & !self.prev_held;
        self.prev_held = held;
        let mut rep = 0u8;
        for i in 0..8 {
            let bit = 1u8 << i;
            if pressed & bit != 0 { self.counters[i] = 0x20; rep |= bit; }
            else if held & bit != 0 {
                self.counters[i] = self.counters[i].wrapping_sub(1);
                if self.counters[i] == 0 { self.counters[i] = 4; rep |= bit; }
            } else { self.counters[i] = 0; }
        }
        self.held = self.remap(held);
        self.pressed = self.remap(pressed);
        self.repeat = self.remap(rep);
    }
    /// Letter printed for text codes EC/EE/F0 (logical bit 6/4/5) under the current config.
    pub fn button_letter(&self, code: u8) -> u8 {
        let want = match code { 0xEC => 6, 0xEE => 4, _ => 5 };
        let c = self.cfg_table[self.config.min(5)];
        if c[0] == want { b'A' } else if c[1] == want { b'B' } else { b'C' }
    }
}

// ---------------------------------------------------------------- RNG

/// `rand16` S:0xFD96, state bytes $786A..$786D.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rng { pub s: [u8; 4] }

impl Rng {
    fn w(&self, i: usize) -> u16 { u16::from_be_bytes([self.s[i], self.s[i + 1]]) }
    fn set_w(&mut self, i: usize, v: u16) { let b = v.to_be_bytes(); self.s[i] = b[0]; self.s[i + 1] = b[1]; }
    pub fn next(&mut self) -> u16 {
        self.s[0] = self.s[0].wrapping_add(self.s[1]);              // add.b $786B,$786A
        let d0 = self.w(2);
        self.set_w(0, self.w(0).wrapping_add(d0));                  // add.w $786C,$786A
        let d0 = (d0 << 8) | self.s[2] as u16;                      // asl.w #8 / or.b $786C
        self.set_w(0, self.w(0).wrapping_add(d0));
        let (v, c) = self.s[2].overflowing_add(self.s[3]);          // add.b $786D,$786C
        self.s[2] = v;
        if c { self.set_w(0, self.w(0).wrapping_add(1)); }
        let (v, c) = self.w(2).overflowing_add(1);                  // addq.w #1,$786C
        self.set_w(2, v);
        if c { self.set_w(0, self.w(0).wrapping_add(1)); }
        self.w(0)
    }
    /// `sub_00FD8C`: d3 + (rand * d4) >> 16.
    pub fn range(&mut self, base: u16, span: u16) -> u16 {
        let r = self.next() as u32;
        ((r * span as u32) >> 16) as u16 + base
    }
}

// ---------------------------------------------------------------- menu stacks

/// `[cap][count][u16 items]` stacks used by the menu machinery (S:0x107DC..0x10840).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmallStack { pub cap: u8, pub items: Vec<u16> }

impl SmallStack {
    pub fn new(cap: u8) -> Self { SmallStack { cap, items: Vec::new() } }
    pub fn get(&self, i: usize) -> Option<u16> { self.items.get(i).copied() }
    pub fn top(&self) -> Option<u16> { self.items.last().copied() }
    /// sub_010806 (fails silently when full)
    pub fn push(&mut self, v: u16) -> bool { if self.items.len() < self.cap as usize { self.items.push(v); true } else { false } }
    /// sub_01071E: push, dropping the oldest when full (FIFO history)
    pub fn push_fifo(&mut self, v: u16) { if self.items.len() >= self.cap as usize && !self.items.is_empty() { self.items.remove(0); } self.items.push(v); }
    pub fn pop(&mut self) -> Option<u16> { self.items.pop() }
    pub fn truncate(&mut self, n: usize) { self.items.truncate(n); }
    pub fn clear(&mut self) { self.items.clear(); }
    pub fn contains(&self, v: u16) -> bool { self.items.contains(&v) }
    pub fn count(&self) -> usize { self.items.len() }
}

// ---------------------------------------------------------------- text-list menu

/// One visible line of the text-list menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuLine {
    /// index into the collected items ($9852)
    Item(usize),
    /// scroll marker line: render stream S:0x10EFA (up, page 2) or S:0x10F10/0x10F26 (down, page 1)
    Arrow { down: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuResult {
    /// 1-based item number (`$79F4`)
    Selected(u8),
    /// `$79F4 = FF` (only allowed when depth > 0)
    Cancel,
}

/// Vertical text-list command menu (`menu_show` S:0x10CBE, `menu_input` S:0x11004,
/// key reader S:0x110B0, navigation S:0x11150).
pub struct TextMenu {
    pub count: usize,     // $9853
    pub depth: usize,     // $9893
    pub page: u8,         // $9715: 0 no paging, 1 first page, 2 second page
    pub cursor: u8,       // $9710 (line)
    pub lines: u8,        // $9712
    pub wide: bool,       // $7893 != 0
    k: [u8; 5],           // S:0x10CB8..0x10CBC: page1 items, page2 offset, wrap-up line, page2 last+1, max lines
    key_state: u8,        // $9713
    key_t0: u16,          // $9714
}

impl TextMenu {
    /// `remembered` = `$9716` (cursor remembered for the top level). Returns the menu and the
    /// value to store back into `$9716` (page-2 adjustment).
    pub fn new(count: usize, depth: usize, remembered: u8, wide: bool) -> (TextMenu, u8) {
        let k = if wide { [5, 4, 3, 4, 6] } else { [6, 5, 2, 3, 7] };
        let mut m = TextMenu { count, depth, page: 0, cursor: 0, lines: 0, wide, k, key_state: 0, key_t0: 0 };
        let mut rem = remembered;
        if count == 8 || (wide && count == 7) {
            m.page = 1;
            if depth == 0 && rem >= k[0] { m.page = 2; rem -= k[1]; }
        }
        let mut n = 0u8;
        while n < k[4] && m.line(n).is_some() { n += 1; }
        m.lines = if m.page == 2 { k[4] } else { n };
        m.cursor = if depth != 0 { 0 } else if n.wrapping_sub(1) < rem { 0 } else { rem };
        (m, rem)
    }

    /// `sub_010E5A`: what is drawn on line `l` (None = past the last item).
    pub fn line(&self, l: u8) -> Option<MenuLine> {
        let idx = match self.page {
            1 => { if l == self.k[0] { return Some(MenuLine::Arrow { down: true }); } l as usize }
            2 => { let d = l.wrapping_add(self.k[1]); if d == self.k[1] { return Some(MenuLine::Arrow { down: false }); } d as usize }
            _ => l as usize,
        };
        if idx < self.count { Some(MenuLine::Item(idx)) } else { None }
    }

    /// Visible lines for rendering: line i at text-window Y = i*9, X = 0x10; the line under the
    /// cursor uses colour 1, the others colour 4. Cursor sprite Y = 0x8F + 9*cursor (+0x10 wide).
    pub fn visible(&self) -> Vec<MenuLine> {
        (0..self.k[4]).map_while(|l| self.line(l)).collect()
    }

    /// Key reader S:0x110B0. `frame` = `$7848`, `style` = `$79FE`.
    fn keys(&mut self, pressed: u8, held: u8, repeat: u8, frame: u16, style: u16) -> u8 {
        if pressed != 0 { self.key_state = 0; return pressed; }
        let f = frame as u8;
        let t = self.key_t0 as u8;
        match self.key_state {
            0 => {
                self.key_t0 = frame;
                if held == 0 { return 0; }
                self.key_state = 1;
                self.key_t0 = frame.wrapping_sub(1);
                if (f ^ self.key_t0 as u8) & 0x1F != 0 { return 0; }
                self.key_state = 2;
                held
            }
            1 => { if (f ^ t) & 0x1F != 0 { return 0; } self.key_state = 2; held }
            _ => {
                if style == 3 { return repeat; }
                let mask = if style >= 2 { 7 } else { 0x0F };
                if (f ^ t) & mask != 0 { 0 } else { held }
            }
        }
    }

    fn flip_page(&mut self) { self.page ^= 3; }

    /// One frame of input. Returns Some(result) when the menu closes.
    pub fn input(&mut self, pressed: u8, held: u8, repeat: u8, frame: u16) -> Option<MenuResult> {
        let d = self.keys(pressed, held, repeat, frame, 0);
        if self.count != 1 {
            match d {
                1 | 4 => { self.up(); return None; }
                2 | 8 => { self.down(); return None; }
                _ => {}
            }
        }
        if pressed == btn::L_CANCEL { return if self.depth != 0 { Some(MenuResult::Cancel) } else { None }; }
        if pressed == btn::L_CONFIRM {
            if self.page == 1 && self.cursor == self.k[0] { return None; }
            if self.page == 2 && self.cursor == 0 { return None; }
            let mut r = self.cursor + 1;
            if self.page == 2 { r += self.k[1]; }
            return Some(MenuResult::Selected(r));
        }
        None
    }

    fn up(&mut self) {
        loop {
            let c = self.cursor as i16 - 1;
            if c == 0 && self.page == 2 { self.cursor = 0; continue; }
            if c >= 0 { self.cursor = c as u8; return; }
            self.cursor = self.lines - 1;
            if self.page == 0 { return; }
            self.cursor -= 1;
            if self.page == 1 { self.cursor = self.k[2]; }
            self.flip_page();
            return;
        }
    }

    fn down(&mut self) {
        loop {
            self.cursor += 1;
            let d0 = self.cursor + 1;
            if d0 == self.lines && self.page == 1 { continue; }
            let wrap = self.cursor == self.lines || (self.page == 2 && self.cursor == self.k[3]);
            if wrap {
                self.cursor = 0;
                if self.page != 0 {
                    if self.page == 1 { self.cursor = 1; }
                    self.flip_page();
                }
            }
            return;
        }
    }
}

// ---------------------------------------------------------------- save layout

/// Offsets inside the 0x1C0-byte save block (S:$9C00, built by S:0x14F12).
pub mod save {
    pub const SIZE: usize = 0x1C0;
    pub const COUNTER: usize = 0x00;
    pub const PLAYTIME: usize = 0x02;
    pub const OPTIONS: usize = 0x0A;
    pub const SCRIPT_FILE: usize = 0x0C;
    pub const RESUME_PC: usize = 0x0E;
    pub const HISTORY: usize = 0x10;
    pub const SCENE: usize = 0x18;
    pub const PCM_LOOP: usize = 0x1A;
    pub const VOICE: usize = 0x1C;
    pub const MODES: usize = 0x1D;
    pub const OBJECTS: usize = 0x20;
    pub const VARS: usize = 0x60;
    pub const VARS_LEN: usize = 0x160;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_font() -> Font {
        // every glyph: a single pixel row 0x80 in row 0, width 5
        Font { glyphs: vec![[0x80, 0, 0, 0, 0, 0, 0, 0]; GLYPH_COUNT], widths: vec![5; GLYPH_COUNT] }
    }

    #[test]
    fn typewriter_one_glyph_per_frame() {
        let f = test_font();
        let mut w = TextWindow::new();
        w.start(&[0xFC, 1, 0xFE, 1, 0xFB, 0, 0, b'A', b'B', b'C', 0xFF]);
        w.tick(&f); // FC 01 consumes the first frame
        assert_eq!(w.x, 0);
        w.tick(&f); assert_eq!(w.x, 5);
        w.tick(&f); assert_eq!(w.x, 10);
        w.tick(&f); assert_eq!(w.x, 15);
        assert!(w.busy);
        w.tick(&f); assert!(!w.busy);
        assert_eq!(w.pix[0], 1);
        assert_eq!(w.pix[5], 1);
    }

    #[test]
    fn delay_zero_is_instant() {
        let f = test_font();
        let mut w = TextWindow::new();
        w.start(&[0xFC, 0, 0xFB, 0, 8, b'A', b'B', 0xFF]);
        w.tick(&f);
        assert!(!w.busy);
        assert_eq!(w.x, 10);
        assert_eq!(w.y, 8);
    }

    #[test]
    fn message_paging() {
        let f = test_font();
        let names = vec![(3u8, vec![]), (2u8, b"Bob".to_vec())];
        let mut text = Vec::new();
        for i in 0..8 { text.extend_from_slice(format!("line{i}").as_bytes()); text.push(0xF2); }
        text.push(0xFF);
        let mut m = MessageBox::new(0x0002, 0, 0); // speaker index 1
        let mut w = TextWindow::new();
        let mut pages = 0;
        for _ in 0..2000 {
            let ev = m.tick(&mut w, &text, &names, m.state == 5);
            w.tick(&f);
            if ev == MsgEvent::ClearWindow { pages += 1; }
            if ev == MsgEvent::Done { break; }
        }
        assert_eq!(m.state, 6);
        assert_eq!(pages, 1); // 8 lines with a name line: 6 + 2
    }

    #[test]
    fn input_repeat_and_remap() {
        let cfg = [[6, 4, 5], [6, 5, 4], [4, 6, 5], [4, 5, 6], [5, 4, 6], [5, 6, 4]];
        let mut i = Input { config: 0, cfg_table: cfg, prev_held: 0, counters: [0; 8], held: 0, pressed: 0, repeat: 0 };
        i.update(btn::C);
        assert_eq!(i.pressed, btn::L_CONFIRM);
        let mut reps = 0;
        for _ in 0..40 { i.update(btn::C); if i.repeat != 0 { reps += 1; } }
        assert_eq!(reps, 3); // after 32 frames, then every 4
        i.config = 3; // A→4 B→5 C→6
        i.update(0); i.update(btn::B);
        assert_eq!(i.pressed, btn::L_CONFIRM);
        assert_eq!(i.button_letter(0xF0), b'B');
        assert_eq!(Input::clean(0x0F), 0x0A);
    }

    #[test]
    fn rng_deterministic() {
        let mut a = Rng::default();
        let mut b = Rng::default();
        let xs: Vec<u16> = (0..16).map(|_| a.next()).collect();
        let ys: Vec<u16> = (0..16).map(|_| b.next()).collect();
        assert_eq!(xs, ys);
        assert!(xs.windows(2).any(|w| w[0] != w[1]));
        assert!(a.range(2, 3) >= 2);
    }

    #[test]
    fn menu_plain_wrap() {
        let (mut m, _) = TextMenu::new(3, 1, 0, false);
        assert_eq!(m.visible().len(), 3);
        assert_eq!(m.input(btn::UP, btn::UP, 0, 10), None);
        assert_eq!(m.cursor, 2);
        m.input(btn::DOWN, btn::DOWN, 0, 11);
        assert_eq!(m.cursor, 0);
        assert_eq!(m.input(btn::L_CONFIRM, 0, 0, 12), Some(MenuResult::Selected(1)));
        assert_eq!(m.input(btn::L_CANCEL, 0, 0, 13), Some(MenuResult::Cancel));
        let (mut top, _) = TextMenu::new(3, 0, 0, false);
        assert_eq!(top.input(btn::L_CANCEL, 0, 0, 13), None);
    }

    #[test]
    fn menu_paging_eight_items() {
        let (mut m, _) = TextMenu::new(8, 1, 0, false);
        assert_eq!(m.page, 1);
        let v = m.visible();
        assert_eq!(v.len(), 7);
        assert_eq!(v[6], MenuLine::Arrow { down: true });
        for _ in 0..5 { m.input(btn::DOWN, btn::DOWN, 0, 0); }
        assert_eq!((m.page, m.cursor), (1, 5));
        m.input(btn::DOWN, btn::DOWN, 0, 0);        // skips the arrow line → page 2, item 6
        assert_eq!((m.page, m.cursor), (2, 1));
        assert_eq!(m.visible(), vec![MenuLine::Arrow { down: false }, MenuLine::Item(6), MenuLine::Item(7)]);
        assert_eq!(m.input(btn::L_CONFIRM, 0, 0, 0), Some(MenuResult::Selected(7)));
        m.input(btn::DOWN, btn::DOWN, 0, 0);
        m.input(btn::DOWN, btn::DOWN, 0, 0);        // wraps to page 1, item 0
        assert_eq!((m.page, m.cursor), (1, 0));
        m.input(btn::UP, btn::UP, 0, 0);            // wraps to the last item (page 2, line 2)
        assert_eq!((m.page, m.cursor), (2, 2));
        m.input(btn::UP, btn::UP, 0, 0);
        m.input(btn::UP, btn::UP, 0, 0);            // from item 6 up to item 5 on page 1
        assert_eq!((m.page, m.cursor), (1, 5));
    }

    #[test]
    fn stacks() {
        let mut s = SmallStack::new(3);
        for v in 1..=4 { s.push_fifo(v); }
        assert_eq!(s.items, vec![2, 3, 4]);
        assert!(!s.push(9));
    }
}
