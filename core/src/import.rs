//! Disc image importer: turns the user's `.cue` + `.bin` file(s) into what the web front end plays, without any extraction
//! pipeline outside the browser. The same code runs natively (tests, `snt import-disc`) and in WebAssembly (a Web Worker).
//!
//! Input is a [`DiscIo`] (byte ranges of the image files, read lazily: the 625 MB image is never in memory), output goes to a
//! [`Sink`] as named append-only streams, which the browser stores in OPFS:
//!
//! | stream | content |
//! |---|---|
//! | `header.bin`, `ip.bin` | boot code of the disc (as `snt extract` writes them) |
//! | `files/NAME` | every ISO9660 file except PCMLD_01.BIN (identical to `snt extract` output) |
//! | `voices.pcm` | all voice clips: signed 8-bit mono (sign-magnitude PCM decoded, 16276 Hz) |
//! | `sfx.pcm` | PCM effect banks and the drum kit: signed 8-bit mono |
//! | `cdda.pcm` | CD-DA tracks as on the disc: 16-bit LE stereo 44.1 kHz (only when `copy_cdda`) |
//! | `pcm.json`, `fm.json` | manifests in the format of `audio/web/{pcm,fm}.json`; `file` is a key, `at: [stream, offset, bytes]` its place |
//! | `meta.json` | what was imported (digest, track table, stream sizes); written last, so its presence means "complete" |
//!
//! FM music and effects are NOT rendered here: the driver is run without the chip ([`audio::render::render_dry`]) to publish exact
//! lengths and loop points, and [`FmRenderer`] renders one sound's audio when it is needed.
use crate::audio::{self, kdriver::KDriver, render, PcmSfxEntry, VoiceEntry};
use crate::disc::{self, Cue, SegaHeader, TrackExtent, TrackMode, RAW_SECTOR, USER_SECTOR};
use crate::sha256::{hex, sha256, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

// ------------------------------------------------------------------ the one game this importer accepts

/// A disc the importer recognises. Only identifiers and a digest of the disc files are stored (no game data).
pub struct KnownDisc {
    pub title: &'static str,
    /// start of the Sega header serial field (offset 0x180)
    pub serial: &'static str,
    /// region field of the Sega header (offset 0x1F0)
    pub region: &'static str,
    /// SHA-256 over [`files_digest`]: the header, the IP and every file of the ISO9660 volume
    pub digest: &'static str,
}

pub const SNATCHER_USA: KnownDisc = KnownDisc {
    title: "Snatcher (USA)", serial: "GM T-95035", region: "J",
    digest: "71651e0604eaa2c4042e2b44abbce92cbf307cb05e1fbd5a62da12224ddf07ea",
};

/// Streams longer than this are written in pieces of this many bytes.
pub const CHUNK_SECTORS: u32 = 256;
/// Pieces of the PCM voice file are also this big when no voice block is pending.
pub const MAX_FM_ID: u8 = 0x7A;
pub const VOICES_STREAM: &str = "voices.pcm";
pub const SFX_STREAM: &str = "sfx.pcm";
pub const CDDA_STREAM: &str = "cdda.pcm";
pub const PCMLD: &str = "PCMLD_01.BIN";
/// files needed by the FM renderer and the PCM extraction
const NEED: [&str; 4] = ["FMWR_1.BIN", "PCMLT_01.BIN", "PCMDRMDT.BIN", PCMLD];

#[derive(Debug, Clone)]
pub struct ImportError {
    /// stable code for the front end to translate: cue, format, short, io, notsega, notsnatcher, edition, region, missing, digest, data
    pub code: &'static str,
    pub detail: String,
}
impl ImportError {
    fn new(code: &'static str, detail: impl Into<String>) -> Self { ImportError { code, detail: detail.into() } }
    pub fn to_json(&self) -> String { format!("{{\"code\":\"{}\",\"detail\":{}}}", self.code, jstr(&self.detail)) }
}
impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}: {}", self.code, self.detail) }
}
type Res<T> = Result<T, ImportError>;
fn io_err(e: String) -> ImportError { ImportError::new("io", e) }

