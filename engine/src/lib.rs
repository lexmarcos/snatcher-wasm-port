//! Snatcher engine: reimplementation of the game logic (script VM, sequencer,
//! scene composer) that renders into a 320x224 RGBA framebuffer.
//! The wasm boundary is the `Game` type below; everything else is plain Rust so
//! it can be unit-tested natively (`cargo test -p snengine`).
use wasm_bindgen::prelude::*;

pub mod fb;
pub mod vdp;

pub const W: usize = 320;
pub const H: usize = 224;

/// Genesis pad bits used by `Game::tick`.
pub mod pad {
    pub const UP: u16 = 1 << 0;
    pub const DOWN: u16 = 1 << 1;
    pub const LEFT: u16 = 1 << 2;
    pub const RIGHT: u16 = 1 << 3;
    pub const B: u16 = 1 << 4;
    pub const C: u16 = 1 << 5;
    pub const A: u16 = 1 << 6;
    pub const START: u16 = 1 << 7;
}

#[wasm_bindgen]
pub struct Game {
    fb: fb::Framebuffer,
    files: std::collections::HashMap<String, Vec<u8>>,
    frame: u64,
    pad: u16,
    events: Vec<String>,
}

#[wasm_bindgen]
impl Game {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Game {
        Game { fb: fb::Framebuffer::new(), files: Default::default(), frame: 0, pad: 0, events: vec![] }
    }
    /// Provide a raw game file (e.g. "SP06.BIN") read from the user's disc extraction.
    pub fn load_file(&mut self, name: &str, data: Vec<u8>) {
        self.files.insert(name.to_ascii_uppercase(), data);
    }
    pub fn has_file(&self, name: &str) -> bool { self.files.contains_key(&name.to_ascii_uppercase()) }
    /// Advance one 60 Hz frame with the current pad state.
    pub fn tick(&mut self, pad: u16) {
        self.pad = pad;
        self.frame += 1;
        self.fb.test_pattern(self.frame as u32, pad);
    }
    pub fn width(&self) -> usize { W }
    pub fn height(&self) -> usize { H }
    /// Pointer to RGBA8 pixels (W*H*4) in wasm memory.
    pub fn framebuffer_ptr(&self) -> *const u8 { self.fb.px.as_ptr() }
    /// Drain pending audio/system events as a JSON array string.
    pub fn take_events(&mut self) -> String {
        let s = format!("[{}]", self.events.join(","));
        self.events.clear();
        s
    }
    pub fn debug_info(&self) -> String { format!("{{\"frame\":{},\"pad\":{}}}", self.frame, self.pad) }
}

impl Default for Game { fn default() -> Self { Self::new() } }
