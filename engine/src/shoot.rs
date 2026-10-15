//! Shooting sequences – sequencer command 0x18 runs `shoot_tick` (S:0x1208E) every frame
//! until `$78AF == 0`. Set up by SYS MINIGAME (op 42 sub 4: `$78E0` = stage 0..0x0E).
//! Byte-for-byte port of S:0x1208E–0x1314E (docs/re/interact.md §2).
//!
//! Objects (slot = (addr − 0x7A00) / 0x50): 21 shot/flash ($8090), 22 pad crosshair ($80E0),
//! 23 crosshair jitter ($8130), 24 enemy attack ($8180), 26.. templates ($8220), 32–40 the
//! nine targets ($8400–$86D0), 48 life meter ($8900), 56/57 countdown ($8B80/$8BD0),
//! 58/59 counters ($8C20/$8C70). The extra per-object fields the shooting code uses
//! (+$17 cell, +$24 state, +$2C timer) are kept here (the animation runtime never reads them).
use crate::game::{Engine, Out};
use crate::menus::{set_x, set_y};

// stage tables (SUBCODE)
const T_HIT_SFX: u32 = 0x11F10;   // sound on enemy hit
const T_READY_SFX: u32 = 0x11F20; // sound when the blaster is drawn
const T_SHOT_SFX: u32 = 0x11F30;  // sound when firing
const T_ATTACK_SFX: u32 = 0x11F40;
const T_FIRST_PHASE: u32 = 0x11F50;
const T_SPAWN_DELAY: u32 = 0x11F60;
const T_SPEED: u32 = 0x11F70;     // initial difficulty `$78E2`
const T_KILLS_PAD: u32 = 0x11F80; // enemies to kill (pad) – 0 = no limit
const T_KILLS_GUN: u32 = 0x11F90; // enemies to kill (light gun)
const T_SPAWNS: u32 = 0x11FA0;    // number of spawns (0 = no limit)
const T_ACTIVE_PAD: u32 = 0x11FB0; // max simultaneous targets (pad)
const T_ACTIVE_GUN: u32 = 0x11FC0;
const T_FLAGS: u32 = 0x11FD0;     // ≠0: enemies attack (bit7: loop attacks)
const T_AIM_X: u32 = 0x11FE0;     // pad crosshair x by column
const T_AIM_Y: u32 = 0x11FE6;     // pad crosshair y by row
const T_CELL_XY: u32 = 0x11FEC;   // 9 x then 9 y positions of the cells (copied to $7920)
const T_RAND: u32 = 0x12010;      // 32-entry random table 0..8 (sub_012030)
const T_TYPE0: u32 = 0x12C9A;
const T_TYPE10: u32 = 0x12CE2;
const LIFE_CELLS: u32 = 0x12EBC;  // VRAM addresses of the life-meter cells

const O_SHOT: usize = 21; const O_AIM: usize = 22; const O_JIT: usize = 23; const O_ATK: usize = 24;
const O_TPL: usize = 26; const O_T0: usize = 32; const O_LIFE: usize = 48;
// slot = (addr − 0x7A00) / 0x50: countdown READY/GO! labels $8B80/$8BD0, counters $8C20/$8C70
const O_CD0: usize = 56; const O_CD1: usize = 57; const O_CNT0: usize = 58; const O_CNT1: usize = 59;

#[derive(Clone)]
pub struct Shoot {
    pub f17: [u8; 64],
    pub f24: [u8; 64],
    pub f2c: [u16; 64],
    /// one frame of latency for the main-CPU light-gun handshake (GA flag bit 5)
    handshake: u8,
}

impl Default for Shoot { fn default() -> Self { Shoot { f17: [0; 64], f24: [0; 64], f2c: [0; 64], handshake: 0 } } }

fn rw(e: &Engine, a: u32) -> u16 { e.regs.get(a) }
fn ww(e: &mut Engine, a: u32, v: u16) { e.regs.set(a, v) }
fn inc(e: &mut Engine, a: u32) { let v = rw(e, a).wrapping_add(1); ww(e, a, v) }
fn tb(e: &Engine, t: u32, i: u16) -> u8 { e.mem.u8(t + i as u32) }
fn stage(e: &Engine) -> u16 { rw(e, 0x78E0) }
fn gun(e: &Engine) -> bool { rw(e, 0x7972) != 0 }
fn kills_t(e: &Engine) -> u8 { tb(e, if gun(e) { T_KILLS_GUN } else { T_KILLS_PAD }, stage(e)) }
fn active_t(e: &Engine) -> u8 { tb(e, if gun(e) { T_ACTIVE_GUN } else { T_ACTIVE_PAD }, stage(e)) }
fn rand_tab(e: &mut Engine) -> u16 { let r = e.disp.sub.rand() & 0x1F; tb(e, T_RAND, r) as u16 }
/// `subfn_06(n, -1)` (S:0x1530E): PCM effect from the current bank (no mute test here, only cmd 0x0C tests `$7890`). It sets `$1511E`
/// bit 3, which the PCM driver clears (S:0x15998) when an effect channel has played to its end. Oracle (stage 4, bank 33): an effect on
/// channel 1 started at f37 (77 frames) cleared the bit at f118 although a later channel-0 effect (179 frames) was still playing, so the
/// bit is cleared by the FIRST channel that finishes. Lengths = sample length from the audio manifest (`Engine::ix.sfx_len`).
fn pcm(e: &mut Engine, n: u8) {
    if e.voice_left == 0 { e.out.push(Out::PcmSfx { id: n as u16, bank: e.pcm_bank }); }
    if let Some(&(len, ch)) = e.ix.sfx_len.get(&(e.pcm_bank, n as u16)) {
        e.ix.pcm_ch_until[(ch & 1) as usize] = e.frame + len as u64;
    }
    e.ix.pcm_busy = true;
}
/// Per frame: the PCM driver clears bit 3 when a channel's effect ended.
fn pcm_poll(e: &mut Engine) {
    for c in 0..2 {
        let u = e.ix.pcm_ch_until[c];
        if u != 0 && e.frame >= u { e.ix.pcm_ch_until[c] = 0; e.ix.pcm_busy = false; }
    }
}
/// `subfn_12(n, 0)`: FM effect with done flag `$980F` (0xFF / 0x3D = stop).
fn snd(e: &mut Engine, n: u8) { e.fm_cmd(n as u16, false); }
fn palch(e: &mut Engine, d: u32) { let m = std::mem::take(&mut e.mem); e.disp.sub.palch_start(&m, d); e.mem = m; }
fn tilech(e: &mut Engine, d: u32) { let m = std::mem::take(&mut e.mem); e.disp.sub.tilech_start(&m, d); e.mem = m; }
/// sub_012042: byte `d7` of the object's animation script (state → entry index header).
fn script_b(e: &Engine, slot: usize, d7: u16) -> u16 { e.mem.u8(e.disp.sub.objs[slot].script + d7 as u32) as u16 }

