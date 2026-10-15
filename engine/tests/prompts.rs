//! Button prompts (controllers): the text placeholders EC/EE/F0 and the word START print the original letters in the
//! default `Genesis` style (byte-identical to the engine before this feature) and the icon of the physical control
//! in the other styles. Needs the user's extracted assets (assets/raw); skipped when they are absent.
use snengine::game::{Engine, FILE_NAMES};
use sncore::prompt::{self, Style};

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

/// The engine's behaviour before the feature: only the placeholders were replaced, by the letter of `$78BA`.
fn legacy(e: &Engine, raw: &[u8]) -> Vec<u8> {
    let i = e.input.as_ref().unwrap();
    raw.iter().map(|&c| if matches!(c, 0xEC | 0xEE | 0xF0) { i.button_letter(c) } else { c }).collect()
}

#[test]
fn genesis_style_is_byte_identical_and_icon_styles_change_only_prompts() {
    let Some(mut e) = engine() else { return };
    let mut checked = 0;
    for n in (6..=9).chain([26, 29, 30]) {
        let Some(d) = e.files.get(&format!("SP{n:02}.BIN")).cloned() else { continue };
        e.script = d;
        let raw = e.script.get(0x3800..).unwrap().to_vec();
        let placeholders = raw.iter().filter(|c| matches!(c, 0xEC | 0xEE | 0xF0)).count();
        let starts = raw.windows(7).filter(|w| w[0] == 0xFB && &w[1..6] == b"START" && w[6] == 0xF9).count();
        for cfg in 0..6 {
            e.input.as_mut().unwrap().config = cfg;
            e.prompt = Default::default();
            assert_eq!(e.message_text(), legacy(&e, &raw), "SP{n:02} cfg {cfg}: genesis output changed");
            for (style, ctl) in [(1u8, [2u8, 1, 0, 8]), (2, [2, 1, 0, 8]), (3, [2, 1, 0, 8]), (4, [0, 1, 2, 3])] {
                e.prompt.set(style, ctl[0], ctl[1], ctl[2], ctl[3]);
                let t = e.message_text();
                assert_eq!(t.len(), raw.len());
                let base = legacy(&e, &raw);
                let diff = (0..t.len()).filter(|&i| t[i] != base[i]).count();
                assert_eq!(diff, placeholders + starts * 5, "SP{n:02} style {style}: only prompts may change");
                assert!(t.iter().zip(&base).all(|(a, b)| a == b || *a >= prompt::ICON_BASE && *a < prompt::ICON_BASE + prompt::ICON_COUNT as u8));
            }
        }
        checked += placeholders + starts;
    }
    assert!(checked >= 15, "expected the known placeholders in SP06/07/09/26, found {checked}");
}

#[test]
fn each_style_prints_the_icon_of_the_control_behind_the_genesis_letter() {
    let Some(mut e) = engine() else { return };
    e.script = e.files.get("SP07.BIN").cloned().unwrap();
    let raw = e.script.get(0x3800..).unwrap().to_vec();
    let pos = raw.iter().position(|&c| c == 0xEC).expect("SP07 has {EC}");
    // Genesis A/B/C -> which control; $78BA decides which letter the EC function is
    for cfg in 0..6usize {
        e.input.as_mut().unwrap().config = cfg;
        let letter = e.input.as_ref().unwrap().button_letter(0xEC);
        let idx = match letter { b'A' => 0, b'B' => 1, _ => 2 };
        let mut ctl = [prompt::CTL_WEST, prompt::CTL_EAST, prompt::CTL_SOUTH, prompt::CTL_START];
        ctl.swap(0, 2); // pretend a remap: A on south, C on west
        for (style, base) in [(Style::Xbox, 0u8), (Style::PlayStation, 9), (Style::Switch, 18)] {
            e.prompt.set(style as u8, ctl[0], ctl[1], ctl[2], ctl[3]);
            assert_eq!(e.message_text()[pos], prompt::ICON_BASE + base + ctl[idx], "cfg {cfg} style {style:?}");
        }
        e.prompt.set(Style::Keyboard as u8, 0, 1, 2, 3);
        assert_eq!(e.message_text()[pos], 0xA1 + idx as u8);
    }
}

#[test]
fn icon_glyphs_render_in_the_text_window_and_originals_are_untouched() {
    let Some(e) = engine() else { return };
    let font = e.font.as_ref().expect("font from SUBCODE");
    assert_eq!(font.glyphs.len(), sncore::actions::GLYPH_COUNT + prompt::ICON_COUNT);
    // the first 102 glyphs are the game's own: 'A' and the arrow 0x80 as documented
    assert_eq!(font.glyphs[(b'A' - 0x20) as usize][4], 0b0111_1110);
    let mut w = sncore::actions::TextWindow::new();
    w.start(&[0xFC, 0, 0xF9, 1, 0xFE, 1, 0xFB, 0, 0, prompt::ICON_BASE, prompt::ICON_BASE + 9, 0xFF]);
    while w.busy { w.tick(font); }
    let lit = |x0: usize, x1: usize| (0..8).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| w.pix[y * 256 + x] == 1).count();
    assert!(lit(0, 9) > 20 && lit(9, 18) > 10, "both icons draw pixels (advance 9 each)");
    assert_eq!(lit(18, 40), 0);
}
