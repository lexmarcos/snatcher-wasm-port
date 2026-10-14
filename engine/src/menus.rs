//! Player-interaction helpers around the command menu (docs/re/interact.md):
//! * the list-menu highlight bar (object 0) positioning (S:0x10F90),
//! * the text-entry menus – phone keypad (`$79FE == 2`) and alphabet keyboard
//!   (`$79FE == 3`) – with the original matching rules (S:0x113A0–0x11E28),
//! * the free "pointer" mode (`$79FE == 1`, S:0x10846 / S:0x10990),
//! * sequencer command 0x15 (built-in scene + grid position, S:0x1036C / S:0x1449A),
//! * light-gun input registers ($7972, $7985, $798A/$798B).
//!
//! The original runs these loops in the sub CPU's VM thread (they block until a result);
//! here each is a per-frame state machine driven from `Engine::menu_frame`.
use crate::game::{Engine, Out, VmPhase};
use sncore::actions::btn;
use sncore::gfx;

// ------------------------------------------------------------------ addresses (SUBCODE)
const KEYPAD_STREAM: u32 = 0x11454; // digits "1 2 3 / 4 5 6 / 7 8 9 / * 0 #" render stream
const GRID_PTRS: u32 = 0x114D4; // $79F6 per style (2: 0x114DC keypad, 3: 0x11512 keyboard)
const GRID_PARAMS: u32 = 0x1148E; // [$97F0 start cell, $97F1 stride] / +4: [$97F7 x0, $97F8 y0] per style
const KEY_ANIM: u32 = 0x115CA; // keyboard: obj62 script index per cell (key-press animation)
const LAYOUT_PAL: [u32; 2] = [0x1138C, 0x11382]; // cmd 22 palette channel: keypad / keyboard (S:0x1133A)
const DIAL_LEN: u32 = 0x11938; // direct dial: expected length by first digit (S:0x11916)
const PTR_MAP: u32 = 0x109E8; // pointer-mode region map, 36 bytes per row (S:0x1089C)
const BUILTIN_SCENES: u32 = 0x146CE; // `scene_load_builtin` table (S:0xF696)
const GRID_POS: u32 = 0x144C6; // cmd 15 object-1 positions [x, y] words (S:0x144AE)

/// Interaction state that is not part of the original VM (kept beside `Engine`).
#[derive(Default)]
pub struct Ix {
    pub entry: Option<Entry>,
    /// last typed string ($9759), reused by the second matching step ($971A == 2)
    pub typed: Vec<u8>,
    /// light gun: present on port 2 (`$7983 == 1`), position in screen pixels, buttons
    pub gun_present: bool,
    pub gun_x: i32,
    pub gun_y: i32,
    pub gun_buttons: u8,
    gun_prev: u8,
    pub shoot: Option<crate::shoot::Shoot>,
    /// fidelity harness: sub RNG seeded from this counter before every frame-handler run (None = normal)
    pub rng_seq: Option<u32>,
    /// PCM effect (length in frames, channel) by (bank, effect id) from the audio manifest
    pub sfx_len: std::collections::HashMap<(u16, u16), (u32, u8)>,
    /// `$1511E` bit 3 and the frame each of the two PCM effect channels ends (0 = idle)
    pub pcm_busy: bool,
    pub pcm_ch_until: [u64; 2],
    /// fidelity harness: logical (held, pressed, repeat) = $7990/$7991/$7992 the original's handler run saw (overrides the pad)
    pub force_in: Option<(u8, u8, u8)>,
    /// the player calibrated the gun (title screen) or a save with a calibration was loaded; until then a default calibration is used
    pub gun_cal_done: bool,
    /// fidelity harness: the original's ($7985, $798A, $798B) of the handler run (overrides the computed gun registers)
    pub force_gun: Option<(u8, u8, u8)>,
    /// pointer mode (`$79FE == 1`): B is held, the view is being moved
    pub ptr_moving: bool,
    /// text entry finished, waiting for the close effect (`$7892 == 0`, S:0x11C28): (result, frames waited)
    pub closing: Option<(u8, u16)>,
    pub ptr_logged: bool,
}

// ------------------------------------------------------------------ object helpers

/// Write the integer part of an object's X (word at +6) – `move.w v,$6(a0)`.
pub fn set_x(e: &mut Engine, slot: usize, v: i32) { let o = &mut e.disp.sub.objs[slot & 63]; o.x = (o.x & 0xFFFF) | (v << 16); }
pub fn set_y(e: &mut Engine, slot: usize, v: i32) { let o = &mut e.disp.sub.objs[slot & 63]; o.y = (o.y & 0xFFFF) | (v << 16); }
/// Byte write to +7 / +0x0B (low byte of the integer part).
fn set_x_lo(e: &mut Engine, slot: usize, v: u8) { let o = &mut e.disp.sub.objs[slot & 63]; o.x = (o.x & !0x00FF_0000) | ((v as i32) << 16); }
fn set_y_lo(e: &mut Engine, slot: usize, v: u8) { let o = &mut e.disp.sub.objs[slot & 63]; o.y = (o.y & !0x00FF_0000) | ((v as i32) << 16); }
fn x_lo(e: &Engine, slot: usize) -> u8 { (e.disp.sub.objs[slot & 63].x >> 16) as u8 }
fn y_lo(e: &Engine, slot: usize) -> u8 { (e.disp.sub.objs[slot & 63].y >> 16) as u8 }

