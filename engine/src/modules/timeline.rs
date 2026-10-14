//! CD-synchronised timeline modules: intro DATA_D1 (entry S:0x29266), intro part 2
//! DATA_D0 (S:0x2A828) and the late-game sequence DATA_Y16 (S:0x290D4).
//!
//! All three share one shape (docs/re/modules.md §3):
//! ```text
//! $7896 < 0 : first call → subfn_19(track) (CD play); then wait until subfn_18 == 0x0100,
//!             clr.l $787A, $7896 = 0
//! else      : entry = table[$7896] (6 bytes: handler offset word from the table base,
//!             CD time long MM SS FF 00); if entry.time > $787A: (D1/D0 only) Start → $7846 = -1;
//!             return.  Otherwise $7896 += 1 and run the handler.
//! ```
//! Entries with time 0 run one per frame; a handler can `subq.w #1,$7896` to repeat itself.
//! The tables are read from the user's DATA file at run time; the handler addresses are
//! translated below (each one is quoted from the disassembly, re/out/dis/modules_*.asm).
use super::SubServices;
use crate::game::{Engine, Module, Out};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind { D1, D0, Y16 }

impl Kind {
    /// (table base, entry count, CD track, Start skips)
    fn spec(self) -> (u32, usize, u16, bool) {
        match self {
            // S:0x29276 `move.l $2929A(pc,d0.w)`, table ends where code starts at S:0x29346; track: S:0x29352
            Kind::D1 => (0x29298, 29, 3, true),
            // S:0x2A838, last entry S:0x2A926 (done, 04:30:00), code at S:0x2A92C; track: S:0x2A938
            Kind::D0 => (0x2A85A, 35, 5, true),
            // S:0x290E4.., table ends at S:0x291A4 (code); track 0x13: S:0x291B0; no Start check
            Kind::Y16 => (0x290FC, 28, 0x13, false),
        }
    }
}

pub struct Timeline {
    kind: Kind,
    /// `$7896` (-1 = CD start phase)
    step: i32,
    /// `$7898` (sub-step of the CD start phase)
    sub: u8,
    /// `$784E` (frame timer used by Y16)
    timer: u16,
    entries: Vec<(u32, u32)>, // (handler address, time)
    /// intro driver `mode2_intro` S:0x14A52 sub-state (0/1 = file load and wait, both gated on
    /// `($7848 & 0x1F) == 0`; 2 = module running; 3 = skip fade)
    drv: u8,
    /// frame when the intro driver's file load completes
    ready: u64,
    /// last step written to the trace log (waiting handlers repeat)
    logged: i32,
}

impl Timeline {
    pub fn new(kind: Kind) -> Self { Timeline { kind, step: -1, sub: 0, timer: 0, entries: vec![], drv: if kind == Kind::Y16 { 2 } else { 0 }, ready: 0, logged: -2 } }

    fn load_table(&mut self, e: &Engine) {
        let (t, n, _, _) = self.kind.spec();
        self.entries = (0..n as u32)
            .map(|i| (t + e.mem.u16(t + 6 * i) as u32, e.mem.u32(t + 6 * i + 2)))
            .collect();
    }

    fn scene(e: &mut Engine, n: usize) {
        // S:0x29412 / S:0x2AA22 / S:0x292C8: subfn_17 then subfn_16(n)
        e.sub_scene_reset();
        e.sub_scene(n);
    }

