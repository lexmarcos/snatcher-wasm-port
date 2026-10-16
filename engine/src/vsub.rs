//! Voice subtitles (docs/voice-subtitles.md): while a voice clip plays, its pt-BR subtitle is drawn next to the dialogue portraits.
//!
//! The original game never shows text with a voice clip: the message box stays dark and empty while the portrait frames of the people talking
//! (64x64 px, three slots at X 24, 96 and 168, rows 149..212) sit in the dialogue strip, the talking one with an animated mouth. The subtitle is a pure
//! overlay on the finished frame: it changes no game state, no script flow and no timing, and with the option off (or in English) nothing is
//! drawn, so the output stays byte-identical. It never covers a portrait. The text is rendered by the game's own glyph renderer (`TextWindow`, 10 px
//! pitch layout of pt-BR dialogue) into a private 256x64 bitmap and blitted where the strip has room (`sncore::vwrap::layout_for`):
//!   * no portrait: the message box, full width, name on its own line;
//!   * one portrait (or two that leave a column >= 120 px): a lane next to it, name on its own line;
//!   * two or more portraits (the 80 px gap between two is 11 characters): a black band full width just above the portraits, name inline;
//!   * no strip (the picture reaches the bottom of the screen): a compact band at the bottom edge.
//! The free width depends on the screen, so the text is wrapped and paged at run time (`vwrap::paginate`, the game's real glyph widths).
//! The layout is chosen once the portraits are on screen (a few frames after the clip starts) and kept for the clip; it only changes when a portrait
//! appears where the text is (at once) or at a page boundary.
//!
//! Timing: the clip's `voice_left` (length + 6 frames) is the display time; the pages share it in proportion to their text. A clip whose subtitle needs more
//! reading time than the clip lasts keeps it up for a short hold (reading speed `READ_CPS`, at most `HOLD_MAX` frames); the hold ends as soon as the
//! script starts another voice or does anything that changes the picture.
use crate::game::{Engine, H, W};
use sncore::actions::{Font, TextWindow, WIN_H, WIN_W};
use sncore::i18n as i;
use sncore::vwrap::{self, VKind, VLayout};

pub use sncore::i18n::{voice_display_frames as display_frames, voice_glyph_count as glyph_count, voice_page_starts as page_starts, VOICE_HOLD_MAX as HOLD_MAX, VOICE_HOLD_MIN as HOLD_MIN, VOICE_READ_CPS as READ_CPS};
/// Screen row where the message window (256x64) starts.
pub const STRIP_Y: usize = 150;
/// First screen row of the portrait frames (the object sits at 146, its frame is drawn from 149).
pub const PORTRAIT_TOP: usize = 149;
/// Last screen row (exclusive) of the dialogue strip that is scanned for portraits.
const STRIP_END: usize = 214;
pub use sncore::vwrap::SETTLE;
/// Palette of the message window (the ROM palette of the message-box channel S:0x13622): index -> Mega Drive colour word.
const PALETTE: [u16; 8] = [0, 3822, 238, 2158, 3808, 3650, 1248, 0];

#[derive(Default)]
pub struct VoiceSubs {
    /// the front end's option "voice subtitles"
    pub on: bool,
    cur: Option<Active>,
    win: Option<TextWindow>,
    /// what the last frame drew (tests, debugging)
    pub last: Option<VsubInfo>,
}

/// What the overlay drew in the last frame: the layout, the page, the occupied strip columns it saw, and the rectangle it painted
/// (text and backing, screen pixels, x0 y0 x1 y1 with x1 / y1 exclusive).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VsubInfo { pub kind: VKind, pub page: usize, pub pages: usize, pub runs: Vec<(u32, u32)>, pub rect: (usize, usize, usize, usize), pub speaker: u8 }

