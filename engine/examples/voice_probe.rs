//! Voice probe (docs/voice-inventory.md): run the engine headless from script entry points and report, for every voice clip
//! that starts, what the screen shows while it plays (message box with text / empty box / scene art / black screen).
//! Dev tool only; the engine is not changed.
//!
//!   cargo run --release -p snengine --example voice_probe -- <outdir> <max-frames> [flags=zero|ones|alt|randN] SPnn:pc[:choice,choice[:scene]] [...]
//!
//! Output (stdout), one line per voice start:
//!   PROBE SPnn:pc id=<voice id> frame=<n> box=<empty|text|art|black> msg=<0|1> top=<non-black % of the upper picture> low=<non-black % of the box area> png=<file>
//! `flags=ones` starts with every story flag set (reaches the branches that need them). Screenshots go to <outdir> (gitignored work/).
use snengine::game::{Engine, Out, FILE_NAMES, H, W};
use std::io::BufWriter;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let out = a.get(1).cloned().expect("outdir");
    let max_frames: u64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(2500);
    let mut flags = "zero".to_string();
    let mut probes: Vec<String> = vec![];
    for x in &a[3..] {
        if let Some(f) = x.strip_prefix("flags=") { flags = f.to_string(); } else { probes.push(x.clone()); }
    }
    std::fs::create_dir_all(&out).unwrap();
    let raw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw/files");
    let aw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/audio/web");
    let pcm = std::fs::read_to_string(aw.join("pcm.json")).unwrap_or_default();
    let fm = std::fs::read_to_string(aw.join("fm.json")).unwrap_or_default();
    let mut files: Vec<(String, Vec<u8>)> = vec![];
    for n in FILE_NAMES.iter() {
        if *n == "PCMLD_01.BIN" { continue; }
        if let Ok(d) = std::fs::read(raw.join(n)) { files.push((n.to_string(), d)); }
    }
    for p in probes {
        let tag = { let mut h: u32 = 0x811c9dc5; for b in p.bytes() { h ^= b as u32; h = h.wrapping_mul(16777619); } format!("{h:08x}") };
        let mut it = p.split(':');
        let (f, pc) = (it.next().expect("SPnn"), it.next().expect("pc"));
        let path: Vec<u16> = it.next().map(|x| x.split(',').filter_map(|h| u16::from_str_radix(h, 16).ok()).collect()).unwrap_or_default();
        let mut e = Engine::new();
        if !pcm.is_empty() { e.load_audio_manifests(&pcm, &fm); }
        for (n, d) in &files { e.load_file(n, d.clone()); }
        let scene: Option<u16> = it.next().and_then(|x| u16::from_str_radix(x, 16).ok());
        e.jump_path(57 + f[2..].parse::<u16>().unwrap(), u16::from_str_radix(pc, 16).unwrap(), &path);
        if let Some(r) = scene {
            // show the scene the analysis found for this voice before the script runs (what an earlier SCENE node would have done)
            use sncore::script::vm::Action;
            e.queue.push(Action { cmd: 0x0F, args: vec![] });
            e.queue.push(Action { cmd: 0x11, args: vec![2, 0x8000, r >> 8] });
            e.queue.push(Action { cmd: 0x12, args: vec![r & 0xFF] });
        }
        match flags.as_str() {
            "ones" => for b in e.vm.save[sncore::script::vm::VARS_OFF..].iter_mut() { *b = 0xFF; },
            "alt" => for (i, b) in e.vm.save[sncore::script::vm::VARS_OFF..].iter_mut().enumerate() { *b = if i % 2 == 0 { 0xAA } else { 0x55 }; },
            x if x.starts_with("rand") => {
                // pseudo-random story flags (seed after "rand"), to reach branches that need particular variable values
                let mut st: u64 = x[4..].parse::<u64>().unwrap_or(1).wrapping_mul(0x9E3779B97F4A7C15) | 1;
                for b in e.vm.save[sncore::script::vm::VARS_OFF..].iter_mut() { st ^= st << 13; st ^= st >> 7; st ^= st << 17; *b = (st >> 24) as u8; }
            }
            _ => {}
        }
        let mut pending: Vec<(u64, u16)> = vec![]; // (capture frame, id)
        let mut last_event = 0u64;
        let mut seen = std::collections::HashSet::new();
        for fr in 1..=max_frames {
            let pad = if fr % 40 == 0 { 0x20 } else { 0 };
            e.tick(pad);
            for o in e.out.drain(..) {
                if let Out::Voice { id } = o {
                    last_event = fr;
                    let len = e.voice_len.get(&id).copied().unwrap_or(60) as u64;
                    pending.push((fr + ((len * 6 / 10).max(20)).min(len.saturating_sub(3)).max(4), id));
                }
            }
            let mut k = 0;
            while k < pending.len() {
                if pending[k].0 <= fr {
                    let (_, id) = pending.remove(k);
                    if !seen.insert(id) { continue; }
                    let fb = &e.fb;
                    let px = |x: usize, y: usize| -> bool { let i = (y * W + x) * 4; fb[i] != 0 || fb[i + 1] != 0 || fb[i + 2] != 0 };
                    let frac = |x0: usize, x1: usize, y0: usize, y1: usize| -> f64 {
                        let mut n = 0usize;
                        for y in y0..y1 { for x in x0..x1 { if px(x, y) { n += 1; } } }
                        100.0 * n as f64 / ((x1 - x0) * (y1 - y0)) as f64
                    };
                    let top = frac(8, W - 8, 4, 120.min(H));
                    // layout of the 256x224 screen: picture above y 136, black gap, dialogue strip (portraits / message box) y 150..214
                    let gap = frac(8, W - 8, 140, 148);
                    let bot = frac(8, W - 8, 216, 224);
                    let strip = frac(0, W, 150, 214);
                    let cls = if top < 1.0 && strip < 1.0 && gap < 1.0 { "black" } else if gap > 10.0 || bot > 10.0 { "fullscreen" } else if strip < 1.0 { "strip-empty" } else { "strip-portraits" };
                    let low = strip;
                    // text drawn in the picture (document / terminal screens): rows of the picture area with many bright / dark transitions
                    let bright = |x: usize, y: usize| -> bool { let i = (y * W + x) * 4; fb[i] >= 200 && fb[i + 1] >= 200 && fb[i + 2] >= 200 };
                    let mut txt = 0usize;
                    for y in 4..136.min(H) {
                        let (mut tr, mut nb, mut prev) = (0usize, 0usize, false);
                        for x in 0..W { let b = bright(x, y); if b != prev { tr += 1; } if b { nb += 1; } prev = b; }
                        if tr >= 12 && (8..=120).contains(&nb) { txt += 1; }
                    }
                    // dialogue strip icons: runs of non-black columns in y 150..214 separated by >= 4 black columns, hashed (FNV-1a) without position
                    let mut icons: Vec<String> = vec![];
                    let colnb = |x: usize| (150..214).any(|y| px(x, y));
                    let mut x = 0;
                    while x < W {
                        if !colnb(x) { x += 1; continue; }
                        let x0 = x;
                        let mut gap = 0;
                        while x < W && gap < 4 { if colnb(x) { gap = 0; } else { gap += 1; } x += 1; }
                        let x1 = x - gap;
                        if x1 - x0 < 20 { continue; }
                        let mut h: u64 = 0xcbf29ce484222325;
                        let mut rows = (214, 150);
                        for y in 150..214 { if (x0..x1).any(|xx| px(xx, y)) { rows.0 = rows.0.min(y); rows.1 = y; } }
                        for y in rows.0..(rows.0 + 20).min(rows.1 + 1) { for xx in x0..x1 { let i = (y * W + xx) * 4; for b in 0..3 { h ^= fb[i + b] as u64; h = h.wrapping_mul(0x100000001b3); } } }
                        icons.push(format!("{:08x}:{}x{}", h as u32, x1 - x0, rows.1 - rows.0 + 1));
                    }
                    let name = format!("{out}/{f}_{pc}_{tag}_{id}.png");
                    let w = BufWriter::new(std::fs::File::create(&name).unwrap());
                    let mut enc = png::Encoder::new(w, W as u32, H as u32);
                    enc.set_color(png::ColorType::Rgba);
                    enc.set_depth(png::BitDepth::Eight);
                    enc.write_header().unwrap().write_image_data(fb).unwrap();
                    println!("PROBE {f}:{pc} id={id} frame={fr} box={cls} msg={} top={top:.1} low={low:.1} gap={gap:.1} bot={bot:.1} txt={txt} icons={} png={name}", e.msg.is_some() as u8, icons.join(","));
                } else { k += 1; }
            }
            // give up when nothing happens for a long time (waiting on a menu that auto-press cannot pass)
            if fr > last_event + 1800 { break; }
        }
    }
}
