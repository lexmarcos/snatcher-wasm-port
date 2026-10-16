//! Scene runtime: objects + animation scripts, palette channels, VRAM upload lists,
//! scroll and sprite-table building. Re-implementation of the sub-CPU display code
//! (addresses refer to the SUBCODE listing, see docs/re/graphics.md §5).
//!
//! Frame order (S:0xD71E–0xD736): [game logic] → `obj_update_all` (skipped while a VRAM
//! list is pending) → `scroll_tick` → `sat_build` → `palch_run` → `tilech_run` →
//! `vram_list_tick`.

use super::{scene_chunks, Chunk, Sprite, SubMem, Vdp};

/// Events the display code raises for other subsystems (sound, CD sync …).
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Anim op 0x9D (S:0xFBDE): `subfn_12(d0=id, d1=0)` – sound request (HYPOTHESIS: FM/PCM sfx).
    Sound { id: u8, kind: u8 },
    /// Anim op 0x9F (S:0xFC0C): `subfn_06(d0=id, d1=-1)` – periodic request (HYPOTHESIS: voice/sfx).
    Periodic { id: u8 },
    /// Chunk type 8: screen mode word for M:$236406.
    ScreenMode(u16),
    /// Chunk types 5/6/7 (S:0xF7CA): `$7893 = payload[1]` (portrait/wide text-window layout).
    Byte7893(u8),
    /// $B6403 request: bit0 clear plane A (0xE000, 4 KiB), bit1 clear plane B (0xC000, 4 KiB).
    ClearPlanes(u8),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Obj {
    pub active: u16,     // +$00 (1 = alive)
    pub blink_mask: u16, // +$02 (sprite skipped when mask & frame counter)
    pub flip: u8,        // +$05 (low byte of the +$04 word set by anim op 0xA0): bit0 hflip, bit1 vflip, bit2/3 follow plane A/B scroll
    pub x: i32,          // +$06 16.16
    pub y: i32,          // +$0A 16.16
    pub vx_set: i32,     // +$0E
    pub vy_set: i32,     // +$12
    pub pal_override: u8, // +$16 (bit7 = use bits 0-1 as palette)
    pub prio: u16,       // +$18
    pub group: u8,       // +$1C
    pub wait_time: u32,  // +$20 (op 0x99: wait for CD time)
    pub flags: u8,       // +$25 bit0 paused, bit1 hidden, bit2 waiting for time
    pub fast: u8,        // +$26
    pub rep: u8,         // +$27 repeat counter
    pub idx: u16,        // +$28 script index (0xFFFF = before start)
    pub delay: i16,      // +$2A
    pub anim_table: u32, // +$2E
    pub script: u32,     // +$32
    pub frame: u32,      // +$36 current frame data address (0 = none)
    pub vx: i32,         // +$40
    pub vy: i32,         // +$44
    pub parent: u16,     // +$48
    pub child: u16,      // +$4A
    pub sibling: u16,    // +$4C
    pub f4e: u8,         // +$4E
    pub hold: u8,        // +$4F
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PalCh {
    pub kind: u8,
    pub cnt: u8,
    pub off: u8,
    pub n: u8,
    pub list: u32,   // +$4 current frame-pointer slot
    pub desc: u32,   // +$8 descriptor
    pub steps: u8,   // +$C
}

/// Tile-stream channel (S:$8E00, 7 × 8 bytes).
#[derive(Debug, Clone, Copy, Default)]
pub struct TileCh {
    pub kind: u8,  // +0: bits0-3 kind, bits4-5 buffer, bit7 "already decompressed"
    pub step: u8,  // +1
    pub ptr: u32,  // +2 current entry
}

#[derive(Debug, Clone, Default)]
pub struct Scroll {
    pub mode: u16,        // $8F00
    pub vel: [i16; 4],    // $8F02..$8F08 (8.8): A.x A.y B.x B.y
    pub delta: [i32; 4],  // $8F18 $8F1C $8F28 $8F2C (16.16)
    pub pos: [i32; 4],    // $8F30..$8F3C (16.16)
}

#[derive(Clone)]
pub struct Runtime {
    pub objs: Vec<Obj>,
    pub palch: [PalCh; 12],
    pub palch_rr: usize,
    /// 128-colour shadow (S:$B6100): set A 0..63, set B 64..127.
    pub shadow: [u16; 128],
    pub pal_dirty: u8,
    pub vram_list: Option<u32>,
    pub vram_delay: u16,
    pub scroll: Scroll,
    pub tilech: [TileCh; 7],
    /// Pending effect requests $8F0A (raster channel), $8F0B (scroll/wave channel), $8F0C (reset).
    pub fx_a: u8,
    pub fx_b: u8,
    pub fx_reset: u8,
    /// One-shot mailbox to the main CPU (S:$B6408, S:$B640C), consumed by the display.
    pub mail: (u8, u8),
    /// mailbox value waiting for the next frame (see `scroll_tick`)
    pub mail_next: (u8, u8),
    /// S:$7891 → S:$B6405: palette-split request (persistent on the main side; 0xFF = refresh only).
    pub split_req: u8,
    /// `split_req` after the mailbox delay (see `tick`)
    pub split_out: u8,
    split_next: [u8; 4],
    /// Word RAM staging buffers S:0xB8000/0xBA000/0xBC000/0xBE000 (8 KiB each).
    pub tilebuf: Vec<Vec<u8>>,
    pub rng: [u8; 4],
    pub frame: u16,
    pub budget: u16, // $78B2
    pub cd_time: u32, // $787A
    pub pause_groups: u8, // $78C6
    pub events: Vec<Event>,
    /// S:$7866 (anim op A3): load the default text palette at the next scene reset (S:0xF5FC)
    pub f7866: u8,
    /// S:$788A (anim ops A4 bit0, AE bit1): palette black-out control for S:0xF560
    pub f788a: u8,
    /// S:$788B (anim ops 9B/9C): keep palettes on scene clear
    pub f788b: u8,
    /// S:$7887: scene-reset plane-clear handshake (1 = requested, 2 = main clearing); while it or a
    /// VRAM list ($7886) is pending the sub frame skips mode logic and object updates (S:0xD6EE)
    pub f7887: u8,
    /// `self.frame` of the last DMA-queue push (`wr_dmaq` = S:$B6000, drained by the main CPU in its
    /// next VBlank): tile channels on buffer 0 and the VRAM list wait while it is non-empty
    pub last_dma: u16,
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime {
            objs: vec![Obj::default(); 64], palch: [PalCh::default(); 12], palch_rr: 11,
            shadow: [0; 128], pal_dirty: 0, vram_list: None, vram_delay: 0, scroll: Scroll::default(),
            tilech: [TileCh::default(); 7], tilebuf: vec![vec![0u8; 0x2000]; 4],
            fx_a: 0, fx_b: 0, fx_reset: 0, mail: (0, 0), mail_next: (0, 0), split_req: 0, split_out: 0, split_next: [0; 4],
            rng: [0; 4], frame: 0, budget: 1, cd_time: 0, pause_groups: 0, events: vec![],
            f7866: 0, f788a: 0, f788b: 0, f7887: 0, last_dma: 0xFF00,
        }
    }
}

