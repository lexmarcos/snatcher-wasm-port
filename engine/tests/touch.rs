//! Touch description of the game (`Engine::touch_ui`, `Engine::touch_step`, engine/src/touch.rs) against the local disc: the rectangles lie on what
//! the screen shows, and a closed-loop "tap" (the same loop the web front end runs in "touch the game" mode) selects the right command, types the
//! right key, picks the right slot. Skipped without assets/raw. Overlay pictures (game art, gitignored) go to work/shots/mobile/engine/.
use snengine::game::{Engine, Top, FILE_NAMES, H, W};
use snengine::touch::TouchUi;
use sncore::actions::MenuResult;

const UP: u8 = 1; const DOWN: u8 = 2; const LEFT: u8 = 4; const RIGHT: u8 = 8;
const B: u8 = 0x10; const C: u8 = 0x20; const START: u8 = 0x80;

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
    if let (Ok(p), Ok(f)) = (std::fs::read_to_string(aw.join("pcm.json")), std::fs::read_to_string(aw.join("fm.json"))) { e.load_audio_manifests(&p, &f); }
    Some(e)
}

/// A press the way the front end makes it: two frames down, two frames up (a press during a redraw is lost, so callers wait for `ready`).
fn press(e: &mut Engine, bits: u8) { e.tick(bits); e.tick(bits); e.tick(0); e.tick(0); }
fn ui(e: &Engine) -> TouchUi { e.touch_ui() }

/// Run (pressing confirm on messages and the first menu item) until `kind` shows with `ok`, at most `max` frames.
fn until(e: &mut Engine, max: u32, ok: impl Fn(&TouchUi) -> bool) -> bool {
    for _ in 0..max {
        let u = ui(e);
        if ok(&u) { return true; }
        if u.kind == "message" && u.ready || u.kind == "menu" && u.ready && !ok(&u) { press(e, C); } else { e.tick(0); }
    }
    false
}

/// Tap menu line `i`: step the cursor with Up/Down while the menu is ready, then confirm.
fn tap_line(e: &mut Engine, i: i32) -> bool {
    for _ in 0..400 {
        let u = ui(e);
        if u.kind != "menu" { return false; }
        if !u.ready { e.tick(0); continue; }
        if u.cur == i { e.tick(C); e.tick(C); e.tick(0); e.tick(0); return true; }
        press(e, if i < u.cur { UP } else { DOWN });
    }
    false
}

fn tap_cell(e: &mut Engine, cell: i32) -> bool {
    for _ in 0..600 {
        let u = ui(e);
        if u.kind != "entry" { return false; }
        if !u.ready { e.tick(0); continue; }
        if u.cur == cell { press(e, C); return true; }
        let d = e.touch_step(cell as u8);
        if d == 0 { return false; }
        press(e, d);
    }
    false
}

fn save_png(e: &Engine, rects: &[snengine::touch::TRect], name: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../work/shots/mobile/engine");
    let _ = std::fs::create_dir_all(&dir);
    let mut px = e.fb.clone();
    for r in rects {
        for x in r.x.max(0)..(r.x + r.w).min(W as i32) { for y in [r.y, r.y + r.h - 1] { if (0..H as i32).contains(&y) { let o = (y as usize * W + x as usize) * 4; px[o..o + 4].copy_from_slice(&[255, 0, 255, 255]); } } }
        for y in r.y.max(0)..(r.y + r.h).min(H as i32) { for x in [r.x, r.x + r.w - 1] { if (0..W as i32).contains(&x) { let o = (y as usize * W + x as usize) * 4; px[o..o + 4].copy_from_slice(&[255, 0, 255, 255]); } } }
    }
    let w = std::io::BufWriter::new(std::fs::File::create(dir.join(name)).unwrap());
    let mut enc = png::Encoder::new(w, W as u32, H as u32);
    enc.set_color(png::ColorType::Rgba); enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&px).unwrap();
}