/// Sequencer command 0x18. Returns true when the shooting sequence is over.
pub fn cmd18(e: &mut Engine) -> bool {
    if e.regs.get_b(0x78AF) == 0 { e.ix.shoot = None; return true; }
    let mut s = e.ix.shoot.take().unwrap_or_default();
    tick(e, &mut s);
    let done = e.regs.get_b(0x78AF) == 0;
    if !done { e.ix.shoot = Some(s); }
    done
}

/// Sequencer command 0x1F (S:0x13150, `sub_013150`): the score-counter tally that follows the minigame. Runs every other sub vint
/// (`$7848` even) with `$78E8` as its step: 0 counts the idx of the two counter objects (slots 58/59, +0x28 = `$8C48`/`$8C98`) down,
/// the second counter only while it is above the first, until the first reached 0 (or < 0); 1 resets both counter objects (flags 3, idx -1)
/// and the READY/GO! label indices; 2 ends (returns true, `$78E8` = 0).
pub fn cmd1f(e: &mut Engine) -> bool {
    if e.frame & 1 != 0 { return false; }
    match rw(e, 0x78E8) {
        0 => {
            let v = e.disp.sub.objs[O_CNT0].idx as i16;
            if v <= 0 { inc(e, 0x78E8); return false; }
            let v = v - 1;
            e.disp.sub.objs[O_CNT0].idx = v as u16;
            let d1 = e.disp.sub.objs[O_CNT1].idx as i16;
            if v - d1 < 0 { e.disp.sub.objs[O_CNT1].idx = (d1 - 1) as u16; }
            false
        }
        1 => {
            for o in [O_CNT0, O_CNT1] { e.disp.sub.objs[o].flags = 3; e.disp.sub.objs[o].idx = 0xFFFF; }
            e.disp.sub.objs[O_CD0].idx = 0; e.disp.sub.objs[O_CD1].idx = 0;
            inc(e, 0x78E8);
            false
        }
        _ => { ww(e, 0x78E8, 0); true }
    }
}

/// `shoot_tick` S:0x1208E.
fn tick(e: &mut Engine, s: &mut Shoot) {
    // an object killed by its animation (op 0x00 -> `Obj::default()`) loses its extra fields too: the original clears the whole 0x50-byte slot
    // (oracle: target slot zero after the kill, `$78EC` stops counting it)
    for k in O_T0..O_T0 + 9 {
        let o = &e.disp.sub.objs[k];
        if o.script == 0 && o.anim_table == 0 && o.active == 0 && s.f24[k] != 0xFF { s.f17[k] = 0; s.f24[k] = 0; s.f2c[k] = 0; }
    }
    pcm_poll(e);
    ww(e, 0x1511E, if e.ix.pcm_busy { 0x0800 } else { 0 });
    match rw(e, 0x78E8) {
        0 => phase_setup(e, s),
        1 => {
            input(e, s);
            match rw(e, 0x78FA) {
                0 => {
                    for o in [O_CD0, O_CD1] { e.disp.sub.objs[o].flags = 1; e.disp.sub.objs[o].idx = 1; }
                    ww(e, 0x7946, 0x80); ww(e, 0x78FA, 1);
                }
                1 => {
                    let t = rw(e, 0x7946);
                    if t != 0 { if t & 0x3F == 0 { snd(e, 0x11); } ww(e, 0x7946, t - 1); } else { ww(e, 0x78FA, 2); }
                }
                _ => {
                    e.disp.sub.objs[O_CD0].active = 0; e.disp.sub.objs[O_CD1].active = 0;
                    snd(e, 0x12); ww(e, 0x78FA, 0); ww(e, 0x78E8, 2);
                    if e.trace { e.out.push(Out::Log("shoot: play phase".into())); }
                }
            }
        }
        2 => {
            input(e, s);
            shot(e, s);
            if stage(e) == 0x0E {
                if rw(e, 0x78EE) != 0 { set_x(e, O_AIM, 0x70); set_y(e, O_AIM, 0x38); }
                inc(e, 0x78E6);
                if rw(e, 0x78E6) >= 0x100 { ww(e, 0x78E8, 3); ww(e, 0x78EA, 0); }
            } else {
                targets(e, s);
                spawner(e, s);
                attack(e, s);
                end_check(e, s);
            }
        }
        3 => {
            let k = rw(e, 0x7914);
            if k != 0 && k == rw(e, 0x790C) { ww(e, 0x7916, 1); }
            let st = stage(e);
            if st != 0 && st != 0x0B { let d = if rw(e, 0x7910) >= 6 { 0x11E8C } else { 0x11E92 }; palch(e, d); }
            e.disp.sub.objs[O_JIT].active = 0; e.disp.sub.objs[O_AIM].active = 0;
            ww(e, 0x7970, 0); ww(e, 0x78E8, 4); ww(e, 0x78E4, 0);
            snd(e, 0xFF);
        }
        4 => match rw(e, 0x78EA) {
            0 => {
                // S:0x13072: wait while the PCM driver reports an effect playing ($1511E bit 3)
                if e.ix.pcm_busy { return; }
                if active_t(e) == 1 { inc(e, 0x78EA); return; }
                inc(e, 0x78E4);
                if rw(e, 0x78E4) >= 0x40 { inc(e, 0x78EA); return; }
                if rw(e, 0x7910) >= 6 { return; }
                input(e, s); shot(e, s);
            }
            1 => { inc(e, 0x78EA); e.disp.sub.objs[O_SHOT].active = 0; e.disp.sub.palch = Default::default(); }
            2 => { inc(e, 0x78EA); ww(e, 0x78E4, 0); palch(e, 0x11E7E); }
            3 => {
                let st = stage(e);
                if st == 0 || st == 0x0B { inc(e, 0x78EA); return; }
                inc(e, 0x78E4);
                if rw(e, 0x78E4) < 2 { return; }
                ww(e, 0x78E4, 0); tilech(e, 0x11F02); inc(e, 0x78EA);
            }
            _ => {
                inc(e, 0x78E4);
                if rw(e, 0x78E4) < 2 { return; }
                ww(e, 0x78E4, 0); ww(e, 0x78E8, 5); ww(e, 0x78EA, 0); palch(e, 0x11E74);
            }
        },
        _ => {
            inc(e, 0x78E4);
            if rw(e, 0x78E4) < 2 { return; }
            ww(e, 0x78F8, 0); ww(e, 0x78E8, 0); ww(e, 0x78E2, 0); ww(e, 0x7970, 0);
            e.regs.set_b(0x78AF, 0);
        }
    }
}