impl Runtime {
    pub fn new() -> Self { Self::default() }

    /// RNG S:0xFD96 (state bytes $786A..$786D).
    pub fn rand(&mut self) -> u16 {
        let r = &mut self.rng;
        r[0] = r[0].wrapping_add(r[1]);
        let w01 = |r: &[u8; 4]| u16::from_be_bytes([r[0], r[1]]);
        let set01 = |r: &mut [u8; 4], v: u16| { let b = v.to_be_bytes(); r[0] = b[0]; r[1] = b[1]; };
        let d0 = u16::from_be_bytes([r[2], r[3]]);
        let v = w01(r).wrapping_add(d0); set01(r, v);
        let d0b = ((d0 << 8) & 0xFF00) | r[2] as u16;
        let v = w01(r).wrapping_add(d0b); set01(r, v);
        let (s, c) = r[2].overflowing_add(r[3]);
        r[2] = s;
        if c { let v = w01(r).wrapping_add(1); set01(r, v); }
        let w23 = u16::from_be_bytes([r[2], r[3]]);
        let (s, c) = w23.overflowing_add(1);
        r[2] = (s >> 8) as u8; r[3] = s as u8;
        if c { let v = w01(r).wrapping_add(1); set01(r, v); }
        w01(r)
    }

    fn rand_range(&mut self, lo: u16, span: u16) -> u16 {
        let r = self.rand() as u32;
        ((r * span as u32) >> 16) as u16 + lo
    }

    /// Kill all objects (subfn_11 on every slot) and stop palette channels.
    pub fn reset(&mut self) {
        for o in self.objs.iter_mut() { *o = Obj::default(); }
        self.palch = [PalCh::default(); 12];
        self.tilech = [TileCh::default(); 7];
        self.vram_list = None;
    }

