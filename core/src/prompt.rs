//! Button prompts: icon glyphs for the text placeholders EC/EE/F0 ("draw blaster" / "light" / "fire")
//! and for the word START, plus the two-layer resolution used by the engine (docs/re/web.md section "Button prompts").
//!
//! Layer 1 (the game): the pad configuration `$78BA` maps a function (EC/EE/F0) to a Genesis letter A/B/C
//! (`actions::Input::button_letter`). Layer 2 (the front end): a Genesis letter is produced by a physical control
//! of the active device; the front end tells the engine which one through `Prompt::set`. The icon drawn is the
//! glyph of that physical control in the style of the active device.
//!
//! The glyphs are our own 1 bpp pixel art (generic shapes, no logos). They live in a separate table that is
//! appended after the original 102 font glyphs (codes 0x86..): the original glyphs are never changed.

/// First code of the icon glyphs (the original font ends with 0x85).
pub const ICON_BASE: u8 = 0x86;
/// Controls of a gamepad family that can be shown: ids 0..8.
pub const PAD_CONTROLS: usize = 9;
pub const CTL_SOUTH: u8 = 0;
pub const CTL_EAST: u8 = 1;
pub const CTL_WEST: u8 = 2;
pub const CTL_NORTH: u8 = 3;
/// Left / right bumper (LB RB / L1 R1 / L R) and trigger (LT RT / L2 R2 / ZL ZR).
pub const CTL_L1: u8 = 4;
pub const CTL_R1: u8 = 5;
pub const CTL_L2: u8 = 6;
pub const CTL_R2: u8 = 7;
/// Menu (Xbox) / Options (PlayStation) / + (Switch).
pub const CTL_START: u8 = 8;
/// Keyboard keys shown by the keyboard style (ids 0..3).
pub const KEY_Z: u8 = 0;
pub const KEY_X: u8 = 1;
pub const KEY_C: u8 = 2;
pub const KEY_ENTER: u8 = 3;
/// Zero-width filler (used to keep byte offsets when a word is replaced by an icon).
pub const ICON_BLANK: u8 = ICON_BASE + 3 * PAD_CONTROLS as u8 + 4; // 0xA5
pub const ICON_COUNT: usize = 3 * PAD_CONTROLS + 4 + 1; // 32

/// Which icon set the game shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style { Genesis = 0, Xbox = 1, PlayStation = 2, Switch = 3, Keyboard = 4 }

impl Style {
    pub fn from_u8(v: u8) -> Style {
        match v { 1 => Style::Xbox, 2 => Style::PlayStation, 3 => Style::Switch, 4 => Style::Keyboard, _ => Style::Genesis }
    }
}

/// Active prompt style and the physical control behind each Genesis button (A, B, C, START).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prompt { pub style: Style, pub ctl: [u8; 4] }

impl Default for Prompt { fn default() -> Self { Prompt { style: Style::Genesis, ctl: [CTL_WEST, CTL_EAST, CTL_SOUTH, CTL_START] } } }

impl Prompt {
    pub fn set(&mut self, style: u8, a: u8, b: u8, c: u8, start: u8) { self.style = Style::from_u8(style); self.ctl = [a, b, c, start]; }
    /// Glyph code of a physical control in the active style (None = style `Genesis` or id out of range).
    pub fn glyph(&self, ctl: u8) -> Option<u8> {
        let fam = match self.style {
            Style::Genesis => return None,
            Style::Xbox => 0, Style::PlayStation => 1, Style::Switch => 2,
            Style::Keyboard => return if ctl <= KEY_ENTER { Some(ICON_BASE + 3 * PAD_CONTROLS as u8 + ctl) } else { None },
        };
        if (ctl as usize) < PAD_CONTROLS { Some(ICON_BASE + (fam * PAD_CONTROLS) as u8 + ctl) } else { None }
    }
    /// The byte to put in the text for `letter` (b'A' / b'B' / b'C', the game's own placeholder result).
    pub fn button(&self, letter: u8) -> u8 {
        let i = match letter { b'A' => 0, b'B' => 1, _ => 2 };
        self.glyph(self.ctl[i]).unwrap_or(letter)
    }
    /// Replace the colour-3 word "START" (`FB 'START' F9`, the only place the game names its START button, SP09) by the
    /// START icon followed by four zero-width fillers, so that the text keeps its length and the offsets stay valid.
    pub fn subst_start(&self, t: &mut [u8]) {
        let Some(g) = self.glyph(self.ctl[3]) else { return };
        let mut i = 1;
        while i + 5 < t.len() {
            if t[i - 1] == 0xFB && &t[i..i + 5] == b"START" && t[i + 5] == 0xF9 {
                t[i] = g; for k in 1..5 { t[i + k] = ICON_BLANK; }
                i += 5;
            } else { i += 1; }
        }
    }
}

