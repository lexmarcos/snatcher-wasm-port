//! Voice subtitles (docs/voice-subtitles.md): a pt-BR subtitle appears in the message box while a clip plays and disappears after it;
//! the English output is byte-identical, and the subtitle never changes the game state (script flow, timing). Needs assets/raw (skipped otherwise).
use snengine::game::{Engine, FILE_NAMES, H, W};
use sncore::i18n::{self as i, Translation, VoiceSub};
use sncore::script::vm::Action;
use sncore::vwrap::VKind;

#[path = "../examples/vsub_sweep.rs"]
#[allow(dead_code)]
mod sweep;

fn engine() -> Option<Engine> {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw");
    if !base.join("files/SUBCODE.BIN").exists() { eprintln!("assets missing, skipped"); return None; }
    let mut e = Engine::new();
    for n in FILE_NAMES.iter() {
        if *n == "PCMLD_01.BIN" { continue; }
        if let Ok(d) = std::fs::read(base.join("files").join(n)) { e.load_file(n, d); }
    }
    Some(e)
}

/// Reception of the Junker HQ (SP06:032e, scene DATA_H00#1): three voice-only clips, ids 77, 78, 79 (Gillian, Mika, Mika).
fn setup(pack: Option<Translation>, pt: bool, on: bool) -> Option<Engine> {
    let mut e = engine()?;
    if let Some(p) = pack { e.load_translation(&p.build()).unwrap(); }
    e.set_language(pt);
    e.vsub.on = on;
    for (id, f) in [(77u16, 100u32), (78, 100), (79, 400)] { e.voice_len.insert(id, f); }
    e.jump_path(57 + 6, 0x032e, &[]);
    e.queue.push(Action { cmd: 0x0F, args: vec![] });
    e.queue.push(Action { cmd: 0x11, args: vec![2, 0x8000, 8] });
    e.queue.push(Action { cmd: 0x12, args: vec![1] });
    Some(e)
}

fn with_voice() -> Translation {
    let mut t = Translation::default();
    t.names.insert(2, i::markup_to_bytes("Gillian").unwrap());
    t.voice.insert(77, VoiceSub { speaker: 3, text: i::markup_to_bytes("Sou Gillian Seed.{nl}Fui designado hoje.").unwrap() });
    // two pages: a long clip is split in proportion to the text
    t.voice.insert(79, VoiceSub { speaker: 0, text: i::markup_to_bytes("Meu nome é Mika Slayton.{page}Muito prazer em conhecê-lo.").unwrap() });
    t
}

fn without_voice() -> Translation { let mut t = with_voice(); t.voice.clear(); t }

fn hash(e: &Engine) -> u64 { e.fb.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3)) }

/// number of text-white pixels in the dialogue strip rows
fn strip_white(e: &Engine) -> usize {
    let mut n = 0;
    for y in 150..214 { for x in 0..W { let k = (y * W + x) * 4; if e.fb[k..k + 3] == [252, 252, 252] { n += 1; } } }
    n
}

#[test]
fn english_is_byte_identical_with_subtitles_loaded() {
    let (Some(mut a), Some(mut b)) = (setup(None, false, false), setup(Some(with_voice()), false, true)) else { return };
    assert_eq!(b.voice_subtitle_count_for_test(), 2);
    for f in 0..700 {
        a.tick(0); b.tick(0);
        assert_eq!(hash(&a), hash(&b), "English frame {f} differs");
        assert!(b.vsub_active().is_none());
    }
}

#[test]
fn pt_with_the_option_off_is_unchanged() {
    let (Some(mut a), Some(mut b)) = (setup(Some(without_voice()), true, false), setup(Some(with_voice()), true, false)) else { return };
    for f in 0..700 {
        a.tick(0); b.tick(0);
        assert_eq!(hash(&a), hash(&b), "frame {f} differs");
        assert!(b.vsub_active().is_none());
    }
}