/// A subtitle placed for the screen it was placed on: layout, pages of lines, the frame each page starts at.
struct Placed {
    lay: VLayout,
    /// what the strip looked like when it was placed
    key: (bool, Vec<(u32, u32)>),
    ptop: usize,
    pages: Vec<Vec<Vec<u8>>>,
    starts: Vec<u32>,
    /// text lines of the band (the longest page): the band keeps its height for the whole clip
    band_lines: usize,
    /// speaker name in front of the first line of every page (`Band`, `Compact`)
    prefix: Vec<u8>,
}

struct Active {
    id: u16,
    speaker: u8,
    text: Vec<u8>,
    /// frames the subtitle stays up: the clip (`voice_left` at the start) plus the hold
    total: u32,
    t: u32,
    /// first frame the subtitle is drawn (`SETTLE` when the clip loads a portrait scene)
    delay: u32,
    placed: Option<Placed>,
    drawn: Option<usize>,
}

impl Engine {
    /// A voice clip starts (sequencer command 0x0D, `id` >= 36): look the subtitle up and start it.
    pub(crate) fn vsub_start(&mut self, id: u16, play: u32) {
        self.vsub.cur = None;
        if !self.vsub.on || !self.i18n.pt { return; }
        let Some(v) = self.i18n.pack.voice.get(&id) else { return };
        let delay = if self.portrait_scene(id).is_some() { SETTLE } else { 0 };
        // the reading time counts from the first frame the text is on screen
        let (_, hold) = display_frames(play.saturating_sub(delay), glyph_count(&v.text));
        self.vsub.cur = Some(Active { id, speaker: v.speaker, text: v.text.clone(), total: play + hold, t: 0, delay, placed: None, drawn: None });
    }

    /// The sequencer is about to run command `cmd`: commands that change the picture or draw a message end the subtitle.
    pub(crate) fn vsub_on_command(&mut self, cmd: u16) {
        if self.vsub.cur.is_some() && matches!(cmd, 0x02 | 0x08 | 0x0F | 0x11 | 0x12 | 0x15 | 0x16 | 0x18 | 0x1E | 0x1F | 0x20) { self.vsub.cur = None; self.vsub.last = None; }
    }

    /// Number of subtitles in the installed pack (tests).
    pub fn voice_subtitle_count_for_test(&self) -> usize { self.i18n.pack.voice.len() }

    /// Clip id of the subtitle on screen (tests, debugging).
    pub fn vsub_active(&self) -> Option<u16> { self.vsub.cur.as_ref().map(|a| a.id) }

    /// Layout and geometry the overlay used in the last frame (tests, debugging); `None` when no subtitle was drawn.
    pub fn vsub_info(&self) -> Option<&VsubInfo> { if self.vsub.cur.is_some() { self.vsub.last.as_ref() } else { None } }

    /// One frame of subtitle time (called once per `tick`).
    pub(crate) fn vsub_step(&mut self) {
        let end = match self.vsub.cur.as_mut() { Some(a) => { a.t += 1; a.t > a.total } None => return };
        if end || self.menu.is_some() || self.msg.is_some() || self.top != crate::game::Top::Game { self.vsub.cur = None; self.vsub.last = None; }
    }

