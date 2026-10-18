//! Graphics formats and a minimal Mega Drive display model for Snatcher.
//! RE notes with evidence: docs/re/graphics.md.
//!
//! Layers:
//! - [`SubMem`]: sub-CPU address space view (DATA_xx at 0x28000, SUBCODE at 0xD400, …);
//!   the game data stores absolute sub-CPU pointers, so the runtime reads through it.
//! - [`Chunk`] / [`scene_chunks`]: scene records of a DATA file (`subfn_16`, S:0xF68E).
//! - [`Vdp`]: VRAM/CRAM/VSRAM/sprite model + H32 (256×224) line renderer with the
//!   game's mid-screen palette split (main H-INT handler M:0xFF0C4A).
//! - [`anim`]: object/animation runtime (S:0xF7F0…S:0xFD68).

use crate::lzkn;

pub const SCREEN_W: usize = 256;
pub const SCREEN_H: usize = 224;
/// Load address of DATA_xx.BIN in sub PRG-RAM (S:0x1761C loads them to 0x28000).
pub const DATA_BASE: u32 = 0x28000;
/// Load address of SUBCODE.BIN (sub program).
pub const SUBCODE_BASE: u32 = 0xD400;

/// Convert a Mega Drive CRAM word (`0000BBB0GGG0RRR0`) to RGBA8.
/// Uses the common linear 0..7 → 0..252 scaling (×36).
pub fn md_rgba(w: u16) -> [u8; 4] {
    let r = ((w >> 1) & 7) as u8;
    let g = ((w >> 5) & 7) as u8;
    let b = ((w >> 9) & 7) as u8;
    [r * 36, g * 36, b * 36, 255]
}

// ---------------------------------------------------------------------------------------------
// Sub-CPU memory view

#[derive(Default, Clone)]
pub struct SubMem {
    regions: Vec<(u32, Vec<u8>)>,
    /// pt-BR picture patch (docs/i18n-graphics.md); `None` = the original pictures
    pub gfx: Option<std::rc::Rc<crate::gfxpatch::GfxPatch>>,
    /// debug trace of every blob decoded through [`SubMem::decomp`] (sub address, hash of the original, length, patched); off by default
    pub trace: Option<std::cell::RefCell<Vec<TraceEv>>>,
}

/// One event of the debug trace of [`SubMem`] (blob decoded / written to VRAM / tile-channel DMA).
#[derive(Clone, Debug)]
pub enum TraceEv {
    /// blob decoded at sub address `src` (hash and length of the original decoded data)
    Blob { src: u32, hash: u64, len: usize, patched: bool },
    /// the last blob was written to VRAM at `dst` by a scene VRAM list
    VramUp { dst: u16, len: usize },
    /// tile channel: staging buffer `buf` bytes `off..off+len` DMA'd to VRAM `vram`
    TileSend { buf: usize, vram: u16, off: usize, len: usize },
}