/// Queue-command-0x1C style sound (S:0x13E0A): `$980F = n`, PCM effect n.
pub fn sfx_1c(e: &mut Engine, n: u16) {
    e.regs.set_b(0x980F, if n == 0x3D { 0 } else { n as u8 });
    e.sfx_left = 30;
    e.out.push(Out::Pcm { id: n, kind: 0 });
}

/// Draw a render stream into the text window at once (cmd 05 with `FC 00`).
pub fn draw_stream(e: &mut Engine, s: &[u8]) {
    let Some(font) = e.font.as_ref() else { return };
    let mut win = std::mem::take(&mut e.text);
    win.start(s);
    let mut guard = 0;
    while win.busy && guard < 4096 { win.tick(font); guard += 1; }
    e.text = win;
    e.text_dirty = true;
}

pub(crate) fn stream_at(e: &Engine, addr: u32) -> Vec<u8> {
    let mut v = vec![];
    for i in 0..256 { let b = e.mem.u8(addr + i); v.push(b); if b == 0xFF { break } }
    v
}

// ------------------------------------------------------------------ list menu cursor bar

/// S:0x10FB0: object 0 (highlight bar created by the VM-thread init list S:0xED0E) at
/// X = 0x10, Y = 0x8F + 9*line (+0x10 with the wide portrait layout `$7893`), shown.
pub fn list_cursor_pos(e: &mut Engine, line: usize) {
    let wide = e.regs.get_b(0x7893) != 0;
    let y = 0x8F + 9 * line as i32 + if wide { 0x10 } else { 0 };
    set_x(e, 0, 0x10);
    set_y(e, 0, y);
}

pub fn list_cursor(e: &mut Engine, line: usize) {
    list_cursor_pos(e, line);
    let o = &mut e.disp.sub.objs[0];
    if o.script != 0 { o.active = 1; o.flags = 0; } // `move.w #1,obj_slots` + `clr.b $7A25` (S:0x1104E/0x10FFE); the bar exists only after the init list; message boxes clear `active` (S:0x13604)
}

/// S:0x1106E: `$7A25 = 3` when the menu closes (bar hidden).
pub fn list_close(e: &mut Engine) { e.disp.sub.objs[0].flags = 3; }

// ------------------------------------------------------------------ light gun

/// Raw gun reading for a screen position (px, py), HYPOTHESIS-level absolute values: measured on the ORIGINAL main-CPU code
/// (M:0xFF148A handler, re/oracle machine.py gun model `m.gun`, docs/re/shooting.md): X = 2 * (mean of two H-counter reads) tracks
/// the aimed pixel 1:1 with an offset of about -66 (the handler reads the counter ~400 cycles after the TH interrupt), Y = V counter =
/// aimed line + 1. A calibration (`$7988/$7989 = raw - 0x80`) removes the offsets, so only the 1:1 scale matters for play.
pub const GUN_RAW_X_OFF: i32 = -66;
pub const GUN_RAW_Y_OFF: i32 = 1;
pub fn gun_raw(px: i32, py: i32) -> (u8, u8) {
    (((px.clamp(0, 255) + GUN_RAW_X_OFF) as u8) & 0xFE, (py.clamp(0, 223) + GUN_RAW_Y_OFF) as u8)
}
/// Y bias between the bullseye of the calibration screen (centre y = 101, measured on the oracle: value 0x80 there) and the aim point the cell
/// thresholds (S:0x12516: value 0x60/0xA0, 0x30/0x58) assume (value 0x80 at y = 132, i.e. cell rows [16,52) [52,92) [92,..) of the 3x3 picture). The ORIGINAL code, calibrated on the
/// bullseye, therefore splits the rows at y = 21/61 (oracle: `re/oracle/g5` scan, docs/re/shooting.md); the web port compensates so that the cell
/// under the pointer is the cell that is shot (HYPOTHESIS: real hardware needed no such correction).
pub const GUN_CAL_BIAS_Y: u8 = 31;
/// Calibration equivalent to a perfect aim at the design centre: value = (px, py - 4).
pub fn gun_default_calib() -> (u8, u8) {
    let (rx, ry) = gun_raw(128, 128 + 4);
    (rx.wrapping_sub(0x80), ry.wrapping_sub(0x80))
}

// ------------------------------------------------------------------ frame hook