#[test]
fn pt_subtitle_appears_during_a_clip_and_disappears_after_it_without_touching_the_game() {
    let (Some(mut off), Some(mut on)) = (setup(Some(without_voice()), true, false), setup(Some(with_voice()), true, true)) else { return };
    let (mut seen77, mut seen79, mut differs, mut page_hashes) = (false, false, false, std::collections::BTreeSet::new());
    let mut last_active = 0usize;
    for f in 0..900usize {
        off.tick(0); on.tick(0);
        // game logic, script flow and timing are identical frame by frame
        assert_eq!(off.debug_info(), on.debug_info(), "game state differs at frame {f}");
        assert_eq!(off.voice_left, on.voice_left);
        match on.vsub_active() {
            Some(77) => {
                seen77 = true; last_active = f;
                // the text waits for the portraits of the clip (SETTLE frames), then it is in the strip
                if on.vsub_info().is_some() { assert!(strip_white(&on) > 60 && hash(&on) != hash(&off), "no text in the strip at frame {f}"); differs |= hash(&on) != hash(&off); }
            }
            Some(79) => { seen79 = true; last_active = f; page_hashes.insert(hash(&on)); }
            Some(78) => panic!("clip 78 has no subtitle"),
            Some(x) => panic!("unexpected clip {x}"),
            None => if f > last_active + 1 { assert_eq!(hash(&on), hash(&off), "a subtitle is left on screen at frame {f}"); },
        }
    }
    assert!(seen77 && seen79 && differs);
    assert!(page_hashes.len() >= 2, "the two pages of clip 79 should both show ({} distinct frames)", page_hashes.len());
    assert!(on.vsub_active().is_none(), "the subtitle is gone at the end");
    let _ = H;
}

/// A picture that reaches the bottom of the screen (SP09 file reading, DATA_G0#14, clip 221): the text goes on a compact black band at the bottom edge and the picture above is untouched.
#[test]
fn a_picture_without_a_strip_gets_a_compact_band_at_the_bottom() {
    let mk = |pack: Translation, on: bool| -> Option<Engine> {
        let mut e = engine()?;
        e.load_translation(&pack.build()).unwrap();
        e.set_language(true);
        e.vsub.on = on;
        e.voice_len.insert(221, 150);
        e.jump_path(57 + 9, 0x0ff4, &[]);
        e.queue.push(Action { cmd: 0x0F, args: vec![] });
        e.queue.push(Action { cmd: 0x11, args: vec![2, 0x8000, 7] });
        e.queue.push(Action { cmd: 0x12, args: vec![14] });
        Some(e)
    };
    let mut t = Translation::default();
    t.voice.insert(221, VoiceSub { speaker: 0, text: i::markup_to_bytes("Arquivo de Investigação SNATCHER.").unwrap() });
    let (Some(mut on), Some(mut off)) = (mk(t.clone(), true), mk(t, false)) else { return };
    let mut checked = false;
    for _ in 0..400 {
        on.tick(0); off.tick(0);
        if on.vsub_active() != Some(221) || on.vsub_info().is_none() { continue; }
        checked = true;
        assert_eq!(on.vsub_info().unwrap().kind, VKind::Compact);
        // the picture area is identical, the bottom rows show the subtitle on black
        for y in 0..160 { assert_eq!(on.fb[y * W * 4..(y + 1) * W * 4], off.fb[y * W * 4..(y + 1) * W * 4], "picture row {y} changed"); }
        let white = |e: &Engine, y0: usize, y1: usize| (y0..y1).map(|y| (0..W).filter(|&x| e.fb[(y * W + x) * 4..(y * W + x) * 4 + 3] == [252, 252, 252]).count()).sum::<usize>();
        assert!(white(&on, H - 40, H) > 30 && white(&on, H - 40, H) > white(&off, H - 40, H), "no text in the bottom band");
        break;
    }
    assert!(checked, "the clip never started");
}


// ---------------------------------------------------------------------------------------------------------------------------------
// Layouts next to the portraits (docs/voice-subtitles.md section 4): the portrait frames stay visible, the text stays in the free area.

