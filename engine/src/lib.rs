//! Snatcher engine: reimplementation of the game logic (script VM, sequencer,
//! text/menus, scene composer) rendering into a 256x224 RGBA framebuffer (H32 mode).
//! The wasm boundary is the `Game` type below; everything else is plain Rust so
//! it can be unit-tested natively (`cargo test -p snengine`).
use wasm_bindgen::prelude::*;

pub mod fb;
pub mod game;

pub use game::{Engine, Out, H, W};

#[wasm_bindgen]
pub struct Game {
    e: Engine,
    events: Vec<String>,
}

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() { s.push(T[(n >> (18 - 6 * i)) as usize & 63] as char) } else { s.push('=') }
        }
    }
    s
}

#[wasm_bindgen]
impl Game {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Game { Game { e: Engine::new(), events: vec![] } }
    /// Provide a raw game file (e.g. "SP06.BIN") from the user's disc extraction.
    pub fn load_file(&mut self, name: &str, data: Vec<u8>) { self.e.load_file(name, data) }
    pub fn has_file(&self, name: &str) -> bool { self.e.files.contains_key(&name.to_ascii_uppercase()) }
    /// Start the game (currently: new game straight into the scenario, SP06:0).
    pub fn boot(&mut self) { self.e.new_game() }
    /// Debug scene jump, e.g. "SP01:0" (developer scene-select script) or "SP06:0".
    pub fn debug_jump(&mut self, spec: &str) -> bool {
        let Some((f, pc)) = spec.split_once(':') else { return false };
        let f = f.trim().to_ascii_uppercase();
        let Some(num) = f.strip_prefix("SP").and_then(|n| n.parse::<u16>().ok()) else { return false };
        let Ok(pc) = u16::from_str_radix(pc.trim(), 16) else { return false };
        self.e.jump(57 + num, pc);
        true
    }
    /// Voice clip lengths in frames (from the audio manifest), so waits match the audio.
    pub fn set_voice_len(&mut self, id: u16, frames: u32) { self.e.voice_len.insert(id, frames); }
    /// Advance one 60 Hz frame. `pad` = Genesis bits S A C B R L D U (bit7..0).
    pub fn tick(&mut self, pad: u16) {
        self.e.tick(pad as u8);
        for o in self.e.out.drain(..) {
            self.events.push(match o {
                Out::Music { track } => format!("{{\"t\":\"music\",\"track\":{track}}}"),
                Out::MusicStop => "{\"t\":\"music_stop\"}".into(),
                Out::Voice { id } => format!("{{\"t\":\"voice\",\"id\":{id}}}"),
                Out::Pcm { id, kind } => format!("{{\"t\":\"pcm\",\"id\":{id},\"kind\":{kind}}}"),
                Out::Save { slot, data } => format!("{{\"t\":\"save\",\"slot\":{slot},\"data\":\"{}\"}}", b64(&data)),
                Out::Log(s) => format!("{{\"t\":\"log\",\"msg\":{:?}}}", s),
            });
        }
    }
    pub fn width(&self) -> usize { W }
    pub fn height(&self) -> usize { H }
    /// Pointer to RGBA8 pixels (W*H*4) in wasm memory.
    pub fn framebuffer_ptr(&self) -> *const u8 { self.e.fb.as_ptr() }
    /// Drain pending audio/system events as a JSON array string.
    pub fn take_events(&mut self) -> String {
        let s = format!("[{}]", self.events.join(","));
        self.events.clear();
        s
    }
    pub fn debug_info(&self) -> String { self.e.debug_info() }
}

impl Default for Game { fn default() -> Self { Self::new() } }
