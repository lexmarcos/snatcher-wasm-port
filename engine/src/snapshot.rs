//! Snapshots of the running game (autosave, docs/re/web.md section 9).
//!
//! The whole engine state that a safe point can have is written to a byte vector and read back into a fresh engine that has the same disc
//! files loaded. The state is exact: running N frames after a restore gives the same frames and the same state as running on without
//! one (tests in `engine/tests/snapshot.rs`). The front end owns the audio, so a restored engine also reports what was playing
//! (`Out::MusicAt` / `Out::FmAt`) and the front end starts it again at that position.
//!
//! A *safe point* is where the engine waits for the player and nothing that cannot be restored is in flight: the command menu, or a message
//! box that waits for the confirm button. Not at a safe point: a voice clip or PCM effect playing (the front end cannot resume a clip),
//! a finite FM jingle or effect playing, the text-entry menus, shooting, a window or scene change in progress, the title, the intro and the ending.
use sncore::gfx;
use sncore::snap::{Reader, Snap, SnapResult, Writer};

use crate::game::{Engine, Out, Top, VmPhase, FILE_NAMES};

/// Bump when the meaning of the stored state changes in a way the layout fingerprint cannot see.
pub const FORMAT: u16 = 1;
const MAGIC: &[u8; 4] = b"SNAS";
/// Fixed part before the payload: magic, format, layout, disc, language flag, pack hash, kind, script, scene, play time (3), frame, payload length.
const HEADER_LEN: usize = 4 + 2 + 8 + 8 + 1 + 8 + 1 + 2 + 2 + 3 + 8 + 4;

/// Why a snapshot was not taken or not used. `code()` is what the front end switches on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapError {
    /// not a snapshot, or a snapshot format this build does not know
    Format,
    /// made by an engine build with another state layout
    Engine,
    /// made with other disc files
    Disc,
    /// made in the other language, or with another translation pack
    Language,
    /// a game file the snapshot needs is not loaded
    Missing(String),
    /// damaged or cut short
    Corrupt(String),
    /// the engine is not at a safe point
    NotSafe,
}

impl SnapError {
    pub fn code(&self) -> &'static str {
        match self {
            SnapError::Format => "format", SnapError::Engine => "engine", SnapError::Disc => "disc", SnapError::Language => "lang",
            SnapError::Missing(_) => "missing", SnapError::Corrupt(_) => "corrupt", SnapError::NotSafe => "notsafe",
        }
    }
    pub fn detail(&self) -> String {
        match self { SnapError::Missing(s) | SnapError::Corrupt(s) => s.clone(), _ => String::new() }
    }
}

/// What the header says about a snapshot (readable without restoring it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub format: u16,
    pub layout: u64,
    pub disc: u64,
    pub pt: bool,
    pub pack: u64,
    /// 1 = command menu, 2 = message box waiting for the confirm button
    pub kind: u8,
    pub script: u16,
    pub scene: u16,
    /// game clock HH MM SS (BCD)
    pub play: [u8; 3],
    pub frame: u64,
}

impl Info {
    /// Act of the story (1-3) by the script file; 0 when it is none of the three (the developer scene menu).
    pub fn act(&self) -> u8 { act_of(self.script) }
    pub fn json(&self) -> String {
        format!("{{\"format\":{},\"kind\":{},\"script\":{},\"scene\":{},\"act\":{},\"pt\":{},\"play\":\"{:02X}:{:02X}:{:02X}\",\"frame\":{}}}",
            self.format, self.kind, self.script, self.scene, self.act(), self.pt, self.play[0], self.play[1], self.play[2], self.frame)
    }
}

/// Act of a script file id (SP06-SP19 act 1, SP20-SP35 act 2, SP36-SP38 act 3; docs/voice-subtitles.md).
pub fn act_of(script_id: u16) -> u8 {
    match sncore::script::sp_number(script_id) { Some(6..=19) => 1, Some(20..=35) => 2, Some(36..=38) => 3, _ => 0 }
}

fn fnv(h: &mut u64, b: &[u8]) { for &x in b { *h = (*h ^ x as u64).wrapping_mul(0x0000_0100_0000_01B3); } }