/// Read access to the image file(s) named by the cue sheet (index = position in [`Cue::files`]).
pub trait DiscIo {
    /// Fill `buf` from byte `offset` of file `file`. Reading past the end is an error.
    fn read(&mut self, file: usize, offset: u64, buf: &mut [u8]) -> Result<(), String>;
}

/// Where the import puts its output: named append-only streams.
pub trait Sink {
    fn write(&mut self, stream: &str, data: &[u8]) -> Result<(), String>;
    /// The stream is complete (the browser closes its file handle).
    fn finish(&mut self, stream: &str) -> Result<(), String>;
    /// `phase` is one of: header, files, voices, fm, cd, done; `done`/`total` count bytes of the whole import.
    fn progress(&mut self, phase: &str, done: u64, total: u64);
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// copy the CD-DA tracks into the `cdda.pcm` stream (otherwise `meta.json` only says where they are in the image files)
    pub copy_cdda: bool,
}

// ------------------------------------------------------------------ JSON helpers

pub fn jstr(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c { '"' => o.push_str("\\\""), '\\' => o.push_str("\\\\"), c if (c as u32) < 32 => { let _ = write!(o, "\\u{:04x}", c as u32); } c => o.push(c) }
    }
    o.push('"');
    o
}

// ------------------------------------------------------------------ disc access

struct Disc<I: DiscIo> {
    io: I,
    layout: Vec<TrackExtent>,
    raw: Vec<u8>,
    user: Vec<u8>,
}

impl<I: DiscIo> Disc<I> {
    fn data_track(&self) -> &TrackExtent { &self.layout[0] }

    /// Stream `len` bytes of ISO user data starting at sector `lba` of the data track, in pieces of at most CHUNK_SECTORS sectors.
    fn stream(&mut self, lba: u32, len: u64, f: &mut dyn FnMut(&[u8]) -> Res<()>) -> Res<()> {
        let tr = self.data_track().clone();
        let nsec = len.div_ceil(USER_SECTOR as u64);
        if lba as u64 + nsec > tr.sectors as u64 {
            return Err(ImportError::new("short", format!("data track: sectors {}..{} are beyond its {} sectors", lba, lba as u64 + nsec, tr.sectors)));
        }
        let (mut done, mut left) = (0u64, len);
        while done < nsec {
            let n = (nsec - done).min(CHUNK_SECTORS as u64) as usize;
            let off = tr.start + (lba as u64 + done) * tr.sector_size as u64;
            self.raw.resize(n * tr.sector_size, 0);
            self.io.read(tr.file, off, &mut self.raw).map_err(|e| ImportError::new("short", format!("data track: {e}")))?;
            let piece: &[u8] = if tr.sector_size == RAW_SECTOR {
                self.user.clear();
                for (k, sec) in self.raw.chunks_exact(RAW_SECTOR).enumerate() {
                    if sec[0] != 0 || sec[1..11].iter().any(|&b| b != 0xFF) || sec[11] != 0 {
                        return Err(ImportError::new("format", format!("data track sector {}: no sync pattern (is this a raw MODE1/2352 image?)", lba as u64 + done + k as u64)));
                    }
                    if sec[15] != 1 { return Err(ImportError::new("format", format!("data track sector {}: mode {} (expected 1)", lba as u64 + done + k as u64, sec[15]))); }
                    self.user.extend_from_slice(&sec[16..16 + USER_SECTOR]);
                }
                &self.user
            } else { &self.raw };
            let take = (piece.len() as u64).min(left) as usize;
            f(&piece[..take])?;
            left -= take as u64; done += n as u64;
        }
        Ok(())
    }

    fn read_user(&mut self, lba: u32, len: usize) -> Res<Vec<u8>> {
        let mut v = Vec::with_capacity(len);
        self.stream(lba, len as u64, &mut |c| { v.extend_from_slice(c); Ok(()) })?;
        Ok(v)
    }
}

/// What the importer learned from the disc header.
#[derive(Debug, Clone)]
pub struct DiscInfo { pub header: SegaHeader, pub title: String }

