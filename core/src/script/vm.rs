//! Reference interpreter for the scenario VM – a byte-level port of the sub-CPU handlers
//! (S:0x16974 dispatch, S:0x16CDC op table, handlers S:0x16E08–0x177A2, helpers
//! S:0x1699A–0x16CDA, main-loop state 2 S:0x16814). Spec: docs/re/script.md.
//!
//! The VM is engine-agnostic: one call to [`Vm::run_pass`] evaluates the node at `pc` and
//! returns the action-queue entries (the original `$9940` queue) that the engine must perform
//! (dialogue, images, sound, file loads …). Afterwards the engine calls [`Vm::finish_pass`],
//! shows the menu if one was collected and reports the choice with [`Vm::choose`].
//! Addresses that scripts PEEK/POKE and engine registers used by snapshots are accessed via
//! the [`Host`] trait using their original sub-CPU addresses.

use super::{op_info, var_read, var_write, HANDLER_CASE, HANDLER_LIST};

/// Size of the save block `$9C00..$9DBF` (written to backup RAM by the save command).
pub const SAVE_SIZE: usize = 0x1C0;
/// Offset of the variable bit array (`$9C60`) inside the save block.
pub const VARS_OFF: usize = 0x60;
/// Script load address (the address argument of Q 11 for scripts).
pub const SCRIPT_ADDR: u32 = 0x1A800;
/// DATA file load address (Q 11 for SCENE).
pub const DATA_ADDR: u32 = 0x28000;
/// File id of the first script of a new game (SP06).
pub const NEW_GAME_SCRIPT: u16 = 0x3F;

/// Bounded word stack with the original layout semantics (S:0x1071E–0x10840).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
    pub cap: u8,
    pub items: Vec<u16>,
}

impl Stack {
    pub fn new(cap: u8) -> Self { Stack { cap, items: Vec::with_capacity(cap as usize) } }
    pub fn len(&self) -> u8 { self.items.len() as u8 }
    pub fn is_empty(&self) -> bool { self.items.is_empty() }
    /// S:0x107DC
    pub fn get(&self, i: u8) -> Option<u16> { self.items.get(i as usize).copied() }
    /// S:0x107FE
    pub fn top(&self) -> Option<u16> { self.items.last().copied() }
    /// S:0x107F4: get(count - n) with byte arithmetic
    pub fn peek_from_top(&self, n: u8) -> Option<u16> { self.get(self.len().wrapping_sub(n)) }
    /// S:0x10806 / S:0x10732: push if not full
    pub fn push(&mut self, v: u16) -> bool {
        if self.len() < self.cap { self.items.push(v); true } else { false }
    }
    /// S:0x1071E: push, dropping the oldest element when full
    pub fn push_drop_oldest(&mut self, v: u16) {
        if self.len() >= self.cap && !self.items.is_empty() { self.items.remove(0); }
        self.push(v);
    }
    /// S:0x10778
    pub fn drop_top(&mut self) { self.items.pop(); }
    /// S:0x10816
    pub fn pop(&mut self) -> Option<u16> { self.items.pop() }
    /// S:0x10788
    pub fn contains(&self, v: u16) -> bool { self.items.contains(&v) }
    /// S:0x1082C
    pub fn truncate(&mut self, n: u8) { self.items.truncate(n as usize) }
    /// S:0x10840
    pub fn clear(&mut self) { self.items.clear() }
}

/// One entry of the action queue (`$9940`): command number and its argument words.
/// Q 11 (load file) carries the 32-bit address as two words (hi, lo) followed by the file id,
/// exactly like the original queue layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub cmd: u16,
    pub args: Vec<u16>,
}

impl Action {
    fn new(cmd: u16, args: &[u16]) -> Self { Action { cmd, args: args.to_vec() } }
    fn load(addr: u32, file: u16) -> Self { Action::new(0x11, &[(addr >> 16) as u16, addr as u16, file]) }
}

