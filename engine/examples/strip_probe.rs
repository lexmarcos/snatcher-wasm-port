//! Strip probe (docs/voice-subtitles.md, layout): run from a script entry point and, while a voice clip plays, log the portrait objects
//! (slots 5..14 of the display object table) and the non-black column runs of the dialogue strip.
//!   strip_probe <SPnn:pc[:scene]> <frames> [png-dir]       frame-by-frame log of the changes
//!   strip_probe survey <frames> <roots...>                  one line per clip: the portraits (slot x, talking) at the start, 25/50/75 % and the end
use snengine::game::{Engine, Out, FILE_NAMES, H, W};

fn make(p: &str) -> Engine {
    let raw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw/files");
    let aw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/audio/web");
    let pcm = std::fs::read_to_string(aw.join("pcm.json")).unwrap_or_default();
    let fm = std::fs::read_to_string(aw.join("fm.json")).unwrap_or_default();
    let mut e = Engine::new();
    if !pcm.is_empty() { e.load_audio_manifests(&pcm, &fm); }
    for n in FILE_NAMES.iter() { if *n == "PCMLD_01.BIN" { continue; } if let Ok(d) = std::fs::read(raw.join(n)) { e.load_file(n, d); } }
    let mut it = p.split(':');
    let (f, pc) = (it.next().unwrap(), it.next().unwrap());
    // `SPnn:pc[:scene]` (old) or `SPnn:pc:menu path:scene` as in vsub_sweep (path = comma-separated hex)
    let (third, fourth) = (it.next(), it.next());
    let (path, scene_s): (Vec<u16>, Option<&str>) = match fourth {
        Some(sc) => (third.unwrap_or("").split(',').filter_map(|h| u16::from_str_radix(h, 16).ok()).collect(), Some(sc)),
        None => (vec![], third),
    };
    let scene: Option<u16> = scene_s.and_then(|x| u16::from_str_radix(x, 16).ok());
    e.jump_path(57 + f[2..].parse::<u16>().unwrap(), u16::from_str_radix(pc, 16).unwrap(), &path);
    if let Some(r) = scene {
        use sncore::script::vm::Action;
        e.queue.push(Action { cmd: 0x0F, args: vec![] });
        e.queue.push(Action { cmd: 0x11, args: vec![2, 0x8000, r >> 8] });
        e.queue.push(Action { cmd: 0x12, args: vec![r & 0xFF] });
    }
    e
}

