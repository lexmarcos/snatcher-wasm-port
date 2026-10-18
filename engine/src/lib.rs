//! Snatcher engine: reimplementation of the game logic (script VM, sequencer,
//! text/menus, scene composer) rendering into a 256x224 RGBA framebuffer (H32 mode).
//! The wasm boundary is the `Game` type below; everything else is plain Rust so
//! it can be unit-tested natively (`cargo test -p snengine`).
use wasm_bindgen::prelude::*;

pub mod fb;
pub mod game;
pub mod modules;
pub mod menus;
pub mod shoot;
pub mod vsub;
pub mod importer;
pub mod snapshot;

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
    pub fn boot(&mut self) { self.e.boot() }
    /// Skip the title/intro and start a new game directly.
    pub fn new_game(&mut self) { self.e.new_game() }
    /// Provide a save block for CONTINUE (slot 1..4).
    pub fn set_save(&mut self, slot: usize, data: Vec<u8>) { if (1..=4).contains(&slot) { self.e.saves[slot - 1] = Some(data); } }
    /// Debug scene jump, e.g. "SP01:0" (developer scene-select script) or "SP06:0".
    pub fn debug_jump(&mut self, spec: &str) -> bool {
        let Some((f, rest)) = spec.split_once(':') else { return false };
        // optional third part `:rrrr` (hex): show the scene resource (DATA file index and image, like the SCENE node) before the script runs
        let (pc, scene) = match rest.split_once(':') { Some((pc, sc)) => (pc, u16::from_str_radix(sc.trim(), 16).ok()), None => (rest, None) };
        let f = f.trim().to_ascii_uppercase();
        let Some(num) = f.strip_prefix("SP").and_then(|n| n.parse::<u16>().ok()) else { return false };
        let Ok(pc) = u16::from_str_radix(pc.trim(), 16) else { return false };
        self.e.jump(57 + num, pc);
        if let Some(r) = scene {
            use sncore::script::vm::Action;
            self.e.queue.push(Action { cmd: 0x0F, args: vec![] });
            self.e.queue.push(Action { cmd: 0x11, args: vec![2, 0x8000, r >> 8] });
            self.e.queue.push(Action { cmd: 0x12, args: vec![r & 0xFF] });
        }
        true
    }
    /// Voice clip lengths in frames (from the audio manifest), so waits match the audio.
    pub fn set_voice_len(&mut self, id: u16, frames: u32) { self.e.voice_len.insert(id, frames); }
    /// Light gun (mouse) state: `present` = a gun is plugged into port 2 (the game uses it when
    /// the option is on), `x`/`y` = screen pixels, `trigger`/`aux` = buttons (bit5/bit6 of $7985).
    pub fn set_gun(&mut self, present: bool, x: i32, y: i32, trigger: bool, aux: bool) {
        let ix = &mut self.e.ix;
        ix.gun_present = present; ix.gun_x = x; ix.gun_y = y;
        ix.gun_buttons = (trigger as u8) << 5 | (aux as u8) << 6;
    }
    /// PCM effect length in frames for (bank, effect id) – the shooting code waits for the last effect (`$1511E` bit 3).
    pub fn set_sfx_len(&mut self, bank: u16, id: u16, frames: u32, chan: u8) { self.e.ix.sfx_len.insert((bank, id), (frames, chan)); }
    /// FM sound duration in frames; `loops` = true for sounds with a loop (never "done").
    pub fn set_fm_len(&mut self, id: u16, frames: u32, loops: bool) { self.e.fm_len.insert(id, if loops { None } else { Some(frames) }); }
    /// Button prompts (docs/re/web.md): `style` 0 = original letters (default), 1 Xbox, 2 PlayStation, 3 Switch, 4 keyboard icons.
    /// `a`/`b`/`c`/`start` = id of the physical control that produces Genesis A/B/C/START (`sncore::prompt::CTL_*` / `KEY_*`).
    /// The text placeholders EC/EE/F0 (via the pad configuration `$78BA`) and the word START then print that control's icon.
    pub fn set_prompt(&mut self, style: u8, a: u8, b: u8, c: u8, start: u8) { self.e.prompt.set(style, a, b, c, start); }
    /// Change only the style (keeps the control ids).
    pub fn set_prompt_style(&mut self, style: u8) { self.e.prompt.style = sncore::prompt::Style::from_u8(style); }
    /// Language of the game text: "en" (default, original text) or "pt-BR". Applies from the next message or menu drawn.
    pub fn set_language(&mut self, lang: &str) { self.e.set_language(lang.eq_ignore_ascii_case("pt-br") || lang.eq_ignore_ascii_case("pt")); }
    /// Install the pt-BR translation pack (`snt i18n-build`, served as /i18n/pt-BR.pack). Returns the number of strings, or -1 if the pack is invalid.
    pub fn load_translation(&mut self, pack: Vec<u8>) -> i32 { self.e.load_translation(&pack).map(|n| n as i32).unwrap_or(-1) }
    /// Voice subtitles (docs/voice-subtitles.md): draw the pt-BR subtitle of a voice clip in the message box while it plays. Only has an effect in pt-BR
    /// with a pack that holds subtitles (`voice_subtitle_count`); English output is never changed. Applies from the next clip.
    pub fn set_voice_subtitles(&mut self, on: bool) { self.e.vsub.on = on; }
    /// Number of voice subtitles in the installed pack.
    pub fn voice_subtitle_count(&self) -> u32 { self.e.i18n.pack.voice.len() as u32 }
    /// Clip id whose subtitle is on screen now, -1 when there is none (debugging, screenshot tests).
    pub fn voice_subtitle_active(&self) -> i32 { self.e.vsub_active().map(|x| x as i32).unwrap_or(-1) }
    /// Layout of the subtitle drawn in the last frame: 0 none, 1 message box (no portrait), 2 lane next to the portrait, 3 band above two or more portraits,
    /// 4 compact band (no dialogue strip). Debugging and screenshot tests.
    pub fn voice_subtitle_layout(&self) -> i32 {
        use sncore::vwrap::VKind;
        self.e.vsub_info().map_or(0, |i| match i.kind { VKind::Box => 1, VKind::Lane => 2, VKind::Band => 3, VKind::Compact => 4 })
    }
    /// Seconds since the engine started the CD-DA track now playing (its CD clock, frames / 60), -1 when no track plays. The front end keeps the audio on it.
    pub fn cd_position(&self) -> f64 { self.e.cd_start.map_or(-1.0, |s| self.e.frame.saturating_sub(s) as f64 / 60.0) }
    /// Advance one 60 Hz frame. `pad` = Genesis bits S A C B R L D U (bit7..0).
    pub fn tick(&mut self, pad: u16) {
        self.e.tick(pad as u8);
        for o in self.e.out.drain(..) {
            self.events.push(match o {
                Out::Music { track } => format!("{{\"t\":\"music\",\"track\":{track}}}"),
                Out::MusicStop => "{\"t\":\"music_stop\"}".into(),
                Out::Voice { id } => format!("{{\"t\":\"voice\",\"id\":{id}}}"),
                Out::Pcm { id, kind } => format!("{{\"t\":\"pcm\",\"id\":{id},\"kind\":{kind}}}"),
                Out::Fm { id, music } => format!("{{\"t\":\"fm\",\"id\":{id},\"music\":{music}}}"),
                Out::PcmSfx { id, bank } => format!("{{\"t\":\"pcmsfx\",\"id\":{id},\"bank\":{bank}}}"),
                Out::Save { slot, data } => format!("{{\"t\":\"save\",\"slot\":{slot},\"data\":\"{}\"}}", b64(&data)),
                Out::Log(s) => format!("{{\"t\":\"log\",\"msg\":{:?}}}", s),
                Out::MusicAt { track, at_ms } => format!("{{\"t\":\"music\",\"track\":{track},\"at\":{}}}", at_ms as f64 / 1000.0),
                Out::FmAt { id, music, at_ms } => format!("{{\"t\":\"fm\",\"id\":{id},\"music\":{music},\"at\":{}}}", at_ms as f64 / 1000.0),
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