/// Engine services needed by the VM.
pub trait Host {
    /// Word read of an engine register by its sub-CPU address (SYS PEEK, snapshots).
    fn peek(&mut self, addr: u32) -> u16;
    /// Word write (SYS POKE, snapshot restore).
    fn poke(&mut self, addr: u32, val: u16);
    /// Byte write.
    fn poke_byte(&mut self, addr: u32, val: u8);
    /// Animation-object snapshot (64 bytes, format of S:0x14FB4) into the save block.
    fn snapshot_objects(&mut self, out: &mut [u8]) { out.fill(0) }
}

/// Result of [`Vm::finish_pass`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassEnd {
    /// No menu: run the next pass.
    Continue,
    /// Show a menu: items are text offsets (relative to 0x3800) in the current script.
    Menu(Vec<u16>),
}

#[derive(Debug, Clone)]
pub struct Vm {
    /// `$9C00..$9DBF`
    pub save: Vec<u8>,
    /// `$9800`
    pub pc: u16,
    /// `$9802` ($FFFF = none)
    pub pending: u16,
    /// `$980A`
    pub scene: u16,
    /// `$980C`
    pub script_id: u16,
    /// `$9808`
    pub state: u16,
    /// `$9804`
    pub depth: u8,
    /// `$9805`
    pub leaf: bool,
    pub marks: Stack,
    pub menu: Stack,
    pub path: Stack,
    pub calls: Stack,
    pub history: Stack,
    pub gosubs: Stack,
    /// d5 – "value" register shared by all handlers
    pub d5: u16,
    /// RNG state `$786A` (a) and `$786C` (c)
    pub rng_a: u16,
    pub rng_c: u16,
    queue: Vec<Action>,
    /// guard against runaway recursion in malformed data
    depth_guard: u32,
}

crate::snap_struct!(Stack { cap, items });
crate::snap_struct!(Action { cmd, args });
crate::snap_struct!(Vm { save, pc, pending, scene, script_id, state, depth, leaf, marks, menu, path, calls, history, gosubs, d5, rng_a, rng_c, queue, depth_guard });

fn rd(s: &[u8], o: usize) -> u16 {
    if o + 1 < s.len() { u16::from_be_bytes([s[o], s[o + 1]]) } else { 0xFFFF }
}

impl Vm {
    /// State-1 initialisation for a new game (S:0x16734–0x167A0).
    pub fn new_game() -> (Vm, Vec<Action>) {
        let mut vm = Vm::blank();
        vm.script_id = NEW_GAME_SCRIPT;
        vm.state = 2;
        let q = vec![Action::load(SCRIPT_ADDR, NEW_GAME_SCRIPT), Action::new(0x0F, &[])];
        (vm, q)
    }

    /// Fresh VM with the stack capacities of state 1 and $FFFF markers.
    pub fn blank() -> Vm {
        Vm {
            save: vec![0; SAVE_SIZE],
            pc: 0,
            pending: 0xFFFF,
            scene: 0xFFFF,
            script_id: 0xFFFF,
            state: 1,
            depth: 0,
            leaf: false,
            marks: Stack::new(8),
            menu: Stack::new(31),
            path: Stack::new(15),
            calls: Stack::new(18),
            history: Stack::new(3),
            gosubs: Stack::new(16),
            d5: 0,
            rng_a: 0,
            rng_c: 0,
            queue: Vec::new(),
            depth_guard: 0,
        }
    }

    /// State-1 "continue" path: restore from a save block (S:0x167A2–0x167CA).
    /// Returns the actions to run (script load + scene restore).
    pub fn from_save(save: &[u8], host: &mut dyn Host) -> (Vm, Vec<Action>) {
        let mut vm = Vm::blank();
        vm.save[..save.len().min(SAVE_SIZE)].copy_from_slice(&save[..save.len().min(SAVE_SIZE)]);
        let w = |o: usize| u16::from_be_bytes([vm.save[o], vm.save[o + 1]]);
        // $79EA (long) and $79F0 from $9C02..$9C07
        host.poke(0x79EA, w(2));
        host.poke(0x79EC, w(4));
        host.poke(0x79F0, w(6));
        let id = w(0x0C);
        vm.pc = w(0x0E);
        // history stack image (cap, count, 3 words) at $9C10
        vm.history = Stack::new(vm.save[0x10].max(3));
        for i in 0..vm.save[0x11].min(3) as usize { vm.history.items.push(w(0x12 + 2 * i)); }
        vm.script_id = id;
        vm.queue.push(Action::load(SCRIPT_ADDR, id));
        vm.restore_scene(host);
        vm.state = 2;
        let q = std::mem::take(&mut vm.queue);
        (vm, q)
    }

