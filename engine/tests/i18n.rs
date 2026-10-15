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

// ---------------------------------------------------------------- pt-BR dialogue layout (10 px line pitch, full-height accented capitals)

use sncore::actions::{Font, MessageBox, MsgEvent, TextWindow, WIN_H, WIN_W};

const ACC: usize = i::TALL_ACCENT_ROWS as usize;

/// Window pixels of colour 1 as (x, y) (the corner tile of `txt_clear` is colour 7).
fn ink(w: &TextWindow) -> Vec<(usize, usize)> {
    (0..WIN_H).flat_map(|y| (0..WIN_W).map(move |x| (x, y))).filter(|&(x, y)| w.pix[y * WIN_W + x] == 1).collect()
}

fn run_stream(f: &Font, tall: bool, x: u8, y: u8, s: &[u8]) -> TextWindow {
    let mut w = TextWindow::new();
    let mut st = vec![0xFC, 0x00, 0xF9, 0x01, 0xFB, x, y, 0xFE, 0x01];
    st.extend_from_slice(s);
    st.push(0xFF);
    w.start_with(&st, tall);
    for _ in 0..8 { w.tick(f); }
    assert!(!w.busy);
    w
}

/// Pages of a message as the engine builds them: (page window, lines on the page) for a SAY speaker value `spk` (0 none, 1 narration, >= 2 named).
fn pages_of(f: &Font, tall: bool, spk: u16, text: &str) -> Vec<TextWindow> {
    let mut raw = i::markup_to_bytes(text).unwrap();
    raw.push(i::END);
    let names = vec![(3u8, b"Name".to_vec()); 4];
    let mut m = MessageBox::new(spk, 0, 0).tall(tall);
    let mut win = TextWindow::new();
    let mut pages = vec![];
    for _ in 0..20000 {
        if win.busy { win.set_fast(); win.tick(f); continue; }
        let confirm = m.state == 5;
        if confirm { let mut w = TextWindow::new(); w.pix = win.pix.clone(); pages.push(w); }
        if m.tick(&mut win, &raw, &names, confirm) == MsgEvent::Done { break; }
    }
    pages
}

/// First text line Y, line pitch and lines per page of every dialogue window type (the numbers of docs/i18n.md "Dialogue and menu layouts").
#[test]
fn dialogue_line_positions_per_window_type() {
    assert_eq!(i::TALL_PITCH, 10);
    // (speaker value, first line y, lines per page) in the pt-BR layout and in the original one
    for (spk, tall_y, tall_n, en_y, en_n) in [(2u8, 9 + ACC as u32, 5usize, 9u32, 6usize), (3, 9 + ACC as u32, 5, 9, 6), (1, ACC as u32, 6, 0, 7), (0, ACC as u32, 6, 0, 7)] {
        assert_eq!(i::first_line_y(spk as u32, true), tall_y, "spk {spk}");
        assert_eq!(i::lines_per_page(spk as u32, true), tall_n, "spk {spk}");
        assert_eq!(i::first_line_y(spk as u32, false), en_y);
        assert_eq!(i::lines_per_page(spk as u32, false), en_n);
    }
    let Some(e) = engine() else { return };
    let f = e.font.as_ref().unwrap();
    // 13 lines "ÉgÉg" (accent row + descender row used on every line) per window type: where does each line draw, and where does the page end?
    let text = (0..13).map(|k| format!("ÉgÁ{k}")).collect::<Vec<_>>().join("{nl}");
    for (spk, tall) in [(3u16, true), (1, true), (2, true), (0, true), (3, false), (1, false), (0, false)] {
        let pages = pages_of(f, tall, spk, &text);
        let speaker = spk as u32;
        let (first, n, pitch) = (i::first_line_y(speaker, tall) as usize, i::lines_per_page(speaker, tall), if tall { 10 } else { 9 });
        assert!(pages.len() >= 2 && pages.len() == (13 + n - 1) / n, "spk {spk} tall {tall}: {} pages", pages.len());
        // every ink pixel of page 1 (below the name line) lies in the cell of one of the `n` lines: rows y0 - accent rows .. y0 + 7
        let below_name = if speaker >= 2 { 9 } else { 0 };
        let px: Vec<(usize, usize)> = ink(&pages[0]).into_iter().filter(|p| p.1 >= below_name).collect();
        let acc = if tall { ACC } else { 0 };
        let cell = |k: usize| (first + pitch * k - acc, first + pitch * k + 7);
        let mut lines = vec![];
        for k in 0..n {
            let (lo, hi) = cell(k);
            let rows: Vec<usize> = px.iter().map(|p| p.1).filter(|&y| y >= lo && y <= hi).collect();
            assert!(!rows.is_empty(), "spk {spk} tall {tall}: line {k} is empty");
            lines.push((*rows.iter().min().unwrap(), *rows.iter().max().unwrap()));
        }
        assert_eq!(px.iter().filter(|p| !(0..n).any(|k| p.1 >= cell(k).0 && p.1 <= cell(k).1)).count(), 0, "spk {spk} tall {tall}: ink outside the line cells");
        for (k, (top, bottom)) in lines.iter().enumerate() {
            let y0 = first + pitch * k;
            assert_eq!(*bottom, y0 + 7, "spk {spk} tall {tall} line {k}: last row (descender of g)");
            // 10 px layout: the accent of the capital sits in the `ACC` rows above the glyph cell; 9 px: the compact accented capital starts at row 0
            assert_eq!(*top, y0 - acc, "spk {spk} tall {tall} line {k}: first row");
        }
        assert!(lines.last().unwrap().1 < WIN_H);
        // the window really holds `n` lines: a further line would need rows beyond the bitmap or the next page
        assert!(first + pitch * n + 7 >= WIN_H, "spk {spk} tall {tall}: a line {n} would still fit");
    }
}