// The fields that make up the engine state. Every `Engine` field is named here, either stored or skipped on purpose (the compiler checks).
// Skipped: the disc files and what is made from them (`files`, `mem`, `font`, `names`, `script`), what the front end sets (`prompt`, `voice_len`,
// `fm_len`, `saves`, `input`'s button table, the gun readings, `vsub.on`, the language), the frame buffer (drawn again after a restore), pending
// events (`out`), debug switches, and what is fixed at a safe point (`top`, `module`, `mod_delay`, `reboot_in`, the text-entry and shooting states).
macro_rules! engine_state {
    ($($f:ident : $t:ty),* $(,)? ; skip $($s:ident),* $(,)?) => {
        /// The stored fields, decoded.
        struct State { $($f: $t),* }
        fn put_state(e: &Engine, w: &mut Writer) {
            let Engine { $($f,)* $($s: _,)* } = e;
            $( w.tag(stringify!($f)); Snap::put($f, w); )*
        }
        fn get_state(r: &mut Reader) -> SnapResult<State> { Ok(State { $($f: Snap::get(r)?),* }) }
        fn apply_state(s: State, e: &mut Engine) { let State { $($f),* } = s; $( e.$f = $f; )* }
    };
}

engine_state!(
    disp: gfx::Display, vm: sncore::script::vm::Vm, phase: VmPhase, queue: Vec<sncore::script::vm::Action>, qi: usize, seq_step: u16, seq_timer: u32,
    msg: Option<sncore::actions::MessageBox>, text: sncore::actions::TextWindow, menu: Option<(sncore::actions::TextMenu, Vec<u16>)>,
    menu_close: Option<(sncore::actions::MenuResult, u32)>, menu_open_t: u32, sub_stall: u32, menu_open_ready: u32, menu_mv: Option<(u32, usize, usize)>,
    e06e_hist: [u8; 2], menu_in: (u8, u8, u8), menu_in_cur: (u8, u8, u8), e061_hist: [u8; 2], regs: crate::game::Regs, data_file: Option<u16>,
    script_file: Option<u16>, frame: u64, voice_left: u32, sfx_left: u32, voice_music_left: u32, cd_start: Option<u64>, pcm_bank: u16, text_dirty: bool,
    voice_age: u32, intro_part: u8, fm_music: Option<(u16, u64)>, fm_loops: Vec<u16>;
    skip files, mem, font, names, input, prompt, script, fid_rng, out, fb, trace, top, module, mod_delay, voice_len, fm_len, saves, module_factory,
    ix, reboot_in, i18n, vsub);

/// Sub-CPU addresses of the game clock (HH MM SS FF, BCD; `play_time_tick`).
const CLOCK: u32 = 0x79EA;

impl Engine {
    /// 0 = not a safe point, 1 = the command menu waits for a key, 2 = a message box waits for the confirm button.
    pub fn safe_point(&self) -> u8 { self.safe_check().unwrap_or(0) }