impl SubMem {
    pub fn new() -> Self { Self::default() }
    /// Map `bytes` at sub address `base` (replaces any region with the same base).
    pub fn map(&mut self, base: u32, bytes: Vec<u8>) {
        self.regions.retain(|r| r.0 != base);
        self.regions.push((base, bytes));
    }
    /// The mapped regions as (base, bytes), for snapshots.
    pub fn regions(&self) -> &[(u32, Vec<u8>)] { &self.regions }
    /// Drop every region whose base is not in `keep` (a restore maps the others again).
    pub fn retain_bases(&mut self, keep: &[u32]) { self.regions.retain(|r| keep.contains(&r.0)); }
    /// Decode the LZKN1 blob at sub address `addr`; with a pt-BR picture patch installed the redrawn bytes replace the original ones.
    pub fn decomp(&self, addr: u32) -> Result<(Vec<u8>, usize), lzkn::LzError> {
        let s = self.slice_at(addr).ok_or(lzkn::LzError::Truncated)?;
        let (mut out, used) = lzkn::decompress(s)?;
        let hash = if self.trace.is_some() { crate::gfxpatch::fnv64(&out) } else { 0 };
        let patched = self.gfx.as_ref().map(|g| g.apply(&mut out)).unwrap_or(false);
        self.trace_ev(TraceEv::Blob { src: addr, hash, len: out.len(), patched });
        Ok((out, used))
    }
    pub fn trace_ev(&self, e: TraceEv) { if let Some(t) = &self.trace { t.borrow_mut().push(e); } }
    pub fn slice_at(&self, addr: u32) -> Option<&[u8]> {
        let a = addr & 0xFF_FFFF;
        for (b, d) in &self.regions {
            if a >= *b && ((a - b) as usize) < d.len() {
                return Some(&d[(a - b) as usize..]);
            }
        }
        None
    }
    pub fn u8(&self, a: u32) -> u8 { self.slice_at(a).map(|s| s[0]).unwrap_or(0) }
    pub fn u16(&self, a: u32) -> u16 { (self.u8(a) as u16) << 8 | self.u8(a + 1) as u16 }
    pub fn i16(&self, a: u32) -> i16 { self.u16(a) as i16 }
    pub fn u32(&self, a: u32) -> u32 { (self.u16(a) as u32) << 16 | self.u16(a + 2) as u32 }
    pub fn bytes(&self, a: u32, n: usize) -> Vec<u8> { (0..n as u32).map(|i| self.u8(a + i)).collect() }
}

// ---------------------------------------------------------------------------------------------
// DATA file structure

/// Header of a DATA_xx file mapped at [`DATA_BASE`].
#[derive(Debug, Clone)]
pub struct DataHeader {
    pub name: String,
    pub scene_table: u32,
    /// Present only in DATA_D0/D1/D2/Y16: sub-CPU code entry (`jsr ($28008)`, S:0xE168).
    pub code_entry: Option<u32>,
}

pub fn data_header(file: &[u8]) -> Option<DataHeader> {
    let rd = |o: usize| u32::from_be_bytes([file[o], file[o + 1], file[o + 2], file[o + 3]]);
    let p0 = rd(0);
    if !(DATA_BASE..DATA_BASE + 0x20).contains(&p0) { return None; }
    let name_off = (p0 - DATA_BASE) as usize;
    let name: String = file[name_off..name_off + 16].iter().take_while(|&&c| c != 0 && c.is_ascii_graphic())
        .map(|&c| c as char).collect();
    Some(DataHeader { name, scene_table: rd(4), code_entry: if name_off >= 12 { Some(rd(8)) } else { None } })
}

/// Number of scenes in a scene table (the first word offset bounds the table).
/// Entries are read while their index is below the smallest offset seen so far
/// (the records follow the table, but not in order).
pub fn scene_count(mem: &SubMem, table: u32) -> usize {
    let mut min = u32::MAX;
    let mut i = 0u32;
    while 2 * i < min && i < 1024 {
        let o = mem.u16(table + 2 * i) as u32;
        // a record can never start inside the table itself
        if o < 2 * (i + 1) { break; }
        min = min.min(o);
        i += 1;
    }
    i as usize
}

pub fn scene_addr(mem: &SubMem, table: u32, n: usize) -> u32 {
    table + mem.u16(table + 2 * n as u32) as u32
}

