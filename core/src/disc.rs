//! Disc image access: MODE1/2352 track 1 -> 2048-byte sectors, Sega CD header, ISO9660.
use crate::be32;

pub const RAW_SECTOR: usize = 2352;
pub const USER_SECTOR: usize = 2048;

/// Track 1 user data (2048-byte sectors), built from a raw MODE1/2352 image.
pub struct DataTrack {
    pub data: Vec<u8>,
}

impl DataTrack {
    /// `raw` = bytes of the .bin; `sectors` = number of sectors in track 1.
    pub fn from_raw(raw: &[u8], sectors: usize) -> Result<Self, String> {
        let mut data = Vec::with_capacity(sectors * USER_SECTOR);
        for s in 0..sectors {
            let o = s * RAW_SECTOR;
            let sec = raw.get(o..o + RAW_SECTOR).ok_or("image truncated")?;
            if sec[0] != 0 || sec[1..11].iter().any(|&b| b != 0xFF) || sec[11] != 0 {
                return Err(format!("sector {s}: bad sync pattern"));
            }
            if sec[15] != 1 {
                return Err(format!("sector {s}: mode {} (expected 1)", sec[15]));
            }
            data.extend_from_slice(&sec[16..16 + USER_SECTOR]);
        }
        Ok(DataTrack { data })
    }
    pub fn sector(&self, n: usize) -> &[u8] { &self.data[n * USER_SECTOR..(n + 1) * USER_SECTOR] }
    pub fn read(&self, lba: u32, len: u32) -> &[u8] {
        let o = lba as usize * USER_SECTOR;
        &self.data[o..o + len as usize]
    }
}

#[derive(Debug, Clone)]
pub struct SegaHeader {
    pub disc_id: String,
    pub volume: String,
    pub ip_offset: u32,
    pub ip_size: u32,
    pub ip_entry: u32,
    pub ip_work: u32,
    pub sp_offset: u32,
    pub sp_size: u32,
    pub sp_entry: u32,
    pub sp_work: u32,
    pub date: String,
    pub domestic_name: String,
    pub serial: String,
    pub region: String,
}

fn ascii(d: &[u8]) -> String { d.iter().map(|&c| if (32..127).contains(&c) { c as char } else { '.' }).collect::<String>().trim_end().to_string() }

impl SegaHeader {
    pub fn parse(s0: &[u8]) -> Result<Self, String> {
        if &s0[0..14] != b"SEGADISCSYSTEM" { return Err("missing SEGADISCSYSTEM".into()); }
        Ok(SegaHeader {
            disc_id: ascii(&s0[0..16]),
            volume: ascii(&s0[0x10..0x1B]),
            ip_offset: be32(s0, 0x30), ip_size: be32(s0, 0x34), ip_entry: be32(s0, 0x38), ip_work: be32(s0, 0x3C),
            sp_offset: be32(s0, 0x40), sp_size: be32(s0, 0x44), sp_entry: be32(s0, 0x48), sp_work: be32(s0, 0x4C),
            date: ascii(&s0[0x50..0x60]),
            domestic_name: ascii(&s0[0x120..0x150]),
            serial: ascii(&s0[0x180..0x18E]),
            region: ascii(&s0[0x1F0..0x1F3]),
        })
    }
}

#[derive(Debug, Clone)]
pub struct IsoFile {
    pub name: String,
    pub lba: u32,
    pub size: u32,
    pub is_dir: bool,
}

fn le32(d: &[u8], o: usize) -> u32 { u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) }

/// Parse the root directory of the ISO9660 volume (Snatcher has a flat root).
pub fn iso_root(t: &DataTrack) -> Result<Vec<IsoFile>, String> {
    iso_root_with(&mut |lba, len| Ok(t.data.get(lba as usize * USER_SECTOR..lba as usize * USER_SECTOR + len).ok_or("read past the data track")?.to_vec()))
}