/// Phase 0 (S:0x120B2, sub-steps `$78EA`).
fn phase_setup(e: &mut Engine, s: &mut Shoot) {
    match rw(e, 0x78EA) {
        0 => { inc(e, 0x78EA); s.handshake = 0; ww(e, 0x78E4, 0); palch(e, 0x11E7E); }
        1 => {
            inc(e, 0x78E4);
            if rw(e, 0x78E4) < 2 { return; }
            // waits for the main CPU to acknowledge the gun-detect request (GA flag bit 5)
            s.handshake = s.handshake.saturating_add(1);
            ww(e, 0x78E4, 0);
            palch(e, 0x11E6A);
            let st = stage(e);
            if st == 0 || st == 0x0B { ww(e, 0x78EA, rw(e, 0x78EA) + 2); return; }
            tilech(e, 0x11EE6);
            if st != 5 { tilech(e, 0x11ED8); tilech(e, 0x11EF4); }
            inc(e, 0x78EA);
        }
        2 => {
            // $7902/$7904 = plane-B scroll (COMSTAT5/6)
            let p = e.disp.sub.scroll.pos;
            ww(e, 0x7902, (p[2] >> 16) as u16); ww(e, 0x7904, (p[3] >> 16) as u16);
            inc(e, 0x78EA);
        }
        _ => {
            let o = &mut e.disp.sub.objs[48]; o.active = 0; o.flags = 3;
            s.f2c[O_LIFE] = 6;
            for a in (0x78F8..0x791C).step_by(2) { ww(e, a, 0); }
            e.disp.sub.objs[O_SHOT].active = 0; e.disp.sub.objs[O_SHOT].flags = 0; s.f17[O_SHOT] = 0xFF;
            e.disp.sub.objs[O_ATK].active = 0; e.disp.sub.objs[O_ATK].flags = 0;
            e.disp.sub.objs[O_AIM].active = 0; e.disp.sub.objs[O_AIM].flags = 0;
            e.disp.sub.objs[O_JIT].active = 0; e.disp.sub.objs[O_JIT].flags = 0;
            ww(e, 0x78E6, 0); ww(e, 0x78F2, 0);
            init_targets(e, s);
            let st = stage(e);
            ww(e, 0x7944, tb(e, T_SPAWN_DELAY, st) as u16);
            ww(e, 0x78E8, tb(e, T_FIRST_PHASE, st) as u16);
            ww(e, 0x78EA, 0);
            if rw(e, 0x78E2) == 0 { ww(e, 0x78E2, tb(e, T_SPEED, st) as u16); }
        }
    }
}

/// sub_01220C.
fn init_targets(e: &mut Engine, s: &mut Shoot) {
    let hide = tb(e, T_FLAGS, stage(e)) != 0;
    for k in 0..9 {
        let o = &mut e.disp.sub.objs[O_T0 + k];
        o.idx = 2; o.delay = 0; s.f24[O_T0 + k] = 0;
    }
    // S:0x12248: `move.w d1,(a0)` ten times without advancing a0 (original quirk): only target 0's active word is cleared
    if hide { e.disp.sub.objs[O_T0].active = 0; }
    for k in 0..18u32 { let v = e.mem.u16(T_CELL_XY + 2 * k); ww(e, 0x7920 + 2 * k, v); }
    let set = |e: &mut Engine, a: u32, v: u16| ww(e, a, v);
    match stage(e) {
        2 => { set(e, 0x7922, 0x48); set(e, 0x7934, 0x08); set(e, 0x7928, 0x48); set(e, 0x793A, 0x23); }
        5 => { set(e, 0x7922, 0x5D); set(e, 0x7934, 0x36); }
        6 => { set(e, 0x792C, 0x26); set(e, 0x793E, 0x48); }
        7 => { set(e, 0x792A, 0x96); set(e, 0x793C, 0x30); }
        9 => { set(e, 0x7922, 0x60); set(e, 0x7934, 0x00); }
        0x0A => { set(e, 0x7932, 0); set(e, 0x7936, 0); set(e, 0x793E, 0x58); set(e, 0x7942, 0x58); }
        0x0D => { set(e, 0x7932, 0x18); set(e, 0x7936, 0x18); set(e, 0x7938, 0x28); set(e, 0x7928, 0x6E); set(e, 0x793A, 0x27); set(e, 0x793C, 0x28); }
        _ => {}
    }
}