#[derive(Debug, Clone, PartialEq)]
pub struct PalDesc {
    pub addr: u32,
    pub kind: u8,
    pub delay: u8,
    /// Byte offset into the 256-byte palette shadow (0x00–0x7F set A, 0x80–0xFF set B).
    pub cram_off: u8,
    pub count: u16,
    pub frames: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VramUpload { pub src: u32, pub delay: u16, pub dst: u16 }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjSpawn { pub slot: u8, pub flags: u8, pub x: i16, pub y: i16, pub anim_table: u32, pub script: u32 }

#[derive(Debug, Clone, PartialEq)]
pub enum Chunk {
    Palette(PalDesc),
    VramList { addr: u32, items: Vec<VramUpload> },
    Objects(Vec<ObjSpawn>),
    Link(Vec<(u8, u8)>),
    Byte7893(u8),
    ScreenMode(u16),
    Chunked(u8),
    Unknown(u8, Vec<u8>),
}

/// Parse a palette channel descriptor at `a` (format: `subfn_08` S:0xE41A).
pub fn pal_desc(mem: &SubMem, a: u32) -> PalDesc {
    let mut frames = vec![];
    let mut p = a + 4;
    while mem.u16(p) != 0xFFFF && frames.len() < 256 {
        frames.push(mem.u32(p) & 0xFF_FFFF);
        p += 4;
    }
    PalDesc { addr: a, kind: mem.u8(a), delay: mem.u8(a + 1), cram_off: mem.u8(a + 2), count: mem.u8(a + 3) as u16 + 1, frames }
}

pub fn vram_list(mem: &SubMem, a: u32) -> Vec<VramUpload> {
    let mut v = vec![];
    let mut p = a;
    while mem.u16(p) != 0xFFFF && v.len() < 256 {
        v.push(VramUpload { src: mem.u32(p) & 0xFF_FFFF, delay: mem.u16(p + 4), dst: mem.u16(p + 6) });
        p += 8;
    }
    v
}

/// Parse the chunk list of a scene record (S:0xF6A6 loop).
pub fn scene_chunks(mem: &SubMem, rec: u32) -> Vec<Chunk> {
    let mut out = vec![];
    let mut p = rec;
    for _ in 0..64 {
        let ty = mem.u8(p);
        if ty & 0x80 != 0 { break; }
        let len = mem.u8(p + 1) as u32;
        let pl = p + 2;
        let end = pl + len;
        out.push(match ty {
            0 => Chunk::Palette(pal_desc(mem, pl)),
            1 => Chunk::VramList { addr: pl, items: vram_list(mem, pl) },
            2 => {
                let mut v = vec![];
                let mut q = pl;
                while q + 12 <= end {
                    let rel = mem.i16(q + 10) as i32;
                    v.push(ObjSpawn {
                        slot: mem.u8(q), flags: mem.u8(q + 1), x: mem.i16(q + 2), y: mem.i16(q + 4),
                        anim_table: mem.u32(q + 6) & 0xFF_FFFF, script: ((q + 12) as i32 + rel) as u32,
                    });
                    q += 12;
                }
                Chunk::Objects(v)
            }
            3 | 4 => {
                let mut v = vec![];
                let mut q = pl;
                while q + 2 <= end { v.push((mem.u8(q), mem.u8(q + 1))); q += 2; }
                Chunk::Link(v)
            }
            5 | 6 | 7 => Chunk::Byte7893(mem.u8(pl + 1)),
            8 => Chunk::ScreenMode(mem.u16(pl)),
            9 => Chunk::Chunked(mem.u8(pl + 1)),
            _ => Chunk::Unknown(ty, mem.bytes(pl, len as usize)),
        });
        p = end;
    }
    out
}

// ---------------------------------------------------------------------------------------------
// VDP model

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sprite { pub y: i16, pub size: u8, pub link: u8, pub attr: u16, pub x: i16 }

crate::snap_struct!(Sprite { y, size, link, attr, x });

#[derive(Clone)]
pub struct Vdp {
    pub vram: Vec<u8>,
    /// Palette set A (CRAM contents at the top of the frame, M:$FFEB80).
    pub cram: [u16; 64],
    /// Palette set B (M:$FFEC00); colours 16–63 replace set A from line `split_line`.
    pub cram_b: [u16; 64],
    /// Line where the mode-0 H-INT handler (M:0xFF0C4A, counter 0x88) starts copying
    /// set B lines 1–3 into CRAM, 4 colours per line. `None` = no split.
    pub split_line: Option<u16>,
    pub plane_a: u16,
    pub plane_b: u16,
    /// Plane size in cells (VDP reg 0x90: 0x11 → 64×64, 0x01 → 64×32).
    pub plane_w: u16,
    pub plane_h: u16,
    pub hscroll_a: i16,
    pub hscroll_b: i16,
    pub vscroll_a: i16,
    pub vscroll_b: i16,
    pub sprites: Vec<Sprite>,
    pub display: bool,
    pub backdrop: u8,
    /// Per-line raster program from the main-CPU display logic (None = static frame).
    pub raster: Option<raster::Raster>,
}

crate::snap_struct!(Vdp { vram, cram, cram_b, split_line, plane_a, plane_b, plane_w, plane_h, hscroll_a, hscroll_b, vscroll_a, vscroll_b, sprites, display, backdrop, raster });

impl Default for Vdp {
    fn default() -> Self {
        // In-game register set: main boot state 1 ends with M:0xFF1538 → table M:0xFF15A2/A6
        // (`9010 833C 8238 8406 857F 8D3E`): H32, planes 32×64 cells, A=0xE000, B=0xC000,
        // window 0xF000, sprites 0xFE00, hscroll 0xF800; $E048=1 (M:0xFF0A42).
        Vdp {
            vram: vec![0; 0x10000], cram: [0; 64], cram_b: [0; 64], split_line: Some(137),
            plane_a: 0xE000, plane_b: 0xC000, plane_w: 32, plane_h: 64,
            hscroll_a: 0, hscroll_b: 0, vscroll_a: 0, vscroll_b: 0,
            sprites: vec![], display: true, backdrop: 0, raster: None,
        }
    }
}

impl Vdp {
    pub fn write_vram(&mut self, dst: u16, data: &[u8]) {
        for (i, b) in data.iter().enumerate() {
            self.vram[(dst as usize + i) & 0xFFFF] = *b;
        }
    }