/// Same as [`iso_root`] over any source of 2048-byte user data: `read(lba, len)` returns `len` bytes starting at sector `lba`.
/// The importer uses it to list the files straight from the image, one directory extent at a time.
pub fn iso_root_with(read: &mut dyn FnMut(u32, usize) -> Result<Vec<u8>, String>) -> Result<Vec<IsoFile>, String> {
    let pvd = read(16, USER_SECTOR)?;
    if &pvd[1..6] != b"CD001" || pvd[0] != 1 { return Err("no ISO9660 PVD at sector 16".into()); }
    let root = &pvd[156..156 + 34];
    let (ext, size) = (le32(root, 2), le32(root, 10));
    // both-endian check
    if be32(root, 6) != ext || be32(root, 14) != size { return Err("root record: LE/BE mismatch".into()); }
    if size as usize > 16 << 20 { return Err("root directory too large".into()); }
    let dir = read(ext, size as usize)?;
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < dir.len() {
        let l = dir[i] as usize;
        if l == 0 { i = (i / USER_SECTOR + 1) * USER_SECTOR; continue; }
        if l < 34 || i + l > dir.len() { return Err("bad directory record".into()); }
        let r = &dir[i..i + l];
        let nl = r[32] as usize;
        if 33 + nl > l { return Err("bad directory record name".into()); }
        let raw = &r[33..33 + nl];
        if !(nl == 1 && raw[0] <= 1) {
            let mut name = String::from_utf8_lossy(raw).to_string();
            if let Some(p) = name.find(';') { name.truncate(p); }
            out.push(IsoFile { name, lba: le32(r, 2), size: le32(r, 10), is_dir: r[25] & 2 != 0 });
        }
        i += l;
    }
    Ok(out)
}


// ------------------------------------------------------------------ .cue sheets

/// Frames (sectors) per second of a CD; a cue `MM:SS:FF` is a sector count.
pub const FRAMES_PER_SEC: u32 = 75;

/// `MM:SS:FF` -> sectors.
pub fn msf_to_sectors(s: &str) -> Result<u32, String> {
    let p: Vec<&str> = s.split(':').collect();
    if p.len() != 3 { return Err(format!("bad time {s:?}")); }
    let n = |x: &str| x.parse::<u32>().map_err(|_| format!("bad time {s:?}"));
    let (m, sec, f) = (n(p[0])?, n(p[1])?, n(p[2])?);
    if sec >= 60 || f >= FRAMES_PER_SEC { return Err(format!("bad time {s:?}")); }
    Ok((m * 60 + sec) * FRAMES_PER_SEC + f)
}

/// Sectors -> `MM:SS:FF`.
pub fn sectors_to_msf(n: u32) -> String { format!("{:02}:{:02}:{:02}", n / FRAMES_PER_SEC / 60, n / FRAMES_PER_SEC % 60, n % FRAMES_PER_SEC) }

#[derive(Debug, Clone, PartialEq)]
pub enum TrackMode {
    /// `MODE1/2352`: 16-byte header (sync, address, mode 1) + 2048 user bytes + EDC/ECC.
    Mode1Raw,
    /// `MODE1/2048`: user data only.
    Mode1Cooked,
    /// `AUDIO`: 2352 bytes of 16-bit little-endian stereo 44.1 kHz PCM per sector.
    Audio,
    /// anything else (MODE2/..., CDG): not supported by the importer
    Other(String),
}

impl TrackMode {
    pub fn sector_size(&self) -> Option<usize> {
        match self { TrackMode::Mode1Raw | TrackMode::Audio => Some(RAW_SECTOR), TrackMode::Mode1Cooked => Some(USER_SECTOR), TrackMode::Other(_) => None }
    }
}

/// One `TRACK` of a cue sheet. Positions are in sectors **relative to the start of its `FILE`**.
#[derive(Debug, Clone, PartialEq)]
pub struct CueTrack {
    pub number: u32,
    pub mode: TrackMode,
    /// index into [`Cue::files`]
    pub file: usize,
    /// `INDEX 00` (start of the pregap stored in the file), if any
    pub index00: Option<u32>,
    /// `INDEX 01` (start of the track proper)
    pub index01: u32,
    /// `PREGAP` command: silent sectors that are NOT stored in any file
    pub pregap: u32,
}

/// A parsed cue sheet: one `FILE` for a single-bin image (chdman) or one per track (Redump).
#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    /// file names exactly as written in the sheet
    pub files: Vec<String>,
    pub tracks: Vec<CueTrack>,
}