    /// Run the handler at `h`. Returns Some(v) when the module sets `$7846`.
    fn run(&mut self, e: &mut Engine, h: u32) -> Option<i8> {
        let start = e.raw_pressed() & 0x80 != 0;
        match (self.kind, h) {
            // ---------------- DATA_D1
            (Kind::D1, 0x29376) => Self::scene(e, 0),
            (Kind::D1, 0x2937C) => Self::scene(e, 1),
            (Kind::D1, 0x29382) => {
                // scene 2, then $8F1C = 0x110 (plane A Y delta, 16.16 high word) + subfn_13(1)
                Self::scene(e, 2);
                e.disp.sub.scroll.delta[1] = 0x110 << 16;
                e.sub_scroll(1);
            }
            (Kind::D1, 0x29398) => Self::scene(e, 3),
            (Kind::D1, 0x2939E) => Self::scene(e, 4),
            (Kind::D1, 0x293A4) => Self::scene(e, 5),
            (Kind::D1, 0x293AA) => Self::scene(e, 6),
            (Kind::D1, 0x293B0) => Self::scene(e, 7),
            (Kind::D1, 0x293B6) => Self::scene(e, 8),
            (Kind::D1, 0x293BC) => Self::scene(e, 9),
            (Kind::D1, 0x293C2) => Self::scene(e, 10),
            (Kind::D1, 0x293C8) => Self::scene(e, 11),
            (Kind::D1, 0x293EA) => e.sub_scroll(8),
            (Kind::D1, 0x293F2) => e.sub_scroll(9),
            (Kind::D1, 0x293FA) => e.sub_scroll(10),
            (Kind::D1, 0x29402) => e.sub_scroll(11),
            (Kind::D1, 0x2940A) => e.sub_scroll(12),
            (Kind::D1, 0x29422) => e.sub_pal(0x2942C),
            (Kind::D1, 0x29430) => e.sub_pal(0x2943A),
            (Kind::D1, 0x293DA) => { e.sub_scene_reset(); self.step = 0; return Some(1); } // $7846 = 1
            // ---------------- DATA_D0
            (Kind::D0, 0x2A98E) => Self::scene(e, 0),
            (Kind::D0, 0x2A994) => Self::scene(e, 1),
            (Kind::D0, 0x2A99A) => Self::scene(e, 2),
            (Kind::D0, 0x2A9A0) => { e.disp.vdp.plane_b = 0xE000; Self::scene(e, 3); } // $B642D = FF → reg 4 = 7
            (Kind::D0, 0x2A9AE) => { e.disp.vdp.plane_b = 0xC000; Self::scene(e, 4); } // $B642D = 1 → reg 4 = 6
            (Kind::D0, a @ 0x2A9BC..=0x2A9F8) if (a - 0x2A9BC) % 6 == 0 => Self::scene(e, 5 + ((a - 0x2A9BC) / 6) as usize),
            (Kind::D0, 0x2A95C) => { /* bset #4,$FF800F: main stops its vblank sound tasks (M:0xFF079C) */ }
            (Kind::D0, 0x2A966) => e.cd_play(4),
            (Kind::D0, 0x2A970) => {
                // wait until track 4 plays, then clr.l $787A
                if e.cd_status() == super::CD_PLAYING { e.cd_time_reset(); } else { self.step -= 1; }
            }
            (Kind::D0, 0x2A9FE) => { e.disp.sub.objs[16].flags = 1; } // move.b #1,$7F25
            (Kind::D0, 0x2AA40) => e.sub_pal(0x2AA4A),
            (Kind::D0, 0x2AA4E) => e.sub_pal(0x2AA58),
            (Kind::D0, 0x2AA5C) => e.sub_pal(0x2AA66),
            (Kind::D0, 0x2AA12) => { e.sub_scene_reset(); self.step = 0; return Some(1); }
            // ---------------- DATA_Y16
            (Kind::Y16, a @ 0x291D4..=0x29204) if (a - 0x291D4) % 6 == 0 => Self::scene(e, ((a - 0x291D4) / 6) as usize),
            (Kind::Y16, 0x29310) => e.sub_pal(0x2931A),
            (Kind::Y16, 0x292E6) => e.sub_pal(0x292F0),
            (Kind::Y16, 0x292F4) => e.sub_pal(0x292FE),
            (Kind::Y16, 0x2920A) => {
                // track ended (status 0) or paused (0x500) → $784E = 0x12C; skip to entry 24 if saved before
                let s = e.cd_status() & 0xFF00;
                if s == 0 || s == 0x500 {
                    self.timer = 0x12C;
                    if e.vm.save.first().copied().unwrap_or(0) != 0 { self.step = 0x18; }
                } else { self.step -= 1; }
            }
            (Kind::Y16, 0x2923A) => {
                self.timer = self.timer.wrapping_sub(1);
                if self.timer != 0 { self.step -= 1; } else { self.timer = 0x40; e.sub_pal(0x292E2); }
            }
            (Kind::Y16, 0x29250) | (Kind::Y16, 0x29280) => {
                self.timer = self.timer.wrapping_sub(1);
                if self.timer != 0 { self.step -= 1; }
            }
            (Kind::Y16, 0x2925C) => e.cd_play(2),
            (Kind::Y16, 0x29266) => {
                if e.cd_status() != super::CD_PLAYING { self.step -= 1; } else { self.timer = 0x18; }
            }
            (Kind::Y16, 0x2928C) => { self.timer = 0x12C; Self::scene(e, 9); }
            (Kind::Y16, 0x29298) => {
                if !start { self.step -= 1; } else { self.timer = 0x40; e.sub_pal(0x292E2); }
            }
            (Kind::Y16, 0x292AE) => {
                self.timer = self.timer.wrapping_sub(1);
                if self.timer != 0 { self.step -= 1; }
            }
            (Kind::Y16, 0x292BC) => { self.step = 0; return Some(1); }
            (k, h) => e.out.push(Out::Log(format!("{k:?}: unknown timeline handler {h:#x}"))),
        }
        None
    }
}

