//! DATA_D2 title module (entry S:0x285CE) with the main-CPU boot screens in front of it.
//!
//! Boot (`boot::Boot`, main CPU IP code M:0xFF2B66/0xFF2BE4): copyright notice, Konami laser
//! logo. Then the sub CPU's mode machine (mode 1, S:0xE046) calls the D2 code once per sub frame:
//! state `$7896` / sub-state `$7898` / timer `$784E` as in docs/re/actions.md §9.1 and
//! docs/re/modules.md §4 (every state below is quoted from re/out/dis/modules_D2.asm).
//!
//! Internal backup RAM only: the cartridge device screen (state 0 sub 1, `$B6502`) and the
//! "no backup RAM" / "memory full" message screens (states 6 and 8) are never reached because
//! the internal RAM is always formatted and has room (`$CDAD` = 1, `$CDAF` = 1).
use super::boot::Boot;
use super::SubServices;
use crate::game::{Engine, Module, Out};
use crate::touch::TouchUi;

const D2_STATE_TABLE_A: u32 = 0x29142; // slot-select build-up (21 lists), second lists at B
const D2_STATE_TABLE_B: u32 = 0x29412;
const D2_STATE_TABLE_C: u32 = 0x296C6; // slot-select close (20 lists)
const D2_CANCEL_LIST: u32 = 0x29806;
const D2_OPT_TABLE_A: u32 = 0x29292; // options build-up (24 lists), second lists at B
const D2_OPT_TABLE_B: u32 = 0x29554;
const D2_FADE_IN: u32 = 0x288E6; // 32-frame title palette fade-in
const D2_FADE_OUT2: u32 = 0x290F2; // `83 00 00 3f`

/// Frames between the boot screens finishing (`$E022` = 2, oracle frame 991) and the first call
/// of the D2 code (oracle frame 997): main `$E022` 2 → 3, sub mode 0 → 1 (`sub_0150F4(0x1E)`,
/// PCM idle wait) → 2.
const MODE1_GAP: u8 = 3;

pub struct Title {
    boot: Option<Boot>,
    boot_tried: bool,
    gap: u8,
    /// `$28ADB`: previous raw-pressed byte (debounce at the entry)
    prev: u8,
    /// `$7896` / `$7898` / `$784E`
    st: u16,
    sub: u16,
    timer: u16,
    /// `$28AD8`: options cursor row 0..4
    sel: u16,
    /// `$28ADA`: slot select was cancelled (B)
    cancelled: bool,
    /// frame counter inside the module (debug)
    frames: u32,
    /// first 8 bytes of each save block (S:0x28644 table)
    slots: Option<[[u8; 8]; 4]>,
    /// calibration screen object variables (objects 17..19: `+$3A` mode, `+$3C` counter, `+$3E` blinks, `+$40` shown)
    cal: [[u16; 4]; 3],
    /// `$7850` (calibration screen timeout)
    wait50: u16,
}

impl Title {
    pub fn new() -> Self {
        Title { boot: None, boot_tried: false, gap: 0, prev: 0, st: 0, sub: 0, timer: 0, sel: 0, cancelled: false, frames: 0, slots: None, cal: [[0; 4]; 3], wait50: 0 }
    }

    // ------------------------------------------------------------------ helpers

    fn next_state(&mut self) { self.st += 1; self.sub = 0; }   // sub_0287BE
    fn next_sub(&mut self) { self.sub += 1; }                   // sub_0287C8

    /// `subjt_15` with the descriptor `table + word(table + off)`; returns false when no slot is free.
    fn tile_list(e: &mut Engine, table: u32, off: u32) {
        let a = table + e.mem.u16(table + off) as u32;
        e.sub_tile(a);
    }

    fn obj_flags(e: &mut Engine, n: usize, f: u8) { e.disp.sub.objs[n].flags = f; }
    fn obj_idx(e: &mut Engine, n: usize, v: u16) { e.disp.sub.objs[n].idx = v; }
    fn obj_idx_get(e: &Engine, n: usize) -> u16 { e.disp.sub.objs[n].idx }

