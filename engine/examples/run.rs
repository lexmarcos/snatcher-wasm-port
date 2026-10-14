//! Native runner: run the engine headless and dump frames as PNG.
//! usage: cargo run --release -p snengine --example run -- <frames> <shot-every> <outdir> [inputs] [jump]
//!   inputs: comma list of "frame:bits" (bits held for 1 frame), e.g. "200:32,300:32"
//!           or "auto" = press C (confirm) every 40 frames
//!   jump:   e.g. SP01:0
use snengine::game::{Engine, FILE_NAMES, H, W};
use std::io::BufWriter;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let frames: u64 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(600);
    let every: u64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(60);
    let out = a.get(3).cloned().unwrap_or("/tmp/sn".into());
    let inputs = a.get(4).cloned().unwrap_or_default();
    std::fs::create_dir_all(&out).unwrap();
    let raw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw/files");
    let mut e = Engine::new();
    e.trace = true;
    for n in FILE_NAMES.iter() {
        if *n == "PCMLD_01.BIN" { continue; }
        if let Ok(d) = std::fs::read(raw.join(n)) { e.load_file(n, d); }
    }
    match a.get(5) {
        Some(j) => {
            let (f, pc) = j.split_once(':').unwrap();
            e.jump(57 + f[2..].parse::<u16>().unwrap(), u16::from_str_radix(pc, 16).unwrap());
        }
        None => e.new_game(),
    }
    let mut presses = std::collections::HashMap::new();
    if inputs != "auto" {
        for p in inputs.split(',').filter(|s| !s.is_empty()) {
            let (f, b) = p.split_once(':').unwrap();
            presses.insert(f.parse::<u64>().unwrap(), b.parse::<u8>().unwrap());
        }
    }
    let mut last = String::new();
    let mut last_fx = String::new();
    let mut sheet: Vec<Vec<u8>> = vec![];
    for f in 1..=frames {
        let pad = if inputs == "auto" { if f % 40 == 0 { 0x20 } else { 0 } } else { *presses.get(&f).unwrap_or(&0) };
        e.tick(pad);
        for o in e.out.drain(..) { println!("{f:6} {o:?}"); }
        let m = &e.disp.main;
        let ms = format!("fx e060={:x} e061={:x} e06c={:x} e064={:x} e036={:x} e070={:x}", m.e060, m.e061, m.e06c, m.e064, m.e036, m.e070);
        if std::env::var("FXTRACE").is_ok() && ms != last_fx { println!("{f:6} {ms}"); last_fx = ms; }
        let d = e.debug_info();
        if d.split("\"seq\"").nth(1) != last.split("\"seq\"").nth(1) { println!("{f:6} {d}"); last = d; }
        if std::env::var("CRAM").is_ok() && f % every == 0 { println!("cram A {:03x?}\ncram B {:03x?}", &e.disp.vdp.cram[..32], &e.disp.vdp.cram_b[..32]); }
        if f % every == 0 {
            sheet.push(e.fb.clone());
            let p = format!("{out}/f{f:06}.png");
            let w = BufWriter::new(std::fs::File::create(&p).unwrap());
            let mut enc = png::Encoder::new(w, W as u32, H as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&e.fb).unwrap();
        }
    }
    // contact sheet: 4 columns
    let cols = 4usize; let rows = (sheet.len() + cols - 1) / cols;
    if rows > 0 {
        let (sw, sh) = (W * cols, H * rows);
        let mut img = vec![0u8; sw * sh * 4];
        for (i, fb) in sheet.iter().enumerate() {
            let (cx, cy) = ((i % cols) * W, (i / cols) * H);
            for y in 0..H { let d = ((cy + y) * sw + cx) * 4; img[d..d + W * 4].copy_from_slice(&fb[y * W * 4..(y + 1) * W * 4]); }
        }
        let w = BufWriter::new(std::fs::File::create(format!("{out}/sheet.png")).unwrap());
        let mut enc = png::Encoder::new(w, sw as u32, sh as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&img).unwrap();
    }
}
