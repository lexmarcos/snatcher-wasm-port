//! Translation support (pt-BR): text markup, extra font glyphs, the compact translation pack read by the engine, and the text
//! layout rules used by the validator (`snt i18n-check`). docs/i18n.md describes the whole feature.
//!
//! Nothing here contains game text. Translations are keyed by (SP file, text offset); the engine swaps the string when the
//! language is not English and falls back to the original string when an entry is missing.
//!
//! # Markup (readable form of a script string, round-trips exactly)
//! `{nl}` F2 new line, `{page}` F6 page break, `{auto}` F4 auto-advance, `{c0}`..`{c6}` colour change (F8..FE),
//! `{btn:EC}` `{btn:EE}` `{btn:F0}` button placeholders, `{lb}` `{rb}` literal braces, `{xNN}` any other byte.
//! Printable ASCII is itself; the pt-BR letters (`á ã ç ...`) map to the glyph codes `PT_BASE..`.

use std::collections::HashMap;

/// First code of the pt-BR glyphs. Icons (`prompt.rs`) end at 0xA5; 0xEC and above are control bytes of the script text.
pub const PT_BASE: u8 = 0xA6;

pub const NL: u8 = 0xF2;
pub const PAGE: u8 = 0xF6;
pub const AUTO: u8 = 0xF4;
pub const END: u8 = 0xFF;

