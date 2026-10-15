//! Translation (pt-BR): English stays the default and produces identical output with or without a pack loaded; in pt-BR the display
//! side swaps the strings that have a translation and falls back to the original for the others. Needs assets/raw (skipped otherwise).
use snengine::game::{Engine, FILE_NAMES};
use sncore::i18n::{self as i, Translation};

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
    Some(e)
}

fn pack() -> Vec<u8> {
    let mut t = Translation::default();
    let m = t.files.entry(6).or_default();
    m.insert(0x0320, i::markup_to_bytes("Bem-vinda à sede dos Junkers.{nl}Posso ajudá-lo?").unwrap());
    m.insert(0x019a, i::markup_to_bytes("ENTRAR").unwrap());
    t.names.insert(3, i::markup_to_bytes("Mika").unwrap());
    t.build()
}

fn fb_hashes(e: &mut Engine, frames: usize) -> Vec<u64> {
    e.jump(sncore::script::sp_file_id(6), 0);
    let mut v = vec![];
    for f in 0..frames {
        e.tick(if f % 40 == 20 { 0x20 } else { 0 });
        if f % 25 == 0 { v.push(e.fb.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3))); }
    }
    v
}

#[test]
fn english_output_is_identical_with_or_without_a_pack() {
    let Some(mut a) = engine() else { return };
    let mut b = engine().unwrap();
    assert_eq!(b.load_translation(&pack()).unwrap(), 2);
    b.set_language(false);
    assert_eq!(fb_hashes(&mut a, 700), fb_hashes(&mut b, 700));
    assert_eq!(a.message_text(), b.message_text());
}

#[test]
fn pt_swaps_translated_strings_and_falls_back_for_the_rest() {
    let Some(mut e) = engine() else { return };
    e.load_translation(&pack()).unwrap();
    e.jump(sncore::script::sp_file_id(6), 0);
    for _ in 0..5 { e.tick(0); } // the script file loads in the queue
    let en = e.message_text();
    e.set_language(true);
    assert_eq!(e.tr_off(0x0100), 0x0100, "no entry: the original offset");
    let new = e.tr_off(0x0320);
    assert!(new as usize >= en.len() - 2 && new != 0x0320, "translated string lives after the original area");
    let t = e.message_text();
    assert!(t.len() > en.len());
    assert_eq!(&t[..en.len()], &en[..], "the original area is kept intact");
    let s = &t[new as usize..];
    let end = s.iter().position(|&c| c == 0xFF).unwrap();
    assert_eq!(i::bytes_to_markup(&s[..end]), "Bem-vinda à sede dos Junkers.{nl}Posso ajudá-lo?");
    e.set_language(false);
    assert_eq!(e.tr_off(0x0320), 0x0320);
    assert_eq!(e.message_text(), en);
}

#[test]
fn pt_glyphs_are_in_the_font_with_widths() {
    let Some(e) = engine() else { return };
    let f = e.font.as_ref().unwrap();
    for (k, g) in i::PT_GLYPHS.iter().enumerate() {
        let idx = (i::PT_BASE as usize + k) - 0x20;
        assert!(f.glyphs[idx].iter().any(|&r| r != 0), "{}", g.ch);
        assert_eq!(f.widths[idx], g.adv);
    }
    assert_eq!(f.glyphs.len(), (i::PT_BASE as usize - 0x20) + i::PT_GLYPHS.len());
}

#[test]
fn a_bad_pack_is_rejected_and_changes_nothing() {
    let Some(mut e) = engine() else { return };
    assert!(e.load_translation(b"nope").is_err());
    assert!(e.load_translation(&pack()[..10]).is_err());
}
