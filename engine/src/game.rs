//! The in-game engine: VM thread + action sequencer + text/menus + display, one call per
//! 60 Hz frame. Mirrors the sub CPU's frame (S:0xD5CC…0xD746, see docs/re/actions.md §5.1):
//! input → sequencer (if busy) → mode/objects → scroll/sprites/palettes/VRAM → text renderer,
//! plus the VM thread (S:0x166BA) which runs one script pass whenever the sequencer is idle.
use std::collections::HashMap;

use sncore::actions::{self, btn, Font, Input, MenuLine, MenuResult, MessageBox, MsgEvent, TextMenu, TextWindow};
use sncore::gfx::{self, Display, SubMem};
use sncore::script::vm::{Action, Host, PassEnd, Vm};

pub const W: usize = gfx::SCREEN_W;
pub const H: usize = gfx::SCREEN_H;

/// Game file index (the SP's file table, S:0x62FA; = ISO order without the .TXT files).
pub const FILE_NAMES: [&str; 97] = [
    "DATA_A0.BIN", "DATA_B0.BIN", "DATA_D0.BIN", "DATA_D1.BIN", "DATA_D2.BIN", "DATA_F0.BIN", "DATA_F4.BIN",
    "DATA_G0.BIN", "DATA_H00.BIN", "DATA_H01.BIN", "DATA_H02.BIN", "DATA_H03.BIN", "DATA_H04.BIN", "DATA_H05.BIN",
    "DATA_H06.BIN", "DATA_H07.BIN", "DATA_H08.BIN", "DATA_H09.BIN", "DATA_H11.BIN", "DATA_H12.BIN", "DATA_H13.BIN",
    "DATA_H14.BIN", "DATA_H15.BIN", "DATA_I0.BIN", "DATA_J0.BIN", "DATA_K1.BIN", "DATA_M0.BIN", "DATA_O0.BIN",
    "DATA_P0.BIN", "DATA_Q3.BIN", "DATA_Q5.BIN", "DATA_Q8.BIN", "DATA_S0.BIN", "DATA_S1.BIN", "DATA_S2.BIN",
    "DATA_T0.BIN", "DATA_U0.BIN", "DATA_Y00.BIN", "DATA_Y01.BIN", "DATA_Y03.BIN", "DATA_Y04.BIN", "DATA_Y05.BIN",
    "DATA_Y06.BIN", "DATA_Y07.BIN", "DATA_Y08.BIN", "DATA_Y09.BIN", "DATA_Y10.BIN", "DATA_Y11.BIN", "DATA_Y12.BIN",
    "DATA_Y13.BIN", "DATA_Y14.BIN", "DATA_Y15.BIN", "DATA_Y16.BIN", "FMWR_1.BIN", "PCMDRMDT.BIN", "PCMLD_01.BIN",
    "PCMLT_01.BIN", "SP00.BIN", "SP01.BIN", "SP02.BIN", "SP03.BIN", "SP04.BIN", "SP05.BIN", "SP06.BIN", "SP07.BIN",
    "SP08.BIN", "SP09.BIN", "SP10.BIN", "SP11.BIN", "SP12.BIN", "SP13.BIN", "SP14.BIN", "SP15.BIN", "SP16.BIN",
    "SP17.BIN", "SP18.BIN", "SP19.BIN", "SP20.BIN", "SP21.BIN", "SP22.BIN", "SP23.BIN", "SP24.BIN", "SP25.BIN",
    "SP26.BIN", "SP27.BIN", "SP28.BIN", "SP29.BIN", "SP30.BIN", "SP31.BIN", "SP32.BIN", "SP33.BIN", "SP34.BIN",
    "SP35.BIN", "SP36.BIN", "SP37.BIN", "SP38.BIN", "SUBCODE.BIN",
];

/// Engine registers addressed by their original sub-CPU addresses (SYS PEEK/POKE, snapshots).
#[derive(Default)]
pub struct Regs { pub w: HashMap<u32, u16> }