/// `shoot_input` S:0x12400: draw/holster (A), pad aiming, recoil after being hit.
fn input(e: &mut Engine, _s: &mut Shoot) {
    if rw(e, 0x78F8) >= 2 {
        // loc_01254E: shaken after a hit
        if rw(e, 0x78F8) & 2 != 0 {
            let d = (e.disp.sub.rand() & 15) as i32;
            set_x(e, O_JIT, d - 8);
            let y = (e.disp.sub.objs[O_AIM].y >> 16) + d - 8; set_y(e, O_AIM, y);
            let d = (e.disp.sub.rand() & 15) as i32;
            set_y(e, O_JIT, d - 8);
            let x = (e.disp.sub.objs[O_AIM].x >> 16) + d - 8; set_x(e, O_AIM, x);
            ww(e, 0x7946, (rw(e, 0x78E2) >> 2) + 0x18);
        }
        ww(e, 0x78F8, rw(e, 0x78F8) - 2);
        return;
    }
    set_x(e, O_JIT, 0); set_y(e, O_JIT, 0);
    if rw(e, 0x78F2) != 0 || rw(e, 0x78E8) >= 4 { return; }
    let d0 = if gun(e) { e.regs.get_b(0x7985) } else { e.input.as_ref().map(|i| i.pressed).unwrap_or(0) };
    if d0 & 0x40 != 0 {
        ww(e, 0x7970, 0);
        let rdy = rw(e, 0x78F8) ^ 1;
        ww(e, 0x78F8, rdy);
        let mut d7 = 0u16;
        let mut gun_path = false;
        if rdy != 0 {
            let n = tb(e, T_READY_SFX, stage(e)); pcm(e, n);
            d7 = 1; ww(e, 0x7918, 1);
            if gun(e) { ww(e, 0x7970, 1); e.disp.sub.objs[O_AIM].active = 0; gun_path = true; }
        }
        if !gun_path {
            e.disp.sub.objs[O_AIM].active = d7;
            ww(e, 0x7900, 4);
            set_x(e, O_AIM, 0x70); set_y(e, O_AIM, 0x38);
        }
        e.disp.sub.objs[O_JIT].active = d7;
    }
    if rw(e, 0x78F8) == 0 || gun(e) || rw(e, 0x78EE) != 0 { return; }
    let h = e.input.as_ref().map(|i| i.held).unwrap_or(0);
    let (mut d5, mut d4) = (0u32, 4u16);
    if h & 8 != 0 { d5 += 2; d4 += 1; }
    if h & 4 != 0 { d4 = d4.wrapping_sub(1); } else { d5 += 2; }
    let x = e.mem.u16(T_AIM_X + d5) as i32; set_x(e, O_AIM, x);
    let mut d6 = 0u32;
    if h & 2 != 0 { d6 += 2; d4 += 3; }
    if h & 1 != 0 { d4 = d4.wrapping_sub(3); } else { d6 += 2; }
    let y = e.mem.u16(T_AIM_Y + d6) as i32; set_y(e, O_AIM, y);
    ww(e, 0x7900, d4);
}

/// shoot_gun_cell S:0x12516.
fn gun_cell(e: &mut Engine) {
    let x = e.regs.get_b(0x798A); let y = e.regs.get_b(0x798B);
    let c = if x < 0x60 { 0 } else if x < 0xA0 { 1 } else { 2 };
    let r = if y < 0x30 { 0 } else if y < 0x58 { 3 } else { 6 };
    ww(e, 0x7900, c + r);
}

/// sub_0125F2: the shot (object 21) state machine `$78FA`.
fn shot(e: &mut Engine, s: &mut Shoot) {
    if gun(e) && s.f2c[O_SHOT] != 0 {
        s.f2c[O_SHOT] += 1;
        if s.f2c[O_SHOT] >= 2 { s.f2c[O_SHOT] = 0; gun_cell(e); }
    }
    let st = stage(e);
    let mut state = rw(e, 0x78FA);
    loop {
        match state {
            0 => {
                if e.disp.sub.objs[O_SHOT].active != 0 || s.f17[O_SHOT] != 0xFF || rw(e, 0x78E8) == 4 || rw(e, 0x78F8) != 1 { return; }
                let d0 = if gun(e) { e.regs.get_b(0x7985) } else { e.input.as_ref().map(|i| i.pressed).unwrap_or(0) };
                if d0 & 0x40 != 0 || d0 & 0x20 == 0 { return; }
                if st == 0x0E { ww(e, 0x78EE, 1); }
                inc(e, 0x7908);
                ww(e, 0x78FA, 4);
                if gun(e) { s.f2c[O_SHOT] = 1; }
                return;
            }
            4 => { if rw(e, 0x78E2) >= 0x10 { ww(e, 0x78FA, 5); return; } state = 5; }
            5 => { ww(e, 0x78FA, 1); if rw(e, 0x78E2) >= 4 { return; } state = 1; }
            1 => {
                let o = &mut e.disp.sub.objs[O_SHOT]; o.active = 1; o.idx = 0;
                let d0 = if st == 0x0E { 4 } else { e.regs.get_b(0x7901) as u16 };
                s.f17[O_SHOT] = d0 as u8;
                let x = e.mem.u16(T_CELL_XY + 2 * d0 as u32) as i32;
                let y = e.mem.u16(T_CELL_XY + 18 + 2 * d0 as u32) as i32;
                set_x(e, O_SHOT, x); set_y(e, O_SHOT, y);
                ww(e, 0x78FA, 2);
                let n = tb(e, T_SHOT_SFX, st); pcm(e, n);
                return;
            }
            2 => {
                if e.disp.sub.objs[O_SHOT].idx < 4 { return; }
                if st == 0x0E {
                    ww(e, 0x790C, 1); ww(e, 0x78E8, 3); ww(e, 0x78EA, 0); pcm(e, 0x47);
                    return;
                }
                ww(e, 0x78FA, 3);
                return;
            }
            3 => { hit_test(e, s); return; }
            _ => return,
        }
    }
}