// ---------------------------------------------------------------- glyph art

const DISC: [&str; 8] = ["..####..", ".######.", "########", "########", "########", "########", ".######.", "..####.."];
const RING: [&str; 8] = ["..####..", ".#....#.", "#......#", "#......#", "#......#", "#......#", ".#....#.", "..####.."];
const CAP: [&str; 8] = [".######.", "########", "########", "########", "########", "########", "########", ".######."];
const DSHAPE: [&str; 8] = ["######..", "#######.", "########", "########", "########", "#######.", "######..", "........"];
const CROSS: [&str; 8] = ["##...##.", "###.###.", ".#####..", "..###...", ".#####..", "###.###.", "##...##.", "........"];
const TRIANGLE: [&str; 8] = ["...#....", "...#....", "..#.#...", "..#.#...", ".#...#..", ".#...#..", "#######.", "........"];
const SQUARE: [&str; 8] = ["#######.", "#.....#.", "#.....#.", "#.....#.", "#.....#.", "#.....#.", "#######.", "........"];
const BAR: [&str; 8] = [".######.", "........", "........", "........", "........", "........", "........", "........"];

/// 3x5 letters used inside icons.
fn letter(c: char) -> [&'static str; 5] {
    match c {
        'A' => [".#.", "#.#", "###", "#.#", "#.#"],
        'B' => ["##.", "#.#", "##.", "#.#", "##."],
        'C' => [".##", "#..", "#..", "#..", ".##"],
        'L' => ["#..", "#..", "#..", "#..", "###"],
        'R' => ["##.", "#.#", "##.", "#.#", "#.#"],
        'T' => ["###", ".#.", ".#.", ".#.", ".#."],
        'X' => ["#.#", "#.#", ".#.", "#.#", "#.#"],
        'Y' => ["#.#", "#.#", ".#.", ".#.", ".#."],
        'Z' => ["###", "..#", ".#.", "#..", "###"],
        '1' => [".#.", "##.", ".#.", ".#.", "###"],
        '2' => ["##.", "..#", ".#.", "#..", "###"],
        _ => ["...", "...", "...", "...", "..."],
    }
}

type Art = [[u8; 8]; 8]; // [row][col] = 0/1

