//! Touch description of the game for the web front end's "touch the game" mode (docs/re/web.md section 11).
//!
//! The front end draws no pad in that mode: it turns taps on the picture into the pad presses the game already understands, and for that it
//! needs to know what the game waits for and where the tappable things are on the 256x224 screen. `Engine::touch_ui` answers both, read-only
//! (it changes no game state and is not part of a snapshot). Rectangles are in game pixels (the framebuffer's coordinates).
//!
//! Kinds (the string the front end switches on):
//! * `none`      nothing interactive (a scene plays, a file loads): a tap sends the confirm button, which is harmless there
//! * `message`   a message box (printing or waiting for the confirm button): a tap sends confirm
//! * `menu`      the command list menu: one rectangle per visible line, `cur` = the line under the cursor; `pointer` = the free view of the dark room
//! * `entry`     the phone keypad / alphabet keyboard: one rectangle per key (`id` = grid cell), `cur` = cell under the cursor
//! * `shoot`     a shooting stage: taps aim and fire (the light gun path); `drawn` = the blaster is drawn
//! * `title`     the title menu (NEW GAME / CONTINUE), `slots` the save-slot screen, `options` the settings screen, `calibrate` the gun screen
//! * `skip`      a part that only waits for Start (intro, a `wait for Start` sequencer step): the page shows a Start button
//! `ready` is true when the game accepts a key right now (a menu is not redrawing, a keyboard is not animating); the front end waits for it
//! between two presses, because a press made while the game is busy is lost.
use crate::game::{Engine, Top, VmPhase};
use sncore::actions::{btn, MenuLine};

/// One tappable rectangle in game pixels. `id`: menu line / grid cell / slot / row, depending on the kind.
/// `kind`: 0 = an item, 1 = the "page up" arrow line of a paged menu, 2 = the "page down" arrow line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TRect { pub x: i32, pub y: i32, pub w: i32, pub h: i32, pub id: i32, pub kind: u8 }

#[derive(Clone, Debug, Default)]
pub struct TouchUi {
    pub kind: &'static str,
    pub ready: bool,
    /// the Back button (cancel / backspace) does something now
    pub back: bool,
    /// the Start button does something now (`label` names it for the page: `start` or `ok`)
    pub start: bool,
    pub label: &'static str,
    pub cur: i32,
    pub page: i32,
    pub rects: Vec<TRect>,
    /// physical pad bits (`btn::A/B/C`) that give the game's logical confirm / cancel / extra under the button configuration of the options screen
    pub confirm: u8,
    pub cancel: u8,
    pub extra: u8,
    /// menu: the free view (pointer mode of the dark room) is on, a drag moves it
    pub pointer: bool,
    /// shoot: the blaster is drawn
    pub drawn: bool,
    /// shoot: the stage is in its playing phase (not the countdown or the results)
    pub playing: bool,
}

impl TouchUi {
    pub fn new(kind: &'static str) -> Self { TouchUi { kind, label: "start", ..Default::default() } }

    pub fn json(&self) -> String {
        let mut s = format!(
            "{{\"kind\":\"{}\",\"ready\":{},\"back\":{},\"start\":{},\"label\":\"{}\",\"cur\":{},\"page\":{},\"confirm\":{},\"cancel\":{},\"extra\":{},\"pointer\":{},\"drawn\":{},\"playing\":{},\"rects\":[",
            self.kind, self.ready, self.back, self.start, self.label, self.cur, self.page, self.confirm, self.cancel, self.extra, self.pointer, self.drawn, self.playing);
        for (i, r) in self.rects.iter().enumerate() {
            if i > 0 { s.push(','); }
            s += &format!("{{\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"id\":{},\"k\":{}}}", r.x, r.y, r.w, r.h, r.id, r.kind);
        }
        s.push_str("]}");
        s
    }
}

/// Screen y of the first command-menu line (`0x8F`, the bar object's Y of S:0x10FB0) and the line pitch; the portrait layout (`$7893`) sits 16 px lower.
pub const MENU_Y0: i32 = 0x8F;
pub const MENU_PITCH: i32 = 9;

impl Engine {
    /// Physical pad bits that produce the logical confirm (bit 5), cancel (bit 4) and extra (bit 6) buttons (inverse of the S:0xDFF6 remap).
    pub fn touch_roles(&self) -> (u8, u8, u8) {
        let Some(i) = self.input.as_ref() else { return (btn::C, btn::B, btn::A) };
        let c = i.cfg_table[i.config.min(5)];
        let phys = |bit: u8| -> u8 { if c[0] == bit { btn::A } else if c[1] == bit { btn::B } else if c[2] == bit { btn::C } else { 0 } };
        (phys(5), phys(4), phys(6))
    }