/// tbl012624_3 S:0x1274A: resolve the shot against the target in its cell.
fn hit_test(e: &mut Engine, s: &mut Shoot) {
    let d7 = s.f17[O_SHOT] as u16;
    let t = O_T0 + (d7 as usize % 9);
    let st = stage(e);
    let state = s.f24[t] & 0x0F;
    if (2..5).contains(&state) {
        let v = (s.f24[t] & 0xF0) | 6;
        s.f24[t] = v;
        if (v >> 4) & 1 == 0 {
            // innocent (type 0/2)
            inc(e, 0x790C);
            speed_down(e);
            if tb(e, T_FLAGS, st) == 0 { pcm(e, 0x0E); }
        } else {
            if rw(e, 0x790A) & 7 == 7 { speed_down(e); }
            inc(e, 0x790A);
            if st == 0 || st == 0x0B {
                let o = &mut e.disp.sub.objs[O_CNT1]; o.active = 1; o.flags = 1; o.idx = o.idx.wrapping_add(1);
            }
            let n = tb(e, T_HIT_SFX, st); pcm(e, n);
        }
        if st == 0x0A { stage10(e, s, d7); }
        else if st == 9 { ww(e, 0x7970, 0); ww(e, 0x78F2, 1); }
    } else if st == 0x0A {
        stage10(e, s, d7);
    } else {
        if gun(e) || tb(e, T_FLAGS, st) == 0 { speed_up(e); }
        miss(e);
    }
    s.f17[O_SHOT] = 0xFF;
    e.disp.sub.objs[O_SHOT].active = 0;
    ww(e, 0x78FA, 0);
}

/// loc_012862 (stage 0x0A): trap cells (table S:0x12CE2 bit1) hurt the player, others count
/// as a miss.
fn stage10(e: &mut Engine, s: &mut Shoot, d7: u16) {
    if tb(e, T_TYPE10, d7) & 2 == 0 { miss(e); return; }
    pcm(e, 0x32);
    player_hit(e, s);
}

fn miss(e: &mut Engine) {
    inc(e, 0x790E);
    if rw(e, 0x790E) == 0 { ww(e, 0x790E, 0xFF); }
}

/// sub_01204E
fn speed_down(e: &mut Engine) { let v = rw(e, 0x78E2); if v >= 2 { ww(e, 0x78E2, v - 2); } }
/// sub_01205C
fn speed_up(e: &mut Engine) {
    let st = stage(e);
    let cap = if st != 0 && st != 0x0B && gun(e) { 0x30 } else { 0x20 };
    let v = rw(e, 0x78E2);
    if v < cap { ww(e, 0x78E2, v + 1); }
}

/// sub_012E66 + loc_012E70: player hit – sound, one life-meter cell cleared, `$7910`++.
fn player_hit(e: &mut Engine, s: &mut Shoot) {
    snd(e, 0x13);
    life_down(e, s);
}

fn life_down(e: &mut Engine, s: &mut Shoot) {
    if rw(e, 0x7910) >= 6 { return; }
    // the meter index lives in object 48's +$2C (S:0x12E7A)
    s.f2c[O_LIFE] = s.f2c[O_LIFE].wrapping_sub(1);
    let idx = s.f2c[O_LIFE];
    let a = e.mem.u16(LIFE_CELLS + 2 * (idx as u32 & 7)) as usize;
    // tile-stream descriptor S:0x12EA8 copies 3 nametable words 0x86F0 (blank cell) there
    e.disp.vdp.write_vram(a as u16, &[0x86, 0xF0, 0x86, 0xF0, 0x86, 0xF0]);
    inc(e, 0x7910);
}

/// sub_012884: update the nine targets (state = +$24 low nibble).
fn targets(e: &mut Engine, s: &mut Shoot) {
    ww(e, 0x78EC, 0);
    for k in 0..9 { target(e, s, O_T0 + k); }
}

fn advance_if_done(e: &mut Engine, s: &mut Shoot, t: usize, d7: u16) {
    // sub_012920: when the animation reached the state's end index, start the next state
    let d5 = script_b(e, t, d7);
    if d5 != e.disp.sub.objs[t].idx { return; }
    e.disp.sub.objs[t].idx = d5 + 2;
    next_state(e, s, t);
}

/// loc_012934: state++, timer = 2*rand_tab + `$78E2`.
fn next_state(e: &mut Engine, s: &mut Shoot, t: usize) {
    s.f24[t] = s.f24[t].wrapping_add(1);
    let d2 = rand_tab(e);
    s.f2c[t] = (d2 << 1).wrapping_add(rw(e, 0x78E2));
}

/// loc_012AD2: jump to state d7 (animation from after entry `script[d7-1]`).
fn goto_state(e: &mut Engine, s: &mut Shoot, t: usize, d7: u16) {
    let d7 = d7 - 1;
    s.f24[t] = (s.f24[t] & 0xF0) + d7 as u8;
    let d5 = script_b(e, t, d7);
    e.disp.sub.objs[t].idx = d5 + 2;
    next_state(e, s, t);
}

fn timer_expired(s: &mut Shoot, t: usize) -> bool {
    s.f2c[t] = s.f2c[t].wrapping_sub(1);
    (s.f2c[t] as i16) < 0
}

fn target(e: &mut Engine, s: &mut Shoot, t: usize) {
    let st = stage(e);
    let mut d0 = s.f24[t];
    if d0 == 0xFF { d0 = 0; }
    let state = d0 & 0x0F;
    if state != 0 { inc(e, 0x78EC); }
    let flags = tb(e, T_FLAGS, st);
    match state {
        0 => {
            let d5 = script_b(e, t, 0);
            if d5 == 2 { e.disp.sub.objs[t].flags = 3; }
            else if d5 >= 0x80 || s.f24[t] == 0xFF { s.f24[t] = 0xFF; e.disp.sub.objs[t].flags = 3; e.disp.sub.objs[t].active = 0; }
            else { e.disp.sub.objs[t].active = 1; }
        }
        1 => { e.disp.sub.objs[t].flags = 0; advance_if_done(e, s, t, 1); }
        2 => {
            if !timer_expired(s, t) { return; }
            advance_if_done(e, s, t, 2);
            if s.f24[t] & 0x0F == 2 { s.f2c[t] = 0; return; }
            if flags == 0 { return; }
            if kills_t(e) as u16 >= rw(e, 0x790A) { return; }
            goto_state(e, s, t, 5);
        }
        3 => {
            if !timer_expired(s, t) { return; }
            advance_if_done(e, s, t, 3);
            if s.f24[t] & 0x0F == 3 { s.f2c[t] = 0; return; }
            if flags == 0 || s.f24[t] & 0xF0 == 0x20 { return; }
            if rw(e, 0x78FE) != 0 || rw(e, 0x7946) != 0 || rw(e, 0x78F8) >= 2 { return; }
            ww(e, 0x791A, (t - O_T0) as u16);
            ww(e, 0x78FE, 1);
        }
        4 => {
            if !timer_expired(s, t) { return; }
            advance_if_done(e, s, t, 4);
            if s.f24[t] & 0x0F == 4 { s.f2c[t] = 0; return; }
            if flags == 0 { return; }
            if flags & 0x80 == 0 {
                if kills_t(e) as u16 >= rw(e, 0x790A) { return; }
                if rand_tab(e) >= 6 { return; }
            }
            goto_state(e, s, t, 3);
        }
        5 => {
            advance_if_done(e, s, t, 5);
            if s.f24[t] & 0x0F == 5 { return; }
            let o = &mut e.disp.sub.objs[t]; o.idx = o.idx.wrapping_sub(2);
            if flags == 0 && s.f24[t] >= 0x30 { inc(e, 0x7910); }
            s.f24[t] = 0;
        }
        6 => {
            s.f24[t] = (s.f24[t] & 0xF0) + 6;
            let d5 = script_b(e, t, 6);
            e.disp.sub.objs[t].idx = d5;
            next_state(e, s, t);
        }
        7 => {
            advance_if_done(e, s, t, 7);
            if s.f24[t] & 0x0F != 8 { return; }
            let o = &mut e.disp.sub.objs[t]; o.idx = o.idx.wrapping_sub(2);
            s.f24[t] = 0;
            if st != 0 && st != 0x0B && st != 7 { e.disp.sub.objs[t].flags = 3; }
        }
        _ => {}
    }
}

