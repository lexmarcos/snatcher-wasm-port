//! Regression tests that follow the path a player takes in the browser: power-on → title →
//! START → options → intro (skipped with START) → Junker HQ, then menus chosen by their text
//! (TALK → MIKA …) like a player would, using only pad input. Needs the user's extracted
//! assets (assets/raw); skipped when they are absent.
//!
//! 1. Dialogue text must be visible in portrait dialogues (bug: the message panel was solid
//!    white with no text – the renderer copied an oracle artefact that wrote white to CRAM 0..3).
//! 2. Skipping the intro must stop its CD-DA track before the game's own music starts
//!    (bug: the intro track kept playing in Junker HQ).
use snengine::game::{Engine, Out, VmPhase, FILE_NAMES, H, W};

fn engine() -> Option<Engine> {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw");
    if !base.join("files/SUBCODE.BIN").exists() { eprintln!("assets missing, skipped"); return None; }
    let mut e = Engine::new();
    for n in FILE_NAMES.iter() {
        if *n == "PCMLD_01.BIN" { continue; }
        if let Ok(d) = std::fs::read(base.join("files").join(n)) { e.load_file(n, d); }
    }
    for (n, f) in [("HEADER.BIN", "header.bin"), ("IP.BIN", "ip.bin")] {
        if let Ok(d) = std::fs::read(base.join(f)) { e.load_file(n, d); }
    }
    let aw = base.join("../audio/web");
    if let (Ok(p), Ok(f)) = (std::fs::read_to_string(aw.join("pcm.json")), std::fs::read_to_string(aw.join("fm.json"))) {
        e.load_audio_manifests(&p, &f);
    }
    Some(e)
}

const START: u8 = 0x80;
const C: u8 = 0x20;
const DOWN: u8 = 0x02;

fn item_text(e: &Engine, off: u16) -> String {
    let o = 0x3800 + off as usize;
    e.script.get(o..).unwrap_or(&[]).iter().take_while(|&&c| c != 0xFF && c != 0xF2)
        .filter(|c| (0x20..0x7F).contains(*c)).map(|&c| c as char).collect()
}

/// Drive the engine like a player. `want` = menu item texts to pick in order (others: first item).
/// Presses START every 60 frames until the game proper starts, then C to advance messages.
/// Calls `stop(e)` every frame; returns the frame when it returned true.
fn play(e: &mut Engine, want: &[&str], max: u64, mut stop: impl FnMut(&Engine) -> bool) -> Option<u64> {
    e.boot();
    let mut wi = 0usize;
    let mut held_gap = 0u32;
    for f in 0..max {
        let mut pad = 0u8;
        if held_gap > 0 { held_gap -= 1; } else if e.top != snengine::game::Top::Game {
            if f % 60 == 30 { pad = START; held_gap = 2; }
        } else if e.menu.is_some() && e.phase != VmPhase::MenuClose {
            if let Some((m, items)) = e.menu.as_ref() {
                let names: Vec<String> = items.iter().map(|&o| item_text(e, o)).collect();
                if std::env::var("PATHDBG").is_ok() { eprintln!("{f} menu {names:?} cursor {} want {:?}", m.cursor, want.get(wi)); }
                let target = want.get(wi).and_then(|w| names.iter().position(|n| n.trim() == *w)).unwrap_or(0);
                if (m.cursor as usize) < target { pad = DOWN; } else if m.cursor as usize == target {
                    pad = C;
                    if want.get(wi).map(|w| names.get(target).map(|n| n.trim() == *w).unwrap_or(false)).unwrap_or(false) { wi += 1; }
                }
                held_gap = 20;
            }
        } else if f % 40 == 0 { pad = C; held_gap = 2; }
        e.tick(pad);
        if std::env::var("PATHDBG").is_ok() && f % 50 == 0 && f > 2300 && f < 3200 { eprintln!("{f} e036 {} busy {} {}", e.disp.main.e036, e.text.busy, e.debug_info()); }
        if std::env::var("PATHDBG").is_ok() && f % 500 == 0 { eprintln!("{f} top {:?} phase {:?} menu {} dbg {}", e.top, e.phase, e.menu.is_some(), e.debug_info()); }
        if let Ok(dir) = std::env::var("PATHSHOT") { if f % 100 == 0 { save_png(&format!("{dir}/f{f:05}.png"), &e.fb); } }
        if stop(e) { return Some(f); }
        e.out.clear(); // the front end drains events every frame
    }
    None
}