fn bright(e: &Engine, x0: i32, y0: i32, w: i32, h: i32) -> (u32, f64, f64) {
    let (mut n, mut sx, mut sy) = (0u32, 0f64, 0f64);
    for y in y0.max(0)..(y0 + h).min(H as i32) { for x in x0.max(0)..(x0 + w).min(W as i32) {
        let o = (y as usize * W + x as usize) * 4;
        if e.fb[o] as u32 + e.fb[o + 1] as u32 + e.fb[o + 2] as u32 > 300 { n += 1; sx += x as f64; sy += y as f64; }
    } }
    if n == 0 { (0, 0.0, 0.0) } else { (n, sx / n as f64, sy / n as f64) }
}

#[test]
fn message_then_menu_lines_lie_on_the_text_and_a_tap_selects_that_command() {
    let Some(mut e) = engine() else { return };
    e.new_game();
    assert!(until(&mut e, 6000, |u| u.kind == "menu" && u.ready), "no menu reached");
    for _ in 0..20 { e.tick(0); }
    let u = ui(&e);
    assert_eq!(u.kind, "menu");
    assert!(u.rects.len() >= 3, "{u:?}");
    assert!(!u.back, "the top menu has no Back");
    assert_eq!((u.confirm, u.cancel, u.extra), (C, B, 0x40), "default button configuration");
    for r in &u.rects {
        assert_eq!(r.h, 9);
        let (n, _, cy) = bright(&e, r.x, r.y, r.w, r.h);
        assert!(n > 6, "no text pixels in the rectangle of line {}: {r:?}", r.id);
        assert!((cy - (r.y as f64 + 4.0)).abs() < 4.0, "text of line {} is off its rectangle (centroid y {cy}, rect {r:?})", r.id);
    }
    // nothing between the first line and the picture above it (the text window starts at the first rectangle)
    let first = &u.rects[0];
    assert_eq!(bright(&e, 0, first.y - 6, 256, 5).0, 0, "text above the first rectangle");
    save_png(&e, &u.rects, "menu.png");
    // tap the third line: the game's menu result is item 3
    let n = u.rects.len() as i32;
    let target = n - 1;
    assert!(tap_line(&mut e, target));
    assert_eq!(e.menu_close.as_ref().map(|m| m.0), Some(MenuResult::Selected(target as u8 + 1)), "tapped line {target}");
}

#[test]
fn keypad_keys_lie_on_the_digits_and_a_tap_types_the_digit() {
    let Some(mut e) = engine() else { return };
    e.jump(57 + 35, 0); // SP35: the videophone keypad
    assert!(until(&mut e, 4000, |u| u.kind == "entry" && u.ready && !u.rects.is_empty()), "no keypad");
    for _ in 0..30 { e.tick(0); }
    let u = ui(&e);
    assert_eq!(u.rects.len(), 12, "12 keys");
    assert!(u.back && !u.start, "Back = backspace; the dialling keypad has no OK (it ends by itself)");
    for r in &u.rects {
        let (n, cx, cy) = bright(&e, r.x - 2, r.y - 2, r.w + 4, r.h + 4);
        assert!(n > 4, "no glyph on key {r:?}");
        let (rx, ry) = (r.x as f64 + 8.0, r.y as f64 + 8.0);
        assert!((cx - rx).abs() < 6.0 && (cy - ry).abs() < 6.0, "glyph of key {r:?} is off its centre ({cx},{cy})");
    }
    save_png(&e, &u.rects, "keypad.png");
    // '7' = grid code 0x37
    let grid = 0x114DCu32;
    let cell = (0..48u32).find(|&c| e.mem.u8(grid + c) == b'7').unwrap() as i32;
    assert!(tap_cell(&mut e, cell));
    for _ in 0..30 { e.tick(0); }
    let ent = e.ix.entry.as_ref().expect("entry still open (a single digit does not complete a number)");
    assert_eq!(ent.buf[0], b'7');
}