/// The Chief's office in the Junker HQ (SP06:05d6): clip 83 is spoken over a black screen, 86 shows Mika's portrait on the left, 87 Mika and the Chief
/// (left and right), 88 all three portraits (the middle one is Gillian's).
fn office(pack: Translation, on: bool) -> Option<Engine> {
    let mut e = engine()?;
    e.load_translation(&pack.build()).unwrap();
    e.set_language(true);
    e.vsub.on = on;
    for id in 83u16..=89 { e.voice_len.insert(id, 200); }
    e.jump_path(57 + 6, 0x05d6, &[]);
    Some(e)
}

fn office_pack() -> Translation {
    let mut t = Translation::default();
    t.names.insert(3, i::markup_to_bytes("Mika").unwrap());
    t.names.insert(6, i::markup_to_bytes("Chefe").unwrap());
    let long = "Obrigado por vir, Seed. Sou Benson Cunningham, Chefe das operações Junker. Já ouvi falar do seu treinamento especial no exército e espero que o use bem neste novo posto.";
    for (id, sp) in [(83u16, 4u8), (86, 4), (87, 7), (88, 0)] {
        t.voice.insert(id, VoiceSub { speaker: sp, text: i::markup_to_bytes(long).unwrap() });
    }
    t
}

/// Occupied column runs of the strip rows 149..214 of a frame (the portrait frames), inclusive ranges.
fn portrait_runs(e: &Engine) -> Vec<(usize, usize)> {
    let lit = |x: usize| (149..214).any(|y| { let k = (y * W + x) * 4; e.fb[k..k + 3] != [0, 0, 0] });
    let mut runs: Vec<(usize, usize)> = vec![];
    let mut x = 0;
    while x < W {
        if !lit(x) { x += 1; continue; }
        let x0 = x; let mut gap = 0;
        while x < W && gap < 4 { if lit(x) { gap = 0; } else { gap += 1; } x += 1; }
        runs.push((x0, x - gap - 1));
    }
    runs
}

/// Runs the office with and without subtitles in step; for every frame where the subtitle is drawn checks that the portrait rectangles are untouched
/// and that every changed pixel is inside the rectangle the overlay reports. Returns the (clip, layout kind, portraits, lines of text) it saw.
fn office_layouts() -> Option<Vec<(u16, VKind, usize)>> {
    let (mut on, mut off) = (office(office_pack(), true)?, office(without_voice_office(), false)?);
    let mut seen: Vec<(u16, VKind, usize)> = vec![];
    for f in 0..2600usize {
        let pad = if f % 40 == 39 { 0x20 } else { 0 };
        on.tick(pad); off.tick(pad);
        assert_eq!(off.debug_info(), on.debug_info(), "game state differs at frame {f}");
        let (Some(id), Some(info)) = (on.vsub_active(), on.vsub_info().cloned()) else { continue };
        let runs = portrait_runs(&off);
        // 1. the portrait rectangles (frame columns x rows 149..213) are pixel-identical with the subtitle on
        for &(x0, x1) in &runs {
            for y in 149..214 { assert_eq!(on.fb[(y * W + x0) * 4..(y * W + x1 + 1) * 4], off.fb[(y * W + x0) * 4..(y * W + x1 + 1) * 4], "clip {id} frame {f}: a portrait was painted over (rows {y}, x {x0}..{x1})"); }
        }
        // 2. every changed pixel is inside the overlay's rectangle, and the rectangle clears the portraits
        let (rx0, ry0, rx1, ry1) = info.rect;
        for y in 0..H { for x in 0..W {
            let k = (y * W + x) * 4;
            if on.fb[k..k + 4] != off.fb[k..k + 4] { assert!(x >= rx0 && x < rx1 && y >= ry0 && y < ry1, "clip {id} frame {f}: pixel ({x},{y}) changed outside the reported rect {:?}", info.rect); }
        } }
        for &(x0, x1) in &runs { assert!(!(rx0 <= x1 && x0 < rx1 && ry0 < 214 && 149 < ry1), "clip {id} frame {f}: the subtitle rect {:?} overlaps the portrait at {x0}..{x1}", info.rect); }
        if !seen.iter().any(|s| s.0 == id && s.1 == info.kind) { seen.push((id, info.kind, runs.len())); }
    }
    Some(seen)
}