impl Regs {
    pub fn get(&self, a: u32) -> u16 { self.w.get(&a).copied().unwrap_or(0) }
    pub fn set(&mut self, a: u32, v: u16) { self.w.insert(a, v); }
    pub fn get_b(&self, a: u32) -> u8 {
        if a & 1 == 0 { (self.get(a) >> 8) as u8 } else { self.get(a & !1) as u8 }
    }
    pub fn set_b(&mut self, a: u32, v: u8) {
        let w = self.get(a & !1);
        let n = if a & 1 == 0 { (w & 0x00FF) | (v as u16) << 8 } else { (w & 0xFF00) | v as u16 };
        self.set(a & !1, n);
    }
}

impl Host for Regs {
    fn peek(&mut self, addr: u32) -> u16 { self.get(addr) }
    fn poke(&mut self, addr: u32, val: u16) { self.set(addr, val) }
    fn poke_byte(&mut self, addr: u32, val: u8) { self.set_b(addr, val) }
}

/// A sub-CPU code module that lives inside a DATA file (title DATA_D2, intro DATA_D1/D0,
/// late-game DATA_Y16; docs/re/actions.md §9) reimplemented in Rust. Called once per frame
/// like `jsr ($28008)` from the mode machine (S:0xE046).
pub trait Module {
    fn name(&self) -> &'static str;
    /// One frame. Return `Some(v)` when the module sets `$7846` (1 = done, -1/0xFF = skipped).
    fn tick(&mut self, e: &mut Engine) -> Option<i8>;
}

/// Top-level mode (`subfn_02` S:0xE046, state `$7842`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Top { Title, Intro, Game, Ending, Stopped }

/// Where the VM thread is (S:0x16814 loop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmPhase { Pass, Draining, Menu, Stopped }

/// Output events for the front end (audio, saves).
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Music { track: u16 },
    MusicStop,
    Voice { id: u16 },
    Pcm { id: u16, kind: u8 },
    Save { slot: u16, data: Vec<u8> },
    Log(String),
}

pub struct Engine {
    pub files: HashMap<String, Vec<u8>>,
    pub mem: SubMem,
    pub disp: Display,
    pub font: Option<Font>,
    pub names: Vec<(u8, Vec<u8>)>,
    pub input: Option<Input>,
    pub vm: Vm,
    pub script: Vec<u8>,
    pub phase: VmPhase,
    pub queue: Vec<Action>,
    pub qi: usize,
    pub seq_step: u16,
    pub seq_timer: u32,
    pub msg: Option<MessageBox>,
    pub text: TextWindow,
    pub menu: Option<(TextMenu, Vec<u16>)>,
    pub regs: Regs,
    pub data_file: Option<u16>,
    pub frame: u64,
    pub out: Vec<Out>,
    /// frames left until the current voice / PCM effect reports "done" ($980E / $980F)
    pub voice_left: u32,
    pub sfx_left: u32,
    /// CD time base: frame when the current CD-DA track started
    pub cd_start: Option<u64>,
    /// optional durations (frames) of voice clips, filled by the front end from a manifest
    pub voice_len: HashMap<u16, u32>,
    pub text_dirty: bool,
    pub fb: Vec<u8>,
    pub trace: bool,
    pub top: Top,
    pub module: Option<Box<dyn Module>>,
    /// intro part counter (DATA_D1 then DATA_D0, table S:0x14AA0)
    pub intro_part: u8,
    /// save blocks available to CONTINUE (slot 1..4 → 0x1C0 bytes), provided by the front end
    pub saves: [Option<Vec<u8>>; 4],
    /// factory for the module of a given top mode (set by lib.rs; None = skip the module)
    pub module_factory: Option<fn(Top, u8) -> Option<Box<dyn Module>>>,
}

fn rd16(a: &[u16], i: usize) -> u16 { a.get(i).copied().unwrap_or(0) }

impl Default for Engine { fn default() -> Self { Self::new() } }