    /// Decompress an LZKN1 blob at sub address `src` and write it to VRAM (`subfn_07`).
    pub fn upload_lzkn(&mut self, mem: &SubMem, src: u32, dst: u16) -> Result<usize, lzkn::LzError> {
        let (out, _) = mem.decomp(src)?;
        self.write_vram(dst, &out);
        Ok(out.len())
    }

    #[inline]
    fn pixel(&self, tile: u16, x: usize, y: usize) -> u8 {
        let a = (tile as usize * 32 + y * 4 + x / 2) & 0xFFFF;
        let b = self.vram[a];
        if x & 1 == 0 { b >> 4 } else { b & 15 }
    }

    /// Fetch a plane pixel: returns (colour index 0..63 with 0 meaning transparent, priority).
    fn plane_px(&self, base: u16, hs: i16, vs: i16, sx: usize, sy: usize) -> (u8, bool) {
        let pw = self.plane_w as i32 * 8;
        let ph = self.plane_h as i32 * 8;
        let px = ((sx as i32 - hs as i32).rem_euclid(pw)) as usize;
        let py = ((sy as i32 + vs as i32).rem_euclid(ph)) as usize;
        let e = u16::from_be_bytes([
            self.vram[(base as usize + 2 * ((py / 8) * self.plane_w as usize + px / 8)) & 0xFFFF],
            self.vram[(base as usize + 2 * ((py / 8) * self.plane_w as usize + px / 8) + 1) & 0xFFFF],
        ]);
        let tile = e & 0x7FF;
        let (mut tx, mut ty) = (px & 7, py & 7);
        if e & 0x800 != 0 { tx = 7 - tx; }
        if e & 0x1000 != 0 { ty = 7 - ty; }
        let c = self.pixel(tile, tx, ty);
        if c == 0 { (0, e & 0x8000 != 0) } else { ((((e >> 13) & 3) as u8) << 4 | c, e & 0x8000 != 0) }
    }

