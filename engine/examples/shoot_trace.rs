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
    0x7940, 0x7942, 0x7944, 0x7946, 0x7948, 0x794A, 0x794C, 0x794E, 0x1511E, 0x7970, 0x7972, 0x7984, 0x7986, 0x7988, 0x798A, 0x7990, 0x7992,
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
        // "frame:x:y:trig:aux,.." or "@file.json" = [[frame,x,y,trig,aux],..] (shoot_rec.py --gun)
        let g = if let Some(f) = g.strip_prefix('@') {
            let t = std::fs::read_to_string(f).unwrap();
            t.replace("[[", "").replace("]]", "").split("], [").map(|x| x.replace(", ", ":")).collect::<Vec<_>>().join(",")
        } else { g };
        for p in g.split(',').filter(|s| !s.is_empty()) {
            let f: Vec<i64> = p.split(':').map(|x| x.parse().unwrap()).collect();
            let b = (f.get(3).copied().unwrap_or(0) as u8) << 5 | (f.get(4).copied().unwrap_or(0) as u8) << 6;
            guns.push((f[0] as u64, f[1] as i32, f[2] as i32, b));
        }
    }
    // SCHED=oracle.jsonl.sched: per frame the logical pad bytes ($7990/$7991/$7992) of every sub handler run that frame (shoot_rec.py).
    // PCM sample copies (S:0x15C32) overrun a frame, so a frame has 0, 1 or 2 runs and a run can see a pad sampled in an earlier frame;
    // this reproduces that schedule so the logic can be compared frame-exact (it is not part of the engine).
    let mut sched: Vec<Vec<[u8; 6]>> = vec![];
    if let Ok(sp) = std::env::var("SCHED") {
        for l in std::fs::read_to_string(sp).unwrap().lines() {
            let v: Vec<u32> = l.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            let n = v[1] as usize;
            sched.push((0..n).map(|k| { let mut r = [0u8; 6]; for j in 0..6 { r[j] = v[2 + 6 * k + j] as u8; } r }).collect());
        }
    }
    let gun_force = std::env::var("GUNFORCE").is_ok();
    let raw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw/files");
    let mut e = Engine::new();
    for n in FILE_NAMES.iter() {
        if *n == "PCMLD_01.BIN" { continue; }
        if let Ok(d) = std::fs::read(raw.join(n)) { e.load_file(n, d); }
    }
    // SFXLEN=file: lines "bank id frames chan" (re/oracle/shoot_sfxlen.py writes it from assets/audio/web/pcm.json)
    if let Ok(f) = std::env::var("SFXLEN") {
        for l in std::fs::read_to_string(f).unwrap().lines() {
            let v: Vec<u32> = l.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            if v.len() == 4 { e.ix.sfx_len.insert((v[0] as u16, v[1] as u16), (v[2], v[3] as u8)); }
        }
    }
    e.jump(57 + 1, 0);

    // navigation with feedback: root menu -> item 5 (SHOOTING), act menu -> `group`, stage list -> `item`; messages are confirmed
    let targets = [5usize, group, item];
    let (mut menu_no, mut prev_ws, mut since, mut last_press, mut k) = (0usize, 0u8, 0u32, 0u32, 0u32);
    for _ in 0..100 { e.tick(0); e.out.clear(); k += 1; }
    loop {
        let ws = e.wait_state();
        if ws == 2 && prev_ws != 2 { menu_no += 1; since = 0; }
        prev_ws = ws;
        since += 1;
        let mut pad = 0u8;
        if k - last_press > 12 {
            if ws == 1 { pad = 0x20; last_press = k; }
            else if ws == 2 && since > 8 {
                let cur = e.menu.as_ref().map(|(m, _)| m.cursor as usize).unwrap_or(0);
                let tgt = targets.get(menu_no.wrapping_sub(1)).copied().unwrap_or(0);
                if menu_no == 0 { pad = 0; }
                else if cur != tgt { pad = 2; last_press = k; } else { pad = 0x20; last_press = k; }
            }
        }
        if menu_no == 0 && ws == 0 && k % 50 == 49 { pad = 0x20; }
        k += 1;
        e.tick(pad);
        e.out.clear();
        if e.regs.get(0x78E8) != 0 || e.regs.get(0x78EA) >= 2 { break; }
        if k > 8000 { eprintln!("stage did not start (menu {menu_no})"); std::process::exit(1); }
    }
    eprintln!("stage started after {k} frames: $78E0={:04x} pcm_bank={}", e.regs.get(0x78E0), e.pcm_bank);
    e.ix.rng_seq = Some(0); // counting starts with the first recorded handler run
    let mut f = std::fs::File::create(&out).unwrap();
    let mut gpos = (0i32, 0i32, false);
    let mut snd: Vec<String> = vec![];
    for fr in 0..=frames {
        if fr > 0 {
            let pad = *pads.get(&fr).unwrap_or(&0);
            let mut gb = 0u8;
            for g in &guns { if g.0 <= fr { gpos = (g.1, g.2, true); } if g.0 == fr { gb = g.3; } }
            if gpos.2 { e.ix.gun_present = true; e.ix.gun_x = gpos.0; e.ix.gun_y = gpos.1; e.ix.gun_buttons = gb; }
            match sched.get(fr as usize) {
                Some(runs) => {
                    if runs.is_empty() { e.sub_stall = 1; e.tick(0); }
                    for r in runs { e.ix.force_in = Some((r[0], r[1], r[2])); if gun_force { e.ix.force_gun = Some((r[3], r[4], r[5])); } e.tick(0); }
                }
                None => { e.tick(pad); }
            }
            snd.clear();
            for o in e.out.drain(..) {
                match o {
                    snengine::game::Out::PcmSfx { id, .. } => snd.push(format!("[\"06\",{}]", id)),
                    snengine::game::Out::Fm { id, music: false } => snd.push(format!("[\"12\",{}]", id)),
                    _ => {}
                }
            }
            if shots.contains(&fr) { write_png(&format!("{}.e{:05}.png", out.trim_end_matches(".jsonl"), fr), &e.fb); }
        }
        let mut w = String::new();
        for &a in WORDS { w += &format!("\"{:04X}\":{},", a, e.regs.get(a)); }
        w += &format!("\"78AF\":{}", e.regs.get_b(0x78AF));
        w += &format!(",\"q\":{}", e.queue.get(e.qi).map(|a| a.cmd as u32).unwrap_or(0));
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
