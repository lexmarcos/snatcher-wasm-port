//! Minimal Mega Drive VDP model used by the scene composer.
//!
//! The original game draws everything through the VDP (tiles in VRAM, plane
//! nametables, sprite table, CRAM palettes). Reproducing the VDP's *data model*
//! lets the engine translate the game's drawing routines almost 1:1 and gives a
//! pixel-exact composition, without emulating timing (no FIFO, no H-int raster
//! effects unless the game needs them).
//!
//! Semantics follow the public documentation (Plutiedev "VDP registers",
//! Sega Genesis Software Manual): control-port command words, auto-increment,
//! 68k→VRAM DMA, fill and copy; planes A/B (+window), per-tile priority,
//! sprites with link list, 9-bit BGR CRAM.
use crate::fb::{cram_to_rgba, Framebuffer};
use crate::{H, W};

#[derive(Clone)]
pub struct Vdp {
    pub vram: Vec<u8>,      // 64 KiB
    pub cram: [u16; 64],
    pub vsram: [u16; 40],
    pub regs: [u8; 24],
    // port state
    pending: Option<u16>,
    code: u8,
    addr: u32,
}

impl Default for Vdp { fn default() -> Self { Self::new() } }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target { Vram, Cram, Vsram, None }

impl Vdp {
    pub fn new() -> Self {
        Vdp { vram: vec![0; 0x10000], cram: [0; 64], vsram: [0; 40], regs: [0; 24], pending: None, code: 0, addr: 0 }
    }

    // ---------------- port interface ----------------
    /// Write a word to the control port (0xC00004).
    pub fn ctrl_w(&mut self, v: u16) {
        if let Some(first) = self.pending.take() {
            self.code = ((first >> 14) as u8 & 3) | (((v >> 2) as u8) & 0x3C);
            self.addr = (first as u32 & 0x3FFF) | ((v as u32 & 3) << 14);
            if self.code & 0x20 != 0 && self.regs[1] & 0x10 != 0 {
                self.dma_from_ctrl();
            }
            return;
        }
        if v & 0xC000 == 0x8000 {
            let r = ((v >> 8) & 0x1F) as usize;
            if r < 24 { self.regs[r] = v as u8 }
            return;
        }
        self.pending = Some(v);
    }
    /// Write a long to the control port (two words, high first).
    pub fn ctrl_l(&mut self, v: u32) { self.ctrl_w((v >> 16) as u16); self.ctrl_w(v as u16); }

    fn target(&self) -> Target {
        match self.code & 0x0F { 1 => Target::Vram, 3 => Target::Cram, 5 => Target::Vsram, _ => Target::None }
    }
    fn inc(&self) -> u32 { self.regs[15] as u32 }

    /// Write a word to the data port (0xC00000).
    pub fn data_w(&mut self, v: u16) {
        self.pending = None;
        if self.code & 0x20 != 0 && self.regs[23] & 0xC0 == 0x80 {
            // VRAM fill: first write sets data, then fill `len` bytes.
            self.code &= !0x20;
            self.fill(v);
            return;
        }
        self.write_target(v);
        self.addr = (self.addr + self.inc()) & 0xFFFF;
    }
    pub fn data_l(&mut self, v: u32) { self.data_w((v >> 16) as u16); self.data_w(v as u16); }

    fn write_target(&mut self, v: u16) {
        match self.target() {
            Target::Vram => {
                let a = (self.addr & 0xFFFE) as usize;
                let (hi, lo) = ((v >> 8) as u8, v as u8);
                // byte-swapped when the address is odd (documented behaviour)
                if self.addr & 1 != 0 { self.vram[a] = lo; self.vram[a + 1] = hi } else { self.vram[a] = hi; self.vram[a + 1] = lo }
            }
            Target::Cram => { self.cram[((self.addr >> 1) & 0x3F) as usize] = v & 0x0EEE }
            Target::Vsram => { let i = ((self.addr >> 1) as usize) % 40; self.vsram[i] = v & 0x07FF }
            Target::None => {}
        }
    }

    fn dma_len(&self) -> u32 {
        let l = self.regs[19] as u32 | ((self.regs[20] as u32) << 8);
        if l == 0 { 0x10000 } else { l }
    }

    fn dma_from_ctrl(&mut self) {
        match self.regs[23] & 0xC0 {
            0xC0 => {
                // VRAM copy (byte granularity)
                let mut src = (self.regs[21] as u32 | ((self.regs[22] as u32) << 8)) & 0xFFFF;
                for _ in 0..self.dma_len() {
                    let b = self.vram[src as usize];
                    self.vram[(self.addr & 0xFFFF) as usize] = b;
                    src = (src + 1) & 0xFFFF;
                    self.addr = (self.addr + self.inc()) & 0xFFFF;
                }
                self.code &= !0x20;
            }
            0x80 => { /* fill: waits for data port write */ }
            _ => { /* 68k→VDP: performed by `dma_68k` with the caller's memory */ }
        }
    }