    /// Draw the subtitle onto the finished frame `fb` (RGBA, W x H).
    pub(crate) fn vsub_overlay(&mut self, fb: &mut [u8]) {
        self.vsub.last = None;
        let Some(font) = self.font.as_ref() else { return };
        let Some(a) = self.vsub.cur.as_mut() else { return };
        if a.t < a.delay { return; }
        let names = if !self.i18n.pack.names.is_empty() { &self.i18n.names } else { &self.names };
        let (strip, runs, ptop) = scan_strip(fb);
        let page_now = |p: &Placed| p.starts.iter().rposition(|&s| s <= a.t.saturating_sub(1)).unwrap_or(0);
        // place on the first frame; afterwards only when the strip changed and the text would cover a portrait (at once) or at a page boundary
        let replace = match a.placed.as_ref() {
            None => Some((a.text.clone(), a.delay)),
            Some(p) if p.key != (strip, runs.clone()) => {
                let k = page_now(p);
                if conflicts(&p.lay, strip, &runs) || a.drawn != Some(k) {
                    let rest: Vec<u8> = p.pages[k..].iter().map(|pg| pg.join(&b' ')).collect::<Vec<_>>().join(&i::PAGE);
                    Some((rest, p.starts[k]))
                } else { None }
            }
            Some(_) => None,
        };
        if let Some((text, base)) = replace {
            a.placed = Some(place(font, names, a.speaker, &text, a.total, base, strip, runs.clone(), ptop));
            a.drawn = None;
        }
        let pl = a.placed.as_ref().unwrap();
        let page = page_now(pl);
        let win = self.vsub.win.get_or_insert_with(TextWindow::new);
        if a.drawn != Some(page) {
            win.clear();
            // the last cell of the bitmap (rows 56.., x 248..) is filled with colour 7 by `clear` (a nametable artefact); the subtitle never shows it
            for y in 56..64 { for x in 248..256 { win.pix[y * WIN_W + x] = 0; } }
            draw_page(win, font, names, a.speaker, pl, page);
            a.drawn = Some(page);
        }
        let lay = &pl.lay;
        // rows of the bitmap in use and where they go on the screen
        let (oy, bx, by0, by1) = match lay.kind {
            VKind::Box | VKind::Lane => (STRIP_Y, 0, 0, 0),
            VKind::Band => {
                let rows = 10 * pl.band_lines;
                let oy = pl.ptop.saturating_sub(1 + rows);
                (oy, 16, oy.saturating_sub(3), pl.ptop)
            }
            VKind::Compact => {
                let band = (2 + 10 * pl.band_lines + 6).min(WIN_H);
                (H - band + 3, 0, H - band, H)
            }
        };
        if by1 > by0 {
            let (x0, x1) = if lay.kind == VKind::Band { (bx, W - bx) } else { (0, W) };
            for y in by0..by1 { for x in x0..x1 { let k = (y * W + x) * 4; fb[k] = 0; fb[k + 1] = 0; fb[k + 2] = 0; fb[k + 3] = 255; } }
        }
        let (mut rx0, mut ry0, mut rx1, mut ry1) = (W, H, 0usize, 0usize);
        for y in 0..WIN_H {
            if oy + y >= H { break; }
            for x in 0..WIN_W {
                let c = win.pix[y * WIN_W + x] as usize & 15;
                if c == 0 { continue; }
                let rgba = sncore::gfx::md_rgba(*PALETTE.get(c).unwrap_or(&0));
                let k = ((oy + y) * W + x) * 4;
                fb[k..k + 4].copy_from_slice(&rgba);
                rx0 = rx0.min(x); rx1 = rx1.max(x + 1); ry0 = ry0.min(oy + y); ry1 = ry1.max(oy + y + 1);
            }
        }
        if by1 > by0 { let (x0, x1) = if lay.kind == VKind::Band { (bx, W - bx) } else { (0, W) }; rx0 = rx0.min(x0); rx1 = rx1.max(x1); ry0 = ry0.min(by0); ry1 = ry1.max(by1); }
        self.vsub.last = Some(VsubInfo { kind: lay.kind, page, pages: pl.pages.len(), runs: pl.key.1.clone(), rect: (rx0, ry0, rx1, ry1), speaker: a.speaker });
    }
}