    /// Render one frame (H32) into `fb` (RGBA8, 256×224).
    pub fn render(&self, fb: &mut [u8]) {
        assert!(fb.len() >= SCREEN_W * SCREEN_H * 4);
        let mut line_spr = [(0u8, false); SCREEN_W];
        let mut cram = self.cram;
        let split = match &self.raster { Some(r) => r.pal_split, None => self.split_line };
        for y in 0..SCREEN_H {
            let (vsa, vsb, disp, hsb, hsa) = match &self.raster {
                Some(r) => {
                    let l = r.lines[y];
                    let hb = r.line_hscroll_b.as_ref().map(|v| v[y]).unwrap_or(self.hscroll_b);
                    if l.hscroll_zero { (l.vscroll_a, l.vscroll_b, l.display, 0, 0) } else { (l.vscroll_a, l.vscroll_b, l.display, hb, self.hscroll_a) }
                }
                None => (self.vscroll_a, self.vscroll_b, self.display, self.hscroll_b, self.hscroll_a),
            };
            if let Some(sl) = split {
                let y16 = y as u16;
                if y16 >= sl && y16 < sl + 12 {
                    let k = (y16 - sl) as usize;
                    for i in 0..4 { cram[16 + 4 * k + i] = self.cram_b[16 + 4 * k + i]; }
                }
            }
            let row = &mut fb[y * SCREEN_W * 4..(y + 1) * SCREEN_W * 4];
            if !disp {
                for p in row.chunks_mut(4) { p.copy_from_slice(&[0, 0, 0, 255]); }
                continue;
            }
            self.sprite_line(y, &mut line_spr);
            for x in 0..SCREEN_W {
                let (b, bp) = self.plane_px(self.plane_b, hsb, vsb, x, y);
                let (a, ap) = self.plane_px(self.plane_a, hsa, vsa, x, y);
                let (s, sp) = line_spr[x];
                // Priority order (high to low): S(hi) A(hi) B(hi) S(lo) A(lo) B(lo).
                let layers = [(s, sp), (a, ap), (b, bp), (s, false), (a, false), (b, false)];
                let mut col = self.backdrop;
                let order: [(u8, bool); 6] = layers;
                let mut found = false;
                for pass in [true, false] {
                    for &(c, pr) in order.iter() {
                        if pr == pass && c & 15 != 0 { col = c; found = true; break; }
                    }
                    if found { break; }
                }
                let rgba = md_rgba(cram[(col & 63) as usize]);
                row[x * 4..x * 4 + 4].copy_from_slice(&rgba);
            }
        }
    }

    /// Sprites on line `y` following the link list (max 64), first sprite has top priority.
    fn sprite_line(&self, y: usize, out: &mut [(u8, bool); SCREEN_W]) {
        for o in out.iter_mut() { *o = (0, false); }
        if self.sprites.is_empty() { return; }
        let mut idx = 0usize;
        let mut count = 0;
        let mut drawn = 0;
        loop {
            let s = match self.sprites.get(idx) { Some(s) => *s, None => break };
            let w = ((s.size >> 2) & 3) as i32 + 1;
            let h = (s.size & 3) as i32 + 1;
            let sy = s.y as i32 - 128;
            let yy = y as i32 - sy;
            if yy >= 0 && yy < h * 8 && drawn < 16 {
                drawn += 1;
                let vf = s.attr & 0x1000 != 0;
                let hf = s.attr & 0x800 != 0;
                let ty = if vf { h * 8 - 1 - yy } else { yy };
                for xx in 0..w * 8 {
                    let sx = s.x as i32 - 128 + xx;
                    if !(0..SCREEN_W as i32).contains(&sx) { continue; }
                    if out[sx as usize].0 & 15 != 0 { continue; }
                    let tx = if hf { w * 8 - 1 - xx } else { xx };
                    let tile = (s.attr & 0x7FF) as i32 + (tx / 8) * h + ty / 8;
                    let c = self.pixel(tile as u16, (tx & 7) as usize, (ty & 7) as usize);
                    if c != 0 {
                        out[sx as usize] = ((((s.attr >> 13) & 3) as u8) << 4 | c, s.attr & 0x8000 != 0);
                    }
                }
            }
            count += 1;
            if s.link == 0 || count >= 64 { break; }
            idx = s.link as usize;
        }
    }