/// sub_012B2E: target spawner (`$78FC`).
fn spawner(e: &mut Engine, s: &mut Shoot) {
    let st = stage(e);
    match rw(e, 0x78FC) {
        0 => {
            let v = rw(e, 0x7944).wrapping_sub(1); ww(e, 0x7944, v);
            if (v as i16) >= 0 { return; }
            let k = kills_t(e) as u16;
            if k != 0 && k <= rw(e, 0x790A) { return; }
            let n = tb(e, T_SPAWNS, st) as u16;
            if n != 0 && n == rw(e, 0x7906) { return; }
            if active_t(e) as u16 == rw(e, 0x78EC) { return; }
            let d2 = rand_tab(e);
            ww(e, 0x7944, (d2 << 1) + 0x10 + rw(e, 0x78E2));
            if rand_tab(e) >= 7 { ww(e, 0x7944, 5); }
            ww(e, 0x78FC, 1);
        }
        1 => {
            let d2 = rand_tab(e);
            let t = O_T0 + (d2 as usize % 9);
            // `move.l a0,$791C` (a0 = $8400 + 0x50 * cell)
            ww(e, 0x791C, 0); ww(e, 0x791E, (0x7A00 + 0x50 * t) as u16);
            if s.f24[t] == 0xFF || s.f24[t] & 0x0F != 0 { return; }
            ww(e, 0x78FC, 2);
            inc(e, 0x7906);
            let ty = target_type(e, s, t, d2);
            s.f24[t] = (ty << 4) as u8;
            if s.f24[t] == 0x20 { inc(e, 0x7914); ww(e, 0x7906, rw(e, 0x7906).wrapping_sub(1)); }
        }
        _ => {
            let t = ((rw(e, 0x791E) as usize).wrapping_sub(0x7A00) / 0x50) & 63;
            let mut d5 = script_b(e, t, 0);
            if d5 != 2 { d5 += 2; }
            e.disp.sub.objs[t].idx = d5;
            e.disp.sub.objs[t].active = 1;
            if s.f24[t] != 0x20 && (st == 0 || st == 0x0B) {
                let o = &mut e.disp.sub.objs[O_CNT0]; o.active = 1; o.flags = 1; o.idx = o.idx.wrapping_add(1);
            }
            s.f24[t] = (s.f24[t] & 0xF0) | 1;
            ww(e, 0x78FC, 0);
            if st == 0 || st == 0x0B { snd(e, 0x22); }
        }
    }
}

/// sub_012C1E: target type for a spawn (1 enemy, 2 special, 0 innocent; table S:0x12C2A).
fn target_type(e: &mut Engine, _s: &mut Shoot, t: usize, d7: u16) -> u16 {
    match stage(e) {
        0 => {
            let d2 = rand_tab(e) & 3;
            let d1 = tb(e, T_TYPE0, d2) as u16;
            copy_template(e, t, d1.wrapping_sub(1));
            e.disp.sub.objs[t].idx = 2;
            d1
        }
        0x0B => {
            let d2 = rand_tab(e);
            let d1 = tb(e, T_TYPE0, d2) as u16;
            copy_template(e, t, d1 + 1);
            e.disp.sub.objs[t].idx = 2;
            d1
        }
        2 => {
            let mut d0 = if d7 < 3 { 1 } else { 0 };
            d0 |= rw(e, 0x7912);
            let d1 = d0;
            ww(e, 0x7912, (d0 & 2) ^ 2);
            copy_template(e, t, d1);
            (d1 << 1) | 1
        }
        4 => { copy_template(e, t, 0); 1 }
        0x0A => tb(e, T_TYPE10, d7) as u16,
        0x0E => 2,
        _ => 1,
    }
}

/// sub_012CF0: copy palette/animation of template object 26+n to the target.
fn copy_template(e: &mut Engine, t: usize, n: u16) {
    let src = e.disp.sub.objs[(O_TPL + n as usize) & 63];
    let o = &mut e.disp.sub.objs[t];
    o.pal_override = src.pal_override; o.delay = src.delay;
    o.anim_table = src.anim_table; o.script = src.script; o.frame = src.frame;
}