/// Split a cue line into words; `"..."` keeps spaces ("Snatcher (USA) (Track 01).bin").
fn cue_words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut quoted, mut any) = (false, false);
    for c in line.chars() {
        match c {
            '"' => { quoted = !quoted; any = true; }
            c if c.is_whitespace() && !quoted => { if any || !cur.is_empty() { out.push(std::mem::take(&mut cur)); any = false; } }
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() { out.push(cur); }
    out
}

/// One track of the layout computed by [`Cue::layout`]: where its data lives in the files.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackExtent {
    pub number: u32,
    pub mode: TrackMode,
    pub file: usize,
    /// bytes per sector in that file (2352, or 2048 for MODE1/2048)
    pub sector_size: usize,
    /// byte offset of INDEX 01 inside the file
    pub start: u64,
    /// sectors from INDEX 01 to the next track's INDEX 00 (or INDEX 01), or to the end of the file
    pub sectors: u32,
}

impl Cue {
    pub fn parse(text: &str) -> Result<Cue, String> {
        let mut cue = Cue { files: vec![], tracks: vec![] };
        for (ln, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let w = cue_words(line);
            let Some(kw) = w.first() else { continue };
            let at = |m: String| format!("cue line {}: {m}", ln + 1);
            match kw.to_ascii_uppercase().as_str() {
                "FILE" => {
                    if w.len() < 3 { return Err(at("FILE needs a name and a type".into())); }
                    let ty = w[w.len() - 1].to_ascii_uppercase();
                    if ty != "BINARY" && ty != "MOTOROLA" { return Err(at(format!("file type {ty} is not supported (need BINARY)"))); }
                    cue.files.push(w[1..w.len() - 1].join(" "));
                }
                "TRACK" => {
                    if w.len() < 3 { return Err(at("TRACK needs a number and a mode".into())); }
                    if cue.files.is_empty() { return Err(at("TRACK before any FILE".into())); }
                    let number = w[1].parse::<u32>().map_err(|_| at("bad track number".into()))?;
                    let mode = match w[2].to_ascii_uppercase().as_str() {
                        "MODE1/2352" => TrackMode::Mode1Raw,
                        "MODE1/2048" => TrackMode::Mode1Cooked,
                        "AUDIO" => TrackMode::Audio,
                        other => TrackMode::Other(other.to_string()),
                    };
                    cue.tracks.push(CueTrack { number, mode, file: cue.files.len() - 1, index00: None, index01: 0, pregap: 0 });
                }
                "INDEX" => {
                    if w.len() < 3 { return Err(at("INDEX needs a number and a time".into())); }
                    let t = cue.tracks.last_mut().ok_or_else(|| at("INDEX before any TRACK".into()))?;
                    let pos = msf_to_sectors(&w[2]).map_err(&at)?;
                    match w[1].as_str() {
                        "00" | "0" => t.index00 = Some(pos),
                        "01" | "1" => t.index01 = pos,
                        _ => {} // INDEX 02.. (sub-indices) do not matter
                    }
                }
                "PREGAP" => {
                    let t = cue.tracks.last_mut().ok_or_else(|| at("PREGAP before any TRACK".into()))?;
                    t.pregap = msf_to_sectors(w.get(1).map(|s| s.as_str()).unwrap_or("")).map_err(&at)?;
                }
                _ => {} // REM, CATALOG, TITLE, PERFORMER, FLAGS, ISRC, POSTGAP ...
            }
        }
        if cue.tracks.is_empty() { return Err("the cue sheet has no TRACK".into()); }
        Ok(cue)
    }