impl Module for Timeline {
    fn name(&self) -> &'static str {
        match self.kind { Kind::D1 => "intro DATA_D1", Kind::D0 => "intro DATA_D0", Kind::Y16 => "late game DATA_Y16" }
    }

    fn tick(&mut self, e: &mut Engine) -> Option<i8> {
        // the sub frame skips the mode machine while a VRAM list / scene-reset handshake is pending
        if e.mode_blocked() { return None; }
        // Intro driver (S:0x14A52): the DATA file is loaded when `$7848 & 0x1F == 0` and its
        // arrival is polled with the same gating (the engine maps the file at once).
        match self.drv {
            0 | 1 => {
                // Intro driver S:0x14A52 states 0/1: both act only when `$7848 & 0x1F == 0` (`$7848` = sub vint
                // counter; the oracle's gate frames are ≡ 5 mod 32 on the title run). State 0: if the CD is playing
                // pause it and wait for the next gate, else `subfn_04` (load DATA file); state 1: poll the load
                // (the HLE disc needs ~35 frames for DATA_D1: ROMREADN at 1669, CD play at 1704).
                // Verified against the oracle on two START times (loads at 1669 and 1734/1733 model).
                let gate = e.frame % 32 == 5;
                if self.drv == 0 && gate {
                    if self.kind == Kind::D1 && e.cd_start.is_some() { e.cd_start = None; e.out.push(Out::MusicStop); }
                    else { self.drv = 1; self.ready = e.frame + 35; }
                } else if self.drv == 1 && e.frame >= self.ready { self.drv = 2; }
                return None;
            }
            3 => {
                // S:0x14B20: 10 frames of fade, then $7846 = -1 and subfn_17
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 { e.sub_scene_reset(); return Some(-1); }
                return None;
            }
            _ => {}
        }
        let r = self.timeline(e);
        match (r, self.kind) {
            (Some(-1), Kind::D1 | Kind::D0) => {
                // S:0x14B04: skipped → palette fade 83 00 00 3F for 10 frames
                self.drv = 3; self.timer = 10; e.sub_pal(0x14B1C);
                None
            }
            (Some(_), Kind::D1) => { e.disp.sub.objs.iter_mut().for_each(|o| *o = Default::default()); r } // subfn_03
            (Some(_), Kind::D0) => {
                // mode 2 → 7 (S:0xE1AA): scroll reset, plane B = 0xC000, objects and palette channels off
                e.sub_scroll(0xFF);
                e.disp.vdp.plane_b = 0xC000;
                e.disp.sub.objs.iter_mut().for_each(|o| *o = Default::default());
                e.disp.sub.palch = Default::default();
                r
            }
            _ => r,
        }
    }
}

impl Timeline {
    fn timeline(&mut self, e: &mut Engine) -> Option<i8> {
        let (_, _, track, skippable) = self.kind.spec();
        if self.entries.is_empty() { self.load_table(e); }
        if self.step < 0 {
            if self.sub == 0 { self.sub = 1; e.cd_play(track); return None; }
            if e.cd_status() == super::CD_PLAYING { e.cd_time_reset(); self.sub = 0; self.step = 0; }
            return None;
        }
        let Some(&(h, t)) = self.entries.get(self.step as usize) else { return Some(1) };
        // `cmp.l $787A,d1; bls run` → run when entry time <= CD time
        if t > e.cd_time() {
            if skippable && e.raw_pressed() & 0x80 != 0 { self.step = 0; return Some(-1); } // $7846 = $FFFF
            return None;
        }
        self.step += 1;
        if e.trace && self.logged != self.step - 1 {
            self.logged = self.step - 1;
            e.out.push(Out::Log(format!("{:?} step {} handler {h:#x} t={t:08x} cd={:08x}", self.kind, self.step - 1, e.cd_time())));
        }
        self.run(e, h)
    }
}