#[test]
fn keyboard_keys_and_a_tap_types_the_letter() {
    let Some(mut e) = engine() else { return };
    e.jump(57 + 4, 0); // SP04: Jordan terminal; C every 60 frames reaches ENTER NAME
    let mut f = 0;
    loop {
        let u = ui(&e);
        if u.kind == "entry" && u.ready && !u.rects.is_empty() { break; }
        f += 1; assert!(f < 4000, "no keyboard");
        if u.kind == "menu" && u.ready { press(&mut e, if u.cur == 0 && u.rects.len() >= 2 && false { DOWN } else { C }); } else if u.kind == "message" && u.ready { press(&mut e, C); } else { e.tick(0); }
    }
    for _ in 0..30 { e.tick(0); }
    let u = ui(&e);
    assert_eq!(u.rects.len(), 26 + 3 + 3, "26 letters, '-', '.', apostrophe, backspace, space, OK");
    assert!(u.back && u.start && u.label == "ok");
    save_png(&e, &u.rects, "keyboard.png");
    let grid = 0x11512u32;
    for ch in [b'J', b'Q'] {
        let cell = (0..208u32).find(|&c| e.mem.u8(grid + c) == ch).unwrap() as i32;
        assert!(tap_cell(&mut e, cell), "tap {}", ch as char);
        for _ in 0..20 { e.tick(0); }
    }
    let ent = e.ix.entry.as_ref().unwrap();
    assert_eq!(&ent.buf[..2], b"JQ");
    // BACKSPACE key (code 9) removes the last letter
    let bs = (0..208u32).find(|&c| e.mem.u8(grid + c) == 9).unwrap() as i32;
    assert!(tap_cell(&mut e, bs));
    for _ in 0..40 { e.tick(0); }
    let ent = e.ix.entry.as_ref().unwrap();
    assert_eq!(ent.buf[0], b'J');
    assert_eq!(ent.buf[1], 0);
}

#[test]
fn title_slots_options_states() {
    let Some(mut e) = engine() else { return };
    let mut blk = vec![0u8; 0x1C0];
    blk[0] = 3; blk[2..6].copy_from_slice(&[1, 0x23, 0x45, 0x10]); blk[0x0A] = 0x05; blk[0x0C..0x0E].copy_from_slice(&0x3Fu16.to_be_bytes());
    e.saves[0] = Some(blk.clone());
    e.saves[2] = Some(blk);
    e.boot();
    let mut n = 0;
    while ui(&e).kind != "title" { e.tick(0); n += 1; assert!(n < 3000, "title never shown"); }
    for _ in 0..40 { e.tick(0); }
    let u = ui(&e);
    assert_eq!((u.kind, u.rects.len(), u.cur), ("title", 2, 1), "CONTINUE is the default with saves: {u:?}");
    assert!(u.start);
    save_png(&e, &u.rects, "title.png");
    // tap NEW GAME: Up, wait for the cursor, Start -> the options screen
    press(&mut e, UP);
    let mut k = 0; while ui(&e).cur != 0 { e.tick(0); k += 1; assert!(k < 60, "cursor did not move"); }
    press(&mut e, START);
    k = 0; while ui(&e).kind != "options" { e.tick(0); k += 1; assert!(k < 600, "options never shown: {:?}", ui(&e)); }
    for _ in 0..10 { e.tick(0); }
    let o = ui(&e);
    assert_eq!(o.rects.len(), 4, "GUN ADJUST is hidden without the gun device");
    assert_eq!(o.cur, 0);
    save_png(&e, &o.rects, "options.png");
    // a separate engine for the slot screen: CONTINUE then the slot rectangles
    let mut e = engine().unwrap();
    let mut blk = vec![0u8; 0x1C0];
    blk[0] = 3; blk[2..6].copy_from_slice(&[1, 0x23, 0x45, 0x10]); blk[0x0A] = 0x05; blk[0x0C..0x0E].copy_from_slice(&0x3Fu16.to_be_bytes());
    e.saves[0] = Some(blk.clone()); e.saves[2] = Some(blk);
    e.boot();
    n = 0; while ui(&e).kind != "title" { e.tick(0); n += 1; assert!(n < 3000); }
    for _ in 0..40 { e.tick(0); }
    press(&mut e, START); // CONTINUE
    n = 0; while ui(&e).kind != "slots" { e.tick(0); n += 1; assert!(n < 900, "slots never shown: {:?}", ui(&e)); }
    for _ in 0..10 { e.tick(0); }
    let s = ui(&e);
    assert_eq!(s.rects.iter().map(|r| r.id).collect::<Vec<_>>(), vec![0, 2], "only the slots with data are tappable");
    assert!(s.back);
    save_png(&e, &s.rects, "slots.png");
    // slot 3 (index 2): Down moves to the next slot with data
    press(&mut e, DOWN);
    k = 0; while ui(&e).cur != 2 { e.tick(0); k += 1; assert!(k < 60); }
    press(&mut e, START);
    k = 0; while e.top != Top::Game && ui(&e).kind != "options" { e.tick(0); k += 1; assert!(k < 900); }
    assert_eq!(e.regs.get_b(0x78B7), 2, "slot 3 was chosen");
    let _ = (LEFT, RIGHT);
}