    /// `subfn_16` / S:0xF6A6: run a scene record's chunk list.
    pub fn load_scene(&mut self, mem: &SubMem, rec: u32) {
        for c in scene_chunks(mem, rec) {
            match c {
                Chunk::Palette(pd) => { self.palch_start(mem, pd.addr); }
                Chunk::VramList { addr, .. } => { self.vram_list = Some(addr); self.vram_delay = 0; }
                Chunk::Objects(v) => {
                    for s in v {
                        let slot = s.slot as usize & 63;
                        // S:0xF718 `tst.b (a1)` tests the HIGH byte of the active word (live objects hold 1 -> 0x0001 -> byte 0),
                        // so an occupied slot is simply re-created (subfn_11 clears it first); `rts` would end the chunk
                        if (self.objs[slot].active >> 8) != 0 { break; }
                        let mut o = Obj { active: 1, flags: s.flags, x: (s.x as i32) << 16, y: (s.y as i32) << 16,
                            anim_table: s.anim_table, script: s.script, idx: 0xFFFF, ..Obj::default() };
                        // S:0xF74A: initial frame (skipping A1/A7 palette-select entries)
                        let mut e = s.script;
                        loop {
                            let b0 = mem.u8(e);
                            if b0 < 0x80 { o.frame = frame_addr(mem, s.anim_table, b0); break; }
                            if b0 == 0xA1 || b0 == 0xA7 { o.pal_override = mem.u8(e + 1); e += 4; continue; }
                            break;
                        }
                        self.objs[slot] = o;
                    }
                }
                Chunk::Link(v) => {
                    for (a, b) in v {
                        let (a, b) = (a as usize & 63, b as usize & 63);
                        self.objs[b].sibling = self.objs[a].child;
                        self.objs[a].child = b as u16;
                        self.objs[b].parent = a as u16;
                    }
                }
                Chunk::ScreenMode(w) => self.events.push(Event::ScreenMode(w)),
                Chunk::Byte7893(b) => self.events.push(Event::Byte7893(b)),
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------------ palette channels

    /// `subfn_08` S:0xE41A. Returns false if no channel is free.
    pub fn palch_start(&mut self, mem: &SubMem, desc: u32) -> bool {
        let mut i = self.palch_rr;
        for _ in 0..12 {
            i = (i + 1) % 12;
            if self.palch[i].kind == 0 {
                let kind = mem.u8(desc);
                self.palch[i] = PalCh { kind, cnt: 0, off: mem.u8(desc + 2), n: mem.u8(desc + 3), list: desc + 4, desc, steps: 0 };
                if kind >= 0x80 {
                    for c in self.palch.iter_mut() { if c.kind == 6 { c.kind = 0; } }
                    self.palch[i].kind = kind & 0x7F;
                }
                return true;
            }
        }
        false
    }

    /// S:0xE4B4: run every active palette channel once.
    pub fn palch_run(&mut self, mem: &SubMem) {
        for k in 0..12 {
            let i = (self.palch_rr + 1 + k) % 12;
            if self.palch[i].kind != 0 { self.palch_step(mem, i); }
        }
    }

    fn palch_delay(&mut self, mem: &SubMem, i: usize) -> bool {
        let c = &mut self.palch[i];
        if c.cnt != 0 { c.cnt -= 1; return false; }
        c.cnt = mem.u8(c.desc + 1);
        self.pal_dirty |= if c.off & 0x80 != 0 { 2 } else { 1 };
        true
    }

    fn palch_step(&mut self, mem: &SubMem, i: usize) {
        let kind = self.palch[i].kind;
        let off = self.palch[i].off as usize / 2;
        let n = self.palch[i].n as usize + 1;
        let target = |mem: &SubMem, c: &PalCh| mem.u32(c.list) & 0xFF_FFFF;
        match kind {
            1 | 6 => {
                if !self.palch_delay(mem, i) { return; }
                let src = target(mem, &self.palch[i]);
                for k in 0..n { if off + k < 128 { self.shadow[off + k] = mem.u16(src + 2 * k as u32); } }
                self.palch[i].list += 4;
                if mem.u16(self.palch[i].list) == 0xFFFF {
                    if kind == 1 { self.palch[i].kind = 0; self.palch[i].cnt = 0; } else { self.palch[i].list = self.palch[i].desc + 4; }
                }
            }
            2 | 4 => {
                let v = if kind == 4 { 0x0EEE } else { 0 };
                for k in 0..n { if off + k < 128 { self.shadow[off + k] = v; } }
                self.palch[i].kind = 9;
                self.palch_step(mem, i);
            }
            3 | 5 => {
                if !self.palch_delay(mem, i) { return; }
                for k in 0..n {
                    if off + k >= 128 { break; }
                    let c = self.shadow[off + k];
                    let (mut b, mut g, mut r) = ((c >> 8) & 0xE, (c >> 4) & 0xE, c & 0xE);
                    if kind == 3 {
                        b = b.saturating_sub(2); g = g.saturating_sub(2); r = r.saturating_sub(2);
                    } else {
                        b = (b + 2).min(0xE); g = (g + 2).min(0xE); r = (r + 2).min(0xE);
                    }
                    self.shadow[off + k] = b << 8 | g << 4 | r;
                }
                self.palch[i].steps += 1;
                if self.palch[i].steps >= 8 { self.palch[i].kind = 0; }
            }
            7 | 8 | 9 => {
                if !self.palch_delay(mem, i) { return; }
                let src = target(mem, &self.palch[i]);
                let first = mem.u16(src);
                for k in 0..n {
                    if off + k >= 128 { break; }
                    let t = match kind {
                        7 => first,
                        8 => {
                            let c = self.shadow[off + k];
                            let s = (c & 0xF) + ((c >> 4) & 0xF) + ((c >> 8) & 0xF);
                            let v = (s / 3) & 0xE;
                            v << 8 | v << 4 | v
                        }
                        _ => mem.u16(src + 2 * k as u32),
                    };
                    self.shadow[off + k] = fade_toward(self.shadow[off + k], t);
                }
                self.palch[i].steps += 1;
                if self.palch[i].steps >= 8 { self.palch[i].kind = 0; }
            }
            10 => {
                for k in 0..n { if off + k < 128 { self.shadow[off + k] = 0; } }
                self.palch[i].kind = 0;
            }
            _ => { self.palch[i].kind = 0; }
        }
    }

    // ------------------------------------------------------------------ objects

    /// S:0xF7F0 obj_update_all (skipped by the caller while a VRAM list is pending).
    pub fn obj_update_all(&mut self, mem: &SubMem) {
        for i in 0..64 {
            let mut budget = self.budget;
            loop {
                let o = self.objs[i];
                if (o.group != 0 || o.fast != 0) && self.pause_groups != 0 { break; }
                if o.active != 1 { break; }
                if o.flags & 4 != 0 {
                    if self.cd_time < o.wait_time { break; }
                    self.objs[i].flags &= !4;
                }
                if self.objs[i].flags & 1 != 0 { break; }
                self.anim_tick(mem, i);
                budget = budget.wrapping_sub(1);
                if budget == 0 || budget > 0x8000 { break; }
                if self.objs[i].fast == 0 { break; }
            }
        }
        for i in 0..64 {
            let o = self.objs[i];
            if o.active != 1 || (o.group & self.pause_groups) != 0 { continue; }
            let idx = if o.idx == 0xFFFF { 0 } else { o.idx };
            let e = o.script + 4 * idx as u32;
            let b0 = mem.u8(e);
            if b0 < 0x80 { self.objs[i].frame = frame_addr(mem, o.anim_table, b0); }
            if o.flags & 1 == 0 {
                self.objs[i].x = o.x.wrapping_add(o.vx);
                self.objs[i].y = o.y.wrapping_add(o.vy);
            }
        }
    }

    /// S:0xF8C0
    fn anim_tick(&mut self, mem: &SubMem, i: usize) {
        self.objs[i].delay = self.objs[i].delay.wrapping_sub(1);
        if self.objs[i].delay > 0 {
            let o = self.objs[i];
            if mem.u8(o.script + 4 * o.idx as u32) != 0x97 { self.objs[i].flags &= !2; }
            return;
        }
        let advance = self.objs[i].hold == 0;
        self.anim_step(mem, i, advance);
    }

    /// S:0xF8F2 (advance=true) / S:0xF8F6 (advance=false): run script entries until a frame.
    fn anim_step(&mut self, mem: &SubMem, i: usize, mut advance: bool) {
        for _guard in 0..256 {
            if advance { self.objs[i].idx = self.objs[i].idx.wrapping_add(1); }
            advance = true;
            self.objs[i].flags &= !2;
            let e = self.objs[i].script + 4 * self.objs[i].idx as u32;
            let b = [mem.u8(e), mem.u8(e + 1), mem.u8(e + 2), mem.u8(e + 3)];
            let w = u16::from_be_bytes([b[2], b[3]]);
            if b[0] < 0x80 {
                self.set_delay(i, &b);
                return;
            }
            let slot = |n: u8| (n as usize) & 63;
            match b[0] - 0x80 {
                0x00 => { self.objs[i] = Obj::default(); return; }
                0x01 => { self.objs[i].idx = b[1] as u16; advance = false; }
                0x02 | 0x03 => {
                    let cnt = if b[0] == 0x83 {
                        if self.objs[i].rep == 0 {
                            let r = self.rand_range(2, (b[2].wrapping_sub(b[1])) as u16) as u8;
                            self.objs[i].rep = r;
                        }
                        b[2]
                    } else { b[1] };
                    if self.objs[i].rep != cnt {
                        self.objs[i].rep += 1;
                        self.objs[i].idx = b[3] as u16;
                        advance = false;
                    } else { self.objs[i].rep = 0; }
                }
                0x04 => { let d = (w as i16 as i32) - (self.objs[i].x >> 16); self.move_tree(i, d, 0); }
                0x05 => { let d = (w as i16 as i32) - (self.objs[i].y >> 16); self.move_tree(i, 0, d); }
                0x06 => {
                    let v = ((w as i16 as i32) << 16) | ((b[1] as i32) << 8);
                    self.objs[i].vx_set = v;
                    let p = self.objs[i].parent as usize;
                    self.objs[i].vx = if p != 0 { v.wrapping_add(self.objs[p].vx) } else { v };
                    self.prop_vel(i);
                }
                0x07 => {
                    let v = ((w as i16 as i32) << 16) | ((b[1] as i32) << 8);
                    self.objs[i].vy_set = v;
                    let p = self.objs[i].parent as usize;
                    self.objs[i].vy = if p != 0 { v.wrapping_add(self.objs[p].vy) } else { v };
                    self.prop_vel(i);
                }
                0x08 => { let d = self.objs[i].script.wrapping_add(w as i16 as i32 as u32); if !self.palch_start(mem, d) { return; } }
                0x09 => { self.palch = [PalCh::default(); 12]; }
                0x0A => { let d = self.objs[i].script.wrapping_add(w as i16 as i32 as u32); if !self.tilech_start(mem, d) { return; } }
                0x0B => {}
                0x0C | 0x0D | 0x0E | 0x0F => {
                    let sel = if b[1] != 0 { 1 } else { 0 };
                    match b[0] {
                        0x8C => { self.scroll_mode(1); self.scroll.delta[sel * 2] = (w as i16 as i32) << 16; }
                        0x8D => { self.scroll_mode(1); self.scroll.delta[sel * 2 + 1] = (w as i16 as i32) << 16; }
                        0x8E => { self.scroll_mode(2); self.scroll.vel[sel * 2] = w as i16; }
                        _ => { self.scroll_mode(2); self.scroll.vel[sel * 2 + 1] = w as i16; }
                    }
                }
                0x10..=0x13 => { self.objs[i].flags |= 1; self.tree_flags(i, true); } // ops 90–93 (actions.md §8.2)
                0x14 => { let s = slot(b[1]); self.objs[s].flags &= 2; self.tree_flags(s, false); }
                0x15 => { let s = slot(b[1]); self.objs[s] = Obj::default(); }
                0x16 => { let s = slot(b[1]); self.objs[s].flags = 3; }
                0x17 => { self.objs[i].flags |= 2; self.set_delay(i, &b); return; }
                0x18 => { self.objs[i].f4e |= 2; }
                0x19 | 0x1A => { self.objs[i].wait_time = u32::from_be_bytes([b[1], b[2], b[3], 0]); self.objs[i].flags |= 4; }
                0x1B => { self.objs[i].f4e |= 1; self.f788b = b[1]; }
                0x1C => { self.objs[i].f4e |= 0x80; self.f788b = self.objs[i].f4e; }
                0x23 => { self.f7866 = 1; }
                0x24 => { self.f788a |= 1; }
                0x2E => { self.f788a |= 2; }
                0x1D => { self.events.push(Event::Sound { id: b[1], kind: 0 }); }
                // op 9F (S:0xFC0C): `$7890 == 0` and no voice ($1511E & 7 == 0) -> subfn_06(arg); executed once per script step (oracle, Alton plaza: one
                // call, effect 0x1B, `$1511E` bit 3 set for the sample length). The old `frame & 7` gating skipped the effect most of the time.
                0x1F => { self.events.push(Event::Periodic { id: b[1] }); }
                0x20 => { self.objs[i].flip = b[3]; }
                0x22 => { self.objs[i].hold = 0; if self.vram_list.is_some() { self.objs[i].hold = 1; return; } }
                0x27 => { self.objs[i].pal_override = b[1]; }
                0x28 => { self.scroll_mode(b[1]); }
                0x29 => { self.objs[i].hold = 0; if (self.frame & 2) == 0 { self.objs[i].hold = 1; return; } }
                0x2B => { self.split_req = b[1]; }
                0x2C => { self.objs[i].fast = 1; }
                0x2D => { self.events.push(Event::Sound { id: b[1], kind: 1 }); }
                0x2F => { self.objs[i].group = b[1]; }
                _ => { /* 0x11-13,0x1A,0x1E,0x21,0x23-26,0x2A,0x2B,0x2E,0x30,0x31: no visual effect modelled */ }
            }
        }
    }

    /// S:0xFD68: frame entry `[frame][rnd][b2][b3]`: rnd==0 → delay = word(b2,b3),
    /// else a random delay in [b2, b3) (S:0xFD8C).
    fn set_delay(&mut self, i: usize, b: &[u8; 4]) {
        self.objs[i].delay = if b[1] == 0 {
            u16::from_be_bytes([b[2], b[3]]) as i16
        } else {
            self.rand_range(b[2] as u16, (b[3] as u16).wrapping_sub(b[2] as u16)) as i16
        };
    }

    fn move_tree(&mut self, i: usize, dx: i32, dy: i32) {
        self.objs[i].x = self.objs[i].x.wrapping_add(dx << 16);
        self.objs[i].y = self.objs[i].y.wrapping_add(dy << 16);
        self.objs[i].delay = -1;
        let mut c = self.objs[i].child as usize;
        let mut guard = 0;
        while c != 0 && guard < 64 {
            self.objs[c].x = self.objs[c].x.wrapping_add(dx << 16);
            self.objs[c].y = self.objs[c].y.wrapping_add(dy << 16);
            c = self.objs[c].sibling as usize;
            guard += 1;
        }
    }

    fn prop_vel(&mut self, i: usize) {
        let (vx, vy) = (self.objs[i].vx, self.objs[i].vy);
        let mut c = self.objs[i].child as usize;
        let mut guard = 0;
        while c != 0 && guard < 64 {
            self.objs[c].vx = self.objs[c].vx_set.wrapping_add(vx);
            self.objs[c].vy = self.objs[c].vy_set.wrapping_add(vy);
            c = self.objs[c].sibling as usize;
            guard += 1;
        }
    }

    fn tree_flags(&mut self, i: usize, pause: bool) {
        let mut c = self.objs[i].child as usize;
        let mut guard = 0;
        while c != 0 && guard < 64 {
            if pause { self.objs[c].flags |= 1 } else { self.objs[c].flags &= 2 }
            c = self.objs[c].sibling as usize;
            guard += 1;
        }
    }

    /// `subfn_13` S:0xF3F8 (scroll mode / screen-effect request).
    /// FF/FC: reset scroll state, then (next tick) send FF/FC to the main CPU's second effect
    /// channel; FD or <7: `$8F0B` (second channel: 1/2 = sub-driven scroll, 3–6 waves);
    /// <3 also sets the scroll mode (1 = absolute once); ≥7: `$8F0A` (raster effect channel).
    pub fn scroll_mode(&mut self, m: u8) {
        match m {
            0xFF | 0xFC => { self.scroll = Scroll::default(); self.fx_a = 0; self.fx_b = 0; self.fx_reset = m; }
            0xFD | 0..=6 => {
                self.fx_b = m;
                if m < 3 { self.scroll.mode = if m == 1 { 0x0101 } else { m as u16 }; } else { self.scroll.mode = 0; }
            }
            _ => self.fx_a = m,
        }
    }

    /// `subfn_22` S:0xF446 / S:0xF47C. Fills the one-shot mailbox `mail` (S:$B6408/$B640C).
    pub fn scroll_tick(&mut self) {
        // Delivery to the main CPU is one frame late: oracle (fid_watch.py): `scroll_mode(0x0E)` is requested by the
        // sub thread in the frame where `$7A25` becomes 3 and the main `$E060` changes in the NEXT frame.
        self.mail = std::mem::take(&mut self.mail_next);
        if self.fx_reset != 0 {
            self.mail_next = (0, self.fx_reset);
            self.fx_reset = 0;
            return;
        }
        if self.fx_a != 0 { self.mail_next.0 = self.fx_a; }
        if self.fx_b != 0 { self.mail_next.1 = self.fx_b; }
        self.fx_a = 0; self.fx_b = 0;
        if self.scroll.mode == 0 { return; }
        let d = std::mem::take(&mut self.scroll.delta);
        let absolute = self.scroll.mode >> 8 == 1;
        if absolute { self.scroll.mode &= 0xFF; }
        for k in 0..4 {
            let base = if absolute { 0 } else { self.scroll.pos[k] };
            let v = (self.scroll.vel[k] as i32) << 8;
            self.scroll.pos[k] = base.wrapping_add(d[k]).wrapping_add(v);
        }
    }

    /// S:0xED28 sat_build: objects → sprite list (link order = slot order).
    pub fn sat_build(&mut self, mem: &SubMem, vdp: &mut Vdp) {
        let mut out: Vec<Sprite> = Vec::with_capacity(80);
        for i in 0..64 {
            let o = self.objs[i];
            if o.active == 0 || o.frame == 0 || o.flags & 2 != 0 { continue; }
            if o.blink_mask == 0xFFFF || (o.blink_mask & self.frame) != 0 { continue; }
            let f = o.frame;
            let cnt = mem.u16(f) as usize + 1;
            let mut y0 = (o.y >> 16) + 0x80;
            let mut x0 = (o.x >> 16) + 0x80;
            if o.flip & 4 != 0 { x0 += self.scroll.pos[0] >> 16; y0 += self.scroll.pos[1] >> 16; }
            if o.flip & 8 != 0 { x0 += self.scroll.pos[2] >> 16; y0 += self.scroll.pos[3] >> 16; }
            let mut p = f + 2;
            for _ in 0..cnt {
                let mut yo = mem.i16(p) as i32;
                let sz = mem.u16(p + 2);
                let pr = o.prio & 0xFC;
                let mut attr = mem.u16(p + 4) | pr.rotate_right(3);
                let mut xo = mem.i16(p + 6) as i32;
                p += 8;
                if o.flip & 2 != 0 { yo = -yo - ((((sz >> 5) & 0x18) as i32) + 8); }
                if o.flip & 1 != 0 { xo = -xo - ((((sz >> 7) & 0x18) as i32) + 8); }
                attr ^= ((o.flip as u16) & 3) << 11;
                if o.pal_override & 0x80 != 0 { attr = (attr & 0x9FFF) | (((o.pal_override & 3) as u16) << 13); }
                out.push(Sprite { y: (y0 + yo) as i16, size: ((sz >> 8) & 0xF) as u8, link: 0, attr, x: (x0 + xo) as i16 });
                if out.len() >= 64 { break; }
            }
            if out.len() >= 64 { break; }
        }
        for k in 0..out.len() { out[k].link = if k + 1 < out.len() { (k + 1) as u8 } else { 0 }; }
        vdp.sprites = out;
    }

    /// `subfn_15` S:0xE81A: start a tile-stream channel; descriptor `[kind][_]` + entries.
    pub fn tilech_start(&mut self, mem: &SubMem, desc: u32) -> bool {
        for c in self.tilech.iter_mut() {
            if c.kind == 0 {
                *c = TileCh { kind: mem.u8(desc), step: 0, ptr: desc + 2 };
                return true;
            }
        }
        false
    }

    /// S:0xE846 tilech_run (the caller skips it while a VRAM list is pending).
    pub fn tilech_run(&mut self, mem: &SubMem, vdp: &mut Vdp) {
        for k in 0..7 {
            let c = self.tilech[k];
            if c.kind == 0 { continue; }
            let buf = ((c.kind >> 4) & 3) as usize;
            // NOTE S:0xE8F2 & co wait for `wr_dmaq` to be empty when the buffer index is 0 (`move.b d1,d0; bne go;
            // tst.w wr_dmaq; bne skip`). Modelling that as "skip for 2 frames after any push" starves the channels
            // (options screen lists get dropped); the oracle shows the panel built ~2 frames slower than the
            // unthrottled model, which is accepted as an approximation (docs/re/modules.md §4).
            let send = |rt: &mut Runtime, vdp: &mut Vdp, e: u32| {
                rt.last_dma = rt.frame;
                // S:0xE8C6: DMA buffer + off(+6) → VRAM (+4), len (+8) bytes.
                let (vram, off, len) = (mem.u16(e + 4), mem.u16(e + 6) as usize, mem.u16(e + 8) as usize);
                mem.trace_ev(crate::gfx::TraceEv::TileSend { buf, vram, off, len });
                let end = (off + len).min(0x2000);
                let data = rt.tilebuf[buf][off.min(end)..end].to_vec();
                vdp.write_vram(vram, &data);
            };
            let decomp = |rt: &mut Runtime, e: u32| {
                if let Ok((out, _)) = mem.decomp(mem.u32(e) & 0xFF_FFFF) {
                    let n = out.len().min(0x2000);
                    rt.tilebuf[buf][..n].copy_from_slice(&out[..n]);
                }
            };
            // Dispatch S:0xE89C: `movea.l 0(pc,d0.w)` with d0 = (kind&0xF)*4 and the PC base at the
            // extension word (S:0xE89E), so kind n uses table slot n-1 (S:0xE8A2): 1 → S:0xE8F2,
            // 2 → S:0xE93C, 3 → S:0xE986, 4 → S:0xE9DC, 5 → S:0xEA88. Kind 0 never occurs.
            match (c.kind & 0xF).wrapping_sub(1) {
                0 => { // one entry per frame, S:0xE8F2
                    let e = c.ptr;
                    if c.kind & 0x80 == 0 { decomp(self, e); }
                    send(self, vdp, e);
                    self.tilech[k].ptr = e + 10;
                    if mem.u16(e + 10) == 0xFFFF { self.tilech[k].kind = 0; }
                }
                1 => { // raw copies, all entries, S:0xE93C
                    let mut e = c.ptr;
                    for _ in 0..256 {
                        let (src, off, len) = (mem.u32(e) & 0xFF_FFFF, mem.u16(e + 6) as usize, mem.u16(e + 8) as usize);
                        for j in 0..len { if off + j < 0x2000 { self.tilebuf[buf][off + j] = mem.u8(src + j as u32); } }
                        send(self, vdp, e);
                        e += 10;
                        if mem.u16(e) == 0xFFFF { break; }
                    }
                    self.tilech[k].kind = 0;
                }
                2 => { // first entry 10 bytes, then 6-byte [vram][off][len] reusing the buffer, S:0xE986
                    let mut e = c.ptr;
                    if c.kind & 0x80 == 0 { decomp(self, e); }
                    for _ in 0..256 {
                        send(self, vdp, e);
                        if mem.u16(e + 10) == 0xFFFF { break; }
                        e += 6;
                    }
                    self.tilech[k].kind = 0;
                }
                3 | 4 => { // dissolve (S:0xE9DC / S:0xEA88), 65 steps
                    let e = c.ptr;
                    if c.kind & 0xF == 4 { // kind 4 = dissolve-in (S:0xE9DC)
                        if c.kind & 0x80 == 0 { decomp(self, e); self.tilech[k].kind |= 0x80; }
                        let st = c.step as usize;
                        let d1 = st & 7;
                        let idx = ((((st >> 3) * 5) & 7) + d1) & 7;
                        const MASK: [u8; 16] = [0x02, 0xF0, 0x03, 0x0F, 0x01, 0xF0, 0x03, 0xF0, 0x00, 0xF0, 0x02, 0x0F, 0x00, 0x0F, 0x01, 0x0F];
                        let boff = d1 * 4 + MASK[idx * 2] as usize;
                        let m = MASK[idx * 2 + 1];
                        let base = mem.u16(e + 6) as usize + boff;
                        let tiles = (mem.u16(e + 8) as usize) >> 5;
                        for t in 0..tiles { let a = base + t * 32; if a < 0x2000 { self.tilebuf[buf][a] &= m; } }
                        send(self, vdp, e);
                    }
                    self.tilech[k].step += 1;
                    if self.tilech[k].step >= 0x41 { self.tilech[k].kind = 0; }
                }
                _ => { self.tilech[k].kind = 0; }
            }
        }
    }

    /// S:0xE34E vram_list_tick: one upload per frame after its delay.
    pub fn vram_list_tick(&mut self, mem: &SubMem, vdp: &mut Vdp) {
        // S:0xE34E: `$7887` handshake first (1 → request main plane clear, 2 → done), nothing else this frame
        if self.f7887 != 0 { self.f7887 = if self.f7887 == 1 { 2 } else { 0 }; return; }
        let Some(p) = self.vram_list else { return };
        if self.vram_delay != 0 { self.vram_delay -= 1; return; }
        if mem.u16(p) == 0xFFFF { self.vram_list = None; return; }
        // `$7884` reload (S:0xE3A2); the next entry also waits for the main CPU to drain the DMA queue
        // (`tst.w wr_dmaq`, S:0xE38A): at least one idle frame after each upload (title scene 0: 9 entries
        // take 19 frames in the oracle)
        self.vram_delay = mem.u16(p + 4).max(1);
        self.last_dma = self.frame;
        let src = mem.u32(p) & 0xFF_FFFF;
        let dst = mem.u16(p + 6);
        // subfn_07 S:0xF326 → lz_to_wordram S:0xF32C: decompress into Word RAM S:0xB8000
        // (= tile-stream buffer 0, later re-sent by channels with bit 7 set), then DMA to VRAM.
        if let Ok((out, _)) = mem.decomp(src) {
            for (k, chunk) in out.chunks(0x2000).enumerate().take(3) {
                self.tilebuf[k][..chunk.len()].copy_from_slice(chunk);
            }
            mem.trace_ev(crate::gfx::TraceEv::VramUp { dst, len: out.len() });
            vdp.write_vram(dst, &out);
        }
        self.vram_list = Some(p + 8);
    }

    /// One display frame in the game's order. `logic_ran` mirrors the main loop gating.
    pub fn tick(&mut self, mem: &SubMem, vdp: &mut Vdp) {
        if self.vram_list.is_none() && self.f7887 == 0 { self.obj_update_all(mem); }
        self.scroll_tick();
        self.sat_build(mem, vdp);
        if self.vram_list.is_none() { self.palch_run(mem); } // S:0xE4B4: `tst.b $7886; bne rts`
        if self.vram_list.is_none() { self.tilech_run(mem, vdp); }
        self.vram_list_tick(mem, vdp);
        // Palette shadow → CRAM (M:0xFF32AC copies on dirty bits; we copy every frame).
        vdp.cram.copy_from_slice(&self.shadow[..64]);
        vdp.cram_b.copy_from_slice(&self.shadow[64..]);
        // Scroll → VDP (M:0xFF3CC6: VSRAM = -COMSTAT4/6, hscroll = COMSTAT3/5).
        vdp.hscroll_a = (self.scroll.pos[0] >> 16) as i16;
        vdp.vscroll_a = -((self.scroll.pos[1] >> 16) as i16);
        vdp.hscroll_b = (self.scroll.pos[2] >> 16) as i16;
        vdp.vscroll_b = -((self.scroll.pos[3] >> 16) as i16);
        // 3 frames: main `$E036` is set one frame after `$7891` and the raster split shows two frames later (oracle,
        // fid_watch `m:E036w` and screens: voice portrait window white from frame N+3/N+4)
        self.split_out = self.split_next[3];
        self.split_next = [std::mem::take(&mut self.split_req), self.split_next[0], self.split_next[1], self.split_next[2]];
        self.frame = self.frame.wrapping_add(1);
    }
}

/// Frame data address: `anim_table + word(anim_table + 2*frame)` (S:0xF886).
pub fn frame_addr(mem: &SubMem, table: u32, frame: u8) -> u32 {
    table + mem.u16(table + 2 * frame as u32) as u32
}

/// One fade step of S:0xE7A8 (each component moves by one level toward the target).
pub fn fade_toward(cur: u16, t: u16) -> u16 {
    let step = |c: u16, t: u16, unit: u16| -> u16 {
        if c == t { c } else if c > t { c - unit } else { c + unit }
    };
    step(cur & 0xF, t & 0xF, 2) | step(cur & 0xF0, t & 0xF0, 0x20) | step(cur & 0xF00, t & 0xF00, 0x200)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::{self, SubMem, Vdp};

    #[test]
    fn rng_matches_68k_sequence_shape() {
        let mut rt = Runtime::new();
        let a = rt.rand();
        let b = rt.rand();
        assert_ne!(a, b);
    }

    #[test]
    fn fade_step() {
        assert_eq!(fade_toward(0x000, 0xEEE), 0x222);
        assert_eq!(fade_toward(0xE0E, 0x000), 0xC0C);
        assert_eq!(fade_toward(0x246, 0x246), 0x246);
    }

    /// Runs every scene of every DATA file for 400 frames (needs the locally extracted
    /// assets/raw; skipped when absent). Checks the runtime never panics and terminates lists.
    #[test]
    fn all_scenes_run() {
        let raw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw/files");
        let Ok(sub) = std::fs::read(raw.join("SUBCODE.BIN")) else { eprintln!("assets missing, skipped"); return };
        let mut files: Vec<_> = std::fs::read_dir(&raw).unwrap().filter_map(|e| e.ok()).map(|e| e.path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("DATA_")).collect();
        files.sort();
        let mut fb = vec![0u8; gfx::SCREEN_W * gfx::SCREEN_H * 4];
        let mut scenes = 0;
        for f in files {
            let bytes = std::fs::read(&f).unwrap();
            let mut mem = SubMem::new();
            mem.map(gfx::SUBCODE_BASE, sub.clone());
            mem.map(gfx::DATA_BASE, bytes.clone());
            let hdr = gfx::data_header(&bytes).unwrap();
            for n in 0..gfx::scene_count(&mem, hdr.scene_table) {
                let (mut vdp, mut rt) = (Vdp::default(), Runtime::new());
                rt.load_scene(&mem, gfx::scene_addr(&mem, hdr.scene_table, n));
                for _ in 0..400 { rt.tick(&mem, &mut vdp); }
                vdp.render(&mut fb);
                assert!(rt.vram_list.is_none(), "{f:?} scene {n}: VRAM list did not finish");
                scenes += 1;
            }
        }
        eprintln!("{scenes} scenes ran");
        assert!(scenes > 1000);
    }
}