/// sub_012D9A: enemy attacks (object 24, `$78FE`).
fn attack(e: &mut Engine, s: &mut Shoot) {
    let st = stage(e);
    if tb(e, T_FLAGS, st) == 0 { return; }
    match rw(e, 0x78FE) {
        0 => {
            let o = &mut e.disp.sub.objs[O_ATK]; o.active = 0; o.idx = 0;
            let t = rw(e, 0x7946); if t != 0 { ww(e, 0x7946, t - 1); }
        }
        1 => {
            let c = rw(e, 0x791A) as u32;
            let x = rw(e, 0x7920 + 2 * c) as i32; let y = rw(e, 0x7932 + 2 * c) as i32;
            set_x(e, O_ATK, x); set_y(e, O_ATK, y);
            let o = &mut e.disp.sub.objs[O_ATK]; o.active = 1; o.idx = 2;
            ww(e, 0x78FE, 2);
            let n = tb(e, T_ATTACK_SFX, st); if n != 0 { pcm(e, n); }
        }
        2 => { if script_b(e, O_ATK, 0) == e.disp.sub.objs[O_ATK].idx { ww(e, 0x78FE, 3); } }
        _ => {
            ww(e, 0x78F8, rw(e, 0x78F8) + 0x38);
            ww(e, 0x78FE, 0);
            e.disp.sub.objs[O_ATK].active = 0;
            if st == 4 { pcm(e, 0x34); life_down(e, s); } else { player_hit(e, s); }
        }
    }
}