impl Engine {
    pub fn new() -> Self {
        let (vm, _) = Vm::new_game();
        Engine {
            files: HashMap::new(), mem: SubMem::new(), disp: Display::new(), font: None, names: vec![], input: None,
            vm, script: vec![], phase: VmPhase::Stopped, queue: vec![], qi: 0, seq_step: 0, seq_timer: 0, msg: None,
            text: TextWindow::new(), menu: None, regs: Regs::default(), data_file: None, frame: 0, out: vec![],
            voice_left: 0, sfx_left: 0, cd_start: None, voice_len: HashMap::new(), text_dirty: true,
            fb: vec![0; W * H * 4], trace: false,
            top: Top::Stopped, module: None, intro_part: 0, saves: [None, None, None, None], module_factory: None,
        }
    }

    pub fn load_file(&mut self, name: &str, data: Vec<u8>) {
        let name = name.to_ascii_uppercase();
        if name == "SUBCODE.BIN" {
            self.mem.map(gfx::SUBCODE_BASE, data.clone());
            self.font = Some(Font::from_subcode(&data));
            self.names = actions::speaker_names(&data);
            self.input = Some(Input::from_subcode(&data));
        }
        self.files.insert(name, data);
    }

    pub fn file_by_index(&self, idx: u16) -> Option<&Vec<u8>> {
        FILE_NAMES.get(idx as usize).and_then(|n| self.files.get(*n))
    }

    /// Power-on: title screen module (DATA_D2), then intro or continue, then the game.
    pub fn boot(&mut self) {
        self.top = Top::Title;
        self.enter_module(Top::Title, 4);
    }

    /// Map the module's DATA file at 0x28000 and create its module (if implemented).
    fn enter_module(&mut self, top: Top, file: u16) {
        self.cmd_load(gfx::DATA_BASE, file);
        self.module = self.module_factory.and_then(|f| f(top, self.intro_part));
        if self.module.is_none() { self.module_done(1); }
    }

    /// Mode machine transitions after a module set `$7846` (S:0xE046 modes 1, 2, 6).
    fn module_done(&mut self, v: i8) {
        self.module = None;
        match self.top {
            Top::Title => {
                if self.regs.get_b(0x785C) != 0 {
                    // CONTINUE: VM state 1 continue branch with the loaded save block
                    let slot = self.regs.get_b(0x78B7) as usize;
                    let blk = self.saves.get(slot.saturating_sub(1)).cloned().flatten().unwrap_or_default();
                    self.continue_game(&blk);
                } else {
                    self.top = Top::Intro; self.intro_part = 0;
                    self.enter_module(Top::Intro, 3);
                }
            }
            Top::Intro => {
                // table S:0x14AA0: DATA_D1 then DATA_D0; skipping (-1) ends the intro at once
                if v >= 0 && self.intro_part == 0 { self.intro_part = 1; self.enter_module(Top::Intro, 2); }
                else { self.new_game(); }
            }
            Top::Ending => { self.regs.set_b(0x78AE, 0xFF); self.top = Top::Stopped; }
            _ => {}
        }
    }

    /// Continue from a save block (VM state 1, `$785C` != 0).
    pub fn continue_game(&mut self, blk: &[u8]) {
        self.top = Top::Game;
        let (vm, q) = Vm::from_save(blk, &mut self.regs);
        self.vm = vm;
        self.start_queue(q);
        self.phase = VmPhase::Draining;
    }

    /// Start a new game at SP06:0 (VM main-loop state 1, new-game branch).
    pub fn new_game(&mut self) {
        self.top = Top::Game;
        let (vm, q) = Vm::new_game();
        self.vm = vm;
        self.start_queue(q);
        self.phase = VmPhase::Draining;
    }

    /// Debug: jump to `SPnn:pc` (hex pc) with a fresh VM (e.g. "SP01:0" = developer scene select).
    pub fn jump(&mut self, script_id: u16, pc: u16) {
        self.top = Top::Game;
        let mut vm = Vm::blank();
        vm.state = 2;
        vm.script_id = script_id;
        vm.pc = pc;
        self.vm = vm;
        self.start_queue(vec![Action { cmd: 0x11, args: vec![0x1, 0xA800, script_id] }, Action { cmd: 0x0F, args: vec![] }]);
        self.phase = VmPhase::Draining;
    }

