//! pt-BR pictures (docs/i18n-graphics.md): English output stays byte-identical with or without the pack, in pt-BR the redrawn pictures are decoded
//! from the patched blobs (title menu, act card, videophone banner, GAME OVER). Needs the user's assets and the built pack (`scripts/build-web.sh`
//! or `snt i18n-build i18n/pt-BR i18n/build/pt-BR.pack`); skipped when either is absent.
use snengine::game::{Engine, FILE_NAMES};

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

fn pack() -> Option<Vec<u8>> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../i18n/build/pt-BR.pack")).ok()
}

fn title_frame(pt: Option<&[u8]>, lang_pt: bool) -> Option<Vec<u8>> {
    let mut e = engine()?;
    if let Some(p) = pt { e.load_translation(p).unwrap(); }
    e.set_language(lang_pt);
    e.mem.trace = Some(Default::default());
    e.boot();
    for _ in 0..1200 { e.tick(0); e.out.clear(); }
    Some(e.fb.clone())
}

#[test]
fn english_pictures_are_identical_with_or_without_the_pack() {
    let Some(p) = pack() else { eprintln!("pack missing, skipped"); return };
    let (Some(a), Some(b)) = (title_frame(None, false), title_frame(Some(&p), false)) else { return };
    assert_eq!(a, b, "English title picture must not change when the pt-BR pack is loaded");
}

#[test]
fn pt_br_title_menu_uses_the_redrawn_blob() {
    let Some(p) = pack() else { eprintln!("pack missing, skipped"); return };
    let (Some(en), Some(pt)) = (title_frame(None, false), title_frame(Some(&p), true)) else { return };
    assert_ne!(en, pt, "the pt-BR title shows NOVO JOGO instead of NEW GAME");
    // only the menu line differs: rows outside the menu (the SNATCHER logo) are identical
    let w = snengine::game::W; 
    let row = |f: &Vec<u8>, y: usize| f[y * w * 4..(y + 1) * w * 4].to_vec();
    for y in 0..120 { assert_eq!(row(&en, y), row(&pt, y), "row {y} above the menu changed"); }
    assert!((150..180).any(|y| row(&en, y) != row(&pt, y)));
}

#[test]
fn pt_br_scene_blobs_are_patched_only_in_pt_br() {
    let Some(p) = pack() else { eprintln!("pack missing, skipped"); return };
    let Some(mut e) = engine() else { return };
    e.load_translation(&p).unwrap();
    for pt in [false, true] {
        e.set_language(pt);
        e.mem.trace = Some(Default::default());
        // the first title scene: blob at S:0x2D8AC holds the menu words
        e.boot();
        for _ in 0..1100 { e.tick(0); e.out.clear(); }
        let patched = e.mem.trace.as_ref().unwrap().borrow().iter().filter(|t| matches!(t, sncore::gfx::TraceEv::Blob { src: 0x2d8ac, patched: true, .. })).count();
        assert_eq!(patched > 0, pt, "blob patched = {patched} for pt = {pt}");
    }
}
