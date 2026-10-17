//! Voice subtitle sweep (docs/voice-subtitles.md, section 9): play script entry points with the pt-BR pack and the subtitles on, and for every voice
//! clip that starts check what the overlay does:
//!   (a) a clip that has a subtitle shows it within `MAX_LAG` frames of its start (`LATE`; `MISSING` when it never shows),
//!   (b) what is drawn is the text of the clip that started last (`WRONG`: never the text of an earlier clip),
//!   (c) nothing is drawn after the clip + hold is over (`STALE`) nor while a message box is open (`MSG`), and the painted rectangle never covers a portrait frame (`COVER`),
//!   (d) a subtitle is never taken away while its clip still plays and no message, menu or other clip came (`EARLY`, with the command that did it).
//! `CUT` = the script itself ended the clip before the subtitle could show (another clip or a message within `MAX_LAG` frames): not a violation.
//!
//!   vsub_sweep <pack> <frames> <roots.txt|SPnn:pc[:path[:scene]]> ...      (a file has one root per line, as work/voice/probes.txt)
//!   env: FLAGS=zero|ones|alt|randN (story flags, as voice_probe), ALL=1 (print OK clips too), ONLY=<id> (log of that clip), PNG=<dir> (frame of every violation)
//! One line per clip, a summary, exit status 1 when there is a violation. `engine/tests/vsub.rs` runs the same code (`run`) over a list of roots.
use snengine::game::{Engine, Out, FILE_NAMES, H, W};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_LAG: u32 = 24;

pub fn load_files() -> Option<(Vec<(String, Vec<u8>)>, String, String)> {
    let raw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw/files");
    if !raw.join("SUBCODE.BIN").exists() { return None; }
    let aw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/audio/web");
    let pcm = std::fs::read_to_string(aw.join("pcm.json")).unwrap_or_default();
    let fm = std::fs::read_to_string(aw.join("fm.json")).unwrap_or_default();
    let mut files: Vec<(String, Vec<u8>)> = vec![];
    for n in FILE_NAMES.iter() { if *n == "PCMLD_01.BIN" { continue; } if let Ok(d) = std::fs::read(raw.join(n)) { files.push((n.to_string(), d)); } }
    Some((files, pcm, fm))
}

pub fn make(files: &[(String, Vec<u8>)], pcm: &str, fm: &str, pack: &[u8], root: &str, flags: &str) -> Engine {
    let mut e = Engine::new();
    if !pcm.is_empty() { e.load_audio_manifests(pcm, fm); }
    for (n, d) in files { e.load_file(n, d.clone()); }
    let mut it = root.split(':');
    let (f, pc) = (it.next().unwrap(), it.next().unwrap());
    let path: Vec<u16> = it.next().map(|x| x.split(',').filter_map(|h| u16::from_str_radix(h, 16).ok()).collect()).unwrap_or_default();
    let scene: Option<u16> = it.next().and_then(|x| u16::from_str_radix(x, 16).ok());
    e.load_translation(pack).expect("valid pack");
    e.set_language(true);
    e.vsub.on = true;
    e.jump_path(57 + f[2..].parse::<u16>().unwrap(), u16::from_str_radix(pc, 16).unwrap(), &path);
    if let Some(r) = scene {
        use sncore::script::vm::Action;
        e.queue.push(Action { cmd: 0x0F, args: vec![] });
        e.queue.push(Action { cmd: 0x11, args: vec![2, 0x8000, r >> 8] });
        e.queue.push(Action { cmd: 0x12, args: vec![r & 0xFF] });
    }
    // story flags go in after the jump: `jump` starts a blank VM and would wipe them
    match flags {
        "ones" => for b in e.vm.save[sncore::script::vm::VARS_OFF..].iter_mut() { *b = 0xFF; },
        "alt" => for (i, b) in e.vm.save[sncore::script::vm::VARS_OFF..].iter_mut().enumerate() { *b = if i % 2 == 0 { 0xAA } else { 0x55 }; },
        x if x.starts_with("rand") => {
            // pseudo-random story flags (the seed after "rand"), as voice_probe does
            let mut st: u64 = x[4..].parse::<u64>().unwrap_or(1).wrapping_mul(0x9E3779B97F4A7C15) | 1;
            for b in e.vm.save[sncore::script::vm::VARS_OFF..].iter_mut() { st ^= st << 13; st ^= st >> 7; st ^= st << 17; *b = (st >> 24) as u8; }
        }
        _ => {}
    }
    e
}