/// Portrait frames by the object table: (slot of the frame object, x, talking) for every active object at y = 146 (the frame).
fn portraits(e: &Engine) -> Vec<(usize, i32, bool)> {
    let mut v = vec![];
    let _ = &e.fb;
    for i in 5..15 {
        let o = &e.disp.sub.objs[i];
        if o.active != 0 && (o.y >> 16) == 146 {
            let m = &e.disp.sub.objs[i - 1];
            v.push((i, o.x >> 16, m.active != 0 && m.flags & 1 == 0));
        }
    }
    v.sort_by_key(|p| p.1);
    v
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.get(1).map(|s| s.as_str()) == Some("survey") {
        let max: u64 = a[2].parse().unwrap();
        for p in &a[3..] {
            let mut e = make(p);
            let mut starts: Vec<(u64, u16, u64)> = vec![];
            let mut cur: Vec<(u64, String)> = vec![];
            let mut ends: Vec<(u64, u16, u64, String)> = vec![];
            let _ = &mut cur;
            let mut hist: std::collections::VecDeque<Vec<u16>> = Default::default();
            for fr in 1..=max {
                e.tick(if fr % 40 == 0 { 0x20 } else { 0 });
                hist.push_back((0..64).map(|k| e.disp.sub.objs[k].idx).collect());
                if hist.len() > 14 { hist.pop_front(); }
                for o in e.out.drain(..) { if let Out::Voice { id } = o { starts.push((fr, id, e.voice_left as u64)); } }
                let probes: Vec<(u64, u16, u64)> = starts.iter().filter(|s| { let d = fr - s.0; [30, s.2 / 4, s.2 / 2, 3 * s.2 / 4, s.2.saturating_sub(4)].iter().any(|&k| k == d) }).cloned().collect();
                for (f0, id, len) in probes {
                    let ps = portraits(&e).iter().map(|p| {
                        let moved = hist.len() >= 14 && hist.front().unwrap()[p.0 - 1] != hist.back().unwrap()[p.0 - 1];
                        let mut h: u32 = 0x811c9dc5;
                        for y in 150..166 { for x in p.1 as usize..p.1 as usize + 64 { let k = (y * W + x) * 4; for b in 0..3 { h ^= e.fb[k + b] as u32; h = h.wrapping_mul(16777619); } } }
                        format!("{}{}{}:{:08x}", p.1, if p.2 { "*" } else { "" }, if moved { "m" } else { "" }, h) }).collect::<Vec<_>>().join(",");
                    ends.push((f0, id, fr - f0, ps));
                    let _ = len;
                }
                if starts.last().map_or(false, |s| fr > s.0 + 1800) { break; }
            }
            let mut by: std::collections::BTreeMap<(u64, u16), Vec<(u64, String)>> = Default::default();
            for (f0, id, d, ps) in ends { by.entry((f0, id)).or_default().push((d, ps)); }
            for ((f0, id), v) in by { println!("{p} id={id} f={f0} {}", v.iter().map(|(d, s)| format!("+{d}:[{s}]")).collect::<Vec<_>>().join(" ")); }
        }
        return;
    }
    if a.get(1).map(|s| s.as_str()) == Some("mouth") {
        // mouth <root> <frames> <from> <to>: per frame, for every portrait: (flags, idx, frame address) of its three objects
        let mut e = make(&a[2]);
        let (max, from, to): (u64, u64, u64) = (a[3].parse().unwrap(), a[4].parse().unwrap(), a[5].parse().unwrap());
        let mut v0 = 0u64;
        for fr in 1..=max {
            e.tick(if fr % 40 == 0 { 0x20 } else { 0 });
            for o in e.out.drain(..) { if let Out::Voice { id } = o { println!("{fr}: VOICE {id} len={}", e.voice_left); v0 = fr; } }
            if v0 > 0 && fr >= v0 + from && fr <= v0 + to {
                let mut s = format!("{}:", fr - v0);
                for p in portraits(&e) { s += &format!("  x{}:", p.1); for k in 2..=0 { let _ = k; } for sl in [p.0 - 2, p.0 - 1, p.0] { let o = &e.disp.sub.objs[sl]; s += &format!(" [{sl} fl{:x} i{} f{:x}]", o.flags, o.idx, o.frame & 0xFFFF); } }
                println!("{s}");
            }
            if v0 > 0 && fr > v0 + to { if a.get(6).is_none() { break; } v0 = 0; }
        }
        return;
    }
    let p = a.get(1).cloned().expect("SPnn:pc");
    let max: u64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1500);
    let png = a.get(3).cloned();
    let mut e = make(&p);
    let mut last = String::new();
    let mut voice_end = 0u64;
    for fr in 1..=max {
        e.tick(if fr % 40 == 0 { 0x20 } else { 0 });
        for o in e.out.drain(..) { if let Out::Voice { id } = o { println!("{fr}: VOICE {id} len={}", e.voice_left); voice_end = fr + e.voice_left as u64; } }
        let mut s = String::new();
        for i in 5..15 {
            let o = &e.disp.sub.objs[i];
            if o.active != 0 { s += &format!(" [{i} x={} y={} {}{}]", o.x >> 16, o.y >> 16, if o.flags & 1 != 0 { "P" } else { "-" }, if o.flags & 2 != 0 { "h" } else { "-" }); }
        }
        let fb = &e.fb;
        let colnb = |x: usize| (150..214).any(|y| { let k = (y * W + x) * 4; fb[k] != 0 || fb[k + 1] != 0 || fb[k + 2] != 0 });
        let mut runs = vec![]; let mut x = 0;
        while x < W { if !colnb(x) { x += 1; continue; } let x0 = x; let mut g = 0; while x < W && g < 4 { if colnb(x) { g = 0 } else { g += 1 } x += 1; } runs.push((x0, x - g - 1)); }
        let r: Vec<String> = runs.iter().filter(|r| r.1 - r.0 > 8).map(|&(a, b)| format!("x{a}-{b}")).collect();
        let line = format!("{s} | {}", r.join(" "));
        if line != last && fr <= voice_end + 30 { println!("{fr}: {line}"); last = line; }
        if let Some(d) = &png { if voice_end > 0 && fr % 20 == 0 && fr < voice_end { let _ = std::fs::create_dir_all(d); let w = std::io::BufWriter::new(std::fs::File::create(format!("{d}/{fr:05}.png")).unwrap()); let mut enc = png::Encoder::new(w, W as u32, H as u32); enc.set_color(png::ColorType::Rgba); enc.set_depth(png::BitDepth::Eight); enc.write_header().unwrap().write_image_data(&e.fb).unwrap(); } }
    }
}
