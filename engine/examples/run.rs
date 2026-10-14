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
    let aw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/audio/web");
    if let (Ok(p), Ok(f)) = (std::fs::read_to_string(aw.join("pcm.json")), std::fs::read_to_string(aw.join("fm.json"))) {
        e.load_audio_manifests(&p, &f);
        println!("audio manifests: {} voices, {} fm", e.voice_len.len(), e.fm_len.len());
    }
    for n in FILE_NAMES.iter() {
        if *n == "PCMLD_01.BIN" { continue; }
        if let Ok(d) = std::fs::read(raw.join(n)) { e.load_file(n, d); }
    }
    // main-CPU boot screens need the IP image (HEADER.BIN + IP.BIN of the disc, assets/raw/{header,ip}.bin)
    for (n, f) in [("HEADER.BIN", "header.bin"), ("IP.BIN", "ip.bin")] {
        if let Ok(d) = std::fs::read(raw.join("..").join(f)) { e.load_file(n, d); }
    }
    // SAVES=0:file.hex,2:file.hex → CONTINUE slots (hex text of the 0x1C0-byte save block, slot index 0..3)
    if let Ok(v) = std::env::var("SAVES") {
        for p in v.split(',').filter(|x| !x.is_empty()) {
            let (i, f) = p.split_once(':').unwrap();
            let hex = std::fs::read_to_string(f).unwrap();
            let bytes: Vec<u8> = (0..hex.trim().len() / 2).map(|k| u8::from_str_radix(&hex.trim()[2 * k..2 * k + 2], 16).unwrap()).collect();
            e.saves[i.parse::<usize>().unwrap()] = Some(bytes[..0x1C0.min(bytes.len())].to_vec());
        }
    }
    match a.get(5) {
        Some(j) if j == "boot" => e.boot(),
        Some(j) => {
            // SPnn:pc[:item,item,..] (hex text offsets of an already chosen menu path)
            let mut it = j.split(':');
            let (f, pc) = (it.next().unwrap(), it.next().unwrap());
            let path: Vec<u16> = it.next().map(|p| p.split(',').filter_map(|x| u16::from_str_radix(x, 16).ok()).collect()).unwrap_or_default();
            e.jump_path(57 + f[2..].parse::<u16>().unwrap(), u16::from_str_radix(pc, 16).unwrap(), &path);
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
    let shot_at: Vec<u64> = std::env::var("SHOTAT").map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
    let mut last = String::new();
    let mut last_fx = String::new();
    let mut sheet: Vec<Vec<u8>> = vec![];
    for f in 1..=frames {
        let pad = if inputs == "auto" { if f % 40 == 0 { 0x20 } else { 0 } } else { *presses.get(&f).unwrap_or(&0) };
        if let Ok(v) = std::env::var("POKE") { // "frame:hexaddr:value,..." word pokes of sub-CPU registers before the tick
            for it in v.split(',') { let p: Vec<&str> = it.split(':').collect(); if p.len() == 3 && p[0].parse::<u64>().ok() == Some(f) { e.regs.set(u32::from_str_radix(p[1], 16).unwrap(), p[2].parse().unwrap()); } }
        }
        if let Ok(v) = std::env::var("GUN") { // light gun: "x:y:from_frame" (screen px), buttons via GUNB="frame:bits,.." (0x20 trigger, 0x40 aux, held 1 frame)
            let p: Vec<u64> = v.split(':').filter_map(|x| x.parse().ok()).collect();
            if p.len() == 3 && f >= p[2] {
                let b = std::env::var("GUNB").ok().and_then(|g| g.split(',').filter_map(|it| it.split_once(':')).find(|(a, _)| a.parse::<u64>().ok() == Some(f)).and_then(|(_, b)| b.parse::<u8>().ok())).unwrap_or(0);
                e.ix.gun_present = true; e.ix.gun_x = p[0] as i32; e.ix.gun_y = p[1] as i32; e.ix.gun_buttons = b;
            }
        }
        e.tick(pad);
        if let Ok(v) = std::env::var("WATCH") { // "hexaddr,..." word registers printed every frame
            let mut l = format!("{f:6} watch");
            for w in v.split(',') { l += &format!(" {w}={:04x}", e.regs.get(u32::from_str_radix(w, 16).unwrap())); }
            println!("{l}");
        }
        for o in e.out.drain(..) { println!("{f:6} {o:?}"); }
        let m = &e.disp.main;
        let ms = format!("fx e060={:x} e061={:x} e06c={:x} e064={:x} e036={:x} e070={:x}", m.e060, m.e061, m.e06c, m.e064, m.e036, m.e070);
        if std::env::var("FXTRACE").is_ok() && ms != last_fx { println!("{f:6} {ms}"); last_fx = ms; }
        let d = e.debug_info();
        if d.split("\"seq\"").nth(1) != last.split("\"seq\"").nth(1) { println!("{f:6} {d}"); last = d; }
        if std::env::var("SCROLLDBG").is_ok() && f % every == 0 {
            let v = &e.disp.vdp; let sc = &e.disp.sub.scroll; let m = &e.disp.main;
            println!("{f:6} scroll: vdp vs {} {} hs {} {} | sub pos {:x?} delta {:x?} mode {} | main eb00 {:x?} e700 {:x?} e076 {} e078 {:x} | plane {:x} {:x} {}x{} display {}",
                v.vscroll_a, v.vscroll_b, v.hscroll_a, v.hscroll_b, sc.pos, sc.delta, sc.mode, m.eb00, m.e700, m.e076, m.e078, v.plane_a, v.plane_b, v.plane_w, v.plane_h, v.display);
        }
        if let Ok(v) = std::env::var("VRAMDUMP") { if f == frames { std::fs::write(v, &e.disp.vdp.vram).unwrap(); } }
        if std::env::var("OBJDUMP").is_ok() && f == frames {
            for (i, o) in e.disp.sub.objs.iter().enumerate() { if o.active == 1 { println!("obj {i} x {} y {} flags {:02x} idx {} script {:x} frame {:x}", o.x >> 16, o.y >> 16, o.flags, o.idx, o.script, o.frame); } }
            for (i, sp) in e.disp.vdp.sprites.iter().enumerate() { println!("spr {i} {:?}", sp); }
        }
        if std::env::var("CRAM").is_ok() && f % every == 0 { println!("cram A {:03x?}\ncram B {:03x?}", &e.disp.vdp.cram[..32], &e.disp.vdp.cram_b[..32]); }
        if (shot_at.is_empty() && f % every == 0) || shot_at.contains(&f) {
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
