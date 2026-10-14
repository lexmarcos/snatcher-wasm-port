//! Compact YM2612 (OPN2) model, written for the Snatcher port from public hardware
//! documentation (register map, envelope/phase behaviour, detune table from the YM2608/OPN
//! manuals). Integer arithmetic, runs at the chip rate (clock / 144 ≈ 53267 Hz on NTSC).
//! Not cycle-exact; no timers, no busy flag, no ladder effect. SSG-EG is ignored.

/// NTSC Mega Drive FM clock (53.693175 MHz / 7).
pub const CLOCK_NTSC: f64 = 53_693_175.0 / 7.0;
pub const CHIP_RATE: f64 = CLOCK_NTSC / 144.0;

#[derive(Clone, Copy, PartialEq)]
enum EgPhase { Attack, Decay, Sustain, Release }

#[derive(Clone)]
struct Op {
    dt: u8, mul: u8, tl: u8, ks: u8, ar: u8, am: bool, dr: u8, sr: u8, sl: u8, rr: u8,
    phase: u32,
    att: i32,          // 0 (loud) .. 1023 (silent)
    eg: EgPhase,
    key: bool,
}

impl Default for Op {
    fn default() -> Self {
        Op { dt: 0, mul: 0, tl: 0x7F, ks: 0, ar: 0, am: false, dr: 0, sr: 0, sl: 0, rr: 0,
             phase: 0, att: 1023, eg: EgPhase::Release, key: false }
    }
}

#[derive(Clone, Default)]
struct Ch {
    fnum: u16, block: u8, fnum_latch: u8,
    alg: u8, fb: u8, left: bool, right: bool, ams: u8, pms: u8,
    ops: [Op; 4],      // logical order op1..op4
    fb1: i32, fb2: i32,
    // ch3 special mode frequencies for op1..op3 (index by logical op)
    sp_fnum: [u16; 3], sp_block: [u8; 3], sp_latch: u8,
}

pub struct Ym2612 {
    ch: [Ch; 6],
    addr: [u8; 2],
    lfo_en: bool, lfo_freq: u8, lfo_cnt: u32, lfo_step: u32,
    ch3_mode: u8,
    dac_en: bool, dac: i32,
    eg_cnt: u32, eg_sub: u8,
    /// optional register write log (port, reg, value) for debugging
    pub log: Option<Vec<(u8, u8, u8)>>,
    /// debug: bit n set = channel n muted
    pub mute: u8,
    logsin: [u16; 256],
    exp: [u16; 256],
}

const DT_TAB: [[u8; 32]; 4] = [
    [0; 32],
    [0,0,0,0,1,1,1,1,1,1,1,1,2,2,2,2,2,3,3,3,4,4,4,5,5,6,6,7,8,8,8,8],
    [1,1,1,1,2,2,2,2,2,3,3,3,4,4,4,5,5,6,6,7,8,8,9,10,11,12,13,14,16,16,16,16],
    [2,2,2,2,2,3,3,3,4,4,4,5,5,6,6,7,8,8,9,10,11,12,13,14,16,17,19,20,22,22,22,22],
];

/// Envelope increments: rows by rate pattern, 8 steps each.
const EG_INC: [[u8; 8]; 17] = [
    [0,1,0,1,0,1,0,1], [0,1,0,1,1,1,0,1], [0,1,1,1,0,1,1,1], [0,1,1,1,1,1,1,1],
    [1,1,1,1,1,1,1,1], [1,1,1,2,1,1,1,2], [1,2,1,2,1,2,1,2], [1,2,2,2,1,2,2,2],
    [2,2,2,2,2,2,2,2], [2,2,2,4,2,2,2,4], [2,4,2,4,2,4,2,4], [2,4,4,4,2,4,4,4],
    [4,4,4,4,4,4,4,4], [4,4,4,8,4,4,4,8], [4,8,4,8,4,8,4,8], [4,8,8,8,4,8,8,8],
    [8,8,8,8,8,8,8,8],
];

