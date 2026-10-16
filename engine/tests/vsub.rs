//! Voice subtitles (docs/voice-subtitles.md): a pt-BR subtitle appears in the message box while a clip plays and disappears after it;
//! the English output is byte-identical, and the subtitle never changes the game state (script flow, timing). Needs assets/raw (skipped otherwise).
use snengine::game::{Engine, FILE_NAMES, H, W};
use sncore::i18n::{self as i, Translation, VoiceSub};
use sncore::script::vm::Action;

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
            Some(77) => { seen77 = true; last_active = f; assert!(strip_white(&on) > 60 && hash(&on) != hash(&off), "no text in the strip at frame {f}"); differs |= hash(&on) != hash(&off); }
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