#[test]
fn shooting_stage_is_a_shoot_kind_and_drawing_shows() {
    let Some(mut e) = engine() else { return };
    // SP01 debug menu: SHOOTING group, first stage (the same route as engine/examples/shoot_trace.rs: group 0 item 0)
    e.jump(57 + 1, 0);
    let mut f = 0u32;
    let mut stage = 0;
    while ui(&e).kind != "shoot" {
        f += 1; assert!(f < 9000, "never reached the shooting stage, last {:?}", ui(&e));
        let u = ui(&e);
        if u.kind == "menu" && u.ready {
            // debug menu: pick the entry called SHOOTING at depth 0, then the first items
            let items = e.menu.as_ref().map(|(_, it)| it.clone()).unwrap_or_default();
            let names: Vec<String> = items.iter().map(|&o| e.script.get(0x3800 + o as usize..).unwrap_or(&[]).iter().take_while(|&&c| c != 0xFF).filter(|c| (0x20..0x7F).contains(*c)).map(|&c| c as char).collect()).collect();
            let want = if stage == 0 { names.iter().position(|n| n.trim() == "SHOOTING").unwrap_or(0) } else { 0 };
            if stage == 0 { stage = 1; }
            if !tap_line(&mut e, want as i32) { e.tick(0); }
        } else if u.kind == "message" && u.ready { press(&mut e, C); } else { e.tick(0); }
    }
    let u = ui(&e);
    assert_eq!(u.kind, "shoot");
    assert!(!u.drawn);
}

#[test]
fn touch_roles_follow_the_button_configuration_of_the_options_screen() {
    let Some(mut e) = engine() else { return };
    // every configuration $78BA 0..5 maps the physical A B C to the logical extra / cancel / confirm differently: the touch buttons must give the logical ones
    for cfg in 0..6usize {
        e.input.as_mut().unwrap().config = cfg;
        let (confirm, cancel, extra) = e.touch_roles();
        let mut seen = std::collections::HashSet::new();
        for (phys, logical) in [(confirm, 0x20u8), (cancel, 0x10), (extra, 0x40)] {
            assert!(phys == 0x10 || phys == 0x20 || phys == 0x40, "config {cfg}: physical bit {phys:#x}");
            assert!(seen.insert(phys), "config {cfg}: two roles on one button");
            let i = e.input.as_mut().unwrap();
            i.update(0); i.update(phys);
            assert_eq!(i.pressed & 0x70, logical, "config {cfg}: pressing {phys:#x} must give logical {logical:#x}");
            i.update(0);
        }
    }
}
