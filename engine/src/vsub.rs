//! Voice subtitles (docs/voice-subtitles.md): while a voice clip plays, its pt-BR subtitle is drawn in the empty message box.
//!
//! The original game never shows text with a voice clip (the box stays dark and empty, checked on the real BIOS). The subtitle is a pure
//! overlay on the finished frame: it changes no game state, no script flow and no timing, and with the option off (or in English) nothing is
//! drawn, so the output stays byte-identical. The text is rendered by the game's own glyph renderer (`TextWindow`, 10 px pitch layout of pt-BR
//! dialogue, same positions as a message with / without a name) into a private 256x64 bitmap and blitted onto the dialogue strip (screen y 150..214,
//! where the message window is) over a black backing, which hides the portrait icons exactly as a real message does. On the 3 % of clips
//! whose picture reaches the bottom of the screen (no strip) the text sits on a compact black band at the bottom edge.
//!
//! Timing: the clip's `voice_left` (length + 6 frames) is the display time; the pages (`{page}` of the markup) share it in proportion to
//! their text. A clip whose subtitle needs more reading time than the clip lasts keeps it up for a short hold (reading speed
//! `READ_CPS`, at most `HOLD_MAX` frames); the hold ends as soon as the script starts another voice or does anything that changes the picture.
use crate::game::{Engine, H, W};
use sncore::actions::{TextWindow, WIN_H, WIN_W};
use sncore::i18n as i;

pub use sncore::i18n::{voice_display_frames as display_frames, voice_glyph_count as glyph_count, voice_page_starts as page_starts, VOICE_HOLD_MAX as HOLD_MAX, VOICE_HOLD_MIN as HOLD_MIN, VOICE_READ_CPS as READ_CPS};
/// Screen row where the message window (256x64) starts.
pub const STRIP_Y: usize = 150;
/// Palette of the message window (the ROM palette of the message-box channel S:0x13622): index -> Mega Drive colour word.
const PALETTE: [u16; 8] = [0, 3822, 238, 2158, 3808, 3650, 1248, 0];

#[derive(Default)]
pub struct VoiceSubs {
    /// the front end's option "voice subtitles"
    pub on: bool,
    cur: Option<Active>,
    win: Option<TextWindow>,
}

struct Active {
    id: u16,
    speaker: u8,
    pages: Vec<Vec<u8>>,
    /// first frame (since the clip start) of every page
    starts: Vec<u32>,
    /// frames the subtitle stays up: the clip (`voice_left` at the start) plus the hold
    total: u32,
    t: u32,
    drawn: Option<usize>,
    /// bitmap rows used by the drawn page (the compact band uses them)
    rows: usize,
}

impl Engine {
    /// A voice clip starts (sequencer command 0x0D, `id` >= 36): look the subtitle up and start it.
    pub(crate) fn vsub_start(&mut self, id: u16, play: u32) {
        self.vsub.cur = None;
        if !self.vsub.on || !self.i18n.pt { return; }
        let Some(v) = self.i18n.pack.voice.get(&id) else { return };
        let pages: Vec<Vec<u8>> = v.text.split(|&c| c == i::PAGE).map(|p| p.to_vec()).collect();
        let chars: usize = pages.iter().map(|p| glyph_count(p)).sum();
        let (play, hold) = display_frames(play, chars);
        let total = play + hold;
        self.vsub.cur = Some(Active { id, speaker: v.speaker, starts: page_starts(&pages, total), pages, total, t: 0, drawn: None, rows: 0 });
    }

    /// The sequencer is about to run command `cmd`: commands that change the picture or draw a message end the subtitle.
    pub(crate) fn vsub_on_command(&mut self, cmd: u16) {
        if self.vsub.cur.is_some() && matches!(cmd, 0x02 | 0x08 | 0x0F | 0x11 | 0x12 | 0x15 | 0x16 | 0x18 | 0x1E | 0x1F | 0x20) { self.vsub.cur = None; }
    }

    /// Number of subtitles in the installed pack (tests).
    pub fn voice_subtitle_count_for_test(&self) -> usize { self.i18n.pack.voice.len() }

    /// Clip id of the subtitle on screen (tests, debugging).
    pub fn vsub_active(&self) -> Option<u16> { self.vsub.cur.as_ref().map(|a| a.id) }