    /// `sub_02902C` / `loc_02904A`: sound mode request (`$78A2` = 1 → mono F3/FC else stereo F4/FD).
    fn sound_mode(e: &mut Engine, mono: bool) {
        let (fm, pcm) = if mono { (0xF3, 0xFC) } else { (0xF4, 0xFD) };
        e.out.push(Out::Fm { id: fm, music: false });
        e.sound06(pcm);
    }

    /// `sub_0290E2`: 10-frame fade out (palette channel `83 00 00 3F`).
    fn fade_out(&mut self, e: &mut Engine) { self.timer = 0x0A; e.sub_pal(D2_FADE_OUT2); }

    // ------------------------------------------------------------------ slots (sub_028664, sub_028D26)

    /// `sub_028664`: BRMSERCH/BRMREAD of "SNATCHER_01".."04" → `$78B6` bits, `$79E2[]`, `$78B7`.
    fn read_slots(e: &mut Engine) -> [[u8; 8]; 4] {
        let mut bits = 0u8;
        let mut tab = [[0u8; 8]; 4];
        e.regs.set_b(0x78B7, 0);
        for i in (0..4).rev() {
            e.regs.set(0x79E2 + 2 * i as u32, 0);
            if let Some(b) = e.saves[i].as_ref() {
                e.regs.set_b(0x78B7, i as u8);
                bits |= 1 << i;
                e.regs.set(0x79E2 + 2 * i as u32, 1);
                for k in 0..8 { tab[i][k] = b.get(k).copied().unwrap_or(0); }
            }
        }
        e.regs.set_b(0x78B6, bits);
        tab
    }

    /// `sub_028D26`: slot panel nametable words into Word RAM buffer 0 (S:0xB8256 + 0x100·slot).
    fn draw_slots(e: &mut Engine, tab: &[[u8; 8]; 4]) {
        fn put(e: &mut Engine, off: usize, w: u16) {
            let b = &mut e.disp.sub.tilebuf[0];
            if off + 1 < b.len() { b[off] = (w >> 8) as u8; b[off + 1] = w as u8; }
        }
        for k in 0..4usize {
            let a0 = 0x256 + 0x100 * k;
            if e.regs.get(0x79E2 + 2 * k as u32) == 0 {
                // "empty": 6 tiles 0x459C..0x45A5
                for (o, w) in [(0, 0x459Cu16), (2, 0x459D), (4, 0x459E), (6, 0x459F), (8, 0x45A0), (0x40, 0x45A1), (0x42, 0x45A2), (0x44, 0x45A3), (0x46, 0x45A4), (0x48, 0x45A5)] { put(e, a0 + o, w); }
                continue;
            }
            let t = tab[k];
            let d = t[2] & 0xF0;
            if d != 0 { let w = (d as u16 >> 3) + 0x4580; put(e, a0, w); put(e, a0 + 0x40, w + 1); }
            let w = ((t[2] & 0x0F) as u16) * 2 + 0x4580; put(e, a0 + 2, w); put(e, a0 + 0x42, w + 1);
            put(e, a0 + 4, 0x4594); put(e, a0 + 0x44, 0x4595);
            let w = ((t[3] & 0xF0) as u16 >> 3) + 0x4580; put(e, a0 + 6, w); put(e, a0 + 0x46, w + 1);
            let w = ((t[3] & 0x0F) as u16) * 2 + 0x4580; put(e, a0 + 8, w); put(e, a0 + 0x48, w + 1);
            let w = u16::from_be_bytes([t[6], t[7]]) * 2 + 0x4596; put(e, a0 - 0x68, w); put(e, a0 - 0x28, w + 1);
            let d = t[0] & 0xF0;
            if d != 0 { let w = (d as u16 >> 3) + 0x4580; put(e, a0 + 0x16, w); put(e, a0 + 0x56, w + 1); }
            let w = ((t[0] & 0x0F) as u16) * 2 + 0x4580; put(e, a0 + 0x18, w); put(e, a0 + 0x58, w + 1);
        }
    }