    fn start_queue(&mut self, q: Vec<Action>) {
        self.queue = q;
        self.qi = 0;
        self.seq_step = 0;
        self.seq_timer = 0;
    }

    pub fn busy(&self) -> bool { self.qi < self.queue.len() }

    // ------------------------------------------------------------------ frame

    pub fn tick(&mut self, pad: u8) {
        self.frame += 1;
        if let Some(inp) = self.input.as_mut() { inp.update(pad); }
        if self.voice_left > 0 { self.voice_left -= 1; if self.voice_left == 0 { self.regs.set_b(0x980E, 0); } }
        if self.sfx_left > 0 { self.sfx_left -= 1; if self.sfx_left == 0 { self.regs.set_b(0x980F, 0); } }
        if let Some(s) = self.cd_start { self.disp.sub.cd_time = cd_time_bcd(self.frame - s); }

        // VM thread (runs while the sequencer is idle)
        if self.top == Top::Game {
            if !self.busy() { self.vm_thread(); }
            if self.busy() { self.seq_run(); }
            if self.vm.state == 3 && self.top == Top::Game {
                // SYS SETSTATE(1): mode 6 – late-game module DATA_Y16 (file 0x34)
                self.top = Top::Ending;
                self.enter_module(Top::Ending, 0x34);
            }
        } else if let Some(mut m) = self.module.take() {
            match m.tick(self) {
                Some(v) => { self.module = None; self.module_done(v); }
                None => { if self.module.is_none() { self.module = Some(m); } }
            }
        }

        self.disp.frame(&self.mem);
        for ev in self.disp.sub.events.drain(..) {
            match ev {
                gfx::anim::Event::Sound { id, kind } => self.out.push(Out::Pcm { id: id as u16, kind }),
                gfx::anim::Event::Periodic { id } => self.out.push(Out::Pcm { id: id as u16, kind: 0xFF }),
                _ => {}
            }
        }
        if let Some(font) = self.font.as_ref() {
            if self.text.busy { self.text.tick(font); self.text_dirty = true; }
        }
        if self.text_dirty { self.upload_text(); self.text_dirty = false; }
        let mut fb = std::mem::take(&mut self.fb);
        self.disp.render(&mut fb);
        self.fb = fb;
    }

    /// Text bitmap (256×64, 4bpp) → VRAM tiles 0–255, row-major (S:0x13F46 DMA rows).
    fn upload_text(&mut self) {
        let v = &mut self.disp.vdp.vram;
        for ty in 0..8 {
            for tx in 0..32 {
                let t = ty * 32 + tx;
                for r in 0..8 {
                    for c in 0..4 {
                        let x = tx * 8 + c * 2; let y = ty * 8 + r;
                        let hi = self.text.pix[y * actions::WIN_W + x] & 15;
                        let lo = self.text.pix[y * actions::WIN_W + x + 1] & 15;
                        v[t * 32 + r * 4 + c] = hi << 4 | lo;
                    }
                }
            }
        }
    }

    /// `txt_window_init` S:0x13D4E: clear bitmap and write the 32×8 nametable at VRAM 0xEC80.
    fn text_window_init(&mut self) {
        self.text.clear();
        for i in 0..256u16 {
            let w = 0x8000 | if i < 255 { i } else { 0 };
            let a = 0xEC80 + 2 * i as usize;
            self.disp.vdp.vram[a] = (w >> 8) as u8;
            self.disp.vdp.vram[a + 1] = w as u8;
        }
        self.text_dirty = true;
    }

    // ------------------------------------------------------------------ VM thread

    fn vm_thread(&mut self) {
        match self.phase {
            VmPhase::Stopped => {}
            VmPhase::Draining => {
                // queue finished (sequencer command 00 / end of list)
                if self.vm.state != 2 { self.phase = VmPhase::Stopped; self.out.push(Out::Log(format!("vm state {}", self.vm.state))); return; }
                match self.vm.finish_pass() {
                    PassEnd::Continue => { self.phase = VmPhase::Pass; self.vm_pass(); }
                    PassEnd::Menu(items) => self.open_menu(items),
                }
            }
            VmPhase::Pass => self.vm_pass(),
            VmPhase::Menu => self.menu_frame(),
        }
    }