    /// Render a whole plane (debug/export): `cols`×`rows` cells starting at cell (0,0),
    /// transparent pixels left as alpha 0.
    pub fn render_plane(&self, base: u16, cols: usize, rows: usize) -> Vec<u8> {
        let mut img = vec![0u8; cols * 8 * rows * 8 * 4];
        let w = cols * 8;
        for cy in 0..rows {
            for cx in 0..cols {
                let a = base as usize + 2 * (cy * self.plane_w as usize + cx);
                let e = u16::from_be_bytes([self.vram[a & 0xFFFF], self.vram[(a + 1) & 0xFFFF]]);
                for y in 0..8 {
                    for x in 0..8 {
                        let tx = if e & 0x800 != 0 { 7 - x } else { x };
                        let ty = if e & 0x1000 != 0 { 7 - y } else { y };
                        let c = self.pixel(e & 0x7FF, tx, ty);
                        if c == 0 { continue; }
                        let rgba = md_rgba(self.cram[(((e >> 13) & 3) as usize) * 16 + c as usize]);
                        let o = ((cy * 8 + y) * w + cx * 8 + x) * 4;
                        img[o..o + 4].copy_from_slice(&rgba);
                    }
                }
            }
        }
        img
    }
}

// ---------------------------------------------------------------------------------------------
// Static scene application (no timing): what the scene looks like once loaded.

/// Apply a palette descriptor's final colours to the 128-colour shadow (set A 0..63, set B 64..127).
pub fn apply_palette_final(mem: &SubMem, pd: &PalDesc, shadow: &mut [u16; 128]) {
    let start = pd.cram_off as usize / 2;
    let src = match pd.kind {
        1 | 2 | 6 | 9 => pd.frames.first().copied(),
        _ => None,
    };
    match (pd.kind, src) {
        (_, Some(s)) => {
            for i in 0..pd.count as usize {
                if start + i < 128 { shadow[start + i] = mem.u16(s + 2 * i as u32); }
            }
        }
        (10, None) | (3, None) => {
            for i in 0..pd.count as usize { if start + i < 128 { shadow[start + i] = 0; } }
        }
        (4, None) | (5, None) => {
            for i in 0..pd.count as usize { if start + i < 128 { shadow[start + i] = 0x0EEE; } }
        }
        _ => {}
    }
}

/// Apply all VRAM uploads and final palettes of a scene to `vdp` + palette shadow.
pub fn apply_scene_static(mem: &SubMem, rec: u32, vdp: &mut Vdp, shadow: &mut [u16; 128]) -> Vec<String> {
    let mut log = vec![];
    for c in scene_chunks(mem, rec) {
        match c {
            Chunk::Palette(pd) => apply_palette_final(mem, &pd, shadow),
            Chunk::VramList { items, .. } => {
                for u in items {
                    match vdp.upload_lzkn(mem, u.src, u.dst) {
                        Ok(n) => log.push(format!("vram {:06x} -> {:04x} ({} bytes)", u.src, u.dst, n)),
                        Err(e) => log.push(format!("vram {:06x} -> {:04x} FAILED {:?}", u.src, u.dst, e)),
                    }
                }
            }
            _ => {}
        }
    }
    vdp.cram.copy_from_slice(&shadow[..64]);
    vdp.cram_b.copy_from_slice(&shadow[64..]);
    log
}

// ---------------------------------------------------------------------------------------------
// Text font (S:0x13E9C draws 1bpp glyphs into a 4bpp bitmap; S:0x13F94 advances by width).

/// 1bpp 8×8 glyphs, (char-0x20)*8, MSB = leftmost pixel (S:0x13EB8 `lea $14B7C,a5`).
pub const FONT_ADDR: u32 = 0x14B7C;
/// Proportional advance in pixels per glyph (S:0x13F94 `lea $14EAC,a5`).
pub const FONT_WIDTH_ADDR: u32 = 0x14EAC;
/// Glyph count: the bitmap ends where the width table starts ((0x14EAC-0x14B7C)/8).
pub const FONT_GLYPHS: usize = 102;

/// Returns (rows, advance) for character code `ch` (0x20..0x85).
pub fn font_glyph(mem: &SubMem, ch: u8) -> ([u8; 8], u8) {
    let i = ch.wrapping_sub(0x20) as u32;
    let mut g = [0u8; 8];
    for (r, b) in g.iter_mut().enumerate() { *b = mem.u8(FONT_ADDR + i * 8 + r as u32); }
    (g, mem.u8(FONT_WIDTH_ADDR + i))
}

pub mod anim;
pub mod raster;

/// Complete display pipeline: sub-CPU scene runtime + main-CPU effects/raster + VDP.
/// Call [`Display::frame`] once per 60 Hz frame, then [`Display::render`].
pub struct Display {
    pub vdp: Vdp,
    pub sub: anim::Runtime,
    pub main: raster::MainDisplay,
    /// Main-CPU-only display (boot copyright/Konami logo, M:0xFF2BE4): the sub display
    /// runtime is not run and the VDP state is whatever the main-side code set
    /// (no raster program, sprites from `main_sat`).
    pub main_only: bool,
    /// Sprite list built by main-CPU code (M:$FFED00, DMA'd by M:0xFF179C when $E048 = 0).
    pub main_sat: Vec<Sprite>,
}

crate::snap_struct!(Display { vdp, sub, main, main_only, main_sat });

impl Default for Display {
    fn default() -> Self {
        Display { vdp: Vdp::default(), sub: anim::Runtime::new(), main: raster::MainDisplay::new(), main_only: false, main_sat: vec![] }
    }
}

impl Display {
    pub fn new() -> Self { Self::default() }