    // ------------------------------------------------------------------ options (sub_028F9C / sub_029066)

    /// `sub_028F9C`: left/right handling of the current row.
    fn option_row(&mut self, e: &mut Engine, raw: u8) {
        if e.regs.get_b(0x7983) != 1 {
            e.regs.set(0x78B8, 0);
            if self.sel == 3 { self.sel = 4; }
        }
        match self.sel {
            0 => if e.regs.get_b(0x7983) == 1 && raw & 0x0C != 0 { let v = e.regs.get(0x78B8) ^ 1; e.regs.set(0x78B8, v); },
            1 => {
                let mut c = e.regs.get(0x78BA) as i32;
                if raw & 8 != 0 { c += 1; if c == 6 { c = 0; } else { self.set_cfg(e, c as u16); return; } }
                if raw & 4 != 0 { c -= 1; if c < 0 { c = 5; } }
                self.set_cfg(e, c as u16);
            }
            2 => if raw & 0x0C != 0 {
                let v = e.regs.get(0x78A2) ^ 1;
                e.regs.set(0x78A2, v);
                Self::sound_mode(e, v != 0);
            },
            _ => {}
        }
    }

    fn set_cfg(&self, e: &mut Engine, c: u16) {
        e.regs.set(0x78BA, c);
        if let Some(i) = e.input.as_mut() { i.config = c as usize; }
    }

    /// `sub_029066`: cursor / value objects (objects 32..36, script index `+$28`).
    fn option_objects(&self, e: &mut Engine) {
        let sel = self.sel;
        Self::obj_idx(e, 32, sel * 4);
        let mut d = if sel != 0 { 2 } else { 0 };
        d += e.regs.get(0x78B8);
        Self::obj_idx(e, 33, d * 2);
        Self::obj_idx(e, 36, (d * 2) & 2);
        let mut d = if sel == 1 { 0 } else { 6 };
        d += e.regs.get(0x78BA);
        Self::obj_idx(e, 34, d * 2);
        let mut d = if sel == 2 { 0 } else { 2 };
        d += e.regs.get(0x78A2);
        Self::obj_idx(e, 35, d * 2);
    }

    // ------------------------------------------------------------------ the module

    fn run(&mut self, e: &mut Engine) -> Option<i8> {
        // entry debounce (S:0x285CE): a non-zero identical new-press byte is ignored
        let raw = e.raw_pressed();
        let prev = self.prev;
        self.prev = raw;
        if raw != 0 && prev == raw { return None; }
        match self.st {
            0 => match self.sub {
                0 => {
                    // sound mode from `$78A2` (zero at power-on), `$9C00` = 0, BRMINIT (`$CDAD` = 1)
                    let mono = e.regs.get(0x78A2) != 0;
                    Self::sound_mode(e, mono);
                    self.next_sub();
                }
                _ => self.next_state(), // `$B6502` = 0 (no RAM cartridge)
            },
            1 => match self.sub {
                0 => { self.next_sub(); e.cd_play(2); }
                _ => { if e.cd_status() == super::CD_PLAYING { self.next_state(); } }
            },
            2 => {
                e.sub_scene_reset();
                e.sub_scroll(1);
                e.disp.sub.scroll.delta[1] = 0x100 << 16; // `$8F1C = $100`
                e.sub_scene(0);
                self.timer = 0x18;
                self.next_state();
            }
            3 => {
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 { e.sub_scroll(7); self.timer = 4; self.next_state(); }
            }
            4 => {
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 { e.sub_pal(D2_FADE_IN); self.timer = 0x56; self.next_state(); }
            }
            5 => {
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 { self.next_state(); }
            }
            6 => self.next_state(), // `$CDAD` != 0: backup RAM formatted
            7 => {
                if self.sub == 0 { self.slots = Some(Self::read_slots(e)); self.next_sub(); }
                else {
                    self.next_state();
                    if e.regs.get_b(0x78B6) != 0 { self.next_state(); } // saves exist: skip the free-space state
                }
            }
            8 => self.next_state(), // enough free blocks (`$CDAF` = 1): skip the message
            9 => self.state9(e, raw),
            10 => self.state10(e, raw),
            11 => return self.state11(e, raw),
            12 => {
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 {
                    self.st = 0; self.sub = 0;
                    e.sub_scene_reset();
                    return Some(1); // `$7846 = 1`
                }
            }
            _ => {}
        }
        None
    }