    pub fn vars(&self) -> &[u8] { &self.save[VARS_OFF..] }
    pub fn var(&self, desc: u16) -> u16 { var_read(&self.save[VARS_OFF..], desc) }
    pub fn set_var(&mut self, desc: u16, v: u16) { var_write(&mut self.save[VARS_OFF..], desc, v) }

    /// One VM pass (S:0x16870): evaluate the node at `pc`. Returns the queued actions.
    pub fn run_pass(&mut self, script: &[u8], host: &mut dyn Host) -> Vec<Action> {
        self.leaf = false;
        self.depth_guard = 0;
        let pc = self.pc as usize;
        self.dispatch(script, pc, host);
        std::mem::take(&mut self.queue)
    }

    /// Steps after the queued actions completed (S:0x16840–0x1694E without the menu UI).
    pub fn finish_pass(&mut self) -> PassEnd {
        if self.pending != 0xFFFF {
            self.history.push_drop_oldest(self.pc);
            self.pc = self.pending;
            self.pending = 0xFFFF;
        }
        if !self.menu.is_empty() {
            return PassEnd::Menu(self.menu.items.clone());
        }
        let m = self.marks.top().unwrap_or(0) as u8;
        self.path.truncate(m);
        self.menu.clear();
        PassEnd::Continue
    }

    /// Menu result (S:0x168CA): `Some(i)` = item i (0-based) chosen, `None` = cancel.
    pub fn choose(&mut self, sel: Option<usize>) {
        match sel {
            Some(i) => {
                if let Some(v) = self.menu.get(i as u8) { self.path.push(v); }
            }
            None => {
                let m = self.marks.top().unwrap_or(0) as u8;
                if m != self.path.len() { self.path.drop_top(); }
            }
        }
        self.menu.clear();
    }

    // ---------------------------------------------------------------- helpers

    fn handler(s: &[u8], a: usize) -> u32 {
        s.get(a).and_then(|&op| op_info(op)).map(|i| i.handler).unwrap_or(0)
    }
    fn types(s: &[u8], a: usize) -> (u8, u8, u8, bool) {
        match s.get(a).and_then(|&op| op_info(op)) {
            Some(i) => (i.t1, i.t2, i.t3, i.has_link()),
            None => (0, 0, 0, false),
        }
    }