    fn vm_pass(&mut self) {
        if self.script.is_empty() { return; }
        let script = std::mem::take(&mut self.script);
        let q = self.vm.run_pass(&script, &mut self.regs);
        self.script = script;
        if self.trace { self.out.push(Out::Log(format!("pass pc={:04x} q={:?}", self.vm.pc, q.iter().map(|a| (a.cmd, a.args.clone())).collect::<Vec<_>>()))); }
        self.start_queue(q);
        self.phase = VmPhase::Draining;
    }

    fn open_menu(&mut self, items: Vec<u16>) {
        let depth = self.vm.path.len() as usize;
        let remembered = self.regs.get_b(0x9716);
        let wide = self.regs.get_b(0x7893) != 0;
        let (m, rem) = TextMenu::new(items.len(), depth, remembered, wide);
        self.regs.set_b(0x9716, rem);
        self.menu = Some((m, items));
        self.phase = VmPhase::Menu;
        self.disp.sub.scroll_mode(if wide { 0x12 } else { 0x0D });
        self.draw_menu();
    }

    fn script_text(&self, off: u16) -> &[u8] {
        let o = 0x3800 + off as usize;
        self.script.get(o..).unwrap_or(&[])
    }

    /// Draw the visible menu lines (S:0x10D9A: `FC 00 F9 01 FE c FB 10 y text FF`).
    fn draw_menu(&mut self) {
        let Some((m, items)) = self.menu.as_ref() else { return };
        let lines = m.visible();
        let cursor = m.cursor as usize;
        let items = items.clone();
        let font = match self.font.as_ref() { Some(f) => f, None => return };
        self.text.clear();
        let mut win = std::mem::take(&mut self.text);
        for (i, l) in lines.iter().enumerate() {
            let col = if i == cursor { 1 } else { 4 };
            let mut s = vec![0xFC, 0x00, 0xF9, 0x01, 0xFE, col, 0xFB, 0x10, (i * 9) as u8];
            match l {
                MenuLine::Item(k) => {
                    let t = self.script_text(items[*k]);
                    for &c in t.iter().take(0x20) { if c == 0xFF || c == 0xF2 { break } s.push(self.letter(c)); }
                }
                MenuLine::Arrow { down } => { s.extend_from_slice(if *down { &[0x82u8][..] } else { &[0x80u8][..] }); }
            }
            s.push(0xFF);
            win.start(&s);
            while win.busy { win.tick(font); }
        }
        self.text = win;
        self.text_dirty = true;
    }

    fn letter(&self, c: u8) -> u8 {
        match (c, self.input.as_ref()) { (0xEC | 0xEE | 0xF0, Some(i)) => i.button_letter(c), _ => c }
    }

    fn menu_frame(&mut self) {
        let (pressed, held, repeat) = self.input.as_ref().map(|i| (i.pressed, i.held, i.repeat)).unwrap_or((0, 0, 0));
        let frame = self.frame as u16;
        let Some((m, _)) = self.menu.as_mut() else { self.phase = VmPhase::Pass; return };
        let before = (m.cursor, m.page);
        let r = m.input(pressed, held, repeat, frame);
        let moved = (m.cursor, m.page) != before;
        let depth0 = m.depth == 0;
        let cursor = m.cursor;
        match r {
            None => { if moved { self.draw_menu(); } }
            Some(res) => {
                self.menu = None;
                self.text_window_init();
                match res {
                    MenuResult::Selected(n) => {
                        if depth0 { self.regs.set_b(0x9716, cursor); }
                        self.out.push(Out::Pcm { id: 0x1D, kind: 0 });
                        self.vm.choose(Some(n as usize - 1));
                    }
                    MenuResult::Cancel => self.vm.choose(None),
                }
                self.phase = VmPhase::Pass;
                self.vm_pass();
            }
        }
    }

    // ------------------------------------------------------------------ sequencer