/// Check the Sega CD header against [`SNATCHER_USA`] (cheap: one sector). Distinct errors for: not a Sega CD disc, another game,
/// another edition of Snatcher, a changed region code.
pub fn check_header(s0: &[u8]) -> Res<DiscInfo> {
    if s0.len() < 0x200 || &s0[0..14] != b"SEGADISCSYSTEM" {
        return Err(ImportError::new("notsega", "sector 0 does not start with SEGADISCSYSTEM"));
    }
    let h = SegaHeader::parse(s0).map_err(|e| ImportError::new("notsega", e))?;
    let overseas = String::from_utf8_lossy(&s0[0x150..0x180]).trim().to_string();
    let title = if h.domestic_name.is_empty() { overseas.clone() } else { h.domestic_name.clone() };
    let known = &SNATCHER_USA;
    if !(title.to_ascii_uppercase().contains("SNATCHER") || overseas.to_ascii_uppercase().contains("SNATCHER")) {
        return Err(ImportError::new("notsnatcher", format!("title {title:?}, serial {:?}", h.serial)));
    }
    if !h.serial.starts_with(known.serial) {
        return Err(ImportError::new("edition", format!("serial {:?}, region {:?} (this port needs {} serial {})", h.serial, h.region, known.title, known.serial)));
    }
    if h.region != known.region {
        return Err(ImportError::new("region", format!("region code {:?} (expected {:?})", h.region, known.region)));
    }
    Ok(DiscInfo { header: h, title })
}

/// One entry of the digest: the name, size and SHA-256 of a piece of the disc.
pub fn files_digest(items: &mut Vec<(String, u64, [u8; 32])>) -> String {
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let mut t = String::new();
    for (n, sz, h) in items.iter() { let _ = writeln!(t, "{n}\t{sz}\t{}", hex(h)); }
    hex(&sha256(t.as_bytes()))
}

// ------------------------------------------------------------------ PCM conversion

/// Sign-magnitude RF5C164 byte to signed 8-bit (the 16-bit sample of `audio::sm8_to_i16` is this value << 8).
#[inline]
pub fn sm8_to_s8(b: u8) -> u8 { ((audio::sm8_to_i16(b)) >> 8) as i8 as u8 }
pub fn decode_s8(d: &[u8]) -> Vec<u8> { d.iter().take_while(|&&b| b != 0xFF).map(|&b| sm8_to_s8(b)).collect() }
fn s8_of(samples: &[i16]) -> Vec<u8> { samples.iter().map(|&s| (s >> 8) as i8 as u8).collect() }

// ------------------------------------------------------------------ FM

/// Renders FM sounds of the disc's sound driver (the Rust port of the 68000 driver); holds the small files it needs.
pub struct FmRenderer { ip: Vec<u8>, fmwr: Vec<u8>, kit: render::DrumKit }

impl FmRenderer {
    /// `header` = header.bin, `ip` = ip.bin, plus FMWR_1.BIN, PCMLT_01.BIN, PCMDRMDT.BIN.
    pub fn new(header: &[u8], ip: &[u8], fmwr: &[u8], pcmlt: &[u8], drmdt: &[u8]) -> Result<FmRenderer, String> {
        if header.len() < 0x800 { return Err("header.bin too short".into()); }
        if pcmlt.len() < audio::SFX_TABLE_OFFSET + 16 { return Err("PCMLT_01.BIN too short".into()); }
        let mut full = header[0x200..0x800].to_vec();
        full.extend_from_slice(ip);
        Ok(FmRenderer { ip: full, fmwr: fmwr.to_vec(), kit: render::DrumKit::new(&audio::parse_sfx_table(pcmlt), drmdt) })
    }
    pub fn render(&self, id: u8) -> (render::Rendered, bool) {
        let mut d = KDriver::new(self.ip.clone(), self.fmwr.clone());
        let r = render::render(&mut d, &self.kit, id, FM_MAX_TICKS, 1.0);
        let music = d.sound_type(id) & 0x0F != 0;
        (r, music)
    }
    pub fn dry(&self, id: u8) -> (render::Rendered, bool) {
        let mut d = KDriver::new(self.ip.clone(), self.fmwr.clone());
        let r = render::render_dry(&mut d, id, FM_MAX_TICKS);
        let music = d.sound_type(id) & 0x0F != 0;
        (r, music)
    }
    /// Stream a rendered sound to the sink as 16-bit LE stereo (the `fm/{song|sfx}_XX.s16` file).
    pub fn render_to<S: Sink>(&self, id: u8, sink: &mut S) -> Result<usize, String> {
        let (r, music) = self.render(id);
        let name = fm_stream(id, music);
        let mut buf = Vec::with_capacity(1 << 20);
        for c in r.samples.chunks(1 << 19) {
            buf.clear();
            for s in c { buf.extend_from_slice(&s.to_le_bytes()); }
            sink.write(&name, &buf)?;
        }
        sink.finish(&name)?;
        Ok(r.samples.len() / 2)
    }
}
pub const FM_MAX_TICKS: usize = 60 * 60 * 8;
pub fn fm_stream(id: u8, music: bool) -> String { format!("fm/{}_{:02x}.s16", if music { "song" } else { "sfx" }, id) }