/// Message-box area (lines 150..220, x 100..250 = right of the portrait): count pixels that
/// differ from the area's most common colour.
fn text_pixels(fb: &[u8]) -> (usize, usize) {
    let mut hist = std::collections::HashMap::new();
    let mut px = vec![];
    for y in 150..220.min(H) { for x in 100..250.min(W) {
        let o = (y * W + x) * 4;
        let c = (fb[o], fb[o + 1], fb[o + 2]);
        *hist.entry(c).or_insert(0usize) += 1; px.push(c);
    }}
    let (bg, n) = hist.iter().max_by_key(|(_, n)| **n).map(|(c, n)| (*c, *n)).unwrap();
    let other = px.iter().filter(|c| **c != bg).count();
    let bright_bg = bg.0 as u32 + bg.1 as u32 + bg.2 as u32 > 600;
    (other, if bright_bg { n } else { 0 })
}

/// Bright pixels (all channels > 200) in a screen rectangle.
fn bright(fb: &[u8], x0: usize, x1: usize, y0: usize, y1: usize) -> usize {
    let mut n = 0;
    for y in y0..y1.min(H) { for x in x0..x1.min(W) {
        let o = (y * W + x) * 4;
        if fb[o] > 200 && fb[o + 1] > 200 && fb[o + 2] > 200 { n += 1; }
    }}
    n
}

#[test]
fn portrait_panel_dark_and_dialogue_text_visible_from_title() {
    let Some(mut e) = engine() else { return };
    // TALK at the reception: voice-only exchange with Mika's close-up and Gillian's portrait
    // (SP06 L_032e: CMD0D voice + CMD0A wait, no SAY – the original prints no text here; the
    // oracle running the original code shows the same dark, empty panel).
    let mut seen = 0;
    let r = play(&mut e, &["TALK"], 20000, |e| {
        if e.disp.main.e036 != 0 { seen += 1; } else { seen = 0; }
        seen > 120
    });
    let f = r.expect("never reached the portrait scene from the title screen");
    let b = bright(&e.fb, 100, 250, 150, 220);
    assert!(b < 500, "frame {f}: portrait panel is bright/white ({b} bright px) – CRAM 0..3 overwritten");
    // then the next spoken line with a message box must show light text on the dark box
    let mut typed = 0;
    let r = play_more(&mut e, 20000, |e| {
        if e.msg.is_some() && !e.text.busy { typed += 1; } else { typed = 0; }
        typed > 30
    });
    let f = r.expect("no message box after the portrait scene");
    let t = bright(&e.fb, 16, 256, 150, 215);
    assert!(t > 40, "frame {f}: message box shows no light text ({t} bright px)");
    let all = (150..215).map(|y| (16..256).filter(|&x| { let o = (y * W + x) * 4; e.fb[o] < 60 && e.fb[o + 1] < 60 && e.fb[o + 2] < 60 }).count()).sum::<usize>();
    assert!(all > 5000, "frame {f}: message box background is not dark ({all} dark px)");
}

/// Continue an already running game: advance messages with C, pick the first menu item.
fn play_more(e: &mut Engine, max: u64, mut stop: impl FnMut(&Engine) -> bool) -> Option<u64> {
    let mut gap = 0u32;
    for f in 0..max {
        let mut pad = 0u8;
        if gap > 0 { gap -= 1; } else if f % 40 == 0 { pad = C; gap = 2; }
        e.tick(pad);
        if stop(e) { return Some(f); }
        e.out.clear();
    }
    None
}