pub fn save_png(dir: &str, name: &str, fb: &[u8]) {
    let _ = std::fs::create_dir_all(dir);
    let w = std::io::BufWriter::new(std::fs::File::create(format!("{dir}/{name}.png")).unwrap());
    let mut enc = png::Encoder::new(w, W as u32, H as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(fb).unwrap();
}

struct Clip { id: u16, start: u64, play: u32, entry: bool, first: Option<u64>, last: Option<u64>, active_until: u64, layout: u8 }

/// What a run found: one line per clip start, the verdicts per clip id, and the violations.
#[derive(Default)]
pub struct Report { pub lines: Vec<String>, pub verdicts: BTreeMap<u16, BTreeSet<&'static str>>, pub violations: Vec<String> }

/// Play `root` for up to `frames` frames (it stops when no new clip started for 900 frames) and check every clip. `png`: a directory for a frame of each violation.
pub fn run_root(e: &mut Engine, root: &str, tag_prefix: &str, frames: u64, png: Option<&str>, rep: &mut Report) {
    let mut clips: Vec<Clip> = vec![];
    let mut flagged: Vec<(usize, &'static str, u64)> = vec![];
    let mut early: Vec<(usize, u16, u32, u64)> = vec![];
    let mut prev_act: Option<u16> = None;
    let mut idle = 0u64;
    let mut last_fr = 0u64;
    for fr in 1..=frames {
        e.tick(if fr % 40 == 0 { 0x20 } else { 0 });
        let mut started = false;
        let outs: Vec<Out> = e.out.drain(..).collect();
        for o in outs {
            if let Out::Voice { id } = o {
                let entry = e.vsub_active() == Some(id);
                clips.push(Clip { id, start: fr, play: e.voice_left, entry, first: None, last: None, active_until: 0, layout: 0 });
                started = true;
            }
        }
        // a root stops when no clip it has not played yet started for 1500 frames (menus waiting for a choice, loops)
        if started && !clips[..clips.len() - 1].iter().any(|c| Some(c.id) == clips.last().map(|l| l.id)) { idle = 0; }
        last_fr = fr;
        let ci = clips.len().wrapping_sub(1);
        let act = e.vsub_active();
        // (d) the subtitle went away while its clip still plays and the script did not start another clip
        if !started && prev_act.is_some() && act.is_none() && e.voice_left > 8 && e.msg.is_none() && e.menu.is_none() && clips.last().map_or(false, |c| Some(c.id) == prev_act) {
            early.push((ci, e.vsub.ended_by, e.voice_left, fr));
        }
        prev_act = act;
        let Some(cur) = clips.last_mut() else { continue };
        if act == Some(cur.id) { cur.active_until = fr; }
        if let Some((k, rect, runs)) = e.vsub_info().map(|i| (i.kind, i.rect, i.runs.clone())) {
            let k = k as u8 + 1;
            // (e) the painted rectangle never covers a portrait frame (rows 149..214 of an occupied column run)
            let (rx0, ry0, rx1, ry1) = rect;
            if runs.iter().any(|&(x0, x1)| rx0 as u32 <= x1 && x0 < rx1 as u32 && ry0 < 214 && 149 < ry1) { flagged.push((ci, "COVER", fr)); }
            if act != Some(cur.id) { flagged.push((ci, "WRONG", fr)); if let Some(d) = png { save_png(d, &format!("wrong-{}-{}", cur.id, fr), &e.fb); } }
            if e.msg.is_some() { flagged.push((ci, "MSG", fr)); }
            if cur.first.is_none() { cur.first = Some(fr); cur.layout = k; if let Some(d) = png { if fr - cur.start > MAX_LAG as u64 { save_png(d, &format!("late-{}-{}", cur.id, fr), &e.fb); } } }
            cur.last = Some(fr);
        }
        // (c) nothing after the clip and the longest hold
        if act.is_some() && fr > cur.start + cur.play as u64 + sncore::i18n::VOICE_HOLD_MAX as u64 + 4 { flagged.push((ci, "STALE", fr)); }
        idle += 1;
        if idle > 1500 { break; }
    }
    for (k, c) in clips.iter().enumerate() {
        let next = clips.get(k + 1).map(|n| n.start).unwrap_or(u64::MAX);
        let kinds: BTreeSet<&str> = flagged.iter().filter(|f| f.0 == k).map(|f| f.1).collect();
        let ea: Vec<&(usize, u16, u32, u64)> = early.iter().filter(|x| x.0 == k).collect();
        // clip time the script left it alone (the next clip started)
        let window = next.min(c.start + c.play as u64).saturating_sub(c.start);
        let tag: &'static str = if !ea.is_empty() { "EARLY" }
            else if kinds.contains("COVER") { "COVER" } else if kinds.contains("WRONG") { "WRONG" } else if kinds.contains("MSG") { "MSG" } else if kinds.contains("STALE") { "STALE" }
            else if !c.entry { "NOENTRY" }
            else if let Some(f) = c.first { if f - c.start > MAX_LAG as u64 { "LATE" } else { "OK" } }
            else if window <= MAX_LAG as u64 || c.start + MAX_LAG as u64 >= last_fr { "CUT" }
            else { "MISSING" };
        let why = ea.first().map_or(String::new(), |x| format!(" cmd={:#x} at={} left={}", x.1, x.3, x.2));
        let line = format!("{tag_prefix}{root} id={} start={} play={} first={} lag={} last={} layout={} {tag}{why}", c.id, c.start, c.play,
            c.first.map_or("-".into(), |f| f.to_string()), c.first.map_or("-".into(), |f| (f - c.start).to_string()), c.last.map_or("-".into(), |f| f.to_string()), c.layout);
        if matches!(tag, "LATE" | "MISSING" | "WRONG" | "STALE" | "MSG" | "EARLY" | "COVER") {
            rep.violations.push(line.clone());
            if tag == "MISSING" { if let Some(d) = png { save_png(d, &format!("missing-{}", c.id), &e.fb); } }
        }
        rep.verdicts.entry(c.id).or_default().insert(tag);
        rep.lines.push(line);
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let pack = std::fs::read(&a[1]).expect("pack");
    let frames: u64 = a[2].parse().unwrap();
    let only: Option<u16> = std::env::var("ONLY").ok().and_then(|v| v.parse().ok());
    let all = std::env::var("ALL").is_ok();
    let flags = std::env::var("FLAGS").unwrap_or_default();
    let png = std::env::var("PNG").ok();
    let mut roots: Vec<String> = vec![];
    for x in &a[3..] {
        if std::path::Path::new(x).is_file() { for l in std::fs::read_to_string(x).unwrap().lines() { if !l.trim().is_empty() && !l.starts_with('#') { roots.push(l.trim().to_string()); } } } else { roots.push(x.clone()); }
    }
    let (files, pcm, fm) = load_files().expect("assets/raw/files");
    let mut rep = Report::default();
    for root in &roots {
        let mut e = make(&files, &pcm, &fm, &pack, root, &flags);
        run_root(&mut e, root, "", frames, png.as_deref(), &mut rep);
    }
    for l in &rep.lines {
        let tag = l.rsplit(' ').next().unwrap_or("");
        let _ = tag;
        let id: Option<u16> = l.split(" id=").nth(1).and_then(|r| r.split(' ').next()).and_then(|x| x.parse().ok());
        let ok = l.contains(" OK") || l.contains(" NOENTRY");
        if only.is_some() { if id == only { println!("{l}"); } } else if all || !ok { println!("{l}"); }
    }
    let mut by: BTreeMap<&str, BTreeSet<u16>> = BTreeMap::new();
    for (id, v) in &rep.verdicts { for t in v { by.entry(t).or_default().insert(*id); } }
    println!("clips seen: {}", rep.verdicts.len());
    for (k, v) in &by { println!("{k}: {} clips{}", v.len(), if matches!(*k, "OK" | "CUT" | "NOENTRY") { String::new() } else { format!(": {:?}", v) }); }
    if !rep.violations.is_empty() { println!("{} violation(s)", rep.violations.len()); std::process::exit(1); }
}