/// Called every frame before the VM thread: light-gun registers (S:0xD760, S:0xD7B2) and
/// the pointer-mode view jitter (S:0x10990).
pub fn frame_hook(e: &mut Engine) {
    // $7972 = gun mode: the original needs the GUN option ($78B8) and a gun on port 2 ($7983 == 1)
    // (S:0xD766). The web front end only reports a gun after the player used the mouse, so that
    // choice replaces the option here. `$7972` is read by the shooting code only (S:0x1205C–0x13086).
    let gun = e.ix.gun_present;
    e.regs.set(0x7972, gun as u16);
    e.regs.set_b(0x7983, if e.ix.gun_present { 1 } else { 0x0D });
    // gun buttons: edges like the main CPU's M:0xFF13F6 ($7985 bit5 trigger, bit6 second button)
    let b = e.ix.gun_buttons;
    e.regs.set_b(0x7985, b & !e.ix.gun_prev);
    e.ix.gun_prev = b;
    // Gun position registers as the main CPU reports them (M:0xFF14E0: COMCMD3/4 = X = 2 * H counter, Y = V counter at the TH
    // interrupt) and as the sub CPU derives its values (S:0xD7B2: `$7986/$7987` raw, `$798A/$798B` = raw - `$7988/$7989`, byte wrap).
    // The mouse is given in screen pixels; `gun_raw` converts to the raw hardware values (see there).
    let (rx, ry) = gun_raw(e.ix.gun_x, e.ix.gun_y);
    if !e.ix.gun_cal_done && gun {
        // no calibration done yet (title screen row 3 / loaded save): assume a perfect one (the mouse has no tolerance)
        let (cx, cy) = gun_default_calib();
        e.regs.set_b(0x7988, cx); e.regs.set_b(0x7989, cy);
    }
    e.regs.set_b(0x7986, rx); e.regs.set_b(0x7987, ry);
    let cx = e.regs.get_b(0x7988); let cy = e.regs.get_b(0x7989);
    e.regs.set_b(0x798A, rx.wrapping_sub(cx));
    e.regs.set_b(0x798B, ry.wrapping_sub(cy));
    if let Some((b, x, y)) = e.ix.force_gun { e.regs.set_b(0x7985, b); e.regs.set_b(0x798A, x); e.regs.set_b(0x798B, y); }

    if e.trace && e.regs.get(0x79FE) == 1 && !e.ix.ptr_logged { e.ix.ptr_logged = true; e.out.push(Out::Log("pointer mode on".into())); }
    if e.regs.get(0x79FE) == 1 {
        // S:0x10990: view follows $96D0/$96D4 with ±1 px random jitter
        let mut x = e.regs.get(0x96D0) as i16 as i32;
        let mut y = e.regs.get(0x96D4) as i16 as i32 - 0x28;
        if e.disp.sub.rand() & 0x8000 != 0 { x += 1; }
        if e.disp.sub.rand() & 0x8000 != 0 { y += 1; }
        e.disp.sub.scroll_mode(1);
        e.disp.sub.scroll.delta[0] = x << 16;
        e.disp.sub.scroll.delta[1] = y << 16;
    }
}

// ------------------------------------------------------------------ menu open / frame hooks

/// Called at the start of `open_menu`. Returns true when this module took over the menu.
pub fn open_menu_hook(e: &mut Engine, items: &[u16]) -> bool {
    // `$971A == 2`: second step of a two-step entry – match the previous input at once
    // (menu_show S:0x10CBE returns early, menu_input S:0x11004 → kbd_match_items).
    if e.regs.get(0x971A) == 2 {
        let typed = e.ix.typed.clone();
        let r = match_items(e, items, &typed);
        finish_entry(e, r, false);
        return true;
    }
    // menu_show S:0x10D24: `sub_013d1c` (clear the text window) before the layout branch
    e.text_window_init();
    // menu_pick_layout S:0x1130A: ≥ 9 items → keypad (first item text < 'A') or keyboard
    if items.len() >= 9 {
        let first = e.script.get(0x3800 + items[0] as usize).copied().unwrap_or(0);
        let style = if first >= 0x41 { 3 } else { 2 };
        e.regs.set(0x79FE, style);
        let desc = LAYOUT_PAL[(style - 2) as usize];
        let mem = std::mem::take(&mut e.mem);
        e.disp.sub.palch_start(&mem, desc);
        e.mem = mem;
    }
    let style = e.regs.get(0x79FE);
    if style >= 2 {
        let ent = Entry::open(e, style as u8, items.to_vec());
        if e.trace { e.out.push(Out::Log(format!("text entry opened: style {style}, {} items", items.len()))); }
        e.ix.entry = Some(ent);
        e.phase = VmPhase::Menu;
        return true;
    }
    false
}

/// Called at the start of `menu_frame`. Returns true when the frame was consumed here.
pub fn menu_frame_hook(e: &mut Engine) -> bool {
    if let Some((r, t)) = e.ix.closing {
        // `$7892` = main `$E061` (M:0xFF321C): wait until the closing window effect has finished
        if e.disp.main.e061 != 0 && t < 240 { e.ix.closing = Some((r, t + 1)); return true; }
        e.ix.closing = None;
        finish_entry(e, r, true);
        return true;
    }
    if let Some(mut ent) = e.ix.entry.take() {
        let done = ent.frame(e);
        match done {
            Some(r) => {
                e.ix.typed = ent.typed.clone();
                finish_entry_ui(e, &ent);
                if ent.style == 3 { e.ix.closing = Some((r, 0)); } else { finish_entry(e, r, true); }
            }
            None => e.ix.entry = Some(ent),
        }
        return true;
    }
    if e.regs.get(0x79FE) == 1 && e.vm.path.len() == 0 {
        return pointer_frame(e);
    }
    false
}

// ------------------------------------------------------------------ pointer mode