    /// S:0x1699A
    fn skip(&self, s: &[u8], mut a: usize) -> usize {
        for _ in 0..100_000 {
            let (t1, t2, t3, link) = Self::types(s, a);
            if link { a = rd(s, a + 1) as usize; continue }
            a += 1;
            for t in [t1, t2, t3] {
                match t {
                    0 | 7 => {}
                    4 => a = self.skip(s, a),
                    _ => a += 2,
                }
            }
            return a;
        }
        a
    }
    /// S:0x16A6E
    fn is_list(s: &[u8], a: usize) -> bool { Self::handler(s, a) == HANDLER_LIST }
    /// S:0x16A30
    fn car_pos(s: &[u8], a: usize) -> usize { a + if Self::types(s, a).3 { 3 } else { 1 } }
    /// S:0x16A8E: if the car is a CASE node return its key
    fn car_case(s: &[u8], a: usize) -> Option<u16> {
        if Self::types(s, a).0 != 4 { return None }
        let c = Self::car_pos(s, a);
        if Self::handler(s, c) == HANDLER_CASE { Some(rd(s, c + 1)) } else { None }
    }
    /// S:0x16BE2
    fn count(&self, s: &[u8], mut a: usize) -> u8 {
        let mut n: u8 = 0;
        for _ in 0..10_000 {
            n = n.wrapping_add(1);
            if !Self::is_list(s, a) { return n }
            if let Some(k) = Self::car_case(s, a) { n = n.wrapping_add((k as u8).wrapping_sub(1)); }
            let (_, t2, _, link) = Self::types(s, a);
            if t2 == 7 { return n }
            if link { a = rd(s, a + 1) as usize; continue }
            a = self.skip(s, a + 1);
        }
        n
    }
    /// S:0x16C32 (1-based)
    fn nth(&self, s: &[u8], mut a: usize, n0: u8) -> usize {
        let mut n = n0;
        for _ in 0..10_000 {
            if !Self::is_list(s, a) { return a }
            if let Some(k) = Self::car_case(s, a) { n = n.wrapping_sub((k as u8).wrapping_sub(1)); }
            n = n.wrapping_sub(1);
            if (n as i8) <= 0 { return Self::car_pos(s, a) }
            let (t1, _, _, link) = Self::types(s, a);
            if link { a = rd(s, a + 1) as usize; continue }
            a = if t1 == 4 { self.skip(s, a + 1) } else { a + 3 };
        }
        a
    }
    /// S:0x16ACA: evaluate operand of type `t` at `a`; returns (value, position after operand)
    fn eval(&mut self, s: &[u8], a: usize, t: u8, host: &mut dyn Host) -> (u16, usize) {
        match t {
            1 => { let d = rd(s, a); (self.var(d), a + 2) }
            3 | 5 => (rd(s, a), a + 2),
            _ => {
                self.dispatch(s, a, host);
                (self.d5, self.skip(s, a))
            }
        }
    }
    /// S:0x16C90
    fn guard(&self) -> bool {
        if !self.menu.is_empty() || self.pending != 0xFFFF { return false }
        if !self.leaf { return self.depth == self.path.len() }
        match self.marks.top() {
            None => true,
            Some(m) => (m as u8) < self.depth,
        }
    }
    fn q(&mut self, cmd: u16, args: &[u16]) { self.queue.push(Action::new(cmd, args)) }

    /// RNG S:0x0FD96 (state $786A/$786C)
    pub fn rand16(&mut self) -> u16 {
        let [a_hi, a_lo] = self.rng_a.to_be_bytes();
        let _ = a_lo;
        // add.b $786B,$786A
        let mut a = self.rng_a;
        let hi = a_hi.wrapping_add(self.rng_a as u8);
        a = ((hi as u16) << 8) | (a & 0xFF);
        // add.w $786C,$786A
        a = a.wrapping_add(self.rng_c);
        // d0 = ($786D << 8) | $786C ; add.w d0,$786A
        let [c_hi, c_lo] = self.rng_c.to_be_bytes();
        a = a.wrapping_add(((c_lo as u16) << 8) | c_hi as u16);
        // add.b $786D,$786C ; carry -> $786A+1
        let (nc_hi, carry) = c_hi.overflowing_add(c_lo);
        let mut c = ((nc_hi as u16) << 8) | c_lo as u16;
        if carry { a = a.wrapping_add(1) }
        // addq.w #1,$786C ; carry -> $786A+1
        let (nc, carry2) = c.overflowing_add(1);
        c = nc;
        if carry2 { a = a.wrapping_add(1) }
        self.rng_a = a;
        self.rng_c = c;
        a
    }
    /// S:0x0FD8C: lo + (rand16 * n) >> 16
    pub fn random(&mut self, lo: u16, n: u16) -> u16 {
        let r = self.rand16() as u32;
        ((r * n as u32) >> 16) as u16 + lo
    }

    // ---------------------------------------------------------------- dispatch