    /// State 9: title menu NEW GAME / CONTINUE.
    fn state9(&mut self, e: &mut Engine, raw: u8) {
        match self.sub {
            0 => {
                e.sub_scroll(0xFF);
                if e.regs.get_b(0x78B6) != 0 {
                    e.disp.sub.objs[17].flags &= 2;
                    self.sub = 1;
                } else {
                    e.disp.sub.objs[16].flags &= 2;
                    self.sub = 2;
                }
            }
            1 => {
                if raw & 3 != 0 { e.disp.sub.objs[17].flags &= 2; }
                if raw & 0x80 == 0 { return; }
                e.regs.set(0x785C, 1);
                Self::obj_flags(e, 17, 3);
                let new_game = Self::obj_idx_get(e, 17) == 3;
                if new_game { e.regs.set(0x785C, 0); self.st += 1; }
                self.confirm(e);
            }
            _ => {
                if raw & 0x80 == 0 { return; }
                e.regs.set(0x785C, 0);
                self.st += 1;
                self.confirm(e);
            }
        }
    }

    /// loc_028AAC: confirm sound (FM 0x38 with `$980F`), reset the slot-select variables, next state.
    fn confirm(&mut self, e: &mut Engine) {
        e.fm_cmd(0x38, false);
        self.timer = 0;
        self.sel = 0;
        Self::obj_flags(e, 16, 3);
        Self::obj_flags(e, 17, 3);
        self.next_state();
    }

    /// State 10: slot select (CONTINUE).
    fn state10(&mut self, e: &mut Engine, raw: u8) {
        match self.sub {
            0 => {
                if self.timer == 0x15 { self.slot_interactive(e, raw); return; }
                if self.timer == 3 {
                    let tab = self.slots.unwrap_or_default();
                    Self::draw_slots(e, &tab);
                }
                let off = 2 * self.timer as u32;
                self.timer += 1;
                Self::tile_list(e, D2_STATE_TABLE_A, off);
                if off != 0 { Self::tile_list(e, D2_STATE_TABLE_B, off); }
            }
            1 => {
                if self.timer == 0x14 {
                    self.timer = 0;
                    if !self.cancelled { self.next_state(); return; }
                    self.next_sub();
                    self.timer = 2;
                    e.sub_tile(D2_CANCEL_LIST);
                    return;
                }
                let off = 2 * self.timer as u32;
                self.timer += 1;
                Self::tile_list(e, D2_STATE_TABLE_C, off);
            }
            2 => {
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 {
                    self.st -= 1; // back to the title menu
                    self.sub = 1;
                    e.disp.sub.objs[17].idx = 0;
                    e.disp.sub.objs[17].flags = 1;
                }
            }
            _ => {
                // error screen (scene 4): START → loc_02860E (sub flag bit 7), not reachable here
            }
        }
    }