/// Accented capitals in the dialogue windows are the glyph of the original capital, row for row (rows 0..7 of the cell), plus the accent in the
/// `TALL_ACCENT_ROWS` rows above and, for Ç, the cedilla in row 7. The compact forms (menus, English never uses them) are drawn by `start` and are
/// not changed by the dialogue layout.
#[test]
fn tall_accented_capitals_are_the_original_capitals_plus_accent_rows() {
    let Some(e) = engine() else { return };
    let f = e.font.as_ref().unwrap();
    let y0 = 20usize;
    let bitmap = |w: &TextWindow, x: usize, y: usize| -> u8 { (0..8).fold(0u8, |a, k| if w.pix[y * WIN_W + x + k] == 1 { a | 0x80 >> k } else { a }) };
    for (base, accented) in [('A', "ÁÀÂÃ"), ('E', "ÉÊ"), ('I', "Í"), ('O', "ÓÔÕ"), ('U', "ÚÜ"), ('C', "Ç")] {
        let (b, bw) = glyph_rows(f, base);
        let mut shapes: Vec<Vec<u8>> = vec![];
        for ch in accented.chars() {
            let code = i::pt_code(ch).unwrap();
            let w = run_stream(f, true, 0x18, y0 as u8, &[code]);
            let (g, gw) = glyph_rows(f, ch);
            assert_eq!(gw, bw, "{ch}: advance of {base}");
            // rows 0..6 of the cell = the original capital, untouched
            for r in 0..7 { assert_eq!(bitmap(&w, 0x18, y0 + r), b[r], "{ch}: row {r} of the original {base}"); }
            if ch == 'Ç' {
                assert_eq!(b[7], 0);
                assert_ne!(bitmap(&w, 0x18, y0 + 7), 0, "Ç: cedilla in row 7");
                assert_eq!(bitmap(&w, 0x18, y0 + 7).count_ones(), 1);
                assert!((y0 - ACC..y0).all(|y| bitmap(&w, 0x18, y) == 0), "Ç has no accent");
            } else {
                assert_eq!(bitmap(&w, 0x18, y0 + 7), 0, "{ch}: row 7 stays empty");
                let acc: Vec<u8> = (y0 - ACC..y0).map(|y| bitmap(&w, 0x18, y)).collect();
                assert!(acc.iter().any(|&r| r != 0), "{ch}: accent in the {ACC} rows above the cell");
                shapes.push(acc);
            }
            // nothing above the accent rows, nothing below row 7, nothing right of the advance + 1 px
            assert!(ink(&w).iter().all(|&(x, y)| y >= y0 - ACC && y <= y0 + 7 && x >= 0x18 && x < 0x18 + bw as usize + 1), "{ch}: stray pixels");
            // the compact form of the same glyph is what `start` draws (menus): 8 rows, accent in rows 0..1, the cell never exceeded
            let c = run_stream(f, false, 0x18, y0 as u8, &[code]);
            for r in 0..8 { assert_eq!(bitmap(&c, 0x18, y0 + r), g[r], "{ch}: compact row {r}"); }
            assert!(ink(&c).iter().all(|&(_, y)| y >= y0 && y <= y0 + 7));
        }
        for a in 0..shapes.len() { for b2 in a + 1..shapes.len() { assert_ne!(shapes[a], shapes[b2], "two {base} accents look the same"); } }
    }
}