// ------------------------------------------------------------------ the import

pub struct Summary { pub meta_json: String, pub digest: String }

struct Out<'a, S: Sink> { sink: &'a mut S, sizes: BTreeMap<String, u64>, done: u64, total: u64 }
impl<'a, S: Sink> Out<'a, S> {
    fn put(&mut self, stream: &str, d: &[u8]) -> Res<u64> {
        let at = *self.sizes.get(stream).unwrap_or(&0);
        self.sink.write(stream, d).map_err(io_err)?;
        *self.sizes.entry(stream.to_string()).or_insert(0) += d.len() as u64;
        Ok(at)
    }
    fn fin(&mut self, stream: &str) -> Res<()> { self.sink.finish(stream).map_err(io_err) }
    fn whole(&mut self, stream: &str, d: &[u8]) -> Res<()> { self.put(stream, d)?; self.fin(stream) }
    fn adv(&mut self, phase: &str, n: u64) { self.done += n; self.sink.progress(phase, self.done.min(self.total), self.total); }
}

/// Run the whole import. `file_bytes` = size of each `FILE` of the cue, in cue order.
pub fn import<I: DiscIo, S: Sink>(cue_text: &str, file_bytes: &[u64], io: I, sink: &mut S, opts: Options) -> Res<Summary> {
    let cue = Cue::parse(cue_text).map_err(|e| ImportError::new("cue", e))?;
    if cue.files.len() != file_bytes.len() { return Err(ImportError::new("cue", "the number of image files does not match the cue sheet")); }
    let layout = cue.layout(file_bytes).map_err(|e| ImportError::new("cue", e))?;
    if !matches!(layout[0].mode, TrackMode::Mode1Raw | TrackMode::Mode1Cooked) {
        return Err(ImportError::new("format", "track 1 is not a MODE1 data track"));
    }
    let audio_tracks: Vec<TrackExtent> = layout.iter().filter(|t| t.mode == TrackMode::Audio).cloned().collect();
    let mut disc = Disc { io, layout, raw: Vec::new(), user: Vec::new() };

    // ---- header: cheap checks first, so that a wrong disc fails at once
    sink.progress("header", 0, 1);
    let first = disc.read_user(0, USER_SECTOR * 16).map_err(|e| if e.code == "short" { ImportError::new("notsega", "the data track is too short to be a Sega CD disc") } else { e })?;
    let info = check_header(&first)?;
    let h = info.header.clone();
    let header_bin = first[..0x800].to_vec();
    let need = h.ip_offset as usize + h.ip_size as usize;
    if need > 1 << 20 { return Err(ImportError::new("data", format!("the IP area ({:#x} + {:#x}) is not where a Sega CD boot area is", h.ip_offset, h.ip_size))); }
    let boot = if need <= first.len() { first.clone() } else { disc.read_user(0, need)? };
    if (h.ip_offset as usize + h.ip_size as usize) > boot.len() { return Err(ImportError::new("data", "the IP area is outside the boot sectors")); }
    let ip_bin = boot[h.ip_offset as usize..(h.ip_offset + h.ip_size) as usize].to_vec();
    let mut digest_items: Vec<(String, u64, [u8; 32])> = vec![
        ("@header".into(), header_bin.len() as u64, sha256(&header_bin)),
        ("@ip".into(), ip_bin.len() as u64, sha256(&ip_bin)),
    ];

    // ---- directory
    let mut dir_err: Option<ImportError> = None;
    let listed = disc::iso_root_with(&mut |lba, len| disc.read_user(lba, len).map_err(|e| { let s = e.to_string(); dir_err = Some(e); s }));
    let files: Vec<disc::IsoFile> = listed.map_err(|e| dir_err.take().unwrap_or_else(|| ImportError::new("notsnatcher", format!("ISO9660: {e}"))))?
        .into_iter().filter(|f| !f.is_dir).collect();
    for n in NEED {
        if !files.iter().any(|f| f.name == n) { return Err(ImportError::new("missing", format!("{n} is not on the disc"))); }
    }
    let tr1_sectors = disc.data_track().sectors as u64;
    for f in &files {
        if f.lba as u64 + (f.size as u64).div_ceil(2048) > tr1_sectors {
            return Err(ImportError::new("short", format!("{}: sectors {}+{} are beyond the data track", f.name, f.lba, f.size)));
        }
    }
    let cd_bytes: u64 = if opts.copy_cdda { audio_tracks.iter().map(|t| t.sectors as u64 * RAW_SECTOR as u64).sum() } else { 0 };
    let data_bytes: u64 = files.iter().map(|f| f.size as u64).sum::<u64>() + 0x800 + h.ip_size as u64;
    let mut o = Out { sink, sizes: BTreeMap::new(), done: 0, total: data_bytes + cd_bytes + 1 };
    o.whole("header.bin", &header_bin)?;
    o.whole("ip.bin", &ip_bin)?;
    o.adv("files", 0x800 + h.ip_size as u64);

    // ---- game files (all but the PCM loader image, which becomes voices.pcm), hashed on the way
    let mut small: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut names: Vec<String> = Vec::new();
    for f in files.iter().filter(|f| f.name != PCMLD) {
        let stream = format!("files/{}", f.name);
        let keep = NEED.contains(&f.name.as_str());
        let mut hs = Sha256::new();
        let mut kept = Vec::new();
        disc.stream(f.lba, f.size as u64, &mut |c| {
            hs.update(c);
            if keep { kept.extend_from_slice(c); }
            o.put(&stream, c)?;
            o.adv("files", c.len() as u64);
            Ok(())
        })?;
        if f.size == 0 { o.put(&stream, &[])?; }
        o.fin(&stream)?;
        digest_items.push((f.name.clone(), f.size as u64, hs.finish()));
        names.push(f.name.clone());
        if keep { small.insert(f.name.clone(), kept); }
    }

    // ---- PCM voices (PCMLD_01.BIN decoded block by block; every byte of the file is read once, in order, and hashed)
    let lt = &small["PCMLT_01.BIN"];
    if lt.len() < audio::VOICE_TABLE_ENTRIES * 8 { return Err(ImportError::new("data", "PCMLT_01.BIN is too short for the voice table")); }
    let nsfx = lt.get(audio::SFX_TABLE_OFFSET).copied().unwrap_or(0) as usize;
    if lt.len() < audio::SFX_TABLE_OFFSET + nsfx.max(1) * 16 { return Err(ImportError::new("data", "PCMLT_01.BIN is too short for the effect table")); }
    let voices = audio::parse_voice_table(lt);
    let sfx = audio::parse_sfx_table(lt);
    let ld = files.iter().find(|f| f.name == PCMLD).unwrap().clone();
    let mut order: Vec<&VoiceEntry> = voices.iter().collect();
    order.sort_by_key(|v| v.sector);
    let mut hs = Sha256::new();
    let mut hashed = 0u64;
    let mut voice_at: BTreeMap<u16, (u64, u64)> = BTreeMap::new();   // id -> (offset, bytes) in voices.pcm
    let mut banks: Vec<(u16, Vec<audio::Segment>)> = Vec::new();
    let vrate = audio::fd_to_hz(audio::VOICE_FD).round() as u32;
    for v in order {
        let (start, len) = (v.sector as u64 * 2048, v.sectors as u64 * 2048);
        if start + len > ld.size as u64 { return Err(ImportError::new("data", format!("voice {} lies beyond the end of {PCMLD}", v.id))); }
        if start > hashed {   // bytes between voices (none on the real disc) still belong to the file
            disc.stream(ld.lba + (hashed / 2048) as u32, start - hashed, &mut |c| { hs.update(c); Ok(()) })?;
            hashed = start;
        }
        let blk = disc.read_user(ld.lba + v.sector as u32, len as usize)?;
        if start + len > hashed { hs.update(&blk[(hashed - start) as usize..]); hashed = start + len; }
        if v.kind == 1 { banks.push((v.id, audio::parse_segments(&blk))); }
        else {
            let s = decode_s8(&blk);
            let at = o.put(VOICES_STREAM, &s)?;
            voice_at.insert(v.id, (at, s.len() as u64));
        }
        o.adv("voices", len);
    }
    if hashed < ld.size as u64 {
        disc.stream(ld.lba + (hashed / 2048) as u32, ld.size as u64 - hashed, &mut |c| { hs.update(c); o.adv("voices", c.len() as u64); Ok(()) })?;
    }
    o.fin(VOICES_STREAM)?;
    digest_items.push((PCMLD.into(), ld.size as u64, hs.finish()));

    // ---- is it the right data? (before the long steps)
    let digest = files_digest(&mut digest_items);
    if digest != SNATCHER_USA.digest {
        return Err(ImportError::new("digest", format!("{digest} (a different dump or modified files; expected the data of {})", SNATCHER_USA.title)));
    }

    // ---- banks and drum kit -> sfx.pcm, manifest pcm.json
    let drmdt = &small["PCMDRMDT.BIN"];
    let mut pj = String::from("{\n  \"note\": \"generated by the disc importer; see docs/re/web.md\",\n");
    let _ = write!(pj, "  \"pcm_clock_hz\": {:.3},\n  \"voice_rate_hz\": {vrate},\n  \"voices\": {{\n", audio::PCM_CLOCK_HZ);
    let mut first = true;
    for v in voices.iter().filter(|v| v.kind != 1) {
        let (at, n) = voice_at[&v.id];
        if !first { pj.push_str(",\n"); }
        first = false;
        let _ = write!(pj, "    \"{}\": {{\"file\": \"voice/v{:04}.s8\", \"at\": [\"{VOICES_STREAM}\", {at}, {n}], \"samples\": {n}, \"seconds\": {:.3}, \"kind\": {}, \"env\": {}, \"pan\": {}, \"prio\": {}}}",
            v.id, v.id, n as f64 / vrate as f64, v.kind, v.env, v.pan, v.prio);
    }
    pj.push_str("\n  },\n  \"banks\": {\n");
    for (bi, (id, segs)) in banks.iter().enumerate() {
        let _ = write!(pj, "    \"{id}\": {{");
        for (si, g) in segs.iter().enumerate() {
            let b = s8_of(&g.samples);
            let at = o.put(SFX_STREAM, &b)?;
            let _ = write!(pj, "{}\"{:#x}\": {{\"file\": \"bank/b{:02}_{:05x}.s8\", \"at\": [\"{SFX_STREAM}\", {at}, {}], \"rate\": {}, \"samples\": {}}}", if si > 0 { ", " } else { "" }, g.offset, id, g.offset, b.len(), g.rate, g.samples.len());
        }
        let _ = write!(pj, "}}{}\n", if bi + 1 < banks.len() { "," } else { "" });
    }
    pj.push_str("  },\n  \"drum\": {");
    let dsegs = audio::parse_segments(drmdt);
    for (si, g) in dsegs.iter().enumerate() {
        let b = s8_of(&g.samples);
        let at = o.put(SFX_STREAM, &b)?;
        let _ = write!(pj, "{}\n    \"{:#x}\": {{\"file\": \"drum/d{:05x}.s8\", \"at\": [\"{SFX_STREAM}\", {at}, {}], \"rate\": {}, \"samples\": {}}}", if si > 0 { "," } else { "" }, g.offset, g.offset, b.len(), g.rate, g.samples.len());
    }
    pj.push_str("\n  },\n  \"sfx\": {");
    for (k, e) in sfx.iter().enumerate() { sfx_json(&mut pj, k, e); }
    pj.push_str("\n  }\n}\n");
    o.fin(SFX_STREAM)?;
    o.whole("pcm.json", pj.as_bytes())?;

    // ---- FM manifest: exact lengths and loops from the driver alone (the audio is rendered when needed)
    o.sink.progress("fm", o.done.min(o.total), o.total);
    let fm = FmRenderer::new(&header_bin, &ip_bin, &small["FMWR_1.BIN"], lt, drmdt).map_err(|e| ImportError::new("data", e))?;
    let mut fj = String::from("{\n  \"note\": \"generated by the disc importer (driver run without the chip); 48 kHz stereo; loop_* in sample frames\",\n  \"rate\": 48000,\n  \"sounds\": {");
    for (k, id) in (1..=MAX_FM_ID).enumerate() {
        let (r, music) = fm.dry(id);
        let _ = write!(fj, "{}\n    \"{}\": {{\"file\": \"{}\", \"music\": {}, \"frames\": {}, \"seconds\": {:.3}, \"loop_start\": {}, \"loop_end\": {}, \"ticks\": {}, \"loop_kind\": {}}}",
            if k > 0 { "," } else { "" }, id, fm_stream(id, music), music, r.out_frames, r.out_frames as f64 / 48000.0,
            r.loop_start.map(|x| x.to_string()).unwrap_or("null".into()), r.loop_end, r.frames, r.loop_kind.map(|k| format!("\"{k:?}\"")).unwrap_or("null".into()));
    }
    fj.push_str("\n  }\n}\n");
    o.whole("fm.json", fj.as_bytes())?;

    // ---- CD-DA
    let mut cj = String::new();
    for (k, t) in audio_tracks.iter().enumerate() {
        let bytes = t.sectors as u64 * RAW_SECTOR as u64;
        let mut off = 0u64;
        if opts.copy_cdda {
            let mut buf = vec![0u8; RAW_SECTOR * CHUNK_SECTORS as usize];
            let mut done = 0u32;
            while done < t.sectors {
                let n = (t.sectors - done).min(CHUNK_SECTORS) as usize;
                disc.io.read(t.file, t.start + done as u64 * RAW_SECTOR as u64, &mut buf[..n * RAW_SECTOR]).map_err(|e| ImportError::new("short", format!("track {:02}: {e}", t.number)))?;
                let at = o.put(CDDA_STREAM, &buf[..n * RAW_SECTOR])?;
                if done == 0 { off = at; }
                done += n as u32;
                o.adv("cd", (n * RAW_SECTOR) as u64);
            }
            if t.sectors == 0 { off = *o.sizes.get(CDDA_STREAM).unwrap_or(&0); }
        }
        let _ = write!(cj, "{}\"{}\": {{\"file\": {}, \"start\": {}, \"bytes\": {bytes}, \"off\": {off}}}", if k > 0 { ", " } else { "" }, t.number, t.file, t.start);
    }
    if opts.copy_cdda { o.fin(CDDA_STREAM)?; }

    // ---- meta.json last
    let mut sizes = String::new();
    for (k, (n, sz)) in o.sizes.iter().enumerate() { let _ = write!(sizes, "{}{}: {sz}", if k > 0 { ", " } else { "" }, jstr(n)); }
    let names_j = names.iter().map(|n| jstr(n)).collect::<Vec<_>>().join(", ");
    let meta = format!("{{\n  \"format\": 1,\n  \"title\": {},\n  \"serial\": {},\n  \"region\": {},\n  \"digest\": \"{digest}\",\n  \"files\": [{names_j}],\n  \"cdda\": {{\"copied\": {}, \"rate\": 44100, \"tracks\": {{{cj}}}}},\n  \"streams\": {{{sizes}}}\n}}\n",
        jstr(SNATCHER_USA.title), jstr(&h.serial), jstr(&h.region), opts.copy_cdda);
    o.whole("meta.json", meta.as_bytes())?;
    o.sink.progress("done", o.total, o.total);
    Ok(Summary { meta_json: meta, digest })
}

fn sfx_json(j: &mut String, k: usize, e: &PcmSfxEntry) {
    let (src, off) = if e.addr >= audio::BANK_BASE { ("bank", e.addr - audio::BANK_BASE) } else { ("drum", e.addr - audio::DRUM_BASE) };
    let _ = write!(j, "{}\n    \"{}\": {{\"src\": \"{}\", \"offset\": \"{:#x}\", \"fd\": {}, \"rate_hz\": {:.1}, \"env\": {}, \"pan\": {}, \"prio\": {}, \"chan\": {}}}",
        if k > 0 { "," } else { "" }, e.id, src, off, e.fd, audio::fd_to_hz(e.fd), e.env, e.pan, e.prio, e.chan);
}
