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
        } else if e.phase == VmPhase::Menu {
            if let Some((m, items)) = e.menu.as_ref() {
                let names: Vec<String> = items.iter().map(|&o| item_text(e, o)).collect();
                let target = want.get(wi).and_then(|w| names.iter().position(|n| n.trim() == *w)).unwrap_or(0);
                if (m.cursor as usize) < target { pad = DOWN; } else if m.cursor as usize == target {
                    pad = C;
                    if want.get(wi).map(|w| names.get(target).map(|n| n.trim() == *w).unwrap_or(false)).unwrap_or(false) { wi += 1; }
                }
                held_gap = 20;
            }
        } else if f % 40 == 0 { pad = C; held_gap = 2; }
        e.tick(pad);
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

#[test]
fn portrait_dialogue_text_visible_from_title() {
    let Some(mut e) = engine() else { return };
    // TALK → MIKA at the reception: Mika's close-up with Gillian's portrait (voiced lines)
    let mut seen = 0;
    let r = play(&mut e, &["TALK", "MIKA"], 20000, |e| {
        // a portrait dialogue: palette split active (main $E036) and a message being typed
        if e.disp.main.e036 != 0 && e.text.busy { seen += 1; }
        seen > 60
    });
    let f = r.expect("never reached a portrait dialogue from the title screen");
    let (text, white_bg) = text_pixels(&e.fb);
    assert!(text > 40, "frame {f}: message box has no text pixels ({text})");
    assert_eq!(white_bg, 0, "frame {f}: message box background is a flat bright colour");
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