/// No pt-BR glyph, in any form, draws a pixel outside the cell of its dialogue line: `TALL_ACCENT_ROWS` above .. 7 below the glyph Y, for every
/// line position of every dialogue window type (and nothing is clipped).
#[test]
fn no_glyph_pixel_leaves_its_dialogue_line_cell() {
    let Some(e) = engine() else { return };
    let f = e.font.as_ref().unwrap();
    let mut positions: Vec<u8> = vec![];
    for spk in [3u32, 1, 0] {
        for k in 0..i::lines_per_page(spk, true) as u32 { positions.push((i::first_line_y(spk, true) + i::TALL_PITCH * k) as u8); }
    }
    positions.sort(); positions.dedup();
    let mut glyphs: Vec<u8> = (i::PT_BASE..i::PT_BASE + i::PT_GLYPHS.len() as u8).collect();
    glyphs.extend((0x20u8..0x7F).filter(|c| c.is_ascii_graphic()));
    for &code in &glyphs {
        let expect = run_stream(f, false, 0x18, 20, &[code]); // reference ink count: the glyph is not clipped at y=20
        let n0 = ink(&expect).len();
        for &y in &positions {
            let w = run_stream(f, true, 0xE0, y, &[code]);
            let px = ink(&w);
            assert!(px.iter().all(|&(_, py)| py + ACC >= y as usize && py <= y as usize + 7), "glyph {code:#x} leaves the cell at y={y}");
            assert!(px.len() >= n0, "glyph {code:#x} clipped at y={y}"); // accented capitals gain the accent rows, nothing is lost
            assert!(y as usize + 7 < WIN_H);
        }
    }
}

// ---------------------------------------------------------------- pt-BR player path

/// Pack of the real translation files of the given scripts (`i18n/pt-BR/SPxx.json`, one entry per line) and `names.json`; no JSON library needed.
fn real_pack(sps: &[u8]) -> Option<Vec<u8>> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../i18n/pt-BR");
    let unescape = |s: &str| -> String {
        let (mut o, mut it) = (String::new(), s.chars());
        while let Some(c) = it.next() { if c == '\\' { match it.next() { Some('n') => o.push('\n'), Some(x) => o.push(x), None => {} } } else { o.push(c); } }
        o
    };
    let mut t = Translation::default();
    for &sp in sps {
        let txt = std::fs::read_to_string(dir.join(format!("SP{sp:02}.json"))).ok()?;
        for l in txt.lines() {
            let l = l.trim();
            let (Some(k), Some(p)) = (l.strip_prefix('"').and_then(|r| r.get(..4)), l.find("\"t\": \"")) else { continue };
            let Ok(off) = u16::from_str_radix(k, 16) else { continue };
            let body = &l[p + 6..];
            let body = body.trim_end_matches(',').strip_suffix("\"}")?;
            t.files.entry(sp).or_default().insert(off, i::markup_to_bytes(&unescape(body)).ok()?);
        }
    }
    for l in std::fs::read_to_string(dir.join("names.json")).ok()?.lines() {
        let l = l.trim().trim_end_matches(',');
        if let Some((k, v)) = l.split_once("\": \"") {
            if let (Ok(n), Some(v)) = (k.trim_start_matches('"').parse::<u8>(), v.strip_suffix('"')) { t.names.insert(n - 1, i::markup_to_bytes(&unescape(v)).ok()?); }
        }
    }
    Some(t.build())
}

/// From the power-on screen to the first translated dialogue, with the pad only (START through the title and the intro, then A / C), in pt-BR with
/// the real translation of SP06: the first message box that shows translated text uses the 10 px layout, draws full-height capitals where the text
/// has them and no pixel leaves its line cell; an untranslated string keeps the original 9 px layout.
#[test]
fn pt_player_path_from_the_title_to_the_first_dialogue() {
    let Some(mut e) = engine() else { return };
    let Some(pack) = real_pack(&[6]) else { eprintln!("translation files missing, skipped"); return };
    e.load_translation(&pack).unwrap();
    e.set_language(true);
    e.boot();
    let (mut seen, mut tall_frames) = (None, 0);
    for f in 0..30000u64 {
        let pad = if e.top != snengine::game::Top::Game { if f % 60 == 30 { 0x80 } else { 0 } } else if f % 40 == 0 { 0x20 } else { 0 };
        e.tick(pad);
        e.out.clear();
        if let Some(m) = e.msg.as_ref() {
            if m.state == 5 && !e.text.busy && m.tall && m.speaker >= 1 {
                tall_frames += 1;
                if tall_frames == 3 { seen = Some((f, m.speaker, e.text.pix.clone())); break; }
            }
        }
    }
    let (f, speaker, pix) = seen.expect("no translated message box reached from the title screen");
    let mut w = TextWindow::new();
    w.pix = pix;
    let first = i::first_line_y(speaker as u32 + 1, true) as usize;
    let lines = i::lines_per_page(speaker as u32 + 1, true);
    let px = ink(&w);
    eprintln!("first translated dialogue: frame {f}, speaker index {speaker}, {} ink pixels", px.len());
    assert!(px.len() > 50, "frame {f}: the dialogue window is empty");
    // the page uses the 10 px cells: every ink row (below the name line) belongs to a cell of one of the lines
    let below_name = if speaker >= 1 { 9 } else { 0 };
    assert!(px.iter().filter(|p| p.1 >= below_name).all(|&(_, y)| (0..lines).any(|k| { let y0 = first + 10 * k; y + ACC >= y0 && y <= y0 + 7 })), "frame {f}: ink outside the 10 px line cells");
    assert!(px.iter().any(|&(_, y)| y >= first - ACC && y < first + 8), "frame {f}: first line missing");
}