    /// `seq_run` S:0xFFC4: execute commands until one waits (or the queue ends).
    fn seq_run(&mut self) {
        let mut guard = 0;
        while self.qi < self.queue.len() && guard < 64 {
            guard += 1;
            let a = self.queue[self.qi].clone();
            if self.seq_cmd(&a) {
                self.qi += 1;
                self.seq_step = 0;
                self.seq_timer = 0;
            } else {
                return;
            }
        }
    }

    /// Returns true when the command completed this frame.
    fn seq_cmd(&mut self, a: &Action) -> bool {
        let w = |i: usize| rd16(&a.args, i);
        match a.cmd {
            0x00 | 0x06 | 0x14 | 0x1A => true,
            0x02 => { // pause objects + fade to black, wait 0x30 frames
                if self.seq_step == 0 {
                    for o in self.disp.sub.objs.iter_mut() { o.flags |= 1; }
                    self.seq_step = 1; self.seq_timer = 0x30;
                }
                self.seq_timer = self.seq_timer.saturating_sub(1);
                self.seq_timer == 0
            }
            0x04 => { self.regs.set_b(0x78C3, 0xFF); true }
            0x07 => { // voice/PCM with done flag $980E (subfn_12(n,1))
                let n = w(0);
                self.regs.set_b(0x980E, if n == 0xFF { 0 } else { n as u8 });
                self.voice_left = *self.voice_len.get(&n).unwrap_or(&1);
                self.out.push(Out::Pcm { id: n, kind: 1 });
                true
            }
            0x08 => self.cmd_message(w(0), w(1)),
            0x09 => { self.obj_control(w(0), w(1) as usize) }
            0x0A => { // wait until voice idle or 0xE10 frames
                self.seq_timer += 1;
                self.voice_left == 0 || self.seq_timer >= 0xE10
            }
            0x0B => true, // CD drive ready
            0x0C => {
                if self.regs.get_b(0x7890) == 0 && self.voice_left == 0 { self.out.push(Out::Pcm { id: w(0), kind: 2 }); }
                true
            }
            0x0D => {
                let n = w(0);
                self.regs.set_b(0x9717, 0xFF);
                self.voice_left = *self.voice_len.get(&n).unwrap_or(&90);
                self.out.push(Out::Voice { id: n });
                true
            }
            0x0E => { let t = w(0); self.cd_start = Some(self.frame); self.out.push(Out::Music { track: t }); true }
            0x0F => { self.scene_clear(); true }
            0x10 => {
                if self.seq_step == 0 { self.seq_step = 1; self.seq_timer = w(0) as u32; }
                if self.seq_timer == 0 { return true; }
                self.seq_timer -= 1;
                self.seq_timer == 0
            }
            0x11 => { self.cmd_load(((w(0) as u32) << 16) | w(1) as u32, w(2)); true }
            0x12 => { self.disp.sub.f788a &= !1; self.disp.load_scene(&self.mem, w(0) as usize); true } // HYPOTHESIS: bit0 re-armed per scene
            0x13 => { self.seq_timer += 1; self.regs.get_b(0x980E) == 0 || self.seq_timer >= 0xA8C }
            0x15 => { self.out.push(Out::Log(format!("builtin scene {} pos {}", w(0), w(1)))); true }
            0x16 => { self.text_window_init(); true }
            0x17 => { self.regs.set_b(0x9717, 0xFF); true }
            0x18 => { self.out.push(Out::Log(format!("minigame stage {:#x} (not implemented)", self.regs.get(0x78E0)))); self.regs.set_b(0x78AF, 0); true }
            0x19 => { self.seq_timer += 1; self.regs.get_b(0x980F) == 0 || self.seq_timer >= 0x258 }
            0x1B => {
                let want = ((w(0) as u32) << 16) | ((w(1) as u32) << 8) | w(2) as u32;
                self.seq_timer += 1;
                self.disp.sub.cd_time >= want || self.seq_timer > 60 * 600
            }
            0x1C => {
                let n = w(0);
                self.regs.set_b(0x980F, if n == 0x3D { 0 } else { n as u8 });
                self.sfx_left = 30;
                self.out.push(Out::Pcm { id: n, kind: 0 });
                true
            }
            0x1D => { self.regs.set_b(0x7890, w(0) as u8); true }
            0x1E => {
                self.out.push(Out::Save { slot: w(0), data: self.vm.save.clone() });
                self.regs.set_b(0x79E0, 0);
                true
            }
            0x1F => true,
            0x20 => { self.disp.load_scene(&self.mem, w(0) as usize); true }
            0x21 => { self.input.as_ref().map(|i| i.pressed & btn::START != 0).unwrap_or(true) }
            c => { self.out.push(Out::Log(format!("seq cmd {c:#x} ignored"))); true }
        }
    }