    /// S:0x16974
    pub fn dispatch(&mut self, s: &[u8], a3: usize, host: &mut dyn Host) {
        self.depth_guard += 1;
        if self.depth_guard > 200_000 { return }
        let Some(&op) = s.get(a3) else { return };
        let Some(info) = op_info(op) else { return };
        let w = |o: usize| rd(s, a3 + o);
        match info.handler - 0x15400 {
            // DO
            0x1A08 => self.dispatch(s, a3 + 1, host),
            // SEQ / lists
            0x1A12 => {
                let n = self.count(s, a3);
                let mut i: u8 = 1;
                loop {
                    let a4 = self.nth(s, a3, i);
                    self.dispatch(s, a4, host);
                    if i == n { break }
                    i = i.wrapping_add(1);
                    if i == 0 { break }
                }
            }
            // WHEN
            0x1A52 => {
                let (v, a4) = self.eval(s, a3 + 1, info.t1, host);
                if v != 0 { self.dispatch(s, a4, host) }
            }
            // IFELSE
            0x1A6E => {
                let ptr = w(1) as usize;
                let (v, a4) = self.eval(s, a3 + 3, info.t1, host);
                self.dispatch(s, if v != 0 { a4 } else { ptr }, host);
            }
            // SWITCH
            0x1A96 => {
                let v = self.var(w(1)) as u8;
                let mut a4 = a3 + 3;
                for _ in 0..10_000 {
                    if !Self::is_list(s, a4) {
                        if rd(s, a4 + 1) as u8 == v { self.dispatch(s, a4 + 3, host) }
                        return;
                    }
                    let Some(k) = Self::car_case(s, a4) else { return };
                    a4 = Self::car_pos(s, a4);
                    if k as u8 == v { self.dispatch(s, a4 + 3, host); return }
                    a4 = self.skip(s, a4);
                }
            }
            // CYCLE
            0x1AF0 => {
                let desc = w(1);
                let n = self.count(s, a3 + 3);
                let v = self.var(desc) as u8;
                let a4 = self.nth(s, a3 + 3, v.wrapping_add(1));
                self.dispatch(s, a4, host);
                if self.menu.is_empty() {
                    let v1 = v.wrapping_add(1);
                    if v1 != n { self.set_var(desc, v1 as u16) }
                }
            }
            // RANDOM
            0x1B4C => {
                let desc = w(1);
                let n = self.count(s, a3 + 3);
                let k = self.random(1, n as u16) as u8;
                let a4 = self.nth(s, a3 + 3, k);
                self.dispatch(s, a4, host);
                self.set_var(desc, k as u16);
            }
            // CHOICE
            0x1B94 => {
                if self.pending != 0xFFFF { return }
                let text = w(1);
                if self.depth == self.path.len() {
                    self.menu.push(text);
                    return;
                }
                if self.path.get(self.depth) != Some(text) { return }
                self.depth = self.depth.wrapping_add(1);
                self.dispatch(s, a3 + 3, host);
                if self.depth == self.path.len() && self.menu.is_empty() { self.leaf = true }
                self.depth = self.depth.wrapping_sub(1);
            }
            // CASE
            0x1C18 => {
                if info.t2 != 7 { self.dispatch(s, a3 + 3, host) }
            }
            // NOP
            0x1C2C => {}
            // MARK
            0x1C2E => {
                if !self.guard() { return }
                let m = self.marks.top().unwrap_or(0) as u8;
                if m >= self.depth { return }
                self.marks.push(self.depth as u16);
                if info.t2 == 4 { self.dispatch(s, a3 + 1, host) }
            }
            // UNMARK
            0x1C78 => { if self.guard() { self.marks.drop_top() } }
            // SETALL / CLR / INC
            0x1C90 => { if self.guard() { self.set_var(w(1), 0xFFFF) } }
            0x1CAA => { if self.guard() { self.set_var(w(1), 0) } }
            0x1CC4 => {
                if !self.guard() { return }
                let d = w(1);
                let v = self.var(d);
                let max = 0xFFFFu16 >> (15 - (d & 15));
                if v != max { self.set_var(d, v.wrapping_add(1)) }
            }
            // GOTO / JUMP
            0x1CFA => {
                if !self.guard() { return }
                if info.t1 == 5 {
                    self.pending = w(1);
                } else {
                    let file = w(2);
                    if file != self.script_id {
                        self.queue.push(Action::load(SCRIPT_ADDR, file));
                        self.script_id = file;
                    }
                    self.pending = w(4);
                }
            }
            // CALL
            0x1D50 => {
                if !self.guard() { return }
                self.calls.push(self.script_id);
                let label_off = if info.t1 == 5 { 1 } else {
                    let file = w(2);
                    self.queue.push(Action::load(SCRIPT_ADDR, file));
                    self.script_id = file;
                    4
                };
                let ret = self.gosubs.get(0).unwrap_or(self.pc);
                self.calls.push(ret);
                self.pending = w(label_off);
            }
            // RETURN (not guarded)
            0x1DD2 => {
                if let Some(p) = self.calls.pop() { self.pending = p }
                if let Some(id) = self.calls.pop() {
                    if id != self.script_id {
                        self.queue.push(Action::load(SCRIPT_ADDR, id));
                        self.script_id = id;
                    }
                }
            }
            // GOSUB (inline)
            0x1E0C => {
                self.gosubs.push(self.pc);
                let label = w(1);
                self.pc = label;
                self.dispatch(s, label as usize, host);
                if let Some(p) = self.gosubs.pop() { self.pc = p }
            }
            // HISTORY
            0x1E48 => { self.d5 = self.history.peek_from_top(w(1) as u8).unwrap_or(0xFFFF) }
            // INGOSUB / GOSUBPC
            0x1E66 => {
                if info.t1 == 5 {
                    self.d5 = self.gosubs.contains(w(1)) as u16;
                } else {
                    let n = w(1) as u8;
                    self.d5 = if n == 0 { self.pc } else { self.gosubs.peek_from_top(n).unwrap_or(0xFFFF) };
                }
            }
            // SAY
            0x1EC0 => {
                if !self.guard() { return }
                if info.t1 == 4 { self.q(8, &[w(2), w(4)]) } else { self.q(8, &[0, w(1)]) }
            }
            // SCENE
            0x1F00 => {
                if !self.guard() { return }
                let v = w(1);
                if v == self.scene { return }
                self.scene_load(v);
                self.q(0x12, &[v & 0xFF]);
                self.scene = v;
            }
            // ANIM
            0x1F30 => { if self.guard() { self.q(9, &[w(4), w(2)]) } }
            // ANIMWAIT
            0x1F58 => { if self.guard() { self.q(9, &[3, w(1)]) } }
            // SOUND / CMD19 / CMD0D / CMD0C / CMD0A / MUSIC / CMD13 / CMD0E / CMD0B / WAIT
            0x1F78 => { if self.guard() { self.q(0x1C, &[w(1)]) } }
            0x1F94 => { if self.guard() { self.q(0x19, &[]) } }
            0x1FA4 => { if self.guard() { self.q(0x0D, &[w(1)]) } }
            0x1FC0 => { if self.guard() { self.q(0x0C, &[w(1)]) } }
            0x1FDC => { if self.guard() { self.q(0x0A, &[]) } }
            0x1FEC => { if self.guard() { self.q(0x07, &[w(1)]) } }
            0x2008 => { if self.guard() { self.q(0x13, &[]) } }
            0x2018 => { if self.guard() { self.q(0x0E, &[w(1)]) } }
            0x2034 => { if self.guard() { self.q(0x0B, &[]) } }
            0x2044 => { if self.guard() { self.q(0x10, &[w(1)]) } }
            // SYS
            0x2060 => {
                if !self.guard() { return }
                let f = if info.t1 == 4 { w(2) } else { w(1) };
                self.sys(f, s, a3, host);
            }
            // ADD AND OR NOT EQ LE LT GE GT
            0x224A | 0x226A | 0x228A | 0x22C2 | 0x22E6 | 0x230A | 0x232E | 0x2352 => {
                let (a, a4) = self.eval(s, a3 + 1, info.t1, host);
                let (b, _) = self.eval(s, a4, info.t2, host);
                let t = |c: bool| if c { 0xFFFF } else { 0 };
                self.d5 = match info.handler - 0x15400 {
                    0x224A => a.wrapping_add(b),
                    0x226A => a & b,
                    0x228A => a | b,
                    0x22C2 => t(a == b),
                    0x22E6 => t(a <= b),
                    0x230A => t(a < b),
                    0x232E => t(a >= b),
                    _ => t(a > b),
                };
            }
            0x22AA => {
                let (a, _) = self.eval(s, a3 + 1, info.t1, host);
                self.d5 = if a == 0 { 0xFFFF } else { 0 };
            }
            // LET
            0x2376 => {
                if !self.guard() { return }
                let d = w(1);
                let (v, _) = self.eval(s, a3 + 3, info.t2, host);
                self.set_var(d, v);
            }
            _ => {}
        }
    }