    fn slot_interactive(&mut self, e: &mut Engine, raw: u8) {
        Self::obj_flags(e, 24, 0);
        Self::obj_flags(e, 25, 0);
        let bits = e.regs.get_b(0x78B6);
        let mut slot = e.regs.get_b(0x78B7);
        // up/down: step to the next existing slot (the original loops until `btst` succeeds)
        for _ in 0..8 {
            if raw & 2 != 0 { slot = slot.wrapping_add(1); } else if raw & 1 != 0 { slot = slot.wrapping_sub(1); }
            slot &= 3;
            if bits & (1 << slot) != 0 { break; }
        }
        e.regs.set_b(0x78B7, slot);
        Self::obj_idx(e, 24, slot as u16 * 4);
        Self::obj_idx(e, 25, slot as u16 * 4);
        if raw & 0x80 != 0 {
            e.fm_cmd(0x38, false);
            self.cancelled = false;
            self.load_slot(e, slot as usize);
        } else if raw & 0x10 != 0 {
            self.cancelled = true;
            self.after_pick(e);
        }
    }

    /// loc_028BEC: restore the options saved in the block (`$9C0A`, `$9C08`).
    fn load_slot(&mut self, e: &mut Engine, slot: usize) {
        let Some(b) = e.saves[slot].clone() else { return };
        let o = b.get(0x0A).copied().unwrap_or(0);
        self.set_cfg(e, (o & 7) as u16);
        e.regs.set(0x78B8, (o >> 6 & 1) as u16);
        let mono = o & 0x80 != 0;
        e.regs.set(0x78A2, mono as u16);
        Self::sound_mode(e, mono);
        let cal = u16::from_be_bytes([b.get(8).copied().unwrap_or(0), b.get(9).copied().unwrap_or(0)]);
        e.regs.set(0x7988, cal);
        e.ix.gun_cal_done = cal != 0; // 0/0 = never calibrated
        self.after_pick(e);
    }

    fn after_pick(&mut self, e: &mut Engine) {
        self.timer = 0;
        Self::obj_flags(e, 24, 3);
        Self::obj_flags(e, 25, 3);
        self.next_sub();
    }

    /// State 11: settings screen (+ gun calibration).
    fn state11(&mut self, e: &mut Engine, raw: u8) -> Option<i8> {
        match self.sub {
            0 => {
                if self.timer == 0x18 { self.options_interactive(e, raw); return None; }
                let off = 2 * self.timer as u32;
                self.timer += 1;
                Self::tile_list(e, D2_OPT_TABLE_A, off);
                if off != 0 { Self::tile_list(e, D2_OPT_TABLE_B, off); }
            }
            1 => {
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 { e.sub_scene_reset(); e.sub_scene(7); self.next_sub(); }
            }
            2 => {
                // gun calibration screen init (S:0x29814): COMSTAT0 = 1 (main reads the gun), objects reset
                e.regs.set(0x7970, 1);
                self.timer = 0;
                Self::obj_flags(e, 16, 3);
                Self::obj_idx(e, 16, 0);
                self.cal = [[0; 4]; 3];
                for n in [17usize, 18, 19] { Self::obj_idx(e, n, 0); }
                self.next_sub();
            }
            3 => return self.calibration(e, raw),
            _ => {
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 {
                    if e.regs.get_b(0x7983) != 1 { e.regs.set(0x78B8, 0); self.sel = 0; }
                    e.sub_scene_reset();
                    e.sub_scene(8);
                    // st stays 11 (`subq.w #1,$7896` then sub_0287BE)
                    self.sub = 0;
                    self.timer = 0;
                }
            }
        }
        None
    }

    fn options_interactive(&mut self, e: &mut Engine, raw: u8) {
        for n in 32..=36usize { Self::obj_flags(e, n, 0); } // loc_028E88 (every frame)
        // row navigation (loc_028EA6): the original loops until the row is valid
        for _ in 0..8 {
            if raw & 2 != 0 { self.sel = (self.sel + 1) % 5; }
            else if raw & 1 != 0 { self.sel = if self.sel == 0 { 4 } else { self.sel - 1 }; }
            if self.sel == 3 && e.regs.get(0x78B8) == 0 { continue; }
            break;
        }
        self.option_row(e, raw);
        self.option_objects(e);
        if raw & 0x80 != 0 {
            if self.sel == 3 { self.to_calibration(e); } else { self.start_game(e); }
        } else if raw & 0x70 != 0 {
            if self.sel == 4 { self.start_game(e); } else if self.sel == 3 { self.to_calibration(e); }
        }
    }