/// What the strip shows: (has a strip, occupied column ranges (inclusive) in the strip rows, first occupied row). A column is occupied when any of its
/// pixels is not black; ranges closer than 4 columns are one portrait frame. The picture reaches the bottom of the screen (no strip) when the
/// rows above or below the strip are not mostly black.
fn scan_strip(fb: &[u8]) -> (bool, Vec<(u32, u32)>, usize) {
    let nb = |x0: usize, x1: usize, y0: usize, y1: usize| -> f64 {
        let mut n = 0usize;
        for y in y0..y1 { for x in x0..x1 { let k = (y * W + x) * 4; if fb[k] != 0 || fb[k + 1] != 0 || fb[k + 2] != 0 { n += 1; } } }
        100.0 * n as f64 / ((x1 - x0) * (y1 - y0)) as f64
    };
    if nb(8, W - 8, 140, 148) > 10.0 || nb(8, W - 8, 216, 224) > 10.0 { return (false, vec![], PORTRAIT_TOP); }
    let lit = |x: usize, y: usize| { let k = (y * W + x) * 4; fb[k] != 0 || fb[k + 1] != 0 || fb[k + 2] != 0 };
    let mut runs: Vec<(u32, u32)> = vec![];
    let mut top = STRIP_END;
    let (mut x, mut start, mut gap) = (0usize, None::<usize>, 0usize);
    while x <= W {
        let on = x < W && (PORTRAIT_TOP..STRIP_END).any(|y| lit(x, y));
        if on { if start.is_none() { start = Some(x); } gap = 0; }
        else if let Some(s) = start { gap += 1; if gap >= 4 || x >= W { runs.push((s as u32, (x - gap) as u32)); start = None; gap = 0; } }
        x += 1;
    }
    for &(x0, x1) in &runs { for y in PORTRAIT_TOP..STRIP_END { if (x0 as usize..=x1 as usize).any(|x| lit(x, y)) { top = top.min(y); break; } } }
    (true, runs, if top == STRIP_END { PORTRAIT_TOP } else { top })
}

/// Would the text of layout `lay` sit on a portrait of a strip that now looks like (`strip`, `runs`)?
fn conflicts(lay: &VLayout, strip: bool, runs: &[(u32, u32)]) -> bool {
    match lay.kind {
        VKind::Compact => strip,
        _ if !strip => true,
        VKind::Box => !runs.is_empty(),
        VKind::Lane => runs.iter().any(|&(x0, x1)| x1 >= lay.x_name.saturating_sub(vwrap::LANE_GAP / 2) && x0 <= lay.right),
        VKind::Band => false,
    }
}

fn name_of(names: &[(u8, Vec<u8>)], speaker: u8) -> Option<(u8, &Vec<u8>)> {
    if speaker < 2 { return None; }
    names.get(speaker as usize - 1).filter(|(_, n)| !n.is_empty()).map(|(c, n)| (*c, n))
}

/// Choose the layout for the strip and break the text into pages for it (`base` = frame the text starts at, the pages share `total - base`).
#[allow(clippy::too_many_arguments)]
fn place(font: &Font, names: &[(u8, Vec<u8>)], speaker: u8, text: &[u8], total: u32, base: u32, strip: bool, runs: Vec<(u32, u32)>, ptop: usize) -> Placed {
    let name = name_of(names, speaker);
    let lay = vwrap::layout_for(&runs, strip, name.is_some());
    let mut prefix = vec![];
    if lay.name_inline {
        if let Some((col, n)) = name { prefix.push(0xF8 | col.min(6)); prefix.extend_from_slice(n); prefix.push(0xF9); prefix.extend_from_slice(b": "); }
    }
    let pages = vwrap::paginate(text, &font.widths, &lay, vwrap::text_width(&font.widths, &prefix));
    let pb: Vec<Vec<u8>> = pages.iter().map(|p| p.join(&i::NL)).collect();
    let starts: Vec<u32> = page_starts(&pb, total.saturating_sub(base).max(1)).into_iter().map(|s| s + base).collect();
    let band_lines = pages.iter().map(|p| p.len()).max().unwrap_or(1).max(1);
    Placed { lay, key: (strip, runs), ptop, pages, starts, band_lines, prefix }
}

