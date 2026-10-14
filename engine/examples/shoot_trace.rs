//! SHOOTING fidelity: run the engine into a shooting stage through the SP01 debug menu (same key script as
//! re/oracle/shoot_common.py `start_stage`) and record the per-frame state in the format of re/oracle/shoot_rec.py.
//! usage: cargo run --release -p snengine --example shoot_trace -- <group> <item> '<script>' <frames> <out.jsonl> [shots]
//!   script = "frame:bits[:len],.."   (pad bits for len frames, frame 0 = first tick of the shooting sequencer, $78EA == 1)
//!   env GUNS="frame:x:y:trig:aux,.." light-gun script (screen pixels; frames as above; the gun stays at the last given position)
//! The sub RNG is pinned like the oracle harness (`$786A..D = 0` before every frame, then the frame handler's own step S:0xD644).
use snengine::game::{Engine, FILE_NAMES, H, W};
use std::io::Write;

const WORDS: &[u32] = &[
    0x78E0, 0x78E2, 0x78E4, 0x78E6, 0x78E8, 0x78EA, 0x78EC, 0x78EE, 0x78F0, 0x78F2, 0x78F4, 0x78F6, 0x78F8, 0x78FA, 0x78FC, 0x78FE,
    0x7900, 0x7902, 0x7904, 0x7906, 0x7908, 0x790A, 0x790C, 0x790E, 0x7910, 0x7912, 0x7914, 0x7916, 0x7918, 0x791A, 0x791C, 0x791E,
    0x7920, 0x7922, 0x7924, 0x7926, 0x7928, 0x792A, 0x792C, 0x792E, 0x7930, 0x7932, 0x7934, 0x7936, 0x7938, 0x793A, 0x793C, 0x793E,
    0x7940, 0x7942, 0x7944, 0x7946, 0x7948, 0x794A, 0x794C, 0x794E, 0x7970, 0x7972, 0x7984, 0x7986, 0x7988, 0x798A, 0x7990, 0x7992,
];
const SLOTS: &[usize] = &[21, 22, 23, 24, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 48, 56, 57, 58, 59];

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let group: usize = a[1].parse().unwrap();
    let item: usize = a[2].parse().unwrap();
    let mut pads = std::collections::HashMap::new();
    // '@file.json' = [[frame,bits],..] (one frame each, as written by shoot_rec.py --auto)
    let spec = if let Some(f) = a[3].strip_prefix('@') {
        let t = std::fs::read_to_string(f).unwrap();
        t.replace("[[", "").replace("]]", "").split("], [").map(|x| x.replace(", ", ":")).collect::<Vec<_>>().join(",")
    } else { a[3].clone() };
    for p in spec.split(',').filter(|s| !s.is_empty()) {
        let f: Vec<u64> = p.split(':').map(|x| x.parse().unwrap()).collect();
        let len = f.get(2).copied().unwrap_or(1);
        for k in 0..len { pads.insert(f[0] + k, f[1] as u8); }
    }
    let frames: u64 = a[4].parse().unwrap();
    let out = a[5].clone();
    let shots: Vec<u64> = a.get(6).map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
    let mut guns: Vec<(u64, i32, i32, u8)> = vec![];
    if let Ok(g) = std::env::var("GUNS") {
        for p in g.split(',').filter(|s| !s.is_empty()) {
            let f: Vec<i64> = p.split(':').map(|x| x.parse().unwrap()).collect();
            let b = (f.get(3).copied().unwrap_or(0) as u8) << 5 | (f.get(4).copied().unwrap_or(0) as u8) << 6;
            guns.push((f[0] as u64, f[1] as i32, f[2] as i32, b));
        }
    }

    // SCHED=oracle.jsonl: reproduce the oracle's sub-CPU frame schedule ("h": frame-handler entries per frame; PCM sample copies
    // S:0x15C32 overrun a frame so 0 or 2 entries happen) – only to compare logic frame-exact, not part of the engine
    let mut sched: Vec<u8> = vec![];
    if let Ok(sp) = std::env::var("SCHED") {
        for l in std::fs::read_to_string(sp).unwrap().lines() {
            let h = l.split("\"h\": ").nth(1).and_then(|r| r.split(',').next()).and_then(|x| x.trim().parse().ok()).unwrap_or(1);
            sched.push(h);
        }
    }
    let raw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw/files");
    let mut e = Engine::new();
    for n in FILE_NAMES.iter() {
        if *n == "PCMLD_01.BIN" { continue; }
        if let Ok(d) = std::fs::read(raw.join(n)) { e.load_file(n, d); }
    }
    e.fid_rng = true;
    e.jump(57 + 1, 0);

    let mut nav: Vec<(u8, u32)> = vec![(0, 100), (0x20, 3), (0, 60)];
    for _ in 0..5 { nav.push((2, 3)); nav.push((0, 25)); }
    nav.extend([(0x20, 3), (0, 80), (0x20, 3), (0, 80)]);
    for _ in 0..group { nav.push((2, 3)); nav.push((0, 25)); }
    nav.extend([(0x20, 3), (0, 100)]);
    for _ in 0..item { nav.push((2, 3)); nav.push((0, 25)); }
    nav.push((0x20, 3));
    let mut it = nav.into_iter().flat_map(|(b, n)| std::iter::repeat(b).take(n as usize));
    let mut k = 0u32;
    // navigation phase: until the shooting sequencer ran its first step ($78EA == 1)
    loop {
        let mut pad = it.next().unwrap_or(if k % 50 == 49 { 0x20 } else { 0 });
        if it.size_hint().0 == 0 { pad = if k % 50 == 49 { 0x20 } else { 0 }; }
        k += 1;
        e.tick(pad);
        e.out.clear();
        if e.regs.get(0x78EA) == 1 && e.regs.get(0x78E8) == 0 { break; }
        if k > 6000 { eprintln!("stage did not start"); std::process::exit(1); }
    }
    eprintln!("stage started after {k} frames: $78E0={:04x}", e.regs.get(0x78E0));
    let mut f = std::fs::File::create(&out).unwrap();
    let mut gpos = (0i32, 0i32, false);
    let mut snd: Vec<String> = vec![];
    for fr in 0..=frames {
        if fr > 0 {
            let pad = *pads.get(&fr).unwrap_or(&0);
            let mut gb = 0u8;
            for g in &guns { if g.0 <= fr { gpos = (g.1, g.2, true); } if g.0 == fr { gb = g.3; } }
            if gpos.2 { e.ix.gun_present = true; e.ix.gun_x = gpos.0; e.ix.gun_y = gpos.1; e.ix.gun_buttons = gb; }
            e.pin_sub_rng();
            let h = sched.get(fr as usize).copied().unwrap_or(1);
            if h == 0 { e.sub_stall = 1; }
            e.tick(pad);
            if h >= 2 { e.pin_sub_rng(); e.tick(pad); }
            snd.clear();
            for o in e.out.drain(..) {
                match o {
                    snengine::game::Out::Pcm { id, kind } => snd.push(format!("[\"{}\",{}]", if kind == 2 { "06" } else { "12" }, id)),
                    _ => {}
                }
            }
            if shots.contains(&fr) { write_png(&format!("{}.e{:05}.png", out.trim_end_matches(".jsonl"), fr), &e.fb); }
        }
        let mut w = String::new();
        for &a in WORDS { w += &format!("\"{:04X}\":{},", a, e.regs.get(a)); }
        w += &format!("\"78AF\":{}", e.regs.get_b(0x78AF));
        let mut o = String::new();
        let sh = e.ix.shoot.clone().unwrap_or_default();
        for &n in SLOTS {
            let ob = &e.disp.sub.objs[n];
            if !o.is_empty() { o += ","; }
            o += &format!("\"{}\":[{},{},{},{},{},{},{},{}]", n, ob.active, ob.flags, (ob.x >> 16) as u16, (ob.y >> 16) as u16, ob.idx, sh.f17[n], sh.f24[n], sh.f2c[n]);
        }
        writeln!(f, "{{\"f\":{},\"s\":[{}],\"w\":{{{}}},\"o\":{{{}}}}}", fr, snd.join(","), w, o).unwrap();
    }
    let _ = (W, H);
}

fn write_png(p: &str, fb: &[u8]) {
    let w = std::io::BufWriter::new(std::fs::File::create(p).unwrap());
    let mut enc = png::Encoder::new(w, W as u32, H as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(fb).unwrap();
}