fn art(a: &[&str; 8]) -> Art {
    let mut o = [[0u8; 8]; 8];
    for (r, s) in a.iter().enumerate() { for (c, ch) in s.bytes().enumerate() { o[r][c] = (ch == b'#') as u8; } }
    o
}
/// 4x5 letters for the big icons (disc, ring, keycap): a bit wider than the 3x5 labels so that Z/X/C/A/B/Y stay readable.
fn letter4(c: char) -> [&'static str; 5] {
    match c {
        'A' => [".##.", "#..#", "####", "#..#", "#..#"],
        'B' => ["###.", "#..#", "###.", "#..#", "###."],
        'C' => [".###", "#...", "#...", "#...", ".###"],
        'X' => ["#..#", "#..#", ".##.", "#..#", "#..#"],
        'Y' => ["#..#", "#..#", ".##.", ".##.", ".##."],
        'Z' => ["####", "...#", ".##.", "#...", "####"],
        _ => ["....", "....", "....", "....", "...."],
    }
}
/// Stamp a letter at (row, col): `big` = 4x5 icon letter, else 3x5 label letter; `knock` clears the pixels (letter cut out of a filled shape), else sets them.
fn stamp_with(a: &mut Art, rows: [&str; 5], row: usize, col: usize, knock: bool) {
    for (r, s) in rows.iter().enumerate() {
        for (c, ch) in s.bytes().enumerate() { if ch == b'#' { a[row + r][col + c] = (!knock) as u8; } }
    }
}
fn stamp(a: &mut Art, l: char, row: usize, col: usize, knock: bool) { stamp_with(a, letter(l), row, col, knock) }
fn stamp_big(a: &mut Art, l: char, row: usize, col: usize, knock: bool) { stamp_with(a, letter4(l), row, col, knock) }
fn holes(a: &mut Art, cells: &[(usize, usize)]) { for &(r, c) in cells { a[r][c] = 0; } }
fn pack(a: &Art) -> [u8; 8] {
    let mut o = [0u8; 8];
    for r in 0..8 { for c in 0..8 { if a[r][c] != 0 { o[r] |= 0x80 >> c; } } }
    o
}
fn width_of(a: &Art) -> u8 { (0..8).rev().find(|&c| (0..8).any(|r| a[r][c] != 0)).map_or(0, |c| c as u8 + 1) }
fn line(a: &mut Art, r: usize, c0: usize, c1: usize) { for c in c0..=c1 { a[r][c] = 0; } }

/// Two-letter / one-letter bumper or trigger label under a top bar (letters rows 2..6).
fn label(s: &str) -> Art {
    let mut a = art(&BAR);
    let n = s.chars().count();
    for (i, ch) in s.chars().enumerate() { stamp(&mut a, ch, 2, if n == 1 { 2 } else { 4 * i }, false); }
    a
}

fn pad_family(fam: usize) -> Vec<Art> {
    // order: south east west north  l1 r1 l2 r2  start
    let letters = match fam { 0 => ['A', 'B', 'X', 'Y'], 2 => ['B', 'A', 'Y', 'X'], _ => [' '; 4] };
    let mut v = Vec::new();
    for (i, &ch) in letters.iter().enumerate() {
        v.push(match fam {
            0 => { let mut a = art(&DISC); stamp_big(&mut a, ch, 2, 2, true); a }
            2 => { let mut a = art(&RING); stamp_big(&mut a, ch, 2, 2, false); a }
            _ => art(match i { 0 => &CROSS, 1 => &RING, 2 => &SQUARE, _ => &TRIANGLE }),
        });
    }
    let sh: [&str; 4] = match fam { 0 => ["LB", "RB", "LT", "RT"], 1 => ["L1", "R1", "L2", "R2"], _ => ["L", "R", "ZL", "ZR"] };
    for s in sh { v.push(label(s)); }
    v.push(match fam {
        0 => { let mut a = art(&DISC); for r in [2, 4, 6] { line(&mut a, r, 2, 5); } a }
        1 => { let mut a = art(&DSHAPE); for r in [1, 3, 5] { line(&mut a, r, 1, 4); } a }
        _ => { let mut a = art(&DISC); holes(&mut a, &[(1, 3), (1, 4), (2, 3), (2, 4), (3, 1), (3, 2), (3, 3), (3, 4), (3, 5), (3, 6), (4, 1), (4, 2), (4, 3), (4, 4), (4, 5), (4, 6), (5, 3), (5, 4), (6, 3), (6, 4)]); a }
    });
    v
}