    fn to_calibration(&mut self, e: &mut Engine) {
        e.sound06(0x3A);
        self.fade_out(e);
        self.next_sub();
    }

    fn start_game(&mut self, e: &mut Engine) {
        e.sound06(0x38);
        self.fade_out(e);
        self.next_state();
    }

    /// `sub_02985E`/`sub_02988A`: blink handling of the three target markers (objects 17..19).
    fn cal_blink(&mut self, e: &mut Engine) {
        for k in 0..3 {
            let n = 17 + k;
            Self::obj_flags(e, n, if self.cal[k][3] != 0 { 1 } else { 3 });
            if self.cal[k][0] == 1 {
                self.cal[k][1] += 1;
                if self.cal[k][1] >= 0x10 {
                    self.cal[k][1] = 0;
                    self.cal[k][3] ^= 1;
                    self.cal[k][2] += 1;
                    if self.cal[k][2] >= 6 { self.cal[k][0] = 0; }
                }
            }
        }
    }

    /// State 11 sub 3 (S:0x298B4): gun calibration; `$784E` is the step. Verified against the ORIGINAL with the oracle's light-gun
    /// model (re/oracle machine.py `m.gun`, docs/re/shooting.md): the trigger stores `$7988 = $7986 - 0x80`, `$7989 = $7987 - 0x80`
    /// (S:0x29930), the test crosshair is drawn at (`$798A` - 0x0B, `$798B` - 0x23) (S:0x299B2). The bullseye is centred at (128, 101).
    /// The Y calibration additionally carries `GUN_CAL_BIAS_Y` (see menus.rs) so that the aimed cell is the cell under the pointer.
    fn calibration(&mut self, e: &mut Engine, _raw: u8) -> Option<i8> {
        if _raw & 0x80 != 0 { self.cal_done(e); return None; }
        self.cal_blink(e);
        let trig = e.regs.get_b(0x7985) & 0x20 != 0;
        let aux = e.regs.get_b(0x7985) & 0x40 != 0;
        match self.timer {
            0 => { self.cal[1] = [1, 0, 0, 1]; self.timer += 1; } // (COMFLAG bit 5 answer is immediate)
            1 => if trig {
                e.sound06(0x39);
                self.cal[1][0] = 0; self.cal[1][3] = 0;
                self.cal[2][0] = 1; self.cal[2][3] = 1;
                self.timer += 1;
            },
            2 => {
                let (rx, ry) = (e.regs.get_b(0x7986), e.regs.get_b(0x7987));
                e.regs.set_b(0x7988, rx.wrapping_sub(0x80));
                e.regs.set_b(0x7989, ry.wrapping_sub(0x80).wrapping_add(crate::menus::GUN_CAL_BIAS_Y));
                e.ix.gun_cal_done = true;
                Self::obj_flags(e, 16, 1);
                crate::menus::set_x(e, 16, 0x75);
                crate::menus::set_y(e, 16, 0x5D);
                self.timer += 1;
            }
            3 => if aux {
                self.cal[2][0] = 0; self.cal[2][3] = 0;
                self.cal[0][0] = 1; self.cal[0][3] = 1;
                e.regs.set(0x7970, 0);
                self.timer += 2;
                e.sound12(0x1D, 0);
            } else if trig {
                e.sound06(0x39);
                self.timer += 1;
            },
            4 => {
                let x = e.regs.get_b(0x798A) as i32 - 0x0B;
                let y = e.regs.get_b(0x798B) as i32 + crate::menus::GUN_CAL_BIAS_Y as i32 - 0x23;
                crate::menus::set_x(e, 16, x);
                crate::menus::set_y(e, 16, y);
                self.timer -= 1;
            }
            _ => {
                self.wait50 += 1;
                if self.wait50 >= 0xB4 { self.cal_done(e); }
            }
        }
        None
    }