    fn fill(&mut self, v: u16) {
        // Write the word normally first, then fill the high byte.
        self.write_target(v);
        let b = (v >> 8) as u8;
        for _ in 0..self.dma_len() {
            self.addr = (self.addr + self.inc()) & 0xFFFF;
            if self.target() == Target::Vram { self.vram[(self.addr ^ 1) as usize & 0xFFFF] = b }
        }
    }

    /// 68k→VDP DMA: source address/length from registers 19..23; `read16(addr)` supplies
    /// the source words (the caller maps main RAM / Word RAM). Must be called after the
    /// control word that started the DMA.
    pub fn dma_68k(&mut self, mut read16: impl FnMut(u32) -> u16) {
        let mut src = ((self.regs[21] as u32) | ((self.regs[22] as u32) << 8) | (((self.regs[23] & 0x7F) as u32) << 16)) << 1;
        for _ in 0..self.dma_len() {
            let v = read16(src);
            self.write_target(v);
            src = src.wrapping_add(2);
            self.addr = (self.addr + self.inc()) & 0xFFFF;
        }
        self.code &= !0x20;
    }

    /// Direct convenience writers (used when a game routine is translated at a higher level).
    pub fn write_vram(&mut self, addr: usize, data: &[u8]) {
        for (i, b) in data.iter().enumerate() { self.vram[(addr + i) & 0xFFFF] = *b }
    }
    pub fn set_reg(&mut self, r: usize, v: u8) { self.regs[r] = v }

    // ---------------- rendering ----------------
    fn plane_size(&self) -> (usize, usize) {
        let s = |v: u8| match v & 3 { 0 => 32, 1 => 64, 3 => 128, _ => 32 };
        (s(self.regs[16]), s(self.regs[16] >> 4))
    }
    fn screen_w(&self) -> usize { if self.regs[12] & 0x81 != 0 { 320 } else { 256 } }

    /// Render the current VDP state into `fb` (one full frame, no raster effects).
    pub fn render(&self, fb: &mut Framebuffer) {
        let sw = self.screen_w();
        let display_on = self.regs[1] & 0x40 != 0;
        let bg = cram_to_rgba(self.cram[(self.regs[7] & 0x3F) as usize]);
        // line buffers: (colour index, priority) per layer
        for y in 0..H {
            if !display_on { for x in 0..W { fb.set(x, y, [0, 0, 0, 255]) } continue; }
            let mut line = [(0u8, false); W]; // final colour index + priority used for compositing
            let b = self.plane_line(1, y, sw);
            let a = self.plane_line(0, y, sw);
            let s = self.sprite_line(y, sw);
            for x in 0..sw.min(W) {
                // priority order: hi sprite > hi A > hi B > lo sprite > lo A > lo B > backdrop
                let cands = [(s[x], true), (a[x], true), (b[x], true), (s[x], false), (a[x], false), (b[x], false)];
                let mut c = 0u8;
                for (px, want_hi) in cands {
                    if px.0 & 0x0F != 0 && px.1 == want_hi { c = px.0; break; }
                }
                line[x] = (c, false);
            }
            for x in 0..W {
                let px = if x < sw && line[x].0 & 0x0F != 0 { cram_to_rgba(self.cram[line[x].0 as usize & 0x3F]) } else if x < sw { bg } else { [0, 0, 0, 255] };
                fb.set(x, y, px);
            }
        }
    }

    fn tile_px(&self, tile: usize, px: usize, py: usize) -> u8 {
        let o = (tile * 32 + py * 4 + px / 2) & 0xFFFF;
        let b = self.vram[o];
        if px & 1 == 0 { b >> 4 } else { b & 0x0F }
    }

    fn window_covers(&self, x: usize, y: usize) -> bool {
        let wh = self.regs[17]; let wv = self.regs[18];
        let hcell = (wh & 0x1F) as usize * 16; let vcell = (wv & 0x1F) as usize * 8;
        let in_h = if wh & 0x80 != 0 { x >= hcell } else { x < hcell };
        let in_v = if wv & 0x80 != 0 { y >= vcell } else { y < vcell };
        (hcell != 0 || wh & 0x80 != 0) && in_h || (vcell != 0 || wv & 0x80 != 0) && in_v
    }