    /// Where each track's data is, given the byte size of every `FILE` (same order as [`Cue::files`]).
    /// A track runs from its INDEX 01 up to the next track of the same file (its INDEX 00 if it has one, so the pregap of the
    /// next track is not part of this one) or, for the last track of a file, to the end of the file.
    pub fn layout(&self, file_bytes: &[u64]) -> Result<Vec<TrackExtent>, String> {
        if file_bytes.len() != self.files.len() { return Err("file size list does not match the cue".into()); }
        // sector size per file, from its tracks
        let mut ssz: Vec<Option<usize>> = vec![None; self.files.len()];
        for t in &self.tracks {
            let sz = t.mode.sector_size().ok_or_else(|| format!("track {:02}: mode {:?} is not supported", t.number, match &t.mode { TrackMode::Other(m) => m.clone(), _ => String::new() }))?;
            match ssz[t.file] {
                None => ssz[t.file] = Some(sz),
                Some(o) if o != sz => return Err(format!("file {:?} mixes sector sizes", self.files[t.file])),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for (i, t) in self.tracks.iter().enumerate() {
            let sz = ssz[t.file].unwrap();
            if file_bytes[t.file] % sz as u64 != 0 {
                return Err(format!("{:?}: size {} is not a multiple of {sz} bytes (truncated or not a raw image)", self.files[t.file], file_bytes[t.file]));
            }
            let total = (file_bytes[t.file] / sz as u64) as u32;
            let end = match self.tracks.get(i + 1) {
                Some(n) if n.file == t.file => n.index00.unwrap_or(n.index01),
                _ => total,
            };
            if t.index01 > end || end > total {
                return Err(format!("track {:02}: sectors {}..{} do not fit in {:?} ({} sectors)", t.number, t.index01, end, self.files[t.file], total));
            }
            out.push(TrackExtent { number: t.number, mode: t.mode.clone(), file: t.file, sector_size: sz, start: t.index01 as u64 * sz as u64, sectors: end - t.index01 });
        }
        Ok(out)
    }

    /// Redump-style cue text for a set of per-track files: `names[i]` holds track i (its pregap included, so INDEX 00 is 00:00:00
    /// when the track has one), `pre[i]` = sectors of pregap stored at the start of that file.
    pub fn redump_text(&self, names: &[String], pre: &[u32]) -> String {
        let mut s = String::new();
        for (i, t) in self.tracks.iter().enumerate() {
            s += &format!("FILE \"{}\" BINARY\r\n", names[i]);
            let mode = match &t.mode { TrackMode::Mode1Raw => "MODE1/2352".to_string(), TrackMode::Mode1Cooked => "MODE1/2048".into(), TrackMode::Audio => "AUDIO".into(), TrackMode::Other(m) => m.clone() };
            s += &format!("  TRACK {:02} {mode}\r\n", t.number);
            if pre[i] > 0 { s += &format!("    INDEX 00 {}\r\n", sectors_to_msf(0)); }
            s += &format!("    INDEX 01 {}\r\n", sectors_to_msf(pre[i]));
        }
        s
    }
}

/// Minimal single-bin cue reader kept for the old callers: `(track number, type, index00 lba, index01 lba)` with positions
/// relative to the (only) FILE. Use [`Cue::parse`] for multi-file sheets.
pub fn parse_cue(cue: &str) -> Vec<(u32, String, Option<u32>, u32)> {
    let mut v: Vec<(u32, String, Option<u32>, u32)> = Vec::new();
    if let Ok(c) = Cue::parse(cue) {
        for t in &c.tracks {
            let ty = match &t.mode { TrackMode::Audio => "AUDIO".to_string(), TrackMode::Mode1Raw => "MODE1/2352".into(), TrackMode::Mode1Cooked => "MODE1/2048".into(), TrackMode::Other(m) => m.clone() };
            v.push((t.number, ty, t.index00, t.index01));
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const SINGLE: &str = "FILE \"snatcher.bin\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 00 00:04:00\n    INDEX 01 00:06:00\n  TRACK 03 AUDIO\n    INDEX 00 00:10:10\n    INDEX 01 00:12:10\n";

    #[test]
    fn msf_math() {
        assert_eq!(msf_to_sectors("00:00:00").unwrap(), 0);
        assert_eq!(msf_to_sectors("00:02:00").unwrap(), 150);
        assert_eq!(msf_to_sectors("12:28:64").unwrap(), (12 * 60 + 28) * 75 + 64);
        assert_eq!(sectors_to_msf(msf_to_sectors("52:07:58").unwrap()), "52:07:58");
        assert!(msf_to_sectors("00:60:00").is_err());
        assert!(msf_to_sectors("00:00:75").is_err());
        assert!(msf_to_sectors("1:2").is_err());
        assert!(msf_to_sectors("aa:00:00").is_err());
    }

    #[test]
    fn single_bin() {
        let c = Cue::parse(SINGLE).unwrap();
        assert_eq!(c.files, vec!["snatcher.bin"]);
        assert_eq!(c.tracks.len(), 3);
        assert_eq!(c.tracks[1].index00, Some(300));
        assert_eq!(c.tracks[1].index01, 450);
        // 12:10 = 12*75+10 = 910 for INDEX 01 of track 3; the file holds 2000 sectors
        let l = c.layout(&[2000 * 2352]).unwrap();
        assert_eq!(l[0].sectors, 300);            // data track ends where track 2's pregap (INDEX 00) starts
        assert_eq!((l[1].start, l[1].sectors), (450 * 2352, 10 * 75 + 10 - 450)); // INDEX 01 -> next INDEX 00
        assert_eq!(l[2].sectors, 2000 - 12 * 75 - 10);                             // last track: to the end of the file
        // the old tuple API agrees
        let t = parse_cue(SINGLE);
        assert_eq!(t[1], (2, "AUDIO".to_string(), Some(300), 450));
    }

    #[test]
    fn redump_multi_bin() {
        let text = "FILE \"Snatcher (USA) (Track 01).bin\" BINARY\r\n  TRACK 01 MODE1/2352\r\n    INDEX 01 00:00:00\r\nFILE \"Snatcher (USA) (Track 02).bin\" BINARY\r\n  TRACK 02 AUDIO\r\n    INDEX 00 00:00:00\r\n    INDEX 01 00:02:00\r\nFILE \"Snatcher (USA) (Track 03).bin\" BINARY\r\n  TRACK 03 AUDIO\r\n    INDEX 00 00:00:00\r\n    INDEX 01 00:02:00\r\n";
        let c = Cue::parse(text).unwrap();
        assert_eq!(c.files, vec!["Snatcher (USA) (Track 01).bin", "Snatcher (USA) (Track 02).bin", "Snatcher (USA) (Track 03).bin"]);
        assert_eq!(c.tracks.iter().map(|t| t.file).collect::<Vec<_>>(), vec![0, 1, 2]);
        // positions are file-relative: both audio tracks have INDEX 01 at 150 in their own file
        let l = c.layout(&[1000 * 2352, 600 * 2352, 400 * 2352]).unwrap();
        assert_eq!((l[0].start, l[0].sectors), (0, 1000));
        assert_eq!((l[1].file, l[1].start, l[1].sectors), (1, 150 * 2352, 450)); // the 150-sector pregap is skipped
        assert_eq!((l[2].file, l[2].start, l[2].sectors), (2, 150 * 2352, 250));
    }

    #[test]
    fn pregap_command_is_not_stored_in_the_file() {
        let text = "FILE \"a.bin\" BINARY\n TRACK 01 MODE1/2352\n  INDEX 01 00:00:00\nFILE \"b.bin\" BINARY\n TRACK 02 AUDIO\n  PREGAP 00:02:00\n  INDEX 01 00:00:00\n";
        let c = Cue::parse(text).unwrap();
        assert_eq!(c.tracks[1].pregap, 150);
        let l = c.layout(&[10 * 2352, 20 * 2352]).unwrap();
        assert_eq!((l[1].start, l[1].sectors), (0, 20)); // no INDEX 00: the audio starts at file offset 0
    }

    #[test]
    fn cue_syntax_variants() {
        // unquoted name, lower case keywords, REM / CATALOG lines, BOM, extra spaces
        let c = Cue::parse("file  disc.bin  binary\n track 01 mode1/2352\n  index 01 00:00:00\n").unwrap();
        assert_eq!((c.files[0].as_str(), c.tracks[0].mode.clone()), ("disc.bin", TrackMode::Mode1Raw));
        let c = Cue::parse("\u{feff}REM GENRE x\nCATALOG 0000000000000\nFILE disc.bin BINARY\n TRACK 01 MODE1/2352\n  INDEX 01 00:00:00\n").unwrap();
        assert_eq!(c.files, vec!["disc.bin"]);
        // a name with spaces and no quotes is joined back
        let c = Cue::parse("FILE My Disc.bin BINARY\n TRACK 01 MODE1/2352\n  INDEX 01 00:00:00\n").unwrap();
        assert_eq!(c.files, vec!["My Disc.bin"]);
    }

    #[test]
    fn cue_errors_are_messages_not_panics() {
        assert!(Cue::parse("").is_err());
        assert!(Cue::parse("TRACK 01 AUDIO\n").unwrap_err().contains("before any FILE"));
        assert!(Cue::parse("FILE \"a.wav\" WAVE\n TRACK 01 AUDIO\n  INDEX 01 00:00:00\n").unwrap_err().contains("WAVE"));
        assert!(Cue::parse("FILE \"a.bin\" BINARY\n TRACK 01 AUDIO\n  INDEX 01 00:99:00\n").is_err());
        let c = Cue::parse("FILE \"a.bin\" BINARY\n TRACK 01 MODE2/2352\n  INDEX 01 00:00:00\n").unwrap();
        assert!(c.layout(&[2352]).unwrap_err().contains("not supported"));
        let c = Cue::parse(SINGLE).unwrap();
        assert!(c.layout(&[2352 * 100 + 5]).unwrap_err().contains("multiple of 2352"));
        assert!(c.layout(&[2352 * 100]).unwrap_err().contains("do not fit")); // INDEX beyond the file end
        assert!(c.layout(&[]).is_err());
    }

    #[test]
    fn cooked_data_track_and_redump_writer() {
        let text = "FILE \"d.iso\" BINARY\n TRACK 01 MODE1/2048\n  INDEX 01 00:00:00\nFILE \"a.bin\" BINARY\n TRACK 02 AUDIO\n  INDEX 01 00:00:00\n";
        let c = Cue::parse(text).unwrap();
        let l = c.layout(&[100 * 2048, 5 * 2352]).unwrap();
        assert_eq!((l[0].sector_size, l[0].sectors), (2048, 100));
        assert_eq!((l[1].sector_size, l[1].sectors), (2352, 5));
        let one = Cue::parse(SINGLE).unwrap();
        let w = one.redump_text(&["t1.bin".into(), "t2.bin".into(), "t3.bin".into()], &[0, 150, 150]);
        let back = Cue::parse(&w).unwrap();
        assert_eq!(back.files, vec!["t1.bin", "t2.bin", "t3.bin"]);
        assert_eq!(back.tracks[1].index00, Some(0));
        assert_eq!(back.tracks[1].index01, 150);
        assert_eq!(back.tracks[0].index00, None);
    }

    #[test]
    fn iso_listing_reads_only_what_it_needs() {
        // a tiny image: PVD at sector 16, root directory at sector 17 with one file
        let mut img = vec![0u8; 20 * 2048];
        let pvd = &mut img[16 * 2048..17 * 2048];
        pvd[0] = 1; pvd[1..6].copy_from_slice(b"CD001");
        let root = &mut pvd[156..156 + 34];
        root[0] = 34; root[2..6].copy_from_slice(&17u32.to_le_bytes()); root[6..10].copy_from_slice(&17u32.to_be_bytes());
        root[10..14].copy_from_slice(&2048u32.to_le_bytes()); root[14..18].copy_from_slice(&2048u32.to_be_bytes());
        let d = &mut img[17 * 2048..18 * 2048];
        let name = b"HELLO.BIN;1";
        d[0] = 33 + name.len() as u8 + 1; // padded to even
        d[2..6].copy_from_slice(&18u32.to_le_bytes()); d[10..14].copy_from_slice(&5u32.to_le_bytes());
        d[32] = name.len() as u8; d[33..33 + name.len()].copy_from_slice(name);
        let mut reads = Vec::new();
        let files = iso_root_with(&mut |lba, len| { reads.push((lba, len)); Ok(img[lba as usize * 2048..lba as usize * 2048 + len].to_vec()) }).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!((files[0].name.as_str(), files[0].lba, files[0].size), ("HELLO.BIN", 18, 5));
        assert_eq!(reads, vec![(16, 2048), (17, 2048)]);
    }
}