/// LFO: chip samples per LFO step for the 8 frequencies (3.98..72.2 Hz, 128 steps/period).
const LFO_PERIOD: [u32; 8] = [108, 77, 71, 67, 62, 44, 8, 5];
/// PM depth in 1/1024 of fnum at full LFO deflection, per PMS (0, 3.4, 6.7, 10, 14, 20, 40, 80 cents).
const PM_DEPTH: [i32; 8] = [0, 2, 4, 6, 8, 12, 24, 48];
const AM_SHIFT: [u32; 4] = [8, 3, 1, 0];

fn keycode(fnum: u16, block: u8) -> u8 {
    let f11 = (fnum >> 10) & 1;
    let f10 = (fnum >> 9) & 1;
    let f9 = (fnum >> 8) & 1;
    let f8 = (fnum >> 7) & 1;
    let n4 = f11;
    let n3 = (f11 & (f10 | f9 | f8)) | ((f11 ^ 1) & f10 & f9 & f8);
    ((block << 2) | ((n4 as u8) << 1) | n3 as u8) & 31
}

impl Default for Ym2612 { fn default() -> Self { Self::new() } }

impl Ym2612 {
    pub fn new() -> Self {
        let mut logsin = [0u16; 256];
        let mut exp = [0u16; 256];
        for i in 0..256 {
            let s = ((i as f64 + 0.5) * std::f64::consts::PI / 2.0 / 256.0).sin();
            logsin[i] = (-s.log2() * 256.0).round().min(4095.0) as u16;
            // 2^(-i/256) scaled to 11 bits (+ implicit 1)
            exp[i] = ((2f64.powf(-(i as f64) / 256.0)) * 2048.0).round() as u16;
        }
        let mut y = Ym2612 {
            ch: Default::default(), addr: [0; 2], lfo_en: false, lfo_freq: 0, lfo_cnt: 0, lfo_step: 0,
            ch3_mode: 0, dac_en: false, dac: 0, eg_cnt: 0, eg_sub: 0, log: None, mute: 0, logsin, exp,
        };
        for c in y.ch.iter_mut() { c.left = true; c.right = true; }
        y
    }

    /// Write to port 0 (A1=0) or 1 (A1=1): `addr` then `data` like the real chip.
    pub fn write(&mut self, port: usize, reg: u8, data: u8) {
        if let Some(l) = self.log.as_mut() { l.push((port as u8, reg, data)); }
        self.addr[port] = reg;
        self.data(port, data);
    }

    fn data(&mut self, port: usize, v: u8) {
        let r = self.addr[port];
        if port == 0 && r < 0x30 {
            match r {
                0x22 => { self.lfo_en = v & 8 != 0; self.lfo_freq = v & 7; if !self.lfo_en { self.lfo_step = 0; self.lfo_cnt = 0; } }
                0x27 => { self.ch3_mode = v >> 6; }
                0x28 => {
                    let c = (v & 3) as usize;
                    if c == 3 { return }
                    let c = c + if v & 4 != 0 { 3 } else { 0 };
                    // key bits: 4 = op1(S1), 5 = op2(S2), 6 = op3(S3), 7 = op4(S4)
                    for (i, bit) in [4u8, 5, 6, 7].iter().enumerate() {
                        let on = v & (1 << bit) != 0;
                        self.key(c, i, on);
                    }
                }
                0x2A => { self.dac = (v as i32 - 128) << 6; }
                0x2B => { self.dac_en = v & 0x80 != 0; }
                _ => {}
            }
            return;
        }
        if r < 0x30 { return }
        let cn = (r & 3) as usize;
        if cn == 3 { return }
        let c = cn + port * 3;
        if r < 0xA0 {
            // operator register: slot offset 0=S1(op1),4=S3(op3),8=S2(op2),C=S4(op4)
            let slot = ((r >> 2) & 3) as usize;
            let op = [0usize, 2, 1, 3][slot];
            let o = &mut self.ch[c].ops[op];
            match r & 0xF0 {
                0x30 => { o.dt = (v >> 4) & 7; o.mul = v & 15; }
                0x40 => { o.tl = v & 0x7F; }
                0x50 => { o.ks = v >> 6; o.ar = v & 31; }
                0x60 => { o.am = v & 0x80 != 0; o.dr = v & 31; }
                0x70 => { o.sr = v & 31; }
                0x80 => { o.sl = v >> 4; o.rr = v & 15; }
                _ => {} // 0x90 SSG-EG ignored
            }
            return;
        }
        let ch = &mut self.ch[c];
        match r & 0xFC {
            0xA0 => { ch.fnum = ((ch.fnum_latch as u16 & 7) << 8) | v as u16; ch.block = (ch.fnum_latch >> 3) & 7; }
            0xA4 => { ch.fnum_latch = v & 0x3F; }
            0xA8 => {
                // ch3 special: A8=op3(S3), A9=op1(S1), AA=op2(S2) (port 0 only)
                if port == 0 {
                    let idx = [2usize, 0, 1][cn];
                    let ch3 = &mut self.ch[2];
                    ch3.sp_fnum[idx] = ((ch3.sp_latch as u16 & 7) << 8) | v as u16;
                    ch3.sp_block[idx] = (ch3.sp_latch >> 3) & 7;
                }
            }
            0xAC => { if port == 0 { self.ch[2].sp_latch = v & 0x3F; } }
            0xB0 => { ch.alg = v & 7; ch.fb = (v >> 3) & 7; }
            0xB4 => { ch.left = v & 0x80 != 0; ch.right = v & 0x40 != 0; ch.ams = (v >> 4) & 3; ch.pms = v & 7; }
            _ => {}
        }
    }