    /// What the game waits for and where to tap, see the module docs.
    pub fn touch_ui(&self) -> TouchUi {
        let mut u = match self.top {
            Top::Title | Top::Intro => self.module.as_ref().and_then(|m| m.touch(self)).unwrap_or_else(|| TouchUi::new("none")),
            Top::Game => self.game_touch(),
            _ => TouchUi::new("none"),
        };
        if matches!(self.top, Top::Game) {
            let (c, b, a) = self.touch_roles();
            u.confirm = c; u.cancel = b; u.extra = a;
        } else { u.confirm = btn::C; u.cancel = btn::B; u.extra = btn::A; } // the modules read the raw buttons
        u
    }

    fn game_touch(&self) -> TouchUi {
        let cmd = self.queue.get(self.qi).map(|a| a.cmd);
        if cmd == Some(0x18) {
            let mut u = TouchUi::new("shoot");
            let w = |a: u32| self.regs.get(a);
            u.drawn = w(0x78F8) & 1 != 0;
            u.playing = w(0x78E8) < 4;
            u.ready = true;
            return u;
        }
        if let Some(ent) = self.ix.entry.as_ref() {
            let mut u = TouchUi::new("entry");
            if ent.active() && self.ix.closing.is_none() {
                u.ready = ent.ready();
                u.rects = ent.touch_cells(self);
                u.cur = ent.cell() as i32;
                u.back = true;
                u.start = self.regs.get(0x971E) == 0;
                u.label = "ok";
            }
            return u;
        }
        if self.phase == VmPhase::Menu && self.ix.closing.is_none() {
            if let Some((m, _)) = self.menu.as_ref() {
                if self.menu_close.is_none() {
                    let mut u = TouchUi::new("menu");
                    u.ready = self.menu_mv.is_none();
                    u.cur = m.cursor as i32;
                    u.page = m.page as i32;
                    u.back = m.depth > 0;
                    u.pointer = self.regs.get(0x79FE) == 1 && self.vm.path.is_empty();
                    let y0 = MENU_Y0 + if self.regs.get_b(0x7893) != 0 { 0x10 } else { 0 };
                    for (i, l) in m.visible().into_iter().enumerate() {
                        let kind = match l { MenuLine::Item(_) => 0, MenuLine::Arrow { down: false } => 1, MenuLine::Arrow { down: true } => 2 };
                        u.rects.push(TRect { x: 8, y: y0 + MENU_PITCH * i as i32, w: 240, h: MENU_PITCH, id: i as i32, kind });
                    }
                    return u;
                }
            }
        }
        if self.msg.is_some() {
            let mut u = TouchUi::new("message");
            u.ready = self.wait_state() == 1;
            return u;
        }
        if cmd == Some(0x21) {
            let mut u = TouchUi::new("skip");
            u.start = true; u.ready = true;
            return u;
        }
        TouchUi::new("none")
    }

    /// Pad bits (one direction) that move the text-entry cursor one step toward grid cell `target` along the shortest route, 0 when it is there,
    /// not reachable, or no entry is open.
    pub fn touch_step(&self, target: u8) -> u8 {
        match self.ix.entry.as_ref() { Some(ent) if ent.active() => ent.step_toward(self, target), _ => 0 }
    }
}