    /// One frame of subtitle time (called once per `tick`).
    pub(crate) fn vsub_step(&mut self) {
        let end = match self.vsub.cur.as_mut() { Some(a) => { a.t += 1; a.t > a.total } None => return };
        if end || self.menu.is_some() || self.msg.is_some() || self.top != crate::game::Top::Game { self.vsub.cur = None; }
    }

    /// Draw the subtitle onto the finished frame `fb` (RGBA, W x H).
    pub(crate) fn vsub_overlay(&mut self, fb: &mut [u8]) {
        let Some(font) = self.font.as_ref() else { return };
        let Some(a) = self.vsub.cur.as_mut() else { return };
        let page = a.starts.iter().rposition(|&s| s <= a.t.saturating_sub(1)).unwrap_or(0);
        let win = self.vsub.win.get_or_insert_with(TextWindow::new);
        if a.drawn != Some(page) {
            win.clear();
            // the last cell of the bitmap (rows 56.., x 248..) is filled with colour 7 by `clear` (a nametable artefact); the subtitle never shows it
            for y in 56..64 { for x in 248..256 { win.pix[y * WIN_W + x] = 0; } }
            let names = if !self.i18n.pack.names.is_empty() { &self.i18n.names } else { &self.names };
            a.rows = draw_page(win, font, names, a.speaker, &a.pages[page]);
            a.drawn = Some(page);
        }
        // where the message window is: the dialogue strip, unless the picture reaches the bottom of the screen
        let nb = |x0: usize, x1: usize, y0: usize, y1: usize| -> f64 {
            let mut n = 0usize;
            for y in y0..y1 { for x in x0..x1 { let k = (y * W + x) * 4; if fb[k] != 0 || fb[k + 1] != 0 || fb[k + 2] != 0 { n += 1; } } }
            100.0 * n as f64 / ((x1 - x0) * (y1 - y0)) as f64
        };
        let full = nb(8, W - 8, 140, 148) > 10.0 || nb(8, W - 8, 216, 224) > 10.0;
        let (oy, by0) = if full {
            let band = (a.rows + 6).min(WIN_H);
            (H - band + 3, H - band)
        } else { (STRIP_Y, STRIP_Y) };
        for y in by0..H { for x in 0..W { let k = (y * W + x) * 4; fb[k] = 0; fb[k + 1] = 0; fb[k + 2] = 0; fb[k + 3] = 255; } }
        for y in 0..WIN_H {
            if oy + y >= H { break; }
            for x in 0..WIN_W {
                let c = win.pix[y * WIN_W + x] as usize & 15;
                if c == 0 { continue; }
                let rgba = sncore::gfx::md_rgba(*PALETTE.get(c).unwrap_or(&0));
                let k = ((oy + y) * W + x) * 4;
                fb[k..k + 4].copy_from_slice(&rgba);
            }
        }
    }
}

/// Draw one page of a subtitle into `win` with the layout of a pt-BR message (10 px pitch, x = 0x18). Returns the number of bitmap rows in use.
fn draw_page(win: &mut TextWindow, font: &sncore::actions::Font, names: &[(u8, Vec<u8>)], speaker: u8, page: &[u8]) -> usize {
    let run = |win: &mut TextWindow, s: Vec<u8>| { win.start_with(&s, true); while win.busy { win.tick(font); } };
    if speaker >= 2 {
        if let Some((col, name)) = names.get(speaker as usize - 1) {
            let mut s = vec![0xFC, 0x00, 0xF9, 0x01, 0xFB, 0x10, 0x00, 0xFE, *col];
            s.extend_from_slice(name);
            s.push(0xFF);
            run(win, s);
        }
    }
    let mut y = i::first_line_y(speaker as u32, true) as usize;
    let mut colour = 1u8;
    let mut rows = y;
    for line in page.split(|&c| c == i::NL) {
        let mut s = vec![0xFC, 0x00, 0xF9, 0x01, 0xFE, colour, 0xFB, i::X_SPEECH as u8, y as u8];
        for &c in line {
            match c {
                0xF8..=0xFE => { colour = c & 7; s.push(0xFE); s.push(colour); }
                0xF4 | i::PAGE => {}
                _ => s.push(c),
            }
        }
        s.push(0xFF);
        run(win, s);
        y += i::TALL_PITCH as usize;
        rows = y;
    }
    rows.min(WIN_H)
}