    /// Like `safe_point`, with the reason when it is not one (debug overlay, tests).
    pub fn safe_check(&self) -> Result<u8, &'static str> {
        if self.top != Top::Game { return Err("not in the game"); }
        if self.module.is_some() || self.mod_delay != 0 || self.reboot_in != 0 { return Err("module or restart running"); }
        if self.sub_stall != 0 { return Err("sub CPU busy"); }
        if self.vm.state != 2 || self.script.is_empty() || self.script_file.is_none() || self.data_file.is_none() { return Err("script not running"); }
        // sound in flight that a front end cannot pick up again: a voice clip, a PCM effect, a finite FM jingle or effect (loops are started again)
        if self.voice_left != 0 { return Err("voice clip playing"); }
        if self.ix.pcm_busy || self.ix.pcm_ch_until != [0, 0] { return Err("PCM effect playing"); }
        if self.sfx_left != 0 && self.sfx_left != u32::MAX { return Err("FM effect playing"); }
        if self.voice_music_left != 0 && self.voice_music_left != u32::MAX { return Err("FM jingle playing"); }
        // text entry, shooting, pointer moves, a closing window; a picture or window change still running; a subtitle on screen
        if self.ix.shoot.is_some() { return Err("shooting"); }
        if self.ix.entry.is_some() || self.ix.closing.is_some() || self.ix.ptr_moving { return Err("text entry or pointer"); }
        if self.disp.sub.vram_list.is_some() || self.disp.sub.f7887 != 0 { return Err("picture loading"); }
        if self.vsub_active().is_some() { return Err("subtitle on screen"); }
        if self.phase == VmPhase::Menu && self.menu.is_some() && self.menu_close.is_none() && self.menu_mv.is_none() && !self.busy() { return Ok(1); }
        if self.wait_state() == 1 { return Ok(2); }
        Err("not waiting for the player")
    }

    /// FNV hash of the layout of the stored state (type and field names in order) and the format number: another engine build that changed
    /// the state gets another value, so its snapshots are refused rather than misread.
    pub fn layout_fingerprint() -> u64 {
        static FP: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
        *FP.get_or_init(|| {
            let mut w = Writer::fingerprint();
            let e = Engine::new();
            e.put_payload(&mut w);
            let mut h = w.fp.unwrap();
            fnv(&mut h, &FORMAT.to_be_bytes());
            h
        })
    }

    /// Hash of the disc files that are loaded: names, sizes and a sample of every file (the first and last 256 bytes and one byte in 4096).
    /// Two dumps of the same disc give the same value; another disc, region or a modified file almost surely not.
    pub fn disc_digest(&self) -> u64 {
        let mut h = 0xCBF2_9CE4_8422_2325u64;
        for n in FILE_NAMES.iter() {
            let Some(d) = self.files.get(*n) else { continue };
            fnv(&mut h, n.as_bytes());
            fnv(&mut h, &(d.len() as u32).to_be_bytes());
            fnv(&mut h, &d[..d.len().min(256)]);
            fnv(&mut h, &d[d.len().saturating_sub(256)..]);
            let mut i = 256;
            while i < d.len() { fnv(&mut h, &d[i..i + 1]); i += 4096; }
        }
        h
    }

    /// The header values of the engine as it is now.
    pub fn snapshot_info(&self) -> Info {
        let c = |a: u32| self.regs.get_b(a);
        Info {
            format: FORMAT, layout: Self::layout_fingerprint(), disc: self.disc_digest(), pt: self.i18n.pt,
            pack: if self.i18n.pt { self.i18n.pack_hash } else { 0 }, kind: self.safe_point(),
            script: self.vm.script_id, scene: self.vm.scene, play: [c(CLOCK), c(CLOCK + 1), c(CLOCK + 2)], frame: self.frame,
        }
    }

    /// The stored state as bytes, whether or not this is a safe point (tests compare two engines with it).
    pub fn state_bytes(&self) -> Vec<u8> { let mut w = Writer::new(); self.put_payload(&mut w); w.buf }

    fn put_payload(&self, w: &mut Writer) {
        put_state(self, w);
        w.tag("input");
        match &self.input { Some(i) => { w.buf.push(1); i.put_state(w); } None => w.buf.push(0) }
        w.tag("ix");
        self.ix.put(w);
        // regions mapped besides SUBCODE (reloaded from the file) and the DATA file (reloaded and patched)
        w.tag("extra");
        let extra: Vec<(u32, Vec<u8>)> = self.mem.regions().iter().filter(|r| r.0 != gfx::SUBCODE_BASE && r.0 != gfx::DATA_BASE).cloned().collect();
        extra.put(w);
    }

    /// The state as bytes. Only at a safe point.
    pub fn snapshot(&self) -> Result<Vec<u8>, SnapError> {
        let kind = self.safe_point();
        if kind == 0 { return Err(SnapError::NotSafe); }
        let info = self.snapshot_info();
        let mut p = Writer::new();
        self.put_payload(&mut p);
        let mut out = Vec::with_capacity(HEADER_LEN + p.buf.len() + 8);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&info.format.to_be_bytes());
        out.extend_from_slice(&info.layout.to_be_bytes());
        out.extend_from_slice(&info.disc.to_be_bytes());
        out.push(info.pt as u8);
        out.extend_from_slice(&info.pack.to_be_bytes());
        out.push(info.kind);
        out.extend_from_slice(&info.script.to_be_bytes());
        out.extend_from_slice(&info.scene.to_be_bytes());
        out.extend_from_slice(&info.play);
        out.extend_from_slice(&info.frame.to_be_bytes());
        out.extend_from_slice(&(p.buf.len() as u32).to_be_bytes());
        out.extend_from_slice(&p.buf);
        let mut h = 0xCBF2_9CE4_8422_2325u64;
        fnv(&mut h, &out);
        out.extend_from_slice(&h.to_be_bytes());
        Ok(out)
    }

    /// Read the header of a snapshot and check its checksum (no engine needed).
    pub fn snapshot_header(data: &[u8]) -> Result<Info, SnapError> {
        if data.len() < HEADER_LEN + 8 || &data[..4] != MAGIC { return Err(SnapError::Format); }
        let u16_at = |o: usize| u16::from_be_bytes([data[o], data[o + 1]]);
        let u64_at = |o: usize| u64::from_be_bytes(data[o..o + 8].try_into().unwrap());
        let format = u16_at(4);
        if format != FORMAT { return Err(SnapError::Format); }
        let plen = u32::from_be_bytes(data[HEADER_LEN - 4..HEADER_LEN].try_into().unwrap()) as usize;
        if data.len() != HEADER_LEN + plen + 8 { return Err(SnapError::Corrupt("length".into())); }
        let mut h = 0xCBF2_9CE4_8422_2325u64;
        fnv(&mut h, &data[..HEADER_LEN + plen]);
        if h != u64_at(HEADER_LEN + plen) { return Err(SnapError::Corrupt("checksum".into())); }
        Ok(Info {
            format, layout: u64_at(6), disc: u64_at(14), pt: data[22] != 0, pack: u64_at(23), kind: data[31],
            script: u16_at(32), scene: u16_at(34), play: [data[36], data[37], data[38]], frame: u64_at(39),
        })
    }

    /// Does this build understand the snapshot (format and layout)? Needs no disc files.
    pub fn snapshot_compatible(data: &[u8]) -> Result<Info, SnapError> {
        let i = Self::snapshot_header(data)?;
        if i.layout != Self::layout_fingerprint() { return Err(SnapError::Engine); }
        Ok(i)
    }

    /// Replace the running game by a snapshot. The engine must have the disc files loaded (and the language and translation pack set as they
    /// were); on an error nothing has changed. Pushes the audio events that start the music again.
    pub fn restore(&mut self, data: &[u8]) -> Result<Info, SnapError> {
        let info = Self::snapshot_compatible(data)?;
        if info.disc != self.disc_digest() { return Err(SnapError::Disc); }
        if info.pt != self.i18n.pt || (info.pt && info.pack != self.i18n.pack_hash) { return Err(SnapError::Language); }
        let payload = &data[HEADER_LEN..data.len() - 8];
        let mut r = Reader::new(payload);
        let st = get_state(&mut r).map_err(SnapError::Corrupt)?;
        let input = match r.raw(1).map_err(SnapError::Corrupt)?[0] {
            0 => None,
            1 => Some(sncore::actions::Input::get_state(&mut r).map_err(SnapError::Corrupt)?),
            _ => return Err(SnapError::Corrupt("input".into())),
        };
        let mut ix: crate::menus::Ix = Snap::get(&mut r).map_err(SnapError::Corrupt)?;
        let extra: Vec<(u32, Vec<u8>)> = Snap::get(&mut r).map_err(SnapError::Corrupt)?;
        if r.remaining() != 0 { return Err(SnapError::Corrupt("trailing bytes".into())); }
        // the files the state refers to
        let need = |f: Option<u16>| -> Result<Vec<u8>, SnapError> {
            let f = f.ok_or_else(|| SnapError::Corrupt("no file".into()))?;
            self.file_by_index(f).cloned().ok_or_else(|| SnapError::Missing(FILE_NAMES.get(f as usize).unwrap_or(&"?").to_string()))
        };
        let script = need(st.script_file)?;
        let data_file = need(st.data_file)?;
        let (data_idx, script_idx) = (st.data_file.unwrap(), st.script_file.unwrap());

        // nothing can fail from here: apply
        apply_state(st, self);
        self.top = Top::Game; self.module = None; self.mod_delay = 0; self.reboot_in = 0;
        if let (Some(cur), Some(s)) = (self.input.as_mut(), input) { cur.set_state(s); }
        // what the front end sets stays as it is now
        ix.sfx_len = std::mem::take(&mut self.ix.sfx_len);
        ix.gun_present = self.ix.gun_present; ix.gun_x = self.ix.gun_x; ix.gun_y = self.ix.gun_y; ix.gun_buttons = self.ix.gun_buttons;
        ix.rng_seq = self.ix.rng_seq; ix.force_in = self.ix.force_in; ix.force_gun = self.ix.force_gun;
        self.ix = ix;
        self.sync_gfx();
        self.mem.retain_bases(&[gfx::SUBCODE_BASE]);
        self.map_data_file(data_idx, data_file);
        for (base, bytes) in extra { self.mem.map(base, bytes); }
        self.script = script; self.script_file = Some(script_idx);
        // the translated text area of this script (the pack is the one the snapshot was made with)
        self.i18n_refresh(sncore::script::sp_number(script_idx));
        self.vsub.reset();
        self.out.clear();
        let mut fb = std::mem::take(&mut self.fb);
        self.disp.render(&mut fb);
        self.fb = fb;
        self.push_restored_audio();
        Ok(info)
    }

    /// Start again what was playing: the CD-DA track at its position, looping FM music at its position, looping FM effects.
    fn push_restored_audio(&mut self) {
        if let Some(s) = self.cd_start {
            let track = self.regs.get(crate::modules::CD_TRACK_KEY);
            self.out.push(Out::MusicAt { track, at_ms: ((self.frame.saturating_sub(s)) * 1000 / 60) as u32 });
        }
        if let Some((id, start)) = self.fm_music {
            if self.voice_music_left == u32::MAX { self.out.push(Out::FmAt { id, music: true, at_ms: ((self.frame.saturating_sub(start)) * 1000 / 60) as u32 }); }
        }
        let loops = self.fm_loops.clone();
        for id in loops { self.out.push(Out::Fm { id, music: false }); }
    }
}