    /// S:0x1761C + caller: queue the DATA file load for a scene id if the file changes.
    fn scene_load(&mut self, v: u16) {
        let hi = v >> 8;
        if hi != (self.scene >> 8) & 0xFF {
            self.q(0x0F, &[]);
            self.queue.push(Action::load(DATA_ADDR, hi));
        } else {
            self.q(0x0F, &[]);
        }
    }

    /// SYS sub-functions (S:0x17492).
    fn sys(&mut self, f: u16, s: &[u8], a3: usize, host: &mut dyn Host) {
        let w = |o: usize| rd(s, a3 + o);
        match f {
            0 => self.q(0x04, &[]),
            1 => { self.q(0x1E, &[w(4)]); self.snapshot_full(host) }
            2 => self.q(0x21, &[]),
            3 => self.q(0x1F, &[]),
            4 => {
                host.poke(0x78E0, w(4));
                for a in [0x78E8, 0x78EA, 0x78E4, 0x78E6] { host.poke(a, 0) }
                host.poke_byte(0x78AF, 0xFF);
                self.q(0x18, &[]);
            }
            5 => { self.d5 = host.peek(w(4) as u32) }
            6 => {
                let (addr, val) = (w(5), w(7));
                if addr == 0xFFFF { host.poke_byte(0x1512A, val as u8) } else { host.poke(addr as u32, val) }
            }
            7 => self.q(0x1D, &[(w(4) != 0) as u16]),
            8 => if w(4) != 0 { self.q(0x15, &[w(8), w(10)]) } else { self.q(0x09, &[2, 1]) },
            9 => self.state = w(4).wrapping_add(2),
            10 => {
                host.poke(0x79FE, w(4));
                if w(4) == 1 { for a in [0x96D0, 0x96D2, 0x96D4, 0x96D6] { host.poke(a, 0) } }
            }
            11 => if w(4) == 0 { self.snapshot_scene(host) } else { self.restore_scene(host) },
            12 => {
                self.save[0x95] &= 0xF8;
                for b in &mut self.save[0x96..0x96 + 0x12A] { *b = 0 }
            }
            13 => self.q(0x1B, &[w(5), w(8), w(10)]),
            _ => {}
        }
    }

