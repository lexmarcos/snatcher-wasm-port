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

fn glyph_rows(f: &sncore::actions::Font, ch: char) -> ([u8; 8], u8) {
    let idx = (if (' '..='~').contains(&ch) { ch as u8 } else { i::pt_code(ch).unwrap() } - 0x20) as usize;
    (f.glyphs[idx], f.widths[idx])
}
fn ink_rows(g: &[u8; 8]) -> Vec<usize> { (0..8).filter(|&r| g[r] != 0).collect() }

/// The accented capitals sit on the baseline of the original capitals (row 6), keep the width of the base letter, carry the accent in
/// rows 0..1 only and leave row 7 (descender row) and the inter-line row free. Ç is the original C plus a cedilla in row 7.
#[test]
fn accented_capitals_share_the_baseline_of_the_original_capitals() {
    let Some(e) = engine() else { return };
    let f = e.font.as_ref().unwrap();
    for (base, accented) in [('A', "ÁÀÂÃ"), ('E', "ÉÊ"), ('I', "Í"), ('O', "ÓÔÕ"), ('U', "ÚÜ")] {
        let (b, bw) = glyph_rows(f, base);
        assert_eq!(*ink_rows(&b).last().unwrap(), 6, "original {base} baseline");
        for ch in accented.chars() {
            let (g, w) = glyph_rows(f, ch);
            let rows = ink_rows(&g);
            assert_eq!(*rows.last().unwrap(), 6, "{ch}: baseline must be row 6 like {base}");
            assert_eq!(w, bw, "{ch}: advance of {base}");
            assert!(g[0] != 0, "{ch}: accent reaches row 0 (same top as an original capital)");
            assert_eq!(g[7], 0, "{ch}: row 7 is the descender row");
            assert!(g[2] != 0, "{ch}: body starts in row 2");
            // the body (rows 2..6) is the same height for every accented capital, so they match each other
            assert!(rows.iter().all(|&r| r <= 6));
        }
    }
    let (c, cw) = glyph_rows(f, 'C');
    let (cc, ccw) = glyph_rows(f, 'Ç');
    assert_eq!(cc[..7], c[..7], "Ç = original C rows 0..6");
    assert_eq!(ccw, cw);
    assert!(cc[7] != 0 && cc[7].count_ones() <= 2, "cedilla in row 7");
    // every pt-BR capital is different from the others and from the base letter
    let caps: Vec<[u8; 8]> = "ÁÀÂÃÉÊÍÓÔÕÚÜÇ".chars().map(|c| glyph_rows(f, c).0).collect();
    for a in 0..caps.len() { for b in a + 1..caps.len() { assert_ne!(caps[a], caps[b]); } }
}

/// No text window clips a pt-BR glyph and none leaks into the inter-line row: every glyph is drawn at every line position of the
/// 256x64 window (message box lines y = 0, 9 .. 54, menu lines y = 9 * i) and all its pixels stay inside rows y..y+7.
#[test]
fn pt_glyphs_stay_inside_their_line_cell_in_every_window_position() {
    use sncore::actions::{TextWindow, WIN_W, WIN_H};
    let Some(e) = engine() else { return };
    let f = e.font.as_ref().unwrap();
    for (k, g) in i::PT_GLYPHS.iter().enumerate() {
        let code = i::PT_BASE + k as u8;
        let ink: u32 = f.glyphs[(code - 0x20) as usize].iter().map(|r| r.count_ones()).sum();
        for y in (0..=54u8).step_by(9) {
            for x in [0x10u8, 0x18, 0x28, 0xE0] {
                let mut w = TextWindow::new();
                w.clear();
                w.start(&[0xFC, 0x00, 0xF9, 0x01, 0xFB, x, y, 0xFE, 0x01, code, 0xFF]);
                for _ in 0..4 { w.tick(f); }
                let (mut n, mut outside) = (0u32, 0u32);
                for py in 0..WIN_H { for px in 0..WIN_W {
                    // the clear fills the last tile (248.., 56..) with colour 7; ignore it
                    if px >= 248 && py >= 56 { continue; }
                    if w.pix[py * WIN_W + px] == 1 {
                        n += 1;
                        if py < y as usize || py > y as usize + 7 { outside += 1; }
                    }
                } }
                assert_eq!(n, ink, "{} clipped at x={x} y={y}", g.ch);
                assert_eq!(outside, 0, "{} leaks outside rows {y}..{} (x={x})", g.ch, y + 7);
            }
        }
    }
}