    fn cal_done(&mut self, e: &mut Engine) {
        e.regs.set(0x7970, 0);
        self.fade_out(e);
        self.next_sub();
    }
}

/// Screen rectangles of the baked pictures of the title module (game pixels, measured on the USA disc's scenes; the English and pt-BR
/// pictures share them): NEW GAME / CONTINUE, the four slot panels, the five rows of the options screen (the fifth is the QUIT button).
const TITLE_ITEMS: [(i32, i32, i32, i32); 2] = [(64, 158, 128, 18), (64, 176, 128, 18)];
const SLOT_PANEL: (i32, i32, i32, i32, i32) = (56, 60, 146, 27, 32); // x, y of slot 0, w, h, pitch
const OPTION_ROWS: [(i32, i32, i32, i32); 5] = [(40, 45, 176, 21), (40, 72, 176, 39), (40, 116, 176, 22), (40, 144, 176, 22), (96, 172, 64, 22)];

impl Module for Title {
    fn name(&self) -> &'static str { "title DATA_D2" }

    /// Touch description: the title menu, the slot screen, the options screen and the gun calibration are the interactive states; the rest
    /// (boot screens, fades, scene loads) is `None`.
    fn touch(&self, e: &Engine) -> Option<TouchUi> {
        use crate::touch::TRect;
        if self.boot.is_some() || self.gap > 0 { return None; }
        let rect = |id: i32, (x, y, w, h): (i32, i32, i32, i32)| TRect { x, y, w, h, id, kind: 0 };
        let mut u;
        match (self.st, self.sub) {
            (9, 1) | (9, 2) => {
                u = TouchUi::new("title");
                u.ready = true; u.start = true;
                u.rects.push(rect(0, TITLE_ITEMS[0]));
                if e.regs.get_b(0x78B6) != 0 { u.rects.push(rect(1, TITLE_ITEMS[1])); }
                // the cursor object 17 shows idx 3 on NEW GAME and 1 on CONTINUE (`state9` below reads idx == 3 as NEW GAME)
                u.cur = if self.sub == 1 && e.disp.sub.objs[17].idx != 3 { 1 } else { 0 };
            }
            (10, 0) if self.timer == 0x15 => {
                u = TouchUi::new("slots");
                u.ready = true; u.start = true; u.back = true;
                let bits = e.regs.get_b(0x78B6);
                for k in 0..4 { if bits & (1 << k) != 0 { u.rects.push(rect(k, (SLOT_PANEL.0, SLOT_PANEL.1 + SLOT_PANEL.4 * k, SLOT_PANEL.2, SLOT_PANEL.3))); } }
                u.cur = e.regs.get_b(0x78B7) as i32;
            }
            (11, 0) if self.timer == 0x18 => {
                u = TouchUi::new("options");
                u.ready = true; u.start = true;
                let gun = e.regs.get(0x78B8) != 0;
                for (i, r) in OPTION_ROWS.iter().enumerate() { if i != 3 || gun { u.rects.push(rect(i as i32, *r)); } } // GUN ADJUST exists only with the gun device on
                u.cur = self.sel as i32;
            }
            (11, 3) => { u = TouchUi::new("calibrate"); u.ready = true; u.start = true; }
            _ => return None,
        }
        Some(u)
    }