/// `pointer_mode` S:0x10846 (only at menu depth 0). While B (logical bit 4) is held the view
/// moves 0x200 per loop iteration; the original polls in a tight loop, so per frame we apply
/// `PTR_STEPS` = 261 iterations (oracle: `$96D0` grows by 0x2.0A00 = 261 × 0x200 per frame; re/oracle/interact_jump.py --watch 96D0,96D4). Releasing B computes the region
/// id `$96DC` from the map at S:0x109E8 and shows the menu bar again.
const PTR_STEPS: i32 = 261;
/// diagonal (two D-pad bits): the loop body is longer, oracle ≈ 1.55 px/frame (HYPOTHESIS: ~200 iterations)
const PTR_STEPS_DIAG: i32 = 200;
fn pointer_frame(e: &mut Engine) -> bool {
    let held = e.input.as_ref().map(|i| i.held).unwrap_or(0);
    if held & btn::L_CANCEL == 0 {
        if e.ix.ptr_moving {
            e.ix.ptr_moving = false;
            e.regs.set_b(0x9717, 0);
            e.disp.sub.objs[0].flags = 0;
        }
        // sub_01086E: region under the view centre
        let x = ((e.regs.get(0x96D0) as i16 as i32 + 0x48) >> 2) as u32;
        let y = ((e.regs.get(0x96D4) as i16 as i32 + 0x28) >> 2) as u32;
        let id = e.mem.u8(PTR_MAP + y * 0x24 + x);
        e.regs.set(0x96DC, id as u16);
        return false; // normal list-menu input this frame
    }
    e.ix.ptr_moving = true;
    e.regs.set_b(0x9717, 0xFF);
    e.disp.sub.objs[0].flags = 3;
    let rd = |e: &Engine, a: u32| ((e.regs.get(a) as u32) << 16 | e.regs.get(a + 2) as u32) as i32;
    let mut px = rd(e, 0x96D0); let mut py = rd(e, 0x96D4);
    let steps = if (held & 0x0F).count_ones() >= 2 { PTR_STEPS_DIAG } else { PTR_STEPS };
    for _ in 0..steps {
        // order of S:0x108D6..: up, down, right, left; bounds of S:0x10950/S:0x10962
        if held & 1 != 0 { let v = py - 0x200; if in_bounds(v, 0x250000, -0x270000) { py = v } }
        if held & 2 != 0 { let v = py + 0x200; if in_bounds(v, 0x250000, -0x270000) { py = v } }
        if held & 8 != 0 { let v = px + 0x200; if in_bounds(v, 0x450000, -0x470000) { px = v } }
        if held & 4 != 0 { let v = px - 0x200; if in_bounds(v, 0x450000, -0x470000) { px = v } }
    }
    e.regs.set(0x96D0, (px >> 16) as u16); e.regs.set(0x96D2, px as u16);
    e.regs.set(0x96D4, (py >> 16) as u16); e.regs.set(0x96D6, py as u16);
    true
}

/// S:0x10950/0x10962: accept `v` when v ≥ lo (unsigned compare against the negative bound,
/// i.e. negative values down to `lo`) or 0 ≤ v < hi.
fn in_bounds(v: i32, hi: i32, lo: i32) -> bool { if v < 0 { v >= lo } else { v < hi } }

