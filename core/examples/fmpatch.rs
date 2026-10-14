//! Debug: harmonicity of every FM instrument in FMWR_1 played as a sustained A4 on the YM model.
//! cargo run --release -p sncore --example fmpatch -- <FMWR_1.BIN>
use sncore::audio::ym2612::{Ym2612, CHIP_RATE};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let fm = std::fs::read(&a[1]).unwrap();
    let be32 = |o: usize| u32::from_be_bytes(fm[o..o + 4].try_into().unwrap()) as usize;
    let mut worst = Vec::new();
    for p in 0..256usize {
        let off = be32(p * 4);
        if off == 0 || off + 25 > 0x1E00 { continue }
        let d = &fm[off..off + 25];
        let mut y = Ym2612::new();
        y.write(0, 0xB0, d[0]);
        y.write(0, 0xB4, 0xC0);
        let carriers = [0x80u8, 0x80, 0x80, 0x80, 0xA0, 0xE0, 0xE0, 0xF0][(d[0] & 7) as usize] >> 4;
        for (k, base) in [0x30u8, 0x38, 0x34, 0x3C].iter().enumerate() {
            let o = &d[1 + k * 6..7 + k * 6];
            let mut r = *base;
            for (i, v) in o.iter().enumerate() {
                let mut v = *v;
                if i == 1 && carriers & (1 << k) != 0 { v = v.saturating_sub(0); }
                y.write(0, r, v); r += 0x10;
            }
        }
        // A4: fnum for 440 Hz at block 4: fnum = 440 * 2^20 / CHIP / 2^(4-1)
        let fnum = (440.0 * 1048576.0 / CHIP_RATE / 8.0).round() as u16;
        y.write(0, 0xA4, (4 << 3) | (fnum >> 8) as u8);
        y.write(0, 0xA0, fnum as u8);
        y.write(0, 0x28, 0xF0);
        let n = (CHIP_RATE * 0.5) as usize;
        let mut s = Vec::with_capacity(n);
        for i in 0..n * 2 { let (l, _) = y.clock(); if i >= n { s.push(l as f64); } }
        let total: f64 = s.iter().map(|x| x * x).sum::<f64>();
        if total < 1.0 { continue }
        let mut harm = 0.0;
        let f0 = fnum as f64 * 8.0 * CHIP_RATE / 1048576.0;
        for k in 1..60 {
            let f = f0 * k as f64;
            if f > CHIP_RATE / 2.0 { break }
            // Goertzel power at f
            let w = 2.0 * std::f64::consts::PI * f / CHIP_RATE;
            let c = 2.0 * w.cos();
            let (mut s1, mut s2) = (0.0, 0.0);
            for x in &s { let s0 = x + c * s1 - s2; s2 = s1; s1 = s0; }
            harm += (s1 * s1 + s2 * s2 - c * s1 * s2) * 2.0 / s.len() as f64;
        }
        let ratio = harm / total;
        // half-harmonics too (MUL=0 operators run at f0/2)
        let mut harm2 = 0.0;
        for k in 1..120 {
            let f = f0 * k as f64 / 2.0;
            if f > CHIP_RATE / 2.0 { break }
            let w = 2.0 * std::f64::consts::PI * f / CHIP_RATE;
            let c = 2.0 * w.cos();
            let (mut s1, mut s2) = (0.0, 0.0);
            for x in &s { let s0 = x + c * s1 - s2; s2 = s1; s1 = s0; }
            harm2 += (s1 * s1 + s2 * s2 - c * s1 * s2) * 2.0 / s.len() as f64;
        }
        let muls: Vec<u8> = (0..4).map(|k| d[1 + k * 6] & 15).collect();
        let dts: Vec<u8> = (0..4).map(|k| (d[1 + k * 6] >> 4) & 7).collect();
        let _ = (&muls, &dts);
        worst.push((harm2 / total, p, d[0]));
        if p < 12 { println!("patch {p} fbalg {:02x} mul {:?} dt {:?} ratio {:.3} half {:.3}", d[0], muls, dts, ratio, harm2 / total); }
    }
    worst.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let n = worst.len();
    println!("patches {}: median harmonic ratio {:.3}", n, worst[n / 2].0);
    for w in worst.iter().take(12) { println!("patch {:3} fbalg {:02x} ratio {:.3}", w.1, w.2, w.0); }
}