    fn cmd_load(&mut self, addr: u32, file: u16) {
        let Some(data) = self.file_by_index(file).cloned() else {
            self.out.push(Out::Log(format!("missing file {file:#x}")));
            return;
        };
        match addr {
            0x1A800 => { self.script = data; }
            0x28000 => { self.mem.map(gfx::DATA_BASE, data); self.data_file = Some(file); }
            _ => {}
        }
    }

    /// Sequencer command 0F (S:0x1027E): black-out unless palettes are kept, then
    /// `subfn_17` (S:0xF5E8) scene reset.
    fn scene_clear(&mut self) {
        let sub = &self.disp.sub;
        let keep = (sub.f788b & 0x7F) | (sub.f788a & 1);
        if keep == 0 { self.black_out(); }
        // subfn_17
        if self.disp.sub.f788b == 0 { self.black_out(); }
        for o in self.disp.sub.objs.iter_mut() {
            if o.f4e & 1 == 0 { *o = Default::default(); }
        }
        if self.disp.sub.f7866 != 0 {
            self.disp.sub.f7866 = 0;
            const TEXT_PAL: [u16; 8] = [0x000, 0xEEE, 0x0EE, 0x86E, 0xEE0, 0xE42, 0x4E0, 0x000];
            self.disp.sub.shadow[..8].copy_from_slice(&TEXT_PAL);
        }
        // main screen clear request ($7887 → B6403 = 3): clear planes A and B (not the text rows)
        for a in (0xC000..0xD000).chain(0xE000..0xEC80) { self.disp.vdp.vram[a] = 0; }
    }

    /// S:0xF560: palette black-out (set A from colour 16, or all 64 when $788A bit1/$7866;
    /// set B from colour 16 unless $788A bit2). Runs once until $788A bit0 is cleared.
    fn black_out(&mut self) {
        let sub = &mut self.disp.sub;
        if sub.f788a & 1 != 0 { return; }
        sub.f788a |= 1;
        let all = sub.f788a & 2 != 0 || sub.f7866 != 0;
        sub.f788a &= !2;
        let from = if all { 0 } else { 16 };
        for c in sub.shadow[from..64].iter_mut() { *c = 0; }
        if sub.f788a & 4 == 0 { for c in sub.shadow[64 + 16..128].iter_mut() { *c = 0; } }
        sub.f788a &= !4;
    }

    /// Command 09 (S:0xFDD2): object control on `slot` and its children.
    fn obj_control(&mut self, op: u16, slot: usize) -> bool {
        let mut list = vec![slot & 63];
        let mut c = self.disp.sub.objs[slot & 63].child as usize;
        let mut n = 0;
        while c != 0 && c < 64 && n < 64 { list.push(c); c = self.disp.sub.objs[c].sibling as usize; n += 1; }
        match op {
            0 => for &i in &list { self.disp.sub.objs[i].flags &= 2; },
            1 => for &i in &list { self.disp.sub.objs[i].flags |= 1; },
            2 => for &i in &list { self.disp.sub.objs[i].flags = 3; self.disp.sub.objs[i].f4e = 0; },
            3 => {
                let o = &mut self.disp.sub.objs[slot & 63];
                if o.active != 1 { return true; }
                if o.f4e & 2 != 0 { o.f4e &= !2; return true; }
                self.seq_timer += 1;
                return self.seq_timer > 60 * 120;
            }
            4 => for &i in &list { self.disp.sub.objs[i].f4e = 0xFF; },
            5 => for &i in &list { let o = &mut self.disp.sub.objs[i]; o.f4e = 0; o.flags = 0; o.idx = 0xFFFF; o.delay = -1; },
            _ => {}
        }
        true
    }