#[test]
fn skipping_intro_stops_cd_track() {
    let Some(mut e) = engine() else { return };
    let mut cd_playing = false;
    let mut leaked = None;
    let mut log = vec![];
    // skip the intro at several moments: START every 60 frames reaches the game quickly
    let r = play(&mut e, &[], 6000, |e| {
        for o in &e.out {
            match o {
                Out::Music { track } => { cd_playing = true; log.push(format!("cd {track}")); }
                Out::MusicStop => { cd_playing = false; log.push("stop".into()); }
                Out::Fm { id, music: true } => { log.push(format!("fm {id:#x}")); if cd_playing { leaked = Some(*id); } }
                _ => {}
            }
        }
        log.iter().any(|l| l.starts_with("fm"))
    });
    // Engine::tick drains nothing itself; clear events between frames like the front end
    let _ = r.expect("game never started");
    assert!(leaked.is_none(), "CD-DA still playing when game music {:?} started: {log:?}", leaked);
}

/// Write the framebuffer as a PNG (debugging aid: PATHSHOT=<dir>).
pub fn save_png(path: &str, fb: &[u8]) {
    let w = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut enc = png::Encoder::new(w, W as u32, H as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(fb).unwrap();
}

/// Skip the intro at several moments (frames after the intro module started) and check that
/// no CD-DA track is still playing when the game's own FM music starts.
#[test]
fn skipping_intro_at_several_points_stops_cd_track() {
    for delay in [60u64, 400, 1200, 2400, 8300, 9500] {
        let Some(mut e) = engine() else { return };
        e.boot();
        let (mut intro_at, mut cd_playing, mut leaked, mut log) = (None, false, None, vec![]);
        let mut done = false;
        for f in 0..20000u64 {
            let mut pad = 0u8;
            match e.top {
                snengine::game::Top::Title => if f % 60 == 30 { pad = START },
                snengine::game::Top::Intro => {
                    let s = *intro_at.get_or_insert(f);
                    if f >= s + delay && (f - s - delay) % 60 == 0 { pad = START }
                }
                _ => {}
            }
            e.tick(pad);
            for o in e.out.drain(..) {
                match o {
                    Out::Music { track } => { cd_playing = true; log.push(format!("{f} cd {track}")); }
                    Out::MusicStop => { cd_playing = false; log.push(format!("{f} stop")); }
                    Out::Fm { id, music: true } if e.top == snengine::game::Top::Game => {
                        log.push(format!("{f} fm {id:#x}")); if cd_playing { leaked = Some(id); } done = true;
                    }
                    _ => {}
                }
            }
            if done { break; }
        }
        assert!(done, "skip at +{delay}: game music never started: {log:?}");
        eprintln!("skip at +{delay}: {log:?}");
        assert!(leaked.is_none(), "skip at +{delay}: CD-DA still playing: {log:?}");
    }
}

/// VM state 4 (SETSTATE(2): QUIT INVESTIGATION, game over paths, S:0x1670C): the original runs the black-out queue [02], hands over to the SP, which
/// reloads SUBCODE and restarts at the copyright / Konami logo / title chain (oracle2 = real Sega CD BIOS, a2_taxi, docs/re/oracle2.md; the HLE oracle
/// stays black and the engine used to stay on the GAME OVER picture forever).
#[test]
fn state4_restarts_at_the_title() {
    let Some(mut e) = engine() else { return };
    e.jump(57 + 7, 0x1A07); // SP07 L_1a07 `sys.SETSTATE(2)`
    let (mut s4, mut back) = (None, None);
    for f in 0..3000u64 {
        e.tick(0);
        if e.vm.state == 4 && s4.is_none() { s4 = Some(f); }
        if s4.is_some() && e.top == snengine::game::Top::Title { back = Some(f); break; }
        e.out.clear();
    }
    let (s4, back) = (s4.expect("vm state 4 reached"), back.expect("title chain restarted"));
    // black-out [02] (~50 frames in the port, 36 in the original) + SP reload (90): about 127-150 frames in the original, never instantly
    assert!((100..=260).contains(&(back - s4)), "restart {} frames after state 4", back - s4);
    assert!(e.module.is_some(), "title module (copyright / logo / title) is running");
    assert!(e.menu.is_none() && e.msg.is_none(), "game state was reset");
    for _ in 0..1500 { e.tick(0); e.out.clear(); } // runs through the copyright and logo screens without panicking
    assert_eq!(e.top, snengine::game::Top::Title);
}