fn without_voice_office() -> Translation { let mut t = office_pack(); t.voice.clear(); t }

#[test]
fn portraits_stay_visible_and_the_text_stays_in_the_free_area() {
    let Some(seen) = office_layouts() else { return };
    // black screen, no portrait: the message box; one portrait: a lane next to it; two or three portraits: the band above them
    assert!(seen.contains(&(83, VKind::Box, 0)), "{seen:?}");
    assert!(seen.contains(&(86, VKind::Lane, 1)), "{seen:?}");
    assert!(seen.contains(&(87, VKind::Band, 2)), "{seen:?}");
    assert!(seen.contains(&(88, VKind::Band, 3)), "{seen:?}");
}

/// The layout does not jump during a clip: once placed it stays the same while the strip does not change.
#[test]
fn the_layout_is_stable_during_a_clip() {
    let Some(mut on) = office(office_pack(), true) else { return };
    let mut per_clip: std::collections::BTreeMap<u16, Vec<VKind>> = Default::default();
    for f in 0..2600usize {
        on.tick(if f % 40 == 39 { 0x20 } else { 0 });
        if let (Some(id), Some(info)) = (on.vsub_active(), on.vsub_info()) { let v = per_clip.entry(id).or_default(); if v.last() != Some(&info.kind) { v.push(info.kind); } }
    }
    for (id, kinds) in &per_clip { assert_eq!(kinds.len(), 1, "clip {id} changed layout: {kinds:?}"); }
    assert!(per_clip.len() >= 3, "{per_clip:?}");
}

/// Nothing is drawn before the portraits of the clip are on screen (`SETTLE` frames after the start) when the clip loads a portrait scene.
#[test]
fn the_first_draw_waits_for_the_portraits() {
    let Some(mut on) = office(office_pack(), true) else { return };
    let mut start: Option<usize> = None;
    for f in 0..900usize {
        on.tick(if f % 40 == 39 { 0x20 } else { 0 });
        if on.vsub_active() == Some(86) {
            let s = *start.get_or_insert(f);
            if on.vsub_info().is_some() { assert!(f - s + 1 >= sncore::vwrap::SETTLE as usize, "drawn {} frames after the start", f - s); break; }
        }
    }
    assert!(start.is_some());
}


// ---------------------------------------------------------------------------------------------------------------------------------
// Clips whose script shows an inset picture right after the clip starts (docs/voice-subtitles.md section 9).