    /// `subfn_16(n)` for the DATA file currently mapped at [`DATA_BASE`].
    pub fn load_scene(&mut self, mem: &SubMem, n: usize) -> bool {
        let table = mem.u32(DATA_BASE + 4) & 0xFF_FFFF;
        if n >= scene_count(mem, table) { return false; }
        self.sub.load_scene(mem, scene_addr(mem, table, n));
        true
    }

    /// One frame: sub display tick, mailbox to the main CPU, main VBlank (raster program).
    pub fn frame(&mut self, mem: &SubMem) {
        if self.main_only {
            self.vdp.raster = None;
            self.vdp.sprites = self.main_sat.clone();
            return;
        }
        self.sub.tick(mem, &mut self.vdp);
        let (a, b) = std::mem::take(&mut self.sub.mail);
        self.main.mailbox(a, b);
        let req = std::mem::take(&mut self.sub.split_out);
        if req != 0 { self.main.e036 = if req == 0xFF { 0 } else { req as u16 }; }
        let p = &self.sub.scroll.pos;
        let ss = [(p[0] >> 16) as i16, (p[1] >> 16) as i16, (p[2] >> 16) as i16, (p[3] >> 16) as i16];
        let r = self.main.vblank(ss);
        self.vdp.hscroll_a = self.main.e700.0 as i16;
        self.vdp.hscroll_b = self.main.e700.1 as i16;
        self.vdp.raster = Some(r);
    }

    /// A frame in which the sub CPU's handler did not run (it was busy): only the main CPU's VBlank work (effects, raster).
    pub fn frame_stalled(&mut self) {
        if self.main_only { return; }
        let p = &self.sub.scroll.pos;
        let ss = [(p[0] >> 16) as i16, (p[1] >> 16) as i16, (p[2] >> 16) as i16, (p[3] >> 16) as i16];
        let r = self.main.vblank(ss);
        self.vdp.hscroll_a = self.main.e700.0 as i16;
        self.vdp.hscroll_b = self.main.e700.1 as i16;
        self.vdp.raster = Some(r);
    }

    pub fn render(&self, fb: &mut [u8]) { self.vdp.render(fb) }
}