    /// Command 08: message box (S:0x135B2).
    fn cmd_message(&mut self, spk: u16, off: u16) -> bool {
        if self.msg.is_none() {
            let wide = self.regs.get_b(0x7893);
            self.msg = Some(MessageBox::new(spk, off, wide));
        }
        let confirm = self.input.as_ref().map(|i| i.pressed & btn::L_CONFIRM != 0).unwrap_or(false);
        let fast = self.input.as_ref().map(|i| i.held & btn::L_CONFIRM != 0).unwrap_or(false) || self.regs.get_b(0x9727) != 0;
        if fast && self.text.busy { self.text.set_fast(); }
        let text: Vec<u8> = {
            let t = self.script.get(0x3800..).unwrap_or(&[]);
            t.iter().map(|&c| self.letter(c)).collect()
        };
        let mut m = self.msg.take().unwrap();
        let ev = m.tick(&mut self.text, &text, &self.names, confirm);
        if self.text.busy { self.text_dirty = true; }
        match ev {
            MsgEvent::Done => {
                if spk & 0xFF == 1 { self.regs.set_b(0x9717, 0xFF); }
                return true;
            }
            MsgEvent::ScreenCmd(n) => { if n == 0x0F || n == 0x14 || n == 0x16 { self.text_window_init(); } self.disp.sub.scroll_mode(n); }
            MsgEvent::ClearWindow => self.text_window_init(),
            MsgEvent::None => {}
        }
        self.msg = Some(m);
        false
    }

    pub fn debug_info(&self) -> String {
        let a = self.queue.get(self.qi).map(|a| format!("{:02X}{:?}", a.cmd, a.args)).unwrap_or_default();
        format!("{{\"frame\":{},\"script\":\"{}\",\"pc\":\"{:04x}\",\"phase\":\"{:?}\",\"seq\":\"{}\",\"data\":\"{}\",\"path\":{}}}",
            self.frame,
            FILE_NAMES.get(self.vm.script_id as usize).unwrap_or(&"?"),
            self.vm.pc, self.phase, a,
            self.data_file.and_then(|f| FILE_NAMES.get(f as usize)).unwrap_or(&"-"),
            self.vm.path.len())
    }
}

/// Frames since track start → BCD-ish MSF word used by commands 1B / anim 99 (00:MM:SS:FF as 0x00MMSSFF).
fn cd_time_bcd(frames: u64) -> u32 {
    let f75 = frames * 75 / 60;
    let (m, s, f) = ((f75 / 75 / 60) as u32, ((f75 / 75) % 60) as u32, (f75 % 75) as u32);
    let bcd = |v: u32| ((v / 10) << 4) | (v % 10);
    (bcd(m) << 16) | (bcd(s) << 8) | bcd(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Option<Engine> {
        let raw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw/files");
        if !raw.join("SUBCODE.BIN").exists() { eprintln!("assets missing, skipped"); return None; }
        let mut e = Engine::new();
        for n in FILE_NAMES.iter() {
            if *n == "PCMLD_01.BIN" { continue; }
            if let Ok(d) = std::fs::read(raw.join(n)) { e.load_file(n, d); }
        }
        Some(e)
    }

    #[test]
    fn new_game_reaches_first_menu() {
        let Some(mut e) = engine() else { return };
        e.new_game();
        let mut saw_menu = false;
        for f in 0..6000 {
            // press confirm every 20 frames to advance messages
            let pad = if f % 20 == 0 { btn::C } else { 0 };
            e.tick(pad);
            if e.phase == VmPhase::Menu { saw_menu = true; break; }
        }
        assert!(saw_menu, "never reached a menu: {}", e.debug_info());
        assert_eq!(e.vm.script_id, 0x3F);
    }
}