    fn put16(&mut self, o: usize, v: u16) { self.save[o..o + 2].copy_from_slice(&v.to_be_bytes()) }

    /// S:0x14F12: full snapshot ($9C02..$9C17) then the scene part.
    pub fn snapshot_full(&mut self, host: &mut dyn Host) {
        let (a, b, c, d) = (host.peek(0x79EA), host.peek(0x79EC), host.peek(0x79F0), host.peek(0x7988));
        self.put16(0x02, a);
        self.put16(0x04, b);
        self.put16(0x06, c);
        self.put16(0x08, d);
        let mut f = (host.peek(0x78BA) & 7) as u8;
        if host.peek(0x78B8) != 0 { f |= 0x40 }
        if host.peek(0x78A2) != 0 { f |= 0x80 }
        self.save[0x0A] = f;
        self.put16(0x0C, self.script_id);
        let ret = self.gosubs.get(0).unwrap_or(self.pc);
        self.put16(0x0E, ret);
        // history stack image: cap, count, 3 words
        self.save[0x10] = self.history.cap;
        self.save[0x11] = self.history.len();
        for i in 0..3 { let v = self.history.get(i).unwrap_or(0); self.put16(0x12 + 2 * i as usize, v) }
        self.snapshot_scene(host);
    }

    /// S:0x14F70: scene / music / object snapshot ($9C18..$9C5F).
    pub fn snapshot_scene(&mut self, host: &mut dyn Host) {
        self.put16(0x18, self.scene);
        let v = host.peek(0x15120);
        self.put16(0x1A, v);
        self.save[0x1C] = (host.peek(0x980E) >> 8) as u8; // `move.b cur_music` ($980E is the high byte of the word, S:0x14F82)
        let mut f = (host.peek(0x79EE) & 0xF) as u8 | (((host.peek(0x79FE) & 3) as u8) << 4);
        if host.peek(0x1512A) >> 8 != 0 { f |= 0x40 } // `tst.b` of the even byte
        if host.peek(0x7890) >> 8 != 0 { f |= 0x80 }
        self.save[0x1D] = f;
        let mut objs = [0u8; 64];
        host.snapshot_objects(&mut objs);
        self.save[0x20..0x60].copy_from_slice(&objs);
    }