/// loc_012EC8: end conditions.
fn end_check(e: &mut Engine, s: &mut Shoot) {
    let st = stage(e);
    let flags = tb(e, T_FLAGS, st);
    if flags != 0 && rw(e, 0x7910) >= 6 {
        for k in 0..9 { e.disp.sub.objs[O_T0 + k].flags |= 1; }
        e.disp.sub.palch = Default::default();
        ww(e, 0x78E8, 3); ww(e, 0x7910, 6);
        return;
    }
    let k = kills_t(e) as u16;
    if k != 0 && k > rw(e, 0x790A) { return; }
    let n = tb(e, T_SPAWNS, st) as u16;
    if n != 0 { if rw(e, 0x78FC) != 0 || n > rw(e, 0x7906) { return; } }
    if active_t(e) != 1 && rw(e, 0x78FE) != 0 { return; }
    if rw(e, 0x78FA) != 0 { return; }
    for k in 0..9 {
        let t = O_T0 + k;
        if s.f24[t] == 0xFF { continue; }
        if s.f24[t] & 0x0F != 0 { return; }
        if flags != 0 && st != 7 { e.disp.sub.objs[t].active = 0; }
    }
    if st == 0 || st == 0x0B {
        for o in [O_CD0, O_CD1] { e.disp.sub.objs[o].active = 1; e.disp.sub.objs[o].idx = 2; }
    }
    ww(e, 0x78E8, 3);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Engine with a synthetic copy of the stage tables (S:0x11F00..0x12200): every table row is the stage number times 0x10 + index so
    /// individual tables can be overridden per test; the 32-entry random table (S:0x12010) is the original's shape 0,0,0,1,1,1,1,2,..
    fn engine() -> Engine {
        let mut e = Engine::new();
        let mut m = vec![0u8; 0x300];
        let rand_tab = [0u8, 0, 0, 1, 1, 1, 1, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 7, 7, 7, 7, 8, 8, 8];
        m[0x110..0x130].copy_from_slice(&rand_tab);
        e.mem.map(0x11F00, m);
        e.regs.set_b(0x78AF, 0xFF);
        e
    }
    fn set_table(e: &mut Engine, table: u32, stage: usize, v: u8) {
        let mut d = e.mem.bytes(0x11F00, 0x300);
        d[(table - 0x11F00) as usize + stage] = v;
        e.mem.map(0x11F00, d);
    }

    #[test]
    fn gun_cell_thresholds() {
        // S:0x12516: x < 0x60 -> column 0, < 0xA0 -> 1, else 2; y < 0x30 -> row 0, < 0x58 -> 1, else 2
        let mut e = engine();
        for (x, y, cell) in [(0u8, 0u8, 0u16), (0x5F, 0x2F, 0), (0x60, 0x30, 4), (0x9F, 0x57, 4), (0xA0, 0x58, 8), (0xFF, 0xFF, 8), (0x60, 0x2F, 1), (0xA0, 0x30, 5), (0x5F, 0x58, 6)] {
            e.regs.set_b(0x798A, x); e.regs.set_b(0x798B, y);
            gun_cell(&mut e);
            assert_eq!(rw(&e, 0x7900), cell, "gun value ({x:#x},{y:#x})");
        }
    }

    #[test]
    fn miss_counter_never_wraps_to_zero() {
        // S:0x1288A: the miss counter skips 0 after 0xFFFF (a zero would read as "no miss" in the scenario script)
        let mut e = engine();
        ww(&mut e, 0x790E, 0xFFFE);
        miss(&mut e); assert_eq!(rw(&e, 0x790E), 0xFFFF);
        miss(&mut e); assert_eq!(rw(&e, 0x790E), 0xFF);
    }

    #[test]
    fn difficulty_steps_and_caps() {
        // sub_01204E: speed -2 (never below 0); sub_01205C: +1 up to 0x20, 0x30 with the light gun outside stages 0 / 0x0B
        let mut e = engine();
        ww(&mut e, 0x78E0, 1);
        ww(&mut e, 0x78E2, 1); speed_down(&mut e); assert_eq!(rw(&e, 0x78E2), 1);
        ww(&mut e, 0x78E2, 4); speed_down(&mut e); assert_eq!(rw(&e, 0x78E2), 2);
        ww(&mut e, 0x78E2, 0x20); speed_up(&mut e); assert_eq!(rw(&e, 0x78E2), 0x20);
        ww(&mut e, 0x7972, 1);
        speed_up(&mut e); assert_eq!(rw(&e, 0x78E2), 0x21);
        ww(&mut e, 0x78E0, 0);
        ww(&mut e, 0x78E2, 0x20); speed_up(&mut e); assert_eq!(rw(&e, 0x78E2), 0x20);
    }

    #[test]
    fn hit_test_resolves_enemy_innocent_and_empty_cells() {
        let mut e = engine();
        let mut s = Shoot::default();
        ww(&mut e, 0x78E0, 1);
        set_table(&mut e, T_FLAGS, 1, 6); // enemies attack in this stage
        // enemy (type 1, state 2) in cell 4 -> kill
        s.f17[O_SHOT] = 4; s.f24[O_T0 + 4] = 0x12; ww(&mut e, 0x78FA, 3);
        hit_test(&mut e, &mut s);
        assert_eq!((rw(&e, 0x790A), rw(&e, 0x790C), rw(&e, 0x790E)), (1, 0, 0));
        assert_eq!(s.f24[O_T0 + 4], 0x16, "target goes to state 6 (shot), type kept");
        assert_eq!((rw(&e, 0x78FA), s.f17[O_SHOT]), (0, 0xFF), "shot machine back to idle");
        // innocent (type 0 -> state nibble 2) in cell 2 -> civilian hit counter, speed -2, hit sound only when enemies do not attack
        s.f17[O_SHOT] = 2; s.f24[O_T0 + 2] = 0x02; ww(&mut e, 0x78E2, 8); ww(&mut e, 0x78FA, 3);
        hit_test(&mut e, &mut s);
        assert_eq!((rw(&e, 0x790A), rw(&e, 0x790C), rw(&e, 0x78E2)), (1, 1, 6));
        // empty cell -> miss; with attacking enemies and no gun the speed does not rise
        s.f17[O_SHOT] = 7; ww(&mut e, 0x78E2, 8); ww(&mut e, 0x78FA, 3);
        hit_test(&mut e, &mut s);
        assert_eq!((rw(&e, 0x790E), rw(&e, 0x78E2)), (1, 8));
        // ... the pad in a stage without attacks, or the gun, makes the game harder after a miss
        set_table(&mut e, T_FLAGS, 1, 0);
        s.f17[O_SHOT] = 7; ww(&mut e, 0x78FA, 3);
        hit_test(&mut e, &mut s);
        assert_eq!((rw(&e, 0x790E), rw(&e, 0x78E2)), (2, 9));
    }

    #[test]
    fn stage_ten_trap_cells_hurt() {
        // loc_012862: a shot at a trap cell (table S:0x12CE2 bit 1) costs one life cell, other empty cells are misses
        let mut e = engine();
        let mut s = Shoot::default();
        ww(&mut e, 0x78E0, 0x0A);
        // T_TYPE10 lives at S:0x12CE2 (outside the synthetic region): map it
        e.mem.map(0x12CE2, vec![1, 2, 1, 3, 2, 3, 1, 2, 1, 0]);
        s.f17[O_SHOT] = 1; ww(&mut e, 0x78FA, 3);
        hit_test(&mut e, &mut s);
        assert_eq!((rw(&e, 0x7910), rw(&e, 0x790E)), (1, 0), "cell 1 is a trap: life 1 lost");
        s.f17[O_SHOT] = 0; ww(&mut e, 0x78FA, 3);
        hit_test(&mut e, &mut s);
        assert_eq!((rw(&e, 0x7910), rw(&e, 0x790E)), (1, 1), "cell 0 is empty: a miss");
    }

    #[test]
    fn life_meter_stops_at_six_cells() {
        let mut e = engine();
        let mut s = Shoot::default();
        s.f2c[O_LIFE] = 6;
        for _ in 0..9 { life_down(&mut e, &mut s); }
        assert_eq!(rw(&e, 0x7910), 6, "$7910 counts the lost cells (game over at 6)");
        assert_eq!(s.f2c[O_LIFE], 0);
    }

    #[test]
    fn target_state_machine_advances_on_timers() {
        // sub_012884 / loc_012934: state 2 (target up) counts its timer down, then the script reaches the end marker and the state advances
        let mut e = engine();
        let mut s = Shoot::default();
        ww(&mut e, 0x78E0, 1);
        let t = O_T0;
        // animation script of the target: byte at script+0 = end index of state 0, +1.. = per state end index
        let mut script = vec![0u8; 16]; script[0] = 2; script[2] = 6; script[3] = 8; script[4] = 10;
        e.mem.map(0x20000, script);
        e.disp.sub.objs[t].script = 0x20000;
        s.f24[t] = 0x12; s.f2c[t] = 2; e.disp.sub.objs[t].idx = 6;
        target(&mut e, &mut s, t);                 // timer 2 -> 1
        assert_eq!((s.f24[t], s.f2c[t]), (0x12, 1));
        target(&mut e, &mut s, t);                 // timer 1 -> 0
        assert_eq!((s.f24[t], s.f2c[t]), (0x12, 0));
        target(&mut e, &mut s, t);                 // timer < 0 and the script reached the end index of state 2: next state (3)
        assert_eq!(s.f24[t], 0x13, "state 2 -> 3, type nibble kept");
        assert_eq!(e.disp.sub.objs[t].idx, 8, "animation index = end index of state 2 + 2");
    }

    #[test]
    fn results_phases_end_the_sequence() {
        // phase 3 -> 4 -> 5 -> done: `$78AF` (sequencer "shooting active" flag) drops to 0 and the sequence reports completion
        let mut e = engine();
        ww(&mut e, 0x78E0, 1);
        set_table(&mut e, T_ACTIVE_PAD, 1, 1);  // phase 4 sub 0 is skipped at once for single-target stages
        let mut s = Shoot::default();
        ww(&mut e, 0x78E8, 3);
        let mut frames = 0;
        while e.regs.get_b(0x78AF) != 0 && frames < 200 { tick(&mut e, &mut s); frames += 1; }
        assert_eq!(e.regs.get_b(0x78AF), 0);
        assert_eq!(rw(&e, 0x78E8), 0, "state reset for the next stage");
        assert!(frames < 40, "results take {frames} frames");
    }

    #[test]
    fn pcm_busy_clears_when_the_first_channel_ends() {
        // oracle (stage 4): $1511E bit 3 cleared when the channel-1 effect ended although a longer channel-0 effect still played
        let mut e = engine();
        e.ix.sfx_len.insert((33, 67), (77, 1));
        e.ix.sfx_len.insert((33, 64), (179, 0));
        e.pcm_bank = 33;
        e.frame = 37; pcm(&mut e, 67);
        e.frame = 44; pcm(&mut e, 64);
        assert!(e.ix.pcm_busy);
        e.frame = 113; pcm_poll(&mut e); assert!(e.ix.pcm_busy);
        e.frame = 114; pcm_poll(&mut e); assert!(!e.ix.pcm_busy);
    }
}