    fn tick(&mut self, e: &mut Engine) -> Option<i8> {
        self.frames += 1;
        if !self.boot_tried {
            self.boot_tried = true;
            self.boot = Boot::new(e);
            if self.boot.is_none() { self.gap = 0; }
        }
        if let Some(b) = self.boot.as_mut() {
            if b.tick(e) { self.boot = None; self.gap = MODE1_GAP; }
            return None;
        }
        if self.gap > 0 { self.gap -= 1; return None; }
        if e.mode_blocked() { return None; }
        let before = (self.st, self.sub);
        let r = self.run(e);
        if e.trace && before != (self.st, self.sub) { e.out.push(Out::Log(format!("title st {} sub {}", self.st, self.sub))); }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Top, FILE_NAMES};

    fn engine() -> Option<Engine> {
        let raw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw");
        if !raw.join("files/SUBCODE.BIN").exists() { eprintln!("assets missing, skipped"); return None; }
        let mut e = Engine::new();
        for n in FILE_NAMES.iter() {
            if *n == "PCMLD_01.BIN" { continue; }
            if let Ok(d) = std::fs::read(raw.join("files").join(n)) { e.load_file(n, d); }
        }
        if let (Ok(h), Ok(i)) = (std::fs::read(raw.join("header.bin")), std::fs::read(raw.join("ip.bin"))) {
            e.load_file("HEADER.BIN", h);
            e.load_file("IP.BIN", i);
        }
        Some(e)
    }

    fn run(e: &mut Engine, frames: u64, presses: &[(u64, u8)]) {
        for f in 1..=frames {
            let pad = presses.iter().find(|p| p.0 == f).map(|p| p.1).unwrap_or(0);
            e.tick(pad);
        }
    }

    /// Verified against the oracle (docs/re/modules.md §4): D2 state 9 (title menu) at frame 1139, START on
    /// NEW GAME opens the settings screen at 1301, START there starts the intro (DATA_D1, CD track 3).
    #[test]
    fn title_to_intro() {
        let Some(mut e) = engine() else { return };
        e.boot();
        run(&mut e, 1300, &[]);
        assert_eq!(e.top, Top::Title);
        let mut e = engine().unwrap();
        e.boot();
        run(&mut e, 2400, &[(1301, 0x80), (1612, 0x80)]);
        assert_eq!(e.top, Top::Intro, "the title must hand over to the intro");
        assert_eq!(e.data_file, Some(3), "DATA_D1 is mapped");
    }

    /// Without the IP image (picked files only) the copyright/logo screens are skipped, the title still runs.
    #[test]
    fn title_without_ip() {
        let Some(mut e) = engine() else { return };
        e.files.remove("IP.BIN");
        e.boot();
        run(&mut e, 400, &[(300, 0x80)]);
        assert_eq!(e.top, Top::Title);
    }

    /// CONTINUE: with a save block the default selection is CONTINUE, START opens the slot select and
    /// START on a slot hands the block to the game (`$785C`, `$78B7`).
    #[test]
    fn continue_slot_select() {
        let Some(mut e) = engine() else { return };
        let mut blk = vec![0u8; 0x1C0];
        blk[0] = 3; blk[2..6].copy_from_slice(&[1, 0x23, 0x45, 0x10]); blk[0x0A] = 0x05;
        blk[0x0C..0x0E].copy_from_slice(&0x3Fu16.to_be_bytes());
        e.saves[2] = Some(blk);
        e.boot();
        run(&mut e, 1199, &[]);
        assert_eq!(e.regs.get_b(0x78B6), 0b100, "slot 3 found");
        let mut f = 1199;
        let mut done = false;
        for _ in 0..700 {
            f += 1;
            // START: CONTINUE (1200), START: select the slot (1300), START again on the settings screen (1450)
            e.tick(if f == 1200 || f == 1300 || f == 1450 { 0x80 } else { 0 });
            if e.top == Top::Game { done = true; break; }
        }
        assert!(done, "CONTINUE must start the game (settings screen included)");
        assert_eq!(e.regs.get_b(0x78B7), 2);
        assert_eq!(e.input.as_ref().unwrap().config, 5, "button config restored from the save block");
        assert!(done, "CONTINUE must start the game (option screen included)");
    }
}