/// All icon glyphs in code order (0x86..): 3 families x 9 controls, 4 keys, the zero-width blank. Returns (bitmap, advance).
pub fn icon_glyphs() -> Vec<([u8; 8], u8)> {
    let mut all: Vec<Art> = Vec::new();
    for fam in 0..3 { all.extend(pad_family(fam)); }
    for ch in ['Z', 'X', 'C'] { let mut a = art(&CAP); stamp_big(&mut a, ch, 1, 2, true); all.push(a); }
    let mut e = art(&CAP);
    holes(&mut e, &[(2, 6), (3, 6), (4, 6), (4, 2), (5, 1), (5, 2), (5, 3), (5, 4), (5, 5), (5, 6), (6, 2)]);
    all.push(e);
    let mut out: Vec<([u8; 8], u8)> = all.iter().map(|a| (pack(a), width_of(a) + 1)).collect();
    out.push(([0; 8], 0));
    debug_assert_eq!(out.len(), ICON_COUNT);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_layout() {
        assert_eq!(ICON_BASE as usize + ICON_COUNT, 0xA6);
        assert_eq!(ICON_BLANK, 0xA5);
        let g = icon_glyphs();
        assert_eq!(g.len(), ICON_COUNT);
        assert_eq!(g[ICON_COUNT - 1], ([0; 8], 0));
        // every other glyph has pixels and a non-zero advance of at most 9
        for (b, w) in &g[..ICON_COUNT - 1] { assert!(b.iter().any(|&r| r != 0)); assert!((1..=9).contains(w), "width {w}"); }
    }

    #[test]
    fn all_icons_distinct() {
        let g = icon_glyphs();
        for i in 0..g.len() { for j in i + 1..g.len() { assert_ne!(g[i].0, g[j].0, "glyph {i} == glyph {j}"); } }
    }

    #[test]
    fn genesis_keeps_the_letter() {
        let p = Prompt::default();
        for l in [b'A', b'B', b'C'] { assert_eq!(p.button(l), l); }
        let mut t = *b"\xFBSTART\xF9 x";
        let o = t; p.subst_start(&mut t); assert_eq!(t, o);
    }

    #[test]
    fn each_style_maps_letters_through_the_controls() {
        let mut p = Prompt::default();
        // A on west, B on east, C on south, START on start
        p.set(1, CTL_WEST, CTL_EAST, CTL_SOUTH, CTL_START);
        assert_eq!([p.button(b'A'), p.button(b'B'), p.button(b'C')], [0x86 + 2, 0x86 + 1, 0x86]);
        p.set(2, CTL_WEST, CTL_EAST, CTL_SOUTH, CTL_START);
        assert_eq!([p.button(b'A'), p.button(b'B'), p.button(b'C')], [0x86 + 9 + 2, 0x86 + 9 + 1, 0x86 + 9]);
        p.set(3, CTL_WEST, CTL_EAST, CTL_SOUTH, CTL_START);
        assert_eq!([p.button(b'A'), p.button(b'B'), p.button(b'C')], [0x86 + 18 + 2, 0x86 + 18 + 1, 0x86 + 18]);
        p.set(4, KEY_Z, KEY_X, KEY_C, KEY_ENTER);
        assert_eq!([p.button(b'A'), p.button(b'B'), p.button(b'C')], [0xA1, 0xA2, 0xA3]);
        // out-of-range control falls back to the letter
        p.set(4, 9, 9, 9, 9);
        assert_eq!(p.button(b'B'), b'B');
        p.set(1, 99, 0, 0, 0);
        assert_eq!(p.button(b'A'), b'A');
    }

    #[test]
    fn start_word_becomes_icon_and_keeps_length() {
        let mut p = Prompt::default();
        p.set(2, 2, 1, 0, CTL_START);
        let mut t = b"press the \xFBSTART\xF9 button and GUN'S \xFASTART\xF9".to_vec();
        let n = t.len();
        p.subst_start(&mut t);
        assert_eq!(t.len(), n);
        let i = 10 + 1;
        assert_eq!(t[i], 0x86 + 9 + CTL_START);
        assert_eq!(&t[i + 1..i + 5], &[ICON_BLANK; 4]);
        assert_eq!(&t[n - 6..n - 1], b"START", "colour-2 gun START is untouched");
    }
}

#[cfg(test)]
mod dump {
    /// `cargo test -p sncore --lib prompt::dump -- --ignored --nocapture` prints every icon as ASCII art (design aid).
    #[test]
    #[ignore]
    fn dump_icons() {
        let g = super::icon_glyphs();
        for row in g.chunks(9) {
            for r in 0..8 {
                let mut s = String::new();
                for (b, w) in row { for k in 0..8 { s.push(if b[r] & (0x80 >> k) != 0 { '#' } else { '.' }); } s.push_str(&format!("|{w}  ")); let _ = w; }
                println!("{s}");
            }
            println!();
        }
    }
}