// ------------------------------------------------------------------ text entry

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage { Opening(u16), Active, WaitKey(After, u16) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum After { Enter, Backspace }

/// Keypad / keyboard entry (`$79FE` 2 / 3).
pub struct Entry {
    pub style: u8,
    pub items: Vec<u16>,
    stage: Stage,
    grid: u32,      // $79F6
    cell: u8,       // $97F0
    stride: u8,     // $97F1
    x0: u8,         // $97F7
    y0: u8,         // $97F8
    text_x: u8,     // $97F6
    max: u8,        // $9719
    pub buf: [u8; 0x12], // $97D0
    pos: u8,        // $97F2
    blink: u8,      // $97F4
    blink_t: u8,    // $97F5
    key_state: u8,  // $9713
    key_t0: u8,     // $9714
    saved62: (u8, u8),
    /// direct dial completed: result held back while the original's redraw streams finish
    /// (oracle: digits appear 8 frames after the press, the keypad closes ~14 frames after it)
    pending: Option<(u8, u8)>,
    pub typed: Vec<u8>,
    result: Option<u8>,
}

impl Entry {
    /// Test harness (`Engine::wait_state`): the key loop accepts input.
    pub fn active(&self) -> bool { matches!(self.stage, Stage::Active) }

    /// `loc_0113A0` + `sub_011496` (S:0x113A0–0x11448, S:0x11496).
    fn open(e: &mut Engine, style: u8, items: Vec<u16>) -> Entry {
        let k = (style - 2) as u32;
        let grid = e.mem.u32(GRID_PTRS + 4 * k) & 0xFF_FFFF;
        let mut ent = Entry {
            style, items, stage: Stage::Opening(0), grid,
            cell: e.mem.u8(GRID_PARAMS + 2 * k), stride: e.mem.u8(GRID_PARAMS + 2 * k + 1),
            x0: e.mem.u8(GRID_PARAMS + 4 + 2 * k), y0: e.mem.u8(GRID_PARAMS + 5 + 2 * k),
            text_x: 0x40, max: 0, buf: [0; 0x12], pos: 0, blink: 0, blink_t: 0, key_state: 0, key_t0: 0,
            saved62: (0, 0), pending: None, typed: vec![], result: None,
        };
        if style == 2 {
            let s = stream_at(e, KEYPAD_STREAM);
            draw_stream(e, &s);
            e.disp.sub.scroll_mode(0x0F); // sub_011BDE
            if e.regs.get(0x9718) == 0 { ent.text_x = 0x18; e.regs.set(0x9718, 10); e.disp.sub.objs[63].idx = 2; }
        } else {
            e.regs.set_b(0x78AC, 1);
            e.disp.main.e06e = 0;
            e.disp.sub.scroll_mode(0x10); // sub_011BEE (waits for the window effect, $788C)
            if e.regs.get(0x9718) == 0 { e.regs.set(0x9718, 0x11); ent.text_x = 0x30; e.disp.sub.objs[63].idx = 0; }
        }
        ent.max = e.regs.get_b(0x9719);
        e.regs.set_b(0x79F4, 0);
        ent
    }

    fn after_open(&mut self, e: &mut Engine) {
        // sub_011C30: '_' placeholders at (i*9 + $97F6, 0x21)
        for i in 0..self.max {
            let x = (i as u32 * 9 + self.text_x as u32) as u8;
            draw_stream(e, &[0xFC, 0x00, 0xF9, 0x01, 0xFB, x, 0x21, 0xFE, 0x01, 0x5F, 0xFF]);
        }
        self.pos = 0; self.blink = 0;
        self.blink_draw(e);
        self.buf = [0; 0x12]; // sub_011CB0
        self.place_cursor(e); // sub_011D62
        e.regs.set_b(0x9717, 0);
        // menu_input / sub_011496: bar (obj 0) hidden, cursor object 63 shown
        e.disp.sub.objs[0].active = 1;
        e.disp.sub.objs[0].flags = 3;
        e.disp.sub.objs[63].flags = 0;
        e.disp.sub.objs[63].idx = if self.style == 2 { 2 } else { 0 };
    }

    /// sub_011D62: cursor objects 62/63 at the cell (row from 1, 8 px grid).
    fn place_cursor(&mut self, e: &mut Engine) {
        e.disp.sub.objs[63].flags = 0;
        e.disp.sub.objs[63].idx = if self.style == 2 { 2 } else { 0 };
        e.disp.sub.objs[62].flags = 3;
        let (mut c, mut row) = (self.cell, 1u8);
        while self.stride != 0 && c >= self.stride { row += 1; c -= self.stride; }
        let x = (c << 3).wrapping_add(self.x0);
        let y = (row << 3).wrapping_add(self.y0);
        set_x_lo(e, 63, x); set_x_lo(e, 62, x);
        set_y_lo(e, 63, y); set_y_lo(e, 62, y);
    }

    /// sub_011B86: blinking '_' at the current position (colour 0/1 alternately).
    fn blink_draw(&mut self, e: &mut Engine) {
        let x = (self.pos as u32 * 9 + self.text_x as u32) as u8;
        let col = if self.blink == 0 { self.blink = 1; 0 } else { self.blink = 0; 1 };
        draw_stream(e, &[0xFC, 0x00, 0xF9, 0x01, 0xFB, x, 0x21, 0xFE, col, 0x5F, 0xFF]);
        self.blink_t = (e.frame as u8).wrapping_add(0x20);
    }

    /// sub_0118D4: clear the typed line (cmd 23: S:0xBEC00, 0x320 bytes = tiles 96–120 = y 24–31,
    /// x 0–199), draw it in fixed pitch at ($97F6, 0x18), redraw the blink cursor. Returns true
    /// when direct dial completed the number.
    fn redraw(&mut self, e: &mut Engine) -> bool {
        for y in 24..32 { for x in 0..200 { e.text.pix[y * sncore::actions::WIN_W + x] = 0; } }
        let mut s = vec![0xF7, 0x01, 0xFC, 0x00, 0xF9, 0x01, 0xFB, self.text_x, 0x18, 0xFE, 0x01];
        let t = self.typed_now();
        s.extend_from_slice(&t);
        s.extend_from_slice(&[0xF7, 0x00, 0xFF]);
        draw_stream(e, &s);
        self.blink_draw(e);
        if e.regs.get(0x971E) != 0 {
            // direct dial: complete when the expected length (by first digit) is typed
            let f = self.buf[0];
            let n = if f == b'*' || f == b'#' { 2 } else { e.mem.u8(DIAL_LEN + (f & 0xF) as u32) as usize };
            if n >= 1 && n <= self.buf.len() && self.buf[n - 1] != 0 { return true; }
        }
        false
    }

    /// $9759: the non-zero characters typed so far, or " " when empty (S:0x11966).
    fn typed_now(&self) -> Vec<u8> {
        let mut v: Vec<u8> = self.buf.iter().take(0x12).copied().filter(|&c| c != 0).collect();
        if v.is_empty() { v.push(b' '); }
        v
    }

    /// menu_read_keys S:0x110B0 for styles ≥ 2.
    fn keys(&mut self, e: &Engine) -> u8 {
        let (pressed, held, repeat) = e.input.as_ref().map(|i| (i.pressed, i.held, i.repeat)).unwrap_or((0, 0, 0));
        let f = e.frame as u8;
        if pressed != 0 { self.key_state = 0; return pressed; }
        match self.key_state {
            0 => {
                self.key_t0 = f;
                if held == 0 { return 0; }
                self.key_state = 1;
                self.key_t0 = f.wrapping_sub(1);
                if (f ^ self.key_t0) & 0x1F != 0 { return 0; }
                self.key_state = 2; held
            }
            1 => { if (f ^ self.key_t0) & 0x1F != 0 { return 0; } self.key_state = 2; held }
            _ => {
                if self.style == 3 { return repeat; }
                if (f ^ self.key_t0) & 7 != 0 { 0 } else { held }
            }
        }
    }

    /// sub_0119E0: move from the current cell by `step` following the grid codes.
    fn mv(&mut self, e: &mut Engine, step: i16) {
        let mut d = self.cell as i16 + step;
        let mut dir = step;
        for _ in 0..64 {
            let c = (d as u16 & 0xFF) as u8;
            let code = e.mem.u8(self.grid + c as u32);
            match code {
                0 => return,
                1 => { d = c as i16 + dir; }
                4 => { dir = self.stride as i16; d = c as i16 + dir; }
                5 => { dir = -(self.stride as i16); d = c as i16 + dir; }
                2 => { dir = 1; d = c as i16 + 1; }
                3 => { dir = -1; d = c as i16 - 1; }
                7 => { dir = 1; d = c as i16 - self.stride as i16 + 1; }
                6 => { dir = -1; d = c as i16 + self.stride as i16 - 1; }
                _ => { self.cell = c; self.place_cursor(e); return; }
            }
        }
    }

    fn backspace(&mut self, e: &mut Engine) -> bool {
        sfx_1c(e, 0x1D);
        if self.pos == 0 { return false; }
        let mut d = self.pos as usize;
        if self.buf[d] == 0 { d -= 1; }
        self.buf[d] = 0;
        self.pos = d as u8;
        true
    }

    /// One frame. Returns the menu result ($79F4) when the entry finishes.
    fn frame(&mut self, e: &mut Engine) -> Option<u8> {
        match self.stage {
            Stage::Opening(t) => {
                // keyboard waits for the window effect to report completion ($788C ← main $E06E)
                let ready = self.style == 2 || e.disp.main.e06e != 0 || t > 90;
                if ready && t >= 1 { self.after_open(e); self.stage = Stage::Active; } else { self.stage = Stage::Opening(t + 1); }
                return None;
            }
            Stage::WaitKey(after, t) => {
                // wait until the key-press animation object 62 hid itself (flags == 3); timeout is ours
                if e.disp.sub.objs[62].flags != 3 && e.disp.sub.objs[62].active != 0 && t < 180 { self.stage = Stage::WaitKey(after, t + 1); return None; }
                if self.style == 3 {
                    e.disp.sub.objs[63].flags = 0;
                    set_x_lo(e, 62, self.saved62.0); set_y_lo(e, 62, self.saved62.1);
                }
                self.stage = Stage::Active;
                return match after {
                    After::Enter => Some(self.finish(e)),
                    After::Backspace => None,
                };
            }
            Stage::Active => {}
        }
        if e.frame as u8 == self.blink_t { self.blink_draw(e); } // sub_011B7A
        if let Some((r, t)) = self.pending {
            if t == 0 { self.pending = None; return Some(r); }
            self.pending = Some((r, t - 1));
            return None;
        }
        let d0 = self.keys(e);
        let pressed = e.input.as_ref().map(|i| i.pressed).unwrap_or(0);
        if d0 & 1 != 0 { let s = self.stride as i16; self.mv(e, -s); return None; }
        if d0 & 2 != 0 { let s = self.stride as i16; self.mv(e, s); return None; }
        if d0 & 8 != 0 { self.mv(e, 1); return None; }
        if d0 & 4 != 0 { self.mv(e, -1); return None; }
        if d0 & 0x10 != 0 {
            // cancel = backspace (loc_011710)
            if self.style == 3 {
                self.saved62 = (x_lo(e, 62), y_lo(e, 62));
                set_x_lo(e, 62, 0x58); set_y_lo(e, 62, 0xC0);
                e.disp.sub.objs[62].idx = 0x68; e.disp.sub.objs[62].flags = 0;
                if self.cell != 0x8B { e.disp.sub.objs[63].flags = 3; }
            }
            self.backspace(e);
            if self.redraw(e) { return self.hold_finish(e); }
            if self.style == 3 { self.stage = Stage::WaitKey(After::Backspace, 0); }
            return None;
        }
        if e.regs.get(0x971E) == 0 && pressed & 0x80 != 0 {
            // Start = enter (loc_011792)
            if self.style == 3 {
                self.saved62 = (x_lo(e, 62), y_lo(e, 62));
                set_x_lo(e, 62, 0xC8); set_y_lo(e, 62, 0xC0);
                e.disp.sub.objs[62].idx = 0x6C; e.disp.sub.objs[62].flags = 0;
                e.disp.sub.objs[63].flags = 3;
            }
            sfx_1c(e, 0x1D);
            self.stage = Stage::WaitKey(After::Enter, 0);
            return None;
        }
        if d0 & 0x20 != 0 {
            // confirm = type the character under the cursor (sub_011808)
            let c = e.mem.u8(self.grid + self.cell as u32);
            if e.regs.get(0x971E) != 0 {
                let slot = match c { b'*' => 2, b'#' => 3, _ => (c & 0x1F) as usize };
                e.disp.sub.objs[slot].flags = 0; // keypad button press object
            } else {
                sfx_1c(e, 0x1D);
                if self.style == 3 {
                    e.disp.sub.objs[62].idx = e.mem.u8(KEY_ANIM + self.cell as u32) as u16;
                    e.disp.sub.objs[62].flags = 0;
                }
            }
            match c {
                8 => { self.stage = Stage::WaitKey(After::Enter, 0); sfx_1c(e, 0x1D); return None; }
                9 => { if self.backspace(e) && self.redraw(e) { return self.hold_finish(e); } return None; }
                _ => {
                    let p = self.pos as usize;
                    if p < self.max as usize && p < self.buf.len() {
                        self.buf[p] = c;
                        if p + 1 != self.max as usize { self.pos += 1; }
                        if self.redraw(e) { return self.hold_finish(e); }
                    }
                }
            }
        }
        None
    }

    fn hold_finish(&mut self, e: &mut Engine) -> Option<u8> {
        let r = self.finish(e);
        self.pending = Some((r, 13));
        None
    }

    /// kbd_match_items S:0x11AA8 with the current input.
    fn finish(&mut self, e: &mut Engine) -> u8 {
        let mut d2 = self.pos as usize;
        if self.buf.get(d2).copied().unwrap_or(0) != 0 { d2 += 1; }
        let mut t: Vec<u8> = self.buf[..d2.min(self.buf.len())].to_vec();
        if t.iter().all(|&c| c == b' ' || c == 0) { t = vec![b' ']; }
        self.typed = t.clone();
        let items = self.items.clone();
        let r = match_items(e, &items, &t);
        self.result = Some(r);
        r
    }
}

/// kbd_match_items / kbd_compare S:0x11AA8–0x11B78: result = 1-based index of the first
/// item whose text equals the input (`$971A == 1`: the first item containing the first typed
/// character), or 1 when nothing matches.
pub fn match_items(e: &Engine, items: &[u16], typed: &[u8]) -> u8 {
    let step1 = e.regs.get(0x971A) == 1;
    for (i, &off) in items.iter().enumerate() {
        let t = e.script.get(0x3800 + off as usize..).unwrap_or(&[]);
        let item: Vec<u8> = t.iter().copied().take_while(|&c| c != 0xFF).collect();
        let hit = if step1 { item.contains(typed.first().unwrap_or(&b' ')) } else { item == typed };
        if hit { return (i + 1) as u8; }
    }
    1
}

/// End of a text-entry menu (loc_011DCE): close effect, clear the window, `$971A` step logic.
fn finish_entry_ui(e: &mut Engine, ent: &Entry) {
    e.disp.sub.objs[63].flags = 3;
    e.disp.sub.objs[62].flags = 3;
    if ent.style != 2 {
        if e.regs.get_b(0x78AC) != 0 { e.regs.set_b(0x78AC, 0); e.disp.sub.scroll_mode(0x11); }
    } else {
        e.disp.sub.scroll_mode(0xFE);
    }
}

fn finish_entry(e: &mut Engine, r: u8, _ui: bool) {
    e.disp.sub.objs[0].flags = 3; // `$7A25 = 3`
    e.text_window_init();
    let k = e.regs.get(0x971A).wrapping_add(1);
    if k == 2 { e.regs.set(0x971A, 2); } else { e.regs.set(0x971A, 0); e.regs.set(0x79FE, 0); }
    e.regs.set(0x971E, 0);
    e.regs.set(0x9718, 0);
    e.regs.set_b(0x79F4, r);
    e.vm.choose(Some(r.max(1) as usize - 1));
    e.phase = VmPhase::Pass;
    e.vm_pass();
}

// ------------------------------------------------------------------ sequencer command 15

/// Command 0x15 (S:0x1036C): wait while `$7886`, then built-in scene `n` (SUBCODE table
/// S:0x146CE, `scene_load_builtin`) and move object 1 to grid position `pos` (S:0x144C6).
pub fn cmd15(e: &mut Engine, n: u16, pos: u16) -> bool {
    if e.regs.get_b(0x7886) != 0 { return false; }
    let mem = std::mem::take(&mut e.mem);
    let cnt = gfx::scene_count(&mem, BUILTIN_SCENES);
    if (n as usize) < cnt.max(1) {
        let rec = gfx::scene_addr(&mem, BUILTIN_SCENES, n as usize);
        e.disp.sub.load_scene(&mem, rec);
    }
    e.mem = mem;
    let a = GRID_POS + 4 * pos as u32;
    let (x, y) = (e.mem.u16(a) as i16 as i32, e.mem.u16(a + 2) as i16 as i32);
    set_x(e, 1, x);
    set_y(e, 1, y);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Engine with a script image whose text area (0x3800) holds `texts` (FF-terminated); returns the
    /// item offsets.
    fn engine_with(texts: &[&str]) -> (Engine, Vec<u16>) {
        let mut e = Engine::new();
        let mut script = vec![0u8; 0x3800];
        let mut offs = vec![];
        for t in texts {
            offs.push((script.len() - 0x3800) as u16);
            script.extend_from_slice(t.as_bytes());
            script.push(0xFF);
        }
        e.script = script;
        (e, offs)
    }

    #[test]
    fn exact_match_returns_one_based_index() {
        let (e, items) = engine_with(&["NO MATCH", "ALICE", "ALICEGIBSON", "JOHN"]);
        assert_eq!(match_items(&e, &items, b"ALICE"), 2);
        assert_eq!(match_items(&e, &items, b"ALICEGIBSON"), 3);
        assert_eq!(match_items(&e, &items, b"JOHN"), 4);
    }

    #[test]
    fn no_match_selects_first_item() {
        // S:0x11AA8: the script's first item is the "no matches" branch
        let (e, items) = engine_with(&["NO MATCH", "ALICE", "JOHN"]);
        assert_eq!(match_items(&e, &items, b"BOB"), 1);
        assert_eq!(match_items(&e, &items, b"ALIC"), 1, "prefixes do not match in the exact step");
        assert_eq!(match_items(&e, &items, b" "), 1);
    }

    #[test]
    fn first_step_matches_by_first_letter() {
        // `$971A == 1`: first item whose text contains the first typed character (the "ABCDE" menu)
        let (mut e, items) = engine_with(&["ABCDE", "ADACHI", "JOHN", "KATE"]);
        e.regs.set(0x971A, 1);
        assert_eq!(match_items(&e, &items, b"J"), 3);
        assert_eq!(match_items(&e, &items, b"KONAMI"), 4);
        assert_eq!(match_items(&e, &items, b"A"), 1);
        assert_eq!(match_items(&e, &items, b"Z"), 1, "nothing contains Z: item 1");
    }

    #[test]
    fn empty_input_is_a_single_space() {
        // S:0x11966: the typed string $9759 is the non-zero characters of $97D0.., or " " when empty
        let (e, items) = engine_with(&["NO MATCH", " ", "X"]);
        let mut ent = Entry {
            style: 3, items: vec![], stage: Stage::Active, grid: 0, cell: 0, stride: 0, x0: 0, y0: 0, text_x: 0x30,
            max: 0x11, buf: [0; 0x12], pos: 0, blink: 0, blink_t: 0, key_state: 0, key_t0: 0, saved62: (0, 0),
            pending: None, typed: vec![], result: None,
        };
        assert_eq!(ent.typed_now(), b" ");
        assert_eq!(match_items(&e, &items, &ent.typed_now()), 2);
        ent.buf[0] = b'A'; ent.buf[2] = b'B'; // a hole (backspace) is skipped
        assert_eq!(ent.typed_now(), b"AB");
    }

    #[test]
    fn pointer_bounds_follow_the_original_unsigned_compares() {
        // X: 0 <= v < 0x450000 or -0x470000 <= v < 0 (S:0x10950/0x10962)
        assert!(in_bounds(0x44FE00, 0x450000, -0x470000));
        assert!(!in_bounds(0x450000, 0x450000, -0x470000));
        assert!(in_bounds(-0x470000, 0x450000, -0x470000));
        assert!(!in_bounds(-0x470200, 0x450000, -0x470000));
        assert!(in_bounds(0, 0x450000, -0x470000));
    }

    #[test]
    fn gun_raw_values_follow_the_pixel_one_to_one() {
        // oracle gun model (docs/re/shooting.md): X = 2*H counter ~ px - 66 (even), Y = line + 1
        assert_eq!(gun_raw(128, 100), (62, 101));
        assert_eq!(gun_raw(130, 100).0, 64);
        assert_eq!(gun_raw(200, 0), (134, 1));
        assert_eq!(gun_raw(300, 500), (gun_raw(255, 223).0, 224));
    }

    #[test]
    fn default_calibration_maps_the_picture_to_the_cell_thresholds() {
        // value = raw - calib (S:0xD7B2, byte wrap) must be (px, py - 4): the 3x3 picture rows [16,52) [52,92) [92,..) and columns split at 96/160
        let (cx, cy) = gun_default_calib();
        for (px, py) in [(0, 0), (64, 32), (95, 51), (96, 52), (128, 72), (159, 91), (160, 92), (255, 140)] {
            let (rx, ry) = gun_raw(px, py);
            let (vx, vy) = (rx.wrapping_sub(cx), ry.wrapping_sub(cy));
            assert_eq!(vx as i32, px & !1, "x value of pixel {px}");
            assert_eq!(vy as i32, (py - 4).rem_euclid(256), "y value of line {py}");
        }
    }

    #[test]
    fn calibration_on_the_bullseye_equals_the_default() {
        // title screen: aiming at the bullseye (128, 101) and pulling the trigger stores raw - 0x80 (+ GUN_CAL_BIAS_Y for the rows)
        let (rx, ry) = gun_raw(128, 101);
        let cal = (rx.wrapping_sub(0x80), ry.wrapping_sub(0x80).wrapping_add(GUN_CAL_BIAS_Y));
        assert_eq!(cal, gun_default_calib());
    }
}