    /// One scanline of plane A (0) or B (1): (palette<<4|index, priority).
    fn plane_line(&self, plane: usize, y: usize, sw: usize) -> [(u8, bool); W] {
        let mut out = [(0u8, false); W];
        let (pw, ph) = self.plane_size();
        let base = if plane == 0 { ((self.regs[2] as usize) & 0x38) << 10 } else { ((self.regs[4] as usize) & 7) << 13 };
        let wbase = ((self.regs[3] as usize) & 0x3E) << 10;
        let hs_base = ((self.regs[13] as usize) & 0x3F) << 10;
        let hmode = self.regs[11] & 3; let vmode = (self.regs[11] >> 2) & 1;
        let hs_off = match hmode { 0 => 0, 2 => (y & !7) * 4, 3 => y * 4, _ => (y & 7) * 4 };
        let hs = u16::from_be_bytes([self.vram[(hs_base + hs_off + plane * 2) & 0xFFFF], self.vram[(hs_base + hs_off + plane * 2 + 1) & 0xFFFF]]);
        let hscroll = (hs & 0x3FF) as i32;
        for x in 0..sw.min(W) {
            if plane == 0 && self.window_covers(x, y) {
                let wpw = if sw == 320 { 64 } else { 32 };
                let (cx, cy) = (x / 8, y / 8);
                let e = wbase + (cy * wpw + cx) * 2;
                out[x] = self.cell_px(e, x & 7, y & 7);
                continue;
            }
            let vs = if vmode == 1 { self.vsram[((x / 16) * 2 + plane).min(39)] } else { self.vsram[plane] } as usize;
            let py = (y + vs) % (ph * 8);
            let px = ((x as i32 - hscroll).rem_euclid((pw * 8) as i32)) as usize;
            let e = base + ((py / 8) * pw + px / 8) * 2;
            out[x] = self.cell_px(e, px & 7, py & 7);
        }
        out
    }

    fn cell_px(&self, e: usize, fx: usize, fy: usize) -> (u8, bool) {
        let w = u16::from_be_bytes([self.vram[e & 0xFFFF], self.vram[(e + 1) & 0xFFFF]]);
        let tile = (w & 0x7FF) as usize;
        let pal = ((w >> 13) & 3) as u8;
        let pri = w & 0x8000 != 0;
        let fx = if w & 0x0800 != 0 { 7 - fx } else { fx };
        let fy = if w & 0x1000 != 0 { 7 - fy } else { fy };
        let c = self.tile_px(tile, fx, fy);
        (if c == 0 { 0 } else { pal << 4 | c }, pri)
    }

    fn sprite_line(&self, y: usize, sw: usize) -> [(u8, bool); W] {
        let mut out = [(0u8, false); W];
        let mut filled = [false; W];
        let sat = ((self.regs[5] as usize) & 0x7F) << 9;
        let max = if sw == 320 { 80 } else { 64 };
        let mut i = 0usize;
        let mut count = 0;
        loop {
            let o = sat + i * 8;
            let rd = |k: usize| u16::from_be_bytes([self.vram[(o + k) & 0xFFFF], self.vram[(o + k + 1) & 0xFFFF]]);
            let sy = (rd(0) & 0x3FF) as i32 - 128;
            let size = rd(2) >> 8; let link = (rd(2) & 0x7F) as usize;
            let attr = rd(4); let sx = (rd(6) & 0x1FF) as i32 - 128;
            let wcells = ((size >> 2) & 3) as i32 + 1; let hcells = (size & 3) as i32 + 1;
            if (y as i32) >= sy && (y as i32) < sy + hcells * 8 {
                let mut ry = y as i32 - sy;
                if attr & 0x1000 != 0 { ry = hcells * 8 - 1 - ry }
                for dx in 0..wcells * 8 {
                    let x = sx + dx;
                    if x < 0 || x as usize >= sw.min(W) { continue }
                    let mut rx = dx;
                    if attr & 0x0800 != 0 { rx = wcells * 8 - 1 - dx }
                    let tile = (attr & 0x7FF) as i32 + (rx / 8) * hcells + ry / 8;
                    let c = self.tile_px(tile as usize & 0x7FF, (rx & 7) as usize, (ry & 7) as usize);
                    if c != 0 && !filled[x as usize] {
                        filled[x as usize] = true;
                        out[x as usize] = ((((attr >> 13) & 3) as u8) << 4 | c, attr & 0x8000 != 0);
                    }
                }
            }
            count += 1;
            if link == 0 || link >= max || count >= max { break }
            i = link;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ctrl_word_decodes_vram_write() {
        let mut v = Vdp::new();
        v.ctrl_l(0x4000_0003); // VRAM write at 0xC000
        assert_eq!(v.addr, 0xC000);
        assert_eq!(v.code & 0xF, 1);
        v.regs[15] = 2;
        v.data_w(0x1234);
        assert_eq!(&v.vram[0xC000..0xC002], &[0x12, 0x34]);
    }
    #[test]
    fn cram_write_and_render_plane_b_tile() {
        let mut v = Vdp::new();
        v.set_reg(1, 0x44); v.set_reg(12, 0x81); v.set_reg(15, 2); v.set_reg(4, 0x07); // plane B at 0xE000
        v.ctrl_l(0xC000_0000); // CRAM 0
        v.data_w(0x0000); v.data_w(0x000E); // colour 1 = red
        // tile 1 all colour 1
        v.write_vram(32, &[0x11; 32]);
        // nametable B entry (0,0) = tile 1
        v.write_vram(0xE000, &[0x00, 0x01]);
        let mut fb = Framebuffer::new();
        v.render(&mut fb);
        assert_eq!(&fb.px[0..4], &[252, 0, 0, 255]);
        assert_eq!(&fb.px[(8 * 4)..(8 * 4 + 4)], &[0, 0, 0, 255]);
    }
}