    /// S:0x1500E: restore the scene from the snapshot (queues the needed actions).
    pub fn restore_scene(&mut self, host: &mut dyn Host) {
        let w = |o: usize, s: &[u8]| u16::from_be_bytes([s[o], s[o + 1]]);
        let d7 = w(0x18, &self.save);
        let d6 = w(0x1A, &self.save);
        self.scene_load(d7);
        if d6 != 0xFFFF { self.q(0x0D, &[d6]); self.q(0x0A, &[]); }
        self.q(0x20, &[d7 & 0xFF]);
        self.scene = d7;
        let music = self.save[0x1C];
        if music != 0 { self.q(0x07, &[music as u16]) }
        let f = self.save[0x1D];
        if f & 0x80 != 0 { host.poke_byte(0x7890, 1) }
        host.poke(0x79EE, (f & 0xF) as u16);
        host.poke(0x79FE, ((f >> 4) & 3) as u16);
        host.poke_byte(0x1512A, (f >> 6) & 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct H { m: HashMap<u32, u16> }
    impl Host for H {
        fn peek(&mut self, a: u32) -> u16 { *self.m.get(&a).unwrap_or(&0) }
        fn poke(&mut self, a: u32, v: u16) { self.m.insert(a, v); }
        fn poke_byte(&mut self, a: u32, v: u8) { self.m.insert(a, v as u16); }
    }

    /// Hand-assembled script: a menu with two CHOICEs, the second one has a SAY.
    #[test]
    fn menu_path_and_guard() {
        // 0000 DO SEQ[ CHOICE t=0x0010 { SAY 0x0020 } ; LIST. CHOICE t=0x0030 { SAY 0x0040 } ]
        let mut s = vec![
            0x30, // DO
            0x25, // cons
            0x38, 0x00, 0x10, 0x44, 0x00, 0x20, // CHOICE "a" { SAY }
            0x27, // last
            0x38, 0x00, 0x30, 0x44, 0x00, 0x40,
        ];
        s.resize(0x3800, 0xFF);
        let mut vm = Vm::blank();
        vm.state = 2;
        let mut h = H::default();
        let q = vm.run_pass(&s, &mut h);
        assert!(q.is_empty());
        assert_eq!(vm.finish_pass(), PassEnd::Menu(vec![0x10, 0x30]));
        vm.choose(Some(1));
        assert_eq!(vm.path.items, vec![0x30]);
        let q = vm.run_pass(&s, &mut h);
        assert_eq!(q, vec![Action { cmd: 8, args: vec![0, 0x40] }]);
        assert!(vm.leaf);
        assert_eq!(vm.finish_pass(), PassEnd::Continue);
        assert!(vm.path.is_empty());
    }

    #[test]
    fn cycle_and_vars() {
        // CYCLE v[000.0:4] LIST[ SAY 1 ; SAY 2 ]  (each pass says the next, stays on last)
        let mut s = vec![0x3F, 0x00, 0x03, 0x25, 0x44, 0x00, 0x01, 0x27, 0x44, 0x00, 0x02];
        s.resize(0x3800, 0xFF);
        let mut vm = Vm::blank();
        let mut h = H::default();
        let says: Vec<u16> = (0..3).map(|_| { let q = vm.run_pass(&s, &mut h); vm.finish_pass(); q[0].args[1] }).collect();
        assert_eq!(says, vec![1, 2, 2]);
    }

    #[test]
    fn rng_matches_68k_model() {
        // values derived by hand from S:0x0FD96 starting at a = c = 0
        let mut vm = Vm::blank();
        let v: Vec<u16> = (0..3).map(|_| vm.rand16()).collect();
        assert_eq!(v, vec![0x0000, 0x0101, 0x0504]);
        assert_eq!(vm.rng_c, 0x0303);
    }
}
