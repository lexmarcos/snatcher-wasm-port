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
    let pvd = t.sector(16);
    if &pvd[1..6] != b"CD001" || pvd[0] != 1 { return Err("no ISO9660 PVD at sector 16".into()); }
    let root = &pvd[156..156 + 34];
    let (ext, size) = (le32(root, 2), le32(root, 10));
    // both-endian check
    if be32(root, 6) != ext || be32(root, 14) != size { return Err("root record: LE/BE mismatch".into()); }
    let dir = t.read(ext, size);
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < dir.len() {
        let l = dir[i] as usize;
        if l == 0 { i = (i / USER_SECTOR + 1) * USER_SECTOR; continue; }
        let r = &dir[i..i + l];
        let nl = r[32] as usize;
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

/// Minimal .cue parser for single-bin images: returns (track number, type, index00 lba, index01 lba).
pub fn parse_cue(cue: &str) -> Vec<(u32, String, Option<u32>, u32)> {
    let mut v: Vec<(u32, String, Option<u32>, u32)> = Vec::new();
    let msf = |s: &str| -> u32 {
        let p: Vec<u32> = s.split(':').map(|x| x.parse().unwrap()).collect();
        (p[0] * 60 + p[1]) * 75 + p[2]
    };
    for line in cue.lines() {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.as_slice() {
            ["TRACK", n, ty] => v.push((n.parse().unwrap(), ty.to_string(), None, 0)),
            ["INDEX", "00", t] => v.last_mut().unwrap().2 = Some(msf(t)),
            ["INDEX", "01", t] => v.last_mut().unwrap().3 = msf(t),
            _ => {}
        }
    }
    v
}