/// Draw one page of a placed subtitle into `win` with the layout of a pt-BR message (10 px pitch).
fn draw_page(win: &mut TextWindow, font: &Font, names: &[(u8, Vec<u8>)], speaker: u8, pl: &Placed, page: usize) {
    let run = |win: &mut TextWindow, s: Vec<u8>| { win.start_with(&s, true); while win.busy { win.tick(font); } };
    let lay = &pl.lay;
    let lines = &pl.pages[page.min(pl.pages.len() - 1)];
    let named = name_of(names, speaker);
    let mut y = if lay.name_inline { 2 + 10 * (pl.band_lines - lines.len()) } else { i::first_line_y(if named.is_some() { speaker as u32 } else { 0 }, true) as usize };
    if !lay.name_inline {
        if let Some((col, name)) = named {
            let mut s = vec![0xFC, 0x00, 0xF9, 0x01, 0xFB, lay.x_name as u8, 0x00, 0xFE, col];
            s.extend_from_slice(name);
            s.push(0xFF);
            run(win, s);
        }
    }
    let mut colour = 1u8;
    for (k, line) in lines.iter().enumerate() {
        let mut s = vec![0xFC, 0x00, 0xF9, 0x01, 0xFE, colour, 0xFB, lay.x_text as u8, y as u8];
        let pre: &[u8] = if k == 0 && lay.name_inline { &pl.prefix } else { &[] };
        for &c in pre.iter().chain(line.iter()) {
            match c {
                0xF8..=0xFE => { colour = c & 7; s.push(0xFE); s.push(colour); }
                0xF4 | i::PAGE => {}
                _ => s.push(c),
            }
        }
        s.push(0xFF);
        run(win, s);
        y += i::TALL_PITCH as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_with(boxes: &[(usize, usize)]) -> Vec<u8> {
        let mut fb = vec![0u8; W * H * 4];
        for p in fb.chunks_mut(4) { p[3] = 255; }
        for &(x0, x1) in boxes { for y in PORTRAIT_TOP..PORTRAIT_TOP + 64 { for x in x0..x1 { let k = (y * W + x) * 4; fb[k] = 200; fb[k + 1] = 200; fb[k + 2] = 200; } } }
        fb
    }

    #[test]
    fn the_strip_scan_finds_the_portrait_frames() {
        let (strip, runs, top) = scan_strip(&frame_with(&[(24, 88), (168, 232)]));
        assert!(strip);
        assert_eq!(runs, vec![(24, 87), (168, 231)]);
        assert_eq!(top, PORTRAIT_TOP);
        assert_eq!(scan_strip(&frame_with(&[])).1, vec![]);
        // a portrait at the very edge of the screen ends the run at the last column
        assert_eq!(scan_strip(&frame_with(&[(192, 256)])).1, vec![(192, 255)]);
        // a picture that reaches the bottom of the screen is not a strip
        let mut fb = frame_with(&[]);
        for y in 140..224 { for x in 8..W - 8 { let k = (y * W + x) * 4; fb[k] = 90; } }
        assert!(!scan_strip(&fb).0);
    }

    #[test]
    fn a_new_portrait_on_the_text_forces_a_new_layout_a_vanished_one_waits() {
        let lane = vwrap::layout_for(&[(24, 87)], true, true);
        assert_eq!(lane.kind, VKind::Lane);
        assert!(!conflicts(&lane, true, &[(24, 87)]));
        assert!(conflicts(&lane, true, &[(24, 87), (96, 159)]), "a portrait appeared in the lane");
        assert!(!conflicts(&lane, true, &[]), "a portrait left: the lane stays until the next page");
        let bx = vwrap::layout_for(&[], true, true);
        assert!(conflicts(&bx, true, &[(24, 87)]));
        let band = vwrap::layout_for(&[(24, 87), (168, 231)], true, true);
        assert!(!conflicts(&band, true, &[(24, 87), (96, 159), (168, 231)]) && !conflicts(&band, true, &[]));
        assert!(conflicts(&band, false, &[]) && conflicts(&vwrap::layout_for(&[], false, true), true, &[]));
    }
}