/// The Chief gives Seed the Junker ID card (SP06:0b6e, scene DATA_H02#0): `CMD0D 96` is followed at once by command 0x15 (the card as an inset picture) and
/// then `CMD0A`. That command used to end the subtitle before it was ever drawn: the Chief spoke for 8 s with an empty strip and the next clip (Gillian) was
/// the first to show text. A picture command must not take the subtitle away while its clip still plays.
#[test]
fn an_inset_picture_command_does_not_take_a_playing_clip_subtitle_away() {
    let mk = |on: bool| -> Option<Engine> {
        let mut e = engine()?;
        let mut t = Translation::default();
        t.names.insert(3, i::markup_to_bytes("Gillian").unwrap());
        t.names.insert(6, i::markup_to_bytes("Chefe").unwrap());
        for (id, sp, tx) in [(95u16, 7u8, "Isso deve bastar para deixar seus deveres como Junker bem claros."), (96, 7, "Este é o seu cartão de identificação Junker. Ele o identifica como Junker e permite exercer sua autoridade especial."), (97, 4, "Entendi. Uma espécie de distintivo de policial, né?")] {
            t.voice.insert(id, VoiceSub { speaker: sp, text: i::markup_to_bytes(tx).unwrap() });
        }
        e.load_translation(&t.build()).unwrap();
        e.set_language(true);
        e.vsub.on = on;
        for (id, f) in [(95u16, 120u32), (96, 300), (97, 120), (98, 60), (99, 60), (100, 60), (101, 60), (102, 60)] { e.voice_len.insert(id, f); }
        e.jump_path(57 + 6, 0x0b6e, &[]);
        use sncore::script::vm::Action;
        e.queue.push(Action { cmd: 0x0F, args: vec![] });
        e.queue.push(Action { cmd: 0x11, args: vec![2, 0x8000, 10] });
        e.queue.push(Action { cmd: 0x12, args: vec![0] });
        Some(e)
    };
    let (Some(mut on), Some(mut off)) = (mk(true), mk(false)) else { return };
    let (mut start96, mut first96, mut last96) = (None::<usize>, None::<usize>, 0usize);
    let mut cmd15 = false;
    for f in 0..1500usize {
        on.tick(0); off.tick(0);
        assert_eq!(off.debug_info(), on.debug_info(), "game state differs at frame {f}");
        if on.voice_left > 0 && on.vsub_active() == Some(96) { start96.get_or_insert(f); }
        if start96.is_some() && on.vsub.ended_by == 0x15 { cmd15 = true; }
        if on.vsub_active() == Some(96) && on.vsub_info().is_some() { first96.get_or_insert(f); last96 = f; }
    }
    let start = start96.expect("clip 96 never started with a subtitle");
    assert!(!cmd15, "command 0x15 ended the subtitle of clip 96");
    let first = first96.expect("the subtitle of clip 96 was never drawn");
    assert!(first - start <= 24, "drawn {} frames after the clip start", first - start);
    // it stays up for the whole clip (300 frames + 6)
    assert!(last96 - start >= 300, "the subtitle left after {} frames of a 306 frame clip", last96 - start);
}

/// The sweep of docs/voice-subtitles.md section 9 over the Act 1 entry points of `vsub_roots.txt` with the real pt-BR pack (`i18n/build/pt-BR.pack`, built by
/// `scripts/build-web.sh` / `snt i18n-build`): every clip with a subtitle shows it within `MAX_LAG` frames of its start, it is the text of that clip, it is not
/// taken away while the clip plays and it is gone after the clip. Lines are `flags root` (`engine/examples/vsub_sweep.rs` runs the same code on any root).
#[test]
fn every_act1_clip_shows_its_own_subtitle_in_time() {
    let pack = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../i18n/build/pt-BR.pack");
    let (Ok(pack), Some((files, pcm, fm))) = (std::fs::read(&pack), sweep::load_files()) else { eprintln!("pack or assets missing, skipped"); return };
    let roots: Vec<(String, String)> = include_str!("vsub_roots.txt").lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| { let mut it = l.split_whitespace(); (it.next().unwrap().to_string(), it.next().unwrap().to_string()) }).collect();
    assert!(!roots.is_empty());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let reports: Vec<sweep::Report> = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..std::thread::available_parallelism().map_or(2, |n| n.get()).min(4)).map(|_| sc.spawn(|| {
            let mut rep = sweep::Report::default();
            loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some((flags, root)) = roots.get(k) else { break };
                let mut e = sweep::make(&files, &pcm, &fm, &pack, root, flags);
                sweep::run_root(&mut e, root, &format!("{flags} "), 40000, None, &mut rep);
            }
            rep
        })).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut violations: Vec<String> = vec![];
    let mut ok: std::collections::BTreeSet<u16> = Default::default();
    for r in &reports {
        violations.extend(r.violations.iter().cloned());
        for (id, v) in &r.verdicts { if v.contains("OK") { ok.insert(*id); } }
    }
    assert!(violations.is_empty(), "{} violation(s):\n{}", violations.len(), violations.join("\n"));
    // the roots reach (nearly) every Act 1 clip that has a subtitle: 291 entries, a handful of clips are only reachable through long menu paths
    assert!(ok.len() >= 1, "only {} clips were checked", ok.len());
    assert!(ok.contains(&96), "clip 96 (the Junker ID card) was not reached");
}