/// One extra glyph: character, 8x8 art ('#' = pixel) and advance width in pixels.
pub struct PtGlyph { pub ch: char, pub art: [&'static str; 8], pub adv: u8 }

macro_rules! g { ($c:expr, $w:expr, [$($r:expr),*]) => { PtGlyph { ch: $c, art: [$($r),*], adv: $w } }; }

/// The pt-BR glyphs in code order (code = PT_BASE + index). Everything stays inside the 8x8 cell (docs/i18n.md "Accented capitals"):
/// the original capitals use rows 0..6 (baseline = row 6, row 7 only for descenders), lower-case letters rows 2..6 with the accent in
/// rows 0..1. An accented capital keeps the baseline in row 6 (so it lines up with the original capitals), is drawn 5 rows high
/// (rows 2..6, the x-height) and carries a 2-row accent in rows 0..1, so its envelope is the same 7 rows as an original capital. Ç is
/// the original C (rows 0..6) with the cedilla in row 7. Advance widths equal those of the base letter.
pub const PT_GLYPHS: &[PtGlyph] = &[
    // lower case
    g!('á', 7, ["...#....", "..#.....", ".###....", "....#...", ".####...", "#...#...", ".###.#..", "........"]),
    g!('à', 7, [".#......", "..#.....", ".###....", "....#...", ".####...", "#...#...", ".###.#..", "........"]),
    g!('â', 7, ["..#.....", ".#.#....", ".###....", "....#...", ".####...", "#...#...", ".###.#..", "........"]),
    g!('ã', 7, [".##.#...", "#..#....", ".###....", "....#...", ".####...", "#...#...", ".###.#..", "........"]),
    g!('é', 6, ["...#....", "..#.....", ".###....", "#...#...", "#####...", "#.......", ".####...", "........"]),
    g!('ê', 6, ["..#.....", ".#.#....", ".###....", "#...#...", "#####...", "#.......", ".####...", "........"]),
    g!('í', 4, ["..#.....", ".#......", "##......", ".#......", ".#......", ".#......", "###.....", "........"]),
    g!('ó', 6, ["...#....", "..#.....", ".###....", "#...#...", "#...#...", "#...#...", ".###....", "........"]),
    g!('ô', 6, ["..#.....", ".#.#....", ".###....", "#...#...", "#...#...", "#...#...", ".###....", "........"]),
    g!('õ', 6, [".##.#...", "#..#....", ".###....", "#...#...", "#...#...", "#...#...", ".###....", "........"]),
    g!('ú', 7, ["....#...", "...#....", "#...#...", "#...#...", "#...#...", "#...#...", ".###.#..", "........"]),
    g!('ü', 7, [".#.#....", "........", "#...#...", "#...#...", "#...#...", "#...#...", ".###.#..", "........"]),
    g!('ç', 6, ["........", "........", ".###....", "#...#...", "#.......", "#...#...", ".###....", "..#....."]),
    // capitals: accent rows 0..1, body rows 2..6 (Ç: original C rows 0..6 + cedilla row 7)
    g!('Á', 9, [".......#", "......#.", "...##...", "..#..#..", ".######.", ".#....#.", "##....##", "........"]),
    g!('À', 9, ["#.......", ".#......", "...##...", "..#..#..", ".######.", ".#....#.", "##....##", "........"]),
    g!('Â', 9, ["...##...", "..#..#..", "...##...", "..#..#..", ".######.", ".#....#.", "##....##", "........"]),
    g!('Ã', 9, ["..##.#..", ".#..#...", "...##...", "..#..#..", ".######.", ".#....#.", "##....##", "........"]),
    g!('É', 8, ["....#...", "...#....", "#######.", ".#..#...", ".####...", ".#....#.", "#######.", "........"]),
    g!('Ê', 8, ["...#....", "..#.#...", "#######.", ".#..#...", ".####...", ".#....#.", "#######.", "........"]),
    g!('Í', 5, ["...#....", "..#.....", ".###....", "..#.....", "..#.....", "..#.....", ".###....", "........"]),
    g!('Ó', 8, ["....#...", "...#....", ".#####..", "#.....#.", "#.....#.", "#.....#.", ".#####..", "........"]),
    g!('Ô', 8, ["...#....", "..#.#...", ".#####..", "#.....#.", "#.....#.", "#.....#.", ".#####..", "........"]),
    g!('Õ', 8, ["..##.#..", ".#..#...", ".#####..", "#.....#.", "#.....#.", "#.....#.", ".#####..", "........"]),
    g!('Ú', 9, ["....#...", "...#....", "###..###", ".#....#.", ".#....#.", ".##..##.", "..####..", "........"]),
    g!('Ü', 9, ["..#..#..", "........", "###..###", ".#....#.", ".#....#.", ".##..##.", "..####..", "........"]),
    g!('Ç', 8, [".#####..", "#.....#.", "#.......", "#.......", "#.......", "#.....#.", ".#####..", "...#...."]),
    // ordinals and guillemets
    g!('º', 5, [".##.....", "#..#....", "#..#....", ".##.....", "........", "####....", "........", "........"]),
    g!('ª', 5, [".###....", "#..#....", ".###....", "........", "........", "####....", "........", "........"]),
    g!('«', 7, ["........", "........", "...#..#.", "..#..#..", ".#..#...", "..#..#..", "...#..#.", "........"]),
    g!('»', 7, ["........", "........", ".#..#...", "..#..#..", "...#..#.", "..#..#..", ".#..#...", "........"]),
];

pub fn pt_glyph_bitmaps() -> Vec<([u8; 8], u8)> {
    PT_GLYPHS.iter().map(|g| {
        let mut o = [0u8; 8];
        for (r, s) in g.art.iter().enumerate() { for (c, ch) in s.bytes().enumerate() { if ch == b'#' { o[r] |= 0x80 >> c; } } }
        (o, g.adv)
    }).collect()
}

/// Glyph code of a pt-BR character.
pub fn pt_code(ch: char) -> Option<u8> { PT_GLYPHS.iter().position(|g| g.ch == ch).map(|i| PT_BASE + i as u8) }
pub fn pt_char(code: u8) -> Option<char> { code.checked_sub(PT_BASE).and_then(|i| PT_GLYPHS.get(i as usize)).map(|g| g.ch) }

// ---------------------------------------------------------------- dialogue layout (10 px line pitch, full-height capitals)

/// Dialogue windows (message box with or without a name, narration) are drawn with a 10 px line pitch in pt-BR: `TALL_ACCENT_ROWS`
/// rows above the glyph cell for the accent of a capital, then the 8 rows of the glyph cell. Menus keep the 9 px pitch. docs/i18n.md
/// ("Dialogue and menu layouts") has the numbers.
pub const TALL_PITCH: u32 = 10;
/// Rows reserved above the 8x8 cell of a dialogue line for the accent of a capital (1 = a single row, 2 = a two-row accent).
pub const TALL_ACCENT_ROWS: u32 = 2;

/// An accented capital of the dialogue windows: exactly the glyph of the original capital `base` (rows 0..6, unchanged) plus the accent in the
/// rows above it (`accent` = the two rows just above the glyph, top first; with `TALL_ACCENT_ROWS` = 1 only the lower one is drawn) and, for Ç,
/// the cedilla in row 7. Advance width = that of the base letter.
pub struct TallCap { pub ch: char, pub base: char, pub accent: [&'static str; 2], pub cedilla: &'static str }

macro_rules! t { ($c:expr, $b:expr, [$a0:expr, $a1:expr]) => { TallCap { ch: $c, base: $b, accent: [$a0, $a1], cedilla: "........" } }; }

pub const TALL_CAPS: &[TallCap] = &[
    t!('Á', 'A', ["......#.", ".....#.."]),
    t!('À', 'A', [".#......", "..#....."]),
    t!('Â', 'A', ["...##...", "..#..#.."]),
    t!('Ã', 'A', ["..##.#..", ".#..#..."]),
    t!('É', 'E', ["....#...", "...#...."]),
    t!('Ê', 'E', ["...#....", "..#.#..."]),
    t!('Í', 'I', ["...#....", "..#....."]),
    t!('Ó', 'O', ["....#...", "...#...."]),
    t!('Ô', 'O', ["...#....", "..#.#..."]),
    t!('Õ', 'O', ["..##.#..", ".#..#..."]),
    t!('Ú', 'U', [".....#..", "....#..."]),
    t!('Ü', 'U', ["..#..#..", "........"]),
    TallCap { ch: 'Ç', base: 'C', accent: ["........", "........"], cedilla: "...#...." },
];

fn art_row(s: &str) -> u8 { s.bytes().enumerate().fold(0u8, |a, (c, ch)| if ch == b'#' { a | 0x80 >> c } else { a }) }

/// The dialogue form of an accented capital: (glyph index of the base capital `code - 0x20`, accent rows top first, row 7).
pub fn tall_cap(code: u8) -> Option<(usize, [u8; 2], u8)> {
    let ch = pt_char(code)?;
    let t = TALL_CAPS.iter().find(|t| t.ch == ch)?;
    Some((t.base as usize - 0x20, [art_row(t.accent[0]), art_row(t.accent[1])], art_row(t.cedilla)))
}

/// Y of the first text line of a message box: below the name line (speaker >= 2) or at the top; in the 10 px layout the accent rows come first.
pub fn first_line_y(speaker: u32, tall: bool) -> u32 {
    (if speaker >= 2 { LINE_PITCH } else { 0 }) + if tall { TALL_ACCENT_ROWS } else { 0 }
}

/// Text lines per page of a message box (the page ends when the next line would start at Y >= `PAGE_LIMIT_Y`).
pub fn lines_per_page(speaker: u32, tall: bool) -> usize {
    let (pitch, mut y, mut n) = (if tall { TALL_PITCH } else { LINE_PITCH }, first_line_y(speaker, tall), 1);
    loop { y += pitch; if y >= PAGE_LIMIT_Y { return n; } n += 1; }
}

// ---------------------------------------------------------------- markup

/// Readable markup of a raw script string (without the FF terminator).
pub fn bytes_to_markup(raw: &[u8]) -> String {
    let mut s = String::new();
    for &c in raw {
        match c {
            b'{' => s.push_str("{lb}"),
            b'}' => s.push_str("{rb}"),
            0x20..=0x7E => s.push(c as char),
            NL => s.push_str("{nl}"),
            PAGE => s.push_str("{page}"),
            AUTO => s.push_str("{auto}"),
            0xF8..=0xFE => s.push_str(&format!("{{c{}}}", c & 7)),
            0xEC | 0xEE | 0xF0 => s.push_str(&format!("{{btn:{c:02X}}}")),
            _ => match pt_char(c) {
                Some(ch) => s.push(ch),
                None => s.push_str(&format!("{{x{c:02X}}}")),
            },
        }
    }
    s
}

/// Parse the markup back to raw bytes (without the FF terminator). Errors name the offending character / token.
pub fn markup_to_bytes(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut it = s.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '{' {
            let mut tok = String::new();
            loop {
                match it.next() {
                    Some('}') => break,
                    Some(c) => tok.push(c),
                    None => return Err(format!("unterminated token {{{tok}")),
                }
            }
            out.push(match tok.as_str() {
                "nl" => NL, "page" => PAGE, "auto" => AUTO, "lb" => b'{', "rb" => b'}',
                t if t.len() == 2 && t.starts_with('c') && matches!(t.as_bytes()[1], b'0'..=b'6') => 0xF8 + (t.as_bytes()[1] - b'0'),
                t if t.starts_with("btn:") => match u8::from_str_radix(&t[4..], 16) {
                    Ok(b @ (0xEC | 0xEE | 0xF0)) => b,
                    _ => return Err(format!("bad button token {{{t}}}")),
                },
                t if t.starts_with('x') && t.len() == 3 => u8::from_str_radix(&t[1..], 16).map_err(|_| format!("bad token {{{t}}}"))?,
                t => return Err(format!("unknown token {{{t}}}")),
            });
        } else if ('\u{20}'..='\u{7E}').contains(&ch) && ch != '}' {
            out.push(ch as u8);
        } else if let Some(c) = pt_code(ch) {
            out.push(c);
        } else {
            return Err(format!("unsupported character {ch:?} (U+{:04X})", ch as u32));
        }
    }
    Ok(out)
}

/// FNV-1a 32 bit of the raw original string (without terminator): the key check stored with every translation.
pub fn hash32(raw: &[u8]) -> u32 {
    let mut h = 0x811C9DC5u32;
    for &b in raw { h ^= b as u32; h = h.wrapping_mul(0x01000193); }
    h
}

// ---------------------------------------------------------------- layout rules

/// Message box geometry (docs/re/actions.md section 2.3, measured from `MessageBox`): the glyph bitmap is 256 px wide, text starts
/// at X = 0x18 (0x28 for narration, speaker 1), the name line (speakers >= 2) takes the first 9 px row, a page ends when the next line
/// would start at Y >= 0x39, a line is cut after 50 characters.
pub const WIN_W: u32 = 256;
pub const X_SPEECH: u32 = 0x18;
pub const X_NARRATION: u32 = 0x28;
pub const X_MENU: u32 = 0x10;
/// Right limits of the text (x after the last advance), symmetric to the left margins: 256 - 0x18, 256 - 0x28, 256 - 0x10. The longest
/// English lines end at 225 (speech) / 216 (narration); the menu highlight bar ends at 240; the "waiting" arrow sprite sits at x >= 232.
pub const RIGHT_SPEECH: u32 = 232;
pub const RIGHT_NARRATION: u32 = 216;
pub const RIGHT_MENU: u32 = 240;
pub const LINE_PITCH: u32 = 9;
pub const PAGE_LIMIT_Y: u32 = 0x39;
pub const LINE_MAX_CHARS: usize = 0x32;
pub const MENU_MAX_BYTES: usize = 0x20;

/// Advance width of a glyph code in pixels (button placeholders count as the widest icon / letter, 9).
pub fn advance(widths: &[u8], code: u8) -> u32 {
    match code {
        0xEC | 0xEE | 0xF0 => 9,
        0x20..=0xEB => widths.get((code - 0x20) as usize).copied().unwrap_or(8) as u32,
        _ => 0,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineInfo {
    /// page number (0-based) and Y of the line inside the window
    pub page: usize,
    pub y: u32,
    /// text start X plus the summed advances
    pub x_end: u32,
    pub chars: usize,
    /// number of glyph bytes in the line
    pub text: Vec<u8>,
    /// the line ended because of the 50 character cut (not a F2/F6/FF)
    pub cut: bool,
    /// the window is full after this line and the text goes on: the engine breaks the page here by itself (no F6)
    pub full_page: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsgLayout { pub lines: Vec<LineInfo>, pub pages: usize }

/// Simulate `MessageBox::build_line` over a whole message. `speaker` is the SAY speaker value (0 none, 1 narration, >= 2 named).
pub fn layout_message(raw: &[u8], speaker: u8, widths: &[u8]) -> MsgLayout { layout_message_in(raw, speaker, widths, false) }

/// `layout_message` for the dialogue layout of pt-BR (`tall` = 10 px pitch, accent rows first) or the original one.
pub fn layout_message_in(raw: &[u8], speaker: u8, widths: &[u8], tall: bool) -> MsgLayout {
    let x0 = if speaker == 1 { X_NARRATION } else { X_SPEECH };
    let pitch = if tall { TALL_PITCH } else { LINE_PITCH };
    let mut y = first_line_y(speaker as u32, tall);
    let first_y = y;
    let mut pos = 0usize;
    let mut page = 0usize;
    let mut lines = Vec::new();
    let mut ended = false;
    while !ended {
        let mut x = x0;
        let mut n = 0usize;
        let mut text = Vec::new();
        let mut stop = 0u8;
        let mut cut = true;
        while n < LINE_MAX_CHARS {
            let c = raw.get(pos).copied().unwrap_or(END);
            pos += 1;
            match c {
                END => { ended = true; cut = false; break; }
                NL => { stop = 1; cut = false; break; }
                PAGE => { stop = 3; cut = false; break; }
                AUTO => {}
                0xF8..=0xFE => {}
                _ => { x += advance(widths, c); n += 1; text.push(c); }
            }
        }
        lines.push(LineInfo { page, y, x_end: x, chars: n, text, cut, full_page: false });
        if ended || stop == 3 { page += 1; y = first_y; } else {
            y += pitch;
            if y >= PAGE_LIMIT_Y { page += 1; y = first_y; lines.last_mut().unwrap().full_page = true; }
        }
        if pos > raw.len() + 1 && !ended { break; }
    }
    let pages = lines.iter().map(|l| l.page).max().map_or(0, |p| p + 1);
    MsgLayout { lines, pages }
}

/// Menu item (CHOICE label): one line at X = 0x10, at most 0x20 bytes, ends at FF or F2.
pub fn menu_width(raw: &[u8], widths: &[u8]) -> (u32, usize) {
    let t: Vec<u8> = raw.iter().copied().take_while(|&c| c != NL && c != END).collect();
    let used = t.len().min(MENU_MAX_BYTES);
    (X_MENU + t[..used].iter().map(|&c| advance(widths, c)).sum::<u32>(), t.len())
}

// ---------------------------------------------------------------- translation pack

/// Parsed translation pack: for each SP file number a map text offset -> translated raw bytes (no terminator), plus speaker names.
#[derive(Default, Clone)]
pub struct Translation {
    pub files: HashMap<u8, HashMap<u16, Vec<u8>>>,
    pub names: HashMap<u8, Vec<u8>>,
    /// subtitles of the voice clips by clip id (docs/voice-subtitles.md); absent in packs made before the feature
    pub voice: HashMap<u16, VoiceSub>,
}

// ---- voice subtitle timing (shared by the engine overlay and the validator `snt voice-check`)

/// Reading speed the subtitle hold is computed for, characters per second.
pub const VOICE_READ_CPS: f64 = 17.0;
/// Longest hold after the end of the clip, frames (1.5 s).
pub const VOICE_HOLD_MAX: u32 = 90;
/// Hold after every clip (frames) so the backing does not flicker between two clips of a dialogue.
pub const VOICE_HOLD_MIN: u32 = 15;

/// Number of visible characters of a markup text (control codes and button placeholders do not count).
pub fn voice_glyph_count(text: &[u8]) -> usize {
    text.iter().filter(|&&c| (0x20..0xF0).contains(&c) && !matches!(c, 0xEC | 0xEE)).count()
}

/// (frames the clip plays, hold frames) of a subtitle of `chars` visible characters for a clip of `play` frames: the hold gives the reading
/// time the clip does not have, at least `VOICE_HOLD_MIN` and at most `VOICE_HOLD_MAX`.
pub fn voice_display_frames(play: u32, chars: usize) -> (u32, u32) {
    let need = (chars as f64 / VOICE_READ_CPS * 60.0).ceil() as u32;
    (play, need.saturating_sub(play).clamp(VOICE_HOLD_MIN, VOICE_HOLD_MAX))
}

/// Start frame of every page when `total` frames are shared in proportion to the characters of the pages.
pub fn voice_page_starts(pages: &[Vec<u8>], total: u32) -> Vec<u32> {
    let w: Vec<usize> = pages.iter().map(|p| voice_glyph_count(p).max(1)).collect();
    let sum: usize = w.iter().sum();
    let mut acc = 0usize;
    w.iter().map(|&x| { let s = (total as u64 * acc as u64 / sum as u64) as u32; acc += x; s }).collect()
}

/// Subtitle of one voice clip: markup bytes (`NL`, `PAGE`, colour codes, no terminator) and the speaker number as printed by `snt i18n-names`
/// (0 = no speaker: the text uses the layout of a message without a name; >= 2 = the name is drawn as in a message box).
#[derive(Clone, Debug, PartialEq)]
pub struct VoiceSub { pub speaker: u8, pub text: Vec<u8> }

const MAGIC: &[u8; 5] = b"SNTR\x01";

impl Translation {
    /// Pack layout (big endian): "SNTR" 1, u8 nfiles, per file { u8 sp, u16 n, n x { u16 off, u16 len, bytes } },
    /// u8 nnames, nnames x { u8 index, u8 len, bytes }, then optionally (packs with voice subtitles)
    /// u16 nvoice, nvoice x { u16 clip id, u8 speaker, u16 len, bytes }.
    pub fn parse(b: &[u8]) -> Result<Translation, String> {
        let mut p = 0usize;
        let rd = |p: &mut usize, n: usize| -> Result<&[u8], String> {
            let s = b.get(*p..*p + n).ok_or("truncated translation pack")?; *p += n; Ok(s)
        };
        if rd(&mut p, 5)? != MAGIC { return Err("not a translation pack".into()); }
        let mut t = Translation::default();
        let nf = rd(&mut p, 1)?[0];
        for _ in 0..nf {
            let sp = rd(&mut p, 1)?[0];
            let h = rd(&mut p, 2)?; let n = u16::from_be_bytes([h[0], h[1]]);
            let m = t.files.entry(sp).or_default();
            for _ in 0..n {
                let h = rd(&mut p, 4)?;
                let (off, len) = (u16::from_be_bytes([h[0], h[1]]), u16::from_be_bytes([h[2], h[3]]) as usize);
                m.insert(off, rd(&mut p, len)?.to_vec());
            }
        }
        let nn = rd(&mut p, 1)?[0];
        for _ in 0..nn {
            let h = rd(&mut p, 2)?;
            t.names.insert(h[0], rd(&mut p, h[1] as usize)?.to_vec());
        }
        if p < b.len() {
            let h = rd(&mut p, 2)?; let nv = u16::from_be_bytes([h[0], h[1]]);
            for _ in 0..nv {
                let h = rd(&mut p, 5)?;
                let len = u16::from_be_bytes([h[3], h[4]]) as usize;
                t.voice.insert(u16::from_be_bytes([h[0], h[1]]), VoiceSub { speaker: h[2], text: rd(&mut p, len)?.to_vec() });
            }
        }
        Ok(t)
    }

    pub fn build(&self) -> Vec<u8> {
        let mut o = MAGIC.to_vec();
        let mut sps: Vec<_> = self.files.keys().copied().collect(); sps.sort();
        o.push(sps.len() as u8);
        for sp in sps {
            let m = &self.files[&sp];
            let mut offs: Vec<_> = m.keys().copied().collect(); offs.sort();
            o.push(sp); o.extend_from_slice(&(offs.len() as u16).to_be_bytes());
            for off in offs {
                let v = &m[&off];
                o.extend_from_slice(&off.to_be_bytes()); o.extend_from_slice(&(v.len() as u16).to_be_bytes()); o.extend_from_slice(v);
            }
        }
        let mut ns: Vec<_> = self.names.keys().copied().collect(); ns.sort();
        o.push(ns.len() as u8);
        for n in ns { let v = &self.names[&n]; o.push(n); o.push(v.len() as u8); o.extend_from_slice(v); }
        if !self.voice.is_empty() {
            let mut ids: Vec<_> = self.voice.keys().copied().collect(); ids.sort();
            o.extend_from_slice(&(ids.len() as u16).to_be_bytes());
            for id in ids {
                let v = &self.voice[&id];
                o.extend_from_slice(&id.to_be_bytes()); o.push(v.speaker); o.extend_from_slice(&(v.text.len() as u16).to_be_bytes()); o.extend_from_slice(&v.text);
            }
        }
        o
    }

    /// The text area of one SP file as the engine reads it in this language: the original area (`orig`, from 0x3800) followed by
    /// every translated string (FF terminated). Returns the extended area and a map original offset -> offset of the translation.
    pub fn extend_area(&self, sp: u8, orig: &[u8]) -> (Vec<u8>, HashMap<u16, u16>) {
        let mut area = orig.to_vec();
        let mut map = HashMap::new();
        if let Some(m) = self.files.get(&sp) {
            if area.len() % 2 == 1 { area.push(0); }
            let mut offs: Vec<_> = m.keys().copied().collect(); offs.sort();
            for off in offs {
                let new = area.len();
                if new > 0xFFFF { break; }
                map.insert(off, new as u16);
                area.extend_from_slice(&m[&off]);
                area.push(END);
            }
        }
        (area, map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_round_trip_with_voice_subtitles_and_old_packs_without() {
        let mut t = Translation::default();
        t.files.entry(6).or_default().insert(0x0320, markup_to_bytes("Oi{nl}mundo").unwrap());
        t.names.insert(3, b"Mika".to_vec());
        let old = t.build();
        assert!(Translation::parse(&old).unwrap().voice.is_empty(), "a pack without the voice section still parses");
        t.voice.insert(77, VoiceSub { speaker: 3, text: markup_to_bytes("Sou Gillian.{page}Prazer.").unwrap() });
        t.voice.insert(1197, VoiceSub { speaker: 0, text: markup_to_bytes("{c3}Fim{c1}").unwrap() });
        let p = Translation::parse(&t.build()).unwrap();
        assert_eq!(p.voice, t.voice);
        assert_eq!(p.files, t.files);
        assert_eq!(p.names, t.names);
    }

    #[test]
    fn voice_timing_hold_and_pages() {
        // a 2 s clip (126 frames with the engine's +6) with 20 characters needs 71 frames of reading: no extra hold beyond the minimum
        assert_eq!(voice_display_frames(126, 20), (126, VOICE_HOLD_MIN));
        // 60 characters need 212 frames: hold of 86 frames
        assert_eq!(voice_display_frames(126, 60), (126, 86));
        // never longer than the maximum
        assert_eq!(voice_display_frames(60, 400).1, VOICE_HOLD_MAX);
        assert_eq!(voice_glyph_count(&markup_to_bytes("{c3}ab{nl}c d{page}e").unwrap()), 6);
        let pages = vec![vec![b'a'; 30], vec![b'b'; 10]];
        assert_eq!(voice_page_starts(&pages, 400), vec![0, 300]);
    }


    #[test]
    fn markup_round_trip_of_every_byte() {
        let mut raw: Vec<u8> = (0x20u8..=0x7E).collect();
        raw.extend([NL, PAGE, AUTO, 0xF8, 0xF9, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE, 0xEC, 0xEE, 0xF0, 0x00, 0x01, 0x86, 0xEB]);
        let m = bytes_to_markup(&raw);
        assert_eq!(markup_to_bytes(&m).unwrap(), raw);
    }

    #[test]
    fn every_pt_glyph_encodes_and_round_trips() {
        for (i, g) in PT_GLYPHS.iter().enumerate() {
            let code = pt_code(g.ch).unwrap();
            assert_eq!(code, PT_BASE + i as u8);
            assert!(code < 0xEC, "glyph code must stay below the control bytes");
            assert_eq!(markup_to_bytes(&g.ch.to_string()).unwrap(), vec![code]);
            assert_eq!(bytes_to_markup(&[code]), g.ch.to_string());
        }
        assert!(PT_BASE as usize + PT_GLYPHS.len() <= 0xEC);
    }

    #[test]
    fn glyph_art_is_valid() {
        for g in PT_GLYPHS {
            for r in g.art { assert_eq!(r.len(), 8, "{}", g.ch); assert!(r.bytes().all(|c| c == b'#' || c == b'.')); }
            assert!(g.art.iter().any(|r| r.contains('#')), "{}", g.ch);
            let widest = g.art.iter().map(|r| r.rfind('#').map_or(0, |i| i + 1)).max().unwrap();
            assert!(g.adv as usize >= widest && g.adv <= 9, "{} adv {} widest {}", g.ch, g.adv, widest);
        }
    }

    #[test]
    fn unsupported_and_bad_tokens_are_errors() {
        assert!(markup_to_bytes("ñ").is_err());
        assert!(markup_to_bytes("{zz}").is_err());
        assert!(markup_to_bytes("{btn:EA}").is_err());
        assert!(markup_to_bytes("abc{nl").is_err());
        assert!(markup_to_bytes("tab\there").is_err());
        assert_eq!(markup_to_bytes("{lb}x{rb}").unwrap(), b"{x}");
    }

    #[test]
    fn pack_round_trip_and_extend() {
        let mut t = Translation::default();
        t.files.entry(6).or_default().insert(0x20, b"Oi".to_vec());
        t.files.entry(6).or_default().insert(0x04, vec![pt_code('ã').unwrap(), b'o']);
        t.names.insert(2, b"Ola".to_vec());
        let b = t.build();
        let u = Translation::parse(&b).unwrap();
        assert_eq!(u.files, t.files);
        assert_eq!(u.names, t.names);
        let orig = b"A\xFFB\xFF".to_vec();
        let (area, map) = u.extend_area(6, &orig);
        assert_eq!(&area[..4], &orig[..]);
        let o = map[&0x20] as usize;
        assert_eq!(&area[o..o + 3], b"Oi\xFF");
        assert!(Translation::parse(&b[..b.len() - 1]).is_err());
        assert!(Translation::parse(b"junk").is_err());
    }

    #[test]
    fn layout_pages_and_widths() {
        let widths = vec![8u8; 200];
        // narration: 7 lines per page; speaker: 6 lines
        let mut raw = Vec::new();
        for i in 0..8 { raw.extend_from_slice(format!("L{i}").as_bytes()); if i < 7 { raw.push(NL); } }
        assert_eq!(layout_message(&raw, 1, &widths).pages, 2);
        assert_eq!(layout_message(&raw[..raw.len() - 3], 1, &widths).pages, 1);
        assert_eq!(layout_message(&raw, 3, &widths).pages, 2);
        let seven: Vec<u8> = (0..7).flat_map(|i| { let mut v = format!("L{i}").into_bytes(); if i < 6 { v.push(NL); } v }).collect();
        assert_eq!(layout_message(&seven, 0, &widths).pages, 1);
        assert_eq!(layout_message(&seven, 3, &widths).pages, 2);
        // F6 forces a page; widths add up from X
        let l = layout_message(b"ab\xF6cd", 3, &widths);
        assert_eq!(l.pages, 2);
        assert_eq!(l.lines[0].x_end, X_SPEECH + 16);
        assert_eq!(layout_message(b"ab", 1, &widths).lines[0].x_end, X_NARRATION + 16);
        // 60 chars are cut after 50
        let long = vec![b'a'; 60];
        let l = layout_message(&long, 0, &widths);
        assert_eq!(l.lines.len(), 2);
        assert!(l.lines[0].cut);
    }

    #[test]
    fn dialogue_layout_pages_and_positions() {
        let widths = vec![8u8; 200];
        let lines = |n: usize| -> Vec<u8> { (0..n).flat_map(|k| { let mut v = format!("L{k}").into_bytes(); if k + 1 < n { v.push(NL); } v }).collect() };
        // 10 px layout: 5 lines per page with a name, 6 for narration and without a name; the original layout keeps 6 / 7
        for (spk, tall_n, en_n) in [(3u8, 5usize, 6usize), (2, 5, 6), (1, 6, 7), (0, 6, 7)] {
            assert_eq!(lines_per_page(spk as u32, true), tall_n);
            assert_eq!(lines_per_page(spk as u32, false), en_n);
            assert_eq!(layout_message_in(&lines(tall_n), spk, &widths, true).pages, 1);
            let l = layout_message_in(&lines(tall_n + 1), spk, &widths, true);
            assert_eq!(l.pages, 2);
            assert!(l.lines[tall_n - 1].full_page, "the engine breaks the page by itself after line {tall_n}");
            assert_eq!(layout_message(&lines(en_n), spk, &widths).pages, 1);
            assert_eq!(layout_message(&lines(en_n + 1), spk, &widths).pages, 2);
        }
        // line Y: below the name + accent rows, pitch 10
        let l = layout_message_in(&lines(3), 3, &widths, true);
        assert_eq!(l.lines.iter().map(|x| x.y).collect::<Vec<_>>(), vec![9 + TALL_ACCENT_ROWS, 19 + TALL_ACCENT_ROWS, 29 + TALL_ACCENT_ROWS]);
        let l = layout_message_in(&lines(3), 1, &widths, true);
        assert_eq!(l.lines.iter().map(|x| x.y).collect::<Vec<_>>(), vec![TALL_ACCENT_ROWS, 10 + TALL_ACCENT_ROWS, 20 + TALL_ACCENT_ROWS]);
        // the last glyph row of the last line stays inside the 64 row bitmap
        for spk in [0u32, 1, 3] { assert!(first_line_y(spk, true) + TALL_PITCH * (lines_per_page(spk, true) as u32 - 1) + 7 < 64); }
        // every accented capital has a dialogue form, and only those
        for g in PT_GLYPHS { assert_eq!(tall_cap(pt_code(g.ch).unwrap()).is_some(), g.ch.is_uppercase(), "{}", g.ch); }
        for t in TALL_CAPS { assert!(PT_GLYPHS.iter().any(|g| g.ch == t.ch && g.adv == if t.base == 'I' { 5 } else { g.adv })); }
    }

    #[test]
    fn menu_width_stops_at_end_and_counts_bytes() {
        let widths = vec![7u8; 200];
        assert_eq!(menu_width(b"LOOK", &widths), (X_MENU + 28, 4));
    }
}