    fn key(&mut self, c: usize, op: usize, on: bool) {
        let o = &mut self.ch[c].ops[op];
        if on && !o.key {
            o.phase = 0;
            o.eg = EgPhase::Attack;
            if o.ar >= 31 { o.att = 0; o.eg = EgPhase::Decay; }
        } else if !on && o.key {
            o.eg = EgPhase::Release;
        }
        o.key = on;
    }

    fn op_freq(&self, c: usize, op: usize) -> (u16, u8) {
        let ch = &self.ch[c];
        if c == 2 && self.ch3_mode != 0 && op < 3 {
            (ch.sp_fnum[op], ch.sp_block[op])
        } else {
            (ch.fnum, ch.block)
        }
    }

    fn lfo_value(&self) -> (u32, i32) {
        // AM: triangle 0..126 ; PM: triangle -32..32 (approx)
        let s = self.lfo_step & 127;
        let am = if s < 64 { 126 - s * 2 } else { (s - 64) * 2 };
        let p = (self.lfo_step >> 2) & 31; // 0..31
        let pm = match p { 0..=7 => p as i32, 8..=15 => 16 - p as i32, 16..=23 => -(p as i32 - 16), _ => -(32 - p as i32) };
        (am, pm)
    }

    /// Advance one chip sample; returns (left, right) as i32 (each channel ±8191 summed).
    pub fn clock(&mut self) -> (i32, i32) {
        // LFO
        if self.lfo_en {
            self.lfo_cnt += 1;
            if self.lfo_cnt >= LFO_PERIOD[self.lfo_freq as usize] { self.lfo_cnt = 0; self.lfo_step = (self.lfo_step + 1) & 127; }
        }
        let (lfo_am, lfo_pm) = if self.lfo_en { self.lfo_value() } else { (0, 0) };
        // envelope clock every 3 samples
        self.eg_sub += 1;
        let eg_tick = if self.eg_sub == 3 { self.eg_sub = 0; self.eg_cnt = self.eg_cnt.wrapping_add(1); true } else { false };

        let (mut l, mut r) = (0i32, 0i32);
        for c in 0..6 {
            // phase + envelope update
            for op in 0..4 {
                let (fnum0, block) = self.op_freq(c, op);
                let pms = self.ch[c].pms;
                let fnum = if pms != 0 && lfo_pm != 0 {
                    let d = (fnum0 as i32 * PM_DEPTH[pms as usize] * lfo_pm) >> 13;
                    (fnum0 as i32 + d).clamp(0, 0x7FF) as u16
                } else { fnum0 };
                let kc = keycode(fnum0, block);
                let o = &mut self.ch[c].ops[op];
                let mut inc = ((fnum as u32) << block) >> 1;
                let dtv = DT_TAB[(o.dt & 3) as usize][kc as usize] as u32;
                if o.dt & 4 != 0 { inc = inc.wrapping_sub(dtv) & 0x1FFFF; } else { inc += dtv; }
                inc = if o.mul == 0 { inc >> 1 } else { inc * o.mul as u32 };
                o.phase = (o.phase + inc) & 0xFFFFF;
                if eg_tick {
                    let ksr = kc >> (3 - o.ks);
                    let (base, target) = match o.eg {
                        EgPhase::Attack => (o.ar, 0),
                        EgPhase::Decay => (o.dr, 0),
                        EgPhase::Sustain => (o.sr, 0),
                        EgPhase::Release => ((o.rr << 1) | 1, 0),
                    };
                    let _ = target;
                    let rate = if base == 0 { 0 } else { ((base as u32) * 2 + ksr as u32).min(63) };
                    if rate >= 2 {
                        let (row, shift) = if rate < 48 { ((rate & 3) as usize, 11 - (rate >> 2)) }
                                           else { ((4 + (rate - 48) as usize).min(16), 0) };
                        if self.eg_cnt & ((1 << shift) - 1) == 0 {
                            let inc = EG_INC[row][((self.eg_cnt >> shift) & 7) as usize] as i32;
                            match o.eg {
                                EgPhase::Attack => {
                                    if rate >= 62 { o.att = 0; } else { o.att += (!o.att * inc) >> 4; }
                                    if o.att <= 0 { o.att = 0; o.eg = EgPhase::Decay; }
                                }
                                EgPhase::Decay => {
                                    o.att += inc;
                                    let sl = if o.sl == 15 { 0x3E0 } else { (o.sl as i32) << 5 };
                                    if o.att >= sl { o.att = o.att.min(1023); o.eg = EgPhase::Sustain; }
                                }
                                EgPhase::Sustain | EgPhase::Release => { o.att = (o.att + inc).min(1023); }
                            }
                        }
                    }
                    if o.eg == EgPhase::Decay {
                        let sl = if o.sl == 15 { 0x3E0 } else { (o.sl as i32) << 5 };
                        if o.att >= sl { o.eg = EgPhase::Sustain; }
                    }
                }
            }
            // operator outputs
            let ams = self.ch[c].ams;
            let am_att = if ams == 0 { 0 } else { (lfo_am >> AM_SHIFT[ams as usize]) as i32 };
            let alg = self.ch[c].alg;
            let fb = self.ch[c].fb;
            let ph: [u32; 4] = [0, 1, 2, 3].map(|i| self.ch[c].ops[i].phase >> 10);
            let at: [i32; 4] = [0, 1, 2, 3].map(|i| {
                let o = &self.ch[c].ops[i];
                (o.att + ((o.tl as i32) << 3) + if o.am { am_att } else { 0 }).min(1023)
            });
            let fbm = if fb == 0 { 0 } else { (self.ch[c].fb1 + self.ch[c].fb2) >> (10 - fb) };
            let o1 = self.op_out(ph[0], fbm, at[0]);
            let (o2, o3, o4, out);
            match alg {
                0 => { o2 = self.op_out(ph[1], o1 >> 1, at[1]); o3 = self.op_out(ph[2], o2 >> 1, at[2]); o4 = self.op_out(ph[3], o3 >> 1, at[3]); out = o4; }
                1 => { o2 = self.op_out(ph[1], 0, at[1]); o3 = self.op_out(ph[2], (o1 + o2) >> 1, at[2]); o4 = self.op_out(ph[3], o3 >> 1, at[3]); out = o4; }
                2 => { o2 = self.op_out(ph[1], 0, at[1]); o3 = self.op_out(ph[2], o2 >> 1, at[2]); o4 = self.op_out(ph[3], (o1 + o3) >> 1, at[3]); out = o4; }
                3 => { o2 = self.op_out(ph[1], o1 >> 1, at[1]); o3 = self.op_out(ph[2], 0, at[2]); o4 = self.op_out(ph[3], (o2 + o3) >> 1, at[3]); out = o4; }
                4 => { o2 = self.op_out(ph[1], o1 >> 1, at[1]); o3 = self.op_out(ph[2], 0, at[2]); o4 = self.op_out(ph[3], o3 >> 1, at[3]); out = o2 + o4; }
                5 => { o2 = self.op_out(ph[1], o1 >> 1, at[1]); o3 = self.op_out(ph[2], o1 >> 1, at[2]); o4 = self.op_out(ph[3], o1 >> 1, at[3]); out = o2 + o3 + o4; }
                6 => { o2 = self.op_out(ph[1], o1 >> 1, at[1]); o3 = self.op_out(ph[2], 0, at[2]); o4 = self.op_out(ph[3], 0, at[3]); out = o2 + o3 + o4; }
                _ => { o2 = self.op_out(ph[1], 0, at[1]); o3 = self.op_out(ph[2], 0, at[2]); o4 = self.op_out(ph[3], 0, at[3]); out = o1 + o2 + o3 + o4; }
            }
            let _ = (o2, o3, o4);
            let chd = &mut self.ch[c];
            chd.fb2 = chd.fb1; chd.fb1 = o1;
            let mut s = out.clamp(-8192, 8191);
            if c == 5 && self.dac_en { s = self.dac; }
            if self.mute & (1 << c) != 0 { s = 0; }
            if chd.left { l += s; }
            if chd.right { r += s; }
        }
        (l, r)
    }

    #[inline]
    fn op_out(&self, phase10: u32, modu: i32, att: i32) -> i32 {
        if att >= 1023 { return 0 }
        let p = ((phase10 as i32 + modu) & 0x3FF) as usize;
        let q = p & 0xFF;
        let idx = if p & 0x100 != 0 { 255 - q } else { q };
        let total = self.logsin[idx] as i32 + (att << 2);
        if total >= 13 * 256 { return 0 }
        let m = ((self.exp[(total & 0xFF) as usize] as i32) << 2) >> (total >> 8);
        if p & 0x200 != 0 { -m } else { m }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_sine(y: &mut Ym2612, fnum: u16, block: u8) {
        // ch0, algorithm 7, only op4 audible (TL 0), others silent
        y.write(0, 0xB0, 0x07);
        y.write(0, 0xB4, 0xC0);
        for (slot, tl) in [(0u8, 0x7F), (4, 0x7F), (8, 0x7F), (0xC, 0x00)] {
            y.write(0, 0x30 + slot, 0x01); // MUL 1
            y.write(0, 0x40 + slot, tl);
            y.write(0, 0x50 + slot, 0x1F); // AR max
            y.write(0, 0x60 + slot, 0x00);
            y.write(0, 0x70 + slot, 0x00);
            y.write(0, 0x80 + slot, 0x0F);
        }
        y.write(0, 0xA4, (block << 3) | (fnum >> 8) as u8);
        y.write(0, 0xA0, fnum as u8);
        y.write(0, 0x28, 0xF0);
    }

    #[test]
    fn sine_frequency() {
        let mut y = Ym2612::new();
        setup_sine(&mut y, 644, 4); // ≈ 261.7 Hz
        let n = CHIP_RATE as usize; // one second
        let mut prev = 0; let mut crossings = 0; let mut peak = 0;
        for _ in 0..n {
            let (l, _) = y.clock();
            if prev < 0 && l >= 0 { crossings += 1; }
            prev = l; peak = peak.max(l.abs());
        }
        assert!((crossings as f64 - 261.7).abs() < 2.0, "crossings {crossings}");
        assert!(peak > 7000 && peak <= 8192, "peak {peak}");
    }

    #[test]
    fn key_off_releases() {
        let mut y = Ym2612::new();
        setup_sine(&mut y, 644, 4);
        for _ in 0..1000 { y.clock(); }
        y.write(0, 0x28, 0x00);
        let mut last = 1;
        for _ in 0..(CHIP_RATE as usize / 2) { last = y.clock().0; }
        assert_eq!(last, 0);
    }

    #[test]
    fn keycode_rule() {
        assert_eq!(keycode(0x284, 0), 0);
        assert_eq!(keycode(0x4C3, 4), 19);
    }
}
